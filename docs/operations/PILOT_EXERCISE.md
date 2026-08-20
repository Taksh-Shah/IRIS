# Pilot Exercise Plan — NDMA DMEx Structure (PILOT-001)

**Document ID**: IRIS-PILOT-EXERCISE-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS — DESIGN COMPLETE iter ~154; IMPLEMENT iter ~155)
**Date**: 2026-08-19
**Status**: ACCEPTANCE TARGET — PILOT_001_DESIGN.md AC-4 / AC-13
**Inputs**: PILOT_001_DESIGN.md D-7 (§8) + D-6 (§7); RES-0026 RQ-3/RQ-6 (NDMA
DMEx Guidelines Oct-2024, Mock-Exercises process, Exercise Suraksha Chakra
2025, HSEEP); EMERG-001 (drill mode, DRILL certs, TEST severity, yellow
banner); SEC-001 (quotas — drill never consumes real-SOS capacity).

---

## 1. Schedule (NDMA DMEx structure)

| Month | Activity | NDMA family | Objective | OBS-001 capture |
|-------|----------|-------------|-----------|-----------------|
| M1 | **TTEx** (Tabletop Exercise) + Orientation-cum-Coordination Conference | discussion-based | scenario injects; ESF/SOP testing; observer briefing (observation format) | dry-run metric baseline; observers briefed |
| M2 | **Field Mock Exercise (50 devices, NDRF 12th Bn, Gandhinagar)** | action-based | real deployments; observers w/ NDMA observation format; self-assessment forms; TEST-only SOS/broadcast under DRILL certs; yellow-banner UX | full OBS-001 capture incl. drill-success + SOS-in-TEST; EXP-002/003/005 before/after legs |
| M3 | **Evaluation** | Phase 4 | debrief; good-practices/gaps/weaknesses; **AAR/IP to NDRF** (HSEEP-portable template) | KPI evaluation vs PILOT_KPI_PLAN.md §1 Table 1 (Wilson-CI lower bounds) |

- **Cadence**: ≥1 drill/year per NDMP 2019 (carry). Pilot = the calendar-year
  drill for 2026 (NDRF requirement satisfied within the MoU).

## 2. Drill mechanics (TEST-only; EMERG-001 integration)

1. **Authority root + DRILL certificate chain** are provisioned out-of-band at
   provisioning (PILOT_RUNBOOK.md §2.4) — DRILL certs distinguish exercise
   traffic from real traffic end-to-end.
2. **TEST-only SOS/broadcast**: emergency traffic in an exercise leg carries
   TEST severity + DRILL cert + **yellow banner** UX. SEC-001 quotas apply —
   the drill exercises the rate-limiter under DRILL certificates ONLY, so real
   SOS capacity is never consumed by the drill (EMERG-001 drill suppression +
   SEC-001 quota posture; RES-0026 RQ-6 f.4).
3. **Observers**: independent observers with NDMA observation format; every
   observer briefed at M1, given a structured sheet, debriefed in M3.
4. **Self-assessment forms**: each leaf operator completes NDMA-style
   self-assessment (worked / did-not-work / needs-change per exercise cell).
5. **Debrief + AAR**: M3 debriefing session identifies good practices / gaps /
   weaknesses transparently; **After-Action Report + Improvement Plan to NDRF**
   (HSEEP-portable: program management → conduct → evaluation → improvement
   planning; Exercise Evaluation Guides mapped to OBS-001 metrics).

## 3. Exercise cells and legs

- **Cells**: Command (relay operator + team lead); Field (leaf operators);
  Observers; Evaluation (M3 report team).
- **Scenario injects (M1)**: telecom down, partial BLE coverage, multi-hop
  SCF mule-chain data path, uplink partition, iOS-bg limited-relay — each inject
  mapped to test battery B-1..B-4 (PILOT_001_DESIGN.md §5).
- **EXP legs**: EXP-002/003 (Ahmedabad mobility traces) and EXP-005 (iOS bg)
  are before/after sub-experiments inside M1-M3: baseline at M1, treatment at
  M2, delta evaluated at M3.

## 4. Safety annex (DRIS posture; AC-13)

1. **Communications-only**: drills exercise the communications fabric; NO
   movement of people/machinery beyond normal operations; NO live-hazard
   simulation (NDMA exercises are simulation-based by design).
2. **Public-notice window**: scheduled + publicized drill windows with a
   clear "TEST" banner to avoid confusion with real emergencies.
3. **Observer oversight** throughout; team lead may abort any leg.
4. Real emergencies during a drill window: drill state immediately
   deactivated (EMERG-001 mode transition), real emergency traffic takes
   priority — drill certificates never gate a real response.
5. Battery/OEM limits respected — no exertion beyond normal device operation.

## 5. Success criteria

- TTEx objectives met (ESF/SOP exercised; observers briefed).
- M2 ME: TEST-only SOS P0 delivery ratio ≥ 0.95 Wilson-95% lower bound
  (target 1.0); full OBS-001 capture; no real-SOS capacity consumed by drill.
- M3: debrief completed; AAR/IP delivered to NDRF; KPI evaluation report per
  PILOT_KPI_PLAN.md §1 (targets/floors + Wilson-CI lower bounds).

## 6. Document hooks

- AC-4 (exercise plan vs this doc) + AC-13 (TEST-only SOS + observers +
  debrief + safety annex) evidence at TEST/VERIFY (dry-run + field leg
  records); mapped from PILOT_001_DESIGN.md §8/§11.