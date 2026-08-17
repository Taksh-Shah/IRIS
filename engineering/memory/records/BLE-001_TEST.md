# BLE-001 TEST — Acceptance Evidence Record

**Node**: BLE-001 (P0 TRANSPORT, ANDROID)
**Stage**: TEST (iter 87)
**Date**: 2026-08-16
**Baseline**: IMPLEMENT wiring landed iter 86 (ble_att.rs, ble_advert.rs, ble.rs
hardened); workspace **524 passed / 0 failed / 1 ignored** (iris-core lib 469 +
15 integration suites), clippy **0** (measured iter 87).

---

## Per-AC evidence

| AC | Requirement | Evidence | Status |
|----|-------------|----------|--------|
| **AC-1** | Transport-trait impl registering + selecting via TransportManager like INTERNET-001 | Test `transport::ble::tests::registers_and_is_selectable_via_transport_manager` — real `BleTransport` + `SimulatedBleAdapter` registered with `TransportManager`, selected for a P2/300-B request, `ble-android` id + score > 0. Manager rule 1 (`state >= Available`) holds — BleTransport begins `Available`. | ✅ PASS |
| **AC-2** | MTU negotiate-late: segmenter starts 23-cap, resizes ≤512 B on `on_mtu_changed`, never > min(mtu−5,512) | `ble_att.rs`: `AttSegmenter::new()` defaults `MTU_DEFAULT`=23; `on_mtu_changed` clamps [23,517]; `max_att_payload() = min(mtu−5,512)`; `MAX_MESSAGE_BYTES = u16::MAX`, `MAX_CHUNKS = 255`. Tests: `segmenter_starts_at_default_mtu`, `on_mtu_changed_clamps_and_resizes` (clamp high+low, resize), `default_mtu_forces_many_chunks`, `oversized_declared_total_rejected`. E2E: `large_message_segments_across_mtu_and_reassembles` (30 KB → >10 frames). | ✅ PASS |
| **AC-3** | Discovery = advertise-parse beacon producing candidate PeerInfos; stop_discovery stops scan; scan handle round-trips | `ble_advert.rs` `DiscoveryBeacon::build/parse` (22-B versioned); `BleTransport::discover_peers` drains adapter `scan_results`, parses beacon → `PeerInfo` w/ `("ble", MAC)` transaction address; `stop_discovery` calls `stop_scan`. Tests: `scan_and_advertising_handles_round_trip` (scan + adv handle round-trip), `discover_beacon_maps_to_peer_exactly` (beacon → peer with correct MAC + candidate id prefix), plus `ble_advert` unit suite (8). | ✅ PASS |
| **AC-4** | Connect/send/receive E2E over SimulatedBleAdapter (segment → gatt_write → reassembly → IncomingMessage) | Tests: `simulated_adapter_connects_and_sends` (send receipt bytes_sent), `gatt_write_flows_into_incoming` (inbound reassembled → IncomingMessage w/ correct peer), `large_message_segments_across_mtu_and_reassembles` (30 KB full round-trip, replay in-order), `out_of_order_reassembly_matches_sender_order` (chunks delivered reversed still reassemble). | ✅ PASS |
| **AC-5** | Scan-restart backoff: ≥5/30 s ceiling honored; bursts never blind-restart; backoff arms on refusal | `scan_allowed()`: window `VecDeque` prunes >30 s, refuses at `SCAN_CEILING`=5, arms exponential backoff (1 s doubling to `SCAN_BACKOFF_MAX_S`=30) in `refused_until`; `discover_peers` returns `Err(Protocol "scanned throttled")` when refused. Test: `scan_burst_is_throttled_within_window` (5 allowed, 6th refused w/ throttled message). | ✅ PASS |
| **AC-6** | Adversarial advertise-parse: malformed/truncated/oversized rejected without panic/alloc blowup | `ble_advert.rs` strict parse: `TooShort` <22 B, `UnsupportedVersion`, `UnknownKind`, `ReservedBits`; exhaustive short-len sweep `all_len_2_to_22_rejected_no_panic`; oversized-padded tolerated (BLE advert padding) `oversized_payload_tolerated_without_panic`. `ble_att.rs` reassembler: `too_short_frame_rejected`, `bad_chunk_index_rejected`, `chunk_count/count` bounds, `partials_bounded` (MAX_PARTIALS=64). Transport-level `malformed_gatt_writes_are_dropped_not_panicked` (~5 polluted writes incl. wrong char_uuid + truncated + count-overflow, then transport still works). | ✅ PASS |
| **AC-7** | App-layer security: every inbound frame passes engine envelope-verify seam; unauthenticated connectionless frames only yield candidates, never triggers | Inbound: per-peer poller reassembles frames → `IncomingMessage` on `incoming_tx` broadcast (the engine's envelope-verify/gate input stream, MSG-001/SEC-001 boundary). Connectionless: `discover_peers` only ever yields *candidate* `PeerInfo` (DEC-BLE-0006); no payload action from the beacon. `candidate_peer_id()` zero-pads (explicitly not a trust/value). Wire cap per-peer `MAX_PARTIALS` + TTL prevent alloc blowup (see AC-2/6 tests + design §3). | ✅ PASS |
| **AC-8** | Lifecycle: shutdown() aborts poller + disconnects GATT + state Unavailable (RED-0009-01/02 held) | `shutdown()` drains `pollers` (abort each), sets `Unavailable`, disconnects every live GATT handle. Tests: `shutdown_sets_unavailable_and_disconnects` (RecordingBleAdapter records 1 disconnect, state Unavailable), `send_before_connect_errors` (NotConnected). Per-peer poller is a spawned task with `AbortHandle` in `pollers` — RED-0009-02 regression held. | ✅ PASS |
| **AC-9** | Reliability: per-peer single GATT connection reused; churn bounded | `connections: HashMap<PeerId,(GattHandle,bool)>`; `connect()` on an already-connected peer returns the live link without a new `connect_gatt`. Test: `reconnect_reuses_live_connection` (2× connect → `inner_connect_count()==1`). | ✅ PASS |
| **AC-10** | Background + platform limitations DOCUMENTED (FGS connectedDevice API 34+, PendingIntent filtered scan, batch+flush, OS patch-floor) | `BLE_TRANSPORT_DESIGN.md` §2.2 platform contract (FGS `connectedDevice`, PendingIntent background scan, batch+flush, runtime `isLePeriodicAdvertisingSupported`); §3.4 OS security-patch floor (CVE list, OS-patch-gated); capability matrix `supports_background_android: true` / `supports_background_ios: false`. ACCEPTANCE_POLICY.TRANSPORT: platform-limitations documented. (Doc reconciliation item AC-12 verified below.) | ✅ PASS |
| **AC-11** | FFI contract type-checked: BleAdapter trait implemented + clippy-clean; SimulatedBleAdapter conformance | `BleAdapter` trait with all 9 FFI ops (`start_scan`, `stop_scan`, `start_advertising`, `stop_advertising`, `connect_gatt`, `disconnect_gatt`, `gatt_write`, `set_mtu`, `incoming_gatt_writes`, `scan_results`) implemented by `SimulatedBleAdapter` + test `RecordingBleAdapter`; `cargo clippy --workspace --all-targets` **0 warnings**. | ✅ PASS |
| **AC-12** | Doc reconciliation: BLE.md + TRANSPORT_ABSTRACTION.md + BLE-001 node agree with code | iter 87: `BLE_TRANSPORT_DESIGN.md` §4 capability-matrix row corrected (`max_message_size` = segmented `u16::MAX`, not the 512 ATT cap) + module-layout table refreshed (ble_att.rs/ble_advert.rs actual files, Android FFI contract = the trait, `scan_results` added). `PollingBleAdapter` vs poller-task clarification already recorded in design v1.0. BLE.md/TRANSPORT_ABSTRACTION referenced, no drift on adapter seam. | ✅ PASS |
| **AC-13** | Workspace green + clippy 0; existing 8 scaffold tests held | `cargo test --workspace` = **524 passed / 0 failed / 1 ignored** (iris-core lib 469 + 15 integration suites); all 8 original scaffold tests present + passing (no_adapter, simulated_adapter_connects_and_sends, gatt_write_flows_into_incoming, send_before_connect_errors, shutdown, handles round-trip, capability matrix, mac parser); clippy 0. | ✅ PASS |
| **AC-14** | GATED/BLK-0005 (recorded): physical-device connect/send/receive + battery BENCH = known_limitation | Not executed — recorded gated (PROJECT_GRAPH known_limitations; battery = documented design estimate per RES-0007 gap 3). | ⛔ GATED (known_limitation) |
| **AC-15** | SECURITY_REVIEW: redteam adversarial findings dispositioned | Deferred to SECURITY_REVIEW stage (iter 88). | ⏭ NEXT |
| **AC-16** | VERIFY: independent verification doc evidence table; verifier APPROVE | Deferred to VERIFY stage (iter 89). | ⏭ NEXT |

---

## Test inventory (iter 87)

`cargo test -p iris-core --lib transport::` — 33 BLE tests pass:

| Module | Count | Coverage |
|--------|-------|----------|
| `ble.rs` (transport) | 15 | E2E connect/send, 30-KB segment+reassemble, out-of-order, malformed-drop, beacon discovery, scan throttle + backoff, reconnect reuse, shutdown disconnect, handles round-trip, manager registration, capability matrix, mac parser, no-adapter, send-before-connect |
| `ble_advert.rs` | 8 | build/parse round-trip, too-short, unsupported version, unknown kind, reserved bits, exhaustive short-len, oversized-padded, candidate-id padding |
| `ble_att.rs` | 10 | default MTU, on_mtu_changed clamp, single/multi-chunk round-trip, out-of-order, bad index, oversized total, too-short, stale eviction, partials bound |

## Workspace

- `cargo test --workspace`: **524 passed / 0 failed / 1 ignored** (iris-core lib
  469 + crypto_e2e 3, ml 8, obs 4, sim 8+1 ignored, desktop 5, commands 3,
  engine_roundtrip 3, storage 7, m3 1, pg_store 13)
- `cargo clippy --workspace --all-targets`: **0 warnings**

## Next

- SECURITY_REVIEW (AC-15): redteam adversarial review of ble_att / ble_advert /
  ble.rs transport wiring → findings dispositioned.
- VERIFY (AC-16): independent evidence reproduction → ACCEPT → node transition
  to WIFIAWARE-001.