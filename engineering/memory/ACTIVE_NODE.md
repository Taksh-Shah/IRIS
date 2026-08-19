# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:15:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **DESIGNING** — DISCOVER (iter ~152) → RESEARCH (iter ~153,
  RES-0026, PROCEED) → DESIGN (iter ~154, `PILOT_001_DESIGN.md`, C2 gap
  RESOLVED) → **IMPLEMENT next (iter ~155, docs-only)** → TEST →
  SECURITY_REVIEW → VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **C2 gap**: **RESOLVED** at DESIGN — AC-1..AC-18 defined (testable; device
  rows BLK-0005/env-gated recorded honestly)

## DESIGN COMPLETE (iter ~154, evidence 3) — PILOT_001_DESIGN.md v1.0

`docs/implementation/PILOT_001_DESIGN.md` authored (pattern
BLE_002_DESIGN.md/ANDROID_DESIGN.md/IOS_DESIGN.md) absorbing RES-0026 D-1..D-9:
- **D-1 Topology**: leaves 50-100 (Android relay-primary; iOS limited-relay) +
  1-3 DESKTOP-001 (Tauri) gateways + Android-gateway field fallback;
  gateway-scoped uplink/downlink to private IRIS relay endpoint; mesh-primary
  offline; LoRa excluded (GAP-004).
- **D-2 Relay-cadence rule**: broadcast/telemetry cadence scales with online
  node count (0.6×≤10 … 1.0×31-40, >40 linear `1.0+(N-40)×f`, default f=0.075
  configurable 0.01-0.1) at INTERNET-001/ROUTE-001 seam; P0/P1 + directed exempt.
- **D-3 Test battery B-1..B-4**: NCT-of-N (all-N delivery + latency CDF
  N=20/50/100), partition/healing (uplink down), multi-hop SCF mule-chain,
  iOS limited-relay; SIM-001 pre-validation + pilot field legs; BLK-0005 folded
  into the pilot.
- **D-4 ProvisioningFlow**: first-run identity (Keystore/Keychain) → operator
  mutual-QR ceremony (batch 10-20) → gateway-verified escalation → EMERG-001
  authority-root + DRILL-chain out-of-band bundle → decommission (wipe +
  revoke + rotate); no-recovery documented.
- **D-6 KPI plan**: OBS-001 mapping + thresholds target/floor + **Wilson-CI
  95%** plan (per class/transport, never pooled) + controlled fg/bg battery.
- **D-7 Exercise**: NDMA DMEx M1 TTEx → M2 field ME (observers + self-
  assessment, TEST-only SOS under DRILL certs) → M3 evaluation/AAR to NDRF.
- **D-8 Distribution**: Play **internal** track (≤100 testers) + closed 12×14
  production clock at pilot close; TestFlight external + 90-day refresh;
  signing 2-person + PEPK; OEM battery-kill; telemetry consent.
- **D-9 REG-NOTES**: WPC G.S.R. 853(E) 2021 (865-868) supersedes 865-867;
  SSMI non-applicable; NDRF MoU + STQC GA; open counsel Qs; **no legal opinion**.
- **DEC-PILOT-0001..0009** ratified in `engineering/memory/DECISIONS.md`.
- Risks G-P1..G-P9 → design positions G1..G12; external-facts doc edits staged
  (AC-16).

## Pipeline position

- **PILOT-001 DESIGN COMPLETE (iter ~154)** — 28 COMPLETE nodes. NEXT:
  **IMPLEMENT (iter ~155, docs-only ops deliverables)**.
- Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held (docs-only pass).

## IMPLEMENT scope (iter ~155)

1. `docs/operations/PILOT_RUNBOOK.md` (AC-2): topology, gateway roles/fallback,
   ProvisioningFlow ceremony (batch 10-20, mutual-QR, verified escalation,
   authority-root/DRILL bundle), decommission (wipe+revoke+rotate), day-0/day-N
   ops, incident ownership, diagnostics QR.
2. `docs/operations/PILOT_KPI_PLAN.md` (AC-3/AC-11): OBS-001 mapping,
   thresholds target/floor, Wilson-CI formula + sampling, battery methodology,
   collection cadence + 7-day retention.
3. `docs/operations/PILOT_EXERCISE.md` (AC-4/AC-13): NDMA DMEx M1/M2/M3,
   observers + self-assessment, DRILL certs + TEST-only SOS + yellow banner,
   safety annex.
4. `docs/operations/PILOT_DISTRIBUTION.md` (AC-5/AC-15): Play internal + closed
   12×14, TestFlight 90-day refresh, signing custody, OEM battery-kill,
   telemetry consent, update path.
5. `docs/operations/REG_NOTES.md` (AC-14): WPC 853(E)/2021, SSMI memo, NDRF MoU
   + STQC GA, open counsel Qs.
6. Relay-cadence config seam edit at INTERNET-001/ROUTE-001 seam (AC-6).
7. EXTERNAL-FACTS doc edits (AC-16): GO_TO_MARKET/B2G closed→internal;
   COMPLIANCE_RISK_REGISTER/SPECTRUM 865-867→853(E) 865-868; FIELD_OPS 866.0
   ≤25 mW CONFIRMED.
→ THEN TEST (iter ~156: SIM-001 pre-validation of B-1..B-4) → SECURITY_REVIEW →
VERIFY → ACCEPT.

## Carry-forward context

- TEST-001 infra: nextest + audit/deny + kani/loom + tarpaulin + golden
  vectors + protocol conformance (CI-leg matrix on runners); SIM-001 harness +
  failure-injection corpus for B-1..B-4 pre-validation.
- DEC-0010 pilot gate: pilot devices MUST run provisioned identities + AEAD
  (ANDROID-001 KI TEE / IOS-001 Keychain — satisfied).
- Physical-device rows (live ME, on-air BLE/battery) BLK-0005-gated — the pilot
  itself is the device-gated leg.
- External facts for DOCUMENT/DISTRIBUTION: Play **internal** track; WPC
  **853(E) 2021 865-868**; FIELD_OPS 866.0 MHz ≤25 mW CONFIRMED.