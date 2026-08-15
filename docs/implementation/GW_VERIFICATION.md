# GW-001 Verification Report

**Document ID**: IRIS-GW-VER-001
**Version**: 1.1 (red-team hardened)
**Node**: GW-001 (WP-7)
**Date**: 2026-08-13

---

## Implementation

`crates/iris-core/src/gateway/` — gateway discovery & selection engine:

- **`mod.rs`** — `GatewayType` (Internet/Lora/Satellite/UsbEthernet),
  `GatewayCapability` (bandwidth, latency, reliability, cost, load, queue,
  duty cycle, per-type priority limit), `GatewayCandidate` (hop distance +
  path), `GatewaySelection` (`None`/`All`/`WithBackup`/`Single`),
  `compute_gateway_quality` (weighted 0.30/0.20/0.15/0.20/0.10/0.05 score,
  priority-incompatibility → 0), `GatewayHealthMonitor`
  (`GatewayHealthState` Healthy/Probation/Failed/HardFailed state machine —
  red-team hardening, see below), `GatewayManager` (self-uplink detection,
  neighbor adoption, withdrawal, deterministic selection, read-only
  `reconcile` against DISCO-001 `NeighborTable`), `self_internet_gateway`
  (build capability from INTERNET-001 cost+caps).
- Reuses existing seams without modifying them: `CapabilityBundle` tags
  (`gateway`, `gateway:internet|lora|satellite`), `TopologyEvent::GatewayChanged`.

## Red-Team Hardening (REDTEAM-01..05)

Adversarial review (redteam subagent) surfaced failure/admission-loop and
boundedness weaknesses; all fixed in this version:

1. **Failure-counter reset on advertisement** — a rogue blackhole that
   re-advertised erased accumulated strikes and stayed selected (re-mission
   race). Fixed: counters never reset on advertisement; `advertise → probation`,
   a grant that expires; repeated probation-exit escalates to **HardFailed**
   after `HARD_FAIL_STRIKES` (=2), from which only `record_success` recovers
   (`redteam_rogue_readmission_escalates_to_hard_failed`,
   `redteam_advertisement_does_not_erase_failure_strikes`).
2. **Unbounded registry** — one-way growth with no withdrawal path. Fixed:
   `MAX_GATEWAY_CANDIDATES` (=64) with oldest-confirmed eviction on overflow;
   `reconcile` is now diff-based and withdraws stale/non-gateway/toppled
   neighbors (`redteam_registry_capped_at_max_candidates`,
   `reconcile_withdraws_stale_and_keeps_healthy`).
3. **NaN/inf float poisoning** — advertised floats could poison scoring or
   topple candidates. Fixed: `sanitize_capability`/`sanitize_f32` clamp
   NaN±inf → fallback at the adoption boundary
   (`redteam_nan_fields_sanitized_and_scored_finite`).
4. **Non-deterministic tie-break** — `partial_cmp` + HashMap iteration order
   made equal-scoring selection order-dependent. Fixed: `total_cmp` on
   quality + PeerId tie-break; candidates scored `< MIN_QUALITY_FLOOR` (=0.01)
   filtered (`redteam_selection_tie_break_is_deterministic`).
5. **Health-state API ambiguity** — `Result<bool, ()>` misleading. Replaced
   with plain `bool` (`record_ack_timeout` returns newly-excluded) and
   `Option<TopologyEvent>` (`adopt_from_neighbor` returns change-or-None).

## Acceptance Criteria Verification

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | Self gateway capability derives from an Internet uplink; `is_gateway()` toggles on/off | `self_uplink_connected_marks_gateway`, `self_internet_gateway_from_transport_cost` | ✅ PASS |
| 2 | Peer gateway detected from DISCO-001 capability tags; registry populated | `reconcile_adopts_gateway_tagged_neighbor` (M7: DISCO handshake bundle → NeighborTable → GW-001 adopt; non-gateway tag ignored; `mark_down` → withdrawn) | ✅ PASS |
| 3 | `serves_priority` enforced per type (LoRa ≤P3, Satellite ≤P2, Internet all) | `serves_priority_follows_type_defaults`, `priority_incompatible_gateway_scores_zero` | ✅ PASS |
| 4 | Deterministic scoring; higher reliability/bandwidth + lower latency/cost wins | `quality_score_ranks_expected_order`, `redteam_selection_tie_break_is_deterministic` | ✅ PASS |
| 5 | Selection matrix: P0 → All, P1–P2 → WithBackup, P3+ → Single, none → None | `selection_matrix_single_backup_all`, `no_candidates_returns_none_and_mesh_fallback`, `priority_incompatible_candidates_skipped` | ✅ PASS |
| 6 | 3 timeouts → failed; success recovers; advertisement → probation 0.5; excluded; repeated failure → HardFailed | `health_thresholds_and_recovery`, `health_probation_expires_to_full_score`, `redteam_rogue_readmission_escalates_to_hard_failed`, `redteam_advertisement_does_not_erase_failure_strikes` | ✅ PASS |
| 7 | Failover: primary failure re-selects backup; recovery restores primary | `failover_picks_backup_after_timeouts` | ✅ PASS |
| 8 | Workspace green + clippy 0 warnings | `cargo test --workspace` 190 tests green; `cargo clippy --workspace --all-targets` 0 warnings | ✅ PASS |

## Test Evidence (2026-08-13)

- 19 gateway tests (13 core: module/selection/health/integration + 6
  red-team regression: escalation, strike-preservation, NaN, tie-break, cap,
  diff-withdraw)
- Workspace: iris-core 174 (incl. 19 gateway) + iris-storage 10 + sim
  integration 5 + M3 1 = 190 green

## Protocol Conformance (PROTO-001)

- Gateway advertisement rides the existing DISCO-001 `CapabilityBundle`
  (payload type 11, canonical CBOR, ≤256 B) via capability tags — no new wire
  type; no DISCO-001/INTERNET-001 module edits.
- Failure/selection events surface on the existing `TopologyEvent` channel.

## Known Limitations (recorded in PROJECT_GRAPH.yaml)

- Reliability/load are modeled inputs; live refinement needs transport
  ACK/probe telemetry (SEC-001/AUTH-001 integration).
- Multi-hop gateway propagation (hop_count > 0, gossip re-announcement,
  `GatewayAnnouncement`) deferred — v1 selects from direct-neighbor
  advertisements.
- HardFailed requires a transport-level ACK (`record_success`) to recover —
  design decision: a twice-recidivist gateway must prove itself via the
  transport path before re-serving traffic.
- LoRa/satellite live radios deferred (BLK-0005); capability structs model
  their metrics for when adapters land.

## Verdict: IMPLEMENTED

All 8 acceptance criteria pass with implementation + red-team regression
evidence (19 gateway tests, 190 workspace green, clippy 0 warnings). GW-001
is the P0 gateway-selection component; the mesh-only fallback
(`GatewaySelection::None`) keeps traffic in the mesh/SCF path — no stall on
gateway loss.