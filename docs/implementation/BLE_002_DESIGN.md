# BLE-002 Design — BLE Transport (iOS) v1

**Document ID**: IRIS-BLE-002-DESIGN-001
**Version**: 1.0
**Node**: BLE-002 (P0 TRANSPORT, platform IOS, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-18
**Research basis**: RES-0024 (2026-08-18, iOS CoreBluetooth 2024-2026 SOTA —
verdict CONDITIONAL-PROCEED, 7 DESIGN inputs, 5 gaps G1-G5); RES-0019 R7
(iOS asymmetry deferral); `engineering/memory/records/BLE-002_DISCOVER.md`
(UNDERSTAND, iter ~132); `docs/platforms/IOS.md` (318-line platform design);
`docs/transports/BLE.md` iOS CoreBluetooth §254-360; TRANSPORT_ABSTRACTION.md
(platform-adapter architecture); BLE-001 (reference transport impl —
`BLE_TRANSPORT_DESIGN.md`, AC-1..16, DEC-BLE-0001..8); INTERNET-001 (framing/
backoff pattern); ACCEPTANCE_POLICY.yaml TRANSPORT-type additions; DEC-0009
(gates lifted); BLK-0005 (device-test resource gating).
**Absorbs**: RES-0024 RQ-1..RQ-5 answers + 7 DESIGN inputs + outdated-claims
table; RES-0024 gaps G1-G5 (design-positioned); BLE-002_DISCOVER iOS surface.
**Reconciles**: `docs/platforms/IOS.md` + `docs/transports/BLE.md` vs RES-0024
2024-2026 SOTA (iOS 26 accessory modes are NOT general relaxation; Live
Activity = screen-on scan only; state restoration does not auto-resume;
1-2% duty cycle = L5 estimate).

---

## 1. Scope

Implement the **iOS leg of the IRIS BLE transport** on the existing
`Transport` trait, reusing the **BLE-001 Rust core** (`crates/iris-core/src/
transport/ble.rs` + `ble_att.rs` + `ble_advert.rs`, all platform-neutral). The
platform adapter seam remains the **`BleAdapter` trait** — the same trait
BLE-001 defined; its doc-header already names `IosBleAdapter (Swift/FFI)`.
The actual iOS Swift/CoreBluetooth adapter implementation is **IOS-001 scope**
(like AndroidBleAdapter = ANDROID-001); BLE-002 delivers: (1) the adapted
platform contract for iOS (what `IosBleAdapter` MUST implement, with the iOS
constraints baked in), (2) any Rust-core adjustments needed to honor the iOS
constraints (connect-to-identify discovery path, MTU via
`maximumWriteValueLength`, ~8-peer surface, restore-id seam), (3) AC-1..16
(C2 gap RESOLVED), (4) DEC-BLE-002-xxxx decisions. Physical-device
CoreBluetooth coverage stays **BLK-0005 gated** (iOS Simulator does not
support CoreBluetooth).

**v1 shape (RES-0024 verdict CONDITIONAL-PROCEED)**: same GATT point-to-point
transport for payload exchange, but discovery on iOS is **connect-to-identify**
— the iOS peripheral advertises **only the stable IRIS service UUID** (+
optional local name); scanners must connect and read the identification
characteristic to obtain the beacon/identity (Android can skip via ad-carried
service data). **No service data in iOS advertisements — never.**

**Deliberately NOT relaxed (RES-0024 RQ-1/RQ-2)**: background advertising
remains overflow-area/iOS-iOS-only; there is **no iOS background-scan
PendingIntent analog**; iOS 26 "Common Bluetooth HIDs/Bluetooth LE Custom
Devices" background modes are **AccessorySetupKit accessory-only** — IRIS does
not qualify; Live Activity = **screen-on scan only** (stops at screen sleep),
treated as foreground-adjacent UX, not OS background relaxation.

**Explicitly rejected / deferred for v1 (RES-0024)**: BLE Mesh / PAwR / BIS
(carry BLE-001 DEC-BLE-0002..0004); iOS 26 accessory background modes
(DEC-BLE-002-0007); PendingIntent-style background scan (impossible on iOS —
DEC-BLE-002-0003); per-session dynamic UUID advertisement
(DEC-BLE-002-0001).

---

## 2. Wire-constraints and platform contract (RES-0024 → ACs)

### 2.1 Advertisement budget (RES-0024 RQ-5 / DI-1, DI-2)
- iOS `startAdvertising(_:)` supports **only** `CBAdvertisementDataLocalNameKey`
  + `CBAdvertisementDataServiceUUIDsKey` — **no service data, no manufacturer
  data** (L1, Apple docs).
- Foreground budget: **28-B advertisement + 10-B scan response** (local name
  only). One 128-bit IRIS service UUID = 18 B; remaining ~10 B = short local
  name. No room for discovery-beacon payload duplication.
- **IRIS service UUID is STABLE** — never per-session (a background/foreground
  scanner needs the UUID in its filter; overflow-area hash matching requires a
  stable scanned-for UUID). The Android `DiscoveryBeacon` (ad-carried service
  data) stays Android-only; on iOS the beacon/capability data is served from a
  **GATT identification characteristic**, read after connect.
- The shared `DiscoveryBeacon` build/parse core (`ble_advert.rs`) is reused for
  **both** fetch paths: Android reads it from ad service data; iOS reads it
  from the identification characteristic after connect. One parser, two
  carriers.

### 2.2 MTU (RES-0024 RQ-3 / DI-5)
- iOS has **no MTU-request API** — ATT MTU exchanges are negotiated by the
  stack automatically (up to 517). The app queries the negotiated payload via
  `maximumWriteValueLength(for:)`, which returns ATT payload (MTU−3) **capped
  at 512**.
- `BleAdapter::set_mtu(handle, mtu)` semantics on iOS: request is a no-op; the
  adapter **returns `maximumWriteValueLength(for: .withResponse)`** as the
  negotiated value. Rust core treats MTU as **negotiate-late**: start at 23-B
  default, resize on the returned value; segmenter cap = **min(negotiated−3,
  512)**.
- **iOS 16.x regression guard (DI-5)**: iOS 16.0/16.0.1 reported MTU drop
  185→77→20 (fixed 16.1). Segmenter must treat a reported 20-B payload as
  degraded-but-functional, not an error (defensive; do not loop).
- Aligns with RES-0007/EMERG-001 512-B ceiling and BLE-001 AC-2.

### 2.3 Scanning / discovery (RES-0024 RQ-1/RQ-2/RQ-5 / DI-1, DI-3, DI-4)
- **Background scan requires a service-UUID filter** (no exceptions);
  `allowDuplicates` is effectively false in background; results batched
  (community: 10-30 s); ~1-2% duty cycle (**L5 estimate**, qualitative L1).
  → `ScanFilter.service_uuids = [IRIS_SERVICE_UUID]` is mandatory on iOS.
- **No PendingIntent analog**: the app must be running (foreground or
  background-suspended-but-launched) to scan; system relaunches for
  **connection events** only (not scans); user force-quit disables relaunch.
- **iOS 26 Live Activity** can continue a CoreBluetooth scan while the screen
  is on / Lock Screen visible (even without service-UUID filter?) — field
  report: works on Lock Screen + behind another app, **stops at screen sleep**.
  → Adoption: Live Activity = foreground-adjacent UX affordance, **not** an OS
  background capability; `supports_background_ios` stays **false**.
- **Connect-to-identify flow (DI-1)**: foreground scan (UUID filter) →
  `connect_gatt` → read identification characteristic → parse
  `DiscoveryBeacon`/capabilities → candidate peer (then normal envelope
  security). A backgrounded iOS can connect to an Android node via the UUID
  filter; Android cannot discover a backgrounded iOS node (overflow area is
  iOS-iOS-only). Symmetric discovery requires both nodes foregrounded (or iOS
  screen-on).
- Scan-restart discipline from BLE-001 AC-5 carries over (no blind restart;
  backoff) — iOS has no documented restart ceiling but honors the same
  defensive pattern.

### 2.4 Background / lifecycle (RES-0024 RQ-1/RQ-4 / DI-6)
- **Both** `bluetooth-central` AND `bluetooth-peripheral` UIBackgroundModes
  required for manager restoration (confirmed, needs nuance: restoration works
  for scanned/connected peripheral or scanner with identifiers; iOS 26
  accessory categories restore pairing state without app code — not IRIS).
- **State restoration does NOT auto-resume**: the system does not call
  `startScan`/`startAdvertising` again; the app MUST re-start them in
  `willRestoreState`. Restoration identifiers stable across launches.
- **Relaunch classification**: system-kill relaunch = allowed; user force-quit
  = **never relaunched** (no hang-on-launch logic for that case). Guard against
  relaunch-crash loops (restoration-ID mismatch / restart-while-scanning) with
  debounce/timeout; never initialize BLE state outside a valid launch path.
- Backgrounded iOS: connected links persist (limited relay to already-connected
  peers); cannot accept new connections.
- Required Info.plist strings: `NSBluetoothAlwaysUsageDescription` (+ legacy
  `NSBluetoothPeripheralUsageDescription` for < iOS 13).

### 2.5 Connection ceilings (RES-0024 RQ-3)
- Apple documents **no** concurrent-connection ceiling; community-measured
  **~8 practical** (central and peripheral). Rust core bounds per-peer reuse +
  close-prompty already (BLE-001 AC-9); iOS adapter must honor a
  **~8-peer surface** and never churn connections.

---

## 3. Security posture (RES-0024 RQ-4 → ACs)

1. **App-layer envelope crypto is the trust anchor** (CRYPTO-001 sign+encrypt,
   verify-before-forward, SEC-001 replay/dedup/rate gates). Never depend on BLE
   pairing/bonding or link-layer encryption (unchanged from BLE-001 AC-7).
2. **CVE posture (DI-7)**: verified iOS Bluetooth-stack CVEs
   (CVE-2023-42941, CVE-2024-23241, CVE-2024-44124, CVE-2024-44191) are
   **OS-patchable only** — no in-app mitigation exists. App defense-in-depth is
   the only in-app control. **Document the minimum iOS security-patch floor** in
   the deployment doc (mirrors BLE-001 AC-10/BLK-0005).
3. **Misattributed CVE exclusion (DI-7)**: CVE-2023-28412 (Snap One OvrC),
   CVE-2024-44270 (macOS sandbox), CVE-2021-31714 (not in NVD) are
   **excluded** from the BLE-002 threat model — they are not iOS Bluetooth
   findings.
4. **Defensive parse** (both ad-carried and connect-read beacon paths) with
   adversarial/property tests; never panic/alloc-on-malformed payload.
5. **No unauthenticated-trigger actions**: connect-to-identify still produces
   only a *candidate* peer; every payload flows through envelope verify +
   engine gate.
6. **Relay attacks**: proximity never a security primitive; residual =
   topology distortion, bounded by SEC-001 + ROUTE-002 (BLE-001 carries
   forward).

---

## 4. Module layout (pure Rust, `crates/iris-core/src/transport/ble.rs`)

| Piece | Status | What BLE-002 changes |
|---|---|---|
| `BleAdapter` trait | COMPLETE (BLE-001) | **unchanged as the contract**; doc-comments extended for `IosBleAdapter` semantics (set_mtu = query `maximumWriteValueLength`; scan_results from CBCentralManager; start_advertising = UUID + local name only) |
| `AdvertisementData` | COMPLETE | `service_data` field stays (Android path); iOS adapter passes empty `service_data` + sets `service_uuid` + `local_name` only |
| `ScanFilter` | COMPLETE | iOS: `service_uuids = [IRIS_SERVICE_UUID]` always (mandatory for background) |
| `SimulatedBleAdapter` | COMPLETE | extended for iOS-path tests: `inject_scan_result` with **empty service-data payload** (iOS ad) + `inject_identify_read` (simulated connect-read returns beacon from characteristic) |
| `BleTransport` | COMPLETE (BLE-001) | discovery path branches: ad-carried beacon (Android) vs **connect-to-read beacon (iOS)**; candidate assembled after `DiscoveryBeacon::parse` from either source |
| `ble_att.rs` | COMPLETE | MTU source abstraction: accept externally-reported negotiated payload (iOS `maximumWriteValueLength`), 23-B default, 20-B degraded guard; segmenter cap min(negotiated−3, 512) |
| `ble_advert.rs` | COMPLETE | unchanged — one parser, two carriers (service data on Android; identify characteristic on iOS) |
| `IosBleAdapter` (FFI contract) | not yet | **IOS-001 scope** (Swift/CoreBluetooth): 10-op trait impl over `CBCentralManager`/`CBPeripheralManager` + restoration identifiers; `set_mtu` → `maximumWriteValueLength`; `scan_results` from central callbacks; advertising = UUID + local name only |
| tests | grows (transport::ble baseline 42) | iOS-path tests: connect-to-identify via SimulatedBleAdapter, empty-ad-service-data branch, identification-characteristic read, MTU negotiated-late from maximumWriteValueLength incl. 20-B guard, ~8-peer bound, restoration seam types |

**Capability matrix entry (TRANSPORT-001)**: BLE-001 `caps` unchanged EXCEPT
the iOS leg is documented per-node: `supports_background_ios: false` **held**
(ble.rs:287 + test :1075), `supports_broadcast: true` (discovery), unicast
true, `max_message_size: u16::MAX` (segmented), `range 1..100 m`,
`typical_throughput_bps: 1_000_000`, `cost_class: Free`. TRANSPORT_ABSTRACTION
"iOS Background: Partial" updated to RES-0024 wording.

---

## 5. iOS FFI adapter contract (IOS-001 seam)

`BleAdapter` trait IS the FFI contract; `IosBleAdapter` (Swift/CoreBluetooth)
must implement (under IOS-001):

| Trait op | iOS/CoreBluetooth mapping |
|---|---|
| `start_scan(filter)` | `CBCentralManager.scanForPeripherals(withServices: [IRIS_SERVICE_UUID], options:)` — filter MUST contain IRIS service UUID for background |
| `stop_scan(handle)` | `stopScan()` |
| `start_advertising(data)` | `CBPeripheralManager.startAdvertising([ServiceUUIDsKey: [IRIS], LocalNameKey: ≤10 B])` — **service_data field NEVER transmitted** |
| `stop_advertising(handle)` | `stopAdvertising()` |
| `connect_gatt(address)` | `centralManager.connect(peripheral, options:)` (address → CBPeripheral identity) |
| `disconnect_gatt(handle)` | `cancelPeripheralConnection(_:)` |
| `gatt_write(handle,char_uuid,data)` | `writeValue(data, for: characteristic, type: .withResponse)` |
| `set_mtu(handle,mtu)` | **no-op request; return `peripheral.maximumWriteValueLength(for: .withResponse)`** (negotiated payload, cap 512; may be 23/20 pre/post negotiation) |
| `incoming_gatt_writes()` | drain `peripheralManager(_:didReceiveWrite:)` CBATTRequest data |
| `scan_results()` | drain `centralManager(_:didDiscover:advertisementData:rssi:)` — **advertisementData contains NO IRIS service data** → payload empty; RSSI + identity only |

Lifecycle contract (Info.plist + restoration): both UIBackgroundModes;
`CBCentralManagerOptionRestoreIdentifierKey`/`CBPeripheralManagerOptionRestoreIdentifierKey`
stable "IrisCentralManager"/"IrisPeripheralManager"; `willRestoreState`
handlers MUST re-call `startScan`/`startAdvertising`; never initialize BLE
state outside a valid launch path; iOS 26 accessory categories NOT used.

This crate is **out of scope for BLE-002's CI** (BLK-0005: real iPhone
required; Simulator unsupported) — the trait + `SimulatedBleAdapter`
conformance is the type-checked guarantee (BLE-001 AC-11 pattern).

---

## 6. Acceptance criteria (BLE-002 AC-1..n — resolves the C2 gap)

Defined in `engineering/PROJECT_GRAPH.yaml` BLE-002 node; each mapped to
evidence. Pattern BLE-001 AC-1..16.

- **AC-1** Transport trait impl: `BleTransport` registers + selects via
  `TransportManager` for the iOS leg (capability/eligibility test; BLE-001 base
  held).
- **AC-2** MTU negotiate-late on iOS: segmenter respects 23-B default → resizes
  to `maximumWriteValueLength` result (≤512 payload); 20-B degraded guard
  (RES-0024 RQ-3/DI-5).
- **AC-3** Discovery = connect-to-identify: iOS ad (empty service data, UUID +
  local name) → scan (UUID filter) → connect → read identification
  characteristic → `DiscoveryBeacon::parse` → candidate peers;
  `stop_discovery` stops; round-trip via `SimulatedBleAdapter` (RES-0024
  RQ-5/DI-1).
- **AC-4** Connect/send/receive E2E via `SimulatedBleAdapter` (mock iOS
  peer/pair): payload → segment → GATT write → inbound reassembly →
  `IncomingMessage` (BLE-001 path held for iOS leg).
- **AC-5** Scan-restart discipline: no blind restart; backoff + error
  propagation (carry BLE-001 AC-5); service-UUID filter mandatory (iOS
  contract) enforced.
- **AC-6** Adversarial parse: malformed/truncated/oversized beacon payloads
  rejected without panic/alloc blowup on **both** ad-carried and
  characteristic-carried paths (property + regression tests).
- **AC-7** App-layer security: every inbound frame passes envelope-verify seam;
  unauthenticated frames only produce candidates (RES-0024 RQ-4; BLE-001 AC-7
  held).
- **AC-8** Lifecycle: `shutdown()` stops poller + disconnects + Unavailable;
  restoration-seam types present; no BLE init outside valid launch path
  (RES-0024 DI-6).
- **AC-9** Reliability: per-peer single GATT connection reuse; close promptly;
  **~8-peer bound** (iOS practical ceiling) enforced (RES-0024 RQ-3).
- **AC-10** Background contract DOCUMENTED in BLE.md + IOS.md + deployment:
  both UIBackgroundModes, overflow area (iOS-iOS-only), no PendingIntent
  analog, state restoration does not auto-resume (re-start in willRestoreState),
  system-kill vs user-force-quit, Live Activity screen-on only,
  `supports_background_ios=false`, min iOS security-patch floor, misattributed
  CVE exclusion.
- **AC-11** FFI-contract conformance type-checked: `BleAdapter` trait doc-
  updated for IosBleAdapter semantics + `SimulatedBleAdapter` conformance +
  clippy-clean (ACCEPTANCE_POLICY.TRANSPORT).
- **AC-12** Doc reconciliation: BLE.md + IOS.md + TRANSPORT_ABSTRACTION.md +
  BLE-002 node agree with code + RES-0024 outdated-claims table applied
  (1-2% duty cycle cited as L5; iOS 26 accessory modes not general relaxation).
- **AC-13** Workspace green + clippy 0 + baseline **633/0/1** held
  (BLE-001 transport::ble 42 held).
- **AC-14** (GATED/BLK-0005, recorded): physical-device CoreBluetooth
  connect/send/receive + battery BENCH = known_limitation (iOS Simulator does
  not support CoreBluetooth; real iPhone required).
- **AC-15** SECURITY_REVIEW stage: redteam adversarial findings
  dispositioned/fixed/recorded.
- **AC-16** VERIFY stage: independent verification doc evidence table + verifier
  APPROVE (pattern EMERG-001/SEC-001/BLE-001).

---

## 7. Declared decisions (DEC-BLE-002-xxxx — ratified in DECISIONS.md)

- **DEC-BLE-002-0001** Stable IRIS service UUID for iOS advertising; never
  per-session (background scanner needs the UUID in its filter; overflow-area
  hash matching requires a stable scanned-for UUID). (RES-0024 DI-1/RQ-5)
- **DEC-BLE-002-0002** iOS discovery = connect-to-identify: UUID-only
  advertisement (no service data); beacon/capabilities served from a GATT
  identification characteristic read after connect; shared `DiscoveryBeacon`
  parser, two carriers. (RES-0024 DI-1/RQ-5)
- **DEC-BLE-002-0003** `supports_background_ios` stays **false**; no
  PendingIntent analog; symmetric discovery requires foreground (or iOS 26
  screen-on via Live Activity — foreground-adjacent UX, not OS background).
  (RES-0024 DI-3/RQ-2)
- **DEC-BLE-002-0004** MTU = `maximumWriteValueLength` (no request API);
  negotiate-late 23→≤512; 20-B degraded guard for the iOS 16.x regression.
  (RES-0024 DI-5/RQ-3)
- **DEC-BLE-002-0005** State-restoration hardening: re-start scan/advertise in
  `willRestoreState`; distinguish system-kill relaunch (allowed) vs user
  force-quit (never relaunched); debounce/timeout vs relaunch-crash loops; no
  BLE init outside valid launch path. (RES-0024 DI-6/RQ-4)
- **DEC-BLE-002-0006** CVE posture: verified iOS set (CVE-2023-42941/2024-
  23241/2024-44124/2024-44191) = OS-patchable only; app defense-in-depth is the
  only in-app control; iOS security-patch floor documented; misattributed IDs
  (CVE-2023-28412/2024-44270/2021-31714) excluded from threat model.
  (RES-0024 DI-7/RQ-4)
- **DEC-BLE-002-0007** iOS 26 "Common Bluetooth HIDs / Bluetooth LE Custom
  Devices" background modes REJECTED for IRIS — AccessorySetupKit
  accessory-pairing-only; NOT a general background relaxation. (RES-0024
  RQ-1/RQ-2)
- **DEC-BLE-002-0008** Cross-platform interop: Android ad-carried vs iOS
  connect-to-read both use the shared `DiscoveryBeacon` parse core; overlapping
  reliance on iOS overflow area explicitly NOT designed (iOS-internal, G4).
  (RES-0024 RQ-5)

---

## 8. Risks / known limitations (recorded, not blockers)

- Physical-device CoreBluetooth tests + battery measurement gated (BLK-0005;
  iOS Simulator unsupported). Battery figures stay design estimates.
- iOS background asymmetry (RES-0019 R7/RES-0024): backgrounded iOS hidden from
  Android + iOS peers (overflow iOS-iOS-only, duty-cycled); iOS = limited relay
  when backgrounded; foreground Android/Linux/Windows gateways carry
  relay-heavy work.
- 1-2% background duty cycle + 10-30 s batch latency = L5 community estimates
  (G1) — design-for-missed-advertisement, not acceptance metrics.
- Live Activity scan semantics (screen-on boundary) = L5 forum-documented until
  first-party release-note confirmation (G3) — re-verify at IOS-001.
- Overflow-area format reverse-engineered (L5, G4) — never build interop on it.
- iOS 16.x low-MTU (20-B) regression: degraded-but-functional guard, not a
  blocker (fixed in 16.1).

---

**Next**: IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT
scope = pure-Rust core adjustments (§4) — platform-neutral, no new crate;
`IosBleAdapter` Swift implementation + physical-device coverage under IOS-001.