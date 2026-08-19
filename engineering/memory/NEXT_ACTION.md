# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T12:00:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — DISCOVER (iter ~152)

**Pipeline POSITION: NODE_TRANSITION to PILOT-001 complete.** IOS-001 ACCEPTED
(iter ~151) — commit `d13845c` (SECURITY_REVIEW fixes + records + state; 16
files +700/-478). PROJECT_GRAPH IOS-001 → COMPLETE evidence(9); PROJECT_STATE
completed 27 → **28**, implementing 1 → 0; CHANGELOG **0.3.58**. Baseline
**658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0, fmt clean.

## PILOT-001 node (from PROJECT_GRAPH)

- **Type**: OPERATIONS · **Priority**: P2 · **Status**: DISCOVERED
- **Name**: Field Pilot Deployment — "NGO pilot, campus safety pilot"
- **Deps**: ANDROID-001 ✅ / IOS-001 ✅ / EMERG-001 ✅ / SEC-001 ✅ / LEGAL-001
  (RESEARCH_COMPLETE) — eligible
- **Milestone (Beta/Pilot)**: real-world NGO/campus pilot, 100+ devices

## Next Action (iter ~152 — PILOT-001 DISCOVER)

1. **DISCOVER** PILOT-001 (read-only): catalog pilot scope (NGO pilot + campus
   safety pilot, 100+ devices), deployment topology (Android + iOS leaf nodes,
   gateway + LoRa/SAT outlook, Internet-relay fallback), ops-runbook
   prerequisites (build/signing/TestFlight + PSB for Android, Keychain
   provisioning), **C2 gap check** (no acceptance criteria exist on the node —
   confirm; DISCOVER should record RQ-1..n), BLK-0005 device gates, legal
   carries (LEGAL-001 RESEARCH_COMPLETE open questions).
2. **Write `engineering/memory/records/PILOT-001_DISCOVER.md`** (pattern
   IOS-001_DISCOVER.md / BLE-002_DISCOVER.md): reference surface read
   (TEST-001 test infra, ANDROID-001/IOS-001 platform nodes, EMERG-001 drill
   suppression, PROJECT_STATE critical_path, docs/ops runbook if present).
3. **PROJECT_GRAPH PILOT-001**: status DISCOVERED → **DISCOVERED** (held) +
   evidence(1) + known_limitations baseline + stage_note DISCOVER COMPLETE.
   **PROJECT_STATE / CHANGELOG / meta validation**: refresh `last_validated`.
4. **Context refresh**: ACTIVE_NODE / CURRENT_STATE / NEXT_ACTION → point at
   PILOT-001 DISCOVER COMPLETE → RESEARCH (iter ~153, RES-0026).

→ **After DISCOVER, PIPELINE STATE = PILOT-001 → (RESEARCH → DESIGN → … → ACCEPT)**
→ then LORA-001/SAT-001 (P2 hardware) then MISSION (MVP/Alpha/Beta/Pilot
milestones). Baseline **658/0/1** clippy 0 fmt clean.

## Context

28 COMPLETE nodes. Pipeline: **PILOT-001** (next DISCOVERED node after IOS-001).
Env-gates: Swift/device rows remain ios.yml-macOS-CI + BLK-0005; LEGAL-001 open
legal questions are a PILOT-001 research carry.

## Recent state files touched (iter ~151)

- `engineering/PROJECT_GRAPH.yaml` (IOS-001 → COMPLETE, stage_note ACCEPTED)
- `engineering/PROJECT_STATE.yaml` (completed 27→28, implementing 1→0)
- `engineering/CHANGELOG.md` (0.3.58)
- `engineering/memory/execution-state.yaml` (node → PILOT-001 / DISCOVER)
- `engineering/memory/records/execution-log.md` (iter ~151)