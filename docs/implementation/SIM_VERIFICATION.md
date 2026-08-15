# SIM-001 Verification Report

**Document ID**: IRIS-SIM-VER-001
**Version**: 1.0
**Node**: SIM-001 (WP-6)
**Date**: 2026-08-13

---

## Implementation

`crates/iris-core/src/sim/` — discrete-event simulation

- **`mod.rs`** — `Simulation`: seeded `ChaCha8Rng` + shared atomic virtual
  clock (`VIRTUAL_EPOCH_SECS`); contact schedule + injection timeline executed
  in sorted order; `forward_to` reuses **real ROUTE-001 logic**
  (`max_hops_for_priority` hop budgets, `ForwardedCache` dedup/anti-loop) and
  **real SCF-001 logic** (`ScfEngine::buffer_message`, `mark_forwarded`)
  over `MemoryStorage`. `SimOutcome` with `loop_free()` no-loop evidence and
  `result_anchor` (seed-deliver-inject) for determinism.
- **`scenario.rs`** — scenario builders: `dense_mesh`, `partition_carry`,
  `vehicle_relay`; helpers `inject_standard`, `inject_sos`, `loss`,
  `sim_peer`.
- **`metrics.rs`** — `SimMetrics`: delivery ratio per priority, p50/p95
  latency percentiles, peak storage bytes, evictions, max hops, loop-free
  verdict.

## Acceptance Criteria Verification (ORCH-0001 WP-6)

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | SIM reproducible: same seed → same result anchor | `same_seed_same_result_anchor`; integration `determinism_same_seed_same_anchor` (3 scenario builders) | ✅ PASS |
| 2 | ≥3 reproducible scenarios with delivery ratio + no-loop evidence | `dense_mesh` (ratio >0.9, loop-free), `partition_carry` (SOS 1.0, multi-hop), `vehicle_relay` (both delivered on the relay line) | ✅ PASS |
| 3 | Partition→carry→contact→deliver (SCF acceptance) | `partition_carry_delivers_with_sos` P0 ratio 1.0 + `scenario_2_sparse_partition_carry` | ✅ PASS |
| 4 | No-loop evidence | `loop_free()` assertion on every scenario (delivered exactly once across network) | ✅ PASS |
| 5 | Loss degrades delivery (monotonic sanity) | `loss_reduces_delivery_ratio_monotonically` (60% loss ≤ 0% loss) | ✅ PASS |
| 6 | No new clippy warnings; workspace tests green | `cargo test --workspace` 171 green; clippy 0 warnings | ✅ PASS |

## Test Evidence (2026-08-13)

- 13 SIM tests: mod 3, scenario 3, metrics 2, integration `sim_scenarios` 5
- Workspace: iris-core 155 + sim integration 5 + iris-storage 10 + M3 1 =
  171 green

## SIM-Real World Gap (recorded in PROJECT_GRAPH.yaml)

- SimulatedTransport / contact-schedule model validated only in silico;
  physical calibration (BLE/Wi-Fi Aware) deferred to BLK-0005.
- Latency histogram is measured in virtual milliseconds; wall-clock and
  protocol overheads are not modeled.
- Contact schedules are enumerative (fixed times) — probabilistic mobility
  models are a follow-up.

## Verdict: IMPLEMENTED

All 6 acceptance criteria pass. SIM-001 provides the scenario evidence that
completes SCF-001 ACCEPT (delivery ratio across partition).