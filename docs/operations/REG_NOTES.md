# REG-NOTES — Regulatory / Authorization Codification (PILOT-001)

**Document ID**: IRIS-PILOT-REG-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS — DESIGN COMPLETE iter ~154; IMPLEMENT iter ~155)
**Date**: 2026-08-19
**Status**: ACCEPTANCE TARGET — PILOT_001_DESIGN.md AC-14
**Inputs**: PILOT_001_DESIGN.md D-9 (§10); RES-0026 RQ-4 (WPC G.S.R. 853(E)
2021 gazette, TEC ER Annexures Dec-2024, IT Rules SSMI, NDRF MoU, STQC);
LEGAL-001 carry (LEGAL_RESEARCH.md, INDIA_COMPLIANCE.md, COMPLIANCE_RISK_REGISTER.md).

**Scope statement**: This codification is an **engineering/regulatory surface
record — NOT a legal opinion.** Legal interpretation and sign-off remain the
LEGAL-001 gate (lawyer + operator review required before any commercial
deployment; pilot proceeds under MoU terms).

---

## 1. Governing spectrum instrument — WPC G.S.R. 853(E), 10-Dec-2021

- **Instrument**: "Use of Low Power Equipment in the Frequency Band 865-868
  MHz for Short Range Devices (Exemption from Licence) Rules, 2021", Ministry
  of Communications (WPC), made under **§4 + §7 Indian Telegraph Act 1885**
  and **§4 + §10 Indian Wireless Telegraphy Act 1933**; exemption on
  non-interference, non-protection, shared and non-exclusive basis.
- **SUPERSEDES** the 2005 RFID 865-867 rules that earlier LEGAL-001 carries
  cite. **Governing band = 865-868 MHz**.
- **Table-I (Non-Specific SRD — telemetry, telecommand, alarms, data)**:
  **25 mW e.r.p.**, **duty cycle ≤1%** (applies to entire transmission), FHSS
  max occupied bandwidth ≤50 kHz over ≥58 hop channels, EN 300 220 (ETSI
  harmonized).
- **IRIS field config CONFIRMED compliant** (FIELD_OPERATIONS.md): **866.0
  MHz, ≤25 mW e.r.p.**
- **v1 pilot = BLE + Wi-Fi Aware + Wi-Fi Direct + Internet-relay — all
  unlicensed 2.4/5 GHz short-range operations → no LoRa/RF-equipment trigger.**
  The 865-868 MHz rule set is **LoRa future-leg only** (P2 LORA-001, GAP-004)
  and must be re-validated against TEC ER (Dec-2024) Annexure-G5 + ETA
  before any RF hardware ships.

## 2. IT Rules 2021 — SSMI non-applicability memo (pilot scale)

- "Significant social media intermediary" threshold = **50 lakh registered
  users** (S.O. 942(E), 25-Feb-2021). A 50-100 device pilot / ≤10-device B2B
  POC tier is orders of magnitude below.
- **Position**: SSMI duties (grievance officer, traceability, etc.) are
  **NOT triggered at pilot scale**. Re-verify gazette text (S.O. 942(E)) with
  counsel at legal review (RES-0026 G-P6) and re-assess at every scale-up
  (GA+; district pilot 20 gateways still ≪ 50 lakh).

## 3. Pilot authorization mechanism

- **NDRF 12th Battalion (Gandhinagar) 50-device / 3-month pilot MoU
  (Month 6-9)** is the pilot's authorization (GO_TO_MARKET/B2G carry).
- **STQC certification** planned at GA (**Month 30**) — pilot proceeds under
  MoU terms per COMPLIANCE_RISK_REGISTER (no certification gate on the pilot
  itself).
- B2B 30-day 5-device POC tier and campus/NGO cohorts run under the same
  operator authority chain (operator = accountable organization; relay
  operator holds the private IRIS relay endpoint).

## 4. Open counsel questions (tracked; for LEGAL-001 gate)

| # | Question | Reference |
|---|----------|-----------|
| QC-1 | Does operating an IRIS relay constitute "establishing a telegraph" under §4 ITA 1885 beyond WPC de-licensing? (LEGAL-001 LQ-001) | LEGAL_RESEARCH.md |
| QC-2 | Intermediary classification of the IRIS relay under IT Rules 2021 (and stable definition changes) | LEGAL-001 carry |
| QC-3 | DPDPA fiduciary/consent model for pilot operators collecting consented telemetry | DPDPA Rules 2025; TELEMETRY.md |
| QC-4 | LoRa ETA / WPC type-approval path for the future RF leg (post-pilot, GAP-004) | TEC ER Annexure-G5 |
| QC-5 | Re-confirm IT Rules SSMI threshold gazette text (S.O. 942(E)) | RES-0026 G-P6 |

**No research legal opinion is expressed anywhere in this record or its
inputs.** Lawyer + operator review gate per LEGAL-001 is REQUIRED before any
commercial step.

## 5. Doc-correction hooks (AC-16)

- `COMPLIANCE_RISK_REGISTER.md` mitigation row "865-867 MHz" → point at
  G.S.R. 853(E) 2021 (865-868). `SPECTRUM_CONSIDERATIONS.md` §1.2/§2 → add
  governing-instrument note. `FIELD_OPERATIONS.md` checklist "within 865-867"
  → 865-868 + CONFIRMED. `GO_TO_MARKET.md` §2.1 → add internal-track note.
  See docs touched in the IMPLEMENT pass (iter ~155).

## 6. Acceptance hooks

- AC-14 evidence: this record + §4 registry + RES-0026 RQ-4 source leaf (thc/
  egazette gazette, L1). AC-16 evidence: the §5 doc edits.