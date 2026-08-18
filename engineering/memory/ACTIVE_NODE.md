# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-18T22:00:00Z

## Active Node: IOS-001 — iOS Platform Integration

- **Type**: PLATFORM (Xcode/Swift shell, CoreBluetooth device layer, FFI)
- **Priority**: P0
- **Status**: **DISCOVERED** — DISCOVER next (iter ~141)
- **Deps**: BLE-002 — **COMPLETE** (ACCEPTED iter ~140; eligible)
- **C2 gap**: open (no ACs) — RESOLVED at DESIGN (iter ~142, pattern BLE-002)

## BLE-002 ACCEPTED (iter ~140) — carries forward

- **27 COMPLETE nodes**; independent verifier **APPROVE** (iter ~139).
- **Baseline**: workspace **647/0/1**, `transport::ble` **56/0** (42 BLE-001 +
  11 BLE-002 + 3 BLE-RT), clippy **0**, fmt **clean**.
- **iOS carry-forward contracts** into IOS-001 (documented in
  BLE_002_DESIGN.md + PROJECT_GRAPH known_limitations):
  - **BLE-RT-C003**: no read-timeout/cancellation on the `gatt_read` path in
    the pure-Rust core — mitigated by the 8-probe/scan budget; hard contract
    item for the Swift/CoreBluetooth read path.
  - **BLE-RT-C004**: no probe-admission cache — IOS-001/ANDROID-001 follow-up.
  - **Android↔iOS identify-read vector**: `BleBridge` returns `DeviceNotFound`
    until Android FFI projects a read op (ANDROID-001 follow-up).
- **IOS-001 scope** (from BLE_002_DESIGN.md): Swift `IosBleAdapter`
  implementing the 10-op `BleAdapter` trait (FFI seam) — CoreBluetooth
  `CBCentralManager`/`CBPeripheralManager`, both `UIBackgroundModes`,
  `readValue(for:)` → `gatt_read`, state restoration re-start in
  `willRestoreState`, Live Activity screen-on framing, `maximumWriteValueLength`
  read-only MTU, iOS 16.x 20-B guard, no-broadcast-of-service-data.

## Node context

- **Why now**: BLE-002 COMPLETE (iter ~140) unblocked the P0 platform node.
  PRIORITY_POLICY P0-first: platform apps before P2 transports.
- **Pipeline order**: BLE-002 → **IOS-001** → PILOT-001 (first deployment
  critical path). P2 transports LORA-001/SAT-001 deferred (P0-first).
- **Reference pattern**: **ANDROID-001** (COMPLETE) — platform shell +
  UniFFI/foreign-trait adapter + lifecycle + platform doc reconciliation;
  **BLE-002** — the trait + wire contract IOS-001 implements.
- **iOS surface (RES-0024, SOTA through iOS 26)**: no background BLE
  relaxation; backgrounded iOS undetectable-by-iOS (overflow-area, iOS-iOS
  only) but limited-detectable by Android; ~8-connection ceiling;
  `maximumWriteValueLength` = negotiated ATT payload cap 512 (no request API);
  state restoration does NOT auto-resume scan/advertise; user force-quit
  disables relaunch.

## Next (short)

DISCOVER (iter ~141, IOS-001_DISCOVER.md — catalog iOS shell + FFI contract
surface + C2 gap) → DESIGN (iter ~142, AC-1..n + DEC-IOS-xxx) → RESEARCH if
open questions → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT →
PILOT-001. Baseline **647/0/1** clippy 0 rustfmt clean held.