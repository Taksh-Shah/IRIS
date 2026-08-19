# PILOT-001 DISCOVER / UNDERSTAND — Field Pilot Deployment

**Document ID**: IRIS-PILOT-001-DISCOVER-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS, deps ANDROID-001/IOS-001/EMERG-001/SEC-001
COMPLETE + LEGAL-001 RESEARCH_COMPLETE)
**Date**: 2026-08-19
**Iteration**: ~152
**Stage**: DISCOVER → UNDERSTAND COMPLETE (read-only pass; workspace baseline
658/0/1 clippy 0 rustfmt clean **held**)

---

## 1. Node definition (PROJECT_GRAPH line ~923)

- **PILOT-001** — "Field Pilot Deployment", type **OPERATIONS**, priority **P2**,
  status **DISCOVERED**, description "First real-world deployment: NGO pilot,
  campus safety pilot".
- **dependencies**: `["ANDROID-001", "IOS-001", "EMERG-001", "SEC-001",
  "LEGAL-001"]` — ANDROID-001 ✅ (iter 122), IOS-001 ✅ (iter ~151), EMERG-001 ✅
  (iter 71), SEC-001 ✅ (iter 82) all **COMPLETE**; LEGAL-001
  **RESEARCH_COMPLETE** (open legal questions) → node **eligible** (platform +
  security + emergency pillars all ACCEPTED).
- **Milestone anchors**: Beta (Month 8, iOS+Desktop+SEC-001+LORA-001) and Pilot
  (Month 10, "Real-world NGO/campus pilot, 100+ devices", `[PILOT-001]`).
- **C2 gap**: **CONFIRMED** — no `acceptance_criteria` present → **RESOLVE at
  DESIGN** (pattern BLE-002/ANDROID-001/IOS-001: DESIGN defines AC-1..n).

## 2. What already exists (inherited surface)

| Artifact | Status | Relevance to PILOT-001 |
|---|---|---|
| ANDROID-001 (android/ + crates/iris-android + kotlin/) | COMPLETE | Android app shell + Keystore TEE Ed25519 identity + BLE/WA/WD adapters + FGS/WorkManager background + OEM battery-kill matrix. Primary pilot leaf + relay node. |
| IOS-001 (ios/ + crates/iris-ios + ios.yml) | COMPLETE (iter ~151) | Swift app + CoreBluetooth adapters + Keychain identity + SessionRecovery. Secondary pilot leaf (limited relay when backgrounded). |
| BLE-001/BLE-002, WIFIAWARE-001, WIFIDIRECT-001, INTERNET-001, TRANSPORT-001 | COMPLETE | v1 pilot transports: BLE mesh (both carriers), Wi-Fi Aware, Wi-Fi Direct (large-file), Internet relay (gateway uplink). |
| EMERG-001 (emergency/ authority.rs + drill mode) | COMPLETE | SOS/authority model; authority-root provisioning + drill suppression are **pilot prerequisites** (emergency exercise integration). |
| SEC-001 (security/ ACL + quotas + fuzz 44M+) | COMPLETE | Per-class sender allowlist (RED-0002 ACL-1), rate limits, quota/eviction — pilot security posture. |
| OBS-001 + docs/operations/ (OBSERVABILITY.md, DIAGNOSTICS.md, INCIDENT_MANAGEMENT.md, TELEMETRY.md, FIELD_OPERATIONS.md, LOGGING.md) | COMPLETE docs | Metrics taxonomy + delivery/transport/node-health metrics = **pilot KPI instrumentation**; telemetry opt-in + privacy; incident severity/runbook. |
| TEST-001 (nextest/audit/deny/kani/loom/tarpaulin/golden/protocol-conformance) | COMPLETE | Pilot test plan can reuse CI harnesses + fixtures; **BLK-0005 device legs remain RESOURCE-gated**. |
| LEGAL-001 + docs/legal/ (LEGAL_RESEARCH.md, INDIA_COMPLIANCE.md, COMPLIANCE_RISK_REGISTER.md) | RESEARCH_COMPLETE | NDRF/NDMA-authority structuring of govt pilots; WPC de-licensing 865–867 MHz; STQC certification GA Month 30 (pilot may run under MoU); IT Rules 2021 SSMI. |
| docs/business/ (GO_TO_MARKET.md, B2G.md, B2B.md) | docs | **NDRF 12th Battalion (Gandhinagar, Gujarat) 50-device/3-month pilot MoU (Month 6–9)**; 10-device training-center demo; B2B 30-day free pilot (5 dev + 1 gateway); district pilot ~20 gateways ₹20–30 lakh. |
| docs/research/RESEARCH_GAPS.md + GAP-001..005 (PROJECT_STATE) | docs | Ahmedabad = planned pilot city (mobility traces EXP-002/003); GAP-004 LoRa = REQUIRES_HARDWARE → **LoRa excluded from v1 pilot scope**; iOS bg-BLE gap EXP-005. |
| docs/simulation, docs/testing/FAILURE_TESTING.md | docs | Failure-injection + sim corpus → pilot scenario pre-validation. |

## 3. Pilot scope surface (the PILOT-001 design surface)

**Deployment topology (v1 candidate, to be pinned at DESIGN)**
- Leaf nodes: Android (primary relay) + iOS (limited relay) running IRIS
  apps; transports BLE + Wi-Fi Aware + Wi-Fi Direct + Internet-relay uplink.
- Gateway: desktop shell (DESKTOP-001, Tauri hosting iris-core) or Android
  gateway w/ Internet relay to the IRIS relay; telemetry/observability sink.
- Emergency: EMERG-001 authority root provisioning + drill mode + SOS
  verification exercised under NDRF/NDMA structure.
- Distribution: Google Play **closed track** + Apple **TestFlight**; signing
  + Keychain/Keystore provisioning documented as ops runbook.
- Scale: target 100+ devices (milestone), pilot MoU scoped at 50 devices /
  3 months (NDRF), B2B 5-device POC tier.

**Ops-runbook prerequisites (catalog for DESIGN/IMPLEMENT)**
- Device provisioning & identity bootstrap (QR/pairing, TOFU + verified tier,
  per-device Ed25519/X25519 keys, node registry).
- Battery/OEM survival matrix (ANDROID-001 carry), BGTask/restoration on iOS.
- Telemetry opt-in flow (TELEMETRY.md) + OBS-001 metrics → KPI dashboard.
- Incident management (INCIDENT_MANAGEMENT.md severity ladder + escalation).
- Diagnostics bundle + crash reporting (DIAGNOSTICS.md) opt-in.
- STQC/LoRa/WPC notes: pilot can proceed under MoU; LoRa deferred (GAP-004).

**KPI / measurement surface (DRAFT, formalize at DESIGN)**
- P2P + multi-hop delivery ratio, per-class latency, battery drain, coverage
  (Ahmedabad), SOS/emergency drill success, relay throughput (Wi-Fi Aware/Direct).

## 4. Open questions → RESEARCH (iter ~153, RES-0026)

- **RQ-1**: Pilot topology + gateway architecture: node counts, gateway roles,
  Internet-relay access-point provisioning, offline/mesh-primary test design.
- **RQ-2**: Field identity/trust bootstrap at scale: QR pairing flow, authority
  provisioning for EMERG-001, key escrow/recovery, device decommissioning.
- **RQ-3**: Pilot KPI + measurement plan: metric set (OBS-001), collection
  cadence, battery methodology, statistical plan (50–100 devices), before/after
  scenarios (EXP-002/003/005 alignment).
- **RQ-4**: Legal/regulatory structure: NDRF/NDMA authorization mechanism,
  IT Rules 2021 SSMI exposure at pilot scale, WPC de-licensed bands, STQC
  certification timing vs pilot start (LEGAL-001 carry).
- **RQ-5**: App distribution + ops runbook: Play closed track/TestFlight,
  signing, update path, OEM battery-kill guidance, telemetry consent, incident
  runbook ownership.
- **RQ-6**: Emergency exercise integration: EMERG-001 drill mode, authority
  root, SOS rate-limits in a live field exercise, safety plan.

## 5. Known limitations baseline (write to graph at DISCOVER)

- BLK-0005 device gating: physical-device integration (BLE/wireless on real
  hardware) RESOURCE-gated — pilot itself IS the device-gated leg.
- LEGAL-001 open questions: SSMI/relay licensing/type-approval/satellite —
  require lawyer + operator review (RESEARCH_COMPLETE carry).
- Swift/Android build legs env-gated (macOS CI / Android toolchain host).
- No field-trial (Level 5) data yet; LoRa hardware absent (GAP-004) → LoRa
  out of v1 pilot scope; satellite out of scope (SAT-001 DISCOVERED).
- STQC certification planned at GA (Month 30); pilot proceeds under MoU terms.
- Prior critical-path note: "no production deployment with DevCryptoProvider /
  no identity / no sealer" (DEC-0010 security control) is a **pilot gate** —
  pilot devices MUST run provisioned identities + AEAD (both platform nodes
  ship this).

## 6. Stage outcome

- **UNDERSTAND COMPLETE** — reference surface read (platform nodes, transports,
  EMERG/SEC, OBS/ops docs, legal research, business pilot plans, research gaps);
  C2 gap CONFIRMED (DESIGN resolves); PILOT-001 is the first OPERATIONS node and
  the deployment-critical path terminus for MVP/Alpha/Beta/Pilot milestones.
- **NEXT**: RESEARCH (iter ~153, RES-0026: RQ-1..RQ-6 — deployment topology +
  field provisioning + KPI methodology + legal structure + distribution/runbook
  + emergency exercise) → DESIGN (iter ~154: `docs/implementation/PILOT_001_DESIGN.md`,
  AC-1..n resolves C2, DEC-PILOT-xxxx) → IMPLEMENT → TEST → SECURITY_REVIEW →
  VERIFY → ACCEPT → LORA-001/SAT-001 (P2 hardware) → MISSION.
- Workspace **untouched** (read-only pass) — baseline 658/0/1 held.
