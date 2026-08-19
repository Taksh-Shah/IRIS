# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T09:30:00Z

## Active Node: IOS-001 — iOS Platform Integration

- **Type**: PLATFORM (Xcode/Swift shell, CoreBluetooth device layer, FFI)
- **Priority**: P0
- **Status**: **IMPLEMENTING** — IMPLEMENT + TEST + DOCUMENT COMPLETE
  (iter ~144..148); **SECURITY_REVIEW next (iter ~149, AC-20)**
- **Deps**: BLE-002 / MSG-001 / EMERG-001 — **all COMPLETE** (eligible)
- **C2 gap**: **RESOLVED** at DESIGN (iter ~143) — AC-1..AC-21 defined

## IMPLEMENT COMPLETE (iter ~144..146)

### Tranche 1 (iter ~144) — Rust leg
- **AC-1**: `crates/iris-ios` scaffold — 11-op `FfiBleAdapter` (incl
  `gatt_read` + SimBle tests), `BleBridge` implements core `BleAdapter` all
  11 ops, `IrisEngine` explicit tokio `Handle` (#2576) + `BleTransport::new_ios`
  + `MessageEngine` + inbox forwarder.
- **AC-2**: `uniffi-bindgen 0.31.2 generate --library` → committed
  `ios/IRIS/RustFFI/` (`IrisCore.swift` 96.7KB + `IrisCoreFFI.h` + modulemap).
- **AC-3**: Rust leg 11/11 PASS; Swift leg env-gated → `G-IOS_SPIKE.md`.

### Tranche 2 (iter ~145/146) — Swift app + adapter layer (AC-4..AC-15)
- **Seam**: `Services/CoreBluetoothSeam.swift` (BleCentralSeam/BlePeripheralSeam
  + delegates + IOSBleCentralState) + `CBManagerCentral.swift`/`CBManagerPeripheral.swift`
  (real CB managers, `willRestoreState` re-arm).
- **AC-4..AC-9**: `Services/IosBleAdapter.swift` — 11-op FFI adapter
  (`@unchecked Sendable`, NSLock-confined); AC-5 gatt_read = connect →
  `readValue(IRIS_IDENTIFY_CHARACTERISTIC)` → `didUpdateValueFor` →
  `DiscoveryBeacon` parse (DEC-BLE-002-0015); AC-6 BLE-RT-C003 10s timeout +
  cancel; AC-7 C004 probe budget 8/scan + 60s reject window + budget-exhaustion
  throw; AC-8 MTU ≤512 via `maximumWriteValueLength` + 20-B guard; AC-9 UUID
  filter mandatory + no `service_data`.
- **AC-10/AC-11**: Info.plist UIBackgroundModes ×2 + stable restoration IDs +
  `willRestoreState` re-arm + `session/SessionRecovery.swift` (launch-reason
  classify, debounced re-arm, resubmit every launch, force-quit no-op).
- **AC-12**: `identity/KeychainEd25519.swift` (`com.iris.identity.v1` +
  `com.iris.staticad.v1`, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`,
  no biometric, RFC 8032 KAT).
- **AC-13/AC-14**: `Services/LiveActivityController.swift` (iOS 16.1 gated) +
  `IrisWidgetExtension/` + `Services/BGTaskWiring.swift` + `Notifications.swift`.
- **AC-15**: `.github/workflows/ios.yml` full workflow (rust-bindings /
  build-xcframework / swift-build / test-macos / test-simulator) +
  `ios/Scripts/build-xcframework.sh`.
- **AC-16 (authored)**: Tests/ — `MockCoreBluetooth.swift` + 5 test files
  (IosBleAdapterTests, ProbeAdmissionTests, KeychainIdentityTests,
  SessionRecoveryTests, FfiConformanceTests).
- **Baseline: workspace 658/0/1**, clippy 0, fmt clean — held.

## TEST COMPLETE (iter ~147, AC-16..AC-18)

- **AC-16**: `IOS-001_TEST.md` — 33 Swift XCTest cases across 6 files
  (MockCoreBluetooth + IosBleAdapterTests 13 AC-4..9 + ProbeAdmissionTests 3
  AC-7 + KeychainIdentityTests 5 AC-12 KAT + SessionRecoveryTests 9 AC-10/11 +
  FfiConformanceTests 3 AC-3); execution env-gated → `test-macos` CI job
  (IRIS-MacOSTests scheme).
- **AC-18**: PASS — live re-verify: workspace **658/0/1**, clippy 0, fmt clean.
- **AC-19**: GATED/BLK-0005. Commit `110ef5b`.

## DOCUMENT COMPLETE (iter ~148, AC-17)

All 6 EXTERNAL-FACTS corrections applied (docs-only, baseline 658/0/1 held):
IOS.md Xcode 16+ / `generate --library` / Keychain Ed25519 rewrite / BGTask
sharpening; SWIFT_LAYER.md willRestoreState re-arm; DISCOVER 16.2→16.1.

## Next (short)

SECURITY_REVIEW (iter ~149, AC-20 — redteam on `IosBleAdapter`, seam, Keychain
identity, SessionRecovery/BGTask/LiveActivity) → VERIFY (AC-21) → ACCEPT →
PILOT-001. Baseline **658/0/1**.

## Carry-forward contracts into IOS-001 (from BLE-002)

- **BLE-RT-C003**: read-timeout/cancellation — delivered in Swift `gattRead`
  (10s timeout + cancel), AC-6.
- **BLE-RT-C004**: probe-admission cache — delivered (8-probe/scan budget +
  60s rejection window), AC-7.
- **Android↔iOS identify-read**: `IosBleAdapter.gatt_read` = connect → read
  `IRIS_IDENTIFY_CHARACTERISTIC` (AC-5, DEC-BLE-002-0015) — delivered.

## Node context

- **Why now**: BLE-002 COMPLETE (iter ~140) unblocked the P0 platform node.
- **Pipeline**: BLE-002 → **IOS-001** → PILOT-001. P2 transports deferred
  (P0-first).
- **Reference pattern**: **ANDROID-001** (platform shell + UniFFI foreign-trait
  adapter + lifecycle + doc reconciliation); **BLE-002** (trait + wire contract).
- **Env-gates**: Swift compile/run = macOS CI leg (no Xcode/swiftc on Windows
  host); physical-device rows AC-19 = BLK-0005 (iOS Simulator lacks
  CoreBluetooth).