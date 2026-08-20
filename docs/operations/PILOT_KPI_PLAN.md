# Pilot KPI Plan — Measurement, Thresholds, Statistical Plan (PILOT-001)

**Document ID**: IRIS-PILOT-KPI-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS — DESIGN COMPLETE iter ~154; IMPLEMENT iter ~155)
**Date**: 2026-08-19
**Status**: ACCEPTANCE TARGET — PILOT_001_DESIGN.md AC-3 / AC-11
**Inputs**: PILOT_001_DESIGN.md D-6 (§7); RES-0026 RQ-3 (OBS-001 ↔ NDMA DMEx
mapping, DTN-trial vocabulary arXiv 2603.10153); OBS_DESIGN.md/OBSERVABILITY.md
(metric taxonomy, retention, opt-in); TELEMETRY.md; FIELD_OPERATIONS.md.

---

## 1. KPI set — OBS-001 mapping

Retention: **7-day bounded**; telemetry **opt-in** (consent per §4); OBS-001
`msg.delivered`/`neighbors.active`/`partition.detected` primitives; export is
off-by-default (local only v1 — OBS_VERIFICATION.md).

**Table 1 — KPI thresholds (target / floor)**

| KPI | OBS-001 source | Target | Floor | Reporting |
|-----|----------------|--------|-------|-----------|
| Delivery ratio — P2P | `msg.delivered` + transport counters | 0.95 | 0.90 | per transport + class |
| Delivery ratio — multi-hop | `msg.delivered` (hop ≥2) | 0.90 | 0.85 | per class |
| Latency p50 / p95 / p99 | `msg.delivered.latency_ms` | 2 s / 10 s / 30 s | 5 s / 30 s / 60 s | per class/transport |
| Hop count p50 / p95 | `msg.delivered.hops` | 2 / 5 | 3 / 8 | per class |
| Battery drain — foreground | controlled fg test, fixed cadence | ≤3 %/h | ≤5 %/h | per model |
| Battery drain — background | controlled bg test, fixed cadence | ≤1 %/h | ≤2 %/h | per model |
| Coverage | `neighbors.active`, `partition.detected` | active-neighbor floor per zone | no coverage holes > 500 m | per zone |
| Relay throughput | Wi-Fi Aware/Direct + BLE counters | per docs/performance/* budgets | 80% of budget | per transport |
| Drill success | EMERG-001 drill metrics | TTEx/ME objectives met | debrief + AAR completed | per drill leg |
| SOS delivery (drill, TEST-only) | EMERG-001 SOS path | P0 delivery ratio 1.0 (drill) | 0.95 | per drill leg |

**Table 2 — Statistical plan (Wilson-CI)**

- Count/ratio KPIs are reported with a **Wilson score interval** at 95%
  confidence (small-N, non-normal — justified for 50-100 devices).
- Formula: for `n` samples and `p̂ = k/n`, lower/upper:
  `p̃ = (k + z²/2) / (n + z²) ± (z √(p̂(1-p̂)/n + z²/(4n²))) / (1 + z²/n)`,
  `z = 1.96` (95%).
- Sampling: **per class/transport per leg, never pooled** across legs or
  classes. Reported as `p̂ [L, U] Wilson 95%`.
- Legs: B-1..B-4 (PILOT_001_DESIGN.md §5) and M1/M2/M3 exercise legs
  (PILOT_EXERCISE.md). Each leg records `n` (messages at risk), `k`
  (delivered), per-class split.
- **Threshold evaluation**: floor met iff Wilson lower bound ≥ floor; target
  met iff p̂ ≥ target AND lower bound ≥ floor. Use the lower bound for
  pass/fail decisions (honest small-N).

## 2. Battery methodology

- **Controlled per-device drain test** at FIXED telemetry cadence (not
  field-only): foreground 30-min continuous relay, then background-rested
  60-min, same model cohorts (Android A/B, iOS A/B).
- Report %/h averaged per model; physical rows BLK-0005-gated into the pilot
  (on-device measurements happen in the pilot itself).
- Re-run at every release bump for the pilot cohort (distribution refresh
  cadence — PILOT_DISTRIBUTION.md §3).

## 3. Collection cadence

- Broadcast/telemetry flush interval follows the **relay_cadence** multiplier
  (PILOT_RUNBOOK.md §3) — the cadence IS the online-node-count-scaled interval.
- Directed metrics emit at `msg.delivered`.
- Export to the operator dashboard is gateway-scoped via the private relay
  endpoint; **local-only by default** v1 (OBS_VERIFICATION.md AC-5).
- Storage bounded to 7 days; no payload/PII in metrics (OBS-001 P1-P3).

## 4. Consent / privacy

- Opt-in consent slide in the M1 briefing; opt-out reversible per device.
- Telemetry definitions per TELEMETRY.md (allow-list attrs, HMAC-salted
  hashes, 8-byte ID truncation).
- DPDPA posture: DPDPA Rules 2025 §4(1)(b)/§6(1) privacy-by-design —
  minimization + bounded retention (RES-0012 / OBS_DESIGN carry).
- Data-governance link: docs/legal/DATA_GOVERNANCE.md + COMPLIANCE_RISK_REGISTER.md.

## 5. Acceptance hooks

- AC-3 (metric mapping + thresholds + battery methodology + cadence) and
  AC-11 (Wilson-CI sampling + reporting format) evidence at TEST/VERIFY —
  simulator legs B-1..B-4 pre-validated in SIM-001 harness, field legs at
  M1/M2, KPI evaluation report at M3 (PILOT_EXERCISE.md §3).
- Every KPI row above is an AC-7..AC-10 target in PILOT_001_DESIGN.md §5.