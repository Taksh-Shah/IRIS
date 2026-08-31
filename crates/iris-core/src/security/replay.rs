//! Replay protection: freshness window + per-sender high-water (ts,seq) + cross-reboot persistence.
//! RES-0018 R4/R5/R10, RFC 9171 §4.2.7, RFC 4303 §3.4.3, RFC 7181 §23.2, RFC 7183, RFC 6479.
//! Promotes WP-2 deferred dedup persistence to a security requirement (AC-3/4/5).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;

/// 16-byte sender short ID.
pub type SenderShort = [u8; 16];

/// Current on-disk `ReplaySnapshot` format version. `load_snapshot` refuses
/// any other value rather than misinterpreting a future layout as v1 (PRY-15).
pub const SNAPSHOT_VERSION: u32 = 1;

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
///
/// PRY-7 (known limitation): the engine currently supplies `seq` as
/// [`crate::protocol::MessageId::sequence_hint`] — the first 8 bytes of a
/// UUIDv7-style id, i.e. a millisecond timestamp followed by ~16 random bits.
/// It is **not** a per-sender monotonic counter: two honest messages minted in
/// the same millisecond and arriving in the same wall-clock second are ordered
/// by a random tiebreak, so the numerically-smaller one is rejected as
/// `Replay` regardless of send order. A burst sender (SOS retransmit train,
/// multi-part status, re-fragmented ADUs) loses ~50 % of any millisecond
/// collision. Fixing this correctly means either carrying a real counter or
/// switching to a bounded per-`(sender, ts)` seen-set — both **widen** an
/// accept path and are gated (Tier 2, needs the Section 2 `message_id` owner).
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
                .unwrap_or_default()
                .as_secs(),
            version: SNAPSHOT_VERSION,
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
            .unwrap_or_default()
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

/// Per-sender high-water table plus the local, **unforgeable** registration
/// order used for adversary-resistant eviction (PRY-8). `first_seen` is a
/// monotonic local counter — unlike a mark's `timestamp`, an attacker cannot
/// backdate it to dodge eviction.
#[derive(Default)]
struct HighwaterTable {
    marks: HashMap<SenderShort, HighWaterMark>,
    first_seen: HashMap<SenderShort, u64>,
    next_gen: u64,
}

impl HighwaterTable {
    fn from_marks(marks: HashMap<SenderShort, HighWaterMark>) -> Self {
        let mut next_gen = 0u64;
        let first_seen = marks
            .keys()
            .map(|k| {
                let g = next_gen;
                next_gen += 1;
                (*k, g)
            })
            .collect();
        HighwaterTable {
            marks,
            first_seen,
            next_gen,
        }
    }

    fn remove(&mut self, sender: &SenderShort) {
        self.marks.remove(sender);
        self.first_seen.remove(sender);
    }
}

/// Replay protection engine.
pub struct ReplayEngine {
    /// Per-sender high-water marks (in-memory, hot path).
    highwater: RwLock<HighwaterTable>,
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
            highwater: RwLock::new(HighwaterTable::default()),
            config,
            metrics: ReplayMetrics::default(),
            pending_snapshot: RwLock::new(None),
            last_persist: RwLock::new(Instant::now()),
        }
    }

    /// Create from a persisted snapshot (cross-reboot restore).
    pub fn from_snapshot(snapshot: ReplaySnapshot, config: ReplayConfig) -> Self {
        ReplayEngine {
            highwater: RwLock::new(HighwaterTable::from_marks(snapshot.into_internal())),
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
    /// `seq` = message sequence. See [`HighWaterMark`] — PRY-7: this is
    ///   currently `MessageId::sequence_hint()`, an id-derived value that is
    ///   only *roughly* monotonic across milliseconds, not a true per-sender
    ///   counter.
    /// Returns ReplayDecision.
    pub async fn check(&self, sender: SenderShort, ts: u64, seq: u64) -> ReplayDecision {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Freshness window: too old?
        // GAP-6: correct formula is now - window - skew (accept slow-clock senders).
        // The old formula (now + skew - window) shrunk the past window by skew_budget.
        let min_ts = now
            .saturating_sub(self.config.freshness_window_past.as_secs())
            .saturating_sub(self.config.per_source_skew_budget.as_secs());
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
            let mut table = self.highwater.write().await;

            // Enforce max senders (PRY-8). Minting an Ed25519 identity is free
            // (a node id *is* its public key), so an attacker can register
            // `max_sender_highwater` throwaway senders and try to flush a
            // victim's mark — after which an old replayed message reads as a
            // first sighting.
            //
            //  1. Prefer evicting marks already outside the freshness window
            //     (stale) — the window alone rejects whatever they protected.
            //  2. Otherwise evict the sender with the highest `first_seen`
            //     generation: the *most recently registered*. `first_seen` is
            //     a local monotonic counter, so — unlike a mark's `timestamp`,
            //     which the attacker can backdate — the flood's own entries
            //     are always the newest and get evicted first. A genuine
            //     long-lived sender's in-window mark is preserved.
            if table.marks.len() >= self.config.max_sender_highwater
                && !table.marks.contains_key(&sender)
            {
                let victim = table
                    .marks
                    .iter()
                    .filter(|(_, hw)| hw.timestamp < min_ts)
                    .min_by_key(|(_, hw)| hw.timestamp)
                    .map(|(k, _)| *k)
                    .or_else(|| {
                        table
                            .first_seen
                            .iter()
                            .max_by_key(|(_, gen)| **gen)
                            .map(|(k, _)| *k)
                    });
                if let Some(k) = victim {
                    table.remove(&k);
                }
            }

            if !table.marks.contains_key(&sender) {
                let gen = table.next_gen;
                table.next_gen += 1;
                table.first_seen.insert(sender, gen);
            }
            table
                .marks
                .entry(sender)
                .or_default()
                .check_and_advance(stored_ts, seq)
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
            .unwrap_or_default()
            .as_secs();

        // GAP-6: expand past window by skew_budget to accommodate slow-clock
        // senders, matching check()'s corrected past cutoff.
        // PRY-27: apply the same skew budget to the FUTURE bound too. Untrusted
        // senders (a stranger's phone in a disaster) are the population least
        // likely to have an NTP-synced clock; a clock fast by < skew_budget was
        // hard-dropped `TooFuture` here while the identical device is accepted
        // once it becomes a tracked sender. A future-dated message is not a
        // replay, and this path never advances durable state.
        let min_ts = now
            .saturating_sub(self.config.freshness_window_past.as_secs())
            .saturating_sub(self.config.per_source_skew_budget.as_secs());
        if ts < min_ts {
            self.metrics.too_old.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooOld;
        }

        let max_ts = now
            .saturating_add(self.config.freshness_window_future.as_secs())
            .saturating_add(self.config.per_source_skew_budget.as_secs());
        if ts > max_ts {
            self.metrics.too_future.fetch_add(1, Ordering::Relaxed);
            return ReplayDecision::TooFuture;
        }

        ReplayDecision::Accepted
    }

    /// Get current high-water for a sender (for debugging/monitoring).
    pub async fn get_highwater(&self, sender: SenderShort) -> Option<HighWaterMark> {
        self.highwater.read().await.marks.get(&sender).copied()
    }

    /// Generate a snapshot for cross-reboot persistence (AC-5).
    /// Returns the snapshot to be persisted by the caller (storage layer).
    pub async fn generate_snapshot(&self) -> ReplaySnapshot {
        let highwater = self.highwater.read().await.marks.clone();
        let snapshot = ReplaySnapshot::new(highwater);
        *self.pending_snapshot.write().await = Some(snapshot.clone());
        *self.last_persist.write().await = Instant::now();
        self.metrics.persisted.fetch_add(1, Ordering::Relaxed);
        snapshot
    }

    /// Load a persisted snapshot (cross-reboot restore).
    ///
    /// PRY-15: this is a `pub` runtime method, not a boot-only one. It must
    /// never *regress* anti-replay state: a blind `*highwater = ...` would
    /// discard every mark accumulated since boot (replay window reopens for all
    /// tracked senders) and, fed a stale snapshot, would roll marks backward
    /// (every message since that snapshot becomes replayable). So: reject an
    /// unknown `version`, and **merge** by taking the per-sender max of
    /// stored vs incoming `(ts, seq)` — advance, never rewind.
    pub async fn load_snapshot(&self, snapshot: ReplaySnapshot) {
        if snapshot.version != SNAPSHOT_VERSION {
            tracing::warn!(
                version = snapshot.version,
                expected = SNAPSHOT_VERSION,
                "replay: ignoring snapshot with unrecognised version"
            );
            return;
        }
        let incoming = snapshot.into_internal();
        let mut table = self.highwater.write().await;
        for (sender, mark) in incoming {
            if !table.marks.contains_key(&sender) {
                let gen = table.next_gen;
                table.next_gen += 1;
                table.first_seen.insert(sender, gen);
            }
            table
                .marks
                .entry(sender)
                .or_default()
                .check_and_advance(mark.timestamp, mark.sequence);
        }
    }

    /// Maybe schedule a snapshot if interval elapsed.
    async fn maybe_schedule_snapshot(&self) {
        let last = *self.last_persist.read().await;
        if last.elapsed() >= self.config.persistence_interval {
            // In a real implementation, this would trigger an async persist task
            // For now, we just update the pending snapshot
            let highwater = self.highwater.read().await.marks.clone();
            *self.pending_snapshot.write().await = Some(ReplaySnapshot::new(highwater));
            *self.last_persist.write().await = Instant::now();
        }
    }

    /// Reset all state (for testing).
    pub async fn reset(&self) {
        *self.highwater.write().await = HighwaterTable::default();
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

    /// PRY-7 (documents current behaviour, not desired behaviour): because
    /// `seq` is an id-derived hint rather than a real counter, two same-second
    /// messages presented out of `seq` order — exactly what a millisecond
    /// collision on `MessageId::sequence_hint()` produces — reject the second
    /// one as `Replay` even though it is a distinct honest message. A future
    /// Tier 2 fix that carries a true counter must consciously flip this
    /// assertion (it widens an accept path — see `HighWaterMark` docs).
    #[test]
    fn same_timestamp_lower_seq_is_rejected_after_higher() {
        let mut hw = HighWaterMark::default();
        assert!(hw.check_and_advance(1_000, 500), "first message accepted");
        assert!(
            !hw.check_and_advance(1_000, 400),
            "same ts, lower seq — currently Replay (PRY-7 false reject)"
        );
        assert!(
            hw.check_and_advance(1_001, 1),
            "next second always wins regardless of seq"
        );
    }

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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
    async fn stale_highwater_marks_are_evicted_before_live_ones() {
        // Regression: eviction used `HashMap::keys().next()` — arbitrary order.
        // Minting an Ed25519 identity is free (a node id IS its public key), so
        // an attacker could register `max_sender_highwater` throwaway senders
        // and flush a victim's mark, after which an old replayed message read
        // as a first sighting.
        let cfg = ReplayConfig {
            max_sender_highwater: 8,
            ..Default::default()
        };
        let re = ReplayEngine::new(cfg);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // A live victim with a recent mark.
        let victim = [0xAB_u8; 16];
        assert_eq!(re.check(victim, now, 100).await, ReplayDecision::Accepted);

        // Flood with throwaway senders, all older than the victim.
        for i in 0..32u8 {
            let attacker = [i; 16];
            if attacker == victim {
                continue;
            }
            let _ = re.check(attacker, now - 600, 1).await;
        }

        // The victim's mark must have survived, so replaying its old message
        // is still rejected.
        assert_eq!(
            re.check(victim, now, 100).await,
            ReplayDecision::Replay,
            "a flood of throwaway senders must not flush a live sender's mark"
        );
    }

    /// PRY-8: the all-fresh Sybil flood — every attacker mark is inside the
    /// freshness window (none stale), so eviction falls to the tiebreak. It
    /// must evict the *newest* mark (an attacker's), never the victim's
    /// slightly-older but still-in-window mark.
    #[tokio::test]
    async fn sybil_flood_does_not_flush_in_window_victim() {
        let cfg = ReplayConfig {
            max_sender_highwater: 8,
            ..Default::default()
        };
        let re = ReplayEngine::new(cfg);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Victim spoke 5 s ago — well inside the window.
        let victim = [0xAB_u8; 16];
        assert_eq!(re.check(victim, now - 5, 100).await, ReplayDecision::Accepted);

        // Flood: 40 throwaway identities, every mark fresh (ts == now).
        for i in 0..40u32 {
            let mut attacker = [0u8; 16];
            attacker[..4].copy_from_slice(&i.to_le_bytes());
            attacker[15] = 0xFF; // keep clear of the victim key
            let _ = re.check(attacker, now, 1).await;
        }

        // Victim's mark survived — replaying its old message is still Replay.
        assert_eq!(
            re.check(victim, now - 5, 100).await,
            ReplayDecision::Replay,
            "an all-fresh Sybil flood must not flush the victim's in-window mark"
        );
    }

    #[tokio::test]
    async fn replay_out_of_order_accepted() {
        let re = ReplayEngine::default();
        let sender = [5u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
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
            .unwrap_or_default()
            .as_secs();

        assert_eq!(re.check(sender, now, 1).await, ReplayDecision::Accepted);
        assert_eq!(re.check(sender, now, 1).await, ReplayDecision::Replay);
        assert_eq!(
            re.check(sender, now - 108000, 1).await,
            ReplayDecision::TooOld
        );
        // PRY-27: future bound is now future_window (300) + skew_budget (300).
        assert_eq!(
            re.check_freshness_only(now + 601).await,
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
            .unwrap_or_default()
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

    /// PRY-15: a runtime `load_snapshot` must not roll a live mark backward.
    #[tokio::test]
    async fn load_snapshot_never_regresses_a_live_mark() {
        let re = ReplayEngine::default();
        let sender = [0x5Au8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // Mark advances to seq 10 at `now`.
        assert_eq!(re.check(sender, now, 10).await, ReplayDecision::Accepted);
        let stale = re.generate_snapshot().await; // captures seq 10
        assert_eq!(re.check(sender, now, 25).await, ReplayDecision::Accepted); // live seq 25

        // Re-applying the older snapshot must NOT drop the mark back to 10.
        re.load_snapshot(stale).await;
        assert_eq!(re.get_highwater(sender).await.unwrap().sequence, 25);
        // And a replay of seq 20 (< 25) is still rejected.
        assert_eq!(re.check(sender, now, 20).await, ReplayDecision::Replay);
    }

    /// PRY-15: an unrecognised snapshot version is ignored, not loaded as v1.
    #[tokio::test]
    async fn load_snapshot_rejects_unknown_version() {
        let re = ReplayEngine::default();
        let sender = [0x77u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        re.check(sender, now, 7).await;

        let mut snap = re.generate_snapshot().await;
        snap.version = 999;
        snap.highwater
            .insert(hex::encode([0xAB; 16]), HighWaterMark { timestamp: now, sequence: 1 });

        re.load_snapshot(snap).await;
        // Nothing from the bad-version snapshot was applied.
        assert!(re.get_highwater([0xAB; 16]).await.is_none());
        // The pre-existing live mark is untouched.
        assert_eq!(re.get_highwater(sender).await.unwrap().sequence, 7);
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
            .unwrap_or_default()
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
            .unwrap_or_default()
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
            .unwrap_or_default()
            .as_secs();

        // Untrusted sender: only freshness checked
        assert_eq!(re.check_freshness_only(now).await, ReplayDecision::Accepted);
        // 30 hours ago (beyond 24h default window)
        assert_eq!(
            re.check_freshness_only(now - 108000).await,
            ReplayDecision::TooOld
        );
        // PRY-27: the future bound now includes per_source_skew_budget (default
        // 300s future window + 300s skew = 600s). A clock fast by 7.5 min — an
        // unsynced stranger's phone, the population this path serves — is now
        // accepted (was a hard TooFuture drop).
        assert_eq!(
            re.check_freshness_only(now + 450).await,
            ReplayDecision::Accepted,
            "within future_window + skew_budget"
        );
        // Still bounded: past the combined budget is TooFuture.
        assert_eq!(
            re.check_freshness_only(now + 601).await,
            ReplayDecision::TooFuture
        );
    }
}
