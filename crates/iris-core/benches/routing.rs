//! AC-10 (DEC-TEST-0011) routing benchmarks.
//!
//! Two groups:
//! - PRoPHET (ROUTE-002): the RFC 6693 `meet` (encounter) update and `age`
//!   (aging, decay path exercised via a virtual clock) against a 1,000-node
//!   delivery-predictability table, plus a GTMX+ `decide` and a binary-spray
//!   handoff.
//! - L0 hot path (ROUTE-001, BASELINE_ROUTING.md §Resource Budget): Direct,
//!   KnownPath, Flood recipient selection (10 neighbors), the dedup cache,
//!   and the full `RoutingEngine::decide` chain — the budgeted per-message
//!   cost that ROUT-20 (a 175 KB clone per flood decision) shows was never
//!   actually measured.
//!
//! ROUT-32/ROUT-33: every bench below either resets to a fixed baseline each
//! iteration (`iter_batched`) or varies its message id per iteration, so the
//! reported numbers are the steady-state cost of the operation named, not an
//! artifact of state a prior iteration left behind.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use iris_core::discovery::neighbor_table::NeighborTable;
use iris_core::routing::opportunistic::OpportunisticRouter;
use iris_core::routing::prophet::{DeliveryPredictability, ProphetConfig};
use iris_core::routing::{
    recipients_for_flood, try_direct, try_known_path, ForwardedCache, RoutingEngine, RoutingTable,
    SprayBudget,
};
use iris_core::{LinkQuality, MessageId, MessagePriority, PeerId, PeerInfo, TransportId};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::runtime::Runtime;

fn pid(id: u32) -> PeerId {
    let mut b = [0u8; 32];
    b[..4].copy_from_slice(&id.to_le_bytes());
    PeerId::from_bytes(b)
}

fn peer_info(id: u32) -> PeerInfo {
    PeerInfo {
        peer_id: pid(id),
        addresses: vec![],
        transport_addresses: vec![],
        last_seen: None,
    }
}

fn sample_contact_list(n: u32, seed: u32) -> Vec<(PeerId, f64)> {
    (0..n)
        .map(|i| {
            (
                pid(seed.wrapping_mul(31).wrapping_add(i)),
                0.3 + ((i % 10) as f64) * 0.04,
            )
        })
        .collect()
}

fn bench_prophet(c: &mut Criterion) {
    let config = ProphetConfig::default();
    let mut pred = DeliveryPredictability::new(config);

    // 1,000-node table (BENCH-007 scale).
    for i in 0..1000u32 {
        let mut e = sample_contact_list(8, i);
        pred.meet(&pid(i), &e);
        e.clear();
    }

    // A fresh 32-node snapshot from another node's perspective (CapabilityBundle
    // TOP_N_DP = 32) → the `meet` update with a bounded prediction list.
    let contact = pid(999);
    let neighbor_view: Vec<(PeerId, f64)> = sample_contact_list(32, 42);

    // ROUT-33 (#1, #2, #4): the previous version mutated one shared `pred`
    // across every iteration — by the time the table hit MAX_DP_ENTRIES,
    // every `meet` call additionally paid an O(N) eviction scan
    // (prophet.rs's `min_by`), so the per-iteration cost grew and then
    // plateaued at a value not documented anywhere ("non-stationary"). It
    // also timed `neighbor_view.clone()` (an unrelated Vec allocation) as
    // part of the "meet" measurement. `iter_batched` now clones a fixed
    // 1,000-entry baseline (and the snapshot) in the untimed setup phase, so
    // every iteration measures the same steady-state `meet` cost and "1000"
    // in the sibling bench's name is accurate again.
    c.bench_function("routing/prophet_meet_32_snapshot", |b| {
        b.iter_batched(
            || (pred.clone(), neighbor_view.clone()),
            |(mut local_pred, snapshot)| black_box(local_pred.meet(black_box(&contact), &snapshot)),
            BatchSize::SmallInput,
        )
    });

    // ROUT-33 (#3): the previous version called `age()` repeatedly on the
    // same table with a wall clock that barely moves between iterations, so
    // `k` (whole aging intervals since last update) was 0 on every call
    // after the first — the decay branch (`gamma.powi(k)`) was never
    // exercised; the bench measured a HashMap clear-and-rebuild instead.
    // This version attaches a virtual clock, populates the table at t=0,
    // advances the clock by 5 aging intervals once, and clones that
    // zero-timestamped baseline fresh each iteration — every call now
    // computes k=5 and takes the real decay path.
    let clock = Arc::new(AtomicU64::new(0));
    let mut pred_for_age = DeliveryPredictability::new(config).with_virtual_clock(clock.clone());
    for i in 0..1000u32 {
        let mut e = sample_contact_list(8, i);
        pred_for_age.meet(&pid(i), &e);
        e.clear();
    }
    let aging_intervals_k = 5u64;
    clock.store(
        config.aging_interval.as_secs() * aging_intervals_k,
        Ordering::Relaxed,
    );
    c.bench_function("routing/prophet_age_1000_k5", |b| {
        b.iter_batched(
            || pred_for_age.clone(),
            |mut local_pred| black_box(local_pred.age()),
            BatchSize::SmallInput,
        )
    });

    // ROUT-32: a single reused message id meant `decide`'s per-message
    // GTMX+ monotonicity state (`max_dp_seen[(msg_id, dest)]`, ROUT-7)
    // ratcheted up on every iteration — by iteration ~50 every candidate was
    // already blacklisted, so >99.99% of a multi-million-iteration criterion
    // run measured the early-reject path (`gtmx_advantage`'s `seen` check),
    // not the real GTMX+ selection loop. A fresh message id each iteration
    // means every call is a genuinely new message, matching what `decide`
    // actually costs per forwarding opportunity in production (ROUT-14's
    // capacity bound on `max_dp_seen` keeps the router's memory use flat
    // across the many iterations this generates).
    let mut router = OpportunisticRouter::new(config);
    let candidates: Vec<(PeerId, f64)> = (0..50u32)
        .map(|i| (pid(i), 0.2 + (i as f64) * 0.005))
        .collect();
    c.bench_function("routing/opportunistic_decide_50_candidates", |b| {
        b.iter(|| {
            let msg_id = MessageId::new_v7();
            black_box(router.decide(
                black_box(&msg_id),
                black_box(&pid(7)),
                black_box(&candidates),
                black_box(MessagePriority::P3),
                black_box(3),
            ))
        })
    });

    // Binary spray handoff (ROUTE2_DESIGN.md §Spray) — the header's original
    // claim ("cold-start spray decisions") but never actually benchmarked
    // (ROUT-33 #5). Fresh message id + budget per iteration (ROUT-22's
    // sprayed_to dedup would otherwise reject every repeat on the same
    // contact after the first).
    let spray_contact = pid(500);
    c.bench_function("routing/spray_binary_handoff", |b| {
        b.iter_batched(
            || (MessageId::new_v7(), SprayBudget::new(8)),
            |(msg_id, mut budget)| black_box(router.spray(&msg_id, &spray_contact, &mut budget)),
            BatchSize::SmallInput,
        )
    });
}

/// ROUT-33 (#5): the file is named `routing.rs` but, before this fix,
/// benchmarked nothing in `routing::direct`, `routing::known_path`,
/// `routing::flood`, `routing::dedup_cache` or `RoutingEngine::decide` — the
/// L0 hot path BASELINE_ROUTING.md §Resource Budget actually sets latency
/// targets for (Algorithm 1 <1 us, Algorithm 2 <10 us, Algorithm 3 <1 ms for
/// 10 neighbors, dedup check <0.5 us, full decision <10 ms).
fn bench_l0_hot_path(c: &mut Criterion) {
    let rt = Runtime::new().expect("bench runtime");

    // Algorithm 1: Direct.
    let direct_table = NeighborTable::new(Duration::from_secs(60));
    rt.block_on(direct_table.upsert(&peer_info(7), &TransportId::from("sim"), LinkQuality::Excellent));
    c.bench_function("routing/try_direct", |b| {
        b.iter(|| rt.block_on(try_direct(black_box(pid(7)), black_box(&direct_table))))
    });

    // Algorithm 2: KnownPath. Recipient not a direct neighbor; reachable via
    // a routing-table entry whose next hop is.
    let known_path_neighbors = NeighborTable::new(Duration::from_secs(60));
    rt.block_on(known_path_neighbors.upsert(
        &peer_info(4),
        &TransportId::from("sim"),
        LinkQuality::Good,
    ));
    c.bench_function("routing/try_known_path", |b| {
        b.iter_batched(
            || {
                let mut table = RoutingTable::new(Duration::from_secs(600));
                table.upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
                table
            },
            |mut table| {
                black_box(rt.block_on(try_known_path(
                    black_box(pid(9)),
                    &mut table,
                    &known_path_neighbors,
                )))
            },
            BatchSize::SmallInput,
        )
    });

    // Algorithm 3: Flood recipient selection, 10 neighbors (the budget's own
    // stated scale) — this is the call site ROUT-20's ~175 KB-per-neighbor
    // clone lived on.
    let flood_neighbors = NeighborTable::new(Duration::from_secs(60));
    for n in 1..=10u32 {
        rt.block_on(flood_neighbors.upsert(
            &peer_info(n),
            &TransportId::from("sim"),
            LinkQuality::Good,
        ));
    }
    let flood_sender = pid(1);
    let flood_recipient = pid(999);
    c.bench_function("routing/recipients_for_flood_10_neighbors", |b| {
        b.iter(|| {
            black_box(rt.block_on(recipients_for_flood(
                black_box(&flood_neighbors),
                flood_sender,
                black_box(&[]),
                0,
                &flood_recipient,
            )))
        })
    });

    // Dedup check: is_duplicate against a cache already holding entries (the
    // steady-state case — a fresh empty cache is not representative).
    let mut dedup_cache = ForwardedCache::default();
    for _ in 0..100 {
        dedup_cache.record(MessageId::new_v7());
    }
    let dedup_miss_id = MessageId::new_v7(); // never recorded — the miss path (both bloom generations checked, no exact-set lookup needed)
    c.bench_function("routing/forwarded_cache_is_duplicate_miss", |b| {
        b.iter(|| black_box(dedup_cache.is_duplicate(black_box(&dedup_miss_id))))
    });

    // Full decision chain via RoutingEngine::decide. No route/neighbor
    // matches the recipient, so this exercises Direct-miss + KnownPath-miss
    // + budget check + Flood (10 neighbors) end-to-end — the realistic
    // per-message cost the budget's "full decision <10 ms" line names. Fresh
    // message id per iteration (ROUT-4's dedup gate would otherwise reject
    // every repeat after the first, same issue as ROUT-32).
    let engine_neighbors = NeighborTable::new(Duration::from_secs(60));
    for n in 1..=10u32 {
        rt.block_on(engine_neighbors.upsert(
            &peer_info(n),
            &TransportId::from("sim"),
            LinkQuality::Good,
        ));
    }
    let engine_recipient = pid(999);
    let engine_priority = MessagePriority::P4; // hop budget 3 (BASELINE_ROUTING.md §Algorithm 3)
    c.bench_function("routing/engine_decide_flood_path", |b| {
        b.iter_batched(
            RoutingEngine::new,
            |mut engine| {
                black_box(rt.block_on(engine.decide(
                    MessageId::new_v7(),
                    0,
                    0,
                    u64::MAX,
                    pid(1),
                    engine_recipient,
                    0,
                    engine_priority,
                    vec![],
                    &engine_neighbors,
                )))
            },
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_prophet, bench_l0_hot_path);
criterion_main!(benches);
