# LORA-001 Design — LoRa Gateway Transport v1

**Document ID**: IRIS-LORA-001-DESIGN-001
**Version**: 1.0
**Node**: LORA-001 (P2 TRANSPORT, hardware-gated, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-19
**Research basis**: RES-0027 (2026-08-19, LORA-001 research — verdict
**PROCEED**, RQ-1..RQ-6 ADOPT/ADOPT-WITH-CONDITION, 7 DESIGN inputs D-1..D-7,
gaps G-1..G-5, external facts FC-1..FC-4 ALL APPLIED); `engineering/memory/
records/LORA-001_DISCOVER.md` (UNDERSTAND, iter ~160); `docs/transports/LORA.md`
(327-line physical-layer spec); TRANSPORT_ABSTRACTION.md (LoRa row +
TransportManager architecture); ROUTING_REQUIREMENTS.md REQ-ROUTE-C-001..003;
CONGESTION_CONTROL.md §355 (relay-cadence seam); source surface
`crates/iris-core/src/transport/mod.rs` (Transport trait @315,
`TransportCapabilities` @150, `TransportCostClass` @181, `TransportId` @46,
`LORA_COST` @244); `crates/iris-core/src/transport/manager.rs`
(register @84 / deregister @109 + max_message_size gating); BLE-001/BLE-002
(`BleAdapter` seam pattern), WIFIAWARE-001/WIFIDIRECT-001 (adapter-injected
pattern); RES-0003 (255-B raw-LoRa cap, fragment >255 B), RES-0008 (R7
`next_send_window` airtime budget); FIELD_OPERATIONS.md (866.0 MHz ≤25 mW e.r.p.
CONFIRMED, Waveshare SX1262 HAT / EBYTE E22-900M30S, EXP-LORA-001); ACCEPTANCE_
POLICY.yaml TRANSPORT-type additions; GAP-004 / BLK-0005 (hardware-gated
device coverage).
**Absorbs**: RES-0027 RQ-1..RQ-6 answers + 7 DESIGN inputs + gaps G-1..G-5
(design-positioned); LORA-001_DISCOVER surface; REG-NOTES codification (WPC
G.S.R. 853(E) 2021).
**Reconciles**: `docs/transports/LORA.md` + TRANSPORT_ABSTRACTION.md +
ROUTING_REQUIREMENTS.md + FIELD_OPERATIONS.md vs RES-0027 (FC-1..FC-4 applied
iter ~161 — this design documents and carries those corrections, it does not
re-litigate them).

---

## 1. Scope

Implement the **LoRa long-range transport** on the existing `Transport` trait as
an **adapter-injected** node (BLE-001/BLE-002 `BleAdapter`-seam lesson): a
platform-neutral Rust core in `crates/iris-core/src/transport/lora.rs` plus a
**`LoRaLinkAdapter`** trait whose implementations talk to the external LoRa
module over **AT-serial** (EBYTE E22-900M30S over USB-CDC/UART) or **SPI-native**
(Waveshare SX1262 LoRa HAT on RPi4) firmware. The phone/RPi becomes a LoRa
gateway node (does not require a LoRa radio inside the phone). The physical
SX1262/E22 firmware + GATT-bridge packaging is **hardware-gated** (BLK-0005 /
GAP-004 / RES-0027 G-2) — this node delivers the Rust core, the adapter seam,
the **`DutyCycleTracker`** regulatory enforcer, the **`SimulatedLoRaTransport`**
wiring + link-budget model, and AC-1..17 (C2 gap RESOLVED per ACCEPTANCE_POLICY
WORKFLOW). No new crypto: the envelope layer X25519+ChaCha20-Poly1305
(CRYPTO-001) is transported unchanged (RES-0027 RQ-5).

**v1 shape (RES-0027 verdict PROCEED)**: raw-LoRa peer-to-peer (no LoRaWAN
stack, no network keys, no join ceremony) at **866.0 MHz, ≤25 mW e.r.p.
(≈14 dBm), ≤1% duty (36 s/device/hour)** per WPC G.S.R. 853(E) 2021 Table-I.
Default link **SF9 / BW125 kHz / CR4/5**; **P0 uses SF12** (−129/−136 dBm
sensitivity, AN1200.13 → ≈+7 dB link budget). **`DutyCycleTracker`** (rolling
1-h, 36,000 ms budget, `check_and_consume` → `next_send_window` per RES-0008
R7) is a hard legal floor; priority-proportional airtime 60/25/10/5 under
contention (REQ-ROUTE-C-002); **P0-switch** to the next-best transport on
exhaustion (never held, never overrides the budget); **no runtime override**
exists (Meshtastic `override_duty_cycle_limit` deliberately NOT followed).

**Deliberately NOT relaxed**: duty cycle is never bypassable, including for P0
(WPC violation; LORA.md hard rule — no "emergency exception" to regulations);
LoRaWAN/IN865 (30 dBm EIRP) is a **network-planning parameter for a future
alternate uplink only**, never device power headroom (RES-0027 RQ-1/RQ-3).

**Explicitly deferred / out of scope**: LoRaWAN stack integration + join keys
(DEC-LORA-0006 — future alternate uplink); physical SX1262/E22 firmware +
gateway packaging (BLK-0005 hardware gate; this node's Rust core ships with
`LoRaLinkAdapter` trait += `SimulatedLoRaAdapter` conformance, the
type-checked guarantee per BLE-001 AC-11 pattern); field validation (EXP-LORA-001,
Month 8 — stays SIMULATION_VALIDATED, DEC-LORA-0007).

---

## 2. Wire-constraints and platform contract (RES-0027 → ACs)

### 2.1 Regulatory compliance (RES-0027 RQ-1 / D-7)
- Governing instrument: **WPC G.S.R. 853(E), 10-Dec-2021** — SRD License
  Exemption Rules for **865–868 MHz**, Table-I Non-Specific SRD: **25 mW e.r.p.
  (≈14 dBm)**, **duty-cycle limit 1% (applied to the whole transmission)**,
  EN 300 220 (ETSI harmonized). Supersedes the 2005 RFID 865–867 rules
  (L1, thc.nic.in/G25977.pdf).
- The `LORA.md` legacy "1W (30 dBm)" = **LoRaWAN IN865 network-plan uplink**
  (RP002/TTN: 865.0625–867.1375 MHz, 8 ch, 30 dBm EIRP, 1% duty) — NOT a
  device-level license-exempt emission limit. FC-1 applied (LORA.md Table now
  25 mW/1%). REG-NOTES carry: LoRa-leg-only license-exempt band.
- Field config pin: **866.0 MHz, ≤25 mW e.r.p.** (FIELD_OPERATIONS CONFIRMED
  stands; FC-4). Waveshare **868M** HAT variant → programmed to 866.0 MHz.
- **`ComplianceConfig`** (D-7): `{ freq_hz: 866_000_000, max_erp_dbm: 14,
  duty_percent: 1.0, window_ms: 3_600_000, airtime_budget_ms: 36_000 }`,
  consumed by `DutyCycleTracker` (D-2). Airtime accounting counts the **whole
  packet on-air time** at SF/BW/CR — not just payload bytes — per Table-I's
  "duty-cycle limit on the entire transmission".
- WPC-ETA per-SKU + import licence + TEC ER Annexure-G5 reconciliation (TEC
  Dec-2024 still lists 865–867 MHz — lag G-1) = **LEGAL-001 carry** (hardware
  procurement gate), not a software gate. LoRaWAN/IN865 documented as future
  alternate uplink only (DEC-LORA-0006).

### 2.2 Duty-cycle enforcement (RES-0027 RQ-2 / D-2)
- `DutyCycleTracker` with **rolling (sliding) 1-hour window**, **36,000 ms
  airtime budget** (1% of 3,600,000 ms). Surfaces:
  `check_and_consume(class, airtime_ms) -> Result<NextWindow, BudgetExhausted>`
  returning the **next legal send time** (`next_send_window`) per **RES-0008
  R7** — the router/transport schedules around the window instead of failing
  the send; refusal is **silent-drop/reject** semantics (never panic the
  caller; the routing layer re-routes — RFC 9171 §6.9 analogy, RQ-2 f.5).
- **Priority-proportional airtime under contention** (REQ-ROUTE-C-002): P0 60%,
  P1 25%, P2 10%, P3–P7 share 5%. When the budget recovers, the **backlog
  queue** (REQ-ROUTE-C-003) drains in priority order (recovery ≈1% per 100 s
  of silence).
- **P0 on exhaustion**: switches to the next-best available transport (BLE /
  Wi-Fi / Cellular / Satellite) — P0 is **never held or dropped**, and never
  overrides the duty budget.
- **No runtime override**: no config key, no debug switch equiv of Meshtastic
  `override_duty_cycle_limit` (L4 discussion #3725 — rejected, IRIS has no
  such switch). **Global per-device budget** across all 865–868 TX (device-wide,
  not per-socket).

### 2.3 Radio parameters (RES-0027 RQ-3 / D-4)
- Default link **`RadioProfile { sf: 9, bandwidth_khz: 125, coding_rate: 5
  (CR4/5), freq_hz: 866_000_000 }`**; **`P0_RADIO_PROFILE`** = SF12, same
  BW/CR (sensitivity −136 dBm vs −129 dBm at SF9 → ≈+7 dB P0 link budget at
  ~4× airtime — acceptable: P0 is rare and P0-switch protects it).
- Airtime: LoRa symbol math (`Tsym = 2^SF/BW`; preamble + payload symbols;
  CR4/5 → 4/(4+CR) coding overhead); **per-SF/BW/CR airtime table as a pure
  fn** + unit tests binding the 36 s/h budget math (Semtech AN1200.13
  formula: `(preamble + 4.25 + Nsym) · Tsym`, CRC on, explicit header).
  **CORRECTED at IMPLEMENT (iter ~163)**: the research-era "SF9 255-B ≈
  1.82 s" figure was arithmetically wrong — the canonical formula gives
  **≈1.25 s for a 255-B payload** and **≈1.17 s for the 237-B envelope
  frame** (code anchors: `airtime_ms(SF9, 255) = 1251 ms`,
  `airtime_ms(SF9, 237) = 1169 ms`, `airtime_ms(SF12, 237) = 8528 ms`).
  **CORRECTED at SECURITY_REVIEW (iter ~165, RT-101 CRITICAL)**: the DUTY
  BILLING BASIS is the full on-air frame (`HEADER_LEN + envelope`),
  never the payload alone — payload-only billing under-counted small
  frames up to ~79% and made a Table-I exceedance reachable. Transport
  anchors are therefore `airtime_ms(SF9, 255 B frame) = 1251 ms` and
  `airtime_ms(SF12, 255 B frame) = 9020 ms`; SLIP escapes are carrier
  framing, not on-air, and are not billed.
- **BW500 gateway note**: SF9/BW500 ≈ 4× shorter airtime at ~6 dB sensitivity
  cost — a plausible gate-to-gate uplink option; included in the airtime table,
  **not** a v1 default.
- Message size: **255-B on-air frame cap** = **18-B IRIS header**
  (`[ver u8][prio u8][msg_id 16B]`) + **≤237-B envelope payload** — the
  `P0_MAX_ENVELOPE_BYTES` (237-B V001 envelope) fits exactly. Envelopes
  >237 B reuse the existing **5×51 B fragmentation** (RES-0003) — no new
  fragmentation code. `TransportCapabilities::max_message_size` reports the
  **envelope capacity (237)** so manager gating excludes oversized
  unfragmentable requests; the PHY frame stays ≤255 B.

### 2.4 Bridge protocol (RES-0027 RQ-4 / D-3)
- **BLE GATT primary** bridge (phone as gateway): three characteristics —
  **LORA_TX** (Write, ≤255-B ATT value ≤512-B negotiated MTU — single ATT
  write, no GATT fragmentation), **LORA_RX** (Notify — module RX -> app),
  **LORA_STATUS** (Read/Notify — RSSI/SNR/duty-remaining/battery). The app
  never opens the radio itself; the module advertises the IRIS LoRa bridge
  service.
- **USB-SLIP alternative** (RPi/desktop gateways, CDC-ACM): `frame.encode_slip()`
  (RFC 1055-style END/ESC framing) over the serial path; deterministic
  latency, no BLE renegotiation.
- **`LoRaFrame` encode/decode** — the shared frame format used by both carriers
  (one serializer, two transports).
- **Status telemetry** aggregates module diagnostics (SX1262 RSSI/SNR registers
  / E22 AT responses / voltage sense) into a status packet surfaced via
  LORA_STATUS + OBS-001 `iris.transport.lora.*`.
- **`LoRaLinkAdapter` trait** (D-1/D-3): `open/close/tx/rx/status` —
  abstracts **AT-serial vs SPI-native** firmware surfaces (E22 = AT/UART,
  SX1262 HAT = SPI). Rust core never depends on a module-family API.

---

## 3. Security posture (RES-0027 RQ-5 → ACs)

1. **App-layer envelope crypto is the trust anchor** (CRYPTO-001 sign+encrypt,
   verify-before-forward, SEC-001 replay/dedup/rate gates) — unchanged, no new
   keys/rotations for LoRa (RES-0027 RQ-5 f.5). **P0-never-encrypted** policy
   (FIELD_OPERATIONS) is a routing-level rule, unchanged and transport-
   agnostic — the transport simply carries envelopes.
2. **Duty tracker is the LoRa convergence-layer rate limit** — a flood-defense
   control AND a legal floor; circumvented duty enforcement is the adversarial
   risk (Meshtastic override precedent REJECTED; no switch to attack).
3. **Defensive parse** of LoRaFrame + status telemetry (malformed/truncated/
   oversized rejected without panic/alloc blowup) — pattern carries from
   BLE-002 AC-6.
4. **Multipath dedup is the integrity net**: a message delivered over BLE and
   LoRa is deduplicated once at the envelope layer (envelope hash) — LoRa never
   bypasses dedup/ACK/dispatchgates (MSG-001).
5. **No unauthenticated-trigger actions**: LoRa frames always flow through
   envelope verify + engine gates before any effect.
6. **Hardware-trust honesty**: simulated link-budget outputs are
   **SIMULATION_VALIDATED** only (never field claims); physical-device
   coverage is BLK-0005-gated (RES-0027 RQ-6, G-2/G-3).

---

## 4. Module layout (`crates/iris-core/src/transport/lora.rs`)

| Piece | Status | What LORA-001 delivers |
|---|---|---|
| `Transport` trait (mod.rs @315) | COMPLETE | implemented by `LoRaTransport`; `&self` shared-arc hot-plug register/deregister (manager.rs register @84 / deregister @109 — dongle plug/unplug surface already documented "LoRa dongle hot-plug") |
| `TransportCapabilities` (@150) | COMPLETE | LoRa row: `max_message_size: 237` (envelope capacity inside the 255-B PHY frame; 18-B IRIS header), `supports_broadcast: true`, unicast true, multicast false, `range_m_{min,max,typical}: 2000/15000/5000`, `typical_throughput_bps: 1_757` (raw SF9/BW125/CR4/5 radio rate; **CORRECTED at IMPLEMENT iter ~163** — the drafted 14_000 exceeded the SF7/BW125 ceiling ≈5.5 kbps; sustained under 1% duty is far lower), `typical_latency_ms: 1500`, `requires_infrastructure: false`, bg flags platform-bridge-mediated, `requires_special_hardware: true`, `cost_class: Free`, **`regulatory_band: Some("WPC 865-868 MHz SRD (G.S.R. 853(E) 2021), ≤25 mW e.r.p., ≤1% duty")`** (esp. the SubGHz row — RES-0027 RQ-5) |
| `TransportCostClass` (@181) | COMPLETE | Free (already names LoRa) |
| `LORA_COST` (@244) | COMPLETE | already present (scan 1.5 mA / ad 0.5 / connected 1.2 / tx 10.0/kbps / rx 1.5) |
| `DutyCycleTracker` | **NEW** | rolling 1-h window, 36,000 ms budget, `check_and_consume(class, airtime)->NextWindow` (RES-0008 R7), `duty_cycle_remaining_fraction()`, priority-proportional 60/25/10/5, global per-device, **no override knob**; unit tests (exceed 36 s/h → refused + correct window) |
| `ComplianceConfig` | **NEW** | D-7 rule set (866.0 / ≤25 mW e.r.p. / ≤1% duty) consumed by the tracker |
| `LoRaLinkAdapter` trait | **NEW** | `open/close/tx/rx/status` — the AT-vs-SPI seam (D-1/D-3; BleAdapter-lesson) |
| `AtSerialAdapter` | stub | E22-900M30S UART/AT impl **shape** (BLK-0005-gated real firmware) |
| `SpiNativeAdapter` | stub | SX1262 HAT SPI impl **shape** (BLK-0005-gated real firmware) |
| `SimulatedLoRaAdapter` | **NEW** | deterministic in-memory LoRaLinkAdapter for tests; injectable packet loss / delay / duty-violation to exercise routing (pattern SimulatedBleAdapter/SimP2pCoordinator); carries `SimLoRaConfig` + `SimLinkBudget` (AN1200.13 + Okumura-Hata urban, EXP-LORA-001 calibration file hook) |
| `RadioProfile` / `airtime_ms(...)` | **NEW** | SF9/BW125/CR4/5 default + SF12-P0; pure-fn airtime table (per-SF/BW/CR) bound to the 36 s/h budget |
| `LoRaFrame` encode/decode + `encode_slip` | **NEW** | shared frame format; SLIP (RFC 1055) for USB-CDC-ACM; defensive parse |
| LoRa bridge GATT spec | doc | LORA_TX/RX/STATUS characteristic contract (consumer of LoRaFrame + telemetry) for BLK-0005 gateway firmware |
| `BacklogQueue` | **NEW** seam | REQ-ROUTE-C-003 priority-order drain, recovery ≈1%/100 s of silence; feeds the router seam (D-5) |
| `LoRaTransport` (Transport impl) | **NEW** | wires everything: register/transmit/poll, `duty_cycle_remaining` on the surface (R7), `cost_snapshot`, relay-cadence seam (CONGESTION_CONTROL.md §355 — LoRa = lowest cadence tier), OBS-001 `iris.transport.lora.*` |
| tests | grows | duty unit tests, radio airtime budget tests, sim-link-budget tests, adapter conformance + round-trip tests, adversarial parse tests (AC map, §6) |

**TRANSPORT_ABSTRACTION.md**: LoRa column already current (FC-2 — "WPC 865–868
MHz SRD (G.S.R. 853(E) 2021), 25 mW e.r.p., ≤1% duty"); `TransportManager`
selection honors `max_message_size` gating (manager.rs:142 — >255-B unfragmentable
requests excluded; fragmentable requests pass through to the fragment layer).

---

## 5. Router seam (RES-0027 RQ-5 / D-5)

- **`relay_cadence`** (CONGESTION_CONTROL.md §355, DEC-PILOT-0002 / PILOT-001
  AC-6): LoRa joins the cadence tiers as the **lowest-bandwidth tier** —
  broadcast/telemetry/neighbor-discovery/gossip at worst-case cadence
  multipliers; **exempt**: P0/P1 emergency (EMERG-001 bypass) + directed
  point-to-point. Duty tracker is the hard floor beneath the cadence.
- **Backlog queue** (REQ-ROUTE-C-003 / D-5): held in priority order, drains as
  budget recovers (60/25/10/5 allocation + ≈1%/100 s recovery).
- **OBS-001** `iris.transport.lora.*`: airtime-consumed, duty-remaining,
  packets TX/RX, RSSI/SNR, battery (via LORA_STATUS telemetry).
- **Multipath dedup** + **fragmentation** reused as-is (transport-agnostic
  MSG-001 layers) — no new fragments for LoRa (5×51 B existing).
- **`duty_cycle_remaining`** is added to the LoRa transport surface (R7 seam)
  so the routing engine can move P0 to a full-budget transport on exhaustion.

---

## 6. Acceptance criteria (LORA-001 AC-1..17 — resolves the C2 gap)

Defined in `engineering/PROJECT_GRAPH.yaml` LORA-001 node; each mapped to
evidence at TEST/VERIFY. Pattern BLE-001/BLE-002 AC-1..16.

- **AC-1** Transport trait impl: `LoRaTransport` registers + selects via
  `TransportManager` (standard registry; `max_message_size=255`
  eligibility respected — >255-B unfragmentable requests rejected, fragmentable
  split). Caps SubGHz row + `regulatory_band` (RES-0027 RQ-5/AC-1).
- **AC-2** `DutyCycleTracker` rolling 1-h window + 36,000 ms budget:
  `check_and_consume` enforces Table-I; exceeding 36 s/h → refused + correct
  `next_send_window` (RES-0008 R7). Unit tests (RES-0027 RQ-2).
- **AC-3** Priority-proportional airtime under contention: 60/25/10/5
  (REQ-ROUTE-C-002) enforced by the tracker; **no runtime override exists**
  (no config key / no switch) (RES-0027 RQ-2, Meshtastic override REJECTED).
- **AC-4** P0-switch-on-exhausted: on budget exhaustion the routing layer
  moves P0 to the next-best transport; P0 is never held, never overrides the
  budget; seam type-checked + tested (REQ-ROUTE-C-001/003; RES-0027 RQ-2).
- **AC-5** Backlog queue (REQ-ROUTE-C-003): holds in priority order, drains as
  budget recovers (≈1%/100 s of silence), recovery-path tests (D-5).
- **AC-6** `LoRaLinkAdapter` trait conformance vs `SimulatedLoRaAdapter`
  (type-checked guarantee, pattern BLE-001 AC-11); `AtSerialAdapter` +
  `SpiNativeAdapter` shapes present (BLK-0005 gate recorded) (D-1/D-3).
- **AC-7** Radio profiles: default SF9/BW125/CR4/5 + P0 SF12; `airtime_ms` pure
  fn matches the Semtech AN1200.13 formula (SF9/255-B → 1251 ms, SF9/237-B →
  1169 ms, SF12/237-B → 8528 ms — corrected figures per §2.3), budget math
  bound at 36 s/h; BW500 noted, not default (RES-0027 RQ-3 / D-4).
- **AC-8** Bridge framing: `LoRaFrame` encode/decode + `encode_slip` round-trip
  (RFC 1055), 255-B cap enforced, defensive parse of malformed/truncated/
  oversized frames (no panic/alloc blowup) (D-3; BLE-002 AC-6 pattern).
- **AC-9** Fragmentation + multipath dedup reuse: >255-B messages split via the
  existing **5×51 B** fragment layer; a BLE+LoRa duplicate deduplicates once at
  envelope layer (RES-0003/RQ-5 f.3).
- **AC-10** Relay-cadence seam: LoRa = lowest cadence tier; P0/P1 + directed
  exempt; OBS-001 `iris.transport.lora.*` metrics present (CONGESTION_CONTROL
  §355; D-5).
- **AC-11** No crypto changes: envelope X25519+ChaCha20-Poly1305 carried
  unchanged; P0-never-encrypted policy preserved above the transport
  (RES-0027 RQ-5; §3).
- **AC-12** Sim fidelity: `SimulatedLoRaTransport::new(SimLoRaConfig{...})`;
  `SimLinkBudget` = AN1200.13 sensitivity (−129/−136 dBm) + Okumura-Hata urban;
  EXP-LORA-001 calibration hooks (parameter override); all TEST records tagged
  **SIMULATION_VALIDATED** (RES-0027 RQ-6 / G-2/G-3; D-6).
- **AC-13** Hot-plug lifecycle: register on dongle plug, deregister on unplug
  (manager.rs register/deregister); state machine + `shutdown()` clean
  (RES-0027 RQ-5).
- **AC-14** Doc reconciliation: LORA.md/TRANSPORT_ABSTRACTION/ROUTING_
  REQUIREMENTS/FIELD_OPERATIONS agree with code + RES-0027 (FC-1..FC-4 already
  applied iter ~161; verified this pass). REG-NOTES carry documented.
- **AC-15** (GATED/BLK-0005, recorded): physical-device SX1262/E22 link tests +
  board-level noise-figure + duty cycle under real airtime = known_limitation
  (RES-0027 G-2; hardware procurement + EXP-LORA-001 Month 8).
- **AC-16** SECURITY_REVIEW stage: redteam adversarial findings
  dispositioned/fixed/recorded.
- **AC-17** VERIFY stage: independent verification doc evidence table + verifier
  APPROVE (pattern EMERG-001/SEC-001/BLE-001).

---

## 7. Declared decisions (DEC-LORA-0001..0008 — ratified in DECISIONS.md)

- **DEC-LORA-0001** Field config + compliance: pin **866.0 MHz, ≤25 mW e.r.p.
  (≈14 dBm), ≤1% duty (36 s/h per device)** per WPC G.S.R. 853(E) 2021 Table-I
  (865–868 MHz), as a `ComplianceConfig`; airtime counts the whole transmission.
  WPC-ETA per-SKU + TEC ER Annexure-G5 reconciliation = LEGAL-001 carry.
  (RES-0027 RQ-1/D-7; REG-NOTES carry)
- **DEC-LORA-0002** `DutyCycleTracker` = rolling 1-h window, 36,000 ms budget,
  `check_and_consume -> next_send_window` (RES-0008 R7), global per-device,
  **no runtime override** (Meshtastic `override_duty_cycle_limit` NOT followed).
  (RES-0027 RQ-2/D-2)
- **DEC-LORA-0003** Priority-proportional airtime 60/25/10/5 under contention
  (REQ-ROUTE-C-002); **P0-switch** to next-best transport on exhaustion — P0
  never held, never overrides the budget. (RES-0027 RQ-2/D-2)
- **DEC-LORA-0004** Radio params: default SF9/BW125/CR4/5, P0 SF12 (+7 dB);
  255-B cap; >255-B via existing 5×51 B fragmentation; BW500 gateway note only.
  (RES-0027 RQ-3/D-4)
- **DEC-LORA-0005** Bridge: BLE GATT primary (LORA_TX/RX/STATUS, ≤255-B ATT
  ≤512-B MTU) + USB-SLIP alt (CDC-ACM, RFC 1055); `LoRaLinkAdapter` AT-vs-SPI
  seam so Rust core never depends on a module-family API (BleAdapter lesson).
  (RES-0027 RQ-4/D-3)
- **DEC-LORA-0006** Raw-LoRa P2P for v1; **LoRaWAN/IN865 = future alternate
  uplink only** (network-plan figure, NOT device power headroom). (RES-0027
  RQ-1/RQ-3)
- **DEC-LORA-0007** Sim fidelity: `SimulatedLoRaTransport` + `SimLinkBudget`
  (AN1200.13 + Okumura-Hata urban), EXP-LORA-001 calibration hooks; all LoRa
  test evidence tagged **SIMULATION_VALIDATED** until real hardware (BLK-0005/
  GAP-004). (RES-0027 RQ-6/D-6)
- **DEC-LORA-0008** No crypto changes: envelope X25519+ChaCha20-Poly1305
  transported unchanged; P0-never-encrypted preserved above the transport; duty
  tracker is the LoRa convergence-layer rate limit (RFC 9171 §6.9 analogy).
  (RES-0027 RQ-5; §3)

---

## 8. Risks / known limitations (recorded, not blockers)

- Physical-device LoRa link/duty/battery measurement gated (BLK-0005 / GAP-004);
  SX1262 HAT board-level noise-figure needs measurement at EXP-LORA-001 before
  the airtime table is hardware ground-truth (RES-0027 G-2).
- Okumura-Hata applicability at 866 MHz urban Mumbai is empirical; EXP-LORA-001
  PDR data is authoritative (RES-0027 G-3).
- Bandwidth estimate (≈14 B/s framing peak / ≈1 B/s average at 1% duty) is
  derived, not measured (RES-0027 G-5) — refine from EXP-LORA-001 traces at
  DEPLOY.
- TEC ER Annexure-G5 lists LoRa under 865–867 MHz (lag vs 2021 rules) —
  certification reconcile at ETA time (RES-0027 G-1, LEGAL-001 carry).
- Module-A FCC/EU/IN band-scan overlap: procurement must verify the
  India-enabled variant + 866-MHz antenna per SKU (RES-0027 G-4, LEGAL-001).
- LoRa remains excluded from the v1 PILOT-001 mesh (GAP-004); this node's
  simulated wiring + design is the prerequisite for a later LoRa pilot leg.
- `duty_cycle_remaining` added to the LoRa transport surface is a LoRa-local
  seam; it does not change the base `Transport` trait (no cross-transport
  binary change).

---

**Next**: IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT
scope = pure-Rust `crates/iris-core/src/transport/lora.rs` (DutyCycleTracker,
ComplianceConfig, LoRaLinkAdapter + simulated conformance, RadioProfile/
airtime, LoRaFrame + SLIP, BacklogQueue, SimulatedLoRaTransport + SimLinkBudget,
LoRaTransport impl) — platform-neutral, no new crate; physical SX1262/E22
firmware + bridge packaging under the BLK-0005 hardware gate.