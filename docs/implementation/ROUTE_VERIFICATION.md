# ROUTE-001 Verification Report

**Document ID**: IRIS-ROUTE-VER-001
**Version**: 1.0
**Node**: ROUTE-001 (WP-4)
**Date**: 2026-08-13

---

## Implementation

`crates/iris-core/src/routing/` — L0 deterministic routing
(`docs/routing/BASELINE_ROUTING.md`, ORCH-0001 WP-4):

- **`mod.rs`** — `RoutingEngine` with the four-algorithm chain
  `Direct → KnownPath → Flood → Store`, `ForwardingDecision` enum, `decide()`
  single-entrypoint, `record_contact()` hook (feeds routing table + contact
  log for SCF), `prune()`.
- **`direct.rs`** — Algorithm 1: O(1) neighbor-table lookup when the
  destination is a live neighbor.
- **`known_path.rs`** — Algorithm 2: best-route lookup, next-hop reachability
  verification, entry invalidation on unreachable next hop, 10-min expiry
  prune.
- **`flood.rs`** — Algorithm 3: priority hop budgets (P0 unlimited,
  P1–P3 = 5, P4–P7 = 3), no-backtrack to sender, exclusion of already-flooded
  + peer-dedup hints. Property tests on no-loops + no-reflood.
- **`store.rs`** — Algorithm 4: TTL re-check (RFC 9171 §4.4.2 arrival-time
  semantics shared with MSG-001), store-or-drop decision; P0-never-evicted
  handled by STORE (INV-ROUTE-003).
- **`dedup_cache.rs`** — post-forward dedup window (Bloom 0.1% FPR + exact set
  + ring buffer) per BASELINE §Anti-Loop.

## Acceptance Criteria Verification (BASELINE_ROUTING.md + ORCH-0001 WP-4)

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | Algorithm chain resolves Direct → KnownPath → Flood → Store in order | `direct_when_neighbor`, `known_path_before_flood`, `floods_to_all_eligible_neighbors`, `hop_budget_exhausted_stores` | ✅ PASS |
| 2 | Flood terminates; no loops; hop-count monotonic | `flood_terminates_no_loops_property` (50 randomized topologies, budget never exceeded), `flood_no_backtrack_no_reflood_property` (50 cases) | ✅ PASS |
| 3 | RoutingTable entries expire after 10 min | `prune_expired_entries` (TTL-based prune verified) | ✅ PASS |
| 4 | Next-hop unreachable → route invalidated | `unreachable_next_hop_invalidates` | ✅ PASS |
| 5 | Dedup window prevents re-forwarding | `duplicate_detection_works`, `different_ids_not_duplicates`, `ring_eviction_removes_expired_exact_entries` | ✅ PASS |
| 6 | M4 integration: Discovery → Routing | `m4_discovery_feeds_routing_direct_delivery` (DiscoveryManager neighbor table consumed by RoutingEngine → direct hop) | ✅ PASS |
| 7 | No new clippy warnings; workspace tests green | `cargo test --workspace` 134 green; clippy 0 warnings | ✅ PASS |

## Test Evidence (2026-08-13)

- 24 routing tests: direct 2, known_path 4, flood 7 (incl. 2 property tests),
  store 3, dedup_cache 4, engine 4 (incl. M4 integration)
- Workspace: iris-core 134 + iris-storage 10 + M3 integration 1 = 145 green

## Protocol Conformance (PROTO-001 / DTN_ROUTING.md)

- Store/carry/forward delegation honors `MessageStorage` seam (STORE-001).
- Hop budgets align with BASELINE_ROUTING.md defaults; P0 flood unlimited by
  design (emergency guarantee).
- No-backtrack + dedup-window satisfy the DTN no-loop correctness guarantees.

## Known Limitations (recorded in PROJECT_GRAPH.yaml)

- Routing table is populated by contact records (via `record_contact`, fed by
  DISCO-001 events); ACK-backpropagation and routing-update gossip (payload
  type 10) land with SCF-001.
- `decide()` returns decisions; transport hand-off/retry loop lives in the
  engine integration pass (SIM-001).
- Per-priority LQI-aware transport selection (battery >20% gating) is
  operationalized in SIM-001.

## Verdict: IMPLEMENTED

All 7 acceptance criteria pass with implementation evidence. No blockers on the
WP-5 (SCF-001) / WP-6 (SIM-001) paths.