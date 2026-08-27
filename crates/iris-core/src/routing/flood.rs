//! Algorithm 3: Flood with Hop Limit — BASELINE_ROUTING.md §Algorithm 3.
//!
//! Hop caps per priority: 3 for P4–P7, 5 for P1–P3, unlimited for P0.
//! Anti-loop: no-backtrack to the sender, never re-flood a neighbor already in
//! the caller's already-flooded set, and the recently-forwarded window
//! (`ForwardedCache`) prevents a node from re-processing a known message.
//!
//! Correctness: hop budget is monotone-decreasing on every hop, so the flood
//! terminates; dedup ensures each node forwards a given message at most once.

use crate::discovery::neighbor_table::{best_transport, NeighborTable};
use crate::message::{MessagePriority, PeerId};
use crate::transport::TransportId;

/// Hard per-hop fan-out cap: at most this many neighbors receive a flood copy
/// from a single node on a single hop (ROUT-17, CONGESTION_CONTROL.md §204).
/// 8 is the L1 value in the spec; P0 is exempt (epidemic semantics).
pub const MAX_FLOOD_FANOUT: usize = 8;

/// Priority-derived flood policy (BASELINE_ROUTING.md §Algorithm 3 default
/// max_hops + no-backtrack).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloodPolicy {
    /// Hard hop ceiling. No priority class uses `u8::MAX` — see ROUT-18.
    pub max_hops: u8,
}

/// Hop ceiling per priority band.
///
/// ROUT-18: P0 previously mapped to `u8::MAX` ("unlimited"), which in a
/// dedup-free mesh produced unbounded amplification. It is now bounded to
/// `P0_MAX_HOPS` (16), a real mesh-diameter ceiling matching EMERG_DESIGN.md.
pub const P0_MAX_HOPS: u8 = 16;

/// Hop ceiling per priority band.
pub fn max_hops_for_priority(p: MessagePriority) -> FloodPolicy {
    let max_hops = match p {
        // ROUT-18: use a real diameter bound, not the u8::MAX sentinel.
        MessagePriority::P0 => P0_MAX_HOPS,
        MessagePriority::P1 | MessagePriority::P2 | MessagePriority::P3 => 5,
        MessagePriority::P4 | MessagePriority::P5 | MessagePriority::P6 | MessagePriority::P7 => 3,
    };
    FloodPolicy { max_hops }
}

/// Build the flood recipient list for one message.
///
/// Excludes:
/// - the message source (`sender`) — no backtracking,
/// - neighbors already in `already_flooded`,
/// - the recipient itself if it happens to be a neighbor here (Direct handles
///   that case first; kept out to avoid needless copies).
///
/// ROUT-17: the result is capped at `MAX_FLOOD_FANOUT` to prevent broadcast
/// storms — a fully-connected K-regular mesh with no cap yields K^hop_count
/// transmissions per injected message. `hop_count` is available for future
/// priority-based tapering; the cap applies uniformly for now.
///
/// ROUT-21: each recipient carries its own best-quality live transport
/// (selected via [`best_transport`]) rather than leaving the caller to
/// re-derive one — a flood's recipients are not all reachable over the same
/// transport on a multi-transport node. A `LinkedUp` neighbor with no
/// selectable link is skipped rather than included with nothing to send
/// over.
pub async fn recipients_for_flood(
    neighbor_table: &NeighborTable,
    sender: PeerId,
    already_flooded: &[PeerId],
    hop_count: u8,
    recipient: &PeerId,
) -> Vec<(PeerId, TransportId)> {
    let _ = hop_count; // available for future priority-based taper (ROUT-17 TODO)
    let mut recipients = Vec::new();
    // ROUT-20: use the cheap projection (peer_id, state, links) — this loop
    // never reads `peer_bloom` or `capabilities`, and cloning the full
    // `Neighbor` (peer_bloom alone is ~175 KB) here allocated megabytes per
    // flood decision.
    for nb in neighbor_table.neighbor_summaries().await {
        if nb.peer_id == sender
            || nb.peer_id == *recipient
            || already_flooded.contains(&nb.peer_id)
        {
            continue;
        }
        // Any transport with a live link is enough to route a flood copy.
        if matches!(
            nb.state,
            crate::discovery::neighbor_table::NeighborState::LinkedUp
        ) {
            if let Some(transport) = best_transport(&nb.links) {
                recipients.push((nb.peer_id, transport));
                // ROUT-17: hard fan-out cap — stop collecting once we hit the limit.
                if recipients.len() >= MAX_FLOOD_FANOUT {
                    break;
                }
            }
        }
    }
    recipients
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discovery::neighbor_table::NeighborTable;
    use crate::message::{LinkQuality, PeerInfo};
    use std::time::Duration;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn peer_info(id: u8) -> PeerInfo {
        PeerInfo {
            peer_id: pid(id),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    async fn table_with(ids: &[u8]) -> NeighborTable {
        let t = NeighborTable::new(Duration::from_secs(60));
        for id in ids {
            t.upsert(
                &peer_info(*id),
                &TransportId::from("sim"),
                LinkQuality::Good,
            )
            .await;
        }
        t
    }

    #[tokio::test]
    async fn rout17_fanout_capped_at_max() {
        // Regression: recipients_for_flood had no cap; a K-connected node
        // produced K-1 copies per hop (ROUT-17). Result must never exceed
        // MAX_FLOOD_FANOUT regardless of how many LinkedUp neighbors exist.
        let t = table_with(&(1..=(MAX_FLOOD_FANOUT as u8 + 5)).collect::<Vec<_>>()).await;
        let r = recipients_for_flood(&t, pid(0), &[], 0, &pid(99)).await;
        assert!(
            r.len() <= MAX_FLOOD_FANOUT,
            "flood fanout must not exceed MAX_FLOOD_FANOUT (got {})",
            r.len()
        );
    }

    #[tokio::test]
    async fn flood_excludes_sender_and_prior() {
        let t = table_with(&[1, 2, 3, 4]).await;
        let r = recipients_for_flood(&t, pid(1), &[pid(3)], 0, &pid(99)).await;
        assert_eq!(r.len(), 2);
        assert!(r.iter().any(|(id, _)| *id == pid(2)));
        assert!(r.iter().any(|(id, _)| *id == pid(4)));
    }

    #[tokio::test]
    async fn flood_includes_excellence_bit() {
        let t = table_with(&[2]).await;
        let r = recipients_for_flood(&t, pid(1), &[], 0, &pid(99)).await;
        assert_eq!(r, vec![(pid(2), TransportId::from("sim"))]);
    }

    #[tokio::test]
    async fn priority_hop_budgets() {
        // ROUT-18: P0 must never be u8::MAX again.
        assert_eq!(max_hops_for_priority(MessagePriority::P0).max_hops, P0_MAX_HOPS);
        assert!(
            P0_MAX_HOPS < u8::MAX,
            "P0 hop budget must be a real bound, not u8::MAX sentinel (ROUT-18)"
        );
        assert_eq!(max_hops_for_priority(MessagePriority::P1).max_hops, 5);
        assert_eq!(max_hops_for_priority(MessagePriority::P3).max_hops, 5);
        assert_eq!(max_hops_for_priority(MessagePriority::P4).max_hops, 3);
        assert_eq!(max_hops_for_priority(MessagePriority::P7).max_hops, 3);
    }

    // --- Property tests (ORCH-0001 WP-4 acceptance: no loops, hop count
    // monotonic decrease) ---

    /// Simulate a flood across a real multi-node graph. Verifies:
    /// 1. hop_count never exceeds the priority hop cap,
    /// 2. total transmissions never exceed calls-made x MAX_FLOOD_FANOUT
    ///    (ROUT-17's cap actually bounds the whole run, not just one call),
    /// 3. no node is ever added to the frontier twice,
    /// 4. `reached` (a visited node is adjacent to the recipient, i.e. Direct
    ///    delivery becomes possible on the next decide() call) is true
    ///    whenever the recipient is within `max + 1` graph hops of the
    ///    source (`+1` because ROUT-19 exempts the final Direct hop from the
    ///    flood hop budget) — checked against an independent ground-truth
    ///    BFS over the same graph, not against the function under test.
    ///
    /// ROUT-28: the previous version of this test built one *shared*
    /// `NeighborTable` for all "nodes", so every node saw an identical
    /// neighbor set — it was not a graph. Its three assertions were also
    /// unable to fail: `hop <= max` held by loop construction, `visited.len()
    /// == visited.len()` compared a value with itself, and `reached` was
    /// computed then discarded via `let _ = reached;`. This version gives
    /// each node its own `NeighborTable` (built from a seeded RNG for
    /// reproducibility) and checks reachability independently.
    #[tokio::test]
    async fn flood_terminates_no_loops_property() {
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};
        use std::collections::{HashMap, HashSet};

        /// Shortest-path distance in hops, or `None` if unreachable.
        fn bfs_distance(adjacency: &HashMap<u8, HashSet<u8>>, source: u8, dest: u8) -> Option<u32> {
            if source == dest {
                return Some(0);
            }
            let mut visited = HashSet::new();
            visited.insert(source);
            let mut frontier = vec![source];
            let mut dist = 0u32;
            while !frontier.is_empty() {
                dist += 1;
                let mut next = Vec::new();
                for &node in &frontier {
                    for &nb in &adjacency[&node] {
                        if nb == dest {
                            return Some(dist);
                        }
                        if visited.insert(nb) {
                            next.push(nb);
                        }
                    }
                }
                frontier = next;
            }
            None
        }

        let n = 12u8;
        let max = max_hops_for_priority(MessagePriority::P4).max_hops; // 3
        let source = 1u8;
        let dest = 12u8;

        for seed in 0..50u64 {
            let mut rng = StdRng::seed_from_u64(seed);

            // Symmetric random graph, degree capped at MAX_FLOOD_FANOUT so the
            // fan-out cap never truncates a real edge — otherwise ground-truth
            // BFS (which sees the whole graph) would not be a sound oracle for
            // the capped flood.
            let mut adjacency: HashMap<u8, HashSet<u8>> =
                (1..=n).map(|id| (id, HashSet::new())).collect();
            for id in 1..=n {
                for _ in 0..rng.gen_range(1..=4) {
                    let other = rng.gen_range(1..=n);
                    if other != id
                        && adjacency[&id].len() < MAX_FLOOD_FANOUT
                        && adjacency[&other].len() < MAX_FLOOD_FANOUT
                    {
                        adjacency.get_mut(&id).unwrap().insert(other);
                        adjacency.get_mut(&other).unwrap().insert(id);
                    }
                }
            }

            // One NeighborTable per node — the fix for the shared-table bug.
            let mut tables: HashMap<u8, NeighborTable> = HashMap::new();
            for (&id, neighbors) in &adjacency {
                let t = NeighborTable::new(Duration::from_secs(60));
                for &nb in neighbors {
                    t.upsert(&peer_info(nb), &TransportId::from("sim"), LinkQuality::Good)
                        .await;
                }
                tables.insert(id, t);
            }

            let ground_truth_reachable = bfs_distance(&adjacency, source, dest)
                .map(|d| d <= max as u32 + 1)
                .unwrap_or(false);

            let mut visited: HashSet<u8> = HashSet::new();
            visited.insert(source);
            let mut frontier = vec![source];
            let mut reached = source == dest || adjacency[&source].contains(&dest);
            let mut hop: u8 = 0;
            let mut calls_made: usize = 0;
            let mut total_transmissions: usize = 0;
            let mut total_frontier_entries: usize = 0;
            while !frontier.is_empty() && hop < max && !reached {
                hop += 1;
                let mut next = Vec::new();
                for &node in &frontier {
                    let node_table = &tables[&node];
                    let r = recipients_for_flood(node_table, pid(node), &[], hop, &pid(dest)).await;
                    calls_made += 1;
                    total_transmissions += r.len();
                    for (nb_peer, _transport) in r {
                        let nb_id = nb_peer.0[0];
                        if visited.insert(nb_id) {
                            next.push(nb_id);
                            total_frontier_entries += 1;
                            if adjacency[&nb_id].contains(&dest) {
                                reached = true;
                            }
                        }
                    }
                }
                frontier = next;
            }

            assert!(hop <= max, "seed {seed}: flood must respect hop budget (got {hop} > {max})");
            assert!(
                total_transmissions <= calls_made * MAX_FLOOD_FANOUT,
                "seed {seed}: total transmissions {total_transmissions} exceeded {calls_made} calls x MAX_FLOOD_FANOUT ({MAX_FLOOD_FANOUT})"
            );
            assert_eq!(
                total_frontier_entries,
                visited.len() - 1,
                "seed {seed}: a node was added to the frontier more than once"
            );
            assert_eq!(
                reached, ground_truth_reachable,
                "seed {seed}: flood reached={reached} but ground-truth BFS (distance <= {}) says {ground_truth_reachable}",
                max as u32 + 1
            );
        }
    }

    /// Property: the recipient list never includes the sender (no-backtrack)
    /// and never includes a node already in the flooded set.
    ///
    /// ROUT-29: the previous version built `already` via
    /// `(1..=8).filter(|i| *i != sender.0[0] % 8).map(|i| pid(i + 1))` — a
    /// 1-based/0-based index mismatch (`% 8` compared against a 1-based
    /// range) plus an off-by-one (`pid(i + 1)` instead of `pid(i)`, reaching
    /// `pid(9)`, which isn't even in the table). Net effect: `already`
    /// contained 7-8 of the table's 8 peers for every seed, so
    /// `recipients_for_flood` returned at most one element and the
    /// assertions held trivially. This version uses an explicit `already`
    /// subset and asserts the *exact* expected recipient set, plus a
    /// separate empty-`already` case exercising sender exclusion alone.
    #[tokio::test]
    async fn flood_no_backtrack_no_reflood_property() {
        let t = table_with(&[1, 2, 3, 4, 5, 6, 7, 8]).await;
        let sender = pid(1);

        // Case 1: a real, partial already-flooded set.
        let already = vec![pid(3), pid(5)];
        let r = recipients_for_flood(&t, sender, &already, 0, &pid(99)).await;
        let mut r_ids: Vec<PeerId> = r.iter().map(|(id, _)| *id).collect();
        r_ids.sort_by_key(|p| p.0[0]);
        let expected: Vec<PeerId> = [2u8, 4, 6, 7, 8].into_iter().map(pid).collect();
        assert_eq!(
            r_ids, expected,
            "must be exactly the non-sender, non-already-flooded peers"
        );
        assert!(!r_ids.contains(&sender), "no-backtrack violated");
        for a in &already {
            assert!(!r_ids.contains(a), "re-flood to already-flooded node");
        }

        // Case 2: empty already-flooded set — sender exclusion exercised on
        // its own, every other peer eligible.
        let r2 = recipients_for_flood(&t, sender, &[], 0, &pid(99)).await;
        let mut r2_ids: Vec<PeerId> = r2.iter().map(|(id, _)| *id).collect();
        r2_ids.sort_by_key(|p| p.0[0]);
        let expected2: Vec<PeerId> = (2u8..=8).map(pid).collect();
        assert_eq!(
            r2_ids, expected2,
            "with no already-flooded set, every non-sender peer is eligible"
        );
        assert!(!r2_ids.contains(&sender));
    }
}
