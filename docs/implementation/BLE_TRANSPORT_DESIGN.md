# BLE-001 Design — BLE Transport (Android) v1

**Document ID**: IRIS-BLE-001-DESIGN-001
**Version**: 1.0
**Node**: BLE-001 (P0 TRANSPORT, platform ANDROID, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-16
**Research basis**: RES-0019 (2024-2026 SOTA: connectionless discovery / Android
BLE API / Mesh / PAwR / DLEP / CVE + relay); RES-0007 (design production-ready-v1);
`docs/transports/BLE.md` (480-line platform design); TRANSPORT_ABSTRACTION.md
(platform-adapter architecture); INTERNET-001 (reference transport impl + framing/
backoff pattern, 27/27 tests); ACCEPTANCE_POLICY.yaml TRANSPORT-type additions;
DEC-0009 (gates lifted); BLK-0005 (device-test resource gating).
**Absorbs**: RED-0009-01/02 + RED-0001-06 (already in scaffold `ble.rs`);
RES-0019 R1-R9; RES-0019 Gaps G1-G7 (design-positioned).
**Reconciles**: BLE.md vs scaffold (`PollingBleAdapter` vs live trait —
CLARIFIED: `ble.rs` ships `BleAdapter` trait + `SimulatedBleAdapter` + lifecycle
recorder; `PollingBleAdapter` referenced in doc is the poller task inside
`BleTransport::connect`, not a distinct adapter).

---

## 1. Scope

Implement the BLE transport on the IRIS `Transport` trait following the
**INTERNET-001 reference pattern**: a Rust-core transport that drives a
**platform-neutral adapter trait** (`BleAdapter`), with the real Android
implementation injected at runtime via FFI (uniffi/JNI, Kotlin) and a
`SimulatedBleAdapter` covering the in-memory/CI path (BLK-0005 device tests stay
gated). The work is **pure-Rust in `crates/iris-core`**; the Android Kotlin/JNI
adapter crate is scaffolded as a documented FFI contract but its physical-device
coverage is a recorded `known_limitation`.

**v1 shape (RES-0019 R1, verdict PROCEED)**: GATT point-to-point transport for
payload exchange + Extended/Periodic advertising as a **discovery beacon**
(advertise-parse; connect for bulk). Connectionless channel is for discovery,
not payload.

**Explicitly rejected for v1 (RES-0019 R2/R3/R4, DEC-BLE records)**: BLE Mesh 1.1
(no native AOSP API through 2026; nRF Mesh = only app path; v2 candidate),
PAwR (ESL-only star topology; phones cannot transmit in response slots; v2
candidate for stationary gateways), connectionless isochronous BIS (one-way
LE-Audio-centric; watchlist), DLEP as a transport (RFC 8175; deferred as routing-
metric interface only).

---

## 2. Wire-constraints and platform contract (RES-0019 R6 → ACs)

### 2.1 MTU (RES-0019 R1)
- Android 14+ negotiates **ATT MTU = 517** on the **first** `requestMtu` per ACL;
  subsequent requests on that ACL are disregarded.
- Default (pre-negotiation) MTU = 23. Treat MTU as **negotiate-late**: segmenter
  starts at 23-byte cap, re-sizes on `onMtuChanged`.
- **Capability**: IRIS payloads **≤512 B** ATT payload (517 − 5 header) — aligns
  with RES-0007/EMERG-001 512-B ceiling and scaffold `max_message_size: 512`.
- Sequence: connect → `requestMtu` (first) → on success `onMtuChanged` → segment
  accordingly. **Defensive error path (G5)**: if early `requestMtu` fails
  ("Request Not Supported" wedge on some Android 14 devices) → re-connect instead
  of continuing GATT ops.

### 2.2 Foreground service (RES-0019 R6.1)
- Android/IRIS BLE node runs an **FGS of type `connectedDevice`** (API 34+) to
  keep advertise/scan legal in background.
- Required declarations: `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_CONNECTED_DEVICE`
  (manifest), `POST_NOTIFICATIONS` (API 33+ runtime), `BLUETOOTH_CONNECT` +
  `BLUETOOTH_SCAN` (with `neverForLocation` where appropriate), legacy
  `BLUETOOTH`/`BLUETOOTH_ADMIN`/`ACCESS_FINE_LOCATION` for API < 31.
- User-visible persistent notification (emergency tool posture: user-initiated,
  long-running). Android 16 FGS runtime-quota note documented (G4 planning input).

### 2.3 Scanning (RES-0019 R6.2)
- Background scan = filtered scan via **`PendingIntent`** + filters (Android 8+
  sanctioned path; hardware-offloaded).
- Long-running discovery = batch scan (`reportDelayMillis > 0`) + periodic
  `flushPendingScanResults()`; default long-running scans degrade to opportunistic
  (~30 min); use a loop scan/rest-scan duty cycle.
- **Scan-restart backoff**: ≥5 start/stop per 30 s → silent failure
  (`SCAN_FAILED_APPLICATION_REGISTRATION_FAILED`, hardened on Android 17 → G4).
  Never blind-restart; exponential backoff + explicit failure callbacks.
- Runtime capability checks before use: `isLeExtendedAdvertisingSupported()`,
  `isLe2MPhySupported()`, `isLeCodedPhySupported()`,
  `isLePeriodicAdvertisingSupported()` — never assume by API level.

### 2.4 Advertising
- Single advertisement slot (Android max 4; IRIS uses 1). Extended Advertising
  (≤1650 B legacy? — legacy 251 B vs extended via AdvertisingSet; IRIS discovery
  beacon stays small).
- Discovery beacon payload = signed `NodeAdvertisement`-subset (PeerId short-id,
  service UUID, capability bitmask, freshness), ≤bounded size, no PII; MAC
  randomization (Android ~15 min) → IRIS identifies peers by public-key
  fingerprint, never MAC.

---

## 3. Security posture (RES-0019 R5 → ACs)

1. **App-layer envelope crypto is the trust anchor** (CRYPTO-001 sign+encrypt,
   verify-before-forward, SEC-001 replay/dedup/rate gates). Never depend on BLE
   pairing/bonding or link-layer encryption for security.
2. **Defensive advertise-parse** with adversarial/property tests (the Android
   stack CVE pattern is parser/state bugs); never panic/alloc-on-malformed advert.
3. **No unauthenticated-trigger actions** from connectionless frames (discovery
   only ever produces a *candidate* peer; every payload flows through envelope
   verify + engine gate).
4. **Document the OS security-patch floor** in the Android deployment doc —
   BLE stack CVEs are unpatched-by-app (CVE-2024-43770 GATT RCE, CVE-2025-0074
   SDP RCE, CVE-2025-48539 acl_arbiter zero-click UAF, CVE-2025-22406 BNEP UAF,
   CVE-2025-44557 pairing bypass) — mirrors BLK-0005 resource gating.
5. **Relay attacks**: proximity is never a security primitive in IRIS; relay
   residual = topology distortion only, bounded by SEC-001 + ROUTE-002
   reputation. Channel Sounding (Bluetooth 6.0, 0.3–1 m) = hardware-gated future
   node, not v1.
6. **On-wire capacity guard**: per-peer in-flight + per-peer GATT write pacing;
   no unbounded buffers (mirrors quota/replay discipline).

---

## 4. Module layout (pure Rust, `crates/iris-core/src/transport/ble.rs` growth)

| Piece | Status | What changes |
|---|---|---|
| `BleAdapter` trait | exists (scaffold) | kept + extended: `scan_results()` added to the trait; doc-comments to the Android FFI contract (§5) |
| `SimulatedBleAdapter` | exists (scaffold) | extended: `scan_results()`, `inject_scan_result()`, `last_writes()`, `connect_count()` for deterministic tests |
| `BleTransport` | exists (scaffold) | hardened + complete (iter 86): advertise-parse discovery, MTU-negotiate-late segmenter, per-peer connection reuse, scan backoff, per-peer poller lifecycle (aborted on shutdown RED-0009-02) |
| `ble_att.rs` | new | `AttSegmenter` + `Reassembler` + `FrameError` (iter 86): payload ≤ min(mtu_effective−5,512); `on_mtu_changed` resize; framing/defensive reassembly for inbound |
| `ble_advert.rs` | new | `DiscoveryBeacon` build/parse + `CapabilityBits` + `AdvertError`: strict decoding of the discovery beacon, adversarial tests |
| `AndroidBleAdapter` (FFI contract) | not yet | Android crate (Kotlin) is ANDROID-001; the `BleAdapter` trait IS the FFI contract, `SimulatedBleAdapter` conformance type-checks it (BLK-0005 known_limitation) |
| tests | grew 8 → 33 (transport::ble) | segmenter/reassembly, advertise-parse adversarial, transport lifecycle, scan-restart backoff, rendezvous connect/send/receive via SimulatedBleAdapter, manager registration (AC-1) |

**Capability matrix entry (TRANSPORT-001)**: `caps` in `BleTransport::new` per
BLE.md + RES-0019 — `max_message_size: u16::MAX` (**segmented** — the `AttSegmenter`
fragments payloads ≤65,535 B across GATT writes at the live MTU, so the transport
capability is the wire field cap, not the ATT-attribution-limited 512; MSG-001
fragmentation still applies above that), `supports_broadcast: true`
(advertise-parse discovery), `supports_unicast: true`, `multicast: false`,
`range 1..100 m` (typical 10), `typical_throughput_bps: 1_000_000`,
`cost_class: Free`, `supports_background_android: true` (FGS connectedDevice),
`supports_background_ios: false` (BLE-002).

---

## 5. Android FFI adapter contract (ANDROID-001 seam)

The `BleAdapter` trait IS the FFI contract. Android crate (Kotlin/JNI) must
implement: `start_scan(filter)`, `stop_scan(handle)`, `start_advertising(data)`,
`stop_advertising(handle)`, `connect_gatt(address)`, `disconnect_gatt(handle)`,
`gatt_write(handle,char_uuid,data)`, `set_mtu(handle,mtu)`,
`incoming_gatt_writes()`. Kotlin side maps to `BluetoothLeScanner`/
`BluetoothGattServer`/`BluetoothGatt` with the manifest + FGS declarations of
§2.2. This crate is out of scope for BLE-001's CI (BLK-0005) — the contract +
`SimulatedBleAdapter` conformance is the type-checked guarantee.

---

## 6. Acceptance criteria (BLE-001 AC-1..n — resolves the C2 gap)

Defined in `engineering/PROJECT_GRAPH.yaml` BLE-001 node; each mapped to
evidence. See AC-1..AC-16 there. Highlights:

- **AC-1** Transport trait impl: `BleTransport` registers + selects via
  `TransportManager` like INTERNET-001 (capabilities/eligibility test).
- **AC-2** MTU negotiate-late: segmenter respects 23 default → 512 on
  `on_mtu_changed`; max ATT payload ≤512 B (RES-0019 R1).
- **AC-3** Discovery = advertise-parse (Extended/Periodic beacon) → produces
  candidate peers; stop_discovery stops scan; handle round-trip (scaffold
  already covers).
- **AC-4** Connect/send/receive E2E via `SimulatedBleAdapter` (mock peer):
  payload → segment → GATT write → inbound reassembly → `IncomingMessage`.
- **AC-5** Scan-restart backoff: ≥N start/stop bursts never blind-restart; error
  callbacks propagate (RES-0019 R6.2).
- **AC-6** Adversarial advertise-parse: malformed/truncated/oversized beacons
  rejected without panic/alloc blowup (property + regression tests).
- **AC-7** App-layer security: every inbound frame passes envelope verify recp
  seam (engine gate); unauthenticated frames only produce candidates (RES-0019 R5).
- **AC-8** Lifecycle: `shutdown()` stops poller + disconnects + Unavailable
  (RED-0009-01/02 regression held).
- **AC-9** Reliability: per-peer single GATT connection reuse; close() promptly;
  GATT-client churn bounded (RES-0019 R6.4/G2).
- **AC-10** Background contract DOCUMENTED (FGS connectedDevice, PendingIntent
  scan, patch-floor) in `BLE.md`/deployment docs (ACCEPTANCE_POLICY.TRANSPORT:
  background capability documented + platform limitations documented).
- **AC-11** `SimulatedBleAdapter`/FFI-contract conformance type-checked (adapter
  trait implemented + clippy-clean) (ACCEPTANCE_POLICY.TRANSPORT: adapter trait
  type-checked; capability matrix completed).
- **AC-12** Doc reconciliation: BLE.md + TRANSPORT_ABSTRACTION.md + BLE-001 node
  agree with code (PollingBleAdapter → poller CLARIFIED; no drift).
- **AC-13** Workspace green + clippy 0 + existing 8 scaffold tests held.
- **AC-14** (gated, recorded) Physical device connect/send/receive + battery
  impact (BENCH) = **known_limitation BLK-0005** — battery measured=estimate
  (RES-0007 gap 3 upgraded: documented design estimate, device measurement gated).
- **AC-15** Security review (redteam) findings dispositioned; adversarial
  findings fixed or recorded (SECURITY-review-stage AC, mirrors prior nodes).

---

## 7. Declared decisions (DEC-BLE-0001..n — ratified in DECISIONS.md)

See DECISIONS.md:
- DEC-BLE-0001 v1 = GATT p2p + advertise-parse discovery (RES-0019 R1).
- DEC-BLE-0002 No Mesh 1.1 v1 (R2), DEC-BLE-0003 No PAwR v1 (R3),
  DEC-BLE-0004 No BIS v1 (R4) — all with v2 triggers.
- DEC-BLE-0005 MTU negotiate-late, ≤512 B ATT payloads (R1).
- DEC-BLE-0006 App-layer envelope = trust anchor; OS patch-floor documented (R5).
- DEC-BLE-0007 DLEP deferred as routing-metric interface, RFC 8175 (R8).
- DEC-BLE-0008 Channel Sounding deferred to hardware-gated node (R5/R9).

---

## 8. Risks / known limitations (recorded, not blockers)

- Physical-device tests + battery measurement gated (BLK-0005, RESOURCE after
  DEC-0009). Battery figures are documented design estimates (RES-0007 gap 3).
- Android BLE stack CVEs unpatched-by-app: OS-security-patch-floor documented
  (RES-0019 §4).
- Android 17 scan-throttle enforcement + Android 16 FGS quota: L5 planning inputs
  (G4) — backoff + FGS contract designed against anyway.
- Android-14 MTU "Request Not Supported" wedge (G5) — defensive re-connect path.
- iOS background asymmetry (RES-0019 R7) — BLE-002 scope; discovery beacon must
  fit iOS-compatible advert subset for cross-platform discovery.
- GATT-client registration ceiling ~32 (G2, L5) — connection-reuse design
  bounds it regardless.
- PAwR response-slot transmission impossible on phones (G1) — v2-only gateways.

---
**Next**: IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT scope
= pure-Rust hardening + new modules (§4); platform crate under ANDROID-001.