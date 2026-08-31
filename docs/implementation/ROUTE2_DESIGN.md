# ROUTE-002 Design — L2 Opportunistic Routing (PRoPHET v2 + Binary Spray-and-Wait)

**Document ID**: IRIS-ROUTE2-DESIGN-001
**Version**: 1.0
**Node**: ROUTE-002 (WP-8), type ROUTING, priority P1
**Dependencies**: ROUTE-001 (COMPLETE)
**Date**: 2026-08-13
**Research basis**: RES-0011 (es) — RFC 6693 read in full (IRTF Experimental,
no errata), Lindgren 2003, Grasic CHANTS 2011, Spyropoulos WDTN 2005 + ToN
2008, Keränen ONE 2009.

---

## Scope

Add an **L2 opportunistic routing layer** on top of the ROUTE-001 L0
deterministic engine (`RoutingEngine::decide`: Direct→KnownPath→Flood→Store).
When the L0 chain would fall through to Flood, the opportunistic layer asks:
"is there a neighbor whose **delivery predictability** (DP) for the
destination exceeds my own?" If yes, forward **only to that neighbor**
(single-copy, GTMX+ enforced) instead of flooding every neighbor. When the
routing table has no useful history, L2 falls back to L0 **binary
spray-and-wait** (bounded copies, then wait-phase direct only). If neither
advises forwarding, the message continues to Flood/Store exactly as today —
**no behavioral regression for messages with no DP history** (AC 5).

No wire protocol change: DP exchange rides the existing DISCO-001
`CapabilityBundle` advertisement during contacts (same seam as GW-001).
PRoPHET is IRTF Experimental — treated as research-grade baseline, defaults
compiled-in and versioned (RES-0011 risk 6).

## RFC 6693 v2 equations (RES-0011 §headline)

| Name | Formula | RFC § |
|---|---|---|
| Direct (Eq. 1) | `P(A,B) = P_old + (1 − delta − P_old)·P_encounter(intvl)` | §2.1.2 |
| P_encounter interval scaling | `P_encounter(intvl) = P_encounter_max·(I_typ/intvl)^alpha` for `intvl ≥ I_typ`; `= P_encounter_max` for `intvl < I_typ`; **first contact** `= P_encounter_first` | §2.1.2, §3.3 |
| Aging (Eq. 2) | `P(A,B) = P_old·gamma^K` (`K` = aging intervals since last update) | §2.1.2 |
| Transitivity (Eq. 3 — **MAX form, not additive**) | `P(A,C) = MAX(P_old, P(A,B)·P(B,C)_recv·beta)` | §2.1.2 |
| `P(A,A) = 1` | identity | §2.1.1 |

Defaults (RES-0011 §c, calibrated for IRIS): `P_encounter_first = 0.5`,
`P_encounter_max = 0.7`, `P_first_threshold = 0.1`, `delta = 0.01`,
`alpha = 0.5`, `beta = 0.9`, `gamma` derived per aging unit via Appendix C
(defensive default `0.999` per interval). DPs are **directional** — never
force symmetry (RES-0011 §b9).

## Forwarding strategy — GTMX+ (RFC §3.6, RES-0011 §b2)

- Consult a neighbor `B` for message to `D` iff
  `P(B,D) > P(A,D) + eps` **and** `P(B,D) > P_first_threshold`.
- **P_max monotonicity (GTMX+ anti-oscillation guard)**: track per-message
  `max_dp_seen`; never forward to a node with DP ≤ previously seen maximum.
  Independent of `ForwardedCache`.
- **Hop budget reuse**: candidate must be within the L0 priority hop budget
  (`max_hops_for_priority`). Selecting an opportunistic forward consumes one
  hop just like a flood hop.

## Binary Spray-and-Wait (cold start, RES-0011 §b3)

- Per-message `SprayBudget { remaining: u32 }`, starts at L.
- On contact with a fresh node: **binary** handoff `give = floor(remaining/2)`
  (keep `remaining − give`); in wait phase (`remaining ≤ 1`) only deliver
  directly to the recipient.
- `DEFAULT_SPRAY_L = 5` (P1 budget; ROUT-23 resolution 2026-08-31 — authoritative
  source is OPPORTUNISTIC_ROUTING.md §3.1; EXP-ROUTE-002 validated at L=5).
- Per-priority spray cap: P0 → unlimited (epidemic semantics unchanged),
  P1 = 5, P2 = 3, P3 = 2, P4+ = 1 (direct-only). Values are per-priority
  constants; hop-budget capping removed (all values already ≤ their priority
  hop-budget ceiling).

## State budget

`DeliveryPredictability` is **O(N) per node** (RES-0011 §b5): float DP +
last-meet timestamp per known destination. Transitivity fills toward N —
bounded by `P_first_threshold` discard (§2.1.3.2) + hard cap
`MAX_DP_ENTRIES` (4096, > IRIS design scale 10k-peers-is-aggregate not per
node). DP attributes ride `CapabilityBundle` (≤256 B budget → top-N snapshot
exchange, `TOP_N_DP = 32`).

## Integration points

1. **`crates/iris-core/src/routing/prophet.rs`** (new) — `ProphetConfig`,
   `DeliveryPredictability` (DP map + aging + transitivity + meet +
   top-N snapshot + prune), optional virtual clock for SIM-001 determinism.
2. **`crates/iris-core/src/routing/opportunistic.rs`** (new) —
   `OpportunisticRouter` holding `DeliveryPredictability` + per-message
   `max_dp_seen` + `SprayBudget`; `decide(recipient, candidates_[{PeerId,dp}],
   priority, hop) -> OpportunisticDecision { ForwardTo, SprayHandoff,
   NoAdvantage }`; `OpportunisticReason { Prophet | Spray }`.
3. **`crates/iris-core/src/routing/mod.rs`** — `ForwardingAlgorithm::Opportunistic`
   variant; `RoutingEngine::with_opportunistic(router)` + accessor;
   `opportunistic_decide(...)` hook consulted inside `decide()`
   Direct/KnownPath pass-through unchanged (L0 regression-free), between
   KnownPath and Flood.
4. **`crates/iris-core/src/sim/`** — optional `with_opportunistic()` on
   `Simulation`; per-node `DeliveryPredictability`; `forward_to` consults
   DP exchange + GTMX+ before deciding Flood/relay; **new metrics**
   `overhead_ratio` + `avg_hop_count` (ONE methodology parity, RES-0011 §b8).
5. **`crates/iris-core/src/sim/scenario.rs`** — new `community_ferry`
   scenario (two communities + periodic ferry node) exercising DP learning
   and spray; used to compare opportunistic vs L0 baseline delivery/overhead.

## Acceptance criteria (AC 1–6) — evidence target

1. DP maintained/aged/transitive per RFC §2.1.2 formulas → unit tests
   asserting exact numeric behavior of Eq. 1/2/3 + interval scaling +
   directional asymmetry + `P(A,A)=1`.
2. Opportunistic forwarding integrated with ROUTE-001 engine → `Opportunistic`
   decision surfaced by RoutingEngine between KnownPath and Flood; P0/
   direct/known-path paths byte-identical to L0 (regression tests).
3. Delivery ratio improves or matches L0 in SIM sparse-mobility →
   `community_ferry` scenario: opportunistic delivery ≥ baseline; overhead
   ratio lower.
4. Bounded overhead (no duplicate explosion) → per-priority spray caps (P1=5,
   P2=3, P3=2, P4+=1) + GTMX+; `overhead_ratio` measured; property test:
   copies never exceed L×2 across a 50-node random contact graph.
5. Graceful fallback to L0 when no history → empty DP table returns
   `NoAdvantage` → L0 Flood/Store unchanged; `P_first_threshold` gating;
   cold-start spray path tested.
6. Tests cover 1–5 → unit + property + SIM integration suite; workspace
   green + clippy 0 warnings.

## Failure-mode analysis (LOOP_POLICY step 4)

| Failure | Handling |
|---|---|
| No DP history (cold start) | `NoAdvantage` → L0; binary spray bounds spread |
| Rogue node inflates its DP | GTMX+ P_max monotonicity; DPs are advice only — hop budget + dedup still enforced; routing metadata rides authenticated control plane (SEC-001 follow-up, RES-0011 risk 2) |
| DP table grows | `P_first_threshold` discard + `MAX_DP_ENTRIES` hard cap + top-N snapshot |
| Oscillation A→B→A on DP tie | strict `p_next > p_self + eps`, plus `max_dp_seen` monotonicity per message |
| Aging clock wrong / node churn | `last_meet` per node; stale DPs decay via gamma^K; SIM uses virtual clock (deterministic) |

## Out of scope (recorded for ML-001/ROUTE-002 follow-ups)

- MaxProp-style message-ordering + per-priority age boost (deferred).
- Adaptive `gamma` calibration sweep (EXP-ROUTE-002 exists; add EXP for
  gamma sweep) — defaults compiled-in, verifiable.
- DP attributes as first-class wire type 13/14 (rides CapabilityBundle for
  v1 to stay within PROTO-001 budgets).
- End-to-end authenticated DP exchange (SEC-001/AUTH-001 integration).

## Verification approach

`ROUTE2_VERIFICATION.md` maps AC 1–6 to implementations + tests
(unit/property/integration/SIM). Parameter defaults from RES-0011 §c.
Baseline = ROUTE-001 behavior on identical SIM inputs with opportunistic
disabled.