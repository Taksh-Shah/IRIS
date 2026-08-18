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
use std::collections::HashSet;

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
