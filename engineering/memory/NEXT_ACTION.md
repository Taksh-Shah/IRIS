# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:30:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — TEST (iter ~156, SIM-001 B-1..B-4 pre-validation)

**Pipeline POSITION: PILOT-001 IMPLEMENT COMPLETE (iter ~155, evidence 4).**
Ops deliverables per PILOT_001_DESIGN.md AC-1..AC-16 authored (docs-only):
PILOT_RUNBOOK.md (AC-2), PILOT_KPI_PLAN.md (AC-3/AC-11), PILOT_EXERCISE.md
(AC-4/AC-13), PILOT_DISTRIBUTION.md (AC-5/AC-15), REG_NOTES.md (AC-14) +
relay-cadence seam in CONGESTION_CONTROL.md (AC-6) + EXTERNAL-FACTS doc edits
(AC-16). PROJECT_GRAPH PILOT-001 status DESIGNING→IMPLEMENTING + evidence(4);
PROJECT_STATE designing 1→0 implementing 0→1. Baseline **658/0/1** clippy 0
fmt clean held.

## Next Action (iter ~156 — PILOT-001 TEST, SIM-001 pre-validation of B-1..B-4)

1. **B-1** NCT-of-N relay capacity (AC-7): sim harness in crates/iris-core —
   N=20/50/100 nodes, all-N delivery + latency CDF, compare vs
   PILOT_KPI_PLAN.md Table-1 thresholds (target/floor, Wilson-CI 95% lower
   bound per class/transport).
2. **B-2** Partition/healing (AC-8): uplink-down for T minutes; mesh P1/P2
   floor holds; heal produces no duplicates.
3. **B-3** Multi-hop SCF mule-chain (AC-9): multi-hop delivery + hop CDF within
   contact window.
4. **B-4** iOS limited-relay (AC-10): participate on foreground/restore;
   asymmetry recorded known_limitation (env/BLK-0005-gated honest).
5. **PILOT-001_TEST.md** evidence map (pattern BLE-002_TEST.md) mapping AC-7..
   AC-10 to sim findings; live workspace re-run `cargo test --workspace
   --all-features` + clippy + fmt (AC-17 evidence).
6. **Persist iter ~156** in all durable artifacts (graph evidence(5),
   PROJECT_STATE, execution-state, ACTIVE_NODE/NEXT_ACTION/CURRENT_STATE,
   ALLOCATION, execution-log.md) + commit.
7. → SECURITY_REVIEW (iter ~157, AC-18: adversarial review of provisioning/
   decommission/authority-root/TEST-only-SOS/DRILL-cert/relay-cadence) →
   VERIFY (AC-17/18 independent evidence table) → ACCEPT.

→ **After TEST, PIPELINE STATE = PILOT-001 → SECURITY_REVIEW → VERIFY →
ACCEPT** → then NODE_TRANSITION to LORA-001/SAT-001 (P2 hardware) → mission
milestones. Baseline **658/0/1** clippy 0 fmt clean.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151). Pipeline: **PILOT-001** (first
OPERATIONS node). DESIGN COMPLETE iter ~154 (C2 RESOLVED AC-1..AC-18);
IMPLEMENT COMPLETE iter ~155 (ops docs AC-1..AC-16). Env-gates: Swift/device
rows ios.yml-macOS-CI + BLK-0005; Android toolchain host; LEGAL-001 open
questions are a PILOT-001 legal carry (NDRF/NDMA structuring; WPC 853(E)/2021;
SSMI; DPDPA). Sim legs run on the crates/iris-core SIM-001 harness (no
hardware required).

## Recent state files touched

- `docs/operations/PILOT_RUNBOOK.md` / `PILOT_KPI_PLAN.md` / `PILOT_EXERCISE.md`
  / `PILOT_DISTRIBUTION.md` / `REG_NOTES.md` (iter ~155)
- `docs/routing/CONGESTION_CONTROL.md` (AC-6 relay-cadence seam)
- `docs/business/GO_TO_MARKET.md` + `docs/legal/COMPLIANCE_RISK_REGISTER.md` +
  `docs/legal/SPECTRUM_CONSIDERATIONS.md` + `docs/operations/FIELD_OPERATIONS.md`
  (AC-16)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 IMPLEMENTING, AC(18), evidence(4))
- `engineering/PROJECT_STATE.yaml` (designing 1→0, implementing 0→1)
- `engineering/memory/execution-state.yaml` (stage → TEST)
- `engineering/memory/records/execution-log.md` (iter ~155)