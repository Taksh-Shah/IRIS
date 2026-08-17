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
}
