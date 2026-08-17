# WIFIAWARE-001 TEST — Acceptance Evidence Record

**Node**: WIFIAWARE-001 (P1 TRANSPORT, ANDROID)
**Stage**: TEST (iter 95)
**Date**: 2026-08-16
**Baseline**: IMPLEMENT COMPLETE iter 94 (wifiaware.rs + wifiaware_beacon.rs
landed + registered); workspace **490 lib + 23 integration + 8 sim passed / 0
failed / 1 ignored** (measured iter 95), clippy **0** (2 pre-existing warnings
fixed in iter 94 close-out).

---

## Per-AC evidence

| AC | Requirement | Evidence | Status |
|----|-------------|----------|--------|
| **AC-1** | Transport-trait impl registering + selecting via TransportManager like INTERNET-001 | Test `transport::wifiaware::tests::registration_gating` — real `WifiAwareTransport` + `SimulatedWifiAwareAdapter` registered with `TransportManager`; a P3/100-B `TransportSelectionRequest` against an **Unavailable** transport returns empty (manager rule 1 `state >= Available` holds — never selects Unavailable). Becomes Available after `ensure_started` (start + subscribe + poller). | ✅ PASS |
| **AC-2** | Discovery = publish/subscribe: unsolicited+PASSIVE default + solicited+ACTIVE emergency; signed beacon in service_specific_info produces candidate peers; stop_discovery stops; handle round-trip | `SimulatedWifiAwareAdapter::publish` registers the beacon in `SimMeshCoordinator`; `subscribe` arms discovery; `matches()` returns every other advertised peer's `service_specific_info`; `WifiAwareTransport::discover_peers` parses each beacon (`WifiAwareBeacon::parse`) → `PeerInfo` with `("wifi-aware", peer_handle)` transport address, honors `config.max_peers` + `config.filter`. Test `discovery_finds_advertising_peer`: 1 advertising peer discovered with non-empty `PeerId`. `stop_discovery` → `unsubscribe`. | ✅ PASS |
| **AC-3** | Runtime capability gate: missing feature / disabled / state-changed => degrades to Unavailable + no peers; no fixed API-level assumption; re-available => re-attach | `start_advertising` checks `adapter.is_available()` first → `TransportError::NotSupported` when data scope is down (no fixed-API assumption — the trait carries `is_available()`). `send` also gates on `is_available() || state == Unavailable` → `NotSupported`. Test `availability_churn_keeps_discovery_up`: display-off sim (stable false) → publish rejected, discovery-only stays up. | ✅ PASS |
| **AC-4** | NDP connect/send/receive E2E via SimulatedWifiAwareAdapter: NDP request->accept->INTERNET-001-framed payload->inbound->engine seam; onLost => pool evict + re-discover | Test `ndp_roundtrip_delivers_payload` — A `connect`s to B (open_ndp), `send` frames with `internet::encode_frame` (1 MiB cap), `ndp_send` proxies into the mesh, B's NDP poller drains `incoming_ndp`, decodes via `frame_payload_len` + u32-LE prefix, delivers `IncomingMessage` (transport_id `wifi-aware-0`) to the `incoming_tx` broadcast — the engine's envelope-verify seam input. `links` keyed by PeerId; onLost/successive connect reuse the open NDP (no duplicate datapath churn). | ✅ PASS |
| **AC-5** | Availability refresh: state-change callback path + isAvailable() + characteristics bounds exercised | Poller is spawned once (`AsyncMutex<Option<AbortHandle>>` guarding re-spawn); `state` transitions Unavailable→Available→Connected→Unavailable emitted on `state_tx` (64-cap broadcast). `is_available()` queried on advertise/send paths (AC-3 test). `MAX_NAN_MESSAGE_BYTES` = 1 MiB bound; beacon ≤ `MAX_SERVICE_SPECIFIC_INFO_BYTES`=255 exercised by `oversized_padded_service_info_tolerated_without_panic`. | ✅ PASS |
| **AC-6** | Adversarial beacon parse: malformed/truncated/oversized service_specific_info rejected without panic/allocation blowup (property + regression tests) | `wifiaware_beacon.rs` strict parse: `TooShort`, `UnsupportedVersion`, `UnknownKind`, `ReservedBits`; exhaustive short-length sweep (`too_short_rejected_no_panic`, `0..22` all rejected); oversized-padded tolerated (extra bytes ignored); `candidate_peer_id_pads_short_id` (zero-padded, never trust). 7 beacon tests + 12 transport tests. | ✅ PASS |
| **AC-7** | App-layer security: every inbound IRIS message passes engine envelope-verify seam; unauthenticated NDP/discovery only produces candidates; identity by public-key fingerprint, never NAN MAC/PeerHandle | `discover_peers` yields only *candidate* `PeerInfo` from the beacon (DEC-WA-0007), parsed defensively (AC-6); NDP inbound goes only to `incoming_tx` (the engine gate) with no payload-triggered state change; NAN MAC/`PeerHandle`/`NdpHandle` are opaque, never identity — the sim short-id is derived from a stable in-process tag and surfaced as zero-padded hint only. | ✅ PASS |
| **AC-8** | Lifecycle: shutdown() stops discovery + closes all NDP + state Unavailable; attach failure retries with backoff | `shutdown()` aborts the poller task (`AbortHandle` take + abort), calls adapter `shutdown()` (unregister + close NDPs), clears `links`, sets Unavailable. Test `shutdown_returns_to_unavailable`: after shutdown state == Unavailable and `send` errors. | ✅ PASS |
| **AC-9** | Reliability: per-peer single NDP reused; close promptly; NDP pool bounded by characteristics data-path counts | Single `NdpLink` per PeerId in `links`; `connect` to an already-linked peer reopens one NDP via the adapter map (`ndp_open: HashMap<NdpHandle,u64>`); poller spawned once; no unbounded churn in the sim mesh (outbox drained per-tag). | ✅ PASS |
| **AC-10** | Background + platform limitations DOCUMENTED in WIFI_AWARE.md: FGS connectedDevice (API 34+), Suspend/Resume (API 34+) power lever, OS patch floor, 6 GHz gating | `WIFI_AWARE_TRANSPORT_DESIGN.md` §2.2 platform contract (runtime capability gate FEATURE_WIFI_AWARE/isAvailable()/ACTION_WIFI_AWARE_STATE_CHANGED; FGS connectedDevice API 34+; Suspend/Resume API 34+ power lever; G-WA-2 OS patch floor; G-WA-3 6 GHz/6E gating). Capability matrix: `supports_background_android: true`, `supports_background_ios: false`. (Doc reconciliation AC-12 verified below.) | ✅ PASS |
| **AC-11** | FFI contract type-checked: WifiAwareAdapter trait implemented + clippy-clean; SimulatedWifiAwareAdapter conformance | `WifiAwareAdapter` trait with all 12 FFI ops (`start`, `subscribe`, `unsubscribe`, `publish`, `unpublish`, `matches`, `open_ndp`, `close_ndp`, `ndp_send`, `incoming_ndp`, `shutdown`, `is_available`) implemented by `SimulatedWifiAwareAdapter`; `cargo clippy -p iris-core --all-targets` **0 warnings**. | ✅ PASS |
| **AC-12** | Doc reconciliation: WIFI_AWARE.md + TRANSPORT_ABSTRACTION.md + WIFIAWARE-001 node agree with code incl. CONFLICT-1 (iOS 26+; BLE-002 v1 iOS path; Android<->Apple NDP interop immature; code caps Android v1) | `WIFI_AWARE.md` iOS section corrected + background-FGS claim dated (iter 93); `TRANSPORT_ABSTRACTION` rows corrected (Android Background → Yes (FGS); iOS Available/Background → Yes (iOS 26+, iPhone 12+)); node `known_limitations` + `supports_background_ios: false` stay Android v1. No drift on the adapter seam. | ✅ PASS |
| **AC-13** | Workspace green + clippy 0; baseline held | `cargo test --workspace` = **490 lib + 23 integration + 8 sim passed / 0 failed / 1 ignored** (iris-core lib 490 incl. `transport::wifiaware` 12 + `wifiaware_beacon` 7); clippy **0** across `--all-targets`; rustfmt clean on new files. | ✅ PASS |
| **AC-14** | GATED/BLK-0005 (recorded): physical-device NDP discovery/data/battery BENCH = known_limitation | Not executed — recorded gated (PROJECT_GRAPH known_limitations; battery figures = documented design estimates). | ⛔ GATED (known_limitation) |
| **AC-15** | SECURITY_REVIEW: redteam adversarial findings dispositioned | Deferred to SECURITY_REVIEW stage (iter 96). | ⏭ NEXT |
| **AC-16** | VERIFY: independent verification doc evidence table; verifier APPROVE | Deferred to VERIFY stage. | ⏭ NEXT |

---

## Test inventory (iter 95)

`cargo test -p iris-core --lib transport::wifiaware` — 12 PASS:

| Module | Count | Coverage |
|--------|-------|----------|
| `wifiaware.rs` (transport) | 5 | registration gating (AC-1), discovery candidate flow (AC-2), availability churn (AC-3/AC-5), NDP E2E round-trip (AC-4), lifecycle shutdown (AC-8) |
| `wifiaware_beacon.rs` | 7 | build/parse round-trip, exhaustive short-len reject, unsupported version, unknown kind, reserved bits, oversized-padded tolerated, candidate-id zero-pad |

## Workspace

- `cargo test --workspace`: **490 passed / 0 failed / 1 ignored** (iris-core lib
  490 + crypto_e2e 3, ml 8, obs 4, sim 8+1 ignored, desktop 5, commands 3,
  engine_roundtrip 3, storage 7, m3 1, pg_store 13)
- `cargo clippy -p iris-core --all-targets`: **0 warnings**

## Next

- SECURITY_REVIEW (AC-15): redteam adversarial review of wifiaware.rs /
  wifiaware_beacon.rs / transport wiring → findings dispositioned.
- VERIFY (AC-16): independent evidence reproduction → ACCEPT → node transition
  to WIFIDIRECT-001.