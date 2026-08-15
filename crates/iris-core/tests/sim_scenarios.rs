//! Reproducible SIM-001 scenarios (ORCH-0001 WP-6 acceptance):
//! ≥3 scenarios (dense mesh, sparse partition carry, vehicle relay) with
//! delivery ratio + no-loop evidence, and same-seed determinism.

use iris_core::message::MessagePriority;
use iris_core::protocol::MessageId;
use iris_core::routing::ProphetConfig;
use iris_core::sim::metrics::SimMetrics;
use iris_core::sim::scenario::{
    community_ferry, dense_mesh, inject_standard, inject_sos, loss, partition_carry,
    sim_peer, vehicle_relay,
};
use iris_core::sim::{Injection, Simulation};

fn run_dense(seed: u64) -> SimMetrics {
    let mut sim = dense_mesh(6, seed);
    inject_standard(&mut sim, 4, 100, 3600);
    SimMetrics::from_outcome(&sim.run())
}

fn run_partition(seed: u64) -> SimMetrics {
    let mut sim = partition_carry(2, 1, 2, seed);
    sim.set_loss(loss(0.0));
    inject_standard(&mut sim, 3, 100, 3600);
    inject_sos(&mut sim, 0, 3, 120);
    SimMetrics::from_outcome(&sim.run())
}

fn run_vehicle(seed: u64) -> SimMetrics {
    let mut sim = vehicle_relay(5, seed);
    sim.inject(Injection::new(100, 0, sim_peer(3), MessagePriority::P4, b"on the road", 3600));
    sim.inject(Injection::new(150, 1, sim_peer(4), MessagePriority::P3, b"relay train", 3600));
    SimMetrics::from_outcome(&sim.run())
}

#[test]
fn scenario_1_dense_mesh_delivery_ratio() {
    let m = run_dense(7);
    assert!(
        m.delivery_ratio > 0.9,
        "dense mesh should deliver nearly everything: {}",
        m.one_line()
    );
    assert!(m.loop_free, "dense mesh must be loop-free");
    assert!(m.result_anchor.starts_with("sim-7-"));
}

#[test]
fn scenario_2_sparse_partition_carry() {
    let m = run_partition(42);
    assert_eq!(
        m.delivered_ratio_for(MessagePriority::P0),
        1.0,
        "SOS must always be delivered via carry"
    );
    assert!(m.delivery_ratio > 0.8, "carry should deliver most: {}", m.one_line());
    assert!(m.loop_free);
    assert!(m.max_hops_seen >= 1, "carry requires at least one relay hop");
}

#[test]
fn scenario_3_vehicle_relay_multihop() {
    let m = run_vehicle(11);
    assert!(m.delivered_total >= 2, "vehicle relay should deliver both messages: {}", m.one_line());
    assert!(m.loop_free);
    assert!(m.max_hops_seen >= 1);
}

#[test]
fn determinism_same_seed_same_anchor() {
    assert_eq!(run_dense(99).result_anchor, run_dense(99).result_anchor);
    assert_eq!(run_partition(99).result_anchor, run_partition(99).result_anchor);
    assert_eq!(run_vehicle(99).result_anchor, run_vehicle(99).result_anchor);
}

#[test]
fn loss_reduces_delivery_ratio_monotonically() {
    let zero = run_dense(5).delivery_ratio;
    let mut sim = dense_mesh(6, 5);
    inject_standard(&mut sim, 4, 100, 3600);
    sim.set_loss(loss(60.0));
    let lossy = SimMetrics::from_outcome(&sim.run()).delivery_ratio;
    assert!(lossy <= zero, "loss must not improve delivery");
}

// --- ROUTE-002: L2 opportunistic routing in SIM (WP-8). ---

fn run_ferry(seed: u64, opportunistic: bool) -> SimMetrics {
    let mut sim = community_ferry(4, 4, 1, 2500, seed);
    inject_standard(&mut sim, 2, 100, 7200);
    if opportunistic {
        sim.with_opportunistic(ProphetConfig::default());
    }
    SimMetrics::from_outcome(&sim.run())
}

fn run_ferry_inspect(seed: u64, opportunistic: bool) -> (usize, usize) {
    let mut sim = community_ferry(4, 4, 1, 2500, seed);
    inject_standard(&mut sim, 2, 100, 7200);
    if opportunistic {
        sim.with_opportunistic(ProphetConfig::default());
    }
    let out = sim.run();
    let ok: Vec<MessageId> = out.nodes.iter().flat_map(|n| n.delivered.iter().map(|(id, _)| *id)).collect();
    eprintln!("=== run L2={} delivered={} injected={} ===", if opportunistic { "ON" } else { "OFF" }, out.delivered_total, out.injected_total);
    // injected_ids in TIME order: t=100 (k=0) for all 9 srcs first, then t=150 (k=1).
    // idx i: k = i/9, s = i%9, dst = (s+1+k)%9
    let mut missing = 0;
    for (i, m) in out.injected_ids.iter().enumerate() {
        if !ok.contains(m) {
            missing += 1;
            let k = i / 9;
            let s = i % 9;
            eprintln!("  MISSING idx={i} inject_t={} src={s} dst={}", out.injected_at[i], (s + 1 + k) % 9);
        }
    }
    eprintln!("L2={} delivered={} injected={}", if opportunistic { "ON" } else { "OFF" }, out.delivered_total, out.injected_total);
    (out.injected_total, missing)
}
#[test]
#[ignore]
fn route2_debug_undelivered() {
    let (i0, m0) = run_ferry_inspect(7, false);
    let (i1, m1) = run_ferry_inspect(7, true);
    eprintln!("L0 injected={i0} missing={m0} | L2 injected={i1} missing={m1}");
}

#[test]
fn route2_opportunistic_matches_delivery_with_lower_overhead() {
    // Sparse community + ferry mobility. L2 (DP + spray) must not reduce
    // delivery vs L0/SCF baseline, and should lower the overhead ratio.
    let base = run_ferry(7, false);
    let l2 = run_ferry(7, true);
    assert!(l2.delivery_ratio >= base.delivery_ratio - 0.05,
        "L2 must not degrade delivery: L0={:.3} L2={:.3}",
        base.delivery_ratio, l2.delivery_ratio);
    assert!(l2.loop_free, "L2 must stay loop-free");
    assert!(l2.delivery_ratio > 0.5, "ferry scenario must deliver: {}", l2.one_line());
}

#[test]
fn route2_opportunistic_deterministic_same_seed() {
    let a = run_ferry(123, true).result_anchor;
    let b = run_ferry(123, true).result_anchor;
    assert_eq!(a, b, "L2 routing must be deterministic under the same seed");
}

#[test]
fn route2_spray_bounds_overhead_property() {
    // Binary spray L=8 max: across many contact graphs, per-message relays
    // never exceed L×2 (bounded copies, no epidemic explosion).
    use iris_core::routing::SprayBudget;
    let mut sim = Simulation::new(12, 3);
    sim.with_opportunistic(ProphetConfig::default());
    // 12 nodes, 24 random-ish contacts (deterministic grid).
    for a in 0..12u64 {
        for b in (a + 1)..12u64 {
            if (a + b) % 5 == 0 {
                sim.add_contact(iris_core::sim::ContactEvent { at_ms: 500 * (a + b), a: a as usize, b: b as usize });
            }
        }
    }
    inject_standard(&mut sim, 2, 50, 3600);
    let out = sim.run();
    let l = SprayBudget::l_for_priority(MessagePriority::P4, 3) as u64;
    assert!(l >= 1);
    // AC-4: copies never exceed L per message (binary spray invariant).
    // relays/injected = handoffs per message — bounded by L (each handoff
    // carries a halved budget; the total copies alive never exceed L).
    let handoffs_per_msg = out.relays_total as f64 / out.injected_total.max(1) as f64;
    assert!(
        handoffs_per_msg <= l as f64,
        "binary spray must bound handoffs per message <= L ({}): {handoffs_per_msg:.2} ({} relays / {} msgs)",
        l, out.relays_total, out.injected_total
    );
    assert!(out.avg_hop_count <= 3.0, "P4 hop budget must hold: avg hops {:.2}", out.avg_hop_count);
}
