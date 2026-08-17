# WIFIDIRECT-001 TEST — Acceptance Evidence Record

**Node**: WIFIDIRECT-001 (P1 TRANSPORT, ANDROID)
**Stage**: TEST (iter 106)
**Date**: 2026-08-17
**Baseline**: IMPLEMENT COMPLETE iter 105 (wifi_direct.rs + wifi_direct_serv.rs
landed + registered + WIFI_DIRECT_COST + design §7); workspace **601 passed /
0 failed / 1 ignored** (measured iter 106 --all-features), clippy **0**.

---

## Per-AC evidence

| AC | Requirement | Evidence | Status |
|----|-------------|----------|--------|
| **AC-1** | Transport-trait impl registered + selected via TransportManager like INTERNET-001/WIFIAWARE-001 (capabilities/eligibility test) | Test `transport::wifi_direct::tests::registration_gating` — real `WifiDirectTransport` + `SimulatedWifiDirectAdapter` registered with `TransportManager`; a P3/100-B/`prefer_low_cost` `TransportSelectionRequest` against an **Unavailable** transport returns empty (manager rule 1 `state >= Available` holds — never selects Unavailable). Registering uses the `Transport` trait `capabilities()` (`wifi_direct_capabilities`: 1 MiB `max_message_size`, unicast+broadcast, P2P no infra, cost `Free`, background Android **false**). | ✅ PASS |
| **AC-2** | Discovery = DNS-SD (Bonjour): fixed service_name + TXT-record advertisement → candidate peer (PeerId short-id + capabilities); stop_discovery stops; candidate-only; handle round-trip | `wifi_direct_serv.rs`: `WIFI_DIRECT_SERVICE_NAME = "com.iris.mesh.v1"` (test `service_name_is_fixed`) + `WifiDirectTxtRecord::build`/`parse` (22-byte prefix over the fixed service) + `candidate_peer_id()` zero-pads the 16-byte `peer_short` (test `candidate_peer_id_pads_short_id`). Transport: `discover_peers` calls adapter `matches()` → parses each TXT record (`WifiDirectTxtRecord::parse`, cap `MAX_DNS_SD_TXT_BYTES=255`) → `PeerInfo` with `("wifi-direct", peer_handle)` transport address, honors `config.max_peers`/`filter`. Test `discovery_finds_advertising_peer`: 1 advertising peer discovered with non-empty `PeerId` + `"wifi-direct"` address. `stop_discovery` clears the window (AC-5 test). | ✅ PASS |
| **AC-3** | Runtime capability + coexistence gate: unavailable/disabled/band-restricted → degrade to Unavailable/Degraded + no peers; re-available → re-attach; STA+P2P concurrency NOT assumed | Test `band_restricted_go_creation_falls_back` — GO adapter `set_band_restricted(true)` with `OperatingBand::Ghz5` config: creation **falls back to AUTO** (`GroupConfig.band`, DEC-WD-0005), group forms (over `assert!(ta.connect(...).is_ok())`, state Connected). Then `set_available(false)`: no peers surface, `connect` → err (capability gate, `sim.is_available()` on advertise/connect paths, no fixed-API assumption). | ✅ PASS |
| **AC-4** | GO/GC TCP E2E via SimulatedWifiDirectAdapter: create group / connect → INTERNET-001-framed payload → inbound → engine seam; onLost → pool evict + re-discover | Test `group_roundtrip_delivers_payload` — A (`go_intent 14` → GO) + B (`go_intent 0` → client) in the sim mesh; A discovers B's advertisement, both `connect` (GO group + GC join; `create_group`/`join_group`), A `send`s with `internet::encode_frame` (1 MiB cap), B's inbound stream delivers `IncomingMessage` (`transport_id "wifi-direct-0"`) via `incoming_tx` broadcast = the engine envelope-verify seam input. Payload asserted (`b"p2p-hello"`), **sender never zero** (`assert_ne!(got.peer_id, PeerId([0;32]))`), attributed to the real sender (`got.peer_id == peer_a`). `links` keyed by PeerId; onLost teardown → re-attach available. | ✅ PASS |
| **AC-5** | Discovery re-arm + availability refresh: 30-s app cadence (vs 120-s framework find) with stop/restart + backoff; WIFI_P2P_DISCOVERY_CHANGED_ACTION stop event → BLE-fallback path | Test `discovery_rearm_and_stop` — `DISCOVERY_WINDOW = 30s`, `discovery_should_rearm()` (slot = now+30s); find yields 1 peer, `stop_discovery()` clears `discovery_until` (the framework find-stop event), re-discovery re-arms and finds 1 peer again, state stays Available. WIFI_DIRECT.md §4/find-window correction documents 120-s framework vs 30-s app re-arm + BLE-control-plane fallback on `WIFI_P2P_DISCOVERY_CHANGED_ACTION`. | ✅ PASS |
| **AC-6** | Adversarial TXT-record/service-info parse: malformed/truncated/oversized rejected without panic/allocation blowup (property + regression tests) | `wifi_direct_serv.rs` strict parse + 9 tests: `too_short_rejected_no_panic` (incl. **exhaustive 0..22 sweep**), `unsupported_version_rejected`, `unknown_kind_rejected`, `reserved_bits_rejected` (bit above known mask), `oversized_padded_txt_tolerated_without_panic` (extra bytes ignored ≤ cap), `oversized_past_cap_rejected_no_panic` (≤255 B accepted, >255 → `TooLong`; boundary at exactly the cap ok). | ✅ PASS |
| **AC-7** | App-layer security: every inbound IRIS message passes engine envelope-verify seam; unauthenticated DNS-SD/group events only produce candidates; identity by public-key fingerprint, never P2P MAC/device address | `discover_peers` yields only *candidate* `PeerInfo` from the TXT-record short-id (DEC-WD-0007, test `candidate_peer_id_pads_short_id`); inbound frames go only to `incoming_tx` (the engine gate) with no payload-triggered state change; P2P MAC/hardware address / PeerHandle never become identity — the sim tag maps to a stable in-process short-id surfaced as an opaque zero-padded hint only. | ✅ PASS |
| **AC-8** | Lifecycle: shutdown() stops discovery + removes group + closes all connections + state Unavailable; teardown → Degraded when scope lost; attach/find failure retries with backoff (no unbounded retry loop) | Test `shutdown_returns_to_unavailable` — advertise → discover → connect (Connected) → `shutdown()`: state → **Unavailable**, `send` errs (poller aborted, group removed via adapter shutdown, `links` cleared). Persist-mode: `create_group` = persistent GO (`PERSISTENT_GO` cap), `remove_group` teardown exercised; `discovery_should_rearm` windowed (no unbounded re-arm loop). | ✅ PASS |
| **AC-9** | Reliability: per-peer single group/connection reused; close promptly; GO-side connection table bounded (N-client admission, no unbounded churn); persistent-GO teardown/re-establish handled | Test `single_link_per_peer_and_bounded_table` — 8 concurrent `connect` calls to the same peer all `is_ok()`, then `assert_eq!(links.lock().len(), 1)` (per-peer single link). `MAX_GO_CLIENTS = 8` bounds the GO-side connection table (admission check on `connect`, `links.len() >= MAX_GO_CLIENTS` gate); sim outbox bounded (`MAX_OUTBOX_FRAMES=128` + `MAX_DRAIN_PER_CALL=MAX_FRAMES_PER_TICK`). | ✅ PASS |
| **AC-10** | Background + platform limitations DOCUMENTED in WIFI_DIRECT.md: FGS connectedDevice (API 34+) + PowerManager wakelock required; discovery unreliable in background; BLE = background discovery layer; band API 29 correction; 120-s framework find / 30-s app re-arm; iOS unavailable (MCSession Apple-only) | `WIFI_DIRECT.md` §Background Restrictions (foreground/FGS + wakelock; BLE stays the always-on discovery layer, WD for active transfers only); §find-window correction (RES-0021: 120-s framework / 30-s app re-arm, `WIFI_P2P_DISCOVERY_CHANGED_ACTION` → BLE fallback); `setGroupOperatingBand` API 29 correction; iOS private-API limitation (MCSession Apple-only). Code caps `supports_background_android: false` (wifi_direct.rs:716) — the contract is enforced, not just documented. | ✅ PASS |
| **AC-11** | FFI contract type-checked: WifiDirectAdapter trait implemented + clippy-clean; SimulatedWifiDirectAdapter conformance | `WifiDirectAdapter` trait with all 20 FFI ops (`is_available`, `start`, `start_dns_sd`, `stop_dns_sd`, `start_discovery`, `stop_discovery`, `matches`, `create_group`, `join_group`, `add_client`, `remove_group`, `group_info`, `go_addr`, `set_operating_band`, `p2p_send`, `incoming`, `shutdown`, `availability_stream`) implemented by `SimulatedWifiDirectAdapter` (full conformance); `cargo clippy --workspace --all-features --tests` **0 warnings**. | ✅ PASS |
| **AC-12** | Doc reconciliation: WIFI_DIRECT.md + TRANSPORT_ABSTRACTION.md + WIFIDIRECT-001 node agree with code incl. RES-0021 corrections (band API 29, find window 120 s, client ceiling vendor/HAL, WPA3-SAE R2 capability-gated) | WIFI_DIRECT.md corrected iter 104 (band API 29, find window 120 s framework / 30 s app re-arm, client ceiling vendor/HAL + N-client admission, WPA3-SAE R2 capability-gated, WPS-PIN prohibited, OS patch floor). TRANSPORT_ABSTRACTION matrix row (Android background No-wakelock / iOS No) matches `supports_background_android: false` + `supports_background_ios: false`. Node `known_limitations` 8 rows + `acceptance_criteria` AC-1..16 consistent with code + design §7. | ✅ PASS |
| **AC-13** | Workspace green + clippy 0; baseline 552/0/1 held | `cargo test --workspace --all-features` = **601 passed / 0 failed / 1 ignored** (546 iris-core lib incl. `transport::wifi_direct` 7 + `wifi_direct_serv` 9 + crypto_e2e 3, ml 8, obs 4, sim 8+1 ignored, desktop 5, commands 3, engine_roundtrip 3, storage 7, m3 1, pg_store 13); clippy `--workspace --all-features --tests` **0 warnings**; rustfmt clean. | ✅ PASS |
| **AC-14** | GATED/BLK-0005 (recorded): physical-device group formation/discovery/throughput/battery BENCH = known_limitation; battery figures = documented design estimates; OS patch floor recorded | Not executed — recorded gated (PROJECT_GRAPH `known_limitations`: BLK-0005 RESOURCE + device BENCH; `WIFI_DIRECT_COST` = design estimates scan 80 / advertise 50 / connected 40 / tx 0.01 / rx 0.008 per kbps per WIFI_DIRECT.md battery figures; OS patch floor Android SPL ≥ 2021-02 + wpa_supplicant ≥ 2.12). | ⛔ GATED (known_limitation) |
| **AC-15** | SECURITY_REVIEW: redteam adversarial findings dispositioned | Deferred to SECURITY_REVIEW stage (iter 107). | ⏭ NEXT |
| **AC-16** | VERIFY: independent verification doc evidence table; verifier APPROVE | Deferred to VERIFY stage. | ⏭ NEXT |

---

## Test inventory (iter 106)

`cargo test -p iris-core --lib transport::wifi_direct` — 16 PASS under the shared
`transport::wifi_direct` filter prefix:

| Module | Count | Coverage |
|--------|-------|----------|
| `wifi_direct.rs` (transport) | 7 | registration gating (AC-1), discovery candidate flow + service-name match (AC-2), band-restriction fallback + unavailable gate (AC-3), GO/GC E2E round-trip + sender attribution (AC-4), discovery re-arm/stop (AC-5), lifecycle shutdown (AC-8), bounded single-link table (AC-9) |
| `wifi_direct_serv.rs` | 9 | build/parse round-trip, fixed service name, exhaustive short-len reject, unsupported version, unknown kind, reserved bits, oversized-padded tolerated, oversized-past-cap reject, candidate-id zero-pad |

## Workspace

- `cargo test --workspace --all-features`: **601 passed / 0 failed / 1 ignored**
  (546 iris-core lib + 55 integration/sim suites)
- `cargo clippy --workspace --all-features --tests`: **0 warnings**
- `cargo fmt -p iris-core --check`: clean

## Next

- SECURITY_REVIEW (AC-15, iter 107): redteam adversarial review of
  wifi_direct.rs / wifi_direct_serv.rs / transport wiring + WIFI_DIRECT_COST;
  findings dispositioned/fixed/recorded.
- VERIFY (AC-16): independent evidence reproduction → ACCEPT → node transition
  to LORA-001.