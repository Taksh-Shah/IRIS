//! Scenario builder — SIM-001 (WP-6).
//!
//! Reproducible contact-schedule scenarios that exercise the store-carry-
//! forward path and the routing hop budgets. Each builder returns a ready
//! [`crate::sim::Simulation`]; the same seed yields the same result anchor.

use crate::message::MessagePriority;
use crate::sim::{ContactEvent, Injection, SimLoss, Simulation};

/// Build a peer id for sim index `i` (stable across scenarios).
pub use crate::sim::sim_peer;

/// **Dense mesh**: N nodes all mutually in range, delivering directly.
/// Expected: ~100% delivery, 1-hop latency.
pub fn dense_mesh(nodes: usize, seed: u64) -> Simulation {
    let mut sim = Simulation::new(nodes, seed);
    let t = 1000u64;
    for a in 0..nodes {
        for b in (a + 1)..nodes {
            sim.add_contact(ContactEvent { at_ms: t, a, b });
            sim.add_contact(ContactEvent {
                at_ms: t + 500,
                a,
                b,
            });
        }
    }
    sim
}

/// **Sparse partition + carry**: `mid` carries messages between two isolated
/// islands. Message injected at island A at t=0, carried to island B at a
/// later contact, and returns via a second ferry leg so traffic in BOTH
/// directions is deliverable (two-way ferry — a one-way schedule would strand
/// B→A messages and mask itself as "carry loss"). Expected: delayed but
/// complete delivery (P4, within TTL).
pub fn partition_carry(island_a: usize, mid: usize, island_b: usize, seed: u64) -> Simulation {
    let total = island_a + mid + island_b;
    let mut sim = Simulation::new(total, seed);
    let b_base = island_a + mid;
    // Island A nodes contact the carrier at t=1000 (outbound leg).
    for i in 0..island_a {
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: i,
            b: island_a,
        });
    }
    // Carrier meets island B at t=4000 (outbound delivery); island B fully
    // meshed at t=2000.
    for i in 0..island_b {
        sim.add_contact(ContactEvent {
            at_ms: 4000,
            a: island_a,
            b: b_base + i,
        });
        for j in (i + 1)..island_b {
            sim.add_contact(ContactEvent {
                at_ms: 2000,
                a: b_base + i,
                b: b_base + j,
            });
        }
    }
    // Return leg: carrier meets island A again at t=6000 so B→A messages
    // ride back (two-way carry).
    for i in 0..island_a {
        sim.add_contact(ContactEvent {
            at_ms: 6000,
            a: i,
            b: island_a,
        });
    }
    sim
}

/// **Vehicle relay**: a moving relay node visits a line of stations one after
/// another, carrying messages forward (store-carry-forward over time).
pub fn vehicle_relay(stations: usize, seed: u64) -> Simulation {
    let total = stations + 1; // + relay node at index `stations`
    let relay = stations;
    let mut sim = Simulation::new(total, seed);
    let step = 1000u64;
    for s in 0..stations {
        sim.add_contact(ContactEvent {
            at_ms: step * (s as u64 + 1),
            a: s,
            b: relay,
        });
        // Station-to-station same-line links (backbone within station cluster).
        if s + 1 < stations {
            sim.add_contact(ContactEvent {
                at_ms: step * (s as u64 + 1) + 300,
                a: s,
                b: s + 1,
            });
        }
    }
    sim
}

/// **Community + ferry** (ROUTE-002 sparse mobility): two isolated communities
/// (A: 0..a, B: a..a+b) each internally meshed and periodically linked by a
/// ferry node at index `a+b`. DP learning rewards crosses via the ferry;
/// opportunistic routing should match or exceed L0 delivery with lower
/// overhead. Ferry alternates A ↔ B every `period_ms` for `cycles` round
/// trips; intra-community contacts recur so DPs propagate within a community.
pub fn community_ferry(
    community_a: usize,
    community_b: usize,
    ferry: usize,
    period_ms: u64,
    seed: u64,
) -> Simulation {
    let total = community_a + community_b + ferry;
    let ferry_idx = community_a + community_b;
    let mut sim = Simulation::new(total, seed);
    let b_base = community_a;
    // Intra-community full mesh (within each community), recurring so DP
    // learning spreads and messages relay inside a community.
    for i in 0..community_a {
        for j in (i + 1)..community_a {
            for t in [100u64, 1000, 5000, 9000, 13_000] {
                sim.add_contact(ContactEvent {
                    at_ms: t,
                    a: i,
                    b: j,
                });
            }
        }
    }
    for i in 0..community_b {
        for j in (i + 1)..community_b {
            for t in [200u64, 2000, 6000, 10_000, 14_000] {
                sim.add_contact(ContactEvent {
                    at_ms: t,
                    a: b_base + i,
                    b: b_base + j,
                });
            }
        }
    }
    // Ferry alternates: A at t=0, B at t=period, A again, ... for `cycles`
    // round trips (enough visits that DP advantage builds and carry lands).
    let mut t = 400;
    for _ in 0..(period_ms.min(10_000) / 2500).max(4) * 2 {
        let from_a = (t / period_ms).is_multiple_of(2);
        let member = if from_a { 0 } else { b_base };
        sim.add_contact(ContactEvent {
            at_ms: t,
            a: ferry_idx,
            b: member,
        });
        t += period_ms;
    }
    sim
}

/// **Scheduled ferry** (ML-001 GT arm): a gateway-capable ferry at index
/// `stations` visits a line of `stations` terminals on a mostly-strict
/// schedule — one visit to terminal `s` at `t = s*step + c*period` (fixed
/// cadence), plus ONE deterministic maintenance gap (a missed run ≈ 3 periods
/// of silence at the middle of the timeline). The robust GT (median period)
/// tolerates the gap; GW-001's static mean does not → GT strictly beats static
/// on this regime (ML-001 AC-4). No jitter otherwise.
pub fn periodic_ferry(stations: usize, period_ms: u64, step_ms: u64, seed: u64) -> Simulation {
    let total = stations + 1; // + gateway ferry at index `stations`
    let ferry = stations;
    let mut sim = Simulation::new(total, seed);
    let cycles = 8u64;
    let maintenance_at = 1; // single long gap right after the first cycle
    for c in 0..cycles {
        // Deterministic maintenance gap: one long silence early in the run (a
        // missed ferry run / detour) — ~3x the normal period. Every cycle
        // at/after the boundary shifts forward by the gap, so the schedule is
        // perfectly periodic BEFORE and AFTER, with exactly one long gap at
        // the boundary (no time reversal). Placed in the FIRST cycle window so
        // the train slice contains the outlier while the test slice (late
        // timeline) is cleanly periodic — GT's robust median stays at the true
        // period, the static mean does not (ML-001 AC-4).
        let gap = if c >= maintenance_at {
            3 * period_ms
        } else {
            0
        };
        for s in 0..stations {
            let at = c * period_ms + s as u64 * step_ms + gap;
            sim.add_contact(ContactEvent {
                at_ms: at,
                a: s,
                b: ferry,
            });
            // Terminal-to-terminal backbone within the line (contact
            // frequency so in-line messages relay even before the ferry).
            if s + 1 < stations {
                sim.add_contact(ContactEvent {
                    at_ms: at + step_ms / 2,
                    a: s,
                    b: s + 1,
                });
            }
        }
    }
    sim
}

/// **Random-walk contacts** (ML-001 GT no-help bound): `pairs` contact events
/// with random endpoints at random times. No periodicity → a learned GT timing
/// predictor must NOT beat a non-robust static baseline here (ML-001 AC-4
/// parity); also stresses LP under aperiodic relay opportunities.
pub fn random_walk(nodes: usize, pairs: usize, seed: u64) -> Simulation {
    use rand::Rng;
    use rand::SeedableRng;
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed.wrapping_add(0x5eed_2026));
    let mut sim = Simulation::new(nodes, seed);
    let mut t = 500u64;
    for _ in 0..pairs {
        let a = rng.gen_range(0..nodes);
        let b = rng.gen_range(0..nodes);
        if a != b {
            sim.add_contact(ContactEvent { at_ms: t, a, b });
        }
        // Aperiodic gap between 200 and 2000 ms (uniform).
        t += rng.gen_range(200..=2000);
    }
    sim
}

/// Inject P4 messages from every source node to a rotating peer (used by the
/// three canonical scenarios to produce measurable delivery ratios).
pub fn inject_standard(sim: &mut Simulation, per_node: usize, start_ms: u64, ttl: u64) {
    let n = sim.node_count();
    for s in 0..n {
        for k in 0..per_node {
            let d = (s + 1 + k) % n;
            sim.inject(Injection::new(
                start_ms + k as u64 * 50,
                s,
                sim_peer(d),
                MessagePriority::P4,
                format!("n{s}-k{k}").as_bytes(),
                ttl,
            ));
        }
    }
}

/// Inject an SOS (P0) message: emergency class, effectively unlimited hops.
pub fn inject_sos(sim: &mut Simulation, from: usize, to: usize, at_ms: u64) {
    sim.inject(Injection::new(
        at_ms,
        from,
        sim_peer(to),
        MessagePriority::P0,
        b"SOS",
        7200,
    ));
}

/// Loss helper: convert a percentage to a [`SimLoss`].
pub fn loss(percent: f32) -> SimLoss {
    SimLoss {
        rate: (percent / 100.0).clamp(0.0, 1.0),
    }
}

/// **PILOT-001 B-1: NCT-of-N relay capacity.** `nodes` all mutually in range
/// on a recurring full-mesh schedule (5 rounds); `per_node` P4 messages
/// injected at t=0 from every source to a rotating peer. Near-lossless all-N
/// delivery + latency CDF at N = 20/50/100 (PILOT_001_DESIGN.md §5 B-1,
/// AC-7). Same seed → same result anchor.
pub fn pilot_nct_of_n(nodes: usize, per_node: usize, seed: u64) -> Simulation {
    let mut sim = Simulation::new(nodes, seed);
    for r in 0..5u64 {
        let t = 1000 + r * 1000;
        for a in 0..nodes {
            for b in (a + 1)..nodes {
                sim.add_contact(ContactEvent { at_ms: t, a, b });
            }
        }
    }
    for s in 0..nodes {
        for k in 0..per_node {
            let d = (s + 1 + k) % nodes;
            sim.inject(Injection::new(
                0,
                s,
                sim_peer(d),
                MessagePriority::P4,
                format!("nct{s}-k{k}").as_bytes(),
                3600,
            ));
        }
    }
    sim
}

/// **PILOT-001 B-2: partition/healing.** Two equal, internally full-meshed
/// partitions (A: 0..part, B: part..2*part) with NO inter-partition contact
/// until `heal_at_ms` (the Internet-relay uplink is "down" for that window —
/// B-2's uplink-down proxy at the SIM layer), then a full healing mesh.
/// Intra-partition messages deliver during the partition; cross-partition
/// messages ride the heal. Asserter asserts no duplicate terminal delivery
/// after healing (PILOT_001_DESIGN.md §5 B-2, AC-8).
pub fn pilot_partition_heal(part: usize, heal_at_ms: u64, seed: u64) -> Simulation {
    let total = 2 * part;
    let mut sim = Simulation::new(total, seed);
    let b_base = part;
    for a in 0..part {
        for b in (a + 1)..part {
            sim.add_contact(ContactEvent { at_ms: 1000, a, b });
            sim.add_contact(ContactEvent { at_ms: 5000, a, b });
        }
    }
    for a in 0..part {
        for b in (a + 1)..part {
            sim.add_contact(ContactEvent {
                at_ms: 1000,
                a: b_base + a,
                b: b_base + b,
            });
            sim.add_contact(ContactEvent {
                at_ms: 5000,
                a: b_base + a,
                b: b_base + b,
            });
        }
    }
    for a in 0..part {
        for b in 0..part {
            sim.add_contact(ContactEvent {
                at_ms: heal_at_ms,
                a,
                b: b_base + b,
            });
            sim.add_contact(ContactEvent {
                at_ms: heal_at_ms + 1000,
                a,
                b: b_base + b,
            });
        }
    }
    sim
}

/// **PILOT-001 B-3: multi-hop SCF mule-chain.** A strict chain of `segments`
/// store-carry-forward legs: node `i` meets node `i+1` at `(i+1)*1000` ms,
/// so a message injected at node 0 for node `segments` is carried leg-by-leg
/// and delivered at `segments*1000` ms — the contact-window expectation
/// (latency == legs * per-leg window; hop-count CDF = segments).
pub fn pilot_mule_chain(segments: usize, seed: u64) -> Simulation {
    let total = segments + 1;
    let mut sim = Simulation::new(total, seed);
    for i in 0..segments {
        sim.add_contact(ContactEvent {
            at_ms: (i as u64 + 1) * 1000,
            a: i,
            b: i + 1,
        });
    }
    sim
}

/// **PILOT-001 B-4: iOS limited-relay.** 5 Android leaves in a full mesh plus
/// one iOS leaf (index 5) that participates ONLY inside sparse foregrounded
/// windows: one contact at t=2000 (meets Android 0) and one at t=12000 (meets
/// Android 4). An iOS-recipient message delivers at window 1; an iOS-origin
/// message buffers until window 2 — the limited-relay asymmetry (higher
/// latency, still ≥ floor) is the recorded behavior (PILOT_001_DESIGN.md §5
/// B-4, AC-10; physical legs BLK-0005-gated).
pub fn pilot_ios_limited_relay(seed: u64) -> Simulation {
    let total = 6;
    let ios = 5;
    let mut sim = Simulation::new(total, seed);
    for a in 0..5 {
        for b in (a + 1)..5 {
            sim.add_contact(ContactEvent { at_ms: 1000, a, b });
            sim.add_contact(ContactEvent { at_ms: 5000, a, b });
        }
    }
    sim.add_contact(ContactEvent {
        at_ms: 2000,
        a: 0,
        b: ios,
    });
    sim.add_contact(ContactEvent {
        at_ms: 12_000,
        a: 4,
        b: ios,
    });
    sim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_mesh_delivers_directly() {
        let mut sim = dense_mesh(6, 7);
        inject_standard(&mut sim, 3, 200, 3600);
        let out = sim.run();
        assert!(out.delivered_total > 0);
        assert!(
            out.delivery_ratio > 0.95,
            "mesh must be near-lossless: {}",
            out.delivery_ratio
        );
        assert!(out.loop_free());
    }

    #[test]
    fn partition_carry_delivers_with_sos() {
        let mut sim = partition_carry(2, 1, 2, 42);
        sim.set_loss(loss(0.0));
        inject_sos(&mut sim, 0, 2, 100);
        let out = sim.run();
        assert_eq!(out.delivered_ratio_for(MessagePriority::P0), 1.0);
        assert!(out.loop_free());
    }

    #[test]
    fn vehicle_relay_produces_multihop_delivery() {
        let mut sim = vehicle_relay(4, 11);
        sim.inject(Injection::new(
            100,
            0,
            sim_peer(3),
            MessagePriority::P4,
            b"along the line",
            3600,
        ));
        let out = sim.run();
        assert!(out.delivered_total >= 1);
        assert!(out.max_hops_seen >= 1, "vehicle relay should traverse hops");
    }

    // --- PILOT-001 TEST (iter ~156): SIM-001 pre-validation of B-1..B-4 ---
    use crate::sim::metrics::{wilson_ci_95, SimMetrics};

    #[test]
    fn pilot_b1_nct_of_n_delivers_all_with_latency_cdf() {
        // B-1 NCT-of-N at N=20: all-N delivery + latency CDF within §7
        // thresholds (AC-7). All messages ride the first 1000 ms contact round.
        let sim = pilot_nct_of_n(20, 1, 7);
        let out = sim.run();
        assert_eq!(
            out.delivered_total, out.injected_total,
            "N=20: every injected message delivered (all-N)"
        );
        assert!(out.loop_free());
        let m = SimMetrics::from_outcome(&out);
        // Latency CDF: p50/p95 ≈ 1000 ms (1 hop) — well inside floor (5 s / 30 s).
        assert!(m.latency_p50_ms <= 5000, "p50 {} ms", m.latency_p50_ms);
        assert!(m.latency_p95_ms <= 30_000, "p95 {} ms", m.latency_p95_ms);
        let (lo, _hi) = wilson_ci_95(out.delivered_total, out.injected_total);
        // Honest small-N statistics: perfect delivery at N=20 has Wilson-95%
        // lower bound 0.839 (k==n CI is wide at n<50); the ≥ 0.90 floor is met
        // at the N=50/100 legs (0.929 / 0.963).
        assert!(lo > 0.80 && lo < 0.90, "Wilson-95% lower bound {lo}");
    }

    #[test]
    fn pilot_b1_nct_of_n_scales_to_100() {
        // B-1 NCT-of-N at N=50 and N=100: all-N delivery holds at scale; the
        // Wilson-95% lower bound clears the 0.90 delivery floor at both legs
        // (0.929 / 0.963 — perfect delivery at n>=50 is statistically solid;
        // cf. the n=20 leg in pilot_b1_nct_of_n_delivers_all_with_latency_cdf).
        for n in [50usize, 100] {
            let sim = pilot_nct_of_n(n, 1, 3);
            let out = sim.run();
            assert_eq!(
                out.delivered_total, out.injected_total,
                "N={n}: all-N delivery at scale"
            );
            assert!(out.loop_free());
            let (lo, hi) = wilson_ci_95(out.delivered_total, out.injected_total);
            assert!(
                lo >= 0.90 && hi <= 1.0,
                "N={n}: Wilson-95% lower bound {lo} < 0.90 floor"
            );
        }
    }

    #[test]
    fn pilot_b2_partition_heals_without_duplicates() {
        // B-2 partition/healing (AC-8): uplink down T=60 s (no A<->B contact),
        // mesh continues, heal delivers cross-partition traffic, zero dups.
        let part = 4;
        let heal_at = 60_000;
        let mut sim = pilot_partition_heal(part, heal_at, 99);
        for s in 0..part {
            sim.inject(Injection::new(
                0,
                s,
                sim_peer((s + 1) % part),
                MessagePriority::P2,
                b"a",
                3600,
            ));
        }
        for s in 0..part {
            sim.inject(Injection::new(
                0,
                part + s,
                sim_peer(part + ((s + 1) % part)),
                MessagePriority::P2,
                b"b",
                3600,
            ));
        }
        sim.inject(Injection::new(
            0,
            0,
            sim_peer(part),
            MessagePriority::P2,
            b"cross",
            3600,
        ));
        let out = sim.run();
        let cross_id = out
            .injected_ids
            .last()
            .copied()
            .expect("cross injection id");
        // P1/P2 floor: all 9 P2 messages deliver (partitions + heal).
        assert!(
            out.delivered_ratio_for(MessagePriority::P2) >= 0.9,
            "P2 delivery {}",
            out.delivered_ratio_for(MessagePriority::P2)
        );
        // Intra-partition messages delivered DURING the partition (< heal).
        let intra_during = out
            .nodes
            .iter()
            .flat_map(|n| n.delivered.iter())
            .filter(|(id, at)| *id != cross_id && *at < heal_at)
            .count();
        assert_eq!(intra_during, 8, "8 intra messages before heal");
        // Cross-partition message delivered by the heal.
        let cross_at = out
            .nodes
            .iter()
            .flat_map(|n| n.delivered.iter())
            .find(|(id, _)| *id == cross_id)
            .map(|(_, at)| *at);
        assert!(
            cross_at.is_some() && cross_at.unwrap() >= heal_at,
            "cross delivered at heal: {cross_at:?}"
        );
        // Heal introduces no duplicate terminal delivery.
        assert!(out.loop_free());
    }

    #[test]
    fn pilot_b3_mule_chain_delivers_within_contact_window() {
        // B-3 multi-hop SCF mule-chain (AC-9): hop-count CDF + contact-window
        // expectation — latency == legs × per-leg window (1000 ms each). P3
        // (hop budget 5) so the 4-leg chain fits under the anti-loop budget.
        let segments = 4;
        let mut sim = pilot_mule_chain(segments, 17);
        sim.inject(Injection::new(
            0,
            0,
            sim_peer(segments),
            MessagePriority::P3,
            b"chain",
            20_000,
        ));
        let out = sim.run();
        let id = out.injected_ids[0];
        let delivered_at = out
            .nodes
            .iter()
            .flat_map(|n| n.delivered.iter())
            .find(|(i, _)| *i == id)
            .map(|(_, at)| *at)
            .expect("chain message delivered");
        assert_eq!(delivered_at, segments as u64 * 1000);
        assert_eq!(out.max_hops_seen as usize, segments);
        assert!(out.loop_free());
    }

    #[test]
    fn pilot_b4_ios_limited_relay_participates_on_windows() {
        // B-4 iOS limited-relay (AC-10): iOS leaf participates only in its
        // foregrounded windows → delivers on-window; iOS-origin traffic waits
        // for the next window (asymmetry recorded), still ≥ floor.
        let mut sim = pilot_ios_limited_relay(11);
        sim.inject(Injection::new(
            0,
            0,
            sim_peer(5),
            MessagePriority::P4,
            b"to-ios",
            7200,
        ));
        sim.inject(Injection::new(
            2500,
            5,
            sim_peer(4),
            MessagePriority::P4,
            b"from-ios",
            7200,
        ));
        let out = sim.run();
        assert!(
            out.delivery_ratio >= 0.95,
            "limited-relay floor: {}",
            out.delivery_ratio
        );
        let to_ios = out.injected_ids[0];
        let from_ios = out.injected_ids[1];
        let to_ios_at = out
            .nodes
            .iter()
            .flat_map(|n| n.delivered.iter())
            .find(|(i, _)| *i == to_ios)
            .map(|(_, at)| *at)
            .expect("to-ios delivered at first window");
        assert_eq!(to_ios_at, 2000);
        let from_ios_at = out
            .nodes
            .iter()
            .flat_map(|n| n.delivered.iter())
            .find(|(i, _)| *i == from_ios)
            .map(|(_, at)| *at)
            .expect("from-ios delivered at next window");
        assert_eq!(from_ios_at, 12_000, "iOS-origin waits for next window");
        assert!(out.loop_free());
    }
}
