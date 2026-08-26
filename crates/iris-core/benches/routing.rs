//! AC-10 (DEC-TEST-0011) PRoPHET routing benchmarks.
//!
//! 1,000-node delivery-predictability table: the RFC 6693 `meet` (encounter)
//! update and `age` (aging) — the CPU cost of a real-scale contact event.
//! Cold-start spray decisions over a neighborhood list benchmark the routing
//! hot path used per forwarding opportunity.

use criterion::{criterion_group, criterion_main, Criterion};
use iris_core::message::{MessagePriority, PeerId};
use iris_core::protocol::MessageId;
use iris_core::routing::opportunistic::OpportunisticRouter;
use iris_core::routing::prophet::{DeliveryPredictability, ProphetConfig};
use std::hint::black_box;

fn pid(id: u32) -> PeerId {
    let mut b = [0u8; 32];
    b[..4].copy_from_slice(&id.to_le_bytes());
    PeerId::from_bytes(b)
}

fn bench_routing(c: &mut Criterion) {
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

    let mut router = OpportunisticRouter::new(config);
    let candidates: Vec<(PeerId, f64)> = (0..50u32)
        .map(|i| (pid(i), 0.2 + (i as f64) * 0.005))
        .collect();

    c.bench_function("routing/prophet_meet_32_snapshot", |b| {
        b.iter(|| {
            let mut snapshot = neighbor_view.clone();
            black_box(pred.meet(black_box(&contact), &snapshot));
            snapshot.clear();
        })
    });
    c.bench_function("routing/prophet_age_1000", |b| {
        b.iter(|| black_box(pred.age()))
    });
    let bench_msg_id = MessageId::new_v7();
    c.bench_function("routing/opportunistic_decide_50_candidates", |b| {
        b.iter(|| {
            black_box(router.decide(
                black_box(&bench_msg_id),
                black_box(&pid(7)),
                black_box(&candidates),
                black_box(MessagePriority::P3),
                black_box(3),
            ))
        })
    });
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

criterion_group!(benches, bench_routing);
criterion_main!(benches);
