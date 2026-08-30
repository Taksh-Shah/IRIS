//! srTCM token-bucket rate limiter per (sender, class) — RES-0018 R1, RFC 2697/2698.
//! P0/P1 drop-exempt; unknown-sender aggregate bucket bounded; silent drop on overflow.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::MessagePriority;
use tokio::sync::RwLock;

/// 16-byte sender short ID (SHA-256(pubkey)[..16], per IDENT-001 / AC-4).
pub type SenderShort = [u8; 16];

/// Rate-limiter key: (sender_short, class).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct BucketKey {
    pub sender: SenderShort,
    pub class: MessageClass,
}

/// Message class for rate-limiting (8 classes map to P0-P7 priorities).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MessageClass {
    P0,
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    P7,
    Unknown,
}

impl From<MessagePriority> for MessageClass {
    fn from(p: MessagePriority) -> Self {
        match p.as_u8() {
            0 => MessageClass::P0,
            1 => MessageClass::P1,
            2 => MessageClass::P2,
            3 => MessageClass::P3,
            4 => MessageClass::P4,
            5 => MessageClass::P5,
            6 => MessageClass::P6,
            7 => MessageClass::P7,
            _ => MessageClass::Unknown,
        }
    }
}

/// Token bucket state (srTCM: two rates -> committed + peak; we use single rate
/// with burst as peak for simplicity — RES-0018 R1 references RFC 2697 shape).
#[derive(Clone, Copy, Debug)]
struct Bucket {
    tokens: u64,
    last_refill: Instant,
}

/// Configuration for the rate limiter.
#[derive(Clone, Debug)]
pub struct RateLimiterConfig {
    /// Tokens added per refill interval (rate in tokens/sec).
    pub rate_per_sec: u64,
    /// Maximum bucket capacity (burst allowance).
    pub burst: u64,
    /// Refill interval.
    pub refill_interval: Duration,
    /// P0/P1 are drop-exempt (only queued at priority, never rate-limited).
    pub exempt_p0_p1: bool,
    /// Maximum number of per-sender buckets (prevents unbounded growth).
    pub max_sender_buckets: usize,
    /// Unknown-sender aggregate bucket config.
    pub unknown_sender_rate_per_sec: u64,
    pub unknown_sender_burst: u64,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        RateLimiterConfig {
            rate_per_sec: 100,
            burst: 200,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 10_000,
            unknown_sender_rate_per_sec: 10,
            unknown_sender_burst: 20,
        }
    }
}

/// Whether a message's payload justifies the P0/P1 rate-limit exemption.
///
/// Derived from the content type, never from the sender-chosen priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EmergencyClaim {
    /// Payload is genuine emergency content (SOS / medical / emergency alert).
    Emergency,
    /// Ordinary content, whatever priority it claims.
    Ordinary,
}

/// Result of a rate-limit check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RateLimitDecision {
    /// Within limit; message proceeds.
    Allowed,
    /// Over limit; message silently dropped (no error/NAK).
    SilentDrop,
    /// P0/P1 exempt from drops (queue-priority only).
    Exempt,
}

/// Pure srTCM refill arithmetic (Kani proof target — TEST-001 AC-3).
///
/// Returns the bucket token count after an elapsed `intervals` refill periods
/// at `rate_per_sec` tokens per period, capped at `burst`. Uses saturating
/// arithmetic: malformed (attacker-controlled) inputs must never overflow.
pub(crate) fn refill_tokens(tokens: u64, intervals: u64, rate_per_sec: u64, burst: u64) -> u64 {
    tokens
        .saturating_add(intervals.saturating_mul(rate_per_sec))
        .min(burst)
}

/// Bucket map + insertion-order queue (GAP-3: FIFO eviction instead of random).
struct BucketState {
    map: HashMap<BucketKey, Bucket>,
    /// Insertion order for FIFO eviction at capacity. Contains every key
    /// currently in `map`; stale entries (removed keys) are skipped at eviction.
    order: VecDeque<BucketKey>,
}

impl BucketState {
    fn new(capacity: usize) -> Self {
        BucketState {
            map: HashMap::with_capacity(capacity),
            order: VecDeque::new(),
        }
    }
}

/// srTCM-style token bucket rate limiter per (sender, class).
pub struct RateLimiter {
    buckets: RwLock<BucketState>,
    unknown_bucket: RwLock<Bucket>,
    config: RateLimiterConfig,
    metrics: RateLimiterMetrics,
    /// RT-200: injectable clock. Production uses `Instant::now()`; property
    /// tests freeze time so refill boundaries cannot be crossed by scheduler
    /// jitter (the auto-persisted regression seeds were load-timing luck,
    /// not logic bugs — `refill_tokens` itself is saturating + Kani-proven).
    clock: ClockFn,
}

type ClockFn = Box<dyn Fn() -> Instant + Send + Sync>;

/// Metrics for observability (OBS-001 integration).
#[derive(Default, Debug)]
pub struct RateLimiterMetrics {
    pub allowed: AtomicU64,
    pub dropped: AtomicU64,
    pub exempt: AtomicU64,
    pub unknown_allowed: AtomicU64,
    pub unknown_dropped: AtomicU64,
}

impl RateLimiter {
    /// Create a new rate limiter with default configuration.
    pub fn new(config: RateLimiterConfig) -> Self {
        RateLimiter::with_clock(config, Box::new(Instant::now))
    }

    /// Rate limiter with an injectable clock (RT-200): property tests freeze
    /// time to keep refill-boundary semantics deterministic under load.
    pub fn with_clock(config: RateLimiterConfig, clock: ClockFn) -> Self {
        let now = (clock)();
        RateLimiter {
            buckets: RwLock::new(BucketState::new(1024)),
            unknown_bucket: RwLock::new(Bucket {
                tokens: config.unknown_sender_burst,
                last_refill: now,
            }),
            config,
            metrics: RateLimiterMetrics::default(),
            clock,
        }
    }

    /// Get a snapshot of metrics.
    pub fn metrics(&self) -> RateLimiterMetricsSnapshot {
        RateLimiterMetricsSnapshot {
            allowed: self.metrics.allowed.load(Ordering::Relaxed),
            dropped: self.metrics.dropped.load(Ordering::Relaxed),
            exempt: self.metrics.exempt.load(Ordering::Relaxed),
            unknown_allowed: self.metrics.unknown_allowed.load(Ordering::Relaxed),
            unknown_dropped: self.metrics.unknown_dropped.load(Ordering::Relaxed),
        }
    }

    /// Check rate limit for a sender and class.
    /// Returns Allowed / SilentDrop / Exempt.
    /// P0/P1 are exempt from rate-limit drops (queue-priority only) per DEC-SEC-0001.
    pub async fn check(&self, sender: SenderShort, class: MessageClass) -> RateLimitDecision {
        self.check_with_claim(sender, class, EmergencyClaim::Emergency)
            .await
    }

    /// Rate-limit check that also weighs whether the CONTENT justifies the
    /// P0/P1 exemption.
    ///
    /// The exemption exists so a genuine SOS is never dropped (DEC-SEC-0001).
    /// Granting it on the claimed priority alone made it a free
    /// network-starvation primitive: `priority = P0` on an ordinary `Text`
    /// envelope bought top-of-heap scheduling, multipath fan-out over every
    /// radio, and total freedom from rate limiting. Priority is a hint the
    /// sender picks; the payload type is what the emergency ACL validates.
    pub async fn check_with_claim(
        &self,
        sender: SenderShort,
        class: MessageClass,
        claim: EmergencyClaim,
    ) -> RateLimitDecision {
        // P0/P1 are exempt from rate-limit drops only when the payload really
        // is emergency content.
        if self.config.exempt_p0_p1
            && matches!(class, MessageClass::P0 | MessageClass::P1)
            && claim == EmergencyClaim::Emergency
        {
            self.metrics.exempt.fetch_add(1, Ordering::Relaxed);
            return RateLimitDecision::Exempt;
        }

        let key = BucketKey { sender, class };

        // Use write lock to allow bucket mutation
        let mut state = self.buckets.write().await;

        // GAP-3: FIFO eviction — evict the oldest bucket instead of a random one.
        // Pop from the front of `order`, skipping any keys already removed.
        if state.map.len() >= self.config.max_sender_buckets {
            while let Some(oldest) = state.order.pop_front() {
                if state.map.remove(&oldest).is_some() {
                    break;
                }
                // key was already evicted/forgotten — keep popping
            }
        }

        let now = (self.clock)();
        if !state.map.contains_key(&key) {
            state.map.insert(key, Bucket { tokens: self.config.burst, last_refill: now });
            state.order.push_back(key);
        }
        let bucket = state.map.get_mut(&key).unwrap();

        let decision = Self::check_bucket(
            bucket,
            now,
            self.config.rate_per_sec,
            self.config.refill_interval,
            self.config.burst,
        );
        match decision {
            RateLimitDecision::Allowed => {
                self.metrics.allowed.fetch_add(1, Ordering::Relaxed);
            }
            RateLimitDecision::SilentDrop => {
                self.metrics.dropped.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
        drop(state);
        decision
    }

    /// Check rate limit for unknown sender (aggregate bucket).
    pub async fn check_unknown(&self, class: MessageClass) -> RateLimitDecision {
        if self.config.exempt_p0_p1 && matches!(class, MessageClass::P0 | MessageClass::P1) {
            self.metrics.exempt.fetch_add(1, Ordering::Relaxed);
            return RateLimitDecision::Exempt;
        }

        let mut bucket = self.unknown_bucket.write().await;
        self.refill(
            &mut bucket,
            self.config.unknown_sender_rate_per_sec,
            self.config.unknown_sender_burst,
        );
        if bucket.tokens >= 1 {
            bucket.tokens = bucket.tokens.saturating_sub(1);
            self.metrics.unknown_allowed.fetch_add(1, Ordering::Relaxed);
            RateLimitDecision::Allowed
        } else {
            self.metrics.unknown_dropped.fetch_add(1, Ordering::Relaxed);
            RateLimitDecision::SilentDrop
        }
    }

    /// Check a bucket and consume a token if allowed.
    fn check_bucket(
        bucket: &mut Bucket,
        now: Instant,
        rate_per_sec: u64,
        refill_interval: Duration,
        burst: u64,
    ) -> RateLimitDecision {
        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed >= refill_interval {
            // SEC-RT-11: guard refill_interval < 1s (as_secs() == 0) against
            // division-by-zero panic.
            let period_secs = refill_interval.as_secs().max(1);
            let intervals = elapsed.as_secs() / period_secs;
            if intervals > 0 {
                bucket.tokens = refill_tokens(bucket.tokens, intervals, rate_per_sec, burst);
                bucket.last_refill = now;
            }
        }
        if bucket.tokens >= 1 {
            bucket.tokens = bucket.tokens.saturating_sub(1);
            RateLimitDecision::Allowed
        } else {
            RateLimitDecision::SilentDrop
        }
    }

    /// Lazy refill: add tokens based on elapsed time.
    /// `burst` is the cap for THIS bucket (per-sender burst vs unknown-sender
    /// burst — SEC-RT-09: the unknown aggregate bucket must cap at its own
    /// burst, not the per-sender burst, or a quiet period lets it refill far
    /// past the intended aggregate limit).
    fn refill(&self, bucket: &mut Bucket, rate_per_sec: u64, burst: u64) {
        let now = (self.clock)();
        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed >= self.config.refill_interval {
            // SEC-RT-11: guard refill_interval < 1s (as_secs() == 0) against
            // division-by-zero panic.
            let period_secs = self.config.refill_interval.as_secs().max(1);
            let intervals = elapsed.as_secs() / period_secs;
            if intervals > 0 {
                bucket.tokens = refill_tokens(bucket.tokens, intervals, rate_per_sec, burst);
                bucket.last_refill = now;
            }
        }
    }

    /// Reset all buckets (for testing).
    pub async fn reset(&self) {
        let mut state = self.buckets.write().await;
        state.map.clear();
        state.order.clear();
        drop(state);
        let mut unknown = self.unknown_bucket.write().await;
        unknown.tokens = self.config.unknown_sender_burst;
        unknown.last_refill = (self.clock)();
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(RateLimiterConfig::default())
    }
}

/// RT-200: a frozen clock makes the token-bucket properties hermetic —
/// no scheduler jitter can cross the 1 s refill boundary mid-property,
/// so `allowed <= burst` holds by construction. Refill semantics are
/// covered separately by `rate_limiter_refill_after_interval` (real
/// time, unit test).
#[cfg(test)]
fn frozen_limiter(config: RateLimiterConfig) -> RateLimiter {
    let epoch = Instant::now();
    RateLimiter::with_clock(config, Box::new(move || epoch))
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for MessageClass {
    type Parameters = ();
    type Strategy = proptest::strategy::Union<proptest::prelude::Just<MessageClass>>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        use proptest::prelude::Just;
        proptest::strategy::Union::new(vec![
            Just(MessageClass::P0),
            Just(MessageClass::P1),
            Just(MessageClass::P2),
            Just(MessageClass::P3),
            Just(MessageClass::P4),
            Just(MessageClass::P5),
            Just(MessageClass::P6),
            Just(MessageClass::P7),
            Just(MessageClass::Unknown),
        ])
    }
}

#[cfg(all(test, feature = "proptest"))]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(fut)
    }

    proptest! {
        #[test]
        fn rate_limiter_never_panics_on_valid_input(
            sender in any::<[u8; 16]>(),
            class in any::<MessageClass>(),
            rate in 1..1000u64,
            burst in 1..1000u64,
            n_checks in 1..100usize,
        ) {
            let config = RateLimiterConfig {
                rate_per_sec: rate,
                burst,
                refill_interval: Duration::from_secs(1),
                exempt_p0_p1: true,
                max_sender_buckets: 100,
                unknown_sender_rate_per_sec: 10,
                unknown_sender_burst: 20,
            };
            let rl = frozen_limiter(config);
            for _ in 0..n_checks {
                let _ = block_on(rl.check(sender, class));
            }
        }

        #[test]
        fn rate_limiter_respects_exempt_p0_p1(
            sender in any::<[u8; 16]>(),
            rate in 1..100u64,
            burst in 1..100u64,
            n_checks in 1..50usize,
        ) {
            let config = RateLimiterConfig {
                rate_per_sec: rate,
                burst,
                refill_interval: Duration::from_secs(1),
                exempt_p0_p1: true,
                max_sender_buckets: 100,
                unknown_sender_rate_per_sec: 10,
                unknown_sender_burst: 20,
            };
            let rl = frozen_limiter(config);
            for _ in 0..n_checks {
                prop_assert_eq!(block_on(rl.check(sender, MessageClass::P0)), RateLimitDecision::Exempt);
                prop_assert_eq!(block_on(rl.check(sender, MessageClass::P1)), RateLimitDecision::Exempt);
            }
        }

        #[test]
        fn rate_limiter_silent_drop_on_overflow(
            sender in any::<[u8; 16]>(),
            rate in 1..10u64,
            burst in 1..10u64,
            n_checks in 2..30usize,
        ) {
            let config = RateLimiterConfig {
                rate_per_sec: rate,
                burst,
                refill_interval: Duration::from_secs(1),
                exempt_p0_p1: true,
                max_sender_buckets: 100,
                unknown_sender_rate_per_sec: 10,
                unknown_sender_burst: 20,
            };
            let rl = frozen_limiter(config);
            let mut allowed = 0;
            let mut dropped = 0;
            for _ in 0..n_checks {
                match block_on(rl.check(sender, MessageClass::P4)) {
                    RateLimitDecision::Allowed => allowed += 1,
                    RateLimitDecision::SilentDrop => dropped += 1,
                    _ => {}
                }
            }
            // Token bucket: at most burst initial tokens; excess checks drop
            prop_assert!(allowed <= burst as usize);
            prop_assert_eq!(allowed + dropped, n_checks);
            if n_checks > burst as usize {
                prop_assert!(dropped >= 1);
            }
        }

        #[test]
        fn rate_limiter_class_isolation(
            sender in any::<[u8; 16]>(),
            rate in 1..100u64,
            burst in 1..100u64,
            n_checks in 1..50usize,
        ) {
            let config = RateLimiterConfig {
                rate_per_sec: rate,
                burst,
                refill_interval: Duration::from_secs(1),
                exempt_p0_p1: true,
                max_sender_buckets: 100,
                unknown_sender_rate_per_sec: 10,
                unknown_sender_burst: 20,
            };
            let rl = frozen_limiter(config);
            // Exhaust P4 (its own bucket)
            for _ in 0..burst {
                let _ = block_on(rl.check(sender, MessageClass::P4));
            }
            // P3 owns an independent bucket: P4 exhaustion must not deplete it.
            // P3 must still allow up to its own `burst` tokens.
            let mut p3_allowed = 0;
            for _ in 0..n_checks {
                match block_on(rl.check(sender, MessageClass::P3)) {
                    RateLimitDecision::Allowed => p3_allowed += 1,
                    RateLimitDecision::Exempt => {}
                    _ => {}
                }
            }
            prop_assert!(p3_allowed >= n_checks.min(burst as usize));
        }
    }
}

/// Snapshot of rate limiter metrics.
#[derive(Clone, Copy, Debug)]
pub struct RateLimiterMetricsSnapshot {
    pub allowed: u64,
    pub dropped: u64,
    pub exempt: u64,
    pub unknown_allowed: u64,
    pub unknown_dropped: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rate_limiter_allows_burst_then_drops() {
        let config = RateLimiterConfig {
            rate_per_sec: 10,
            burst: 5,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 10,
            unknown_sender_burst: 20,
        };
        let rl = frozen_limiter(config);
        let sender = [1u8; 16];

        // First 5 should be allowed (burst)
        for _ in 0..5 {
            assert_eq!(
                rl.check(sender, MessageClass::P4).await,
                RateLimitDecision::Allowed
            );
        }
        // 6th should be dropped
        assert_eq!(
            rl.check(sender, MessageClass::P4).await,
            RateLimitDecision::SilentDrop
        );

        let m = rl.metrics();
        assert_eq!(m.allowed, 5);
        assert_eq!(m.dropped, 1);
    }

    #[tokio::test]
    async fn p0_p1_exempt_from_drops() {
        let config = RateLimiterConfig {
            rate_per_sec: 1,
            burst: 1,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 10,
            unknown_sender_burst: 20,
        };
        let rl = frozen_limiter(config);
        let sender = [2u8; 16];

        // P0 exempt
        assert_eq!(
            rl.check(sender, MessageClass::P0).await,
            RateLimitDecision::Exempt
        );
        // P1 exempt
        assert_eq!(
            rl.check(sender, MessageClass::P1).await,
            RateLimitDecision::Exempt
        );
        // P2 not exempt, burst=1 so first allowed, second dropped
        assert_eq!(
            rl.check(sender, MessageClass::P2).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            rl.check(sender, MessageClass::P2).await,
            RateLimitDecision::SilentDrop
        );
    }

    #[tokio::test]
    async fn unknown_sender_aggregate_bucket() {
        let config = RateLimiterConfig {
            rate_per_sec: 5,
            burst: 10,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 2,
            unknown_sender_burst: 3,
        };
        let rl = frozen_limiter(config);

        // Unknown P4: burst=3 allows 3
        for _ in 0..3 {
            assert_eq!(
                rl.check_unknown(MessageClass::P4).await,
                RateLimitDecision::Allowed
            );
        }
        assert_eq!(
            rl.check_unknown(MessageClass::P4).await,
            RateLimitDecision::SilentDrop
        );
    }

    #[tokio::test]
    async fn p0_exemption_requires_genuine_emergency_content() {
        // Regression: the P0/P1 exemption was granted on the claimed priority
        // alone. Setting `priority = P0` on an ordinary Text envelope bought
        // top-of-heap scheduling, multipath fan-out over every radio, and total
        // freedom from rate limiting — a free network-starvation primitive.
        let config = RateLimiterConfig {
            rate_per_sec: 1,
            burst: 2,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 1,
            unknown_sender_burst: 1,
        };
        let rl = frozen_limiter(config);
        let attacker = [0x11u8; 16];
        let responder = [0x22u8; 16];

        // A P0 claim on ordinary content is metered like any other traffic:
        // the tiny bucket drains and then drops.
        let mut dropped = false;
        for _ in 0..8 {
            if rl
                .check_with_claim(attacker, MessageClass::P0, EmergencyClaim::Ordinary)
                .await
                == RateLimitDecision::SilentDrop
            {
                dropped = true;
                break;
            }
        }
        assert!(
            dropped,
            "P0 on ordinary content must be rate limited, not exempt"
        );

        // A genuine emergency payload keeps the exemption — the whole point of
        // DEC-SEC-0001 is that a real SOS is never dropped.
        for _ in 0..50 {
            assert_eq!(
                rl.check_with_claim(responder, MessageClass::P0, EmergencyClaim::Emergency)
                    .await,
                RateLimitDecision::Exempt,
                "genuine emergency content must stay exempt"
            );
        }

        // Lower priorities are unaffected by the claim either way.
        assert_ne!(
            rl.check_with_claim(responder, MessageClass::P4, EmergencyClaim::Emergency)
                .await,
            RateLimitDecision::Exempt,
            "the exemption is P0/P1 only"
        );
    }

    #[tokio::test]
    async fn rate_limiter_metrics_unknown_and_exempt() {
        let config = RateLimiterConfig {
            rate_per_sec: 5,
            burst: 10,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 2,
            unknown_sender_burst: 3,
        };
        let rl = frozen_limiter(config);

        // check_unknown P0 -> exempt metric
        assert_eq!(
            rl.check_unknown(MessageClass::P0).await,
            RateLimitDecision::Exempt
        );
        // check_unknown drains aggregate bucket
        for _ in 0..3 {
            assert_eq!(
                rl.check_unknown(MessageClass::P4).await,
                RateLimitDecision::Allowed
            );
        }
        assert_eq!(
            rl.check_unknown(MessageClass::P4).await,
            RateLimitDecision::SilentDrop
        );

        let m = rl.metrics();
        assert_eq!(m.exempt, 1);
        assert_eq!(m.unknown_allowed, 3);
        assert_eq!(m.unknown_dropped, 1);
    }

    #[tokio::test]
    async fn rate_limiter_refill_after_interval() {
        let config = RateLimiterConfig {
            rate_per_sec: 10,
            burst: 1,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 1,
            unknown_sender_burst: 1,
        };
        // REAL clock here on purpose: this test exercises actual refill
        // semantics across a live 1 s interval (RT-200 keeps it hermetic
        // everywhere else).
        let rl = RateLimiter::new(config);
        let sender = [4u8; 16];

        assert_eq!(
            rl.check(sender, MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            rl.check(sender, MessageClass::P4).await,
            RateLimitDecision::SilentDrop
        );

        // Refill window (>= 2 full intervals of the 1s refill period).
        std::thread::sleep(Duration::from_millis(1100));

        // After refill, the per-sender bucket has tokens again.
        assert_eq!(
            rl.check(sender, MessageClass::P4).await,
            RateLimitDecision::Allowed
        );

        // Unknown aggregate bucket also refills (own burst cap).
        assert_eq!(
            rl.check_unknown(MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
    }

    #[tokio::test]
    async fn rate_limiter_max_sender_buckets_eviction() {
        let config = RateLimiterConfig {
            rate_per_sec: 10,
            burst: 5,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 2,
            unknown_sender_rate_per_sec: 10,
            unknown_sender_burst: 20,
        };
        let rl = frozen_limiter(config);

        // Fill beyond capacity -> oldest bucket evicted, never a panic.
        for i in 0..5u8 {
            let _ = rl.check([i; 16], MessageClass::P4).await;
        }
        let m = rl.metrics();
        assert_eq!(m.allowed, 5);

        rl.reset().await;
        assert_eq!(rl.metrics().allowed, 5); // counters survive bucket reset
        assert_eq!(
            rl.check([0xFF; 16], MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
    }

    #[test]
    fn message_class_maps_all_priorities() {
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P0),
            MessageClass::P0
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P1),
            MessageClass::P1
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P2),
            MessageClass::P2
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P3),
            MessageClass::P3
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P4),
            MessageClass::P4
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P5),
            MessageClass::P5
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P6),
            MessageClass::P6
        );
        assert_eq!(
            MessageClass::from(crate::MessagePriority::P7),
            MessageClass::P7
        );
    }

    #[tokio::test]
    async fn class_isolation() {
        let config = RateLimiterConfig {
            rate_per_sec: 10,
            burst: 5,
            refill_interval: Duration::from_secs(1),
            exempt_p0_p1: true,
            max_sender_buckets: 100,
            unknown_sender_rate_per_sec: 10,
            unknown_sender_burst: 20,
        };
        let rl = frozen_limiter(config);
        let sender = [3u8; 16];

        // Exhaust P4 bucket
        for _ in 0..5 {
            assert_eq!(
                rl.check(sender, MessageClass::P4).await,
                RateLimitDecision::Allowed
            );
        }
        assert_eq!(
            rl.check(sender, MessageClass::P4).await,
            RateLimitDecision::SilentDrop
        );

        // P3 bucket should still be fresh
        for _ in 0..5 {
            assert_eq!(
                rl.check(sender, MessageClass::P3).await,
                RateLimitDecision::Allowed
            );
        }
    }
}
