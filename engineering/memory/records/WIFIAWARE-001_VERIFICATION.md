# WIFIAWARE-001 Verification (VERIFY stage)

- **Node**: WIFIAWARE-001 — Wi-Fi Aware (NAN) Transport (P1 TRANSPORT, ANDROID)
- **Stage**: VERIFY — acceptance evidence for ACCEPT
- **Version**: v1.0
- **Date**: 2026-08-16
- **Precedents**: `INTERNET_TRANSPORT_VERIFICATION.md` (reference transport
  impl), `BLE_001_VERIFICATION.md` v1.0 (AC-1..16 + verifier reproduction
  pattern), `SEC_001_VERIFICATION.md`, `IDENT_VERIFICATION.md`

## Acceptance-criteria outcome

**All 16 acceptance criteria PASS / GATED (recorded).** AC-1..13 verified in
TEST stage (`WIFIAWARE-001_TEST.md`, iter 95); AC-14 recorded-gated
(BLK-0005 — device-only measurement, documented known_limitation);
AC-15 SECURITY_REVIEW resolved iter 96 + independent re-review iter 100
(`WIFIAWARE-001_SECURITY_REVIEW.md` v2); AC-16 this document (independent
verifier reproduction below).

Honest-evidence rule honored: every figure below was re-verified live in this
pass (workspace 552/0/1, `transport::wifiaware` 19, beacon 8, clippy 0, all
regression tests named and present). No unverified claim enters this record.

## AC → evidence table

| AC | Criterion (from PROJECT_GRAPH.yaml) | Evidence | Status |
|----|--------------------------------------|----------|--------|
| AC-1 | WifiAwareTransport is a Transport-trait impl registering + selecting via TransportManager like INTERNET-001 | `transport::wifiaware::tests::registration_gating` — real `WifiAwareTransport` + `SimulatedWifiAwareAdapter` registered with `TransportManager`; Unavailable → empty selection (manager rule 1 `state >= Available`), selectable once Available; transport_id `wifi-aware-0`, display `Wi-Fi Aware`, WIFI_AWARE_COST registered | PASS |
| AC-2 | Discovery = publish/subscribe: unsolicited+PASSIVE default + solicited+ACTIVE emergency; signed beacon in service_specific_info produces candidates; stop_discovery stops; handle round-trip | `WifiAwareAdapter::publish` registers beacon in `SimMeshCoordinator`; `subscribe` arms discovery; `matches()` returns advertised peers' `service_specific_info`; `discover_peers` parses each beacon (`WifiAwareBeacon::parse`) → candidate `PeerInfo` (`{"wifi-aware", peer_handle}`), honoring `config.max_peers` + `config.filter`; `stop_discovery` → `unsubscribe`. Test `discovery_finds_advertising_peer` | PASS |
| AC-3 | Runtime capability gate: missing feature / disabled / state-changed => degrades to Unavailable + no peers (no fixed API-level assumption); re-available => re-attach | `start_advertising` + `connect` gate on `adapter.is_available()` → `TransportError::NotSupported` (no fixed-API assumption — trait carries `is_available()`); `send` gates `is_available() || state == Unavailable` → `NotSupported`; availability re-checked after `open_ndp().await` (NEW-WA-RT-103); watcher passthrough + restore honors existing links. Test `availability_churn_keeps_discovery_up` (display-off sim: publish rejected, discovery-only survives) | PASS |
| AC-4 | NDP connect/send/receive E2E via SimulatedWifiAwareAdapter: request→accept→INTERNET-001-framed payload→inbound→engine seam; onLost => pool evict + re-discover | Test `ndp_roundtrip_delivers_payload` — A `connect`s to B (open_ndp proxy-routed), `send` frames `internet::encode_frame` (1 MiB cap), `ndp_send` → mesh → B's poller drains `incoming_ndp` → `frame_payload_len` + u32-LE decode → `IncomingMessage` on `incoming_tx` (engine envelope-verify seam). **Strengthened iter 100 (NEW-WA-RT-101): asserts `got.peer_id == A's beacon PeerId` (real sender, never `[0u8;32]`)**. `links` keyed by PeerId; reuse reopens single NDP; onLost teardown evicts | PASS |
| AC-5 | Availability refresh: state-change callback + isAvailable() + characteristics bounds exercised | Poller spawned once (`AsyncMutex<Option<AbortHandle>>` re-spawn guard); `state` Unavailable→Available→Connected→Unavailable on `state_tx` (64-cap broadcast); `is_available()` on advertise/send/connect paths; `MAX_NAN_MESSAGE_BYTES` = 1 MiB bound; beacon ≤ `MAX_SERVICE_SPECIFIC_INFO_BYTES` = 255 enforced (WAW-RT-005 `BeaconError::TooLong`). Test `availability_churn_keeps_discovery_up` | PASS |
| AC-6 | Adversarial beacon parse: malformed/truncated/oversized service_specific_info rejected without panic/allocation blowup (property + regression tests) | `wifiaware_beacon.rs` strict parse: `TooShort`, `UnsupportedVersion`, `UnknownKind`, `ReservedBits`, `TooLong` (>255, WAW-RT-005); exhaustive short-length sweep `too_short_rejected_no_panic` (0..22 rejected); `oversized_padded_service_info_tolerated_without_panic`; `oversized_past_cap_rejected_no_panic` (277 B → TooLong, exactly-255 ok); `candidate_peer_id_pads_short_id`. 8 beacon + 19 transport tests — all allocation-bounded on fixed 22-byte prefix | PASS |
| AC-7 | App-layer security: every inbound IRIS message passes engine envelope-verify seam; unauthenticated NDP/discovery only produces candidates; identity by public-key fingerprint, never NAN MAC/PeerHandle | `discover_peers` yields candidate-only `PeerInfo` (DEC-WA-0007/108); NDP inbound only to `incoming_tx` (engine gate) — no payload-triggered state change (positive control 3); NAN MAC/`PeerHandle`/`NdpHandle` opaque, never identity; `peer_for_ndp` fallback all-zero PeerId (never attacker-controlled). **NEW-WA-RT-101 fixed**: `IncomingNdpData.sender: Option<PeerId>` from beacon-derived candidate; zeros only as documented fallback | PASS |
| AC-8 | Lifecycle: shutdown() stops discovery + closes all NDP + state Unavailable; attach failure retries with backoff (no unbounded retry loop) | `shutdown()` takes `connect_gate`, latches `shutdown_flag: AtomicBool`, aborts poller + watcher, closes all NDPs (via adapter `shutdown`), clears links → Unavailable; `ensure_started()` checks `shutdown_flag` + holds gate across bring-up → **resurrection impossible after shutdown (NEW-WA-RT-102)**. Test `shutdown_returns_to_unavailable` (state Unavailable, send errors) + `connect_racing_shutdown_leaks_no_link` (concurrent connect×shutdown → Unavailable, links empty) | PASS |
| AC-9 | Reliability: per-peer single NDP reused; close promptly; NDP pool bounded by characteristics data-path counts (no unbounded churn) | Single `NdpLink` per PeerId in `links`; **`connect_gate: AsyncMutex<()>` serializes whole open critical section (WAW-RT-001)** — reuse check → pool check (`MAX_NDP_POOL`) → `open_ndp().await` → state re-check → `links.push` atomic; test `concurrent_connects_open_single_ndp` (16 racing connects → exactly 1 link). Teardown gated by `is_link_loss_error` (NEW-WA-RT-107) — transient congestion keeps link, terminates promptly on "not open"/"closed"/etc. | PASS |
| AC-10 | Background + platform limitations DOCUMENTED in WIFI_AWARE.md: FGS connectedDevice (API 34+), Suspend/Resume (API 34+) power lever, OS patch floor, 6 GHz gating | `WIFI_AWARE_TRANSPORT_DESIGN.md` §2.2 platform contract (runtime FEATURE_WIFI_AWARE/isAvailable()/ACTION_WIFI_AWARE_STATE_CHANGED gate; FGS connectedDevice API 34+ required for production discovery; Suspend/Resume API 34+ power lever; G-WA-2 OS patch floor incl. iwlwifi CVE-2024-49857; G-WA-3 6 GHz/6E hardware+region gating). Capability matrix `supports_background_android: true` / `supports_background_ios: false` | PASS |
| AC-11 | FFI contract type-checked: WifiAwareAdapter trait implemented + clippy-clean; SimulatedWifiAwareAdapter conformance | `WifiAwareAdapter` trait 12 ops (`start`, `subscribe`, `unsubscribe`, `publish`, `unpublish`, `matches`, `open_ndp`, `close_ndp`, `ndp_send`, `incoming_ndp`, `shutdown`, `is_available`) implemented by `SimulatedWifiAwareAdapter` + `SimMeshCoordinator`; trait doc pins `IncomingNdpData.ndp` = receiver-local handle + `sender` semantics (NEW-WA-RT-101, ANDROID-001 contract). `cargo clippy -p iris-core --all-targets` **0 warnings** | PASS |
| AC-12 | Doc reconciliation: WIFI_AWARE.md + TRANSPORT_ABSTRACTION.md + WIFIAWARE-001 node agree with code incl. CONFLICT-1 (Apple WiFiAware iOS 26+; BLE-002 v1 iOS path; Android<->Apple NDP interop immature; code caps stay Android v1) | iter 93: `WIFI_AWARE.md` iOS section corrected + background-FGS claim dated; `TRANSPORT_ABSTRACTION` matrix corrected (Android Background → Yes (FGS); iOS row → iOS 26+/iPhone 12+, BLE-002 = v1 iOS P2P path; DEC-WA-0008 interop immature); node `known_limitations` keeps code Android v1 (`supports_background_ios: false`). No drift on the adapter seam | PASS |
| AC-13 | Workspace green + clippy 0; baseline 533/0/1 held | **`cargo test --workspace` = 552 passed / 0 failed / 1 ignored** (497 iris-core lib incl. `transport::wifiaware` module 11 + `wifiaware_beacon` 8 = 19 under shared filter prefix); `cargo clippy -p iris-core --all-targets` = 0 warnings; rustfmt clean. Baseline held. | PASS |
| AC-14 (GATED/BLK-0005) | Physical-device NDP discovery/data/battery BENCH = known_limitation | Not executed — recorded gated (PROJECT_GRAPH known_limitations; battery figures = documented design estimates per RES-0020/WIFI_AWARE.md) | ⛔ GATED (known_limitation) |
| AC-15 | SECURITY_REVIEW: redteam adversarial findings dispositioned | `WIFIAWARE-001_SECURITY_REVIEW.md` v2 (iter 96 + independent re-review iter 100): **Pass 1 WAW-RT-001..005 (1 HIGH + 1 MED + 3 LOW) ALL FIXED**; **Pass 2 NEW-WA-RT-101..112 (1 HIGH + 6 MED + 5 LOW) — HIGH + 6 MED ALL FIXED in-pass with regression tests, 5 LOW RECORDED as ANDROID-001 FFI requirements**; 8 positive controls held; independent verifier APPROVED | PASS |
| AC-16 | VERIFY: independent verification doc evidence table; verifier APPROVE | This document §"Live verification" + §"Independent verifier (subagent) reproduction" | PASS |

## Live verification (this pass)

| Check | Command | Result |
|-------|---------|--------|
| Workspace | `cargo test --workspace` | **552 passed / 0 failed / 1 ignored** (iris-core lib 497 + crypto_e2e 3, ml 8, obs 4, sim 8+1 ignored, desktop 5, commands 3, engine_roundtrip 3, storage 7, m3 1, pg_store 13) |
| Wi-Fi Aware transport | `cargo test -p iris-core --lib transport::wifiaware` | **19 passed / 0 failed** (filter prefix `transport::wifiaware` matches both modules: **11 transport + 8 beacon**) |
| Beacon | `cargo test -p iris-core --lib transport::wifiaware_beacon` | **8 passed / 0 failed** |
| Clippy | `cargo clippy -p iris-core --all-targets` | **0 warnings** |
| RT/NEW regressions | `cargo test -p iris-core --lib transport::wifiaware` | 11 present + passing: `concurrent_connects_open_single_ndp`, `connect_racing_shutdown_leaks_no_link`, `outbox_is_bounded_under_flood`, `transient_send_error_keeps_link`, `last_link_death_while_unavailable_is_degraded`, `connect_while_unavailable_is_not_supported`, strengthened `ndp_roundtrip_delivers_payload` + base `registration_gating`, `discovery_finds_advertising_peer`, `availability_churn_keeps_discovery_up`, `shutdown_returns_to_unavailable` |

## Independent verifier (subagent) reproduction

Independent `verifier` subagent dispatched against the workspace (iter 101) with
instructions to reproduce: workspace test totals (552/0/1), clippy (0),
`transport::wifiaware` (19) + `wifiaware_beacon` (8), presence + coverage of the
SECURITY_REVIEW regression set (WAW-RT-001..005 + NEW-WA-RT-101..107 fixes),
the SECURITY_REVIEW v2 dispositions, and absence of any AC row citing a
nonexistent test.

*(Verifier verdict appended on completion — expected APPROVE; any notes
reconciled before ACCEPT.)*

## Independent verifier verdict — APPROVE_WITH_NOTES (reconciled)

Verifier (subagent `ses_ff4f4955dffeEaRGJph0ov2ngs`, iter 101) independently
reproduced: workspace **552 passed / 0 failed / 1 ignored** (per-suite sum
matches exactly), filter `transport::wifiaware` = **19 passed** (11 transport +
8 beacon under shared prefix), beacon alone = **8**, clippy = **0 warnings**;
all 19 named tests exist with correct `#[tokio::test]`/`#[test]` attributes and
all pass; **no AC row cites a nonexistent test/binding**; both HIGH-severity
SECURITY_REVIEW fixes are real code — `connect_gate: AsyncMutex<()>` serializes
the entire connect() critical section (reuse check → pool check → `open_ndp()`
→ post-open re-check → `links.push` → Connected) and shutdown() takes the same
gate + latches `shutdown_flag`; `IncomingNdpData.sender: Option<PeerId>` with
the sim mesh outbox `(to_tag, from_tag, payload)` and poller attribution from
`frame.sender` fully wired, regression asserts `got.peer_id == peer_a` and
`!= [0u8;32]`.

**Single note (reconciled in-pass)**: code `display` string was
`"Wi-Fi Aware (scaffold)"` (iter-94 carryover) vs the doc's `"Wi-Fi Aware"` —
code corrected to `"Wi-Fi Aware"` (matching the design/doc contract and the
sibling naming convention e.g. `"Internet (relay)"`, `"BLE (Android)"`);
`cargo test` 19/19 re-passes and clippy **0 warnings** after the change.

Verdict: **APPROVE** (all evidence real, all counts reproduced, only
cosmetic note reconciled).

## Known limitations (recorded, not blockers)

- Physical-device NDP discovery/data/battery measurement pending hardware
  (BLK-0005, RESOURCE-gated; battery = documented design estimate) — AC-14.
- Wi-Fi Aware availability is runtime-dynamic, not fixed — capability table must
  refresh at runtime (AC-3 implemented); real OEM firmwares vary (G-WA-2 OS
  patch floor required; kernel iwlwifi CVE-2024-49857 patch-gated).
- G-WA-1: NDP MTU is kernel-managed — data path uses dynamic MTU discovery +
  1 MiB framing cap; 20-100 Mbps practical / ~300 Mbps peak is OEM-derived, not
  a guarantee.
- G-WA-3: 6 GHz/6E NAN is hardware + region-gated; v1 targets 2.4/5 GHz floor.
  India ISM regulatory OK on 2.4/5 GHz (U-NII rules vary by band).
- Android<->Apple NDP interop recorded immature (DEC-WA-0008); code stays
  Android v1 (CONFLICT-1); BLE-002 = v1 iOS P2P path.
- ANDROID-001 FFI-contract requirements carry forward from SECURITY_REVIEW
  NEW-WA-RT-108..112: verified-peer NDP reuse key (108), closed-NDP frame prune
  + multi-subscriber availability stream (109), per-call FFI timeouts (110),
  adapter start/subscribe idempotence (111), ring-buffer outbox (112).
- `IncomingNdpData.sender` is best-effort beacon-derived candidate identity;
  authoritative identity remains envelope-verification at the engine seam
  (DEC-WA-0007); all-zero fallback never attacker-controlled.

## Sign-off

Recorded 2026-08-16 (iter 101). WIFIAWARE-001 ready for ACCEPT per AC-1..16
evidence (AC-14 gated by BLK-0005). Transition target: **WIFIDIRECT-001**.