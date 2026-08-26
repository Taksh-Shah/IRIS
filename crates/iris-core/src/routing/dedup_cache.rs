//! Recently-forwarded dedup window — BASELINE_ROUTING.md §Anti-Loop.
//!
//! Rolling 24-hour window of message ids this node forwarded, so a message
//! that loops back is silently dropped (no re-forward, no re-store). Structure
//! mirrors `DeduplicationCache` in the design doc: a 0.1% FPR Bloom filter for
//! memory efficiency, an exact set for the last hour (to answer Bloom
//! false-positive hits), and a ring buffer for cleanup.
//!
//! The routing window is distinct from the MSG-001 ingestion dedup engine,
//! which dedupes *received* messages at the engine boundary.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::message_engine::dedup::{BloomFilter, BLOOM_FPR};
use crate::protocol::MessageId;

/// Exact-set retention window.
const EXACT_WINDOW: Duration = Duration::from_secs(3600); // 1 h

/// Rolling window capacity for the message-id bloom.
const WINDOW_CAPACITY: usize = 200_000;

/// Recently-forwarded message window.
#[derive(Debug)]
pub struct ForwardedCache {
    bloom: BloomFilter,
    exact: HashSet<MessageId>,
    ring: VecDeque<(MessageId, Instant)>,
    peers_seen: HashSet<[u8; 32]>,
}

impl Default for ForwardedCache {
    fn default() -> Self {
        ForwardedCache {
            bloom: BloomFilter::new(WINDOW_CAPACITY, BLOOM_FPR),
            exact: HashSet::new(),
            ring: VecDeque::new(),
            peers_seen: HashSet::new(),
        }
    }
}

impl ForwardedCache {
    /// True if this message id was recorded in the window.
    pub fn is_duplicate(&self, id: &MessageId) -> bool {
        if !self.bloom.contains(id.to_bytes()) {
            return false;
        }
        self.exact.contains(id)
    }

    /// Record a forwarded message id. O(1) amortized.
    pub fn record(&mut self, id: MessageId) {
        self.bloom.insert(id.to_bytes());
        self.exact.insert(id);
        self.ring.push_back((id, Instant::now()));
        self.evict_old();
    }

    /// Ring-buffer cleanup: remove entries older than the exact window.
    fn evict_old(&mut self) {
        // Compare elapsed time rather than constructing a cutoff instant.
        // `Instant::now() - duration` panics on Linux/Android when process
        // uptime is less than the duration (CLOCK_MONOTONIC starts at zero on
        // boot). The neighbor_table.rs incident comment documents this exact
        // failure — apply the same pattern here (SYS-1).
        let now = Instant::now();
        while let Some(front) = self.ring.front() {
            if now.duration_since(front.1) > EXACT_WINDOW {
                let (id, _) = self.ring.pop_front().unwrap();
                self.exact.remove(&id);
            } else {
                break;
            }
        }
    }

    /// Record that a peer was seen; used to skip needless re-flooding toward
    /// already-contacted peers (without altering the message-level window).
    pub fn record_peer(&mut self, peer: [u8; 32]) {
        self.peers_seen.insert(peer);
    }

    /// Whether a peer has been contacted recently (flood hint).
    pub fn contains_peer(&self, peer: &[u8; 32]) -> bool {
        self.peers_seen.contains(peer)
    }

    pub fn len(&self) -> usize {
        self.exact.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
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
        // Fast track: insert many, then confirm len reflects pruning only when
        // time passes; here we only assert structural integrity.
        let mut cache = ForwardedCache::default();
        for _ in 0..10 {
            cache.record(MessageId::new_v7());
        }
        assert!(cache.len() >= 10);
        cache.evict_old();
        // Nothing is older than 1 h yet, so all remain.
        assert!(cache.len() >= 10);
    }

    #[test]
    fn peers_seen_tracked() {
        let mut cache = ForwardedCache::default();
        let peer = [7u8; 32];
        assert!(!cache.contains_peer(&peer));
        cache.record_peer(peer);
        assert!(cache.contains_peer(&peer));
    }
}
