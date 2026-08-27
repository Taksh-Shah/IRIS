//! Recently-forwarded dedup window — BASELINE_ROUTING.md §Anti-Loop.
//!
//! Two-generation rolling Bloom filter: each generation covers 24 hours.
//! Together they provide a 24-hour dedup window (up to 48 h overlap during
//! rotation) with a 0.1% FPR, matching the security model in
//! `ROUTING_SECURITY.md §Replay Prevention`. An exact set covers the most
//! recent hour so false positives from the Bloom are confirmed before Drop.
//!
//! The routing window is distinct from the MSG-001 ingestion dedup engine,
//! which dedupes *received* messages at the engine boundary.

use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::message_engine::dedup::{BloomFilter, BLOOM_FPR};
use crate::protocol::MessageId;

/// A point in time: either wall clock or the SIM virtual clock (ROUT-30;
/// mirrors `prophet.rs`'s `TimePoint`). Entries store elapsed `Duration`
/// since this clock's reference point rather than raw `Instant`s so a test
/// can fast-forward past `EXACT_WINDOW`/`BLOOM_WINDOW` deterministically.
#[derive(Debug, Clone)]
enum TimePoint {
    Wall(Instant),
    Virtual(Arc<AtomicU64>),
}

impl TimePoint {
    fn now(&self) -> Duration {
        match self {
            TimePoint::Wall(epoch) => epoch.elapsed(),
            TimePoint::Virtual(clock) => Duration::from_secs(clock.load(Ordering::Relaxed)),
        }
    }
}

/// Exact-set retention window (recent 1 h — confirmer for bloom hits).
const EXACT_WINDOW: Duration = Duration::from_secs(3600);

/// Bloom generation lifetime — rotate every 24 h.
const BLOOM_WINDOW: Duration = Duration::from_secs(24 * 3600);

/// Per-generation capacity: 200 k message ids, 0.1% FPR → ~351 KB.
const WINDOW_CAPACITY: usize = 200_000;

/// Recently-forwarded message window.
#[derive(Debug)]
pub struct ForwardedCache {
    /// Current bloom generation (0 … BLOOM_WINDOW ago).
    bloom_cur: BloomFilter,
    /// Previous bloom generation (BLOOM_WINDOW … 2·BLOOM_WINDOW ago).
    bloom_prev: BloomFilter,
    /// When `bloom_cur` was started (elapsed since `clock`'s reference);
    /// used to trigger rotation.
    bloom_rotated_at: Duration,
    exact: HashSet<MessageId>,
    ring: VecDeque<(MessageId, Duration)>,
    clock: TimePoint,
}

impl Default for ForwardedCache {
    fn default() -> Self {
        let clock = TimePoint::Wall(Instant::now());
        ForwardedCache {
            bloom_cur: BloomFilter::new(WINDOW_CAPACITY, BLOOM_FPR),
            bloom_prev: BloomFilter::new(WINDOW_CAPACITY, BLOOM_FPR),
            bloom_rotated_at: clock.now(),
            exact: HashSet::new(),
            ring: VecDeque::new(),
            clock,
        }
    }
}

impl ForwardedCache {
    /// Attach the SIM virtual clock (ROUT-30 / SIM-001 determinism). Must be
    /// called immediately after construction — resets `bloom_rotated_at` to
    /// the virtual clock's current reading.
    pub fn with_virtual_clock(mut self, clock: Arc<AtomicU64>) -> Self {
        self.clock = TimePoint::Virtual(clock);
        self.bloom_rotated_at = self.clock.now();
        self
    }

    /// True if this message id was recorded within the 24-hour window.
    ///
    /// ROUT-10 fix: the bloom is the long window; exact is the 1-hour
    /// false-positive confirmer.
    ///
    /// - Not in either bloom generation → definitely not a duplicate.
    /// - In exact (≤ 1 h ago) → confirmed duplicate.
    /// - Bloom hit but not in exact (1 h … 24 h ago) → treat as duplicate,
    ///   accepting the 0.1% FPR the design specifies.
    pub fn is_duplicate(&self, id: &MessageId) -> bool {
        let bytes = id.to_bytes();
        if !self.bloom_cur.contains(bytes) && !self.bloom_prev.contains(bytes) {
            return false;
        }
        if self.exact.contains(id) {
            return true;
        }
        // Bloom hit outside the 1-hour exact window: seen 1–24 h ago.
        // Treat as duplicate (0.1% FPR accepted by design).
        true
    }

    /// Record a forwarded message id. O(1) amortized.
    pub fn record(&mut self, id: MessageId) {
        self.rotate_bloom_if_due();
        self.bloom_cur.insert(id.to_bytes());
        self.exact.insert(id);
        self.ring.push_back((id, self.clock.now()));
        self.evict_old();
        // ROUT-15: enforce count-based cap so an attacker minting message ids
        // at high speed cannot grow exact/ring beyond WINDOW_CAPACITY entries.
        // The bloom stays (it is fixed-size by construction); exact and ring
        // must also respect the bound.
        while self.ring.len() > WINDOW_CAPACITY {
            if let Some((evicted, _)) = self.ring.pop_front() {
                self.exact.remove(&evicted);
            }
        }
    }

    /// Rotate bloom generations when the current one has exceeded 24 h.
    fn rotate_bloom_if_due(&mut self) {
        // saturating_sub rather than a raw cutoff subtraction to avoid the
        // boot-time underflow panic pattern (SYS-1).
        let now = self.clock.now();
        if now.saturating_sub(self.bloom_rotated_at) >= BLOOM_WINDOW {
            self.bloom_prev = std::mem::replace(
                &mut self.bloom_cur,
                BloomFilter::new(WINDOW_CAPACITY, BLOOM_FPR),
            );
            self.bloom_rotated_at = now;
        }
    }

    /// Ring-buffer cleanup: remove entries older than the exact window.
    fn evict_old(&mut self) {
        // saturating_sub rather than a raw cutoff subtraction — same
        // boot-time underflow hazard as SYS-1 (`neighbor_table.rs`).
        let now = self.clock.now();
        while let Some(front) = self.ring.front() {
            if now.saturating_sub(front.1) > EXACT_WINDOW {
                let (id, _) = self.ring.pop_front().unwrap();
                self.exact.remove(&id);
            } else {
                break;
            }
        }
    }

    pub fn len(&self) -> usize {
        self.exact.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }

    #[cfg(test)]
    pub fn ring_len(&self) -> usize {
        self.ring.len()
    }

    #[cfg(test)]
    pub fn exact_len(&self) -> usize {
        self.exact.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_detection_works() {
        let mut cache = ForwardedCache::default();
        let id = MessageId::new_v7();
        assert!(!cache.is_duplicate(&id));
        cache.record(id);
        assert!(cache.is_duplicate(&id));
    }

    #[test]
    fn different_ids_not_duplicates() {
        let mut cache = ForwardedCache::default();
        let a = MessageId::new_v7();
        let b = MessageId::new_v7();
        cache.record(a);
        assert!(!cache.is_duplicate(&b));
    }

    #[test]
    fn ring_eviction_removes_expired_exact_entries() {
        // ROUT-30: the previous version of this test asserted `len() >= 10`
        // both before and after evict_old() with no time advanced — a cache
        // that never evicted anything (or that grew) would also pass. This
        // version uses the virtual clock to advance past EXACT_WINDOW and
        // asserts the entries are actually gone.
        let clock = Arc::new(AtomicU64::new(0));
        let mut cache = ForwardedCache::default().with_virtual_clock(clock.clone());
        for _ in 0..10 {
            cache.record(MessageId::new_v7());
        }
        assert_eq!(cache.len(), 10);

        // Still within the 1h exact window: nothing evicted.
        clock.store(EXACT_WINDOW.as_secs() - 1, Ordering::Relaxed);
        cache.evict_old();
        assert_eq!(cache.len(), 10, "entries within the exact window must remain");
        assert_eq!(cache.ring_len(), 10);

        // Past the 1h window: every entry evicted.
        clock.store(EXACT_WINDOW.as_secs() + 1, Ordering::Relaxed);
        cache.evict_old();
        assert_eq!(cache.len(), 0, "entries past EXACT_WINDOW must be evicted");
        assert_eq!(cache.ring_len(), 0);
    }

    #[test]
    fn rout15_exact_and_ring_bounded_by_window_capacity() {
        // Regression: exact and ring were unbounded within the 1-h time window;
        // an attacker minting message ids at high speed could exhaust RAM (ROUT-15).
        let mut cache = ForwardedCache::default();
        // Insert just past WINDOW_CAPACITY to trigger eviction.
        for _ in 0..(WINDOW_CAPACITY + 5) {
            cache.record(MessageId::new_v7());
        }
        assert!(
            cache.ring_len() <= WINDOW_CAPACITY,
            "ring must be capped at WINDOW_CAPACITY (got {})",
            cache.ring_len()
        );
        assert!(
            cache.exact_len() <= WINDOW_CAPACITY,
            "exact must be capped at WINDOW_CAPACITY (got {})",
            cache.exact_len()
        );
    }

    #[test]
    fn rout10_bloom_hit_outside_exact_window_is_duplicate() {
        // Regression: previously bloom.contains && exact.contains collapsed to
        // exact.contains — a message seen 90 min ago was not a duplicate.
        // After ROUT-10 a bloom hit with no exact entry is still duplicate.
        let mut cache = ForwardedCache::default();
        let id = MessageId::new_v7();
        // Insert directly into bloom_cur without adding to exact.
        cache.bloom_cur.insert(id.to_bytes());
        // exact does not contain it (simulates post-1h eviction).
        assert!(
            cache.is_duplicate(&id),
            "bloom hit outside exact window must be treated as duplicate (ROUT-10)"
        );
    }
}
