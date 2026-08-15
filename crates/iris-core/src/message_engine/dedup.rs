//! Message deduplication — MSG-001 (R5: double-hash Bloom + exact LRU).
//!
//! Two tiers (DEDUPLICATION.md §Dedup Strategy, refined by RES-0010 R5):
//! - **Bloom filter** — 100 K element capacity at 0.1% FPR, ~180 KB bitmap,
//!   double hashing per Kirsch–Mitzenmacher 2008 (two 64-bit hashes combine to
//!   k independent probes: `h_i = h1 + i·h2 (mod m)`). No false negatives.
//! - **Exact LRU** — 10 K most-recent `MessageId`s for exact duplicate
//!   confirmation and instant eviction ordering.
//!
//! A message is a duplicate iff the exact LRU contains it OR the Bloom filter
//! reports it **and** the LRU would have been expected to still hold it — in
//! practice we treat Bloom-hit as duplicate (0.1% FPR acceptable per design,
//! matches the accepted false-positive trade-off). Bloom persistence to storage
//! (R5) is a follow-up in WP-2.

use crate::protocol::MessageId;

/// Number of distinct elements the Bloom filter is sized for.
pub const BLOOM_CAPACITY: usize = 100_000;
/// Target false-positive rate.
pub const BLOOM_FPR: f64 = 0.001;
/// Exact-set LRU capacity.
pub const LRU_CAPACITY: usize = 10_000;

/// Compute the Bloom bitmap size (bits) for capacity `n` at FPR `p`:
/// `m = -n·ln(p) / (ln 2)²`, rounded up to the byte.
pub fn bloom_bits(n: usize, p: f64) -> usize {
    let ln2 = std::f64::consts::LN_2;
    let bits = -(n as f64) * p.ln() / (ln2 * ln2);
    ((bits / 8.0).ceil() as usize) * 8
}

/// Optimal probe count: `k = (m/n)·ln 2`.
pub fn bloom_probes(n: usize, m: usize) -> usize {
    let ln2 = std::f64::consts::LN_2;
    (((m as f64) / (n as f64).max(1.0)) * ln2).round().max(1.0) as usize
}

/// Split a 128-bit hash into two independent 64-bit seeds (blake3 16 bytes).
fn split_hash(h: [u8; 16]) -> (u64, u64) {
    let mut a = [0u8; 8];
    let mut b = [0u8; 8];
    a.copy_from_slice(&h[..8]);
    b.copy_from_slice(&h[8..]);
    (u64::from_le_bytes(a), u64::from_le_bytes(b))
}

/// Fixed-size Bloom filter with double hashing (Kirsch–Mitzenmacher).
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<u8>,
    m: usize,
    k: usize,
    /// Number of inserted elements (for observability only).
    inserted: usize,
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::new(BLOOM_CAPACITY, BLOOM_FPR)
    }
}

impl BloomFilter {
    pub fn new(capacity: usize, fpr: f64) -> Self {
        let m = bloom_bits(capacity.max(1), fpr.max(1e-6));
        let k = bloom_probes(capacity.max(1), m);
        BloomFilter {
            bits: vec![0u8; m / 8],
            m,
            k,
            inserted: 0,
        }
    }

    /// Current bitmap size in bytes.
    pub fn size_bytes(&self) -> usize {
        self.bits.len()
    }

    /// Bit count `m` (reconstruction parameter for the Bloom exchange).
    pub fn m(&self) -> usize {
        self.m
    }

    /// Probe count `k` (reconstruction parameter for the Bloom exchange).
    pub fn k(&self) -> usize {
        self.k
    }

    /// Raw bitmap bytes (for the wire Bloom exchange; DISCO-001 handshake).
    pub fn bits(&self) -> &[u8] {
        &self.bits
    }

    /// Reconstruct a filter from `(m, k, bits)` — inverse of [`BloomFilter::bits`].
    /// Validates that `bits.len() * 8 == m` and `k >= 1`.
    pub fn from_parts(m: usize, k: usize, bits: Vec<u8>) -> Option<Self> {
        if m == 0 || k == 0 || bits.len() != m.div_ceil(8) {
            return None;
        }
        Some(BloomFilter {
            bits,
            m,
            k,
            inserted: 0,
        })
    }

    pub fn probes(&self) -> usize {
        self.k
    }

    fn probe_positions(&self, h: [u8; 16]) -> impl Iterator<Item = usize> + '_ {
        let (h1, h2) = split_hash(h);
        let h2 = h2 | 1; // ensure odd stride → guaranteed full period mod m
        (0..self.k).map(move |i| {
            let step = (i as u64).wrapping_mul(h2);
            h1.wrapping_add(step) as usize % self.m
        })
    }

    pub fn insert(&mut self, h: [u8; 16]) {
        let positions: Vec<usize> = self.probe_positions(h).collect();
        for pos in positions {
            self.bits[pos / 8] |= 1 << (pos % 8);
        }
        self.inserted += 1;
    }

    /// `false` guarantees the element was never inserted; `true` may be a
    /// false positive (FPR ~0.1%).
    pub fn contains(&self, h: [u8; 16]) -> bool {
        self.probe_positions(h)
            .all(|pos| self.bits[pos / 8] & (1 << (pos % 8)) != 0)
    }
}

/// Minimal exact LRU over `MessageId`s using lazy eviction (a `HashMap` +
/// `VecDeque`; stale front entries are pruned on insert).
#[derive(Debug, Default)]
pub struct LruCache {
    set: std::collections::HashSet<MessageId>,
    order: std::collections::VecDeque<MessageId>,
    capacity: usize,
}

impl LruCache {
    pub fn new(capacity: usize) -> Self {
        LruCache {
            set: std::collections::HashSet::with_capacity(capacity),
            order: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Returns `true` if `id` was already present (duplicate).
    pub fn check_and_insert(&mut self, id: MessageId) -> bool {
        if self.set.contains(&id) {
            // Re-arm recency by moving to the back.
            if let Some(pos) = self.order.iter().position(|x| *x == id) {
                self.order.remove(pos);
                self.order.push_back(id);
            }
            return true;
        }
        self.set.insert(id);
        self.order.push_back(id);
        // Lazy eviction: prune stale front entries, then evict the oldest
        // distinct entry while over capacity.
        while self.set.len() > self.capacity {
            if let Some(front) = self.order.pop_front() {
                self.set.remove(&front);
            } else {
                break;
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty()
    }

    pub fn contains(&self, id: &MessageId) -> bool {
        self.set.contains(id)
    }
}

/// Combined dedup engine.
#[derive(Debug)]
pub struct DedupEngine {
    bloom: BloomFilter,
    lru: LruCache,
}

impl Default for DedupEngine {
    fn default() -> Self {
        Self::new(BLOOM_CAPACITY, BLOOM_FPR, LRU_CAPACITY)
    }
}

impl DedupEngine {
    pub fn new(bloom_capacity: usize, fpr: f64, lru_capacity: usize) -> Self {
        DedupEngine {
            bloom: BloomFilter::new(bloom_capacity, fpr),
            lru: LruCache::new(lru_capacity),
        }
    }

    /// Record a message id and report whether it was a duplicate.
    pub fn seen(&mut self, id: MessageId) -> bool {
        let dup = self.bloom.contains(id.to_bytes()) || self.lru.contains(&id);
        self.lru.check_and_insert(id);
        self.bloom.insert(id.to_bytes());
        dup
    }

    pub fn bloom_size_bytes(&self) -> usize {
        self.bloom.size_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mid(seq: u16) -> MessageId {
        let mut b = [0u8; 16];
        b[..2].copy_from_slice(&seq.to_le_bytes());
        MessageId::from_bytes(b)
    }

    #[test]
    fn bloom_sizing_matches_design() {
        // 100 K @ 0.1% ≈ 1.44 M bits ≈ 180 KB, ~10 probes.
        let b = BloomFilter::new(BLOOM_CAPACITY, BLOOM_FPR);
        assert!(b.size_bytes() >= 179_000 && b.size_bytes() <= 181_000, "got {}", b.size_bytes());
        assert!(b.probes() >= 9 && b.probes() <= 11, "got {}", b.probes());
    }

    #[test]
    fn bloom_has_no_false_negatives() {
        let mut b = BloomFilter::new(10_000, 0.001);
        let mut hashes = Vec::new();
        for i in 0..1000u16 {
            let h = mid(i).to_bytes();
            b.insert(h);
            hashes.push(h);
        }
        for h in hashes {
            assert!(b.contains(h), "inserted element must be reported present");
        }
    }

    #[test]
    fn lru_evicts_oldest() {
        let mut c = LruCache::new(3);
        assert!(!c.check_and_insert(mid(1)));
        assert!(!c.check_and_insert(mid(2)));
        assert!(!c.check_and_insert(mid(3)));
        // 4th insert evicts 1
        assert!(!c.check_and_insert(mid(4)));
        assert!(!c.contains(&mid(1)), "oldest evicted");
        assert!(c.contains(&mid(2)));
        assert!(c.len() == 3);
    }

    #[test]
    fn lru_rearms_recency() {
        let mut c = LruCache::new(2);
        c.check_and_insert(mid(1));
        c.check_and_insert(mid(2));
        // Touch 1 → 2 becomes oldest
        assert!(c.check_and_insert(mid(1)), "repeat is duplicate");
        c.check_and_insert(mid(3));
        assert!(c.contains(&mid(1)), "recently touched survives");
        assert!(!c.contains(&mid(2)), "untouched oldest evicted");
    }

    #[test]
    fn dedup_reports_first_win() {
        let mut d = DedupEngine::default();
        assert!(!d.seen(mid(7)), "first arrival is new");
        assert!(d.seen(mid(7)), "second arrival is duplicate");
        assert!(!d.seen(mid(8)), "different id is new");
    }
}