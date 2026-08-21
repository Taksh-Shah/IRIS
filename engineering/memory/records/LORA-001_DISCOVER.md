# LORA-001 DISCOVER / UNDERSTAND — LoRa Gateway Transport

**Document ID**: IRIS-LORA-001-DISCOVER-001
**Version**: 1.0
**Node**: LORA-001 (P2 TRANSPORT, deps TRANSPORT-001 COMPLETE, requires_hardware
true)
**Date**: 2026-08-19
**Iteration**: ~160
**Stage**: DISCOVER → UNDERSTAND COMPLETE (read-only pass; workspace baseline
664/0/1 clippy 0 rustfmt clean **held** — no code touched)

---

## 1. Node definition (PROJECT_GRAPH lines ~414-421)

- **LORA-001** — "LoRa Gateway Transport", type **TRANSPORT**, priority **P2**,
  status **DISCOVERED**, description "LoRa long-range transport via external
  LoRa module + BLE bridge".
- **dependencies**: `["TRANSPORT-001"]` — **COMPLETE** → node **eligible**.
- **requires_hardware**: `true` — external LoRa module (ESP32 + SX1262 class
  radio) connected to the phone via BLE/USB; BLK-0005-adjacent device gating
  applies (DEC-0009 authorizes the software/protocol implementation; physical-
  device integration tests remain RESOURCE-gated known_limitations).
- **C2 gap**: **CONFIRMED** — no `acceptance_criteria` present → **RESOLVE at
  DESIGN** (AC-1..n, same pattern as BLE-001/WIFIAWARE-001/WIFIDIRECT-001/
  BLE-002).
- **Regulatory anchor**: WPC G.S.R. 853(E) 2021 — 865-868 MHz SRD band.
  IRIS field config: **866.0 MHz, ≤25 mW e.r.p.** (FIELD_OPERATIONS.md — CONFIRMED;
  RES-0026 Table-I reconciliation SUPERSEDES the older 865-867 carry).

## 2. What already exists (inherited surface)

| Artifact | Status | Relevance to LORA-001 |
|---|---|---|
| `TRANSPORT-001` — `TransportAdapter` trait + `TransportManager` (crates/iris-core/src/transport/) | COMPLETE | The syscall-level contract LORA-001 implements: `estimated_bandwidth_bps`, `max_message_size_bytes`, `duty_cycle_remaining`, register/deregister, poll/transmit, priority+concurrent-send guidance in TRANSPORT_ABSTRACTION.md. |
| `docs/transports/LORA.md` (319 lines) | exists | The authoritative LoRa design doc: CSS physical layer, WPC 865-868 band + 1% duty cycle, SF/BW/CR parameter tables (default SF9/BW125/CR4/5; P0 = SF12), raw-LoRa-vs-LoRaWAN split (IRIS = raw P2P, LoRaWAN optional uplink), SX1276-vs-SX1262 comparison (SX1262 REQUIRED), BLE GATT bridge (`LORA_TX_CHAR`/`LORA_RX_CHAR`/`LORA_STATUS_CHAR`) + USB-serial SLIP alt, `DutyCycleTracker` (36,000 ms/h, **NO P0 override** — P0 switches transport when budget exhausted), bandwidth budget (SF9 ≈ 1.4 B/s usable, 20 pkts/h), `SimulatedLoRaTransport` skeleton. |
| `docs/transports/TRANSPORT_ABSTRACTION.md` | COMPLETE | Capability matrix LoRa row (Special Hardware: LoRa module; Regulatory: WPC 865-867 MHz — 2021 Gazette correction pending in LORA.md, see RQ-1); `Caps` sub-GHz class; tx_ma_per_kbps + Free cost model rows; P0/P1 concurrent-send (multipath) semantics deduplicated at receiver. |
| `RES-0003` (LoRa Payload Constraints) | COMPLETE | Validates **255 B raw-LoRa P2P payload cap** (Semtech calc + LoRaWAN IN865 refs); IRIS P0 targets raw LoRa, not LoRaWAN; SF12 = longest airtime ("~2+ s at 255 B"); **protocol must fragment** >255 B. |
| `RES-0008` (F-constraints + R7) | COMPLETE | F38: IRIS P0 255 B validated by "severe penalties for larger packets"; **R7: retry/backoff scheduler needs per-transport airtime-budget abstraction (`next_send_window`)** — LoRa duty cycle is NEVER bypassed (INV-EMERG-002); F19/F37 IEEE Access 2024 multihop duty-cycle delay/PDR tradeoffs. |
| `RES-0018` (LoRa-mesh practice) | COMPLETE | Meshtastic = closest deployed analog (256-B packet limit, flood-with-suppression, dedup (sender, packet_id), CSMA/CAD); CVE-2025-52464; duty-cycle as flood defense (EU868 10% vs IRIS WPC 1%); PoW/RFC-4303 window concepts documented. |
| `RES-0026` (WPC reconciliation) | COMPLETE | **G.S.R. 853(E) 2021 Table-I**: 865-868 MHz, **25 mW e.r.p.**, **duty-cycle limit 1%**, EN 300 220 (ETSI harmonized) — SUPERSEDES 865-867; TEC ER Annexure-G5 (Dec-2024) lists LoRa 865-867; ETA/type-approval + import licence path documented. |
| `docs/routing/ROUTING_REQUIREMENTS.md` (REQ-ROUTE-C-001..003) | exists | C-001 LoRa MUST comply WPC 865-867/1% duty (band text stale — 2021 Gazette = 865-868, RQ-1); C-002 airtime budget allocated proportionally by priority (P0 60% / P1 25% / P2 10% / P3-P7 5%); C-003 **transmission backlog queue** — held by priority order, recovery rate 1% per 100 s silence. |
| `docs/operations/FIELD_OPERATIONS.md` | COMPLETE | 866.0 MHz (865-868 SRD, WPC G.S.R. 853(E) 2021 — CONFIRMED; AT+FREQ 866000000); **≤25 mW e.r.p.**; Waveshare SX1262 HAT / EBYTE E22-900M30S; 3 dBi omni 866 MHz N-connector; ≥2 m antenna separation; emergency mode "LoRa only" (3W draw, ~85 h); troubleshooting table (TX ≤14 dBm verification). |
| `docs/operations/REG_NOTES.md` QC-4 + `docs/legal/SPECTRUM_CONSIDERATIONS.md`, `COMPLIANCE_RISK_REGISTER.md` | COMPLETE | LoRa ETA / WPC type-approval path = future RF leg (post-pilot, GAP-004); spectrum + compliance carries for the LoRa leg. |
| **P0 envelope evidence** (PROTO-001/M1, DISC-0008, TEST-001 AC-8) | COMPLETE | `P0_MAX_ENVELOPE_BYTES = 255`; measured **P0 SOS = 237 B** (V001 golden vector, ≤ LoRa budget) with **84 B payload capacity ceiling** (64-B Ed25519 sig dominates); V001 pinned in `tests/golden_vectors/`. |
| `docs/protocol/FRAGMENTATION.md`, `REASSEMBLY.md`, `MESSAGE_MODEL.md` (255 B hard limit) | COMPLETE | 5×51-B fragments for 255-B envelope; reassembly ordering + signature over full payload; encryption at envelope layer (X25519 + ChaCha20-Poly1305, ephemeral pubkey field 16) — LoRa carries the same envelope format. |
| PILOT-001 D-2 relay-cadence scaling rule (`docs/routing/CONGESTION_CONTROL.md` seam) | COMPLETE | `relay_cadence` named policy (≤40 nodes flat 0.6x..1.0x; >40 linear 1.0+(N-40)*f); P0/P1 + directed exempt; airtime-minimization discipline — LoRa airtime = the tightest budget, cadence rule applies at the ROUTE-001 seam. |
| `GAP-004` (PROJECT_STATE) | REQUIRES_HARDWARE | "LoRa performance in dense urban India" — the field-validation gap EXP-LORA-001 targets (RPi4 + SX1262/REYAX RYLR890, RSSI/SNR/PDR at 200 m-2 km, suburban Mumbai, Month 8). |

## 3. LoRa-specific scope surface (the LORA-001 design surface)

**Bridge plane (phone ↔ radio)** — phone is the LoRa gateway node:
- **BLE GATT** primary: `LORA_TX_CHAR (Write)`, `LORA_RX_CHAR (Notify)`,
  `LORA_STATUS_CHAR (Read/Notify: RSSI, SNR, duty cycle remaining, battery)` —
  mirrors the BLE-001/BLE-002 `BleAdapter` seam; ESP32 firmware side.
- **USB serial (RNDIS/CDC-ACM) alt**: SLIP framing (`frame.encode_slip()`);
  `UsbLoRaGateway` with `DutyCycleTracker` gate.
- `LoRaMode` enum already sketched in LORA.md: `Raw { frequency_hz, sf, bw }`
  vs future `LoRaWAN { app_eui, dev_eui, app_key }` (TTN/Jio LoRaWAN uplink).

**Radio plane (RF)**:
- Raw LoRa P2P (direct node→node, IRIS addressing + routing over frames);
  IRIS-managed, no internet required. LoRaWAN only as an *optional* alternate
  uplink when a gateway is reachable.
- Default SF9/BW125/CR4/5 (5-15 km balance); **P0 SOS = SF12** max range
  (airtime ~10.5 s/255 B); BW500 reserved gateway-to-gateway bulk; CR4/8 only
  below -15 dB SNR.
- **255 B max payload**, time-on-air per 255 B @SF9 ≈ 1.8 s → **20 pkts/h,
  ≈1.4 B/s effective** under 1% duty. P4+ NEVER transmitted over LoRa;
  per-class floor P0 60 B / P1 80 B / P2 120 B / P3 150 B.

**Duty cycle / airtime (enforcement) plane**:
- WPC 1% duty = 36,000 ms TX per hour per device, hard legal cap — software
  enforced (`DutyCycleTracker.check_and_consume`), no emergency override.
- P0 policy: head-of-queue + transmit at next budget window; if exhausted,
  switch to next-best transport immediately (never held indefinitely).
- Coupling to R7 `next_send_window` airtime-budget hint at the retry/backoff
  scheduler + REQ-ROUTE-C-001..003 (priority-proportional allocation,
  backlog-queue + 1%/100 s recovery).

**Envelope / crypto plane** — LoRa carries the standard IRIS envelope:
- P0 SOS = 237 B ≤255 B (V001), payload ceiling 84 B, always signed, never
  encrypted (no recipient key).
- Private messages: X25519 + ChaCha20-Poly1305 at the envelope layer (field 16
  ephemeral pubkey) — encryption is transport-agnostic, no LORA-specific change.
- Fragmentation (5×51 B) reused for >255 B messages over LoRa.

**Sim plane** (`SimulatedLoRaTransport` in LORA.md + SIM-001):
- Parameters sketched: `sf`, `bandwidth`, `range_km`, `packet_loss_rate`,
  `duty_cycle_enforced`, `propagation_delay_ms`; `estimated_bandwidth_bps`
  returns 14 (B/s); needs full `TransportAdapter` wiring + realistic link-budget
  error model (Semtech AN1200.22 / Okumura-Hata urban multipliers per
  RESEARCH_GAPS).

## 4. Open questions → RESEARCH (iter ~161, RES-0027)

- **RQ-1 (regulatory reconciliation)**: Resolve the **1% duty / EIRP conflict**
  — LORA.md states "1W (30 dBm)" max EIRP for 865-868 (2021 Gazette) while
  RES-0026 Table-I + FIELD_OPERATIONS use **≤25 mW e.r.p.**; confirm which class
  governs IRIS's config (Table-I non-specific SRD) + precise 1% application
  (per-device, TX-only, 36 s/h); TEC ER Annexure-G5 865-867-vs-2021-865-868
  re-confirmation; equipment ETA/BIS type approval + import licence path.
- **RQ-2 (duty-cycle enforcement semantics)**: Sliding-window vs fixed-hour
  tracker; interaction with R7 `next_send_window` retry hint; REQ-ROUTE-C-001..
  003 priority-proportional allocation (P0 60% / P1 25% / P2 10%); P0-switch-on-
  exhausted transport selection; collision with brief-burst + multi-gateway
  topologies.
- **RQ-3 (radio parameter set)**: Validate SF9/BW125/CR4/5 defaults + SF12-P0
  against 2024-2026 SOTA LoRa practice (Semtech AN1200.x, ETSI EN 300 220,
  Indian regional params — RES-0003 legacy data refresh); 255-B airtime
  correctness for SF/BW/CR table; raw-LoRa-vs-LoRaWAN v1 decision reconfirm.
- **RQ-4 (bridge protocol)**: BLE GATT bridge vs USB serial SLIP — framing,
  flow control, status telemetry (RSSI/SNR/duty/battery), MTU behavior, ESP32
  firmware reference (SX1262 SPI interface), AT-command surface for
  Waveshare/EBYTE modules, CommEvent cadence.
- **RQ-5 (TRANSPORT-001 integration)**: Register/deregister patterns (dongle
  plug/unplug), capability row (`Caps`), `TransportAdapter` impl contract
  (transmit/poll/duty_cycle_remaining/bandwidth/max_size), priority + concurrent
  send reuse (P0/P1 multipath, receiver dedup), fragmentation reuse,
  relay-cadence + congestion-control seam, OBS-001 telemetry (`iris.transport.
  lora.*` RSSI/SNR).
- **RQ-6 (simulation fidelity)**: SimulatedLoRaTransport parameterization +
  validation against GAP-004 / EXP-LORA-001 plan (link budget model choice,
  urban multi-path, packet loss/delay injection, duty-violation test vectors);
  how sim credibility is stated (SIMULATION_VALIDATED, never FIELD_VALIDATED
  without hardware).

## 5. Known limitations baseline (write to graph at ACCEPT)

- BLK-0005: physical-device LoRa integration tests RESOURCE-gated (DEC-0009);
  software/protocol implementation proceeds in Rust sim + bridge contracts.
- GAP-004: LoRa performance in dense urban India REQUIRES_HARDWARE — no
  Level-5 field data (EXP-LORA-001 planned Month 8).
- Regulatory: band/EIRP/duty confusion to reconcile at RESEARCH (RQ-1); ETA/
  BIS/import certification required for production-branded hardware (DIY
  hobbyist grey area only).
- Range figures are simulation inputs, not product guarantees (LORA.md range
  note; maturity labels RESEARCHED/SIMULATION_VALIDATED/FIELD_VALIDATED).
- No LoRa dongle hardware present on dev host (Windows) — env-gated like BLE/
  Wi-Fi hardware legs.

## 6. Stage outcome

- **UNDERSTAND COMPLETE** — reference surface fully read (LORA.md + TRANSPORT-
  ABSTRACTION row + RES-0003/0008/0018/0026 + ROUTING_REQUIREMENTS C-001..003 +
  FIELD_OPERATIONS + REG_NOTES/SPECTRUM + P0-envelope evidence + PILOT-001
  relay-cadence seam + GAP-004); C2 gap confirmed; RQ-1..RQ-6 staged.
- **NEXT**: RESEARCH (iter ~161, RES-0027 RQ-1..RQ-6) → DESIGN (iter ~162:
  `docs/implementation/LORA_001_DESIGN.md`, AC-1..n resolves C2,
  DEC-LORA-xxxx) → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT → SAT-001.
- Workspace **untouched** (read-only pass) — baseline 664/0/1 clippy 0 held.