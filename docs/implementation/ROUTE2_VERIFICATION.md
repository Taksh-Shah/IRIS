# ROUTE-002 Verification Report

**Document ID**: IRIS-ROUTE2-VER-001
**Version**: 1.0
**Node**: ROUTE-002 (WP-8), type ROUTING, priority P1
**Date**: 2026-08-14
**Research basis**: RES-0011 (RFC 6693 PRoPHET v2, Lindgren 2003, Grasic
CHANTS 2011, Spyropoulos WDTN 2005 + ToN 2008, Keränen ONE 2009)
**Design**: docs/implementation/ROUTE2_DESIGN.md v1.0

---

## Implementation

`crates/iris-core/src/routing/` + `crates/iris-core/src/sim/` — L2
opportunistic routing layer on top of the ROUTE-001 L0 engine:

- **`prophet.rs`** — `ProphetConfig` (P_encounter_first=0.5, P_encounter_max=0.7,
  P_first_threshold=0.1, delta=0.01, alpha=0.5, beta=0.9, gamma per Appendix C
  default 0.999/interval) + `DeliveryPredictability`: RFC 6693 §2.1.2 Eq. 1
  direct update with P_encounter interval scaling, Eq. 2 gamma aging, Eq. 3
  **MAX** transitivity (not additive), directional DPs (never forced
  symmetric), P(A,A)=1 identity, top-N snapshot (`TOP_N_DP=32`), hard cap
  `MAX_DP_ENTRIES=4096` + `P_first_threshold` discard, optional virtual clock
  for SIM-001 determinism.
- **`opportunistic.rs`** — `OpportunisticRouter` (one DP table + per-message
  `max_dp_seen` monotonicity guard) + `SprayBudget` (binary spray-and-wait,
  default `L=8`, per-priority caps P0 unlimited / P1–2 = L / P3 = max(4,·) /
  P4+ = 1, tied to L0 hop budgets). `decide(recipient, candidates[{PeerId,dp}],
  priority, budget) → OpportunisticDecision { ForwardTo, SprayHandoff,
  NoAdvantage }` with GTMX+ (strict `p_next > p_self + eps` AND
  `p_next > P_first_threshold` AND `p_next > max_dp_seen`).
- **`routing/mod.rs`** — `ForwardingAlgorithm::Opportunistic` variant;
  `RoutingEngine::with_opportunistic(ProphetConfig)` + `opportunistic()`
  accessors; `decide_opportunistic()` hook consulted in `decide()` between
  KnownPath and Flood (single-copy forward short-circuits the flood); empty
  DP table / no candidates → `None` → L0 chain unchanged (AC 5).
- **`sim/mod.rs`** — `Simulation::with_opportunistic()`; per-node
  `OpportunisticRouter`; `exchange_on_contact()` swaps DP snapshots once per
  contact event; `opportunistic_forward()` applies GTMX+ per message with
  cold-start fallback to L0 relay; `relays_total` + `avg_hop_count` metrics.
- **`sim/scenario.rs`** — `community_ferry(4, 4, 1, 2500, seed)` (two
  isolated communities A:0-3, B:4-7, ferry node 8 alternating A↔B every
  2500 ms) + `inject_standard` P4 messages.

## Acceptance Criteria Verification (ROUTE2_DESIGN.md AC 1-6)

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | DP maintained/aged/transitive per RFC §2.1.2; exact numeric behavior of Eq. 1/2/3 + interval scaling + directional asymmetry + P(A,A)=1 | prophet.rs tests: `first_contact_sets_p_encounter_first`, `direct_update_converges_to_bound`, `aging_decays_by_gamma_per_interval`, `transitivity_uses_max_form`, `dp_is_directional_and_self_is_one`, `snapshot_top_n_sorted_deterministic`, `capped_entries_evict_lowest` (7/7) | ✅ PASS |
| 2 | Opportunistic forwarding integrated with ROUTE-001 engine — `Opportunistic` decision surfaced between KnownPath and Flood; P0/direct/known-path paths unchanged | routing/mod.rs `decide()`: Direct → KnownPath → decide_opportunistic → Flood → Store ordering; `direct_when_neighbor`, `known_path_before_flood`, `floods_to_all_eligible_neighbors`, `hop_budget_exhausted_stores` still green (L0 regression), `with_opportunistic` opt-in (None default) | ✅ PASS |
| 3 | Delivery ratio improves or matches L0 in SIM sparse mobility; overhead lower | `community_ferry` seed 7, 18 P4 msgs, TTL 7200: **L0 del 17/18 (94.4%) overhead 3.18 hops 1.29; L2 del 17/18 (94.4%) overhead 1.41 hops 1.00** — same delivery, −55.6% overhead, −0.29 avg hops, loop-free both. Integration test `route2_opportunistic_matches_delivery_with_lower_overhead` (asserts L2 ≥ L0−0.05 + loop_free + ratio > 0.5) | ✅ PASS |
| 4 | Bounded overhead (no duplicate explosion) — spray cap L=8 + GTMX+ | `route2_spray_bounds_overhead_property`: 12-node grid, 24 contacts, `handoffs_per_msg ≤ L` (binary spray invariant) + avg hops ≤ P4 budget 3; opportunistic.rs `spray_l_by_priority_respects_hop_budget`, `binary_handoff_halves_copies`, `spray_handoff_until_budget_exhausted` | ✅ PASS |
| 5 | Graceful fallback to L0 when no history — empty DP table → NoAdvantage → L0 Flood/Store unchanged; P_first_threshold gating; cold-start spray | opportunistic.rs `gtmx_requires_strict_advantage_above_floor`, `decide_no_advantage_when_candidates_not_better`, `decide_requires_candidate_above_self_and_floor`; routing/mod.rs `decide_opportunistic` returns `None` when no DP view or hop budget exhausted; sim/mod.rs `opportunistic_forward` falls through to L0 when L2 not armed on both ends | ✅ PASS |
| 6 | Tests cover 1-5; workspace green + clippy 0 warnings | 7 prophet + 7 opportunistic + 3 sim integration = 17 ROUTE-002 tests; workspace **208 green** (189 core + 8 sim_scenarios + 10 storage + 1 M3), `cargo clippy --workspace --all-targets` 0 warnings | ✅ PASS |

## Bug Fixed During Verification (role-swap in `opportunistic_forward`)

`sim/mod.rs` `opportunistic_forward` used `split_at_mut`, which for
`src > dst` returned `(dst, src)` — so `a` (the "forwarding" node) was the
**receiver** and GTMX+ decided from the receiver's perspective. Every ferry
contact is 8→0 or 8→4 (src > dst), so the ferry withheld all handoffs and L2
dropped its own ferry-sourced message (idx=17 src=8→dst=1): **L2 16/18
(88.8%) vs L0 17/18 (94.4%)** — an AC-3 violation. The Greater branch now
returns `(hi[0], lo[dst])` = `(src, dst)`.

A/B verified: buggy code panics `route2_opportunistic_matches_delivery_with_
lower_overhead` (0.888 < 0.944 − 0.05); fixed code restores parity. The only
remaining undelivered message (idx=12 src=3→dst=5) is TTL-bound and identical
in L0 and L2: its path requires node 3→0 (t=1000) → ferry∩0 (t=5400) →
ferry∩4 (t=7900) → node 4→5 (t=10000), but TTL 7200 expires it at t=7350 —
a scenario/schedule limitation, not a routing regression.

## Test Evidence (2026-08-14)

- 7 prophet tests (Eq. 1/2/3 numerics, interval scaling, directional DP,
  self-identity, top-N snapshot determinism, entry cap)
- 7 opportunistic tests (GTMX+ floor/strict-advantage, best-candidate
  selection, no-advantage, budget guard, binary handoff, per-priority L,
  spray exhaustion)
- 3 sim integration tests (`route2_opportunistic_matches_delivery_with_lower_
  overhead`, `route2_opportunistic_deterministic_same_seed`,
  `route2_spray_bounds_overhead_property`)
- Workspace: iris-core 189 (incl. prophet 7 + opportunistic 7 + sim 11 +
  gateway 19) + iris-storage 10 + sim_scenarios 8 + M3 1 = **208 green**;
  clippy 0 warnings

## Known Limitations (recorded for follow-ups)

- PRoPHET is IRTF Experimental (RFC 6693); defaults compiled-in and
  versioned, not adaptive. Gamma calibration sweep deferred (EXP exists).
- DP exchange rides DISCO-001 CapabilityBundle (256 B budget → top-N
  snapshot TOP_N_DP=32); first-class wire types 13/14 deferred.
- Per-message `max_dp_seen` is in-session state (not persisted across
  reboot).
- End-to-end authenticated DP exchange deferred to SEC-001/AUTH-001
  integration; DPs are advisory only (hop budget + dedup still enforced).
- MaxProp-style message ordering + per-priority age boost deferred.

## Conclusion

AC 1-6 **all PASS** with unit, property, and SIM integration evidence.
ROUTE-002 L2 opportunistic routing is COMPLETE: matches L0 delivery in the
sparse ferry scenario at 55.6% lower overhead and 1.00 avg hops, bounded
spray copies, loop-free, deterministic under same seed, and regresses
gracefully to L0 when no DP history exists.
