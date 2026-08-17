//! SOS per-sender rate limiting — EMERG-001 (AC-5, DEC-EMERG-0005).
//!
//! Rolling fixed 1-hour window per sender identity:
//!   - the first 3 accepted SOS/RESCUE keep P0,
//!   - the 4th+ within the window are downgraded to P3 (degraded delivery);
//!     the sender (or a UI) surfaces a "retry after Ns" notice,
//!   - a verified CANCEL from the same sender resets the window (legitimate
//!     repeat situations can re-enter P0).
//!
//! Sybil caveat (DEC-EMERG-0005): per-identity limits are only as strong as
//! identity uniqueness; a full SybilGuard-style layer is out of scope for v1
//! (recorded in known_limitations).

use std::collections::{HashMap, VecDeque};

/// Rolling window length (seconds).
pub const SOS_WINDOW_SECS: u64 = 3600;

/// P0 allowance per sender per window.
pub const SOS_P0_ALLOWANCE: usize = 3;

/// Maximum active sender buckets (DEC-EMERG-0005 LRU cap 4096) — bounds
/// memory under Sybil/flood despite per-identity limits (EMERG-RT-002).
pub const SOS_MAX_BUCKETS: usize = 4096;

/// Decision from the rate limiter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// Still within the P0 allowance.
    Allowed { remaining: usize },
    /// Exceeded — downgrade to P3 with a retry hint.
    Downgraded { retry_after_secs: u64 },
}

#[derive(Debug, Default)]
struct SenderWindow {
    /// Accepted SOS timestamps in the current rolling window (kept sorted).
    timestamps: Vec<u64>,
}

/// Bucket key from a sender identity: first 16 bytes. The full identity never
/// enters the limiter map (16 B prefix is a strong per-sender fingerprint;
/// full key bytes stay off memory, mirroring the audit trail's prefix policy).
fn key_of(sender: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 16];
    let take = sender.len().min(16);
    k[..take].copy_from_slice(&sender[..take]);
    k
}

/// Per-sender SOS rate limiter (rolling window). Not thread-safe; the engine
/// guards it with a Mutex (see `provider`).
///
/// Bounded (EMERG-RT-002): the sender set is capped at [`SOS_MAX_BUCKETS`]
/// (LRU eviction) so hostile Sybil/fresh-sender floods cannot grow memory
/// without bound; per-bucket timestamps are trimmed to the rolling window.
#[derive(Debug, Default)]
pub struct SosRateLimiter {
    windows: HashMap<[u8; 16], SenderWindow>,
    /// LRU order: front = oldest (eviction candidate).
    order: VecDeque<[u8; 16]>,
}

impl SosRateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Touch a bucket's LRU position (move to back) and evict the oldest when
    /// the bucket set exceeds the cap.
    fn touch(&mut self, key: [u8; 16]) {
        self.order.retain(|k| *k != key);
        self.order.push_back(key);
        while self.order.len() > SOS_MAX_BUCKETS {
            if let Some(oldest) = self.order.pop_front() {
                self.windows.remove(&oldest);
            }
        }
    }

    /// Classify a gated SOS at `now_unix`.
    pub fn check(&mut self, sender: &[u8], now_unix: u64) -> RateLimitDecision {
        let key = key_of(sender);
        let decision = {
            let window = self.windows.entry(key).or_default();
            window
                .timestamps
                .retain(|t| *t >= now_unix.saturating_sub(SOS_WINDOW_SECS));
            if window.timestamps.len() < SOS_P0_ALLOWANCE {
                RateLimitDecision::Allowed {
                    remaining: SOS_P0_ALLOWANCE - window.timestamps.len(),
                }
            } else {
                let oldest = window.timestamps.first().copied().unwrap_or(now_unix);
                RateLimitDecision::Downgraded {
                    retry_after_secs: oldest
                        .saturating_add(SOS_WINDOW_SECS)
                        .saturating_sub(now_unix),
                }
            }
        };
        self.touch(key);
        decision
    }

    /// Record an accepted SOS (call after an Allowed decision is delivered).
    pub fn record(&mut self, sender: &[u8], now_unix: u64) {
        let key = key_of(sender);
        {
            let window = self.windows.entry(key).or_default();
            window
                .timestamps
                .retain(|t| *t >= now_unix.saturating_sub(SOS_WINDOW_SECS));
            window.timestamps.push(now_unix);
        }
        self.touch(key);
    }

    /// Clear a sender's window after a verified CANCEL (AC-6 reset).
    pub fn reset(&mut self, sender: &[u8]) {
        let key = key_of(sender);
        self.order.retain(|k| *k != key);
        self.windows.remove(&key);
    }

    /// Number of active sender buckets (tests).
    pub fn bucket_count(&self) -> usize {
        self.windows.len()
    }

    // Test-only introspection is fine (module-private semantics).
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_three_allowed_fourth_downgraded() {
        let mut rl = SosRateLimiter::new();
        let sender = [1u8; 16];
        for i in 0usize..3 {
            let d = rl.check(&sender, 1_000 + i as u64);
            assert_eq!(
                d,
                RateLimitDecision::Allowed {
                    remaining: 3usize - i
                }
            );
            rl.record(&sender, 1_000 + i as u64);
        }
        let d = rl.check(&sender, 1_000 + 3);
        assert!(matches!(d, RateLimitDecision::Downgraded { .. }));
    }

    #[test]
    fn resets_after_cancel() {
        let mut rl = SosRateLimiter::new();
        let sender = [2u8; 16];
        rl.record(&sender, 1_000);
        rl.record(&sender, 1_100);
        rl.record(&sender, 1_200);
        assert!(matches!(
            rl.check(&sender, 1_300),
            RateLimitDecision::Downgraded { .. }
        ));
        rl.reset(&sender);
        assert_eq!(
            rl.check(&sender, 1_301),
            RateLimitDecision::Allowed { remaining: 3 }
        );
    }

    #[test]
    fn window_slides_over_time() {
        let mut rl = SosRateLimiter::new();
        let sender = [3u8; 16];
        rl.record(&sender, 1_000);
        rl.record(&sender, 1_100);
        rl.record(&sender, 1_200);
        // 61 minutes later the window has rolled: 1 allowance freed per expired
        // entry; at 1_000 + 3660 the 1_000 entry expires → 2 in window.
        assert_eq!(
            rl.check(&sender, 1_000 + 3_661),
            RateLimitDecision::Allowed { remaining: 1 }
        );
    }

    #[test]
    fn independent_senders() {
        let mut rl = SosRateLimiter::new();
        let a = [4u8; 16];
        let b = [5u8; 16];
        rl.record(&a, 1_000);
        rl.record(&a, 1_001);
        rl.record(&a, 1_002);
        // A exhausted; B fresh.
        assert!(matches!(
            rl.check(&a, 1_003),
            RateLimitDecision::Downgraded { .. }
        ));
        assert_eq!(
            rl.check(&b, 1_003),
            RateLimitDecision::Allowed { remaining: 3 }
        );
    }

    #[test]
    fn lru_caps_sender_buckets() {
        // EMERG-RT-002: hostile fresh-sender flood must not grow the bucket
        // set without bound. Touch SOS_MAX_BUCKETS+1 distinct senders; the
        // oldest bucket is evicted and bucket_count never exceeds the cap.
        let mut rl = SosRateLimiter::new();
        for i in 0..(SOS_MAX_BUCKETS + 8) {
            let mut sender = [0u8; 16];
            sender[..8].copy_from_slice(&(i as u64).to_le_bytes());
            rl.record(&sender, 1_000);
            assert!(rl.bucket_count() <= SOS_MAX_BUCKETS);
        }
        assert_eq!(rl.bucket_count(), SOS_MAX_BUCKETS);
        // LRU eviction freed the oldest sender (id 0), whose window is gone.
        let mut first = [0u8; 16];
        first[..8].copy_from_slice(&0u64.to_le_bytes());
        assert_eq!(
            rl.check(&first, 1_100),
            RateLimitDecision::Allowed { remaining: 3 }
        );
    }
}
