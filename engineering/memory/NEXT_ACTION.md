# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T14:10:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — VERIFY (iter ~158, AC-17/AC-18)

**Pipeline POSITION: PILOT-001 SECURITY_REVIEW COMPLETE (iter ~157, evidence 6).**
Adversarial review of the PILOT-001 ops layer per AC-18 landed in
`engineering/memory/records/PILOT-001_SECURITY_REVIEW.md` (pattern
BLE_002_SECURITY_REVIEW.md) — verdict **PASS-with-fix**: **2 FIXED + 12
RECORDED + 3 INFO, no CRITICAL/HIGH**. FIXED (docs): P-007 (`PILOT_RUNBOOK.md`
§2.4/§5 — DRILL-CA private key 2-person custody, never on a field leaf; weekly
authority-root/DRILL-CA re-verification; re-bundle before each exercise leg),
P-013 (per-device timestamped opt-in consent trail at provisioning — runbook §4
day-0 + `PILOT_KPI_PLAN.md` §4). Positive controls verified in real code
(EMERG-001 `drill.rs`, SEC-001 EmergencyAcl drill cap-exempt, IDENT-001
rotate/trust_store, DEC-0010 gate). AC-17 held: workspace **664/0/1** (was
658/0/1), clippy 0, fmt clean — no Rust changes this pass.

## Next Action (iter ~158 — PILOT-001 VERIFY, AC-17/AC-18)

1. **`PILOT-001_VERIFICATION.md`** independent-verifier evidence table (pattern
   BLE_002_VERIFICATION.md): AC-1..AC-18 rows with evidence + disposition;
   live re-run `cargo test --workspace --all-features` (baseline **664/0/1**) +
   clippy + fmt; confirm every P-00x disposition in
   `PILOT-001_SECURITY_REVIEW.md` (+ both FIXED doc edits) re-located in source.
2. **ACCEPT (iter ~159)**: PROJECT_GRAPH PILOT-001 → COMPLETE, PROJECT_STATE
   completed 28 → **29**, CHANGELOG, **NODE_TRANSITION → LORA-001/SAT-001**
   (P2 hardware) → mission milestones.
3. **Persist each iteration** in all durable artifacts + commit.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151). Pipeline: **PILOT-001** (first
OPERATIONS node). DESIGN COMPLETE iter ~154 (C2 RESOLVED AC-1..AC-18);
IMPLEMENT COMPLETE iter ~155 (ops docs AC-1..AC-16); TEST COMPLETE iter ~156
(SIM-001 B-1..B-4 pre-validation, AC-7..AC-10 + AC-17); SECURITY_REVIEW COMPLETE
iter ~157 (AC-18, 2 FIXED + 12 RECORDED + 3 INFO). Env-gates: Swift/device
rows ios.yml-macOS-CI + BLK-0005; Android toolchain host; LEGAL-001 open
questions are a PILOT-001 legal carry (NDRF/NDMA structuring; WPC 853(E)/2021;
SSMI; DPDPA). Sim legs run on the crates/iris-core SIM-001 harness (no
hardware required).

## Recent state files touched

- `engineering/memory/records/PILOT-001_SECURITY_REVIEW.md` (iter ~157, AC-18)
- `docs/operations/PILOT_RUNBOOK.md` (P-007 DRILL-CA custody + weekly
  re-verification; P-013 consent trail; §4/§5)
- `docs/operations/PILOT_KPI_PLAN.md` (P-013 per-device consent record)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 IMPLEMENTING, AC(18), evidence(6),
  stage_note SECURITY_REVIEW COMPLETE)
- `engineering/PROJECT_STATE.yaml` (implementing 1 held; health/testing_health +
  next_recommended refreshed)
- `engineering/memory/execution-state.yaml` (stage → SECURITY_REVIEW → VERIFY next)
- `engineering/memory/ACTIVE_NODE.md`, `CURRENT_STATE.md`,
  `engineering/memory/records/execution-log.md` (iter ~157)