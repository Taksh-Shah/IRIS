# IOS-001 DISCOVER / UNDERSTAND — iOS Platform Integration

**Document ID**: IRIS-IOS-001-DISCOVER-001
**Version**: 1.0
**Node**: IOS-001 (P0 PLATFORM, deps BLE-002/MSG-001/EMERG-001 all COMPLETE)
**Date**: 2026-08-18
**Iteration**: ~141
**Stage**: DISCOVER → UNDERSTAND COMPLETE (read-only pass; workspace baseline
647/0/1 clippy 0 rustfmt clean **held**)

---

## 1. Node definition (PROJECT_GRAPH line ~736)

- **IOS-001** — "iOS Platform Integration", type **PLATFORM**, priority **P0**,
  status **DISCOVERED**, description "iOS app: Swift/SwiftUI, CoreBluetooth,
  MCF, background execution".
- **dependencies**: `["BLE-002", "MSG-001", "EMERG-001"]` — **all COMPLETE**
  (BLE-002 ACCEPTED iter ~140; MSG-001/EMERG-001 COMPLETE) → node **eligible**.
- **C2 gap**: **CONFIRMED** — no `acceptance_criteria` present → **RESOLVE at
  DESIGN** (pattern BLE-002/ANDROID-001: DESIGN defines AC-1..n).
- **No `ios/` crate/dir exists yet** (confirmed via glob) — platform scaffold
  is net-new under IOS-001 (pattern `crates/iris-android` + `android/` under
  ANDROID-001).

## 2. What already exists (inherited surface)

| Artifact | Status | Relevance to IOS-001 |
|---|---|---|
| `BleAdapter` trait + `SimulatedBleAdapter` (crates/iris-core/src/transport/ble.rs) | COMPLETE (BLE-001/BLE-002) | The FFI contract; doc-comments already name `IosBleAdapter (Swift/FFI)`. **Now 11 ops** (10 BLE-001 + `gatt_read` added at DEC-BLE-002-0015). |
| `BLE_002_DESIGN.md` §5 | COMPLETE | **iOS FFI adapter contract table** mapping 10 ops → CoreBluetooth; **needs `gatt_read` row** (`readValue(for:)` + `didUpdateValueFor` callback) + connect-to-identify flow + BLE-RT-C003 timeout contract. |
| `docs/platforms/IOS.md` (318 lines) | COMPLETE | Full iOS design: min iOS 14 / target 17, Xcode 15+, Swift 5.9+, SwiftUI + MVVM, Core Data, CoreBluetooth managers, MCF supplement, Network.framework, BGTaskScheduler, UniFFI IrisCore.swift + XCFramework layout. |
| `docs/implementation/SWIFT_LAYER.md` | exists | MVVM + Swift Concurrency + Combine; CoreBluetooth Manager section (§165); Swift-Rust FFI usage (§331). |
| `docs/implementation/RUST_CORE.md` | COMPLETE | UniFFI generates `IrisCore.swift` (Swift, via C ABI) — the iOS binding surface. |
| `docs/platforms/ANDROID.md` + `ANDROID_DESIGN.md` | COMPLETE | **Reference platform pattern**: UniFFI foreign-traits injection, adapter lifecycle, identity, FGS/background, doc recon. iOS analog to be defined. |
| `IDENT_DESIGN.md` §keychain | COMPLETE | "iOS (later, IOS-001): identity/X25519 in Keychain" — **open design Q**: Keychain Secure Enclave = ECDSA P-256 only; Android Keystore = Ed25519 (TEE). Cross-platform signature scheme must reconcile (CryptoKit Ed25519 vs Secure Enclave). |
| `RES-0024` (iOS BLE SOTA) | COMPLETE | No background relaxation through iOS 26; overflow-area iOS-iOS; ~8-connection ceiling; `maximumWriteValueLength` cap 512; state restoration no auto-resume; Live Activity screen-on. |
| `docs/transports/BLE.md` §iOS (254-360) | COMPLETE | iOS scan/advertising/MTU/fragmentation docs; will be reconciled at DOC stage. |
| `.github/workflows/ci.yml` | COMPLETE | TEST-001 matrix — **iOS build requires a macOS runner** (see env-gate RQ below). |

## 3. iOS-specific platform surface (the IOS-001 design surface)

**FFI / adapter seam (from BLE_002_DESIGN.md §5, + gatt_read)**
- `IosBleAdapter` (Swift, w/ CoreBluetooth) must implement the `BleAdapter`
  trait: `start_scan` → `scanForPeripherals(withServices:[IRIS_SERVICE_UUID])`;
  `stop_scan` → `stopScan()`; `start_advertising` → `startAdvertising(
  [ServiceUUIDsKey],[LocalNameKey ≤10 B])` — service_data NEVER transmitted;
  `connect_gatt` → `connect(peripheral)`; `disconnect_gatt` →
  `cancelPeripheralConnection`; `gatt_write` → `writeValue(.withResponse)`;
  `set_mtu` → read `maximumWriteValueLength(for:.withResponse)` (no request);
  `incoming_gatt_writes` → `didReceiveWrite` CBATTRequest drain; `scan_results`
  → `didDiscover` (no service data); **`gatt_read` → `readValue(for:)` +
  `centralManager(_:didDiscoverServices:) -> didDiscoverCharacteristics -> periph
  eral(_:didUpdateValueFor:)` with call-timeout** (BLE-RT-C003 carry-forward).
- Lifecycle contract: both UIBackgroundModes; restoration identifiers
  "IrisCentralManager"/"IrisPeripheralManager"; `willRestoreState` MUST
  re-start scan/advertise; never init BLE outside valid launch path.

**Identity (open design surface)**
- Android: Keystore Ed25519 (TEE/StrongBox, API 33). iOS options:
  Secure Enclave (ECDSA P-256 only, native, hw-backed) vs CryptoKit Ed25519
  (app-layer, key persisted in Keychain, portability-class common key). The
  envelope/crypto core is Ed25519-based (CRYPTO-001) → **Ed25519 key in
  Keychain** is the compatible path; Secure Enclave ECDSA would need a second
  key — decision at RESEARCH/DESIGN (RQ-IOS-2).

**Platform / background surface**
- Background: BGTaskScheduler (app refresh) + state restoration; MCF is the
  Apple-to-Apple supplement (not BLE substitute); Live Activity (ActivityKit,
  iOS 16.2+) for screen-on foreground framing; Network.framework for Internet
  relay path parity (INTERNET-001).
- Permissions: `NSBluetoothAlwaysUsageDescription`, `NSLocalNetworkUsageDescription`
  (MCF/Network), background modes plist entries.
- Deployment: XCFramework (ios-arm64 + ios-arm64_x86_64-simulator) hosting the
  compiled Rust (`cargo build --target aarch64-apple-ios` via cargo-lipo/
  xcodebuild run script; CROSS for hostless builds).

## 4. Open questions → RESEARCH (iter ~142, RES-0025)

- **RQ-1**: UniFFI **Swift foreign-trait** support current status (ANDROID-001
  proved `#[uniffi::export(foreign)]` + async-over-FFI on Kotlin w/ UniFFI
  0.31.x). Is the Swift side equally mature (protocols → foreign traits,
  callback interfaces, async fns over FFI)? Primary: uniffi-rs docs + Swift
  generation changelog.
- **RQ-2**: iOS **identity key**: Ed25519-in-Keychain (CryptoKit) vs Secure
  Enclave ECDSA — interop with CRYPTO-001 Ed25519 envelope core; best-practice
  key persistence/accessibility (kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly).
- **RQ-3**: **Build/CI environment**: dev host is **Windows** — no Xcode. iOS
  build is **macOS-runner env-gated** (pattern G-TI-1: macOS+Windows behavioral
  legs). Feasible CI matrix + cross-build alternatives (CROSS/aarch64-apple-ios
  on Linux; XCFramework assembly on macOS runner).
- **RQ-4**: **State restoration + background churn** (willRestoreState re-arm,
  relaunch-for-connections only, user force-quit kills relaunch) — do we need a
  `SessionRecovery` Swift component or is BGTaskScheduler enough (RES-0024 DI-6
  carry).
- **RQ-5**: **Live Activity / ActivityKit** current iOS 18/26 availability +
  screen-on scan continuation; any new lock-screen BLE controls.

## 5. Known limitations baseline (write to graph at DESIGN/ACCEPT)

- BLK-0005 device gating: CoreBluetooth physical tests need real iPhone
  (Simulator unsupported) — same as BLE-002 AC-14.
- Build/run env-gated: no Xcode on dev host → macOS CI runner required.
- iOS backgrounded = limited relay (connected links persist; no new
  connections/ads); overflow-area iOS-iOS-only.
- BLE-RT-C003/C004 + Android↔iOS identify-read carry-forward contracts land
  here (Swift read-timeout/cancellation; probe-admission cache).

## 6. Stage outcome

- **UNDERSTAND COMPLETE** — reference surface read (BLE_002_DESIGN.md §5,
  IOS.md, SWIFT_LAYER.md, RUST_CORE.md, RES-0024, ANDROID pattern, IDENT
  keychain notes); C2 gap confirmed; `ios/` does not exist (net-new).
- **NEXT**: RESEARCH (iter ~142, RES-0025: UniFFI Swift foreign traits +
  Keychain identity + macOS-CI env + ActivityKit) → DESIGN (iter ~143:
  `docs/implementation/IOS_DESIGN.md`, AC-1..n resolves C2, DEC-IOS-xxxx) →
  IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT → PILOT-001.
- Workspace **untouched** (read-only pass) — baseline 647/0/1 held.