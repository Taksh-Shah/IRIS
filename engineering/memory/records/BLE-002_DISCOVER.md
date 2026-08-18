# BLE-002 DISCOVER / UNDERSTAND — BLE Transport (iOS)

**Document ID**: IRIS-BLE-002-DISCOVER-001
**Version**: 1.0
**Node**: BLE-002 (P0 TRANSPORT, platform IOS, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-18
**Iteration**: ~132
**Stage**: DISCOVER → UNDERSTAND COMPLETE (read-only pass; workspace baseline
633/0/1 clippy 0 rustfmt clean **held**)

---

## 1. Node definition (PROJECT_GRAPH line 256)

- **BLE-002** — "BLE Transport — iOS", type TRANSPORT, priority P0, status
  DISCOVERED, description "iOS CoreBluetooth, background modes, iOS background
  limitations".
- **dependencies**: `["TRANSPORT-001"]` — **COMPLETE** → node **eligible**.
- **C2 gap**: **CONFIRMED** — no `acceptance_criteria` present → **RESOLVE at
  DESIGN** (same pattern as BLE-001, AC-1..16 defined in `BLE_TRANSPORT_DESIGN.md`).
- Same platform split as BLE-001 (ANDROID): this is the iOS transport leg of
  the same `BleTransport` Rust-core transport; the platform adapter seam is
  the `BleAdapter` trait (FFI contract).

## 2. What already exists (inherited from BLE-001 / TRANSPORT-001)

| Artifact | Status | Relevance to BLE-002 |
|---|---|---|
| `crates/iris-core/src/transport/ble.rs` (1522 lines) | COMPLETE (BLE-001) | Same `BleTransport` drives iOS; header explicitly names `IosBleAdapter (Swift/FFI)` as the other injected adapter. Core has NO iOS-specific code — the adapter trait is the seam. |
| `BleAdapter` trait + `SimulatedBleAdapter` | COMPLETE | FFI contract for `IosBleAdapter`; `SimulatedBleAdapter` conformance = type-checked guarantee (BLK-0005). |
| `ble_att.rs` (AttSegmenter/Reassembler) | COMPLETE | MTU-negotiate-late, ≤512 B ATT payload — **platform-neutral**, reused as-is on iOS. |
| `ble_advert.rs` (DiscoveryBeacon/CapabilityBits) | COMPLETE | Beacon build/parse platform-neutral; **but iOS cannot carry service data in ads** → connect-to-identify path (see §3). |
| `caps` in `ble.rs:287` | COMPLETE | `supports_background_ios: false` — "iOS background BLE is severely limited" (test at line 1075 pins it). |
| `TRANSPORT_ABSTRACTION.md` | COMPLETE | Platform-adapter architecture; `Caps.supports_background_ios` field exists (mod.rs:165). |
| `docs/transports/BLE.md` iOS CoreBluetooth § (lines 254-360) | COMPLETE | iOS permissions, scan, advertising limitations, MTU, fragmentation. |
| `docs/platforms/IOS.md` (318 lines) | COMPLETE | Full iOS platform design: CoreBluetooth managers, background modes, MCF, BGTaskScheduler, UniFFI, Secure Enclave (ECDSA only), Core Data encryption, testing. |
| `docs/implementation/SWIFT_LAYER.md` | exists | iOS Swift layer seam (UniFFI IrisCore.swift + XCFramework). |
| `ios/` directory | **does not exist** | Platform crate workspace to create under BLE-002 (or IOS-001). |

## 3. iOS-specific constraints (the BLE-002 design surface)

Consolidated from `docs/platforms/IOS.md` + `docs/transports/BLE.md` + RES-0019
R7 (explicitly deferred: "iOS background asymmetry … BLE-002 scope; discovery
beacon must fit iOS-compatible advert subset for cross-platform discovery").

**ATT / wire plane**
- Default ATT MTU 23; CoreBluetooth negotiates up to 512 (iOS uses 185 in
  practice pre-negotiation). `AttSegmenter`'s negotiate-late 23→512 maps
  directly (the `maximumWriteValueLength(for:)` query drives it).
- iOS accepts L2CAP + GATT writes `CBATTRequest` with `offset` — our
  reassembler already models offset-based inbound (BLE-001 verifies).

**Discovery plane (biggest delta vs Android)**
- **iOS cannot include service data in advertisements** — only service UUID +
  local name. The BLE-001 discovery beacon (signed `NodeAdvertisement`-subset in
  service data) is NOT deliverable in iOS ads → **connect-to-identify**: iOS
  peripheral advertises only IRIS service UUID; scanners must connect (or read
  over GATT) to retrieve the beacon/identity. Android can skip this via ad data.
- iOS background advertising **stops/drops to overflow area** → backgrounded iOS
  cannot be discovered by other iOS; can still be *limited*-detected by Android
  (iOS continues advertising in a degraded form).
- `allowDuplicates: true` silently downgraded to `false` in background; scan
  results batched, 10-30 s latency; duty cycle ~1-2% vs 50% foreground →
  discovery tolerates intermittent detection (design-for-missed-advertisement).

**Background / lifecycle plane**
- Both `bluetooth-central` + `bluetooth-peripheral` UIBackgroundModes required
  or Apple won't restore managers after suspension.
- State preservation/restoration: `CBCentralManagerOptionRestoreIdentifierKey` +
  `centralManager(_:willRestoreState:)` / `peripheralManager(_:willRestoreState:)`
  — identifiers stable across launches.
- iOS suspends advertising in background; connected links persist (can relay to
  already-connected peers). **Consequence**: iOS is a **limited relay node** in
  background; Android foreground nodes + Linux/Windows gateways carry the
  relay-heavy work (emergency UX: keep phone in foreground).
- `NSBluetoothAlwaysUsageDescription` (required) + legacy
  `NSBluetoothPeripheralUsageDescription` (< iOS 13).

**Cross-platform note**
- MCF (MultipeerConnectivity) is a *supplement* (Apple-to-Apple), not a
  BLE-substitute — kept OUT of BLE-002 (it rides under IOS-001 MCF surface,
  per iOS.md).
- Discovery beacon must remain Android-iOS interop compatible: iOS advertises
  IRIS service UUID only; the shared `BeaconParser`/`DiscoveryBeacon` core
  handles both ad-carried (Android) and connect-then-read (iOS) fetch paths.

## 4. Open questions for RESEARCH (iter ~133, RES-0024)

- RQ-1: 2024-2026 CoreBluetooth background limitations current state — any
  relaxation/evolution in iOS 18/19 background scan/advertise behavior?
  (RES-0019 was Android-centric; iOS side needs SOTA confirmation.)
- RQ-2: iOS hidden-state overflow advertisement + background scan PendingIntent
  analog (none on iOS) — corpus/library evidence for background-only-Android
  claim in TRANSPORT_ABSTRACTION ("iOS Background: Partial").
- RQ-3: GATT-client/peripheral connection ceilings on iOS (BLE.md says ~8
  practical) + `maximumWriteValueLength` MTU behavior on current iOS/Apple chips
  (device-gated measurement).
- RQ-4: State-restoration bug surface (CVE-2020-...? BLE stack CVEs on iOS
  patched-by-OS, no app fix possible) — any app-level hardenings.
- RQ-5: Cross-platform discovery interop: iOS-only-connect-to-read vs Android
  ad-carried — test-vector implications for `DiscoveryBeacon` (shared core path).

## 5. Stage outcome

- **UNDERSTAND COMPLETE** — reference surface (BLE-001 pipeline + iOS.md + BLE.md
  iOS § + RES-0019 R7 deferral) fully read; C2 gap confirmed; open questions
  enumerated for RESEARCH.
- **NEXT**: RESEARCH (iter ~133, RES-0024 iOS BLE SOTA) → DESIGN
  (iter ~134: `BLE_002_DESIGN.md`, AC-1..n resolves C2, DEC-BLE-002-xxxx) →
  IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT → IOS-001.
- Workspace **untouched** (read-only pass) — baseline 633/0/1 held.