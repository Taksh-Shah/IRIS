# WIFIDIRECT-001 Design — Wi-Fi Direct (P2P) Transport (Android) v1

**Document ID**: IRIS-WIFIDIRECT-001-DESIGN-001
**Version**: 1.0
**Node**: WIFIDIRECT-001 (P1 TRANSPORT, platform ANDROID, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-17
**Research basis**: RES-0021 (2024–2026 Wi-Fi Direct/P2P Android + Linux SOTA:
API lifecycle/deprecation, group security WPA2 vs WPA3-SAE, client limits,
discovery durability, persistent GO + band, OOB provisioning/WPS, wpa_supplicant,
CVEs/OS patch floor; verdict PROCEED); `docs/transports/WIFI_DIRECT.md`
(326-line platform design); TRANSPORT_ABSTRACTION.md (capability matrix);
BLE-001 DESIGN (the transport-design pattern, iter 85) + WIFIAWARE-001 DESIGN
(the FFI-trait + SimMesh pattern, iter 93); INTERNET-001 (reference impl — TCP
framing / 1 MiB cap / pool / backoff, 27/27 tests); ACCEPTANCE_POLICY.yaml
TRANSPORT-type additions; DEC-0009 (gates lifted); BLK-0005 (device-test resource
gating).
**Absorbs**: RES-0021 Q1–Q8 + Gaps G-WD-1..8 (design-positioned) + DESIGN
decisions D-1..D-7 resolved as DEC-WD-0001..0008.
**Reconciles**: WIFI_DIRECT.md vs 2026 reality — Q5 (`setGroupOperatingBand` = **API 29**,
doc said "API 30+"), Q4 (framework P2P find window = **120 s**, doc "30-s" = app
re-arm cadence), Q3 ("1 GO + 8 clients" = vendor/HAL ceiling, not spec
guarantee), Q2 (WPA3-SAE now available via Wi-Fi Direct R2, API 36).

---

## 1. Scope

Implement the Wi-Fi Direct transport on the IRIS `Transport` trait following the
**INTERNET-001 reference pattern** (as WIFIAWARE-001 did): a **platform-neutral
adapter trait** (`WifiDirectAdapter`) driven by a Rust-core transport, with the
real Android implementation injected at runtime via FFI (uniffi/JNI, Kotlin) and
a `SimulatedWifiDirectAdapter` covering the in-memory/CI path (BLK-0005 device
tests stay gated). The work is **pure-Rust in `crates/iris-core`**; the Android
Kotlin/JNI adapter lives under ANDROID-001 and is scaffolded here as a documented
FFI contract only (physical-device coverage = recorded `known_limitation`).

**v1 shape (RES-0021 Q1/Q4/Q5/Q7, verdict PROCEED)**: Wi-Fi Direct is used the
way WIFI_DIRECT.md already specifies — **data plane** on the **BLE control plane**.
BLE already advertises/disceivers in background (BLE-001); when a peer needs a
large transfer (payload > 10 KB threshold from WIFI_DIRECT.md), IRIS starts a
Wi-Fi Direct group and transfers over a **TCP socket on the Group Owner** reusing
**INTERNET-001 framing** (1 MiB cap, pool + backoff). Discovery = **DNS-SD
(Bonjour) service discovery** (`discoverServices`/`addLocalService`) on a
BLE-triggered re-arm window; data path = `createGroup` (persistent GO) /
`connect` (GC); group security = **WPA2-Personal (AES-CCMP)** floor.

**Dual-platform adapter contract**: Android `WifiP2pManager` + Linux
wpa_supplicant. Both surface through the same trait (Q7); the Rust transport owns
the IP/DHCP glue contract (GO static address + client DHCP / IPv6-link-local).

**Explicitly not v1 (RES-0021 Q2/Q6, DEC-WD records)**: WPA3-Personal (SAE),
Wi-Fi Direct R2 pairing bootstrapping (incl. OUT_OF_BAND), DPP, 6 GHz as
requirements. Group security stays WPA2 (PSK/AES-CCMP) — app-layer envelope =
trust anchor; SAE = capability-gated later phase (`isWiFiDirectR2Supported`/
`isPccModeSupported`); **WPS-PIN prohibited** (CVE-attested attack surface);
5 GHz preferred with AUTO fallback; 6 GHz experimental only.

---

## 2. Platform contract and wire constraints (RES-0021 → ACs)

### 2.1 Runtime capability + coexistence gate (Q1/Q3 — AC-3/AC-5)
- Hardware/runtime: Wi-Fi Direct is universally present on certified Android
  devices (unlike Wi-Fi Aware), but **STA+P2P concurrent operation is
  HAL-combination-dependent** (RES-0021 Q3/G-WD-6). IRIS never assumes
  infrastructure-STA + P2P simultaneously: capability-probe via
  `WifiManager.getUsableChannels` / `WifiAvailableChannel`
  (`OP_MODE_WIFI_DIRECT_GO`/`OP_MODE_WIFI_DIRECT_CLI`, API 34) and degrade to
  P2P-only mode when constrained.
- Coexistence channel-avoidance (Android 12+): `COEX_RESTRICTION_WIFI_DIRECT` +
  `ISupplicantP2pIface::setDisallowedFrequencies()` may force P2P off 5 GHz on
  some carriers (`restrict_5g_softap_wifi_direct_for_laa`). IRIS reads allowed
  channels at group creation and prefers an infra-free band; same-band
  interference with infra Wi-Fi (WIFI_DIRECT.md known issue) is mitigated by the
  band hint (2.2).
- State-change handling: `WIFI_P2P_STATE_CHANGED_ACTION`,
  `WIFI_P2P_DISCOVERY_CHANGED_ACTION`, `WIFI_P2P_PEERS_CHANGED_ACTION`,
  `WIFI_P2P_CONNECTION_CHANGED_ACTION` drive transport state
  (Unavailable → Available → Connected → Degraded); peers connection lost →
  `WIFI_P2P_CONNECTION_CHANGED_ACTION` + `onLost` → pool evict + re-discover.
- Background: Wi-Fi Direct discovery/group events are **unreliable in
  background** (Q4) — IRIS only activates Wi-Fi Direct with the app in foreground
  or under an FGS + PowerManager partial wakelock (WIFI_DIRECT.md contract,
  AC-10). BLE remains the always-on discovery layer.

### 2.2 Group topology + data path (Q3/Q5 — AC-2/AC-4/AC-9)
- GO/client: exactly one GO per group (`WifiP2pGroup`), N clients. **Client
  ceiling is vendor/HAL, not spec** (G-WD-1) — IRIS architects an N-client
  admission model: a bounded GO-side connection table (per-client TCP endpoint +
  slot), default cap from design estimates, admitted client queue, no
  hard-coded-8 assumption.
- **GO IP**: Android GO address `192.168.49.1` is practice-stable; DESIGN must
  verify in AOSP `WifiP2pServiceImpl`/DHCP wiring at IMPLEMENT (G-WD-2). The
  Rust transport treats the GO address as an **adapter-provided endpoint**, never
  assumes the literal `192.168.49.1`; the adapter supplies `go_addr` from
  `requestGroupInfo`/`requestConnectionInfo` (Android) or the static-address glue
  (Linux).
- Client IP: IPv4 DHCP (default) with the newer **IPv6-link-local provisioning
  mode** (`GROUP_CLIENT_IP_PROVISIONING_MODE_IPV6_LINK_LOCAL`, Q5) as an
  alternative; TCP endpoint registry keyed by connection.
- Data plane: **plain TCP socket over the GO** with **INTERNET-001 framing**
  (4-byte length prefix + message, 1 MiB cap, pool + backoff) — the existing
  internet.rs framing/pool/backoff machinery transfers unchanged. WPA2 encrypts
  the L2 link; app-layer envelope crypto remains the trust anchor (CRYPTO-001).

### 2.3 Discovery (Q4 — AC-2/AC-3)
- **DNS-SD (Bonjour) pre-association service discovery** over device-name
  filtering: `addLocalService` (TXTRECORD = IRIS service info) + `discoverServices`
  + `setDnsSdResponseListeners` → candidate peers. The P2P service name is fixed
  (`com.iris.mesh.v1`); candidate PeerId comes from the TXT-record advertisement,
  NOT the P2P device MAC (Q4, identity-by-fingerprint doctrine).
- **Discovery window**: framework P2P find is **120 s** (`DISCOVER_TIMEOUT_S`);
  WIFI_DIRECT.md "30-s" is the **app-level re-arm cadence** (G-WD-7). IRIS uses a
  30-s app window with explicit stop, or a single 120-s find with stop/restart —
  decided in DEC-WD-0004. `WIFI_P2P_DISCOVERY_CHANGED_ACTION` (stop event)
  drives the return to BLE control-plane waiting.
- Best-effort on OEM builds (G-WD-5): DNS-SD discovery is not guaranteed
  reliable → BLE control plane remains the durable trigger (already the IRIS
  pattern); Wi-Fi Direct find is an accelerant for the data-plane hop, never the
  sole discovery mechanism.

### 2.4 Persistent GO + band selection (Q5 — AC-9/AC-10)
- **Persistent GO**: `createGroup(WifiP2pConfig)` forms an autonomous GO that
  persists until `removeGroup()`/teardown (Q5); persistent groups are stored and
  can be re-invoked (`p2p_invite` equivalent on Android via
  `WifiP2pConfig.groupOwnerIntent` + reapplied group). IRIS maintains a persistent
  GO for frequently-contacted peers (WIFI_DIRECT.md optimization) — group
  presence is treated as **state to re-establish**, not assumed persistent across
  reboot/Wi-Fi toggle (G-WD-8).
- **GO intent bias** (WIFI_DIRECT.md): fixed infra node → 14, battery-powered →
  7, low-battery → 3; framework default is 6. Predictable GO election hierarchy.
- **Band**: `setGroupOperatingBand` (**API 29**; AUTO/2GHz/5GHz/6GHz) —
  **5 GHz preferred, AUTO fallback**; band-constrained GO creation FAILS if
  unsupported (fall back to AUTO / 2.4 GHz / degrade to BLE). `setGroupOperatingFrequency`
  reserved for future explicit channel pick. MAC randomization is
  platform-inherited and tied to persistent-group presence (Q5) — peer identity
  never depends on the P2P MAC.

### 2.5 Background contract (Q4 — AC-10)
- Wi-Fi Direct group formation + transfer require **foreground or FGS of type
  `connectedDevice` (API 34+)** + PowerManager PARTIAL_WAKE_LOCK held from
  discovery through transfer teardown (WIFI_DIRECT.md contract). Declarations:
  `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_CONNECTED_DEVICE`, `ACCESS_WIFI_STATE`/
  `CHANGE_WIFI_STATE`/`ACCESS_FINE_LOCATION` (API < 31) or `NEARBY_WIFI_DEVICES`
  (API 33+, `neverForLocation` default), runtime `POST_NOTIFICATIONS` (API 33+).
- BLE handles background discovery; Wi-Fi Direct activates only for active
  transfers (screen on / FGS).

### 2.6 Regulatory / spectrum (Q5/G-WD-3)
- 2.4 GHz + 5 GHz P2P: license-exempt ISM/U-NII in India — OK per WIFI_DIRECT.md;
  design floor. 6 GHz experimental (hardware/regulatory-gated, G-WD-3).

---

## 3. Security posture (RES-0021 Q2/Q6/Q8 → AC-6)

1. **App-layer envelope crypto is the trust anchor** (CRYPTO-001 sign+encrypt,
   verify-before-forward, SEC-001 replay/dedup/rate gates). WPA2 protects the L2
   link; **never depend on the P2P group security layer for IRIS security**.
2. **WPA2-Personal (PSK/AES-CCMP) floor** (Q2). **WPS-PIN prohibited** — WPS is
   deprecated client-mode (API 28) and WPS-PIN historically enables brute-force/
   pin-drop attacks (adjacent P2P unauthenticated RCEs: CVE-2021-0326 requires
   only an active P2P search). Group passphrase is **pushed over the authenticated
   BLE control plane** (R2-OOB-style) by the peer that initiates; **PBC is legacy
   fallback only** (dev-doc sample uses PBC). WPA3-SAE = capability-gated later
   phase (`isWiFiDirectR2Supported`/`isPccModeSupported`; `SECURITY_TYPE_WPA3_SAE/
   WPA3_COMPATIBILITY` API 36), DEC-WD-0002 — never a v1 default (G-WD-4).
3. **Defensive advertise/parse**: TXT-record/service-info parsing is adversarial
   with property/regression tests (Q8 parser-CVE pattern) — never panic/alloc on
   malformed records.
4. **No unauthenticated-trigger actions**: discovery + group formation produce
   *candidate/connection* objects only; every IRIS payload flows through envelope
   verify + engine gate (mirrors BLE-001 R5.3 / WIFIAWARE-001 AC-7).
5. **OS patch floor AC** (Q8/G-WD — AC-14): Android security patch level
   **≥ 2021-02** (CVE-2021-0326 P2P RCE), Linux **wpa_supplicant ≥ 2.12** (w1.fi
   2026-1 unauthenticated mgmt-frame memory corruption, 2026-3 SAE NULL-deref),
   kernel carrying 2024–26 Wi-Fi driver fixes (CVE-2024-26895/27053/47712/47724/
   56539/46755; CVE-2025-40321; CVE-2026-31780/46069). Recorded as a gated
   deployment floor; Wi-Fi stack CVEs are unpatched-by-app.
6. **Capacity guards**: GO-side connection table bounded (per-client slots +
   per-client in-flight framing); INTERNET-001 pool/backoff applied; no unbounded
   synchronization (SG-WD pattern from NEW-WA-RT-105 per-tick budget).

---

## 4. Module layout (pure Rust, `crates/iris-core/src/transport/`)

| Piece | Status | What changes |
|---|---|---|
| `WifiDirectAdapter` trait | new | platform contract (Android `WifiP2pManager` + Linux wpa_supplicant), ops: `is_available()`, `start_discovery(config,cb)` (DNS-SD re-arm), `stop_discovery()`, `create_group(config,cb)` (persistent GO), `join/connect(peer,cb)`, `list_group_info()`, `groups()/clients()`, `remove_group()`, `send(peer,data)`, `incoming()`, `go_addr()`, `set_operating_band(band)`, `shutdown()` — maps to `WifiP2pManager` + wpa_cli |
| `WifiDirectTransport` | new | `Transport` impl: capability gate + coexistence (AC-3/AC-5), DNS-SD discovery → candidate peers (AC-2), GO/GC connection table + INTERNET-001 framing reuse over TCP (AC-4/AC-7), lifecycle + persistent GO teardown (AC-8/AC-9), band + wakelock contract (AC-10) |
| `wifidirect_serv`/beacon | new (device-context: merged into wifi_direct.rs) | IRIS DNS-SD TXT-record build/parse (`service_name` + cap bitmask + freshness + PeerId short-id) + defensive decode + adversarial tests (AC-6) |
| `SimulatedWifiDirectAdapter` | new | deterministic in-memory P2P rendezvous (service discovery match, GO/GC group formation, TCP framing E2E, inbound data, availability churn, band-restriction failure paths) for CI — conformance type-checks the FFI contract (AC-11) |
| Android Kotlin/JNI + Linux wpa_supplicant adapters | not yet | ANDROID-001 + DESKTOP-001/LINUX scope; `WifiDirectAdapter` trait IS the FFI contract (BLK-0005 known_limitation) |
| tests | new (`transport::wifi_direct` suite) | adapter conformance, TXT-record adversarial, discovery candidate flow, TCP E2E, availability churn, lifecycle, band-failure degradation |

**Capability matrix entry (TRANSPORT-001)**: `max_message_size: 1 MiB`
(INTERNET-001 framing cap), `supports_broadcast: true` (DNS-SD service discovery),
`supports_unicast: true`, `multicast: false`, `range 50..200 m` (typical 100, LOS),
`typical_throughput_bps: 10_000_000` (10–80 Mbps practical, conservative floor),
`setup_latency_ms: 3000..15000`, `cost_class: Free`, `supports_background_android:
false` (foreground/FGS + wakelock only; BLE covers background),
`supports_background_ios: false` (iOS has no public Wi-Fi Direct API; MCSession is
Apple-to-Apple only — known gap recorded).

---

## 5. Android FFI adapter contract (ANDROID-001 seam)

The `WifiDirectAdapter` trait IS the FFI contract. The Android crate
(Kotlin/JNI) implements the §4 lifecycle around `WifiP2pManager.initialize/
addLocalService/discoverServices/createGroup/connect/removeGroup`,
`DnsSdTxtRecord`/`DnsSdServiceResponseListener`, `requestGroupInfo/
requestConnectionInfo`, the FGS + wakelock (§2.5), and the Wi-Fi P2P broadcast
receivers (§2.1). Linux nodes expose the same trait via a wpa_supplicant CLI
glue (`p2p_group_add`, `p2p_connect`, DNS-SD via `p2p_service_add bonjour` +
`p2p_service_flush/del`, `p2p_invite`, `p2p_get_passphrase`, group iface
`p2p-wlan0-N`) + **IRIS-owned IP/DHCP glue** (GO static address + dnsmasq/udhcpd;
GC DHCP). Type-checked in CI by `SimulatedWifiDirectAdapter` conformance (AC-11);
physical coverage gated (BLK-0005).

---

## 6. Acceptance criteria (WIFIDIRECT-001 AC-1..16 — resolves the C2 gap)

Defined in `engineering/PROJECT_GRAPH.yaml` WIFIDIRECT-001 node (pattern
WIFIAWARE-001/BLE-001 AC-1..16):

- **AC-1**: `WifiDirectTransport` is a `Transport`-trait impl registering +
  selecting via `TransportManager` like INTERNET-001/WIFIAWARE-001
  (capabilities/eligibility test).
- **AC-2**: Discovery = DNS-SD (Bonjour) service discovery: fixed
  `service_name` + TXT-record advertisement → candidate peer (PeerId short-id +
  capabilities); stop_discovery stops; candidate-only (never a trusted state
  change); handle round-trip.
- **AC-3**: Runtime capability + coexistence gate: unavailable/disabled /
  band-restricted → transport degrades to Unavailable/Degraded + no peers
  reported; re-available → re-attach works; STA+P2P concurrency NOT assumed
  (capability-probe aware).
- **AC-4**: GO/GC TCP E2E via `SimulatedWifiDirectAdapter` (mock peer): create
  group / connect → INTERNET-001-framed payload over connection → inbound →
  engine seam; onLost ⇒ pool evict + re-discover.
- **AC-5**: Discovery re-arm + availability refresh: 30-s app cadence (vs 120-s
  framework find) with stop/restart + backoff; `WIFI_P2P_DISCOVERY_CHANGED_ACTION`
  stop event → BLE-fallback path exercised.
- **AC-6**: Adversarial TXT-record/service-info parse: malformed/truncated/
  oversized records rejected without panic/allocation blowup (property +
  regression tests).
- **AC-7**: App-layer security: every inbound IRIS message passes engine
  envelope-verify seam; unauthenticated DNS-SD/group events only produce
  candidates; identity by public-key fingerprint, never P2P MAC/device address
  (RES-0021 Q4; SEC-001/IDENT-001).
- **AC-8**: Lifecycle: shutdown() stops discovery + removes group + closes all
  connections + state Unavailable; teardown → Degraded when scope lost;
  attach/find failure retries with backoff (no unbounded retry loop).
- **AC-9**: Reliability: per-peer single group/connection reused; close promptly;
  GO-side connection table bounded (N-client admission, no unbounded churn);
  persistent-GO teardown/re-establish handled.
- **AC-10**: Background + platform limitations DOCUMENTED in WIFI_DIRECT.md:
  foreground/FGS connectedDevice (API 34+) + PowerManager wakelock required;
  discovery unreliable in background; BLE = background discovery layer; band
  API 29 correction; 120-s framework find / 30-s app re-arm; iOS unavailable
  (MCSession Apple-only).
- **AC-11**: FFI contract type-checked: `WifiDirectAdapter` trait implemented +
  clippy-clean; `SimulatedWifiDirectAdapter` conformance.
- **AC-12**: Doc reconciliation: WIFI_DIRECT.md + TRANSPORT_ABSTRACTION.md +
  WIFIDIRECT-001 node agree with code incl. RES-0021 corrections (band API 29,
  find window 120 s, client ceiling vendor/HAL, WPA3-SAE R2 capability-gated).
- **AC-13**: Workspace green + clippy 0; baseline 552/0/1 held.
- **AC-14 (GATED/BLK-0005, recorded)**: physical-device group formation/
  discovery/throughput/battery BENCH = known_limitation; battery figures =
  documented design estimates; **OS patch floor recorded** (Android SPL ≥
  2021-02, wpa_supplicant ≥ 2.12).
- **AC-15**: SECURITY_REVIEW stage: redteam adversarial findings
  dispositioned/fixed/recorded.
- **AC-16**: VERIFY: independent verification doc evidence table; verifier
  APPROVE (pattern EMERG-001/SEC-001/BLE-001/WIFIAWARE-001).

---

## 7. Reference-impl section (design → code trace, appended at IMPLEMENT)

_(Filled at iter 105 IMPLEMENT — maps each AC to `wifi_direct.rs` +
`wifidirect` TXT-record code + tests + manager.rs registration, pattern
WIFIAWARE-001 DESIGN §9.)_

Filled at iter 105 (IMPLEMENT). Pure-Rust in `crates/iris-core/src/transport/`,
following the WIFIAWARE-001 pattern (adapter trait + Sim coordinator + transport
impl + discovery beacon).

### 7.1 Code → AC mapping

| AC | Evidence |
|----|----------|
| **AC-1** registration/selection via `TransportManager` | `WifiDirectTransport` is a `Transport` impl (`wifi_direct.rs`); `registration_gating` test registers with `TransportManager` and asserts an Unavailable transport is never selected. `transport/mod.rs` registers `wifi_direct` + `wifi_direct_serv`. |
| **AC-2** DNS-SD discovery → candidates | `WifiDirectAdapter::start_dns_sd` (advertisement) + `start_discovery`/`matches`; transport `discover_peers` parses the TXT record → `candidate_peer_id` (short-id, never MAC). `WIFI_DIRECT_SERVICE_NAME = "com.iris.mesh.v1"`; `WifiDirectTxtRecord` build/parse in `wifi_direct_serv.rs`. Tests: `discovery_finds_advertising_peer` + serv round-trip. |
| **AC-3** capability + coexistence gate | `is_available` + `start_dns_sd` gate + band-restricted GO failure → AUTO fallback in `connect` (DEC-WD-0005). Test: `band_restricted_go_creation_falls_back` (falls back to AUTO → group forms; unavailable → no peers / `NotSupported`). |
| **AC-4** GO/GC TCP E2E | Group formation (GO: `create_group` + `add_client`; GC: `join_group`) then `p2p_send` routes INTERNET-001-framed payload through the group to the peer's incoming stream (engine seam). Test: `group_roundtrip_delivers_payload` — frame attributed to sender's TXT candidate PeerId, never zero. |
| **AC-5** discovery re-arm + backoff | `DISCOVERY_WINDOW` (30-s app cadence vs 120-s framework find, G-WD-7) via `discovery_should_rearm`; `stop_discovery` clears the window (`WIFI_P2P_DISCOVERY_CHANGED_ACTION` stop → BLE fallback). Test: `discovery_rearm_and_stop`. |
| **AC-6** adversarial TXT-record parse | `WifiDirectTxtRecord::parse`: rejects ToShort/TooLong/UnsupportedVersion/UnknownKind/ReservedBits without panic; 9 adversarial tests (exhaustive 0..22 len sweep, padding tolerated ≤ 255 B, oversize rejected). |
| **AC-7** envelope-verify seam | Every inbound frame flows to `incoming_tx` (engine envelope-verify seam); DNS-SD/group events produce candidates only; identity = TXT-record short-id → `candidate_peer_id` zero-pad, never the P2P device MAC (DEC-WD-0007). |
| **AC-8** lifecycle | `shutdown()` aborts poller + avail watcher, `remove_group` + adapter `shutdown`, clears links/discovery, latches `shutdown_flag`, state → Unavailable. Test: `shutdown_returns_to_unavailable`. |
| **AC-9** reliability | Per-peer single link (`links.iter().any(peer_id)` reuse check under `connect_gate`); bounded GO table `MAX_GO_CLIENTS = 8` (software admission cap, DEC-WD-0006 — never a hard-coded-8 platform assumption); persistent GO via `GroupConfig.persistent`. Test: `single_link_per_peer_and_bounded_table` (8 concurrent connects → 1 link). |
| **AC-10** background/platform docs | WIFI_DIRECT.md §Foreground + FGS `connectedDevice` (API 34+) + wakelock; BLE = background discovery layer; band API 29; 120-s framework / 30-s app re-arm; iOS unavailable (MCSession Apple-only). Codified in caps: `supports_background_android: false`, `supports_background_ios: false`. |
| **AC-11** FFI conformance type-checked | `WifiDirectAdapter` trait (20 ops: start/start_dns_sd/stop_dns_sd/start_discovery/stop_discovery/matches/create_group/join_group/add_client/remove_group/group_info/go_addr/set_operating_band/p2p_send/incoming/shutdown/is_available/availability_stream) implemented by `SimulatedWifiDirectAdapter`; clippy-clean. |
| **AC-12** doc reconciliation | WIFI_DIRECT.md corrections landed at DESIGN (iter 104): band API 29, find window 120 s, client ceiling vendor/HAL + N-client, WPA3-SAE R2 capability-gated, OS patch floor. Code matches (band = `OperatingBand` enum, AUTO fallback). |
| **AC-13** workspace green + clippy 0 | `cargo test --workspace --all-features`: 601 passed / 0 failed / 1 ignored (was 552/0/1 baseline; +16 WiFi-Direct tests incl. 9 serv + 7 transport); `cargo clippy --workspace --all-features --tests`: 0. |
| **AC-14** GATED (BLK-0005) | Physical-device group formation/discovery/throughput/battery BENCH = recorded `known_limitation`; `WIFI_DIRECT_COST` documented design estimates (scan 80 mA / advertise 50 / connected 40 / tx 0.01 / rx 0.008 — from WIFI_DIRECT.md battery figures); OS patch floor recorded (Android SPL ≥ 2021-02, wpa_supplicant ≥ 2.12). |
| **AC-15** SECURITY_REVIEW | TEST-stage record (iter 106) → SECURITY_REVIEW stage dispositions redteam findings. |
| **AC-16** VERIFY | Independent verification doc evidence table (iter 108), pattern EMERG-001/SEC-001/BLE-001/WIFIAWARE-001. |

### 7.2 Test suite (`transport::wifi_direct`, 16 tests)

- `wifi_direct_serv.rs` (9): build/parse round-trip, fixed service name, exhaustive short-length 0..22, unsupported version, unknown kind, reserved bits, padded-≤255 tolerated, oversize-past-cap rejected, candidate PeerId zero-pad.
- `wifi_direct.rs` (7): GO/GC E2E round-trip + sender attribution, DNS-SD discovery, band-restricted fallback + unavailable gate, discovery re-arm/stop, shutdown lifecycle, bounded single-link table, `TransportManager` registration gating.

### 7.3 Interop note (SDK/UAPI seam)

Android `WifiP2pManager` (`addLocalService`/`discoverServices`/`setDnsSdResponseListeners`/`createGroup`/`connect`/`removeGroup`) + Linux wpa_supplicant (`p2p_service_add bonjour`, `p2p_group_add`, `p2p_connect`, `p2p_invite`, `p2p_get_passphrase`) both surface through `WifiDirectAdapter` (Android crate under ANDROID-001; Linux glue under DESKTOP-001/LINUX, future). GO address is adapter-supplied (`go_addr()`, G-WD-2) — the sim mirrors Android practice-stable `192.168.49.1`; the Rust transport never hard-codes it.

---

## 8. Decisions to ratify (DEC-WD-0001..0008 — DECISIONS.md)

- **DEC-WD-0001**: v1 shape — Wi-Fi Direct = data plane on BLE control plane;
  DNS-SD discovery + TCP-over-GO data path reusing INTERNET-001 framing.
- **DEC-WD-0002**: group security = WPA2-Personal (PSK/AES-CCMP) floor; WPA3-SAE
  capability-gated later phase; OWE infra-only.
- **DEC-WD-0003**: WPS-PIN prohibited; group passphrase pushed over authenticated
  BLE control plane; PBC legacy fallback only.
- **DEC-WD-0004**: discovery strategy — 30-s app re-arm cadence vs 120-s single
  find; `WIFI_P2P_DISCOVERY_CHANGED_ACTION` → BLE fallback.
- **DEC-WD-0005**: persistent GO + band policy — `setGroupOperatingBand` (API 29)
  5 GHz preferred / AUTO fallback; band-constrained GO creation failure → degrade.
- **DEC-WD-0006**: client-capacity model — N-client admission + GO-side connection
  table; no hard-coded-8 (vendor/HAL ceiling, G-WD-1); GO intent bias 14/7/3.
- **DEC-WD-0007**: dual-platform adapter — Android `WifiP2pManager` + Linux
  wpa_supplicant via one trait; IRIS-owned IP/DHCP glue (GO static; client
  DHCP/IPv6-link-local); GO address adapter-supplied, never hard-coded (G-WD-2).
- **DEC-WD-0008**: OS patch floor AC (Android SPL ≥ 2021-02; wpa_supplicant ≥
  2.12; kernel with 2024–26 Wi-Fi driver fixes); single-radio coexistence =
  runtime-availability constraint, degrade to BLE.