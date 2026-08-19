# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T09:30:00Z

---

## Priority: IOS-001 (P0 platform) — TEST (AC-16..AC-18, iter ~146/147)

**IMPLEMENT COMPLETE (iter ~144..146)** — AC-1..AC-15:
- **Tranche 1 (iter ~144)**: `crates/iris-ios` scaffold — 11-op
  `FfiBleAdapter` incl `gatt_read` + SimBle tests; `BleBridge` implements core
  `BleAdapter` all 11 ops; `IrisEngine` explicit tokio `Handle` (#2576);
  `uniffi-bindgen 0.31.2 generate --library` → committed `ios/IRIS/RustFFI/`
  (`IrisCore.swift` 96.7KB + FFI header + modulemap); `G-IOS_SPIKE.md` records
  Swift-5 pin vs #2929, `@unchecked Sendable`, tokio Handle wiring; `ios.yml`
  pre-seed.
- **Tranche 2 (iter ~145/146)**: **all Swift sources authored** (AC-4..AC-15):
  - `IosBleAdapter.swift` — 11-op FFI adapter, `@unchecked Sendable`,
    NSLock-confined; `gattRead` = connect → `readValue(IRIS_IDENTIFY_CHARACTERISTIC)`
    → `didUpdateValueFor` → `DiscoveryBeacon` parse (AC-5, DEC-BLE-002-0015);
    BLE-RT-C003 10s timeout + cancel (AC-6); C004 probe budget 8/scan + 60s
    rejection window + budget-exhaustion throw (AC-7); MTU negotiate-late ≤512
    via `maximumWriteValueLength` + 20-B iOS 16.x guard (AC-8); scan/advertise
    IRIS_SERVICE_UUID filter mandatory + no `service_data` broadcast (AC-9).
  - `CoreBluetoothSeam.swift` + `CBManagerCentral.swift` + `CBManagerPeripheral.swift`
    (mock seam + real CB managers with `willRestoreState` re-arm — AC-10).
  - `KeychainEd25519.swift` — `com.iris.identity.v1` +
    `com.iris.staticad.v1` (X25519), `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`,
    no biometric, RFC 8032 KAT (AC-12).
  - `SessionRecovery.swift` — launch-reason classify + BGTask re-submit every
    launch + force-quit no-op + debounced re-arm (AC-11); `BGTaskWiring.swift`
    (AC-14); `LiveActivityController.swift` iOS 16.1 gated (AC-13) +
    `IrisWidgetExtension/IrisLiveActivityWidget.swift`; `AppDelegate.swift` +
    `IRISApp.swift` wiring.
  - `.github/workflows/ios.yml` full AC-15 (rust-bindings / build-xcframework /
    swift-build / test-macos / test-simulator) + `ios/Scripts/build-xcframework.sh`.
  - Tests authored (AC-16): `MockCoreBluetooth.swift`, `IosBleAdapterTests.swift`
    (AC-4..9), `ProbeAdmissionTests.swift` (AC-7), `KeychainIdentityTests.swift`
    (AC-12 KAT), `SessionRecoveryTests.swift` (AC-10/11), `FfiConformanceTests.swift`
    (AC-3).
- **Baseline: workspace 658/0/1** (iris-core 567 + iris-ios 11), clippy 0,
  fmt clean — held.
- **Env-gated**: Swift compile/run = macOS CI leg (no Xcode/swiftc on Windows
  dev host); physical-device rows = BLK-0005 (iOS Simulator lacks CoreBluetooth).

## Next Action (iter ~146/147 — IOS-001 TEST, AC-16..AC-18)

Author `IOS-001_TEST.md` mapping AC-16..AC-18 to evidence (pattern
ANDROID-001_TEST.md):
- **AC-16**: macOS-host unit tests with IOS-CoreBluetooth-Mock — the 6 authored
  test files (IosBleAdapterTests AC-4..9, ProbeAdmissionTests AC-7,
  KeychainIdentityTests AC-12 RFC 8032 KAT, SessionRecoveryTests AC-10/11,
  FfiConformanceTests AC-3). Execution env-gated → `test-macos` job on macOS
  CI runner (workspace `ios/Tests`).
- **AC-18**: baseline held — workspace **658/0/1**, clippy **0**, fmt clean
  (verified on host this pass).
- **AC-17** (DOCUMENT stage carries the 6 EXTERNAL-FACTS corrections) and
  **AC-19** (GATED/BLK-0005) recorded in evidence.

→ **DOCUMENT (AC-17)** → SECURITY_REVIEW → VERIFY → ACCEPT → PILOT-001.

## Context

Previous action (iter ~145/146): IMPLEMENT tranche 2 — PROJECT_GRAPH IOS-001
evidence updated, workspace **658/0/1**. 27 COMPLETE nodes. Pipeline:
**IOS-001 → PILOT-001**.