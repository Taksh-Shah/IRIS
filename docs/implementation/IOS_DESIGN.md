# IOS-001 Design — iOS Platform Integration v1

**Document ID**: IRIS-IOS-001-DESIGN-001
**Version**: 1.0
**Node**: IOS-001 (P0 PLATFORM, deps BLE-002/MSG-001/EMERG-001 all COMPLETE)
**Date**: 2026-08-18
**Stages**: UNDERSTAND (iter ~141) → RESEARCH (iter ~142, RES-0025) → DESIGN (iter ~143)
**Research basis**: RES-0025 (2026-08-18: UniFFI Swift foreign traits + async-over-FFI; iOS identity key CryptoKit-Ed25519-in-Keychain vs Secure Enclave P-256; macOS-CI env-gate; state-restoration SessionRecovery; ActivityKit Live Activity — verdict **PROCEED**, RQ-1..RQ-5 all ADOPT-WITH-CONDITION, 7 DESIGN inputs, 6 EXTERNAL-FACTS corrections, gaps G-IOS-1..7); `engineering/memory/records/IOS-001_DISCOVER.md` (UNDERSTAND, iter ~141); RES-0024 (iOS CoreBluetooth 2024-2026 SOTA, 7 DESIGN inputs, G1-G5); `docs/implementation/BLE_002_DESIGN.md` (iOS leg transport contract, §5 FFI adapter seam, AC-1..16, DEC-BLE-002-0001..0015); `docs/implementation/ANDROID_DESIGN.md` (reference PLATFORM-node shell pattern: DEC-AND-0001..8, AC-1..15, G-AND-1..6); `docs/platforms/IOS.md` (318-line platform design); `docs/implementation/SWIFT_LAYER.md`; `docs/implementation/RUST_CORE.md`; `engineering/PROJECT_GRAPH.yaml` IOS-001 node (RQ-1..RQ-5, carry-forward contracts); ACCEPTANCE_POLICY.yaml PLATFORM-type additions; DEC-0009 (gates lifted); BLK-0005 (device-test resource gating).
**Absorbs**: RES-0025 RQ-1..RQ-5 answers + 7 DESIGN inputs (§7) + outdated-claims table (§8) + gaps G-IOS-1..7 (§9); BLE-002 carry-forwards **BLE-RT-C003** (read-timeout), **BLE-RT-C004** (probe-admission cache), **DEC-BLE-002-0015** (`BleAdapter::gatt_read` REQUIRED — Android↔iOS identify-read); IOS-001_DISCOVER known-limitations baseline; RES-0024 narrowings carried from BLE-002 (no background relaxation, connect-to-identify, MTU negotiate-late, ~8-peer bound, Live Activity screen-on only).
**Reconciles**: `docs/platforms/IOS.md` + `docs/implementation/SWIFT_LAYER.md` + `docs/implementation/RUST_CORE.md` vs RES-0025 EXTERNAL-FACTS (Xcode 15.0+ build floor STALE → Xcode 16.4/iOS 18 SDK; UDL `generate` command STALE → 0.31 `generate --library`; Secure Enclave section "Same pattern as Android Keystore" CORRECT-but-misapplied → CryptoKit Ed25519 app-layer key; BGProcessingTask wording CONFIRMED+sharpened; SWIFT_LAYER `peripheralManagerDidRestoreState` INCOMPLETE → willRestoreState re-arm; Live Activity floor "16.2+" → **16.1**). Corrections recorded with file:line in §13; actual doc edits are a DOCUMENT-stage follow-up (AC-17).

---

## 1. Scope

Build the **iOS application shell + Swift FFI adapter layer** that hosts the Rust `iris-core` engine on iOS, mirroring the ANDROID-001 shell pattern (`crates/iris-android` + `android/` + committed generated bindings) and the DESKTOP-001 `DesktopEngine` reference embedding (`TransportManager` + register transports + `MessageEngine` + auto inbox forwarder). All protocol/routing/crypto/storage logic stays in Rust (`iris-core` + `iris-storage`); the Swift layer provides the **iOS system-context plumbing** (app lifecycle, background modes, state restoration, Keychain identity, notifications, Live Activity, BGTaskScheduler) and the **platform adapter implementation** (`IosBleAdapter`) injected into Rust via the **UniFFI foreign-trait** seam (RES-0025 RQ-1: Kotlin RES-0022 proof transfers to Swift).

**Nodal type**: PLATFORM (P0) — like ANDROID-001; boundary vs BLE-002 is exact: **the `BleAdapter` trait IS the seam**. BLE-002 delivered the pure-Rust iOS-leg contract (connect-to-identify branch, MTU negotiate-late, `SimulatedBleAdapter` serving the identify characteristic, restoration-seam types, `gatt_read` at DEC-BLE-002-0015) with AC-1..16 COMPLETE. IOS-001 delivers the **Swift/CoreBluetooth implementation of that contract** plus the app shell — nothing changes in the Rust core except a thin `crates/iris-ios` FFI binding crate (mirror `crates/iris-android`).

**Net-new deliverables** (no `ios/` or `crates/iris-ios` exists in-repo today — DISCOVER iter ~141 evidence):

1. `crates/iris-ios` — Rust FFI binding crate: `#[uniffi::export]` `IrisEngine` object + `#[uniffi::export(foreign)]` `BleAdapter` wiring; builds staticlibs for `aarch64-apple-ios` + `aarch64-apple-ios-sim`.
2. `ios/` — Swift app target + widget-extension target + committed UniFFI-generated Swift + XCFramework; `IosBleAdapter` (11-op foreign trait incl. `gatt_read`), CoreBluetooth central/peripheral managers with restoration, `identity/` (Keychain Ed25519 + X25519 static-ad), `SessionRecovery`, BGTask wiring, Live Activity, notifications, Info.plist (`UIBackgroundModes` ×2, `NSSupportsLiveActivities`, usage strings).
3. `.github/workflows/ios.yml` — macOS-runner CI leg (env-gate; dev host is Windows — no Xcode).

**Explicitly deferred** (recorded, not blockers): physical-device CoreBluetooth/battery tests (BLK-0005), Secure-Enclave rescue key (RES-0025 RQ-2), iOS full-background ad acceptance (impossible — DEC-BLE-002-0003), production signing/release automation, store UI flows.

## 2. Design decisions (RES-0025 7 DESIGN inputs absorbed, D-1..D-8)

| # | Decision | Basis (RES-0025) |
|---|----------|-------------------|
| D-1 | **UniFFI 0.31.x Swift seam**: `crates/iris-ios` declares `#[uniffi::export(foreign)]`-style BleAdapter export; generated Swift `protocol BleAdapter`; `IosBleAdapter` = Swift `final class … : BleAdapter, @unchecked Sendable` (generated protocols are `Sendable`); async foreign-trait methods bridge via UniFFI foreign-future/oneshot (Rust does NOT run its own event-loop thread — same proof as Kotlin). | RQ-1 findings 1/2/6/7 (foreign_traits L1, futures L1, #2450) |
| D-2 | **Swift language mode pinned to 5** (`SWIFT_VERSION=5`) — NOT `.v6`/strict concurrency: generated async foreign-trait bindings hard-error (`#SendingClosureRisksDataRace`, mozilla/uniffi-rs#2929 open as of 2026-06-18). Fallback if upgrade forces `.v6`: commit the documented #2929 post-codegen patch (`RustBuffer: @unchecked Sendable` + `@Sendable` helpers). Re-verify on UniFFI 0.32.0 upgrade (G-IOS-1/G-IOS-2). | RQ-1 finding 5 (#2929, #2448) |
| D-3 | **Explicit `tokio::runtime::Handle` injection** into every engine-side async poller — do NOT rely on `#[uniffi::export(async_runtime="tokio")]` (INEFFECTIVE on exported-trait impls, issue #2576; fixed only in 0.32.0 via #2899). Same workaround as DEC-AND-0002. | RQ-1 finding 4 (#2576) |
| D-4 | **FFI contract**: 10-op `BleAdapter` mapping + **`gatt_read` row added** (DEC-BLE-002-0015): connect → `readValue(for: IRIS_IDENTIFY_CHARACTERISTIC)` → `peripheral(_:didUpdateValueFor:)` → shared `DiscoveryBeacon` parse; **BLE-RT-C003** call-timeout; **BLE-RT-C004** probe-admission cache. `IosBleAdapter` re-arms scan/advertise from `willRestoreState`. | RQ-1 design input 2; DEC-BLE-002-0015/0002 |
| D-5 | **Identity (`identity.v1`)** = CryptoKit `Curve25519.Signing.PrivateKey` (Ed25519, RFC 8032 — byte-compatible with Rust `ed25519-dalek`), persisted `rawRepresentation` as `kSecClassGenericPassword`, tag `com.iris.identity.v1`, `kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, **no biometric `kSecAccessControl`** (background `errSecInteractionNotAllowed`); load gated on `isProtectedDataAvailable`. SE ECDSA P-256 rescue key **DEFERRED** (SE is P-256-only — not the identity path; mirrors DEC-AND-0005 StrongBox deferral). Static X25519 advertisement key analogue (`KeychainX25519`) same persistence posture. | RQ-2 findings 1-6 |
| D-6 | **CI env-gate (macOS runner)**: `ios.yml` leg = `runs-on: macos-15` (arm64) + `maxim-lobanov/setup-xcode@v1` `xcode-version: '16.4'` (runner default pin; image updates monthly — G-IOS-6); `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`; staticlibs → `uniffi-bindgen generate --library … --language swift` → `xcodebuild -create-xcframework` (ios-arm64 + ios-arm64_x86_64-simulator) → Swift build + macOS-host unit tests (`IOS-CoreBluetooth-Mock`) + Simulator UI leg. Linux iOS cross-build **rejected** (licensed-SDK workaround; fragile — not worth it). Windows dev host remains the non-iOS env; env-gate recorded (G-TI-1 pattern). | RQ-3 findings 1-6 |
| D-7 | **SessionRecovery** = thin Swift coordinator layered ON `willRestoreState` + BGTaskScheduler (NOT a custom daemon — iOS forbids): launch-reason classification (BLE-restoration / BGTask / user), re-arm scan+advertise on restore with debounce, re-submit BGAppRefresh + BGProcessing at EVERY launch (schedules do not survive force-quit), force-quit-safe no-op, protected-data gating. | RQ-4 findings 1-5 (QA1962, TN3115, forums 685525/809661) |
| D-8 | **Live Activity (ActivityKit)**: `LiveActivityController` — floor **iOS 16.1** (not 16.2), availability-gated; `NSSupportsLiveActivities` (+ `NSSupportsLiveActivitiesFrequentUpdates`) plist + widget-extension target; minimal `ContentState` (status only, **no payload**); `areActivitiesEnabled` + `ActivityAuthorizationError` handled; **screen-on framing only** (stops at screen sleep; iOS 26 screen-on scan continuation is the only background-ish path — RES-0024 carry, L5/G-IOS-3); NEVER a delivery guarantee. | RQ-5 findings 1-5 |

## 3. Architecture / module layout

```
crates/iris-ios/                     # NEW Rust crate (uniffi binding; mirror crates/iris-android)
  src/lib.rs                         # uniffi::setup_scaffolding + crate root
  src/engine.rs                      # IrisEngine #[uniffi::export] object — mirrors DesktopEngine:
                                     #   TransportManager + register injected BleAdapter transport +
                                     #   MessageEngine + auto inbox forwarder + subscribe_inbox stream
  src/ffi/ble_adapter.rs             # #[uniffi::export(foreign)] BleAdapter host wiring (11-op surface)
  src/ffi/error.rs                   # compatible FFI error type (Result contract, ANDROID D-3 pattern)
  Cargo.toml                         # uniffi = "=0.31.2", iris-core, tokio
ios/                                 # NEW iOS app + framework sources
  project.yml                        # XcodeGen spec (versioned; generates IRIS.xcodeproj at scaffold)
  IRIS/                              # Swift app target
    App/IRISApp.swift                # @main SwiftUI entry
    App/AppDelegate.swift            # UIApplicationDelegate — launch-options → SessionRecovery classify
    Services/IosBleAdapter.swift     # 11-op BleAdapter foreign-trait impl over CoreBluetooth (§4)
    Services/CBManagerCentral.swift  # CBCentralManager wrapper: scan/connect/read + willRestoreState
    Services/CBManagerPeripheral.swift # CBPeripheralManager wrapper: GATT server + advertise +
                                     #   willRestoreState (re-arm + debounce)
    Services/SessionRecovery.swift   # launch-reason coordinator (D-7, §6)
    Services/BGTaskWiring.swift      # BGAppRefresh + BGProcessing register / resubmit / expire (§6)
    Services/LiveActivityController.swift # ActivityKit (iOS 16.1+ availability-gated; §7)
    Services/Notifications.swift     # UNUserNotificationCenter (standard + critical-alert seam;
                                     #   entitlement-gated, fallback standard sound)
    Identity/KeychainEd25519.swift   # CryptoKit Ed25519 identity.v1 provisioning/load (D-5, §5)
    Identity/KeychainX25519.swift    # static advertisement X25519 key (X25519StaticAd-ios analogue)
    RustFFI/                         # UniFFI-generated Swift — COMMITTED (IrisCore.swift, IrisCoreFFI.h)
    Resources/Info.plist             # UIBackgroundModes [bluetooth-central, bluetooth-peripheral,
                                     #   fetch, processing]; NSSupportsLiveActivities(+FrequentUpdates);
                                     #   NSBluetoothAlways/NSLocalNetwork/NSLocationWhenInUse usage strings
  IrisWidgetExtension/               # Live Activity widget-extension target (ActivityKit)
  IrisFramework/IrisCore.xcframework # UniFFI-generated XCFramework — COMMITTED (ios-arm64 +
                                     #   ios-arm64_x86_64-simulator slices)
  Scripts/build-xcframework.sh       # cargo build per slice → uniffi-bindgen generate --library →
                                     #   xcodebuild -create-xcframework (CI-leg)
  Tests/                             # macOS-host unit-test target (XCTest; IOS-CoreBluetooth-Mock)
    IosBleAdapterTests.swift         #  11-op conformance + gatt_read flow + timeouts (AC-4..6)
    ProbeAdmissionTests.swift        #  BLE-RT-C004 budget/admission (AC-7)
    KeychainIdentityTests.swift      #  Ed25519 round-trip + accessibility + KAT byte-compat (AC-12)
    SessionRecoveryTests.swift       #  launch-reason + re-arm + BGTask resubmit (AC-10/11)
    FfiConformanceTests.swift        #  Swift-5-mode Sendable conformance + engine calls (AC-3)
.github/workflows/ios.yml            # NEW CI leg (macOS runner matrix; §10/AC-15)
```
## 4. iOS FFI adapter contract (`IosBleAdapter` — the seam; BLE_002_DESIGN.md §5 + DEC-BLE-002-0015)

`BleAdapter` trait (11 ops) IS the FFI contract. `IosBleAdapter` (Swift/CoreBluetooth) implements:

| Trait op | iOS/CoreBluetooth mapping |
|---|---|
| `start_scan(filter)` | `CBCentralManager.scanForPeripherals(withServices: [IRIS_SERVICE_UUID], options:)` — filter MUST contain IRIS service UUID (background requires it); UUID-less config rejected (AC-9) |
| `stop_scan(handle)` | `stopScan()` |
| `start_advertising(data)` | `CBPeripheralManager.startAdvertising([ServiceUUIDsKey: [IRIS], LocalNameKey: ≤10 B])` — **service_data NEVER transmitted** (no service/mfr data — API-level) |
| `stop_advertising(handle)` | `stopAdvertising()` |
| `connect_gatt(address)` | `centralManager.connect(peripheral, options:)` (address → CBPeripheral identity) — subject to probe-admission cache (BLE-RT-C004, AC-7) |
| `disconnect_gatt(handle)` | `cancelPeripheralConnection(_:)` |
| `gatt_write(handle,char_uuid,data)` | `writeValue(data, for: characteristic, type: .withResponse)` |
| `set_mtu(handle,mtu)` | **no-op request; return `peripheral.maximumWriteValueLength(for: .withResponse)`** (negotiated payload, cap 512; 23-B default; 20-B degraded guard — RES-0024 DI-5) |
| `incoming_gatt_writes()` | drain `peripheralManager(_:didReceiveWrite:)` CBATTRequest data |
| `scan_results()` | drain `centralManager(_:didDiscover:advertisementData:rssi:)` — **no IRIS service data in advertisementData** → payload empty; RSSI + identity only |
| **`gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC)`** | connect → `discoverServices([IRIS])` → `discoverCharacteristics` → `peripheral.readValue(for: identifyChar)` → `peripheral(_:didUpdateValueFor:)` → bytes → Rust core `DiscoveryBeacon::parse` → candidate peer. **BLE-RT-C003**: hard call-timeout (default 10 s, configurable) + cancellation → typed FFI error, no hang/leaked task (AC-6, DEC-BLE-002-0015) |

Lifecycle contract (Info.plist + restoration — D-7/DEC-BLE-002-0005): both `bluetooth-central` AND `bluetooth-peripheral` UIBackgroundModes; `CBCentralManagerOptionRestoreIdentifierKey`/`CBPeripheralManagerOptionRestoreIdentifierKey` = stable **"IrisCentralManager"/"IrisPeripheralManager"**; `willRestoreState` (central + peripheral) MUST re-call `startScan`/`startAdvertising` with debounce (no auto-resume); never initialize BLE state outside a valid launch path; ~8-peer bound honored (no churn; RES-0024 RQ-3).

## 5. Identity (KeychainEd25519 / KeychainX25519 — D-5)

- `identity.v1` = CryptoKit `Curve25519.Signing.PrivateKey` (Ed25519, RFC 8032). Persist `rawRepresentation` (32-B seed) as `kSecClassGenericPassword`, tag `com.iris.identity.v1`, `kSecAttrAccessible: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` (**no** `kSecAccessControl` biometric flags — FaceID/TouchID-gated items fail in background with `errSecInteractionNotAllowed`).
- Load: lazy, gated on `isProtectedDataAvailable` (iOS 15+ pre-warm can start process before first unlock).
- `KeychainX25519` — static X25519 advertisement key, same persistence posture (Android X25519 static-ad analogue; DEC-AND-0005).
- Interop: CryptoKit Ed25519 ↔ Rust `ed25519-dalek` are both RFC 8032 — byte-compatible raw representations (32-B seed / 32-B public key); **sender_id = first 16 B of SHA-256(pubkey)** unchanged (RES-0016 D2/DEC-P0006). KAT-style byte-compat spike at IMPLEMENT (G-IOS-5).
- SE exclusion: Secure Enclave is ECDSA P-256-only (L1) → **not** the identity path; documented weaker anchor vs Android TEE Keystore (known_limitation §11); SE ECDSA P-256 "rescue" key DEFERRED (v2 if a hardware-anchored signing requirement appears — mirrors RES-0022 StrongBox deferral).

## 6. SessionRecovery + background wiring (D-7)

`SessionRecovery` (thin coordinator — not a custom daemon):

1. At `application(_:didFinishLaunchingWithOptions:)`: classify launch reason — **BLE-restoration launch** (`bluetoothCentrals`/`bluetoothPeripherals` launch options) vs **BGTask launch** vs **user launch**; gate BLE init on `isProtectedDataAvailable`.
2. Forward `willRestoreState` dictionaries to the CB managers; re-arm scan/advertise with **debounce** (relaunch-crash-loop guard, DEC-BLE-002-0005); peripheral manager re-adds services if needed (SWIFT_LAYER correction absorbed).
3. Re-submit BGAppRefresh + BGProcessing requests at **EVERY launch** (schedules do not survive termination/force-quit — RES-0025 RQ-4).
4. Both task types: expiration handlers + `setTaskCompleted` on every path.
5. Force-quit ⇒ all background launch blocked by the system — sequence is a documented no-op in that case (never hang-on-launch).

`BGTaskWiring` (IOS.md pattern, corrected): maintenance work (routing-maintenance, message expiry, metric flush) is **best-effort only** — heuristic scheduling (no guaranteed cadence); **never schedule critical messaging / emergency delivery on BGTask** (G-IOS-4). APNs wake path stays a relay-reachable fallback only.

## 7. Live Activity (D-8)

`LiveActivityController` (ActivityKit): start only from foreground with `areActivitiesEnabled` check; availability guard **iOS 16.1+**; minimal `ContentState` (neighbor count / relay state / emergency banner — status only, no payload); handle `ActivityAuthorizationError` (visibility / global + per-app maximums); update via ActivityKit while running (best-effort when locked — G-IOS-4/5). Plist: `NSSupportsLiveActivities` + `NSSupportsLiveActivitiesFrequentUpdates`; widget-extension target. Documented as **screen-on framing** (Lock-Screen visible; iOS 26 scan continuation **stops at screen sleep**) — never an OS background relaxation; BLE-002 AC-10 wording carried (`supports_background_ios=false` unchanged in the Rust capability matrix). **Never a delivery guarantee.**

## 8. Security posture

1. **App-layer AEAD envelope = trust anchor** (CRYPTO-001 sign+encrypt, verify-before-forward, SEC-001 replay/dedup/rate gates; BLE-002 §3 hold). Never depend on BLE pairing/bonding or link-layer encryption (BLE-001 AC-7).
2. **Keychain data protection**: identity item = generic-password class with `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly` — the documented class for background services; `ThisDeviceOnly` = device-bound (UID-protected, excluded from backups/iCloud restore).
3. **Secure Enclave EXCLUSION**: identity is an app-layer CryptoKit Ed25519 key (no SE hardware signature ops). Documented tradeoff: weaker anchor than Android TEE Keystore (D-5, known_limitation §11). SE ECDSA P-256 "rescue" path DEFERRED (RES-0025 RQ-2; mirrors DEC-AND-0005).
4. **Swift concurrency**: Swift 5 language mode (D-2); `IosBleAdapter` `final class … : BleAdapter, @unchecked Sendable`; mutable CoreBluetooth state confined to a dedicated serial queue/actor — no shared mutable state across the FFI seam; async FFI callbacks complete via UniFFI oneshot (no Rust-side thread).
5. **CVE posture / patch floor**: verified iOS Bluetooth set (CVE-2023-42941, CVE-2024-23241, CVE-2024-44124, CVE-2024-44191) = OS-patchable only; defense-in-depth = only in-app control; minimum iOS security-patch floor documented in deployment (DEC-BLE-002-0006 carry); misattributed IDs excluded from the threat model.
6. **No unauthenticated-trigger actions**: connect-to-identify produces *candidate* peers only; every payload flows through envelope verify + engine gate (BLE-002 AC-7 hold).
7. **Background honesty**: BGTask = best-effort maintenance; Live Activity carries no payload; emergency P0/P1 path never depends on cloud/ML/LLM/internet (architect invariant — offline-first & emergency-priority preserved).

## 9. Acceptance criteria (AC-1..AC-21 — C2 gap RESOLVED, pattern BLE-002 AC-1..16 / ANDROID-001 AC-1..15)

Every AC maps to evidence; macOS-host unit tests use **IOS-CoreBluetooth-Mock** for Swift central-side logic (iOS Simulator does NOT support CoreBluetooth) — full BLE stays device-gated (BLK-0005).

| AC | Criterion | Evidence / verifier check |
|----|-----------|---------------------------|
| AC-1 | `crates/iris-ios` scaffolds as workspace member (uniffi `=0.31.2`); builds staticlibs for `aarch64-apple-ios` + `aarch64-apple-ios-sim` | CI (macOS runner) `cargo build --release --target …` logs; crate manifest |
| AC-2 | **UniFFI `generate --library` flow** (NOT `src/iris.udl` — stale IOS.md command corrected): bindings for `IrisEngine` + `BleAdapter` foreign trait generated from compiled-library metadata; generated Swift **committed** under `ios/IRIS/RustFFI/` | CI bindings step output; git-tracked artifacts; grep guard |
| AC-3 | **G-IOS spike** (RES-0025 gaps G-IOS-1/G-IOS-2/G-IOS-5): Swift codegen compiles under **Swift 5 language mode** (`SWIFT_VERSION=5`); `IosBleAdapter: BleAdapter, @unchecked Sendable` conforms; async foreign-trait methods bridge via UniFFI foreign futures; Rust pollers run through **explicit tokio `Handle`** (issue #2576 workaround) | macOS-host Swift compile+test log; `cargo test -p iris-ios` spike (pattern ANDROID AC-3/G-AND-3) |
| AC-4 | `IosBleAdapter` implements **ALL 11 ops** (10-op + `gatt_read`) with 1:1 mapping, no orphan ops; service_data NEVER transmitted | Swift compile + op-coverage grep guard (pattern BLE-002 AC-11 / ANDROID AC-11) |
| AC-5 | **Connect-to-identify `gatt_read`** (DEC-BLE-002-0002/0015): connect → `discoverServices` → `discoverCharacteristics` → `readValue(for: IRIS_IDENTIFY_CHARACTERISTIC)` → `didUpdateValueFor` → bytes → `DiscoveryBeacon::parse` → candidate peer; Android↔iOS identify-read vector expressed | CoreBluetooth-mock unit test (Swift) + Rust `SimulatedBleAdapter` round-trip (BLE-002 AC-3 held) |
| AC-6 | **BLE-RT-C003 read-timeout**: `gatt_read` path enforces a hard call-timeout (default 10 s, configurable) + cancellation → typed FFI error surfaced to Rust core; no hang / no leaked task | Mock delays response beyond timeout → timeout error; no-hang test |
| AC-7 | **BLE-RT-C004 probe-admission cache**: probe connects budgeted via admission cache keyed by `CBPeripheral.identifier` (BLE-002 C001/C003 carry — 8-probe/scan budget semantics); no duplicate probe connects to rejected peers within admission window | Unit tests mirroring `c001_ios_leg_probe_budget_caps_probe_connects` |
| AC-8 | MTU negotiate-late: `set_mtu` no-op returns `maximumWriteValueLength(for: .withResponse)` (cap 512); 23-B default; **20-B degraded guard** (RES-0024 DI-5 / iOS 16.x regression) | Mock returns 20/185/512 → adapter reports negotiated payload; segmenter cap min(negotiated−3, 512) honored |
| AC-9 | Scan/advertise contract: `start_scan` rejects UUID-less filter (IRIS service UUID mandatory); `start_advertising` = service-UUID + local-name ≤10 B only — never service data / manufacturer data | Unit tests on mock ad/scan dicts; static inspection |
| AC-10 | Lifecycle/restoration: both `UIBackgroundModes` (`bluetooth-central` + `bluetooth-peripheral`) declared; stable restoration identifiers "IrisCentralManager"/"IrisPeripheralManager"; `willRestoreState` re-arms scan/advertise with debounce; no BLE init outside valid launch path | Info.plist inspection + SessionRecovery unit tests + code inspection (DEC-BLE-002-0005) |
| AC-11 | **SessionRecovery**: launch-reason classification (BLE-restoration / BGTask / user); BGAppRefresh + BGProcessing **re-submitted at EVERY launch**; force-quit-safe no-op; BLE init gated on `isProtectedDataAvailable` | Unit tests with injected launch-options; code inspection |
| AC-12 | Identity: `KeychainEd25519` provisions `com.iris.identity.v1` — CryptoKit `Curve25519.Signing.PrivateKey`, persisted generic password, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, **no biometric `kSecAccessControl`**; `isProtectedDataAvailable` guard; byte-compat with `ed25519-dalek` (RFC 8032 raw seed/pubkey, KAT-style; resolves G-IOS-5); `KeychainX25519` static-ad key same posture | macOS-host Keychain round-trip unit test + RFC 8032 KAT vectors |
| AC-13 | LiveActivityController: ActivityKit availability-gated **iOS 16.1+**; `NSSupportsLiveActivities`(+`FrequentUpdates`) plist + widget-extension target; minimal `ContentState` (status only, no payload); `areActivitiesEnabled` + `ActivityAuthorizationError` handled; **screen-on framing only** (stops at sleep; never a delivery guarantee) | Code inspection + plist inspection + macOS compile with availability guards; device note (BLK-0005, G-IOS-3) |
| AC-14 | BGTaskScheduler wiring: app-refresh + processing identifiers registered; expiration handlers + `setTaskCompleted` on every path; maintenance best-effort only — **no critical messaging scheduled on BGTask** (G-IOS-4) | Code inspection + resubmit/expiration unit tests |
| AC-15 | CI workflow `.github/workflows/ios.yml`: macOS-runner leg (`runs-on: macos-15` arm64 + `maxim-lobanov/setup-xcode@v1` `xcode-version: '16.4'`); `rustup target add` both iOS targets; staticlibs → bindings → `xcodebuild -create-xcframework` (ios-arm64 + ios-arm64_x86_64-simulator) → Swift build + macOS-host tests + Simulator UI leg; runner-image/Xcode pinned at scaffold (G-IOS-6) | Workflow YAML parse-valid + CI run log (env-gated rows recorded, G-TI-1 pattern) |
| AC-16 | macOS-host unit tests GREEN with **IOS-CoreBluetooth-Mock** (central-side adapter logic; Simulator has no CoreBluetooth); full-stack rows stay **BLK-0005 gated** (BLE-002 AC-14 pattern) | CI test log + BLK-0005 resource-gate note |
| AC-17 | Doc reconciliation: all 6 EXTERNAL-FACTS corrections applied — IOS.md (Xcode floor, UniFFI flow, SE section, BGTask wording), SWIFT_LAYER.md (`peripheralManagerDidRestoreState` → willRestoreState re-arm), BLE.md §iOS, IDENT_DESIGN.md §keychain (RES-0025 design input 7) | Diff review vs §13 table (pattern BLE-002 AC-12 / ANDROID AC-12) |
| AC-18 | Workspace green held: `cargo test --workspace --all-features` ≥ **647/0/1**; clippy 0; rustfmt clean (no Rust-core regressions from this node) | CI log re-run at TEST |
| AC-19 | (GATED/BLK-0005, recorded) physical-device CoreBluetooth connect/send/receive + battery + Live Activity screen-on scan continuation (G-IOS-3/G-IOS-7) — recorded `known_limitation`, not blocker | Resource-gate note (DEC-0009) |
| AC-20 | SECURITY_REVIEW next stage: adversarial review of FFI surface + Keychain path + background/restoration wiring | Stage-transition evidence |
| AC-21 | VERIFY next stage: independent verification doc evidence table + verifier APPROVE (pattern EMERG-001/SEC-001/BLE-001) | Stage-transition evidence |

**Carry-forward mapping (every BLE-002 contract lands on ≥1 AC)**: BLE-RT-C003 → AC-6; BLE-RT-C004 → AC-7; DEC-BLE-002-0015 Android↔iOS identify-read → AC-4/AC-5; DEC-BLE-002-0002 connect-to-identify → AC-5; DEC-BLE-002-0004 MTU → AC-8; DEC-BLE-002-0005 restoration → AC-10/AC-11; DEC-BLE-002-0001 stable UUID → AC-9.
## 10. Declared decisions (DEC-IOS-0001..0009 — ratified in DECISIONS.md)

- **DEC-IOS-0001** Identity = Keychain CryptoKit Ed25519 (generic password, `AfterFirstUnlockThisDeviceOnly`, no biometric); SE ECDSA P-256 rescue deferred.
- **DEC-IOS-0002** Swift language mode pinned to 5 (`SWIFT_VERSION=5`) until mozilla/uniffi-rs#2929 resolves; `@unchecked Sendable` conformance for `IosBleAdapter`.
- **DEC-IOS-0003** UniFFI 0.31.x `generate --library` binding flow (stale UDL command corrected) + explicit tokio `Handle` (issue #2576 workaround).
- **DEC-IOS-0004** CI env-gate = macOS runner (`macos-15` + Xcode 16.4 / iOS 18 SDK); Linux iOS cross-build rejected; XCFramework assembled on CI.
- **DEC-IOS-0005** SessionRecovery = thin Swift coordinator over `willRestoreState` + BGTaskScheduler (launch-reason classification, re-arm, re-submit every launch, force-quit no-op).
- **DEC-IOS-0006** ActivityKit/Live Activity floor = iOS 16.1; screen-on framing only; never a delivery guarantee.
- **DEC-IOS-0007** FFI adapter = 11-op `BleAdapter` + `gatt_read` connect-to-identify mapping (DEC-BLE-002-0015 carry) with BLE-RT-C003 timeout + BLE-RT-C004 probe-admission cache.
- **DEC-IOS-0008** Toolchain pin: Xcode 16.4 / iOS 18 SDK / deployment floors per App Store mandate 2025-04-24; Swift 6.x compiler in Swift 5 language mode.
- **DEC-IOS-0009** Security posture: app-layer AEAD trust anchor; Keychain data protection class; Secure-Enclave exclusion documented; iOS security-patch floor carried (DEC-BLE-002-0006).

## 11. Risks / gaps / known limitations (recorded, not blockers — carry to PROJECT_GRAPH)

| # | Risk / gap | Design position |
|---|------------|-----------------|
| G1 (G-IOS-1) | UniFFI #2929 Swift-6 strict-concurrency async foreign-trait hard error — open as of 2026-06-18 | Swift 5 language mode pin (D-2); fallback = committed #2929 post-codegen patch; re-verify on 0.32.0 upgrade |
| G2 (G-IOS-2) | `async_runtime`-on-trait-exports (#2576) fixed only in 0.32.0 | Explicit tokio `Handle` (D-3); if engine migrates to 0.32.x mid-node, re-test removal (G-AND-1 cross-ref) |
| G3 (G-IOS-3) | iOS 26 Live Activity screen-on scan semantics L5 forum-documented only; stops at screen sleep | Physical-device verification required (BLK-0005, AC-19); documented as framing, never a delivery guarantee (AC-13) |
| G4 (G-IOS-4) | BGProcessingTask runtime budget undocumented (heuristic) | Design tolerates best-effort maintenance window only; never schedules critical messaging on BGTask (AC-14) |
| G5 (G-IOS-5) | CryptoKit Ed25519 ↔ Keychain generic-password round-trip + Rust `ed25519-dalek` byte-compat unproven | KAT-against-RFC-8032 spike at IMPLEMENT (AC-3/AC-12) |
| G6 (G-IOS-6) | Runner-image SDK drift (iOS 17 SDK absent on current macos-15; monthly image updates) | Pin Xcode + image tag at scaffold (AC-15); App Store iOS-18-SDK floor anyway |
| G7 (G-IOS-7) | macOS-host CoreBluetooth central testing = proxy, not real stack | Adapter-logic unit tests only; physical iPhone = acceptance surface (AC-16/AC-19, BLE-002 AC-14) |
| G8 | **Android↔iOS identify-read vector** (DEC-BLE-002-0015): `IosBleAdapter.gatt_read` lands here; Android `BleBridge` remains `DeviceNotFound` until ANDROID-001 follow-up | Carrier asymmetry accepted (DEC-BLE-002-0008); interop validated after the ANDROID FFI read op |
| G9 | iOS background asymmetry (RES-0019 R7 / RES-0024): backgrounded iOS hidden from Android + iOS (overflow iOS-iOS-only); limited relay when backgrounded | Foreground Android/Linux/Windows gateways carry relay-heavy work (BLE-002 risk table hold) |
| G10 | Physical-device CoreBluetooth/battery tests + Live Activity screen-on (BLK-0005); no Xcode on Windows dev host (macOS-runner env-gate, G-TI-1 pattern) | Recorded known_limitations; CI leg (AC-15/AC-16/AC-19) |

## 12. Test strategy

1. **macOS-host unit tests** (AC-3/AC-4/AC-5/AC-6/AC-7/AC-8/AC-9/AC-12): `Tests/` XCTest target over **IOS-CoreBluetooth-Mock** — 11-op conformance, `gatt_read` connect-to-identify flow, BLE-RT-C003 timeout, BLE-RT-C004 probe budget, MTU negotiate-late (20/185/512), ad/scan contract, Keychain Ed25519 round-trip + RFC 8032 KAT.
2. **JVM-equivalent Swift tests** (the Kotlin-side role for iOS): `FfiConformanceTests`/`SessionRecoveryTests`/`BGTaskWiring` logic seams abstracted behind protocols — launch-reason classification, re-arm-on-restore, BGTask re-submission, force-quit no-op, protected-data gating (AC-10/AC-11/AC-14).
3. **Rust-side**: `crates/iris-ios` spike tests (G-IOS spike, AC-3) + BLE-002 `SimulatedBleAdapter` round-trip hold (AC-5); workspace baseline 647/0/1 held (AC-18).
4. **CI-leg jobs** (AC-15): `.github/workflows/ios.yml` macOS-runner matrix — targets, bindings, XCFramework, Swift build, macOS-host tests, Simulator UI leg; env-gated rows recorded (pattern TEST-001 G-TI-1/AC-9).
5. **Device-gated rows** (AC-19, BLK-0005): physical CoreBluetooth connect/send/receive, Live Activity screen-on scan continuation (G3), battery — recorded `known_limitation`, not blockers.
6. **AC-20 SECURITY_REVIEW** then **AC-21 VERIFY** (independent verifier reproduction) before ACCEPT.

## 13. Doc corrections applied (RES-0025 §8 EXTERNAL-FACTS → file:line old→new)

Version acceptance: **IOS_DESIGN.md v1.0 accepts all 6 RES-0025 EXTERNAL-FACTS corrections** (statuses: 4 STALE/CORRECTED in IOS.md, 1 CONFIRMED+sharpened, 1 INCOMPLETE in SWIFT_LAYER.md, 1 design-brief correction). Actual doc-file edits deferred to DOCUMENT stage (AC-17).

| # | File | Line(s) | Claim (old) | Correction (new) |
|---|------|---------|-------------|-------------------|
| 1 | `docs/platforms/IOS.md` | 5-8 | "Minimum iOS: 14.0 / Target iOS: 17.0 / Xcode: 15.0+ / Swift: 5.9+" | **STALE**. Build floor = **Xcode 16+ / iOS 18 SDK** (App Store mandate since 2025-04-24, ITMS-90725, L1); Xcode 16 requires macOS Sonoma 14.5+; deployment target can stay lower (Xcode 16 supports iOS 12+, Apple recommends 15+); Swift 6.x compiler with **Swift 5 language mode** for UniFFI compat |
| 2 | `docs/platforms/IOS.md` | 240-244 | "The same `iris.udl` file used for Android generates Swift bindings: `uniffi-bindgen generate src/iris.udl --language swift --out-dir ios/IRIS/RustFFI/`" | **STALE**. 0.31.x flow = **`uniffi-bindgen generate --library <lib.a/.dylib> --language swift --out-dir …`** (metadata-based; `--lib-file` removed in 0.31.0; single-UDL generation deprecated for proc-macro crates; repo uses proc-macro — **no UDL exists** — RUST_CORE.md §FFI UDL sketch also superseded) |
| 3 | `docs/platforms/IOS.md` | 271-298 (§Secure Enclave Key Storage + note line 297) | "Note: Secure Enclave supports ECDSA P-256 only, not Ed25519. Same pattern as Android Keystore — hardware-backed identity key uses ECDSA, session encryption keys managed by Rust core." | **CORRECT but misapplied**. SE is indeed ECDSA P-256-only (L1) — but IRIS identity is **Ed25519** (DEC-0002/CRYPTO-001) → compatible path = **CryptoKit Ed25519 app-layer key in Keychain** (generic password, `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, no biometric), NOT an SE ECDSA identity key; document app-layer-vs-TEE anchor tradeoff; SE rescue deferred |
| 4 | `docs/platforms/IOS.md` | 211 | "BGProcessingTask provides up to ~30 seconds typically, up to a few minutes if conditions are right. Not guaranteed to run on schedule." | **CONFIRMED + sharpen**. BGAppRefresh/BGProcessing are **heuristic — no guaranteed cadence**; scheduling does **not** survive user force-quit; **re-submit at every launch** (L4/L1; DEC-IOS-0005) |
| 5 | `docs/implementation/SWIFT_LAYER.md` | 283-288 | `peripheralManagerDidRestoreState` — "State restoration: re-add services if needed" | **INCOMPLETE**. `willRestoreState` (central + peripheral) must ALSO **re-`startAdvertising`/re-`startScan`** (no auto-resume); debounce to avoid relaunch-crash loops (RES-0024 DI-6 / DEC-BLE-002-0005) |
| 6 | research brief / `IOS-001_DISCOVER.md` §3 | "Live Activity (ActivityKit, iOS 16.2+)" | **CORRECTED**. Availability floor = **iOS 16.1** (Apple news 2022-09-14/10-24, L1); "16.2" is a common mis-remembering (DEC-IOS-0006) |

Plus 2 confirmed carry-rows: BLE_002_DESIGN.md §5 contract table (10 ops) **CONFIRMED, needs `gatt_read` row** — provided in §4 of this document; RES-0024 "iOS 26 Live Activity screen-on scan" **CONFIRMED (L5)** — re-verify at IOS-001 VERIFY (G3).

## 14. Next

IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT scope: `crates/iris-ios` scaffold + G-IOS spike (AC-3) → `ios/` app shell + `IosBleAdapter`/identity/SessionRecovery/BGTask/LiveActivity → `ios.yml` CI leg (AC-15) → macOS-host tests (AC-16). Doc-file edits per §13 land at DOCUMENT stage (AC-17). Physical-device rows remain BLK-0005 gated (AC-19). After ACCEPT: NODE_TRANSITION → PILOT-001.