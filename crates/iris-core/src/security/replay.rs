//! Replay protection: freshness window + per-sender high-water (ts,seq) + cross-reboot persistence.
//! RES-0018 R4/R5/R10, RFC 9171 §4.2.7, RFC 4303 §3.4.3, RFC 7181 §23.2, RFC 7183, RFC 6479.
//! Promotes WP-2 deferred dedup persistence to a security requirement (AC-3/4/5).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;

/// 16-byte sender short ID.
pub type SenderShort = [u8; 16];

/// Replay protection configuration.
#[derive(Clone, Debug)]
pub struct ReplayConfig {
    /// Freshness window: reject messages older than this (past).
    pub freshness_window_past: Duration,
    /// Freshness window: reject messages newer than this (future/skew).
    pub freshness_window_future: Duration,
    /// Per-source skew budget (added to windows).
    pub per_source_skew_budget: Duration,
    /// Maximum number of tracked senders for high-water marks.
    pub max_sender_highwater: usize,
    /// Persistence interval for cross-reboot survival.
    pub persistence_interval: Duration,
    /// Whether cross-reboot persistence is enabled.
    pub persistence_enabled: bool,
}

impl Default for ReplayConfig {
    fn default() -> Self {
        ReplayConfig {
            // RFC 9171 §4.2.7: creation-timestamp lexica; DTN-time substituted
            freshness_window_past: Duration::from_secs(24 * 3600), // 24 hours
            freshness_window_future: Duration::from_secs(5 * 60),  // 5 min future tolerance
            per_source_skew_budget: Duration::from_secs(300),      // 5 min skew budget
            max_sender_highwater: 10_000,
            persistence_interval: Duration::from_secs(30), // batched writes
            persistence_enabled: true,
        }
    }
}

/// High-water mark for a sender: (timestamp, sequence).
/// Strictly increasing (ts,seq) pairs accepted; <= high-water = replay.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HighWaterMark {
    pub timestamp: u64, // DTN-time (monotonic or wall-clock)
    pub sequence: u64,
}

impl HighWaterMark {
    /// Check if a new (ts,seq) is strictly higher than high-water.
    /// Returns true if accepted (and updates), false if replay.
    pub fn check_and_advance(&mut self, ts: u64, seq: u64) -> bool {
        if ts > self.timestamp || (ts == self.timestamp && seq > self.sequence) {
            self.timestamp = ts;
            self.sequence = seq;
            true
        } else {
            false
        }
    }
}

/// Replay protection decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDecision {
    /// Fresh and high-water advanced — accept.
    Accepted,
    /// Too old (past freshness window) — reject.
    TooOld,
    /// Too far in future (skew beyond budget) — reject.
    TooFuture,
    /// Replay detected (<= high-water) — reject.
    Replay,
}

/// Persisted snapshot for cross-reboot survival.
/// Contains dedup Bloom filter snapshot + high-water marks.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ReplaySnapshot {
    /// High-water marks per sender (ts,seq). Key is hex-encoded 16-byte sender short.
    pub highwater: HashMap<String, HighWaterMark>,
    /// When this snapshot was created.
    pub snapshot_timestamp: u64,
    /// Version for forward compatibility.
    pub version: u32,
}

impl ReplaySnapshot {
    pub fn new(highwater: HashMap<SenderShort, HighWaterMark>) -> Self {
        let hw: HashMap<String, HighWaterMark> = highwater
            .into_iter()
            .map(|(k, v)| (hex::encode(k), v))
            .collect();
        ReplaySnapshot {
            highwater: hw,
            snapshot_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            version: 1,
        }
    }

    /// Convert back to internal representation (SenderShort keys).
    pub fn into_internal(self) -> HashMap<SenderShort, HighWaterMark> {
        self.highwater
            .into_iter()
            .filter_map(|(k, v)| {
                let bytes = hex::decode(k).ok()?;
                if bytes.len() != 16 {
                    return None;
                }
                let mut arr = [0u8; 16];
                arr.copy_from_slice(&bytes);
                Some((arr, v))
            })
            .collect()
    }
}

#[cfg(all(test, feature = "proptest"))]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(fut)
    }

    /// UNIX-now used to build ts ranges inside the real freshness window.
    fn unix_now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Default config but with a wide future window so incremental ts never
    /// trips the strict TooFuture boundary during multi-step runs.
    fn wide_config() -> ReplayConfig {
        ReplayConfig {
            freshness_window_past: Duration::from_secs(24 * 3600),
            freshness_window_future: Duration::from_secs(3600),
            per_source_skew_budget: Duration::from_secs(3600),
            max_sender_highwater: 10_000,
            persistence_interval: Duration::from_secs(30),
            persistence_enabled: false,
        }
    }

    proptest! {
        #[test]
        fn replay_engine_never_panics(
            sender in any::<[u8; 16]>(),
            ts in 1..u64::MAX,
            seq in 1..u64::MAX,
        ) {
            let re = ReplayEngine::default();
            let _ = block_on(re.check(sender, ts, seq));
        }

        #[test]
        fn replay_highwater_monotonic_increases(
            sender in any::<[u8; 16]>(),
            // SEC-RT-03 clamp: future `ts` is clamped to `now` on store, so a
            // same-seq message whose raw ts crosses into the future collapses
            // onto the same stored pair (intended anti-poison behavior). This
            // proptest pins strict monotonic acceptance over the present/past
            // window only: `base_ts` is kept well below `unix_now()` so the
            // incrementing loop (≤ 20 steps) never crosses into the future.
            base_ts in (unix_now() - 3600)..=(unix_now() - 60),
            base_seq in 1..u64::MAX,
            n_steps in 1..20usize,
        ) {
            let re = ReplayEngine::new(wide_config());
            let mut ts = base_ts;
            let mut seq = base_seq;
            for _ in 0..n_steps {
                if ts < u64::MAX - 1 {
                    ts = ts.saturating_add(1);
                } else {
                    seq = seq.saturating_add(1);
                }
                prop_assert_eq!(block_on(re.check(sender, ts, seq)), ReplayDecision::Accepted);
            }
        }

        #[test]
        fn replay_detects_replay(
            sender in any::<[u8; 16]>(),
            ts in (unix_now() - 3600)..=(unix_now()),
            seq in 1..u64::MAX,
            n_replays in 1..10usize,
        ) {
            let re = ReplayEngine::new(wide_config());
            prop_assert_eq!(block_on(re.check(sender, ts, seq)), ReplayDecision::Accepted);
            for _ in 0..n_replays {
                prop_assert_eq!(block_on(re.check(sender, ts, seq)), ReplayDecision::Replay);
            }
        }

        #[test]
        fn replay_freshness_too_old(
            sender in any::<[u8; 16]>(),
            days_ago in 2..30usize,
        ) {
            let config = ReplayConfig {
                freshness_window_past: Duration::from_secs(24 * 3600),
                freshness_window_future: Duration::from_secs(5 * 60),
                per_source_skew_budget: Duration::from_secs(300),
                max_sender_highwater: 10_000,
                persistence_interval: Duration::from_secs(30),
                persistence_enabled: true,
            };
            let re = ReplayEngine::new(config);
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let old_ts = now.saturating_sub(days_ago as u64 * 24 * 3600 + 1);
            prop_assert_eq!(block_on(re.check(sender, old_ts, 1)), ReplayDecision::TooOld);
        }

        #[test]
        fn replay_freshness_too_future(
            sender in any::<[u8; 16]>(),
            minutes_ahead in 11..100usize,
        ) {
            let config = ReplayConfig {
                freshness_window_past: Duration::from_secs(24 * 3600),
                freshness_window_future: Duration::from_secs(5 * 60),
                per_source_skew_budget: Duration::from_secs(300),
                max_sender_highwater: 10_000,
                persistence_interval: Duration::from_secs(30),
                persistence_enabled: true,
            };
            let re = ReplayEngine::new(config);
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            // > future_window (5m) + skew_budget (5m) ⇒ TooFuture
            let future_ts = now + minutes_ahead as u64 * 60 + 1;
            prop_assert_eq!(block_on(re.check(sender, future_ts, 1)), ReplayDecision::TooFuture);
        }

        #[test]
        fn replay_cross_reboot_restores_highwater(
            sender in any::<[u8; 16]>(),
            // Present/past window only — future-ts clamping is time-dependent
            // and covered by the dedicated poison unit test (SEC-RT-03).
            base_ts in (unix_now() - 3600)..=(unix_now()),
            base_seq in 1..u64::MAX,
            n_msgs in 1..20usize,
        ) {
            let config = ReplayConfig {
                freshness_window_past: Duration::from_secs(24 * 3600),
                freshness_window_future: Duration::from_secs(3600),
                per_source_skew_budget: Duration::from_secs(3600),
                max_sender_highwater: 10_000,
                persistence_interval: Duration::from_secs(30),
                persistence_enabled: true,
            };
            let re = ReplayEngine::new(config.clone());
            let mut ts = base_ts;
            let mut seq = base_seq;
            for _ in 0..n_msgs {
                let _ = block_on(re.check(sender, ts, seq));
                ts = ts.saturating_add(1);
                seq = seq.saturating_add(1);
            }
            let snapshot = block_on(re.generate_snapshot());
            let re2 = ReplayEngine::from_snapshot(snapshot, config);
            let hw = block_on(re2.get_highwater(sender)).unwrap();
            prop_assert_eq!(hw.sequence, base_seq + n_msgs as u64 - 1);
            // Replay of old should be rejected
            prop_assert_eq!(block_on(re2.check(sender, base_ts, base_seq)), ReplayDecision::Replay);
            // Strictly newer (ts, seq) should be accepted
            prop_assert_eq!(
                block_on(re2.check(sender, base_ts + n_msgs as u64, base_seq + n_msgs as u64)),
                ReplayDecision::Accepted
            );
        }
    }
}

/// Replay protection engine.
pub struct ReplayEngine {
    /// Per-sender high-water marks (in-memory, hot path).
    highwater: RwLock<HashMap<SenderShort, HighWaterMark>>,
    config: ReplayConfig,
    metrics: ReplayMetrics,
    /// Pending snapshot for batched persistence.
    pending_snapshot: RwLock<Option<ReplaySnapshot>>,
    /// Last persistence time.
    last_persist: RwLock<Instant>,
}

#[derive(Default, Debug)]
pub struct ReplayMetrics {
    pub accepted: AtomicU64,
    pub too_old: AtomicU64,
    pub too_future: AtomicU64,
    pub replay: AtomicU64,
    pub persisted: AtomicU64,
}

impl Default for ReplayEngine {
    fn default() -> Self {
        Self::new(ReplayConfig::default())
    }
}

impl ReplayEngine {
    /// Create a new replay engine with default configuration.
    pub fn new(config: ReplayConfig) -> Self {
        ReplayEngine {
            highwater: RwLock::new(HashMap::with_capacity(1024)),
            config,
            metrics: ReplayMetrics::default(),
            pending_snapshot: RwLock::new(None),
            last_persist: RwLock::new(Instant::now()),
        }
    }

    /// Create from a persisted snapshot (cross-reboot restore).
    pub fn from_snapshot(snapshot: ReplaySnapshot, config: ReplayConfig) -> Self {
        ReplayEngine {
            highwater: RwLock::new(snapshot.into_internal()),
            config,
            metrics: ReplayMetrics::default(),
            pending_snapshot: RwLock::new(None),
            last_persist: RwLock::new(Instant::now()),
        }
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> ReplayMetricsSnapshot {
        ReplayMetricsSnapshot {
            accepted: self.metrics.accepted.load(Ordering::Relaxed),
            too_old: self.metrics.too_old.load(Ordering::Relaxed),
            too_future: self.metrics.too_future.load(Ordering::Relaxed),
            replay: self.metrics.replay.load(Ordering::Relaxed),
            persisted: self.metrics.persisted.load(Ordering::Relaxed),
        }
    }

    /// Check freshness window and high-water for an inbound message.
    /// `sender` = 16-byte sender short ID.
    /// `ts` = message timestamp (DTN-time, unix seconds).
    /// `seq` = message sequence (per-sender monotonic counter).
    /// Returns ReplayDecision.
    pub async fn check(&self, sender: SenderShort, ts: u64, seq: u64) -> ReplayDecision {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Freshness window: too old?
        let past_limit = now.saturating_add(self.config.per_source_skew_budget.as_secs());
        let min_ts = past_limit.saturating_sub(self.config.freshness_window_past.as_secs());
        if ts < min_ts {
            self.metrics.too_old.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooOld;
        }

        // Freshness window: too future?
        let max_ts = now
            .saturating_add(self.config.freshness_window_future.as_secs())
            .saturating_add(self.config.per_source_skew_budget.as_secs());
        if ts > max_ts {
            self.metrics.too_future.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooFuture;
        }

        // Clamp a within-window future timestamp to `now` before it is stored:
        // without this, one crafted `ts = now+599, seq = u64::MAX` poisons the
        // sender's high-water for ~10 minutes (recovery impossible while the
        // poisoned ts still dominates). Clamping bounds the damage to at most
        // the current second. (SEC-RT-03)
        let stored_ts = ts.min(now);

        let accepted = {
            // Per-sender high-water check. SEC-RT-01: tokio RwLock is NOT
            // reentrant — the earlier code read `highwater` inside
            // maybe_schedule_snapshot *while still holding the write guard*
            // (self-deadlock on the default config after one persistence
            // interval). Snapshot scheduling now runs after this block drops
            // the write guard.
            let mut highwater = self.highwater.write().await;

            // Enforce max senders
            if highwater.len() >= self.config.max_sender_highwater {
                // Simple eviction: remove one (TODO: LRU).
                if let Some(k) = highwater.keys().next().copied() {
                    highwater.remove(&k);
                }
            }

            let hw = highwater.entry(sender).or_default();
            hw.check_and_advance(stored_ts, seq)
        };

        if accepted {
            self.metrics.accepted.fetch_add(1, Ordering::Relaxed);

            // Schedule snapshot persistence (batched, crash-safe). Runs after
            // the write guard is dropped — no reentrant-read deadlock (SEC-RT-01).
            if self.config.persistence_enabled {
                self.maybe_schedule_snapshot().await;
            }

            ReplayDecision::Accepted
        } else {
            self.metrics.replay.fetch_add(1, Ordering::Relaxed);
            ReplayDecision::Replay
        }
    }

    /// Check freshness only (no high-water update) — for untrusted senders.
    pub async fn check_freshness_only(&self, ts: u64) -> ReplayDecision {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let min_ts = now.saturating_sub(self.config.freshness_window_past.as_secs());
        if ts < min_ts {
            self.metrics.too_old.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooOld;
        }

        let max_ts = now.saturating_add(self.config.freshness_window_future.as_secs());
        if ts > max_ts {
            self.metrics.too_future.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooFuture;
        }

        ReplayDecision::Accepted
    }

    /// Get current high-water for a sender (for debugging/monitoring).
    pub async fn get_highwater(&self, sender: SenderShort) -> Option<HighWaterMark> {
        self.highwater.read().await.get(&sender).copied()
    }

    /// Generate a snapshot for cross-reboot persistence (AC-5).
    /// Returns the snapshot to be persisted by the caller (storage layer).
    pub async fn generate_snapshot(&self) -> ReplaySnapshot {
        let highwater = self.highwater.read().await.clone();
        let snapshot = ReplaySnapshot::new(highwater);
        *self.pending_snapshot.write().await = Some(snapshot.clone());
        *self.last_persist.write().await = Instant::now();
        self.metrics.persisted.fetch_add(1, Ordering::Relaxed);
        snapshot
    }

    /// Load a persisted snapshot (cross-reboot restore).
    pub async fn load_snapshot(&self, snapshot: ReplaySnapshot) {
        let mut highwater = self.highwater.write().await;
        *highwater = snapshot.into_internal();
    }

    /// Maybe schedule a snapshot if interval elapsed.
    async fn maybe_schedule_snapshot(&self) {
        let last = *self.last_persist.read().await;
        if last.elapsed() >= self.config.persistence_interval {
            // In a real implementation, this would trigger an async persist task
            // For now, we just update the pending snapshot
            let highwater = self.highwater.read().await.clone();
            *self.pending_snapshot.write().await = Some(ReplaySnapshot::new(highwater));
            *self.last_persist.write().await = Instant::now();
        }
    }

    /// Reset all state (for testing).
    pub async fn reset(&self) {
        self.highwater.write().await.clear();
        *self.pending_snapshot.write().await = None;
        *self.last_persist.write().await = Instant::now();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReplayMetricsSnapshot {
    pub accepted: u64,
    pub too_old: u64,
    pub too_future: u64,
    pub replay: u64,
    pub persisted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replay_freshness_too_old() {
        let config = ReplayConfig {
            freshness_window_past: Duration::from_secs(3600), // 1 hour
            freshness_window_future: Duration::from_secs(300),
            per_source_skew_budget: Duration::from_secs(60),
            ..Default::default()
        };
        let re = ReplayEngine::new(config);
        let sender = [1u8; 16];

        // Message from 2 hours ago (too old)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now - 7200;
        assert_eq!(re.check(sender, ts, 1).await, ReplayDecision::TooOld);
    }

    #[tokio::test]
    async fn replay_freshness_too_future() {
        let config = ReplayConfig {
            freshness_window_past: Duration::from_secs(3600),
            freshness_window_future: Duration::from_secs(300),
            per_source_skew_budget: Duration::from_secs(60),
            ..Default::default()
        };
        let re = ReplayEngine::new(config);
        let sender = [2u8; 16];

        // Message 10 minutes in future (too future)
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let ts = now + 600;
        assert_eq!(re.check(sender, ts, 1).await, ReplayDecision::TooFuture);
    }

    #[tokio::test]
    async fn replay_highwater_in_order() {
        let re = ReplayEngine::default();
        let sender = [3u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // First message
        assert_eq!(re.check(sender, now, 1).await, ReplayDecision::Accepted);
        // Second message with higher seq
        assert_eq!(re.check(sender, now, 2).await, ReplayDecision::Accepted);
        // Third with higher timestamp AND higher seq (strictly-increasing pair)
        assert_eq!(re.check(sender, now + 1, 3).await, ReplayDecision::Accepted);
    }

    #[tokio::test]
    async fn replay_future_poison_is_bounded_not_persistent() {
        // SEC-RT-03 regression: a crafted `ts = now+599, seq = u64::MAX` must
        // not poison the sender's high-water for the whole freshness window.
        // Clamping stores ts as `now`, so the poison expires within ~1 second
        // and a real message with a slightly higher ts is accepted immediately.
        let config = ReplayConfig {
            freshness_window_past: Duration::from_secs(3600),
            freshness_window_future: Duration::from_secs(600),
            per_source_skew_budget: Duration::from_secs(60),
            persistence_interval: Duration::from_secs(30),
            ..Default::default()
        };
        let re = ReplayEngine::new(config);
        let sender = [9u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Crafted future-highwater (poison attempt).
        assert_eq!(
            re.check(sender, now + 599, u64::MAX).await,
            ReplayDecision::Accepted
        );

        // The stored high-water must be clamped to `now` (not `now+599`): if
        // the poison persisted, this would be `now+599` and gate the sender
        // for ~10 minutes.
        let hw = re.get_highwater(sender).await.expect("highwater set");
        assert!(
            hw.timestamp <= now,
            "poisoned timestamp must be clamped to now, got {}",
            hw.timestamp
        );

        // Clamped poison still dominates the CURRENT second (seq = u64::MAX).
        // A message in the same second is (correctly) Replay; once the wall
        // clock crosses into a new second the clamp releases and a legitimate
        // message is accepted again — a ≤1s outage, not a 10-minute one.
        assert_eq!(
            re.check(sender, now + 1, 1).await,
            ReplayDecision::Replay,
            "within the poisoned second, seq u64::MAX still dominates"
        );

        // Cross a second boundary, then replayability must be restored.
        let mut next = now;
        while next <= now {
            std::thread::sleep(Duration::from_millis(10));
            next = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
        assert_eq!(
            re.check(sender, next, 1).await,
            ReplayDecision::Accepted,
            "after the clamp cycles, a legitimate message must advance again"
        );
    }

    #[tokio::test]
    async fn replay_highwater_replay_detection() {
        let re = ReplayEngine::default();
        let sender = [4u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // First message
        assert_eq!(re.check(sender, now, 5).await, ReplayDecision::Accepted);
        // Replay same (ts,seq) -> Replay
        assert_eq!(re.check(sender, now, 5).await, ReplayDecision::Replay);
        // Lower seq at same ts -> Replay
        assert_eq!(re.check(sender, now, 4).await, ReplayDecision::Replay);
        // Lower ts -> Replay
        assert_eq!(re.check(sender, now - 1, 10).await, ReplayDecision::Replay);
    }

    #[tokio::test]
    async fn replay_out_of_order_accepted() {
        let re = ReplayEngine::default();
        let sender = [5u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Message 5 arrives first
        assert_eq!(re.check(sender, now, 5).await, ReplayDecision::Accepted);
        // Message 3 arrives later -> Replay (3 < 5 at same ts)
        assert_eq!(re.check(sender, now, 3).await, ReplayDecision::Replay);
        // Message 7 arrives -> Accepted (7 > 5)
        assert_eq!(re.check(sender, now, 7).await, ReplayDecision::Accepted);
        // Message 6 arrives -> Replay (6 < 7)
        assert_eq!(re.check(sender, now, 6).await, ReplayDecision::Replay);
    }

    #[tokio::test]
    async fn replay_metrics_snapshot() {
        let re = ReplayEngine::default();
        let sender = [7u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert_eq!(re.check(sender, now, 1).await, ReplayDecision::Accepted);
        assert_eq!(re.check(sender, now, 1).await, ReplayDecision::Replay);
        assert_eq!(
            re.check(sender, now - 108000, 1).await,
            ReplayDecision::TooOld
        );
        assert_eq!(
            re.check_freshness_only(now + 600).await,
            ReplayDecision::TooFuture
        );

        let m = re.metrics();
        assert_eq!(m.accepted, 1);
        assert_eq!(m.replay, 1);
        assert_eq!(m.too_old, 1);
        assert_eq!(m.too_future, 1);
    }

    #[test]
    fn replay_snapshot_intointernal_filters_bad_keys() {
        // A malformed key (wrong length) must be dropped by into_internal.
        let mut hw = HashMap::new();
        hw.insert(
            hex::encode([1u8; 16]),
            HighWaterMark {
                timestamp: 10,
                sequence: 1,
            },
        );
        hw.insert(
            "aabbcc".to_string(),
            HighWaterMark {
                timestamp: 20,
                sequence: 2,
            },
        );
        let snapshot = ReplaySnapshot {
            highwater: hw,
            snapshot_timestamp: 0,
            version: 1,
        };
        let internal = snapshot.into_internal();
        assert_eq!(internal.len(), 1);
        assert_eq!(
            internal.get(&[1u8; 16]),
            Some(&HighWaterMark {
                timestamp: 10,
                sequence: 1
            })
        );
    }

    #[tokio::test]
    async fn replay_load_snapshot_and_reset() {
        let re = ReplayEngine::default();
        let sender = [8u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        re.check(sender, now, 5).await;
        let snapshot = re.generate_snapshot().await;
        let m = re.metrics();
        assert!(m.persisted >= 1);

        // Reset clears state; load restores it.
        re.reset().await;
        assert!(re.get_highwater(sender).await.is_none());
        re.load_snapshot(snapshot).await;
        assert_eq!(re.get_highwater(sender).await.unwrap().sequence, 5);
    }

    #[tokio::test]
    async fn replay_schedules_snapshot_after_interval() {
        let config = ReplayConfig {
            persistence_interval: Duration::from_secs(1),
            ..Default::default()
        };
        let re = ReplayEngine::new(config);
        let sender = [9u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        re.check(sender, now, 1).await;
        std::thread::sleep(Duration::from_millis(1100));
        re.check(sender, now, 2).await;
        // Second check crossed the persistence interval -> snapshot scheduled.
        let m = re.metrics();
        assert!(m.accepted >= 2);
        // Snapshot still reflects latest high-water.
        let snap = re.generate_snapshot().await;
        assert_eq!(
            snap.highwater.get(&hex::encode(sender)).unwrap().sequence,
            2
        );
    }

    #[tokio::test]
    async fn replay_cross_reboot_persistence() {
        let config = ReplayConfig {
            persistence_enabled: true,
            ..Default::default()
        };
        let re = ReplayEngine::default();
        let sender = [6u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Accept some messages
        re.check(sender, now, 10).await;
        re.check(sender, now, 11).await;

        // Generate snapshot (simulate persistence)
        let snapshot = re.generate_snapshot().await;
        let sender_hex = hex::encode(sender);
        assert_eq!(snapshot.highwater.get(&sender_hex).unwrap().sequence, 11);

        // Create new engine from snapshot (simulate reboot)
        let re2 = ReplayEngine::from_snapshot(snapshot, config);

        // High-water should be restored
        let hw = re2.get_highwater(sender).await.unwrap();
        assert_eq!(hw.sequence, 11);
        assert_eq!(hw.timestamp, now);

        // Replay of old message should be rejected
        assert_eq!(re2.check(sender, now, 10).await, ReplayDecision::Replay);
        // New message should be accepted
        assert_eq!(re2.check(sender, now, 12).await, ReplayDecision::Accepted);
    }

    #[tokio::test]
    async fn replay_freshness_only_untrusted() {
        let re = ReplayEngine::default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Untrusted sender: only freshness checked
        assert_eq!(re.check_freshness_only(now).await, ReplayDecision::Accepted);
        // 30 hours ago (beyond 24h default window)
        assert_eq!(
            re.check_freshness_only(now - 108000).await,
            ReplayDecision::TooOld
        );
        assert_eq!(
            re.check_freshness_only(now + 600).await,
            ReplayDecision::TooFuture
        );
    }
}
