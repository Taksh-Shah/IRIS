# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:00:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **RESEARCH_COMPLETE** — DISCOVER (iter ~152) → RESEARCH (iter
  ~153, RES-0026, PROCEED) → **DESIGN next (iter ~154)** → IMPLEMENT → TEST →
  SECURITY_REVIEW → VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **Eligible**: all platform + security + emergency deps COMPLETE
- **C2 gap**: **CONFIRMED** at DISCOVER (no ACs) — DESIGN defines AC-1..n

## RESEARCH COMPLETE (iter ~153, evidence 2) — RES-0026 verdict PROCEED

`engineering/memory/records/research/RES-0026.md` authored (pattern RES-0025,
157 lines; 12 websearch + 4 primary-source passes, L1-L5, no AI citations):
- **RQ-1 ADOPT-WITH-CONDITION**: managed-flood >100-node precedent (Meshtastic
  congestionScalingCoefficient cadence rule at firmware source) + Helene 2024
  gateway-uplink field precedent + Nepal DTN trial metrics → leaves 50-100
  (Android relay-primary, iOS limited-relay) + 1-3 DESKTOP-001 gateways +
  Android-gateway fallback + gateway-scoped Internet-relay; LoRa excluded.
- **RQ-2 ADOPT-WITH-CONDITION**: Briar/SecureJoin QR mutual-scan bootstrap +
  IDENT-001 verified tier + EMERG-001 out-of-band authority provisioning; batch
  ceremony 10-20, no-recovery documented, decommission wipe/revoke/rotate.
- **RQ-3 ADOPT-WITH-CONDITION**: OBS-001 ↔ NDMA DMEx Guidelines (Oct 2024)
  4-phase mapping incl. evaluation/debrief; thresholds + Wilson-CI sample plan
  → DESIGN ACs; EXP-002/003/005 before/after legs.
- **RQ-4 ADOPT** (codification only): WPC G.S.R. 853(E) 2021 = 865-868 MHz SRD
  Table-I (25 mW e.r.p., ≤1% duty) **supersedes 865-867** LEGAL-001 carry;
  SSMI 50-lakh non-applicable; NDRF MoU authorization + STQC GA Month 30; open
  counsel Qs recorded; **no legal opinion**.
- **RQ-5 ADOPT-WITH-CONDITION**: Play **internal** track ≤100 testers primary
  (DISCOVER 'closed track' REFINED); closed-track 12×14 for later production;
  TestFlight external ≤10k + **90-day refresh**; Play App Signing custody.
- **RQ-6 ADOPT**: EMERG-001 drill mode = NDMA Mock Exercise standard;
  DRILL-chain + TEST-only SOS + observers/debrief + safety annex.
- **9 DESIGN inputs D-1..D-9** + external-facts reconciliation + gaps G-P1..G-P9.
- ALLOCATION next RES-0027; PROJECT_GRAPH status RESEARCH_COMPLETE,
  evidence(2); PROJECT_STATE research_complete 1→2, discovered 3→2.

## Pipeline position

- **PILOT-001 RESEARCH COMPLETE (iter ~153, RES-0026 PROCEED)** — 28 COMPLETE
  nodes. NEXT: **DESIGN (iter ~154, `docs/implementation/PILOT_001_DESIGN.md`)**.
- Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held (read-only pass).

## DESIGN scope (iter ~154, PILOT_001_DESIGN.md)

- Architect authors design per RES-0026 verdict + D-1..D-9 (pattern
  BLE_002_DESIGN.md / ANDROID_DESIGN.md / IOS_DESIGN.md), **docs-only pass**.
- Resolve C2 gap: **AC-1..n** (testable; real-deployment device rows
  BLK-0005-gated recorded honestly) + **DEC-PILOT-xxxx** ratified into
  DECISIONS.md.
- Sections: topology; relay-cadence scaling rule (INTERNET-001/ROUTE-001 seam);
  fixed test battery (NCT-of-N, partition/healing, multi-hop SCF mule);
  ProvisioningFlow; KPI plan (Wilson-CI sample plan); NDMA DMEx exercise
  schedule (Month 1 TTEx → Month 2 field ME → Month 3 evaluation);
  distribution + ops runbook (Play internal, TestFlight 90-day, signing
  custody); REG-NOTES codification. Gaps G-P1..G-P9 → known_limitations.

## Carry-forward context (from IOS-001 / ANDROID-001 / TEST-001 / platforms)

- Swift compile/run env-gated → ios.yml macOS CI leg; Android toolchain host;
  physical-device rows BLK-0005 (the pilot is the device-gated leg).
- iOS = limited relay when backgrounded; Android primary relay; EMERG-001
  authority model + SOS rate limits; SEC-001 ACL/quota posture; DEC-0010
  identity/AEAD gate mandatory for all pilot devices.
- TEST-001 infra: nextest + audit/deny + kani/loom + tarpaulin + golden
  vectors + protocol conformance (CI-leg matrix on runners).
- External-facts for DESIGN/DOCUMENT: Play closed → **internal** track; WPC
  **865-867 → G.S.R. 853(E) 2021 865-868**; FIELD_OPS 866.0 MHz ≤25 mW confirmed.
