# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T16:30:00Z

## Active Node: LORA-001 — LoRa Gateway Transport

- **Type**: TRANSPORT (long-range radio transport via external LoRa module + BLE
  bridge)
- **Priority**: P2
- **Status**: **DESIGNING** (iter ~162, evidence 3, AC-1..17 resolved via
  `LORA_001_DESIGN.md` v1.0); **IMPLEMENT next** (iter ~163)
- **Deps**: TRANSPORT-001 — **COMPLETE**
- **Requires hardware**: true (external LoRa module; BLK-0005-adjacent device
  gating applies — GAP-004)
- **Description**: "LoRa long-range transport via external LoRa module + BLE
  bridge" — 866.0 MHz ISM, ≤25 mW e.r.p. (WPC G.S.R. 853(E) 2021 865-868 band,
  carved out of the v1 PILOT scope as GAP-004 REQUIRES_HARDWARE)
- **C2 gap**: **RESOLVED-VIA-RESEARCH+DESIGN** (no acceptance_criteria on the
  node — DESIGN defined AC-1..17, pattern
  BLE-001/WIFIAWARE-001/WIFIDIRECT-001/BLE-002; RESEARCH staged RQ-1..RQ-6 and
  produced DESIGN inputs D-1..D-7; DESIGN authored `LORA_001_DESIGN.md` v1.0)
- **Evidence**: 3 (`LORA-001_DISCOVER.md` iter ~160 + `research/RES-0027.md`
  iter ~161 + `docs/implementation/LORA_001_DESIGN.md` iter ~162)

## DESIGN COMPLETE (iter ~162) — LORA_001_DESIGN.md v1.0 (AC-1..17)

**LORA-001 DESIGN COMPLETE (iter ~162, docs-only pass, baseline 664/0/1 clippy 0
fmt clean held).** `docs/implementation/LORA_001_DESIGN.md` v1.0 authored (346
lines, pattern BLE_002_DESIGN.md, absorbs RES-0027 verdict PROCEED + D-1..D-7 +
gaps G-1..G-5): **C2 gap RESOLVED via AC-1..17.** Scheduled for IMPLEMENT:

- **AC-1..AC-13 (Rust core)** — `LoRaTransport` (TransportAdapter impl:
  register/transmit/poll/`duty_cycle_remaining`/`estimated_bandwidth_bps`≈14/
  `max_message_size_bytes`=255) + `Caps` SubGHz; `DutyCycleTracker` (rolling 1-h
  36,000 ms budget, `check_and_consume` → `next_send_window` R7, 60/25/10/5,
  P0-switch on exhausted, **no runtime override**, duty-violation unit tests);
  `ComplianceConfig` (866.0 / ≤25 mW e.r.p. / 1% per WPC G.S.R. 853(E) 2021
  Table-I); `LoRaFrame` + SLIP encode/decode; `RadioProfile` SF9/BW125/CR4/5 +
  SF12-P0 (+7 dB) + airtime pure fn (Semtech formula); `BacklogQueue`
  (REQ-ROUTE-C-003); `SimulatedLoRaTransport` + `SimLinkBudget` (AN1200.13 +
  Okumura-Hata urban); `LoRaLinkAdapter` AT-vs-SPI seam + `SimulatedLoRaAdapter`
  conformance; OBS-001 `iris.transport.lora.*` metrics + multipath dedup reuse +
  relay-cadence seam.
- **AC-14** — bridge contract BLE GATT (LORA_TX/RX/STATUS ≤255-B ATT ≤512-B MTU)
  + USB-SLIP alt (RFC 1055) documented; phone-as-gateway.
- **AC-15** — workspace green + clippy 0 + baseline **664/0/1** held.
- **AC-16** — SIMULATION_VALIDATED only (BLK-0005/GAP-004 level-5 gating).
- **AC-17** — SECURITY_REVIEW stage pending (redteam adversarial review).

**DEC-LORA-0001..0008 ratified** into DECISIONS.md. **FC-1..FC-4 external facts
verified already applied** (RES-0027 §9). PROJECT_GRAPH LORA-001
RESEARCH_COMPLETE → **DESIGNING** (AC-1..17, evidence 3, stage_note, meta
validation_status); PROJECT_STATE research_complete 2→1, designing 0→1.
**STAGE_TRANSITION → IMPLEMENT (iter ~163).**

## NODE_TRANSITION backdrop (iter ~159) — PILOT-001 ACCEPTED

**PILOT-001 ACCEPTED COMPLETE (iter ~159)** — first OPERATIONS node done; 29
COMPLETE nodes. PROJECT_GRAPH PILOT-001 IMPLEMENTING → **COMPLETE** (evidence 7);
independent verifier APPROVE (iter ~158, HEAD 748eaec, no findings); AC-17
**664/0/1**, clippy 0, fmt clean. **NODE_TRANSITION → LORA-001** (P2 TRANSPORT,
requires_hardware: true, DISCOVERED). SAT-001 (P2 TRANSPORT, requires_hardware:
true, DISCOVERED) remains after LORA-001.