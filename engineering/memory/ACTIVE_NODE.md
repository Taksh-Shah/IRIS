# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:30:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **IMPLEMENTING** — DISCOVER (iter ~152) → RESEARCH (iter ~153,
  RES-0026, PROCEED) → DESIGN (iter ~154, `PILOT_001_DESIGN.md`, C2 gap
  RESOLVED) → IMPLEMENT COMPLETE (iter ~155, docs-only ops deliverables) →
  **TEST next (iter ~156, SIM-001 B-1..B-4 pre-validation)** → SECURITY_REVIEW
  → VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **C2 gap**: **RESOLVED** at DESIGN — AC-1..AC-18 defined; ops docs authored
  at IMPLEMENT (AC-1..AC-16); TEST/SECURITY_REVIEW evidence pending (AC-17/18)

## DESIGN (iter ~154) — PILOT_001_DESIGN.md v1.0 (evidence 3)

`docs/implementation/PILOT_001_DESIGN.md` absorbing RES-0026 D-1..D-9;
DEC-PILOT-0001..0009 ratified. Pinned: D-1 topology (50-100 leaves
Android relay-primary / iOS limited-relay + 1-3 DESKTOP-001 gateways +
Android-gateway fallback + gateway-scoped Internet-relay, mesh-primary, LoRa
excluded GAP-004); D-2 relay-cadence rule (≤40 flat 0.6×..1.0×, >40 linear
`1.0+(N-40)×f` default 0.075; P0/P1 + directed exempt); D-3 test battery
B-1..B-4 (NCT-of-N + latency CDF, partition/healing, multi-hop SCF mule-chain,
iOS limited-relay; SIM-001 pre-validation + field legs); D-4 ProvisioningFlow
(mutual-QR batch 10-20, verified escalation, EMERG authority-root/DRILL-chain
out-of-band bundle, decommission wipe+revoke+rotate, no-recovery); D-5 trust
model (TOFU + verified tier); D-6 KPI + Wilson-CI 95%; D-7 NDMA DMEx M1/M2/M3;
D-8 Play internal + TestFlight distribution; D-9 REG-NOTES (WPC 853(E) 2021
865-868).

## IMPLEMENT COMPLETE (iter ~155, evidence 4) — ops deliverables

1. **`docs/operations/PILOT_RUNBOOK.md`** (AC-2): topology, gateway roles/
   fallback, ProvisioningFlow ceremony (batch 10-20, mutual-QR, verified
   escalation, authority-root/DRILL bundle), decommission (wipe+revoke+rotate),
   day-0/day-N ops, incident ownership, diagnostics QR.
2. **`docs/operations/PILOT_KPI_PLAN.md`** (AC-3/AC-11): OBS-001 mapping,
   thresholds target/floor, Wilson-CI 95% formula + sampling (per class/
   transport, never pooled), controlled fg/bg battery, collection cadence +
   7-day retention.
3. **`docs/operations/PILOT_EXERCISE.md`** (AC-4/AC-13): NDMA DMEx M1 TTEx →
   M2 field ME (observers + self-assessment, TEST-only SOS under DRILL certs,
   yellow banner) → M3 evaluation/AAR to NDRF; safety annex; EXP-002/003/005.
4. **`docs/operations/PILOT_DISTRIBUTION.md`** (AC-5/AC-15): Play internal
   (≤100) + closed 12×14 production clock; TestFlight external + 90-day
   refresh; signing 2-person + PEPK; OEM battery-kill; telemetry consent.
5. **`docs/operations/REG_NOTES.md`** (AC-14): WPC G.S.R. 853(E) 2021 865-868
   supersedes 865-867 (LoRa future-leg only); SSMI non-applicable; NDRF MoU +
   STQC GA; open counsel Qs QC-1..5; no legal opinion.
6. **Relay-cadence seam** (AC-6): section appended to
   `docs/routing/CONGESTION_CONTROL.md` (≤40 flat, >40 linear, P0/P1 + directed
   exempt).
7. **EXTERNAL-FACTS edits** (AC-16): GO_TO_MARKET internal-track note;
   COMPLIANCE_RISK_REGISTER + SPECTRUM_CONSIDERATIONS 865-868/853(E);
   FIELD_OPERATIONS 866.0 CONFIRMED.

## Pipeline position

- **PILOT-001 IMPLEMENT COMPLETE (iter ~155, evidence 4)** — 28 COMPLETE
  nodes. NEXT: **TEST (iter ~156, SIM-001 sim pre-validation of B-1..B-4)**.
- Baseline **658/0/1** (23 suites; iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held (docs-only pass).

## TEST scope (iter ~156)

1. Run SIM-001 pre-validation of battery B-1..B-4 per PILOT_001_DESIGN.md §5:
   B-1 NCT-of-N all-N delivery + latency CDF N=20/50/100; B-2 partition/healing
   (uplink down, heals no-duplicates); B-3 multi-hop SCF mule-chain delivery +
   hop CDF; B-4 iOS limited-relay marker (env/BLK-0005-gated honest).
2. Author `PILOT-001_TEST.md` evidence map (pattern BLE-002_TEST.md) mapping
   AC-7..AC-10.
3. Persist iter ~156 in all durable artifacts + commit.
4. → SECURITY_REVIEW (iter ~157, AC-18) → VERIFY (AC-17/18) → ACCEPT →
   NODE_TRANSITION.