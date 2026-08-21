# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T16:30:00Z

---

## Priority: LORA-001 (P2 TRANSPORT) — IMPLEMENT (iter ~163)

**Pipeline POSITION: LORA-001 DESIGNING (iter ~162, `LORA_001_DESIGN.md` v1.0,
AC-1..17, DEC-LORA-0001..0008, evidence 3).** 29 COMPLETE nodes (PILOT-001
ACCEPTED iter ~159). RESEARCH routed iter ~161 (RES-0027 PROCEED, RQ-1..RQ-6
resolved, DESIGN inputs D-1..D-7). Baseline **664/0/1**, clippy 0, fmt clean
held.

## Next Action (iter ~163 — LORA-001 IMPLEMENT, Rust core)

Author the Rust LoRa modules in `crates/iris-core` per
`docs/implementation/LORA_001_DESIGN.md` **AC-1..AC-17** (pattern
BLE-001/WIFIAWARE-001/WIFIDIRECT-001/BLE-002 records + `transport/ble.rs` style):

1. **AC-1** — `LoRaTransport` implementing `TransportAdapter`
   (register/transmit/poll/`duty_cycle_remaining`/`estimated_bandwidth_bps`≈14/
   `max_message_size_bytes`=255); `Caps` SubGHz row; register/deregister on
   dongle plug/unplug seam.
2. **AC-2 / D-2** — `DutyCycleTracker`: rolling 1-h window, 36,000 ms budget,
   `check_and_consume` → `next_send_window` (R7 coupling);
   priority-proportional 60/25/10/5; P0-switch-on-exhausted; **no runtime
   override**; duty-violation unit tests (exceed 36 s/h → refused).
3. **AC-3** — `ComplianceConfig` (866.0 MHz / ≤25 mW e.r.p. / ≤1% duty per WPC
   G.S.R. 853(E) 2021 Table-I) → `next_send_window` gates on duty + compliance.
4. **AC-4 / D-3** — `LoRaFrame` encode/decode + SLIP (RFC 1055) alt; BLE-GATT
   bridge contract (LORA_TX/RX/STATUS ≤255-B ATT ≤512-B MTU) + USB-SLIP
   documented in DESIGN; `LoRaLinkAdapter` AT-vs-SPI trait seam +
   `SimulatedLoRaAdapter` conformance.
5. **AC-5 / D-4** — `RadioProfile` SF9/BW125/CR4/5 default + SF12-P0 (+7 dB);
   airtime pure fn (Semtech formula); 255-B cap + 5×51 B fragmentation reuse.
6. **AC-6 / D-5** — `BacklogQueue` (REQ-ROUTE-C-003); relay-cadence +
   congestion-control seam (CONGESTION_CONTROL.md §355); OBS-001
   `iris.transport.lora.*` metrics; multipath dedup reuse.
7. **AC-7 / D-6** — `SimulatedLoRaTransport` full wiring + `SimLinkBudget`
   (AN1200.13 sensitivity + Okumura-Hata urban); EXP-LORA-001 calibration hooks;
   **SIMULATION_VALIDATED only** (BLK-0005/GAP-004).
8. **AC-15** — workspace green + clippy 0 + baseline **664/0/1** held;
   `cargo test --workspace --all-features`, `cargo clippy --workspace
   --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`.
9. Update PROJECT_GRAPH LORA-001 evidence(4) + stage_note IMPLEMENT COMPLETE +
   validation_status; PROJECT_STATE designing 1→0 implementing 0→1 → then TEST
   (iter ~164).

## Context

29 COMPLETE nodes. Pipeline: BLE-001 → WIFIAWARE-001 → WIFIDIRECT-001 → BLE-002 →
ANDROID-001 → IOS-001 → **PILOT-001 ✅** → **LORA-001** → SAT-001 → mission
milestones (MVP/Alpha/Beta/Pilot). LORA-001/SAT-001 are P2 `requires_hardware:
true` (external LoRa modules / satellite gateway) — GAP-004, BLK-0005-adjacent
device gating applies. Physical LoRa dongle integration rows are env-gated
(no dongle on Windows host); sim + software core rows are host-verified.
LEGAL-001 open questions (LoRa type approval/WPC-ETA, satellite licensing)
remain a carry — RES-0027 RQ-1.

## Recent state files touched

- `docs/implementation/LORA_001_DESIGN.md` (new, iter ~162, evidence 3, AC-1..17)
- `engineering/memory/DECISIONS.md` (DEC-LORA-0001..0008 ratified)
- `engineering/PROJECT_GRAPH.yaml` (LORA-001 RESEARCH_COMPLETE→DESIGNING, AC-1..17,
  evidence 3; evidence(0) corrupt text repaired)
- `engineering/PROJECT_STATE.yaml` (header, research_complete 2→1, designing 0→1,
  testing_health)
- `engineering/memory/execution-state.yaml` (stage DESIGN → IMPLEMENT, iter ~163)
- `engineering/memory/records/execution-log.md` (iter ~162 + ~162b reconciliation)
- `engineering/memory/ACTIVE_NODE.md`, `CURRENT_STATE.md` (DESIGN complete)