# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T12:00:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **DISCOVERED** — DISCOVER next (iter ~152); then RESEARCH →
  DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot)
- **Eligible**: all core + platform deps COMPLETE; IOS-001 ACCEPTED iter ~151

## Pipeline position

- **IOS-001 ACCEPTED (iter ~151)** — commit `d13845c`: SECURITY_REVIEW fixes +
  IOS-001_SECURITY_REVIEW.md + IOS-001_VERIFICATION.md + state (16 files,
  +700/-478). PROJECT_GRAPH IOS-001 → **COMPLETE** evidence(9); PROJECT_STATE
  completed 27 → **28**, implementing 1 → 0; CHANGELOG **0.3.58**.
- Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held.
- **NODE_TRANSITION → PILOT-001 (DISCOVER, iter ~152).**

## DISCOVER scope (iter ~152) — read-only

- Catalog pilot scope (NGO pilot + campus safety pilot, 100+ devices) and
  deployment topology (Android + iOS leaf nodes; gateway; Internet-relay
  fallback; LoRa/SAT outlook).
- Ops-runbook prerequisites: app signing (Apple TestFlight/App Store + Google
  Play closed track), Keychain/Keystore provisioning, EMERG-001 drill
  suppression, observability (OBS-001) rollout.
- **C2 gap check**: confirm PILOT-001 has no acceptance criteria (expect DESIGN
  to define AC-1..n).
- BLK-0005 device gates + LEGAL-001 carry (IT Rules 2021 SSMI / relay licensing /
  LoRa type approval / satellite licensing — operator + lawyer review flags).
- Write `engineering/memory/records/PILOT-001_DISCOVER.md` (pattern
  IOS-001_DISCOVER.md).

## Carry-forward context (from IOS-001 / TEST-001 / platforms)

- Swift compile/run env-gated → ios.yml macOS CI leg (macos-15 + Xcode 16.4,
  iOS 18 SDK floor); physical-device rows BLK-0005.
- iOS = limited relay node when backgrounded; Android primary relay; EMERG-001
  authority model + SOS rate limits; SEC-001 ACL/quota posture.
- TEST-001 infra: nextest + audit/deny + kani/loom + tarpaulin + golden
  vectors + protocol conformance (CI-leg matrix on runners).
