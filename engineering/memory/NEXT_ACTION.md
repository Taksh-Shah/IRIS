# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:15:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — IMPLEMENT (iter ~155, docs-only)

**Pipeline POSITION: PILOT-001 DESIGN COMPLETE (iter ~154, evidence 3).**
`PILOT_001_DESIGN.md` v1.0 authored — **C2 gap RESOLVED via AC-1..AC-18**;
D-1..D-9 pinned (topology 50-100 leaves + 1-3 gateways + gateway-scoped relay;
relay-cadence rule; test battery B-1..B-4; ProvisioningFlow; trust model; KPI +
Wilson-CI; NDMA DMEx M1/M2/M3; Play internal + TestFlight distribution;
REG-NOTES); **DEC-PILOT-0001..0009 ratified**. PROJECT_GRAPH PILOT-001 status
DESIGNING + acceptance_criteria(18) + evidence(3); PROJECT_STATE
research_complete 2→1 designing 0→1. Baseline **658/0/1** (23 suites;
iris-core 567 + iris-ios 11), clippy 0, fmt clean.

## Next Action (iter ~155 — PILOT-001 IMPLEMENT, docs-only ops deliverables per AC-1..AC-16)

1. **`docs/operations/PILOT_RUNBOOK.md`** (AC-2): topology (50-100 leaves +
   1-3 DESKTOP-001 gateways + Android-gateway fallback + gateway-scoped
   Internet-relay), ProvisioningFlow ceremony (first-run identity platform
   stores → operator mutual-QR batch 10-20 → gateway-verified escalation →
   EMERG-001 authority-root + DRILL-chain out-of-band bundle → decommission
   wipe+revoke+rotate; no-recovery documented), day-0/day-N ops, incident
   severity ownership, diagnostics QR export.
2. **`docs/operations/PILOT_KPI_PLAN.md`** (AC-3/AC-11): OBS-001 metric
   mapping (delivery ratio per class/transport, latency p50/p95/p99, hops,
   battery %/h fg/bg at fixed cadence, coverage, drill success), thresholds
   target/floor table, **Wilson-CI 95%** formula + per-leg sampling (never
   pooled), collection cadence + 7-day bounded retention (opt-in).
3. **`docs/operations/PILOT_EXERCISE.md`** (AC-4/AC-13): NDMA DMEx schedule
   (M1 TTEx injects → M2 field ME 50 devices, observers w/ NDMA observation
   format + self-assessment → M3 evaluation/AAR to NDRF, HSEEP-portable);
   DRILL certificate chain + **TEST-only SOS** under DRILL certs (SEC-001 +
   EMERG-001 drill suppression — real-SOS quota never consumed) + yellow-banner
   UX; safety annex (communications-only, no live-hazard simulation,
   public-notice window); EXP-002/003/005 before/after legs.
4. **`docs/operations/PILOT_DISTRIBUTION.md`** (AC-5/AC-15): Play **internal**
   track (≤100 testers, no review, near-instant) + closed 12×14 production
   clock at pilot close; TestFlight external (≤10k) + **90-day refresh**
   calendar-owned; Play App Signing upload-key custody (2-person + PEPK);
   OEM battery-kill matrix (ANDROID-001 carry); telemetry opt-in consent +
   retention; update path; incident ownership table.
5. **`docs/operations/REG_NOTES.md`** (AC-14): **WPC G.S.R. 853(E) 2021** =
   865-868 SRD (Table-I 25 mW e.r.p., duty ≤1%, FHSS ≥58 hop ≤50 kHz, EN
   300 220) supersedes 865-867 LEGAL-001 carry (LoRa future-leg only; v1 =
   BLE/Wi-Fi unlicensed); SSMI 50-lakh **non-applicability memo** at pilot
   scale; NDRF MoU authorization + STQC GA Month 30; **open counsel questions**
   (relay-as-telegraph §4 ITA 1885, intermediary classification, DPDPA
   consent, LoRa ETA) — **no research legal opinion**.
6. **Relay-cadence config seam edit** (AC-6): broadcast/telemetry cadence
   scales with online-node count (≤40 flat 0.6×..1.0×, >40 linear
   `1.0+(N-40)×f` default 0.075, configurable 0.01-0.1) at the
   INTERNET-001/ROUTE-001 seam; P0/P1 + directed exempt; airtime-minimization
   discipline recorded.
7. **EXTERNAL-FACTS doc edits** (AC-16): `GO_TO_MARKET.md`/`B2G.md` closed
   track → **internal**; `COMPLIANCE_RISK_REGISTER.md`/`SPECTRUM_CONSIDERATIONS.md`
   WPC 865-867 → **853(E) 2021 865-868**; `FIELD_OPERATIONS.md` 866.0 MHz ≤25 mW
   **CONFIRMED** note.
8. **PROJECT_GRAPH PILOT-001**: status DESIGNING → IMPLEMENTING + evidence(4) +
   stage_note IMPLEMENT COMPLETE; meta validation_status refreshed.
   **PROJECT_STATE** designing 1→0, implementing 0→1. Baseline 658/0/1 held
   (docs-only pass; AC-17 re-run at TEST).
9. **Context refresh**: ACTIVE_NODE / CURRENT_STATE / NEXT_ACTION → point at
   PILOT-001 TEST (iter ~156, SIM-001 B-1..B-4 pre-validation).

→ **After IMPLEMENT, PIPELINE STATE = PILOT-001 → TEST → SECURITY_REVIEW →
VERIFY → ACCEPT** → then LORA-001/SAT-001 (P2 hardware) → mission milestones.
Baseline **658/0/1** clippy 0 fmt clean.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151). Pipeline: **PILOT-001** (first
OPERATIONS node). DESIGN COMPLETE iter ~154 — C2 gap RESOLVED AC-1..AC-18.
Env-gates: Swift/device rows ios.yml-macOS-CI + BLK-0005; Android toolchain
host; LEGAL-001 open questions are a PILOT-001 legal carry (NDRF/NDMA
structuring; WPC 853(E)/2021; SSMI; DPDPA).

## Recent state files touched

- `docs/implementation/PILOT_001_DESIGN.md` (iter ~154)
- `engineering/memory/DECISIONS.md` (DEC-PILOT-0001..0009 ratified)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 DESIGNING, AC(18), evidence(3))
- `engineering/PROJECT_STATE.yaml` (research_complete 2→1, designing 0→1)
- `engineering/memory/execution-state.yaml` (stage → IMPLEMENT)
- `engineering/memory/records/execution-log.md` (iter ~154)