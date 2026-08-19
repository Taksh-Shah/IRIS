# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:00:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — DESIGN (iter ~154)

**Pipeline POSITION: PILOT-001 RESEARCH COMPLETE (iter ~153, evidence 2).**
RES-0026.md authored — verdict **PROCEED**: RQ-1..RQ-6 resolved
(ADOPT/ADOPT-WITH-CONDITION); **9 DESIGN inputs D-1..D-9**; external-facts
reconciled (Play closed → **internal** track; WPC **853(E) 2021 865-868**
supersedes 865-867). PROJECT_GRAPH PILOT-001 status RESEARCH_COMPLETE,
evidence(2), stage_note RESEARCH COMPLETE; PROJECT_STATE research_complete
1→2, discovered 3→2. C2 gap still open (no ACs) — **DESIGN resolves**.
Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
fmt clean.

## Next Action (iter ~154 — PILOT-001 DESIGN, `docs/implementation/PILOT_001_DESIGN.md`)

1. **DESIGN** (docs-only pass, pattern BLE_002_DESIGN.md / ANDROID_DESIGN.md /
   IOS_DESIGN.md) per RES-0026 verdict + D-1..D-9:
   - **D-1 Topology**: leaves 50-100 (Android relay-primary; iOS
     limited-relay) + 1-3 DESKTOP-001 (Tauri) gateways + Android-gateway field
     fallback; private IRIS relay endpoint; gateway-scoped uplink/downlink;
     LoRa excluded (GAP-004); mesh-primary offline operation.
   - **D-2 Relay-cadence rule**: broadcast/telemetry cadence scales with
     online-node count (≤40 flat; >40 linear — Meshtastic
     `congestionScalingCoefficient` equivalent) at INTERNET-001/ROUTE-001 seam.
   - **D-3 Test battery**: NCT-of-N relay capacity (all-N delivery + latency
     CDF), partition/healing (uplink down), multi-hop SCF mule-chain with
     contact-window expectation; pre-validated in SIM-001 harness; BLK-0005
     device legs folded into the pilot.
   - **D-4 ProvisioningFlow**: first-run identity (Keystore/Keychain) → operator
     `Add device` mutual-QR ceremony (batch 10-20) → gateway-verified escalation
     → EMERG-001 authority-root + DRILL-chain out-of-band bundle →
     decommission (wipe + revoke + rotate); no escrow/no-recovery documented.
   - **D-5 Trust model**: TOFU + verified tier (RES-0016 D-2); platform-store
     key isolation; Briar/SecureJoin QR precedent.
   - **D-6 KPI plan**: delivery ratio (per class/transport), p50/p95/p99 hops,
     battery (controlled methodology), coverage, drill metrics — thresholds
     target/floor + **Wilson-CI sample plan** as AC-1..n.
   - **D-7 Exercise schedule**: Month 1 TTEx → Month 2 field Mock Exercise →
     Month 3 evaluation (NDMA DMEx); EXP-002/003/005 before/after legs.
   - **D-8 Distribution + ops runbook**: Play **internal** track (≤100 testers),
     closed-track 12×14 later, TestFlight external + 90-day refresh, Play App
     Signing custody (2-person + PEPK), OEM battery-kill checklist, telemetry
     consent + bounded retention, incident severity ownership.
   - **D-9 REG-NOTES**: WPC 853(E)/2021 codification (≤25 mW e.r.p. 866.0 MHz,
     LoRa future-leg only), SSMI non-applicability memo, NDRF MoU + STQC GA,
     open counsel questions (no legal opinion).
2. **AC-1..n** resolve the C2 gap (testable; device rows BLK-0005/env-gated
   recorded honestly); **DEC-PILOT-xxxx** ratified into DECISIONS.md; risks
   G-P1..G-P9 → known_limitations.
3. **PROJECT_GRAPH PILOT-001**: status RESEARCH_COMPLETE → DESIGNING +
   acceptance_criteria + evidence(3) + stage_note DESIGN COMPLETE; meta
   validation_status refreshed. **PROJECT_STATE** designing 0→1,
   research_complete 2→1.
4. **Context refresh**: ACTIVE_NODE / CURRENT_STATE / NEXT_ACTION → point at
   PILOT-001 IMPLEMENT (iter ~155).

→ **After DESIGN, PIPELINE STATE = PILOT-001 → IMPLEMENT → TEST →
SECURITY_REVIEW → VERIFY → ACCEPT** → then LORA-001/SAT-001 (P2 hardware) →
mission milestones. Baseline **658/0/1** clippy 0 fmt clean.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151). Pipeline: **PILOT-001** (first
OPERATIONS node). Env-gates: Swift/device rows ios.yml-macOS-CI + BLK-0005;
Android toolchain host; LEGAL-001 open questions are a PILOT-001
RESEARCH/DESIGN legal carry (NDRF/NDMA structuring; WPC 853(E)/2021).

## Recent state files touched

- `engineering/memory/records/research/RES-0026.md` (iter ~153)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 status RESEARCH_COMPLETE,
  evidence(2), stage_note RESEARCH COMPLETE; meta validation_status)
- `engineering/PROJECT_STATE.yaml` (research_complete 1→2, discovered 3→2,
  next_recommended refreshed)
- `engineering/memory/execution-state.yaml` (stage → DESIGN)
- `engineering/memory/records/execution-log.md` (iter ~153)