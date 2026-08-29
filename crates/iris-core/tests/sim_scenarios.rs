//! Reproducible SIM-001 scenarios (ORCH-0001 WP-6 acceptance):
//! ≥3 scenarios (dense mesh, sparse partition carry, vehicle relay) with
//! delivery ratio + no-loop evidence, and same-seed determinism.

use iris_core::message::MessagePriority;
use iris_core::routing::ProphetConfig;
use iris_core::sim::metrics::SimMetrics;
use iris_core::sim::scenario::{
    community_ferry, dense_mesh, inject_sos, inject_standard, loss, partition_carry, sim_peer,
    vehicle_relay,
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
    sim.inject(Injection::new(
        100,
        0,
        sim_peer(3),
        MessagePriority::P4,
        b"on the road",
        3600,
    ));
    sim.inject(Injection::new(
        150,
        1,
        sim_peer(4),
        MessagePriority::P3,
        b"relay train",
        3600,
    ));
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
    assert!(
        m.delivery_ratio > 0.8,
        "carry should deliver most: {}",
        m.one_line()
    );
    assert!(m.loop_free);
    assert!(
        m.max_hops_seen >= 1,
        "carry requires at least one relay hop"
    );
}

#[test]
fn scenario_3_vehicle_relay_multihop() {
    let m = run_vehicle(11);
    assert!(
        m.delivered_total >= 2,
        "vehicle relay should deliver both messages: {}",
        m.one_line()
    );
    assert!(m.loop_free);
    assert!(m.max_hops_seen >= 1);
}

#[test]
fn determinism_same_seed_same_anchor() {
    assert_eq!(run_dense(99).result_anchor, run_dense(99).result_anchor);
    assert_eq!(
        run_partition(99).result_anchor,
        run_partition(99).result_anchor
    );
    assert_eq!(run_vehicle(99).result_anchor, run_vehicle(99).result_anchor);
}

#[test]
fn loss_reduces_delivery_ratio_monotonically() {
    // SIM-29: the old `lossy <= zero` non-strict inequality is satisfied
    // even if `set_loss` has NO effect whatsoever — since `set_loss` is
    // the sim's only failure knob, this was the one test that could not
    // detect it being silently broken (e.g. `is_lost()` short-circuiting
    // to `false`, or the loss draw wired to the wrong RNG stream). Strict
    // `<` plus a lower bound on the drop closes that gap.
    //
    // Loss rate empirically calibrated (not guessed): dense_mesh(6, ·)'s
    // ~30 contact events for just 4 messages give SCF's retry-until-
    // delivered semantics enough redundancy that 60% loss shows NO
    // measurable effect on seed 5 specifically (verified: zero=1.000,
    // lossy=1.000, an outlier — every other seed tried showed some drop
    // at 60%, just not reliably above a meaningful threshold). 80% loss
    // produced a robust, comfortably-above-threshold drop across every
    // seed sampled (0.2-0.7), so it — not a different seed — is the
    // minimal change that makes this scenario actually discriminating.
    let zero = run_dense(5).delivery_ratio;
    let mut sim = dense_mesh(6, 5);
    inject_standard(&mut sim, 4, 100, 3600);
    sim.set_loss(loss(80.0));
    let lossy = SimMetrics::from_outcome(&sim.run()).delivery_ratio;
    assert!(
        lossy < zero,
        "80% loss must strictly reduce delivery: zero-loss={zero:.3} lossy={lossy:.3}"
    );
    assert!(
        zero - lossy > 0.05,
        "80% loss should visibly move the delivery ratio, not just barely: \
         zero-loss={zero:.3} lossy={lossy:.3}"
    );
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

#[test]
fn route2_opportunistic_matches_delivery_with_lower_overhead() {
    // Sparse community + ferry mobility. L2 (DP + spray) must not reduce
    // delivery vs L0/SCF baseline, and should lower the overhead ratio.
    let base = run_ferry(7, false);
    let l2 = run_ferry(7, true);
    assert!(
        l2.delivery_ratio >= base.delivery_ratio - 0.05,
        "L2 must not degrade delivery: L0={:.3} L2={:.3}",
        base.delivery_ratio,
        l2.delivery_ratio
    );
    assert!(l2.loop_free, "L2 must stay loop-free");
    assert!(
        l2.delivery_ratio > 0.5,
        "ferry scenario must deliver: {}",
        l2.one_line()
    );
    // SIM-17: the test's own name and doc comment promise this — "should
    // lower the overhead ratio" — but until now nothing anywhere read
    // `overhead_ratio`/`overhead()` at all, so ROUTE-002's entire value
    // proposition (GTMX+ selectivity vs epidemic flooding) had zero
    // executable coverage. If this fails, that failure IS the finding —
    // not a threshold to loosen until it passes.
    assert!(
        l2.overhead() < base.overhead(),
        "L2 must lower overhead vs L0: L0={:.3} L2={:.3}",
        base.overhead(),
        l2.overhead()
    );
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
                sim.add_contact(iris_core::sim::ContactEvent {
                    at_ms: 500 * (a + b),
                    a: a as usize,
                    b: b as usize,
                });
            }
        }
    }
    inject_standard(&mut sim, 2, 50, 3600);
    let out = sim.run();
    let l = SprayBudget::l_for_priority(MessagePriority::P4, 3) as u64;
    assert!(l >= 1);
    // SIM-29: AC-4 is a *per-message* bound ("copies never exceed L per
    // message") — the old check computed relays_total / injected_total, a
    // MEAN across all messages, which cannot catch one message exploding
    // to 10xL while the rest are quiet (a mean of L is trivially satisfied
    // by e.g. half the messages at 0 relays and half at 2L). Check the max
    // over the per-message tally instead.
    assert!(
        !out.relays_by_msg.is_empty(),
        "scenario must produce at least one relay to exercise the bound"
    );
    let max_relays_for_one_msg = out.relays_by_msg.values().copied().max().unwrap_or(0);
    assert!(
        max_relays_for_one_msg <= l,
        "binary spray must bound relays for EVERY message <= L ({l}): \
         worst message had {max_relays_for_one_msg} relays"
    );
    assert!(
        out.avg_hop_count <= 3.0,
        "P4 hop budget must hold: avg hops {:.2}",
        out.avg_hop_count
    );
}

#[test]
fn sim32_inject_standard_no_self_addressed_messages() {
    // Regression: `inject_standard` with per_node == n - 1 would produce one
    // self-addressed message per source node (destination formula wraps around
    // to the sender). Those messages can never be delivered and silently deflate
    // the delivery ratio (SIM-32).
    //
    // n=4, per_node=3 (=n-1): k=2 gives d=s for every source. Before the fix
    // 4 undeliverable messages were injected; after the fix they are skipped.
    let mut sim = iris_core::sim::scenario::dense_mesh(4, 1);
    inject_standard(&mut sim, 3, 100, 3600);
    let out = sim.run();
    // For n=4, per_node=3: 4 sources × 3 - 4 self-addressed skips = 8 valid.
    assert_eq!(
        out.injected_total, 8,
        "with n=4 per_node=3, exactly 8 non-self-addressed messages should be injected (SIM-32)"
    );
    // Dense mesh should deliver ≥ 90% of those 8 valid messages.
    assert!(
        out.delivery_ratio > 0.9,
        "delivery ratio {:.2} unexpectedly low (SIM-32)",
        out.delivery_ratio
    );
}

#[test]
fn sim12_injection_at_contact_time_is_carried_on_that_contact() {
    // Regression: contacts sorted before injections at the same timestamp
    // (kind 0=contact, 1=injection), so a message injected at T always missed
    // the T-contacts and waited a full period (SIM-12).
    //
    // Setup: 2-node sim, one contact at t=100, one injection at t=100.
    // With the old ordering: contact fires first (no messages yet) → the
    // message is injected into node-0's buffer, but no further contact fires
    // → the message is never delivered and delivery_ratio == 0.
    // With the fix: injection fires first → message in node-0's buffer → the
    // t=100 contact fires → delivered immediately. delivery_ratio == 1.
    use iris_core::sim::{ContactEvent, Injection, Simulation};
    use iris_core::sim::scenario::sim_peer;

    let mut sim = Simulation::new(2, 42);
    sim.add_contact(ContactEvent { at_ms: 100, a: 0, b: 1 });
    sim.inject(Injection::new(
        100,
        0,
        sim_peer(1),
        iris_core::message::MessagePriority::P4,
        b"test",
        3600,
    ));
    let out = sim.run();
    assert_eq!(
        out.delivery_ratio, 1.0,
        "injection at the same ms as the only contact must be delivered on that contact (SIM-12)"
    );
}
