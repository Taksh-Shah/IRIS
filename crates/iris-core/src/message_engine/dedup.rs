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

/// Hard ceiling on probe count for a filter reconstructed from the wire.
///
/// `bloom_probes` never produces anything near this for realistic parameters;
/// the bound exists purely to stop a hostile `k` from turning `contains()` into
/// an unbounded loop.
pub const MAX_BLOOM_PROBES: usize = 64;

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

/// Split a 128-bit hash into two independent 64-bit seeds.
fn split_hash(h: [u8; 16]) -> (u64, u64) {
    let mut a = [0u8; 8];
    let mut b = [0u8; 8];
    a.copy_from_slice(&h[..8]);
    b.copy_from_slice(&h[8..]);
    (u64::from_le_bytes(a), u64::from_le_bytes(b))
}

/// Per-process key for the dedup hash.
///
/// The probe positions used to be derived from the raw `MessageId` bytes with
/// no hashing at all, despite the doc comment claiming blake3. `MessageId` is
/// 16 attacker-chosen wire bytes, so `h1` was literally `LE(bytes[0..8])` and
/// the `i = 0` probe landed on `h1 % m` — an attacker could place a bit at any
/// chosen position by choosing the id, and with ~k junk messages could set
/// every probe bit of a *target* id, permanently censoring it at that node.
///
/// Keying with a random per-process value means probe positions cannot be
/// computed off-box even though the hash itself is public.
fn dedup_hash_key() -> &'static [u8; 32] {
    use std::sync::OnceLock;
    static KEY: OnceLock<[u8; 32]> = OnceLock::new();
    KEY.get_or_init(|| {
        use rand::RngCore;
        let mut k = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut k);
        k
    })
}

/// Keyed 128-bit digest of a message id, used for Bloom probe derivation.
fn dedup_digest(id: MessageId) -> [u8; 16] {
    let full = blake3::keyed_hash(dedup_hash_key(), &id.to_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&full.as_bytes()[..16]);
    out
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
    ///
    /// Validates `m`, `bits.len()` **and** `k`. `k` arrives from a peer over the
    /// discovery Bloom exchange, and it was previously unchecked: a peer
    /// sending `{ m: 8, k: usize::MAX, bits: vec![0xFF] }` passed validation
    /// (`bits.len() == 8.div_ceil(8)`), and the next `contains()` iterated
    /// `0..usize::MAX`. `.all()` short-circuits only on a zero bit, and every
    /// bit was 1, so the call never returned — an unkillable hang in whichever
    /// task touched it, from three attacker-chosen fields.
    pub fn from_parts(m: usize, k: usize, bits: Vec<u8>) -> Option<Self> {
        // PM-5: cap m at the bits needed for BLOOM_CAPACITY elements. Without
        // this, a peer could send an arbitrarily large m, allocating up to
        // usize::MAX / 8 bytes. The BLOOM_CAPACITY-derived ceiling matches the
        // largest filter this crate generates itself.
        let max_m = bloom_bits(BLOOM_CAPACITY, BLOOM_FPR);
        if m == 0 || m > max_m || k == 0 || k > MAX_BLOOM_PROBES || bits.len() != m.div_ceil(8) {
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

    /// Elements inserted into this generation (drives rotation).
    pub fn inserted(&self) -> usize {
        self.inserted
    }

    #[cfg(test)]
    fn probe_positions_for_test(&self, h: [u8; 16]) -> impl Iterator<Item = usize> + '_ {
        self.probe_positions(h)
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
///
/// The Bloom tier is **generational**: two filters, `current` and `previous`.
/// Once `current` reaches its design capacity it becomes `previous` and a fresh
/// filter takes over, so the bit-set fraction — and therefore the false-positive
/// rate — is bounded no matter how long the node runs.
///
/// The single fill-only filter this replaces was never cleared or rotated. Past
/// ~1 M insertions its bitmap was ~99.9% set and the false-positive rate
/// approached 1.0, at which point the node rejected essentially *all* new
/// traffic as "duplicate" — including P0 SOS — and never recovered without a
/// restart. That happened in normal long-lived operation, not just under
/// attack; an attacker merely accelerated it.
#[derive(Debug)]
pub struct DedupEngine {
    current: BloomFilter,
    previous: BloomFilter,
    lru: LruCache,
    capacity: usize,
    fpr: f64,
    rotations: u64,
}

impl Default for DedupEngine {
    fn default() -> Self {
        Self::new(BLOOM_CAPACITY, BLOOM_FPR, LRU_CAPACITY)
    }
}

impl DedupEngine {
    pub fn new(bloom_capacity: usize, fpr: f64, lru_capacity: usize) -> Self {
        let capacity = bloom_capacity.max(1);
        let fpr = fpr.max(1e-6);
        DedupEngine {
            current: BloomFilter::new(capacity, fpr),
            previous: BloomFilter::new(capacity, fpr),
            lru: LruCache::new(lru_capacity),
            capacity,
            fpr,
            rotations: 0,
        }
    }

    /// Record a message id and report whether it was a duplicate.
    pub fn seen(&mut self, id: MessageId) -> bool {
        let digest = dedup_digest(id);
        let dup = self.lru.contains(&id)
            || self.current.contains(digest)
            || self.previous.contains(digest);

        self.lru.check_and_insert(id);

        if self.current.inserted() >= self.capacity {
            // Retire the older generation rather than letting either saturate.
            self.previous =
                std::mem::replace(&mut self.current, BloomFilter::new(self.capacity, self.fpr));
            self.rotations += 1;
        }
        self.current.insert(digest);
        dup
    }

    /// Total Bloom footprint across both generations.
    pub fn bloom_size_bytes(&self) -> usize {
        self.current.size_bytes() + self.previous.size_bytes()
    }

    /// Number of generation rotations so far (observability).
    pub fn rotations(&self) -> u64 {
        self.rotations
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
        assert!(
            b.size_bytes() >= 179_000 && b.size_bytes() <= 181_000,
            "got {}",
            b.size_bytes()
        );
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

    /// Deterministic distinct ids for saturation testing.
    fn id_n(n: u64) -> MessageId {
        let mut b = [0u8; 16];
        b[..8].copy_from_slice(&n.to_le_bytes());
        MessageId::from_bytes(b)
    }

    #[test]
    fn bloom_rotation_bounds_the_false_positive_rate() {
        // Regression: the single fill-only Bloom saturated and never recovered.
        // Past its design capacity the false-positive rate climbed toward 1.0
        // and the node dropped essentially all new traffic as "duplicate",
        // permanently. Drive well past capacity and confirm fresh ids are still
        // recognised as new.
        let capacity = 512;
        let mut d = DedupEngine::new(capacity, 0.001, 64);

        for n in 0..(capacity as u64 * 20) {
            d.seen(id_n(n));
        }
        assert!(d.rotations() > 0, "the filter must rotate generations");

        // A block of ids never inserted must overwhelmingly read as new.
        let probe_base = 10_000_000u64;
        let mut false_positives = 0;
        let probes = 2_000u64;
        for n in 0..probes {
            let mut fresh = DedupEngine::new(capacity, 0.001, 64);
            std::mem::swap(&mut fresh, &mut d);
            let dup = fresh.seen(id_n(probe_base + n));
            std::mem::swap(&mut fresh, &mut d);
            if dup {
                false_positives += 1;
            }
        }
        // Generous bound: the point is that it is nowhere near saturation,
        // where this would have been ~100%.
        assert!(
            false_positives * 20 < probes,
            "false-positive rate too high after {} inserts: {}/{}",
            capacity * 20,
            false_positives,
            probes
        );
    }

    #[test]
    fn recent_ids_survive_a_rotation() {
        // Rotation must not amnesia the immediately-preceding generation, or a
        // message could legitimately loop back and be re-accepted.
        let capacity = 128;
        let mut d = DedupEngine::new(capacity, 0.001, 8);
        assert!(!d.seen(id_n(1)), "first sighting");

        // Fill just past one generation so a rotation happens.
        for n in 100..(100 + capacity as u64 + 5) {
            d.seen(id_n(n));
        }
        assert!(
            d.seen(id_n(1)),
            "an id from the previous generation must still read as duplicate"
        );
    }

    #[test]
    fn probe_positions_are_not_derived_from_raw_id_bytes() {
        // Regression: probes were an affine function of the raw MessageId, so
        // an attacker could set a chosen bit by choosing the id and censor a
        // target id with ~k crafted messages. Ids differing only in the low 8
        // bytes must not map to related probe positions.
        let bloom = BloomFilter::new(4096, 0.001);
        let a = dedup_digest(id_n(0));
        let b = dedup_digest(id_n(1));
        assert_ne!(a, b);

        let pa: Vec<usize> = bloom.probe_positions_for_test(a).collect();
        let pb: Vec<usize> = bloom.probe_positions_for_test(b).collect();
        assert_ne!(pa, pb, "digests must not collide");

        // The old scheme made probe[0] == LE(id[0..8]) % m, so id 0 landed on
        // position 0. A keyed digest makes that vanishingly unlikely.
        assert!(
            pa[0] != 0 || pb[0] != 1,
            "probe positions still track the raw id"
        );
    }

    #[test]
    fn from_parts_rejects_a_hostile_probe_count() {
        // Regression: `k` arrives from a peer over the discovery Bloom
        // exchange and was unvalidated. `{ m: 8, k: usize::MAX, bits: [0xFF] }`
        // passed (bits.len() == 8.div_ceil(8)), and the next `contains()`
        // iterated 0..usize::MAX — `.all()` short-circuits only on a ZERO bit,
        // and every bit was 1, so the call never returned. An unkillable hang
        // in whichever task touched it, from three attacker-chosen fields.
        assert!(BloomFilter::from_parts(8, usize::MAX, vec![0xFF]).is_none());
        assert!(BloomFilter::from_parts(8, MAX_BLOOM_PROBES + 1, vec![0xFF]).is_none());
        assert!(BloomFilter::from_parts(8, 0, vec![0xFF]).is_none());

        // A filter with realistic parameters still round-trips.
        let ok = BloomFilter::from_parts(8, 4, vec![0xFF]).expect("sane parameters accepted");
        assert_eq!(ok.k(), 4);
        // And terminates.
        assert!(ok.contains([0u8; 16]));

        // PM-5: m must not exceed the BLOOM_CAPACITY-derived ceiling.
        let max_m = bloom_bits(BLOOM_CAPACITY, BLOOM_FPR);
        let oversized_bits = vec![0u8; (max_m + 8).div_ceil(8)];
        assert!(
            BloomFilter::from_parts(max_m + 8, 4, oversized_bits).is_none(),
            "m above BLOOM_CAPACITY ceiling must be rejected"
        );
        // Exactly at the ceiling is allowed.
        let ok_bits = vec![0u8; max_m.div_ceil(8)];
        assert!(
            BloomFilter::from_parts(max_m, 4, ok_bits).is_some(),
            "m exactly at ceiling must be accepted"
        );
    }

    #[test]
    fn dedup_reports_first_win() {
        let mut d = DedupEngine::default();
        assert!(!d.seen(mid(7)), "first arrival is new");
        assert!(d.seen(mid(7)), "second arrival is duplicate");
        assert!(!d.seen(mid(8)), "different id is new");
    }
}
