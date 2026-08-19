# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T12:30:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **DISCOVERED** — DISCOVER COMPLETE (iter ~152); **RESEARCH next
  (iter ~153, RES-0026)** → DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW →
  VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **Eligible**: all platform + security + emergency deps COMPLETE
- **C2 gap**: **CONFIRMED** at DISCOVER (no ACs) — DESIGN defines AC-1..n

## DISCOVER COMPLETE (iter ~152, evidence 1)

`engineering/memory/records/PILOT-001_DISCOVER.md` authored (pattern
IOS-001_DISCOVER.md):
- **Scope cataloged**: 100+ device NGO/campus pilot (NDRF 12th Bn Gandhinagar
  50-device/3-month MoU + B2B POC tier); topology candidate = Android (primary
  relay) + iOS (limited relay) leaves + gateway (DESKTOP-001 or Android) +
  Internet-relay uplink; transports BLE + Wi-Fi Aware + Wi-Fi Direct.
- **Ops prerequisites**: Play closed track + TestFlight distribution, signing,
  Keychain/Keystore provisioning, EMERG-001 authority + drill mode, OBS-001
  KPIs, OEM battery-kill matrix, incident runbook, telemetry opt-in.
- **RQ-1..RQ-6 staged** for RESEARCH (RES-0026): topology/gateway, field
  identity bootstrap, KPI methodology, legal structure, distribution/runbook,
  emergency exercise.
- **Known limitations** written to graph: BLK-0005 device gating, LEGAL-001
  carry, env-gated build legs, no Level-5 field data, LoRa excluded (GAP-004),
  DEC-0010 pilot gate (no DevCryptoProvider/no-identity deployments), STQC GA
  timing (pilot runs under MoU).

## Pipeline position

- **IOS-001 ACCEPTED (iter ~151)** — commit `d13845c` + `cd995ef`; 28 COMPLETE
  nodes; PROJECT_STATE completed 27 → **28**, implementing 1 → 0; CHANGELOG
  **0.3.58**.
- Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held.

## RESEARCH scope (iter ~153, RES-0026) — read-only

- Resolve RQ-1..RQ-6 with best-in-class 2024-2026 sources (disaster-ops /
  field-trial methodology, NDRF/NDMA guidance, app-store closed-track ops,
  emergency-exercise design), evidence-leveled L1-L5, no AI citations.
- Produce DESIGN inputs + verdict per RQ; register ALLOCATION (next RES-0027).
- Write `engineering/memory/records/research/RES-0026.md` (pattern RES-0025).

## Carry-forward context (from IOS-001 / ANDROID-001 / TEST-001 / platforms)

- Swift compile/run env-gated → ios.yml macOS CI leg; Android toolchain host;
  physical-device rows BLK-0005 (the pilot is the device-gated leg).
- iOS = limited relay when backgrounded; Android primary relay; EMERG-001
  authority model + SOS rate limits; SEC-001 ACL/quota posture; DEC-0010
  identity/AEAD gate mandatory for all pilot devices.
- TEST-001 infra: nextest + audit/deny + kani/loom + tarpaulin + golden
  vectors + protocol conformance (CI-leg matrix on runners).
