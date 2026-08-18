# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-18T22:00:00Z

---

## Priority: IOS-001 (P0 platform) — DISCOVER stage (iter ~141)

**BLE-002 ACCEPTED (iter ~140, 27 COMPLETE nodes)** — full pipeline
(DISCOVER→RESEARCH→DESIGN→IMPLEMENT→TEST→SECURITY_REVIEW→VERIFY→ACCEPT).
Independent verifier APPROVE (iter ~139) after VR-01..VR-06 doc-integrity
closure. Baseline **647/0/1**, `transport::ble` **56/0**, clippy **0**, fmt
clean. **NODE_TRANSITION → IOS-001.**

## Next Action (iter ~141 — IOS-001 DISCOVER)

Write `engineering/memory/records/IOS-001_DISCOVER.md` (pattern
BLE-002_DISCOVER.md / ANDROID-001_DISCOVER.md):

- **Reference surface**: BLE_002_DESIGN.md iOS FFI adapter contract table
  (10-op `IosBleAdapter` → CoreBluetooth mapping), `BleAdapter` trait
  (ble.rs — now with `gatt_read`), docs/platforms/IOS.md, docs/transports/
  BLE.md §iOS, RES-0024 (iOS BLE SOTA, no bg relaxation through iOS 26).
- **Catalog the iOS shell surface**: Xcode/Swift app scaffold, CoreBluetooth
  central+peripheral managers, UIBackgroundModes (both), state restoration
  (willRestoreState restart, no auto-resume), scanner/Bluetooth permissions,
  Live Activity foreground framing, 8-connection ceiling, MTU = read-only
  maximumWriteValueLength, iOS 16.x 20-B guard.
- **C2 gap** (no ACs) → confirm and resolve at DESIGN.
- **Eligibility**: deps BLE-002 COMPLETE — confirm; known_limitations baseline.
- **Carry-forward contracts** (from BLE-002): read-timeout/cancellation on
  gatt_read path (BLE-RT-C003), probe-admission cache (BLE-RT-C004),
  Android↔iOS identify-read vector (ANDROID-001 follow-up).
- Baseline **647/0/1** clippy 0 rustfmt clean held (docs-only pass).

## Context

Previous action (iter ~140): BLE-002 ACCEPTED — PROJECT_GRAPH COMPLETE
(evidence 7), PROJECT_STATE completed 26→27, CHANGELOG 0.3.57. 27 COMPLETE
nodes. Pipeline: BLE-002 → **IOS-001** → PILOT-001 (first deployment critical
path). P2 transports LORA-001/SAT-001 deferred after platform nodes (P0-first).