# BLE-002 IMPLEMENT — BLE Transport (iOS) Rust-core adjustments

**Document ID**: IRIS-BLE-002-IMPLEMENT-001
**Date**: 2026-08-18
**Stage**: IMPLEMENT (iter ~135)
**Status**: COMPLETE — TEST next (AC-1..13 evidence → SECURITY_REVIEW → VERIFY → ACCEPT)

## Scope

Per `docs/implementation/BLE_002_DESIGN.md` §4 — **pure-Rust core adjustments
only** (platform-neutral, no new crate, no Swift/Objective-C). The iOS
`IosBleAdapter` Swift/CoreBluetooth implementation is IOS-001 scope; the
`BleAdapter` trait is the type-checked FFI seam.

## Work performed

1. **`BleAdapter::gatt_read` added to the trait** (`crates/iris-core/src/transport/ble.rs`)
   - `fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError>`
   - Doc-contract: `peripheral.readValue(for:)` on iOS / `readCharacteristic` on Android;
     used for the connect-to-identify discovery path (DEC-BLE-002-0002).
   - Decision ratified as **DEC-BLE-002-0015** (DECISIONS.md).

2. **`SimulatedBleAdapter` SERVES `IRIS_IDENTIFY_CHARACTERISTIC`** — GATT-server
   semantics: `gatt_read` of the identify characteristic returns the sim's
   `identify_data`; reads of any other characteristic return `Err(DeviceNotFound)`
   (matching an absent characteristic on a real adapter). No `supports_identify_read`
   flag exists — the non-serving behavior is the `Err(DeviceNotFound)` path
   (SECURITY_REVIEW iter ~137 correction).

3. **Discovery connect-to-read branch wired**: `BleTransport::discover_peers` on an
   empty-payload scan result (iOS ad: UUID + local name only) connects,
   `adapter.gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC)`, parses via the shared
   `DiscoveryBeacon` core, disconnects, and assembles the candidate — the two-carrier
   contract (DEC-BLE-002-0008). Ad-carried (Android) branch unchanged.

4. **All 7 `BleAdapter` impls updated**: SimulatedBleAdapter, RecordingBleAdapter,
   IosPayloadAdapter, TwoMtuAdapter, FailingWriteAdapter, FailingConnectAdapter (all
   in ble.rs) + `BleBridge` (crates/iris-android/src/bridge.rs). BleBridge returns
   `DeviceNotFound` with a documented carry-forward — Android Kotlin FFI has no read
   op yet (ANDROID-001 follow-up); Android skips connect via ad-carried data.

5. **MTU source abstraction confirmed present** (`ble_att.rs`): `payload_to_mtu`
   maps iOS `maximumWriteValueLength` payload → segmenter MTU (payload+5, cap 512,
   23 default, 0 → negotiated ceiling; degraded ≥20 accepted not error) —
   DEC-BLE-002-0004 already implemented at DESIGN-adjacent prior pass; verified
   in-pass.

6. **No `ble-experimental` feature-gating involved** in this pass (any prior stale
   feature cfg confirmed absent in present tree; `progress.rs` matrix + `Cargo.toml`
   clean).

## New tests (transport::ble module, +5)

| Test | AC | What it proves |
|------|----|----------------|
| `ios_leg_registers_and_is_selectable_via_transport_manager` | AC-1 | BleTransport registers + is selectable as the iOS leg through TransportManager (eligible caps, priority P2 data-rate, iOS-background-extension row) |
| `c001_android_leg_never_probe_connects_empty_ads` | AC-3/6 | Ad-carried (Android) peers never trigger connect/read on any leg — two-carrier asymmetry held (0 candidates + 0 connects on empty ads); added at SECURITY_REVIEW iter ~137 (BLE-RT-C001/C002 closure) |
| `ios_connect_to_identify_yields_candidate_from_identify_characteristic` (extended) | AC-3 | UUID-only iOS ad → connect → read identify char → DiscoveryBeacon parse → candidate peer |
| `ios_mtu_payload_sizes_frames_to_full_reported_budget` | AC-2/4 | 23-B default and 20-B degraded guard frames map to the correct ATT payload budget; 512-B-bound identify payload exercised via the connect-to-identify carrier (DEC-BLE-002-0004) |

Also asserted in existing `capability_matrix_marks_ios_background_limited`:
`supports_background_ios: false` held for the iOS leg.

## Verification evidence

- `cargo test -p iris-core transport::ble` — **53 passed / 0 failed** (was 42 at
  BLE-001; +11 in-pass incl. 5 new BLE-002 tests)
- `cargo test --workspace` — all suites green (531 lib + integration), 0 failures,
  no new warnings
- `cargo clippy --workspace --all-targets` — **0 warnings / 0 errors**
- `cargo fmt --all --check` — clean (post-`cargo fmt --all`; 2 pre-existing diffs
  in ble.rs corrected in-pass)
- AC-11 (clippy + SimulatedBleAdapter conformance) PASS; AC-13 workspace green held.

## Known limitations (carried, recorded in graph)

- Android `BleBridge` `gatt_read` returns `DeviceNotFound` until the Android FFI
  exposes a read op (Android↔iOS identify-read vector = ANDROID-001 follow-up with
  the IOS-001 adapter work).
- Physical-device CoreBluetooth tests require real iPhone (BLK-0005 / AC-14 GATED).

## Files touched (this pass)

- `crates/iris-core/src/transport/ble.rs` (trait + sim + 5 test adapters + tests; fmt)
- `crates/iris-android/src/bridge.rs` (BleBridge gatt_read)
- `engineering/memory/DECISIONS.md` (DEC-BLE-002-0015)
- `engineering/PROJECT_GRAPH.yaml`, `engineering/PROJECT_STATE.yaml`,
  `engineering/memory/execution-state.yaml`, `CURRENT_STATE.md`, `ACTIVE_NODE.md`,
  `NEXT_ACTION.md`, `records/execution-log.md`, `CHANGELOG.md` (this pass)
- This record: `engineering/memory/records/BLE-002_IMPLEMENT.md`

## Next

TEST (iter ~136): AC-1..13 evidence map (`BLE-002_TEST.md`) → SECURITY_REVIEW →
VERIFY → ACCEPT → IOS-001.