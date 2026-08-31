//! PRY-13: sharded per-sender state so the security engines do not serialise
//! every inbound message on one process-wide lock.
//!
//! Each of the four engines (`rate_limiter`, `quota`, `replay`, `spam`) crosses
//! the receive path for **every** inbound message and took a single
//! `tokio::sync::RwLock` in **write** mode to do it — one slow task (an
//! allocator stall during a `HashMap` grow, a preempted task, contention under
//! a flood) stalled *all* inbound processing, and flooding a node is precisely
//! how an attacker creates that contention.
//!
//! [`ShardedLocks`] splits the per-sender map into [`SHARD_COUNT`] independent
//! shards, each behind its own lock, routed by a hash of the sender key. Two
//! messages from different senders now contend only if they hash to the same
//! shard. Standard `RwLock` per shard — no custom lock-free atomics, so no
//! `loom` model is required (a concurrency stress bench lives in
//! `benches/security_contention.rs`).
//!
//! Per-engine **count caps** (`max_sender_buckets`, `max_sender_highwater`,
//! `max_sender_stats`) become per-shard: the configured total is divided across
//! shards (rounding up). A sender population spread across shards still hits the
//! same aggregate bound; a flood ground onto a single shard hits its slice
//! sooner — stronger isolation, not weaker.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use tokio::sync::RwLock;

/// Number of independent lock shards per engine. A power of two well above the
/// worker-thread count so cross-shard collisions are rare, small enough that
/// whole-map sweeps (snapshots, `reset`) stay cheap.
pub(crate) const SHARD_COUNT: usize = 64;

/// Divide a configured total capacity across the shards, rounding up so the
/// aggregate is never materially below the configured value.
pub(crate) fn per_shard_cap(total: usize) -> usize {
    total.div_ceil(SHARD_COUNT).max(1)
}

/// The shard index a key routes to (`0..SHARD_COUNT`). Used by sync
/// constructors that partition data before any lock exists.
pub(crate) fn shard_index_for<K: Hash + ?Sized>(key: &K) -> usize {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    (h.finish() as usize) % SHARD_COUNT
}

/// `SHARD_COUNT` independent `RwLock<S>` shards, addressed by a hash of a key.
pub(crate) struct ShardedLocks<S> {
    shards: Box<[RwLock<S>]>,
}

impl<S: Default> Default for ShardedLocks<S> {
    fn default() -> Self {
        Self::with(S::default)
    }
}

impl<S> ShardedLocks<S> {
    /// Build the shards, each initialised by `init()`.
    pub(crate) fn with(init: impl Fn() -> S) -> Self {
        let shards = (0..SHARD_COUNT)
            .map(|_| RwLock::new(init()))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        ShardedLocks { shards }
    }

    /// Build from exactly `SHARD_COUNT` pre-partitioned shard states (sync
    /// constructors — no lock is taken).
    pub(crate) fn from_shards(states: Vec<S>) -> Self {
        assert_eq!(states.len(), SHARD_COUNT, "wrong shard count");
        ShardedLocks {
            shards: states
                .into_iter()
                .map(RwLock::new)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        }
    }

    /// The shard that owns `key`.
    pub(crate) fn shard_for<K: Hash + ?Sized>(&self, key: &K) -> &RwLock<S> {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        &self.shards[(h.finish() as usize) % SHARD_COUNT]
    }

    /// All shards, for whole-map operations (snapshot / reset / count). Callers
    /// lock one shard at a time — never hold two shard guards at once, to keep
    /// the lock order trivially deadlock-free.
    pub(crate) fn shards(&self) -> &[RwLock<S>] {
        &self.shards
    }
}

/// The shard index a key routes to. Test-only — engines never need it.
#[cfg(test)]
pub(crate) fn shard_index<K: Hash + ?Sized>(key: &K) -> usize {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    (h.finish() as usize) % SHARD_COUNT
}

/// Test helper: `count` distinct 16-byte sender shorts that all route to the
/// same shard, so a small per-shard cap can be exercised deterministically.
#[cfg(test)]
pub(crate) fn colliding_senders(count: usize) -> Vec<[u8; 16]> {
    let target = shard_index::<[u8; 16]>(&[0u8; 16]);
    let mut out = Vec::new();
    let mut i = 0u64;
    while out.len() < count {
        let mut s = [0u8; 16];
        s[..8].copy_from_slice(&i.to_le_bytes());
        if shard_index::<[u8; 16]>(&s) == target {
            out.push(s);
        }
        i = i.checked_add(1).expect("found enough colliding senders");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    #[tokio::test]
    async fn distinct_senders_spread_across_shards() {
        let m: ShardedLocks<HashMap<[u8; 16], u64>> = ShardedLocks::default();
        let mut hit = std::collections::HashSet::new();
        for i in 0u32..4096 {
            let mut k = [0u8; 16];
            k[..4].copy_from_slice(&i.to_le_bytes());
            let ptr = m.shard_for(&k) as *const _ as usize;
            hit.insert(ptr);
        }
        // 4096 keys should touch essentially every shard.
        assert!(hit.len() >= SHARD_COUNT - 2, "poor shard spread: {}", hit.len());
    }

    #[tokio::test]
    async fn same_key_always_same_shard() {
        let m: ShardedLocks<HashMap<[u8; 16], u64>> = ShardedLocks::default();
        let k = [7u8; 16];
        let a = m.shard_for(&k) as *const _ as usize;
        let b = m.shard_for(&k) as *const _ as usize;
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn concurrent_writers_do_not_lose_updates() {
        let m: Arc<ShardedLocks<HashMap<u32, u64>>> = Arc::new(ShardedLocks::default());
        let mut tasks = Vec::new();
        for t in 0u32..32 {
            let m = m.clone();
            tasks.push(tokio::spawn(async move {
                for i in 0u32..1000 {
                    let key = (t << 16) | i;
                    let mut g = m.shard_for(&key).write().await;
                    *g.entry(key).or_insert(0) += 1;
                }
            }));
        }
        for t in tasks {
            t.await.unwrap();
        }
        let mut total = 0u64;
        for s in m.shards() {
            total += s.read().await.values().sum::<u64>();
        }
        assert_eq!(total, 32 * 1000);
    }
}
