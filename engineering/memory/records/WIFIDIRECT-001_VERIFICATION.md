# WIFIDIRECT-001 Verification (VERIFY stage)

- **Node**: WIFIDIRECT-001 — Wi-Fi Direct (P2P) Transport (P1 TRANSPORT, ANDROID)
- **Stage**: VERIFY — acceptance evidence for ACCEPT
- **Version**: v1.0
- **Date**: 2026-08-17
- **Precedents**: `INTERNET_TRANSPORT_VERIFICATION.md` (reference transport
  impl), `BLE_001_VERIFICATION.md` v1.0, `WIFIAWARE-001_VERIFICATION.md` v1.0
  (AC-1..16 + verifier reproduction pattern)

## Acceptance-criteria outcome

**All 16 acceptance criteria PASS / GATED (recorded).** AC-1..13 verified in
TEST stage (`WIFIDIRECT-001_TEST.md`, iter 106); AC-14 recorded-gated
(BLK-0005 — device-only measurement, documented known_limitation);
AC-15 SECURITY_REVIEW resolved iter 107 (`WIFIDIRECT-001_SECURITY_REVIEW.md`
v1); AC-16 this document (independent verifier reproduction below).

Honest-evidence rule honored: every figure below was re-verified live in this
pass (workspace **608/0/1** [--all-features], `transport::wifi_direct` **23**,
clippy **0**, fmt clean, all RT regression tests named and present). No
unverified claim enters this record. Follows BLE-001/WIFIAWARE-001
precedents: `bluetooth`-only claims absent, device-tier figures recorded
GATED.

## AC → evidence table

| AC | Criterion (from PROJECT_GRAPH.yaml) | Evidence | Status |
|----|--------------------------------------|----------|--------|
| AC-1 | WifiDirectTransport is a Transport-trait impl registering + selecting via TransportManager like INTERNET-001/WIFIAWARE-001 (capabilities/eligibility test) | `transport::wifi_direct::tests::registration_gating` — real `WifiDirectTransport` + `SimulatedWifiDirectAdapter` registered with `TransportManager`; register → discoverable → selectable; Unavailable → never selected (manager rule 1 `state >= Available`); `transport_id` `wifi-direct-0`, display `Wi-Fi Direct`, `WIFI_DIRECT_COST` registered | PASS |
| AC-2 | Discovery = DNS-SD (Bonjour) service discovery: fixed service_name + TXT-record advertisement → candidate peer (PeerId short-id + capabilities); stop_discovery stops; candidate-only (never a trusted state change); handle round-trip | `WifiDirectAdapter::start_dns_sd` (fixed `WIFI_DIRECT_SERVICE_NAME` "com.iris.mesh.v1", asserted `service_name_is_fixed`); `WifiDirectTxtRecord::build`/`parse` (22-byte: version, cap u16 BE, kind, peer_short 16 B, freshness u16 BE); `matches()` returns DNS-SD matches; `discover_peers` parses TXT (`candidate_peer_id_pads_short_id`) → candidate `PeerInfo` (`{"wifi-direct", peer_handle}`), honoring `config.max_peers` + `config.filter`; `stop_discovery` clears window. Tests `discovery_finds_advertising_peer` + `discovery_rearm_and_stop` | PASS |
| AC-3 | Runtime capability + coexistence gate: unavailable/disabled/band-restricted → transport degrades to Unavailable/Degraded + no peers; re-available → re-attach works; STA+P2P concurrency NOT assumed | `start_advertising`/`connect` gate on `adapter.is_available()` → `TransportError::NotSupported` (no fixed-API assumption — trait carries `is_available()`); `send` gates `is_available() || state == Unavailable` → `NotSupported` (and tears links → Degraded, RT-005); `set_operating_band` AUTO fallback on band-restricted group creation. Tests `band_restricted_go_creation_falls_back` + `send_on_unavailable_tears_down_links` | PASS |
| AC-4 | GO/GC TCP E2E via SimulatedWifiDirectAdapter (mock peer): createGroup/connect → INTERNET-001-framed payload over connection → inbound → engine seam; onLost → pool evict + re-discover | Test `group_roundtrip_delivers_payload` — GO `create_group` (go_intent 14); GC `connect` (go_intent 0) via `join_group`; `send` frames `internet::encode_frame` (1 MiB cap); p2p_send → mesh → poller drains → `frame_payload_len` + u32-LE decode → `IncomingMessage` on `incoming_tx` (engine envelope-verify seam). **Sender NEVER zero + attributed to real sender** (`advertised_txt_is_attributable`); `links` keyed by PeerId, single-link-per-peer (AC-9); group-only membership, discovery re-arms | PASS |
| AC-5 | Discovery re-arm + availability refresh: 30-s app cadence (vs 120-s framework find) with stop/restart + backoff; WIFI_P2P_DISCOVERY_CHANGED_ACTION stop event → BLE-fallback path exercised | `DISCOVERY_WINDOW` 30 s app re-arm; `discovery_should_rearm` + re-arm loop; `stop_discovery` clears window (framework `WIFI_P2P_DISCOVERY_CHANGED_ACTION` → BLE fallback doc contract, AC-10). Test `discovery_rearm_and_stop` (re-arm fires, stop clears window, restart works) | PASS |
| AC-6 | Adversarial TXT-record/service-info parse: malformed/truncated/oversized records rejected without panic/allocation blowup (property + regression tests) | `wifi_direct_serv.rs` strict parse: `TooShort`, `UnsupportedVersion`, `UnknownKind`, `ReservedBits`, `ZeroShortId` (RT-001), `TooLong`; 10 serv tests incl. **exhaustive short-length sweep `too_short_rejected_no_panic` (0..22 rejected)**; `unsupported_version_rejected`; `unknown_kind_rejected`; `reserved_bits_rejected`; `oversized_padded_txt_tolerated_without_panic`; `oversized_past_cap_rejected_no_panic` (277 B → TooLong, exactly-255 ok); `zero_peer_short_rejected` — all allocation-bounded on fixed 22-byte prefix, no panics | PASS |
| AC-7 | App-layer security: every inbound IRIS message passes engine envelope-verify seam; unauthenticated DNS-SD/group events only produce candidates; identity by public-key fingerprint, never P2P MAC/device address | Discovery yields candidate-only `PeerInfo` (never a state change; DEC-WD-0007); TXT `peer_short` zero-pads to `candidate_peer_id` (candidate hint only, never trust boundary); inbound frames reach `incoming_tx` (engine gate) — no payload-triggered state change. **RT-001 (HIGH) closed**: all-zero `peer_short` rejected at parse (`ZeroShortId`), so the unknown-sender `PeerId([0u8;32])` sentinel is unreachable from a well-formed record; sim register/TXT insertion atomic (`upsert_peer`) — no empty-TXT attribution window | PASS |
| AC-8 | Lifecycle: shutdown() stops discovery + removes group + closes all connections + state Unavailable; teardown → Degraded when scope lost; attach/find failure retries with backoff (no unbounded retry loop) | `shutdown()` aborts poller + avail-watcher, calls adapter `shutdown` (remove_group + stop discovery), clears `group_id`/`group_info`/`links`, → Unavailable; `teardown_link` → Degraded on last link death while unavailable (RT-005 pattern); discovery re-arm bounded (30-s window + stop); `ensure_started` spawn-once guarded. Tests `shutdown_returns_to_unavailable` + `discovery_rearm_and_stop` | PASS |
| AC-9 | Reliability: per-peer single group/connection reused; close promptly; GO-side connection table bounded (N-client admission, no unbounded churn); persistent-GO teardown/re-establish handled | Single link per peer (`links` keyed by PeerId, `single_link_per_peer_and_bounded_table`: 8 concurrent connects → exactly 1 link; `MAX_GO_CLIENTS=8` GO-side admission); reuse path re-promotes `Connected` (RT-004, `connect_reuse_after_churn_recovers`); persistent-GO teardown on shutdown (PERSISTENT_GO cap); outbox bounded `MAX_OUTBOX_FRAMES=128` with per-destination eviction (RT-002, `outbox_eviction_keeps_other_destinations`) | PASS |
| AC-10 | Background + platform limitations DOCUMENTED in WIFI_DIRECT.md: foreground/FGS connectedDevice (API 34+) + PowerManager wakelock required; discovery unreliable in background; BLE = background discovery layer; band API 29 correction; 120-s framework find / 30-s app re-arm; iOS unavailable (MCSession Apple-only) | `WIFI_DIRECT.md` + `WIFI_DIRECT_TRANSPORT_DESIGN.md` §"Background contract": FGS connectedDevice (API 34+) + `PARTIAL_WAKE_LOCK`; BLE stays the always-on discovery layer, WD only for active transfers; band `setGroupOperatingBand` = API 29 (RES-0021 Q5 correction); framework find window 120 s / app re-arm 30 s (Q4/G-WD-7); iOS private-API unavailable row. Caps: `supports_background_android: false` / `supports_background_ios: false` | PASS |
| AC-11 | FFI contract type-checked: WifiDirectAdapter trait implemented + clippy-clean; SimulatedWifiDirectAdapter conformance | `WifiDirectAdapter` trait 20 ops (start/start_dns_sd/stop_dns_sd/start_discovery/stop_discovery/matches/create_group/join_group/add_client/remove_group/group_info/go_addr/set_operating_band/p2p_send/incoming/shutdown/is_available/availability_stream) implemented by `SimulatedWifiDirectAdapter` + `SimP2pCoordinator`; `cargo clippy --workspace --all-features --tests` **0 warnings**. RT-010 records ANDROID-001 FFI idempotence contract (adapter start/start_dns_sd idempotence) | PASS |
| AC-12 | Doc reconciliation: WIFI_DIRECT.md + TRANSPORT_ABSTRACTION.md + WIFIDIRECT-001 node agree with code incl. RES-0021 corrections (band API 29, find window 120 s, client ceiling vendor/HAL, WPA3-SAE R2 capability-gated) | iter 104: `WIFI_DIRECT.md` corrected (band API 29, framework find window 120 s / 30 s app re-arm, client ceiling vendor/HAL + N-client note, WPA3-SAE R2 capability-gated, WPS-PIN prohibited, OS patch floor). TRANSPORT_ABSTRACTION matrix row (Android background no-wakelock / iOS No) matches caps. Node `known_limitations` 12 rows + `acceptance_criteria` AC-1..16 consistent with code + design §7 | PASS |
| AC-13 | Workspace green + clippy 0; baseline 552/0/1 held | **`cargo test --workspace --all-features` = 608 passed / 0 failed / 1 ignored** (+56 vs baseline 552; +7 RT regressions on iter-106 601); `cargo clippy --workspace --all-features --tests` = **0 warnings**; `cargo fmt --check` clean. | PASS |
| AC-14 (GATED/BLK-0005) | Physical-device group formation/discovery/throughput/battery BENCH = known_limitation | Not executed — recorded gated (PROJECT_GRAPH known_limitations; battery = `WIFI_DIRECT_COST` documented design estimates per WIFI_DIRECT.md; OS patch floor recorded: Android SPL >= 2021-02, wpa_supplicant >= 2.12) | ⛔ GATED (known_limitation) |
| AC-15 | SECURITY_REVIEW: redteam adversarial findings dispositioned | `WIFIDIRECT-001_SECURITY_REVIEW.md` v1 (iter 107): **WIFIDIRECT-001_RT-001..013 dispositioned — 1 HIGH + 5 MEDIUM + 1 LOW FIXED in-pass + 7 regression tests; 5 RECORDED** (RT-006 lag telemetry, RT-010 → ANDROID-001 FFI idempotence, RT-011 → TRANSPORT-001 manager arbitration, RT-013 shared 1 MiB constant, poll cadence; RT-009 doc-FIXED, RT-012 folded into RT-003). Positive controls held (strict parse, framed decode bounds, candidate-only trust, gated write path, bounded queues, lifecycle latch, group integrity, log hygiene) | PASS |
| AC-16 | VERIFY: independent verification doc evidence table; verifier APPROVE | This document §"Live verification" + §"Independent verifier (subagent) reproduction" | PASS |

## Live verification (this pass)

| Check | Command | Result |
|-------|---------|--------|
| Workspace | `cargo test --workspace --all-features` | **608 passed / 0 failed / 1 ignored** (per-suite sums: 553 iris-core lib + integration/sim/desktop suites) |
| Wi-Fi Direct transport | `cargo test -p iris-core --lib --all-features transport::wifi_direct` | **23 passed / 0 failed** (filter prefix `transport::wifi_direct` matches both modules: **13 transport + 10 serv**) |
| Clippy | `cargo clippy --workspace --all-features --tests` | **0 warnings** |
| Format | `cargo fmt --check` | clean |
| RT regressions (7) | `cargo test -p iris-core --lib --all-features transport::wifi_direct` | Present + passing: `advertised_txt_is_attributable`, `outbox_eviction_keeps_other_destinations`, `join_non_go_peer_is_rejected`, `connect_reuse_after_churn_recovers`, `send_on_unavailable_tears_down_links`, `empty_payload_send_rejected` + serv `zero_peer_short_rejected` |

## Independent verifier (subagent) reproduction

Independent `verifier` subagent dispatched against the workspace (iter 108) with
instructions to reproduce: workspace test totals (608/0/1 --all-features),
clippy (0), `transport::wifi_direct` (23), presence + coverage of the
SECURITY_REVIEW regression set (7 RT tests), the RT-001..013 dispositions,
and absence of any AC row citing a nonexistent test.

*(Verifier verdict appended on completion — expected APPROVE; any notes
reconciled before ACCEPT.)*

## Independent verifier verdict — APPROVE_WITH_NOTES (reconciled)

Verifier (subagent `ses_ff2cab279ffebsAWkoU2shmMk0`, iter 108) independently
reproduced: workspace **608 passed / 0 failed / 1 ignored** (per-suite sum
matches exactly: iris-core lib 553 + crypto_e2e 3, ml 8, obs 4, sim 8+1
ignored, desktop 5, commands 3, engine_roundtrip 3, storage 7, m3 1, pg_store
13); filter `transport::wifi_direct` = **23 passed** (13 transport + 10 serv
under shared prefix); clippy `--workspace --all-features --tests` = **0
warnings** (exit 0); fmt clean. All 27 AC-cited tests exist with correct
attributes and pass; **no AC row cites a nonexistent test/binding**. All 7 RT
regression tests verified as real code:
- RT-001 (HIGH): `ZeroShortId` variant + parse gate (`wifi_direct_serv.rs:94,153-155`);
  atomic `alloc_tag`+`upsert_peer` single-lock (`wifi_direct.rs:278-285`), sole
  call site `register()` (`wifi_direct.rs:479,485`); regressions
  `advertised_txt_is_attributable` (`wifi_direct.rs:1644`) +
  `zero_peer_short_rejected` (`serv:316`).
- RT-002: `proxy_send` same-destination eviction (`wifi_direct.rs:384-398`);
  `outbox_eviction_keeps_other_destinations` (`wifi_direct.rs:1660`).
- RT-003/RT-012: `join_group` requires real GO (`wifi_direct.rs:606-609`) +
  already-in-group guard (`610-632`); `join_non_go_peer_is_rejected`
  (`wifi_direct.rs:1685`).
- RT-004: reuse branch re-promotes `Connected` (`wifi_direct.rs:1171-1181`);
  `connect_reuse_after_churn_recovers` (`wifi_direct.rs:1728`).
- RT-005: `send` tears links → Degraded (`wifi_direct.rs:1244-1260`);
  `send_on_unavailable_tears_down_links` (`wifi_direct.rs:1776`).
- RT-007: `send` rejects empty payload (`wifi_direct.rs:1237-1242`);
  `empty_payload_send_rejected` (`wifi_direct.rs:1819`).
- RT-009: capacity comment 32 MiB corrected (`wifi_direct.rs:77-81`).
RECORDED set consistent with code: RT-006 `broadcast_stream` Lagged skip
(`transport/mod.rs:296-303`), RT-010 `ensure_started` unlatched
(`wifi_direct.rs:906-931`), RT-011 `RadioConflictGroup` test-side only
(`mod.rs:283`), RT-013 three 1-MiB constants (`wifi_direct.rs:70`,
`wifiaware.rs:62`, `internet.rs:41`), poll cadence 10/500 ms
(`wifi_direct.rs:84,87,947-952`).

**Notes (all reconciled in-pass, non-blocking)**:
1. RT-012 confirmed FIXED (folded into RT-003) — record is correct.
2. RECORDED count corrected in record + verification AC-15 row from "6" to
   **5** (RT-006/010/011/013/poll; RT-009 is doc-FIXED, counted in the 7) —
   header + section label + AC-15 row updated in this pass.
3. Clippy cache-hit notice — deterministic result of exact command (exit 0,
   zero warning lines), validated.
4. 3 anonymous empty test suites inflate "15 suites" phrasing harmlessly —
   totals unaffected.

Verdict: **APPROVE** (all evidence real, all counts reproduced, only labeling
nits reconciled).

## Known limitations (recorded, not blockers)

- Physical-device group formation/discovery/throughput/battery measurement
  pending hardware (BLK-0005, RESOURCE-gated; battery = `WIFI_DIRECT_COST`
  documented design estimates) — AC-14.
- iOS unavailable (private API, WIFI_DIRECT.md known gap); Apple MCSession is
  Apple-to-Apple only; iOS<->Android high-bandwidth falls back to BLE.
- Android background: WD discovery + group formation unreliable in background;
  requires foreground/FGS connectedDevice (API 34+) + PARTIAL_WAKE_LOCK; BLE
  stays the always-on discovery layer.
- WPA3-SAE capability-gated later phase (API 36 R2) — v1 floor WPA2-Personal;
  WPS-PIN prohibited; passphrase pushed over authenticated BLE (DEC-WD-0002/3).
- OS patch floor unpatched-by-app (Android SPL >= 2021-02 CVE-2021-0326,
  wpa_supplicant >= 2.12, kernel 2024-26 Wi-Fi fixes).
- Same-band interference: infra Wi-Fi + WD share one radio; band hint
  `setGroupOperatingBand` API 29; group formation latency 2-15 s excludes
  P0/P1.
- RT-006 (recorded): dropped_inbound telemetry undercounts see broadcast
  Lagged skips (cross-transport shared helper; recorded, not fixed).
- RT-010 (recorded → ANDROID-001 FFI): ensure_started() not latched — adapter
  start/start_dns_sd must be idempotent (same-session reuse).
- RT-011 (recorded → TRANSPORT-001): TransportManager RadioConflictGroup
  arbitration not yet implemented; transports rely on adapter availability
  gating.
- RT-013 (recorded): 1 MiB frame cap re-declared per transport — shared-
  constant refactor follow-up (FUTURE_TRANSPORTS), not a bug.
- Poll cadence: 10 ms linked / 500 ms idle = 100 Hz adapter poll (bounded
  per tick MAX_FRAMES_PER_TICK=8) — battery-sensitive, BENCH gated (AC-14).

## Sign-off

Recorded 2026-08-17 (iter 108). WIFIDIRECT-001 ready for ACCEPT per AC-1..16
evidence (AC-14 gated by BLK-0005). Transition target: **LORA-001**.