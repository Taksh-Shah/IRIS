# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T14:10:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **IMPLEMENTING** — DISCOVER (iter ~152) → RESEARCH (iter ~153,
  RES-0026, PROCEED) → DESIGN (iter ~154, `PILOT_001_DESIGN.md`, C2 gap
  RESOLVED) → IMPLEMENT COMPLETE (iter ~155, docs-only ops deliverables) →
  TEST COMPLETE (iter ~156, SIM-001 sim pre-validation of B-1..B-4) →
  **SECURITY_REVIEW COMPLETE (iter ~157, AC-18)** → **VERIFY next (iter ~158)**
  → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **C2 gap**: **RESOLVED** at DESIGN — AC-1..AC-18 defined; ops docs authored
  at IMPLEMENT (AC-1..AC-16); TEST evidence complete (AC-7..AC-10, AC-17);
  SECURITY_REVIEW evidence complete (AC-18); VERIFY evidence pending

## SECURITY_REVIEW COMPLETE (iter ~157, AC-18) — PASS-with-fix

`engineering/memory/records/PILOT-001_SECURITY_REVIEW.md` (pattern
BLE_002_SECURITY_REVIEW.md) — adversarial review of the ops layer: **2 FIXED +
12 RECORDED + 3 INFO, no CRITICAL/HIGH**:

- **FIXED**: **P-007** — DRILL-CA private key held under the 2-person signing
  custody rule (never on a field leaf), weekly authority-root fingerprint +
  DRILL-CA health re-verification, re-bundle before each exercise leg
  (`PILOT_RUNBOOK.md` §2.4/§5); **P-013** — per-device timestamped opt-in
  consent trail captured at provisioning (runbook §4 day-0 + `PILOT_KPI_PLAN.md`
  §4).
- **RECORDED**: P-001 SAS entropy → BQP spec; P-002 hardware attestation
  deferred; P-005 revocation-sync check in weekly drift review; P-006
  decommission order revoke→retire→wipe; P-010 drill-SOS cap/device/leg; P-011
  relay_cadence clamp; P-014 telemetry salt rotation; P-015 sideload contained
  by DEC-0010 gate; P-016 TestFlight validity pre-drill; P-017 diagnostics-QR
  confined.
- **Positive controls verified in real code**: EMERG-001 `drill.rs` (`SosKind::Test`,
  `drill_flag_suppresses`, `verified_drill_suppressed`, `drill_mismatch_rejected`);
  SEC-001 EmergencyAcl drill cap-exempt (drills never consume real-SOS quota);
  IDENT-001 rotate/trust_store (earliest-seen-wins + null-rotation revoke);
  DEC-0010 gate.
- **AC-17 held**: workspace **664/0/1**, clippy 0, fmt clean (no Rust changes).

## Pipeline position

- **PILOT-001 SECURITY_REVIEW COMPLETE (iter ~157, evidence 6)** — 28 COMPLETE
  nodes. NEXT: **VERIFY (iter ~158, AC-17/AC-18)**.

## VERIFY scope (iter ~158)

1. `PILOT-001_VERIFICATION.md` independent-verifier evidence table (pattern
   BLE_002_VERIFICATION.md): AC-1..AC-18 rows; live re-run `cargo test
   --workspace --all-features` (baseline 664/0/1) + clippy + fmt; confirm every
   P-00x disposition + both FIXED doc edits re-located in source.
2. → ACCEPT (iter ~159) + NODE_TRANSITION LORA-001/SAT-001.