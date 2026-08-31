//! Replay protection: freshness window + per-sender high-water (ts,seq) + cross-reboot persistence.
//! RES-0018 R4/R5/R10, RFC 9171 §4.2.7, RFC 4303 §3.4.3, RFC 7181 §23.2, RFC 7183, RFC 6479.
//! Promotes WP-2 deferred dedup persistence to a security requirement (AC-3/4/5).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::RwLock;

use crate::security::sharded::{per_shard_cap, shard_index_for, ShardedLocks, SHARD_COUNT};

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
/// PRY-7: the engine supplies `seq` as
/// [`crate::protocol::MessageId::sequence_hint`] — the first 8 bytes of a
/// UUIDv7-style id (a millisecond timestamp + ~16 random bits), **not** a
/// per-sender monotonic counter. So `ReplayEngine::check` does NOT order by
/// `seq` within a second: it advances `HighWaterMark` across seconds and, for
/// the current second, deduplicates against a bounded exact seen-set of
/// `sequence_hint` values (`HighwaterTable::admit` / `recent_seqs`, cap
/// [`MAX_SEEN_SEQ_PER_SECOND`]). `check_and_advance` below keeps the strict
/// `(ts, seq)` comparison and is used only for the cross-reboot snapshot merge
/// (`load_snapshot`), where exact-set state is not carried.
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
            // PRY-7: the same-second exact seen-set is transient (not in the
            // snapshot), so a same-second replay right after reboot is accepted
            // by design. The cross-second guarantee is what matters and is
            // preserved — an earlier-second replay is still rejected.
            prop_assert_eq!(
                block_on(re2.check(sender, base_ts.saturating_sub(1), base_seq)),
                ReplayDecision::Replay
            );
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
    /// PRY-7: bounded exact seen-set of `sequence_hint` values for the current
    /// second, per sender. `sequence_hint` is not a monotonic counter (random
    /// per-millisecond tail), so within one second we *dedup* rather than
    /// *order*. Transient — never serialized into a snapshot.
    recent_seqs: HashMap<SenderShort, (u64, Vec<u64>)>,
}

/// PRY-7: cap on distinct same-second `sequence_hint` values tracked per
/// sender. Beyond this a same-second message is accepted (a >64-message
/// same-second burst is not a replay; `message_id` dedup still catches exact
/// message replays).
const MAX_SEEN_SEQ_PER_SECOND: usize = 64;

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
            recent_seqs: HashMap::new(),
        }
    }

    fn remove(&mut self, sender: &SenderShort) {
        self.marks.remove(sender);
        self.first_seen.remove(sender);
        self.recent_seqs.remove(sender);
    }

    /// PRY-7: timestamp-primary acceptance. Advance across seconds; within the
    /// current second dedup `seq` against a bounded exact seen-set instead of
    /// requiring strict `seq` ordering (which false-rejects same-millisecond
    /// honest bursts, since `sequence_hint` has a random per-ms tail).
    fn admit(&mut self, sender: SenderShort, ts: u64, seq: u64) -> bool {
        let hw = self.marks.entry(sender).or_default();
        let hw_ts = hw.timestamp;
        if ts > hw_ts {
            hw.timestamp = ts;
            hw.sequence = seq;
            self.recent_seqs.insert(sender, (ts, vec![seq]));
            true
        } else if ts == hw_ts {
            if seq > hw.sequence {
                hw.sequence = seq;
            }
            let (rt, seen) = self
                .recent_seqs
                .entry(sender)
                .or_insert_with(|| (ts, Vec::new()));
            if *rt != ts {
                *rt = ts;
                seen.clear();
            }
            if seen.contains(&seq) {
                false // exact (ts, seq) already seen this second — replay
            } else {
                if seen.len() < MAX_SEEN_SEQ_PER_SECOND {
                    seen.push(seq);
                }
                true
            }
        } else {
            false // ts < high-water — stale
        }
    }
}

/// Replay protection engine.
pub struct ReplayEngine {
    /// Per-sender high-water marks (in-memory, hot path). PRY-13: sharded so
    /// two senders contend only when they hash to the same shard. Per-shard
    /// eviction + `max_sender_highwater` cap (see `per_shard_cap`).
    highwater: ShardedLocks<HighwaterTable>,
    config: ReplayConfig,
    metrics: ReplayMetrics,
    /// Pending snapshot for batched persistence.
    pending_snapshot: RwLock<Option<ReplaySnapshot>>,
    /// Last persistence time.
    last_persist: RwLock<Instant>,
    /// PRY-13: set when a mark has advanced since the last snapshot. The
    /// hot path only flips this flag; the actual (all-shard) snapshot clone
    /// happens off the accept path in `generate_snapshot`.
    snapshot_dirty: AtomicBool,
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
            highwater: ShardedLocks::default(),
            config,
            metrics: ReplayMetrics::default(),
            pending_snapshot: RwLock::new(None),
            last_persist: RwLock::new(Instant::now()),
            snapshot_dirty: AtomicBool::new(false),
        }
    }

    /// Create from a persisted snapshot (cross-reboot restore).
    pub fn from_snapshot(snapshot: ReplaySnapshot, config: ReplayConfig) -> Self {
        // Partition the restored marks across shards (sync — no lock yet).
        let mut buckets: Vec<HashMap<SenderShort, HighWaterMark>> =
            (0..SHARD_COUNT).map(|_| HashMap::new()).collect();
        for (sender, mark) in snapshot.into_internal() {
            buckets[shard_index_for(&sender)].insert(sender, mark);
        }
        let shards = buckets.into_iter().map(HighwaterTable::from_marks).collect();
        ReplayEngine {
            highwater: ShardedLocks::from_shards(shards),
            config,
            metrics: ReplayMetrics::default(),
            pending_snapshot: RwLock::new(None),
            last_persist: RwLock::new(Instant::now()),
            snapshot_dirty: AtomicBool::new(false),
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
    /// `seq` = `MessageId::sequence_hint()` — id-derived, not a true per-sender
    ///   counter. Within a second it is used for exact dedup, not ordering
    ///   (PRY-7 — see [`HighWaterMark`]).
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
            let mut table = self.highwater.shard_for(&sender).write().await;
            let shard_cap = per_shard_cap(self.config.max_sender_highwater);

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
            if table.marks.len() >= shard_cap && !table.marks.contains_key(&sender) {
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
            table.admit(sender, stored_ts, seq)
        };

        if accepted {
            self.metrics.accepted.fetch_add(1, Ordering::Relaxed);

            // PRY-13: the hot path only flips a flag. A dedicated persistence
            // task (or the engine GC loop) calls `generate_snapshot` on the
            // interval — the all-shard clone never runs under the accept path.
            if self.config.persistence_enabled {
                self.snapshot_dirty.store(true, Ordering::Relaxed);
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
        self.highwater
            .shard_for(&sender)
            .read()
            .await
            .marks
            .get(&sender)
            .copied()
    }

    /// Generate a snapshot for cross-reboot persistence (AC-5).
    /// Returns the snapshot to be persisted by the caller (storage layer).
    /// PRY-13: this is the *off-hot-path* all-shard clone — call it from the
    /// persistence task / GC loop, not from `check`.
    pub async fn generate_snapshot(&self) -> ReplaySnapshot {
        let mut all: HashMap<SenderShort, HighWaterMark> = HashMap::new();
        for shard in self.highwater.shards() {
            for (k, v) in shard.read().await.marks.iter() {
                all.insert(*k, *v);
            }
        }
        let snapshot = ReplaySnapshot::new(all);
        *self.pending_snapshot.write().await = Some(snapshot.clone());
        *self.last_persist.write().await = Instant::now();
        self.snapshot_dirty.store(false, Ordering::Relaxed);
        self.metrics.persisted.fetch_add(1, Ordering::Relaxed);
        snapshot
    }

    /// PRY-13: whether a mark has advanced since the last `generate_snapshot`.
    /// A persistence driver polls this to decide when to snapshot.
    pub fn snapshot_pending(&self) -> bool {
        self.snapshot_dirty.load(Ordering::Relaxed)
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
        for (sender, mark) in snapshot.into_internal() {
            let mut table = self.highwater.shard_for(&sender).write().await;
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

    /// Maybe schedule a snapshot if the interval elapsed. PRY-13: this only
    /// advances the persistence clock and bumps the metric — it does NOT clone
    /// the map (that is `generate_snapshot`, off the hot path).
    async fn maybe_schedule_snapshot(&self) {
        let last = *self.last_persist.read().await;
        if last.elapsed() >= self.config.persistence_interval {
            *self.last_persist.write().await = Instant::now();
            self.metrics.persisted.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Reset all state (for testing).
    pub async fn reset(&self) {
        for shard in self.highwater.shards() {
            *shard.write().await = HighwaterTable::default();
        }
        *self.pending_snapshot.write().await = None;
        *self.last_persist.write().await = Instant::now();
        self.snapshot_dirty.store(false, Ordering::Relaxed);
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
        // PRY-7 bonus: because same-second acceptance is now an exact seen-set
        // (not `seq` ordering), a crafted `seq = u64::MAX` no longer dominates
        // the second — a distinct legitimate message in the same second is
        // still accepted. The SEC-RT-03 clamp still bounds any ts-poison to ≤1s.
        assert_eq!(
            re.check(sender, now + 1, 1).await,
            ReplayDecision::Accepted,
            "a distinct same-second message is not blocked by a seq-poison"
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
        // Replay of the EXACT same (ts, seq) -> Replay (seen-set dedup)
        assert_eq!(re.check(sender, now, 5).await, ReplayDecision::Replay);
        // PRY-7: a DISTINCT seq in the same second is a distinct message id,
        // not a replay — `sequence_hint` is not a real counter, so the old
        // "lower seq at same ts -> Replay" rule dropped ~50% of honest
        // same-millisecond bursts.
        assert_eq!(re.check(sender, now, 4).await, ReplayDecision::Accepted);
        // ...but replaying that one again IS caught.
        assert_eq!(re.check(sender, now, 4).await, ReplayDecision::Replay);
        // An earlier second is still a stale replay.
        assert_eq!(re.check(sender, now - 1, 10).await, ReplayDecision::Replay);
    }

    /// PRY-7: a same-second burst whose `sequence_hint` values arrive in
    /// descending order (the random per-millisecond tail can do this) must NOT
    /// lose the numerically-smaller ones as false `Replay`.
    #[tokio::test]
    async fn same_second_descending_seq_burst_all_accepted() {
        let re = ReplayEngine::default();
        let sender = [0xC7u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut accepted = 0;
        for seq in (1..=50u64).rev() {
            if re.check(sender, now, seq).await == ReplayDecision::Accepted {
                accepted += 1;
            }
        }
        assert_eq!(accepted, 50, "every distinct same-second message is accepted");
        // Replaying any of them is still caught.
        assert_eq!(re.check(sender, now, 25).await, ReplayDecision::Replay);
    }

    /// PRY-7: the per-second seen-set is bounded — a >64-message same-second
    /// burst does not grow memory without limit, and still accepts.
    #[tokio::test]
    async fn same_second_seen_set_is_bounded() {
        let re = ReplayEngine::default();
        let sender = [0xC8u8; 16];
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        for seq in 0..200u64 {
            assert_eq!(re.check(sender, now, seq).await, ReplayDecision::Accepted);
        }
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

        // PRY-7: same-second messages are deduplicated by exact seq, not
        // ordered — out-of-order delivery within a second is normal and all
        // distinct message ids are accepted; only exact repeats are rejected.
        assert_eq!(re.check(sender, now, 5).await, ReplayDecision::Accepted);
        assert_eq!(re.check(sender, now, 3).await, ReplayDecision::Accepted);
        assert_eq!(re.check(sender, now, 7).await, ReplayDecision::Accepted);
        assert_eq!(re.check(sender, now, 6).await, ReplayDecision::Accepted);
        // Every one of those, replayed, is caught.
        for s in [5u64, 3, 7, 6] {
            assert_eq!(re.check(sender, now, s).await, ReplayDecision::Replay);
        }
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
        // A replay of a seq already seen this second is still rejected...
        assert_eq!(re.check(sender, now, 10).await, ReplayDecision::Replay);
        assert_eq!(re.check(sender, now, 25).await, ReplayDecision::Replay);
        // ...and an earlier-second replay is still rejected.
        assert_eq!(re.check(sender, now - 1, 5).await, ReplayDecision::Replay);
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

        // A replay from an EARLIER second is still rejected — the cross-second
        // guarantee (a day-old message can't be replayed) is fully preserved.
        assert_eq!(
            re2.check(sender, now - 5, 10).await,
            ReplayDecision::Replay
        );
        // PRY-7 tradeoff: the same-second exact seen-set is transient (not in
        // the snapshot), so immediately after a reboot a distinct same-second
        // message is accepted regardless of its `seq` — an extremely narrow
        // window (reboot within one second of the original) that option (2)
        // of the finding explicitly accepts.
        assert_eq!(re2.check(sender, now, 10).await, ReplayDecision::Accepted);
        // Once seen post-reboot, replaying it is caught again.
        assert_eq!(re2.check(sender, now, 10).await, ReplayDecision::Replay);
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
