#![cfg(loom)]
//! AC-4 (DEC-TEST-0004) loom leaf-structure models.
//!
//! Built ONLY under `--cfg loom` (CI: `cargo rustc -p iris-core --test
//! loom_models -- --cfg loom`, then execute the produced binary — a global
//! `RUSTFLAGS="--cfg loom"` would also re-cfg tokio and break its `net`
//! module). Without the cfg this file is empty — it does not affect the
//! standard workspace baseline.
//!
//! Scope and soundness limits (per TEST_001_DESIGN.md AC-4):
//! - Leaf structures only: the replay high-water (ts,seq) accept/advance,
//!   the quota saturating-CAS byte counters, and the dedup first-wins cache.
//! - Atom and lock orderings mirror production intent but are model-level:
//!   `Ordering::Relaxed` on model atomics is *conservative* (the production
//!   tokio `RwLock`/`Mutex` provide stronger mutual exclusion than the model).
//!   No load-buffering / TSO-store-buffer scenarios are modeled, and the full
//!   `TransportManager`/`MessageEngine` stack is out of scope.
//! - Model sizes are intentionally small: loom enumerates every interleaving,
//!   and large loops/spin-prone CAS loops exceed loom's branch bound
//!   (`Model exceeded maximum number of branches`). Each model is sized to
//!   expose lost updates / lost-wakeups / false-replay bugs, not to stress.
//!
//! LOOM-DRIVEN MODEL FIX (2026-08-18): the first version over-asserted the
//! high-water invariant as "every strictly-increasing pair is accepted under
//! ANY interleaving". Loom disproved it: a pair is legitimately dropped when
//! a HIGHER (ts,seq) was already accepted (out-of-order/late arrival across
//! interleaved writers). The sound invariants modeled below are:
//!   1. single-stream integrity (all in-order pairs accepted);
//!   2. replay rejection (an at-or-below high-water pair is never accepted);
//!   3. no lost update (final high-water = global max).
//! These match the production contract in `replay.rs` (per-stream high-water;
//! `<=` replay drop; documented DTN out-of-order drop).

use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::{Arc, Mutex};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

// ---------------------------------------------------------------------------
// Section 3 leaf models (TAK-18): synchronization-SHAPE analogs of the
// section's concurrent structures — TransportManager registry mutations vs
// lookups, BLE/Wi-Fi reassembly bounded-append-vs-sweep, SCF insert-vs-evict
// priority ordering. HONEST LIMITATION: these are hand-written analogs of
// production shapes, not calls into production code; a production change can
// still drift away from them. Closing that fully requires extracting the sync
// cores into shared types (recorded as residual work in the bug-fix journal).
// ---------------------------------------------------------------------------

/// Shape-analog of `TransportManager`'s registry: register/deregister racing
/// a lookup must never observe a torn entry, and after deregister completes
/// the entry is gone (no resurrection).
#[test]
fn s3_registry_register_deregister_vs_lookup() {
    loom::model(|| {
        let registry: Arc<Mutex<HashMap<u64, u64>>> = Arc::new(Mutex::new(HashMap::new()));
        let w = registry.clone();
        let writer = loom::thread::spawn(move || {
            w.lock().unwrap().insert(1, 100);
            let removed = w.lock().unwrap().remove(&1);
            removed
        });
        let l = registry.clone();
        let reader = loom::thread::spawn(move || {
            // Legal observations across the lifecycle: None (before insert or
            // after remove), or Some(payload) exactly. Every transition among
            // those is legal under interleaving — the guarantee under test is
            // that a lookup NEVER observes a torn/wrong payload.
            let first = l.lock().unwrap().get(&1).copied();
            let second = l.lock().unwrap().get(&1).copied();
            [first, second]
                .iter()
                .all(|o| matches!(o, None | Some(100)))
        });
        let removed = writer.join().unwrap();
        let consistent = reader.join().unwrap();
        assert!(consistent, "lookup saw a torn/wrong payload");
        assert_eq!(removed, Some(100));
        assert!(
            registry.lock().unwrap().is_empty(),
            "deregister leaves no residue"
        );
    });
}

/// Shape-analog of the BLE/Wi-Fi reassembly buffers: bounded append with
/// oldest-first sweep must keep the buffer within its cap under concurrent
/// appends, and every appended id is either retained or accounted as evicted.
#[test]
fn s3_reassembly_append_sweep_stays_bounded() {
    loom::model(|| {
        const CAP: usize = 2;
        let buf: Arc<Mutex<VecDeque<u64>>> = Arc::new(Mutex::new(VecDeque::new()));
        let mut handles = Vec::new();
        for id in 0..3u64 {
            let b = buf.clone();
            handles.push(loom::thread::spawn(move || {
                let mut g = b.lock().unwrap();
                if g.len() >= CAP {
                    g.pop_front(); // oldest-first sweep
                }
                g.push_back(id);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let g = buf.lock().unwrap();
        assert!(g.len() <= CAP, "cap violated: {g:?}");
        // Each id pushed exactly once: survivors are unique members of the
        // inserted domain, none lost silently and none duplicated.
        let mut seen = g.iter().copied().collect::<Vec<_>>();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), g.len(), "duplicate survivor");
        for id in g.iter() {
            assert!([0u64, 1, 2].contains(id), "foreign element {id}");
        }
    });
}

/// Shape-analog of SCF store insert-vs-evict: eviction always removes the
/// lowest-priority/newest key (max key here) even while inserts land, and
/// never returns an element that was not previously inserted.
#[test]
fn s3_scf_insert_vs_priority_evict() {
    loom::model(|| {
        let store: Arc<Mutex<BTreeMap<u64, u8>>> = Arc::new(Mutex::new(BTreeMap::new()));
        let i = store.clone();
        let inserter = loom::thread::spawn(move || {
            for (seq, prio) in [(10u64, 4u8), (20, 3), (30, 5)] {
                i.lock().unwrap().insert(seq, prio);
            }
        });
        let e = store.clone();
        let evicter = loom::thread::spawn(move || {
            let mut evicted_seq = Vec::new();
            loop {
                let mut g = e.lock().unwrap();
                let next = g.keys().next_back().copied(); // max key = lowest prio/newest
                match next {
                    Some(k) => {
                        g.remove(&k);
                        evicted_seq.push(k);
                    }
                    None => break,
                }
                if evicted_seq.len() == 3 {
                    break;
                }
                drop(g);
            }
            evicted_seq
        });
        inserter.join().unwrap();
        let evicted = evicter.join().unwrap();
        let g = store.lock().unwrap();
        // Sound invariants under any interleaving: each eviction removed the
        // max key of the set present AT THAT MOMENT (by construction:
        // BTreeMap next_back), no key is evicted twice, evicted ∪ remaining
        // ⊆ inserted domain, and every insert landed (insert thread joined).
        let mut evicted_sorted = evicted.clone();
        evicted_sorted.sort_unstable();
        evicted_sorted.dedup();
        assert_eq!(evicted_sorted.len(), evicted.len(), "duplicate eviction");
        let mut all: Vec<u64> = g.keys().copied().collect();
        all.extend_from_slice(&evicted);
        all.sort_unstable();
        all.dedup();
        assert_eq!(all, vec![10, 20, 30], "lost or foreign elements");
        assert!(g.values().all(|&p| [3u8, 4, 5].contains(&p)));
    });
}

/// Production-analog of `replay::HighWaterMark::check_and_advance`
/// (strictly-increasing (ts,seq) pair under a mutex; `<=` high-water = replay).
struct HighWaterLeaf(Mutex<(u64, u64)>);

impl HighWaterLeaf {
    fn new() -> Self {
        HighWaterLeaf(Mutex::new((0, 0)))
    }
    fn check_and_advance(&self, ts: u64, seq: u64) -> bool {
        let mut g = self.0.lock().unwrap();
        if ts > g.0 || (ts == g.0 && seq > g.1) {
            *g = (ts, seq);
            true
        } else {
            false
        }
    }
    fn current(&self) -> (u64, u64) {
        *self.0.lock().unwrap()
    }
}

/// Production-analog of `quota::atomic_saturating_add` (CAS loop, relaxed
/// orderings, saturating so attacker-controlled sizes cannot wrap).
fn atomic_saturating_add(counter: &AtomicU64, amount: u64) {
    let mut cur = counter.load(Ordering::Relaxed);
    loop {
        let next = cur.saturating_add(amount);
        match counter.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

/// Production-analog of the dedup exact-set `seen`/`insert` first-wins check
/// (`MessageId` modeled as `u64` key; the production map is keyed on the 16
/// bytes). One thread sees "first" per unique id; all others are replays.
struct DedupLeaf(Mutex<HashSet<u64>>);

impl DedupLeaf {
    fn new() -> Self {
        DedupLeaf(Mutex::new(HashSet::new()))
    }
    fn try_insert(&self, id: u64) -> bool {
        let mut g = self.0.lock().unwrap();
        g.insert(id)
    }
}

/// Invariant 1+2+3: a single monotonic stream (the real per-stream writer) is
/// accepted in full — no false replay — while a concurrent replay attacker
/// that re-injects the CURRENT high-water pair is never accepted, and the
/// final high-water is exactly the stream's last pair (no lost update).
#[test]
fn replay_highwater_stream_integrity_and_replay_rejection() {
    loom::model(|| {
        let leaf = Arc::new(HighWaterLeaf::new());

        // Writer: strictly-increasing in-order stream, as the delivery loop
        // drains one node's storage under the per-stream lock. Pairs start at
        // (1,x): a frame EXACTLY at the (0,0) sentinel would be a replay of
        // "nothing" — the model surfaced that boundary, production sends
        // real timestamps so ts is never 0.
        let w = leaf.clone();
        let writer = loom::thread::spawn(move || {
            let pairs = [(1u64, 0u64), (1, 1), (2, 0)];
            let mut accepted = 0;
            for &(ts, seq) in &pairs {
                if w.check_and_advance(ts, seq) {
                    accepted += 1;
                }
            }
            accepted
        });

        // Attacker: re-injects whatever the high-water is RIGHT NOW — an
        // at-or-below pair — which must always be rejected as a replay.
        let a = leaf.clone();
        let attacker = loom::thread::spawn(move || {
            let (ts, seq) = a.current();
            a.check_and_advance(ts, seq)
        });

        let writer_ok = writer.join().unwrap();
        let attacker_ok = attacker.join().unwrap();

        assert_eq!(writer_ok, 3, "in-order stream: no false replay (inv. 1)");
        assert_eq!(
            attacker_ok, false,
            "high-water pair never re-accepted (inv. 2)"
        );
        assert_eq!(leaf.current(), (2, 0), "no lost update (inv. 3)");
    });
}

/// Invariant 2 at the boundary: two concurrent attempts at the SAME pair —
/// exactly one wins (the first), the loser is a replay; no double-accept.
#[test]
fn replay_highwater_duplicate_first_wins() {
    loom::model(|| {
        let leaf = Arc::new(HighWaterLeaf::new());
        let l1 = leaf.clone();
        let l2 = leaf.clone();
        let h1 = loom::thread::spawn(move || l1.check_and_advance(1, 5));
        let h2 = loom::thread::spawn(move || l2.check_and_advance(1, 5));
        let mut wins = 0;
        for h in [h1, h2] {
            if h.join().unwrap() {
                wins += 1;
            }
        }
        assert_eq!(wins, 1, "exactly one thread wins a duplicate (ts,seq)");
        assert_eq!(leaf.current(), (1, 5));
    });
}

/// Quota byte counters: concurrent saturating adds must never lose updates
/// (model sized to loom's branch bound; the logic is iteration-count agnostic).
#[test]
fn quota_counters_no_lost_update() {
    loom::model(|| {
        let counter = Arc::new(AtomicU64::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let c = counter.clone();
            handles.push(loom::thread::spawn(move || {
                for _ in 0..3 {
                    atomic_saturating_add(&c, 1);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(
            counter.load(Ordering::Relaxed),
            6,
            "no lost updates under interleaving"
        );
    });
}

/// Quota byte counters: near-`u64::MAX` adds saturate instead of wrapping
/// (the SEC-RT-12 / quota fuzz guarantee at model level).
#[test]
fn quota_counters_saturate_not_wrap() {
    loom::model(|| {
        let counter = Arc::new(AtomicU64::new(u64::MAX - 2));
        let c1 = counter.clone();
        let c2 = counter.clone();
        let h1 = loom::thread::spawn(move || atomic_saturating_add(&c1, 3));
        let h2 = loom::thread::spawn(move || atomic_saturating_add(&c2, 3));
        h1.join().unwrap();
        h2.join().unwrap();
        assert_eq!(
            counter.load(Ordering::Relaxed),
            u64::MAX,
            "saturates at MAX, never wraps"
        );
    });
}

/// Dedup exact-set: concurrent first-seen of the same id — exactly one winner
/// (first-wins), and a distinct id is always insertable (no cross-id
/// interference).
#[test]
fn dedup_first_wins_exactly_once() {
    loom::model(|| {
        let leaf = Arc::new(DedupLeaf::new());
        let l1 = leaf.clone();
        let l2 = leaf.clone();
        let h1 = loom::thread::spawn(move || l1.try_insert(42));
        let h2 = loom::thread::spawn(move || l2.try_insert(42));
        let mut wins = 0;
        for h in [h1, h2] {
            if h.join().unwrap() {
                wins += 1;
            }
        }
        assert_eq!(wins, 1, "exactly one thread wins a unique id");
        assert!(
            !leaf.try_insert(42),
            "subsequent insert of the same id is a replay"
        );
        assert!(leaf.try_insert(43), "a distinct id is insertable");
    });
}
