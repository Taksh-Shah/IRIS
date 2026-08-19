# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T12:30:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — RESEARCH (iter ~153, RES-0026)

**Pipeline POSITION: PILOT-001 DISCOVER COMPLETE (iter ~152, evidence 1).**
`PILOT-001_DISCOVER.md` authored — C2 gap CONFIRMED (no ACs; DESIGN resolves,
pattern BLE-002/ANDROID-001/IOS-001). Eligible (ANDROID-001 ✅ / IOS-001 ✅ /
EMERG-001 ✅ / SEC-001 ✅ COMPLETE; LEGAL-001 RESEARCH_COMPLETE). PROJECT_GRAPH
PILOT-001 evidence(1) + stage_note + known_limitations(7) + meta refreshed.
Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0, fmt clean.

## Next Action (iter ~153 — PILOT-001 RESEARCH, RES-0026)

1. **RESEARCH** RQ-1..RQ-6 (from DISCOVER):
   - **RQ-1** pilot topology + gateway architecture (Android+iOS leaf, gateway
     roles, Internet-relay AP provisioning, offline/mesh-primary test design)
   - **RQ-2** field identity/trust bootstrap at scale (QR pairing, TOFU +
     verified tier, EMERG-001 authority provisioning, decommissioning)
   - **RQ-3** pilot KPI + measurement methodology (OBS-001 metric set, battery,
     statistical plan 50–100 devices, EXP-002/003/005 alignment)
   - **RQ-4** legal/regulatory structure (NDRF/NDMA authorization, IT Rules
     2021 SSMI, WPC de-licensed bands, STQC timing — LEGAL-001 carry)
   - **RQ-5** distribution + ops runbook (Play closed track / TestFlight,
     signing, update path, OEM battery-kill matrix, telemetry consent, incident
     runbook)
   - **RQ-6** emergency exercise integration (EMERG-001 drill mode + authority
     root + SOS rate limits in a live field exercise)
2. **Write `engineering/memory/records/research/RES-0026.md`** (pattern
   RES-0025): best-in-class 2024-2026 sources (disaster-ops/field-trial/BCP
   methodology, NDRF/NDMA guidance, app-store closed-track ops, emergency
   exercise design), evidence-leveled L1-L5, **no AI citations**; verdict per
   RQ + DESIGN inputs.
3. **ALLOCATION.md**: register next RES-0027.
4. **PROJECT_GRAPH PILOT-001**: evidence(2) + stage_note RESEARCH COMPLETE;
   meta validation_status refreshed. **PROJECT_STATE** research-complete note.
5. **Context refresh**: ACTIVE_NODE / CURRENT_STATE / NEXT_ACTION → point at
   PILOT-001 DESIGN (iter ~154, `docs/implementation/PILOT_001_DESIGN.md`,
   AC-1..n resolves C2, DEC-PILOT-xxxx).

→ **After RESEARCH, PIPELINE STATE = PILOT-001 → DESIGN → IMPLEMENT → TEST →
SECURITY_REVIEW → VERIFY → ACCEPT** → then LORA-001/SAT-001 (P2 hardware) →
mission milestones. Baseline **658/0/1** clippy 0 fmt clean.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151, commit `d13845c`+`cd995ef`).
Pipeline: **PILOT-001** (first OPERATIONS node). Env-gates: Swift/device rows
ios.yml-macOS-CI + BLK-0005; Android toolchain host; LEGAL-001 open questions
are a PILOT-001 RESEARCH/legal carry (NDRF/NDMA structuring).

## Recent state files touched

- `engineering/memory/records/PILOT-001_DISCOVER.md` (iter ~152)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 evidence(1) + stage_note +
  known_limitations(7); meta validation_status)
- `engineering/PROJECT_STATE.yaml` (next_recommended PILOT-001 ACTIVE)
- `engineering/memory/execution-state.yaml` (stage → RESEARCH)
- `engineering/memory/records/execution-log.md` (iter ~152)