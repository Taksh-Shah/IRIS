# WIFIAWARE-001 Design — Wi-Fi Aware (NAN) Transport (Android) v1

**Document ID**: IRIS-WIFIAWARE-001-DESIGN-001
**Version**: 1.0
**Node**: WIFIAWARE-001 (P1 TRANSPORT, platform ANDROID, deps TRANSPORT-001 COMPLETE)
**Date**: 2026-08-16
**Research basis**: RES-0020 (2024–2026 Wi-Fi Aware/NAN Android SOTA: API floors +
deprecation, NDP security, 6 GHz, data-path framing, background/FGS, ranging,
coexistence, CVEs/privacy; verdict PROCEED); `docs/transports/WIFI_AWARE.md`
(370-line platform design); TRANSPORT_ABSTRACTION.md (capability matrix +
RadioConflictGroup); BLE-001 DESIGN (the transport-design pattern, iter 85);
INTERNET-001 (reference impl — TCP framing / 1 MiB cap / pool / backoff, 27/27
tests); ACCEPTANCE_POLICY.yaml TRANSPORT-type additions; DEC-0009 (gates lifted);
BLK-0005 (device-test resource gating).
**Absorbs**: RES-0020 R1–R9 + Gaps G-WA-1/2/3 (design-positioned);
CONFLICT-1 (Apple WiFiAware framework iOS 26+) — WIFI_AWARE.md +
TRANSPORT_ABSTRACTION 'iOS NO' rows corrected in this pass.
**Reconciles**: WIFI_AWARE.md vs 2026 reality — R1 (NAN NOT deprecated through
API 36/Android 16 + Android 17 new architecture), R5 (background discovery
requires FGS under Android 12+/15/16 limits — the doc's dated "API 10+ no-FGS"
claim is corrected), R9/iOS section (Apple WiFiAware now exists).

---

## 1. Scope

Implement the Wi-Fi Aware transport on the IRIS `Transport` trait following the
**INTERNET-001 reference pattern**: a **platform-neutral adapter trait**
(`WifiAwareAdapter`) driven by a Rust-core transport, with the real Android
implementation injected at runtime via FFI (uniffi/JNI, Kotlin) and a
`SimulatedWifiAwareAdapter` covering the in-memory/CI path (BLK-0005 device tests
stay gated). The work is **pure-Rust in `crates/iris-core`**; the Android
Kotlin/JNI adapter lives under ANDROID-001 and is scaffolded here as a documented
FFI contract only (physical-device coverage = recorded `known_limitation`).

**v1 shape (RES-0020 R1/R4, verdict PROCEED)**: **publish/subscribe NAN
discovery** (IRIS signed beacon carried as `service_specific_info` /
`match_filter` under a fixed `service_name`) + **NDP (NAN Data Path) IPv6 socket
data path** reusing INTERNET-001 TCP framing (1 MiB cap, pool + backoff). NDP is
a kernel-managed network interface → standard socket APIs; this is exactly the
INTERNET-001 shape, so the framing/pool/backoff machinery transfers unchanged.

**Explicitly not v1 (RES-0020 R2/R3, DEC-WA records)**: NDP security suites (NCS
SK/PK/PASN API 30/33/34 + Aware Pairing API 34+, WFA spec 4.0) and 6 GHz/6E as
requirements. NDP stays **open at L2** in v1 (app-layer envelope = trust anchor);
NCS/Pairing = optional hardening for a future v2; 6 GHz/6E = opportunistic only
when hardware + region allow (G-WA-3).

---

## 2. Platform contract and wire constraints (RES-0020 → ACs)

### 2.1 Runtime capability gate (R1, R7 — AC-3/AC-5)
- Hardware: `PackageManager.hasSystemFeature(FEATURE_WIFI_AWARE)`.
- Runtime availability: `WifiAwareManager.isAvailable` + state-change callback
  (`ACTION_WIFI_AWARE_STATE_CHANGED` broadcast registered in the adapter; expects
  re-attach on `true`, full teardown + degrade to BLE/Wi-Fi Direct on `false`).
- OEM firmware-gated ALWAYS: never assume availability from API level alone.
  Single-radio coexistence (R7) surfaces as availability churn → the transport
  treats Wi-Fi Aware as **opportunistic-availability**: absent/disabled ⇒ degrade
  (BLE-001 pattern), not failure.
- `WifiAwareManager.characteristics` surfaced into `TransportCapabilities`
  where meaningful (max service-specific-info bytes → beacon bound, data-path
  counts → pool ceiling). Values remain kernel/OEM-derived (G-WA-1).

### 2.2 Discovery (R1 — AC-2/AC-3)
- Fixed `service_name` (e.g. `com.iris.mesh.v1`); peer identity is the **signed
  advertisement** embedded in `service_specific_info` (bounded by
  `characteristics.maxServiceSpecificInfoLength`; small subset: PeerId short-id,
  capability bitmask, freshness — mirror of BLE-001 discovery beacon), with
  `match_filter` set so non-IRIS publishers are ignored at the platform layer.
- Types: **unsolicited publish + PASSIVE subscribe** default (battery); **solicited
  publish + ACTIVE subscribe** in emergency mode (faster discovery — WIFI_AWARE.md
  convention retained).
- Every discovery match produces a **candidate peer only** — never a trusted
  state change (app-layer envelope verify happens on the data path, R8/AC-6).
- Follow-up messages (`DiscoverySessionCallback.onMessageReceived` / NDP request)
  stay ≤ follow-up bound; used only to request/accept NDP, never to carry IRIS
  payload.

### 2.3 NDP data path (R4 — AC-2/AC-7)
- `WifiAwareNetworkSpecifier` + `NetworkRequest(TANSPORT_WIFI_AWARE)` →
  `ConnectivityManager.NetworkCallback.onAvailable(Network)` → standard sockets
  from `network.socketFactory`. Link-local IPv6 from `onLinkPropertiesChanged`.
- **Reuse INTERNET-001 framing**: TCP [header]data framing with 1 MiB cap,
  connection pool, backoff (G-WA-1: NDP MTU is kernel-managed and device-variable
  — TCP MSS negotiation over the NDP interface handles MTU automatically; IRIS
  frames never assume a fixed wire MTU, and no frame exceeds the 1 MiB cap).
- Callback-driven: `onLost` = connection invalid → pool evict + re-discover
  (peer dissociation); no unauthenticated re-establish.
- Throughput: matrix values retained (20–100 Mbps practical / ~300 Mbps peak,
  OEM-derived, G-WA-1).

### 2.4 Optional discovery metadata — NOT a v1 gate (R6)
- `WifiRttManager.startRanging` (802.11mc FTM / 802.11az NTB API 35+) +
  geofenced discovery (`SubscribeConfig.setMinDistanceMm/setMaxDistanceMm/
  setRangingEnabled`, API 31+) = optional range metadata for future
  proximity-aware routing. Documented, not required to satisfy any AC; enabled
  opportunistically, never trusted (rejected-as-security-primitive in RES-0018).

### 2.5 Background contract (R5 — AC-10)
- Production discovery requires an **FGS of type `connectedDevice`** (API 34+)
  — the old "API 10+ discovery survives without FGS" claim is **dated**: under
  Android 12+ background limits (and Android 15/16 FGS runtime-quota tightening)
  publishing/subscribing in background is FGS-gated. Declarations:
  `FOREGROUND_SERVICE` + `FOREGROUND_SERVICE_CONNECTED_DEVICE` (manifest),
  `ACCESS_WIFI_STATE`/`CHANGE_WIFI_STATE`/`ACCESS_FINE_LOCATION` (API < 31),
  `NEARBY_WIFI_DEVICES` (API 33+, `neverForLocation` where appropriate), runtime
  `POST_NOTIFICATIONS` (API 33+).
- **Suspend/Resume** (API 34+, HAL-gated) is the recognized power lever — use it
  for background cadence; fall back to attach/discovery teardown + re-attach
  where unsupported. Documented, not a v1 acceptance requirement.

### 2.6 Regulatory / spectrum (R3/G-WA-3)
- 2.4 GHz (ch 6) + 5 GHz (ch 44/149) NAN: license-exempt ISM/U-NII in India —
  OK per WIFI_AWARE.md §Regulatory; design floor.
- 6 GHz / Wi-Fi 6E NAN: hardware + region-gated (India U-NII-5..7 rules vary by
  band); opportunistic only. No regulatory dependency for v1.

---

## 3. Security posture (RES-0020 R2/R8 → AC-6)

1. **App-layer envelope crypto is the trust anchor** (CRYPTO-001 sign+encrypt,
   verify-before-forward, SEC-001 replay/dedup/rate gates). v1 NDP is **open at
   L2** — never depend on NCS/PSK/certificates for security. Optional hardening
   (NCS SK/PK/PASN API 30/33/34; Aware Pairing API 34+, WFA 4.0) = v2 candidate
   (DEC-WA-0002).
2. **Defensive advertise-parse** with adversarial/property tests — the Android
   Wi-Fi stack CVE pattern is parser/state bugs; never panic/alloc-on-malformed
   `service_specific_info`.
3. **No unauthenticated-trigger actions**: discovery and NDP setup produce
   *candidate/connection* objects only; every IRIS payload flows through envelope
   verify + engine gate (mirrors BLE-001 R5.3).
4. **OS security-patch floor documented** in the Android deployment doc —
   Wi-Fi stack CVEs are unpatched-by-app (G-WA-2; e.g. kernel iwlwifi
   CVE-2024-49857 is OS-patch-gated). No NAN-specific app-layer CVE wave found
   2024–2026 (RES-0020 R8), but posture assumes it.
5. **Privacy**: NAN MAC randomization is platform-built-in (~30 min rotation,
   factory MAC never used; `IdentityChangedListener` events handled). IRIS
   identifies peers by **public-key fingerprint, never NAN MAC/PeerHandle**.
6. **On-wire capacity guards**: per-peer NDP pool bounded by
   `characteristics` data-path counts; INTERNET-001 backoff applied (mirrors
   quota/replay discipline).

---

## 4. Module layout (pure Rust, `crates/iris-core/src/transport/`)

| Piece | Status | What changes |
|---|---|---|
| `WifiAwareAdapter` trait | new | platform contract: `is_available()`, `attach(callback)`, `publish(config,cb)`, `subscribe(config,cb)`, `request_ndp(peer)`, `accept_ndp(peer)`, `close_ndp(peer)`, `send_ndp(peer,data)`, `incoming_ndp()`, `stop_discovery()`, `shutdown()` — maps 1:1 to Android `WifiAwareManager`/`WifiAwareSession`/`DiscoverySession`/`NetworkCallback` (Kotlin FGS code under ANDROID-001) |
| `WifiAwareTransport` | new | `Transport` impl: capability gate + availability refresh (AC-3/AC-5), discovery → candidate peers (AC-2), NDP pool + INTERNET-001 framing reuse (AC-4/AC-7), lifecycle (AC-8) |
| `wifiaware_beacon.rs` | new | signed IRIS beacon build/parse + `CapabilityBits` + `BeaconError` — defensive decode of `service_specific_info`/`match_filter`, adversarial tests (AC-6) |
| `SimulatedWifiAwareAdapter` | new | deterministic in-memory peer rendezvous (publish/subscribe match, NDP connect, inbound data) for CI E2E — conformance type-checks the FFI contract (AC-11) |
| Android Kotlin/JNI adapter | not yet | ANDROID-001 scope; `WifiAwareAdapter` trait IS the FFI contract (BLK-0005 known_limitation) |
| tests | new (`transport::wifi_aware` suite) | adapter conformance, beacon adversarial, discovery candidate flow, NDP E2E, availability churn, lifecycle |

**Capability matrix entry (TRANSPORT-001)**: `max_message_size: 1 MiB`
(INTERNET-001 framing cap — MSG-001 fragmentation applies above),
`supports_broadcast: true` (publish/subscribe discovery), `supports_unicast: true`,
`multicast: false`, `range 10..300 m` (typical 40), `typical_throughput_bps:
20_000_000` (practical floor of 20–100 Mbps, G-WA-1 conservatism),
`cost_class: Free`, `supports_background_android: true` (FGS connectedDevice),
`supports_background_ios: false` (v1 Android-only; Apple WiFiAware recognized in
docs only, DEC-WA-0008).

---

## 5. Android FFI adapter contract (ANDROID-001 seam)

The `WifiAwareAdapter` trait IS the FFI contract. The Android crate (Kotlin/JNI)
must implement §4 lifecycle around `WifiAwareManager.attach` +
`WifiAwareSession.publish/subscribe` + `WifiAwareNetworkSpecifier`/
`ConnectivityManager.requestNetwork`, the FGS (§2.5), and the
`ACTION_WIFI_AWARE_STATE_CHANGED` receiver (§2.1). Type-checked in CI by
`SimulatedWifiAwareAdapter` conformance (AC-11); physical coverage gated
(BLK-0005).

---

## 6. Acceptance criteria (WIFIAWARE-001 AC-1..n — resolves the C2 gap)

Defined in `engineering/PROJECT_GRAPH.yaml` WIFIAWARE-001 node (pattern BLE-001
AC-1..16):

- **AC-1** `WifiAwareTransport` is a `Transport`-trait impl registering + selecting
  via `TransportManager` like INTERNET-001 (capabilities/eligibility test).
- **AC-2** Discovery = publish/subscribe: unsolicited+PASSIVE default +
  solicited+ACTIVE emergency; signed beacon in `service_specific_info` produces
  candidate peers; `stop_discovery` stops (subscription lifecycle + handle
  round-trip).
- **AC-3** Runtime capability gate: not available (missing feature / disabled /
  state-changed) ⇒ transport degrades to Unavailable + reports no peers (degrade
  to BLE is manager-level); re-available ⇒ re-attach path works.
- **AC-4** NDP connect/send/receive E2E via `SimulatedWifiAwareAdapter` (mock
  peer): NDP request → accept → INTERNET-001-framed payload → inbound → engine
  seam; onLost ⇒ pool evict + re-discover.
- **AC-5** Availability refresh: state-change callback path + `isAvailable()`
  + `characteristics` bounds are exercised (no fixed API-level assumptions).
- **AC-6** Adversarial beacon parse: malformed/truncated/oversized
  `service_specific_info` rejected without panic/allocation blowup (property +
  regression tests).
- **AC-7** App-layer security: every inbound IRIS message passes engine envelope-
  verify seam; unauthenticated NDP/discovery only produces candidates; identity
  by public-key fingerprint, never NAN MAC/PeerHandle (RES-0020 R2/R8).
- **AC-8** Lifecycle: `shutdown()` stops discovery + closes all NDP + state
  Unavailable; attach failure retries with backoff (no unbounded retry loop).
- **AC-9** Reliability: per-peer single NDP reused; close promptly; NDP pool
  bounded by `characteristics` data-path counts (no unbounded churn).
- **AC-10** Background + platform limitations DOCUMENTED in `WIFI_AWARE.md`:
  FGS connectedDevice (API 34+) required for production discovery, Suspend/Resume
  (API 34+) power lever, patch floor, 6 GHz gating (ACCEPTANCE_POLICY.TRANSPORT).
- **AC-11** FFI contract type-checked: `WifiAwareAdapter` implemented + clippy-
  clean; `SimulatedWifiAwareAdapter` conformance.
- **AC-12** Doc reconciliation: `WIFI_AWARE.md` + TRANSPORT_ABSTRACTION.md +
  WIFIAWARE-001 node agree with code, including **CONFLICT-1**: Apple WiFiAware
  (iOS 26+, iPhone 12+) reflected in docs; BLE-002 = v1 iOS path; Android↔Apple
  NDP interop recorded immature; code `caps` stay Android v1.
- **AC-13** Workspace green + clippy 0; baseline 533/0/1 held.
- **AC-14** (GATED/BLK-0005, recorded) physical-device NDP discovery/data/battery
  BENCH = known_limitation; battery figures = documented design estimates.
- **AC-15** SECURITY_REVIEW stage: redteam adversarial findings dispositioned/
  fixed/recorded.
- **AC-16** VERIFY: independent verification doc evidence table; verifier APPROVE
  (pattern EMERG-001/SEC-001/BLE-001).

---

## 7. Declared decisions (DEC-WA-0001..n — ratified in DECISIONS.md)

- DEC-WA-0001: v1 = publish/subscribe NAN discovery + NDP IPv6 socket data path
  reusing INTERNET-001 TCP framing (1 MiB cap, pool + backoff). (RES-0020 R1/R4)
- DEC-WA-0002: NDP open at L2 in v1; app-layer envelope = trust anchor; NCS cipher
  suites + Aware Pairing (API 34+) = optional v2 hardening. (RES-0020 R2)
- DEC-WA-0003: 6 GHz/6E NOT a v1 requirement — opportunistic only, hardware +
  region-gated (G-WA-3). (RES-0020 R3)
- DEC-WA-0004: FGS connectedDevice (API 34+) required for production discovery;
  Suspend/Resume (API 34+) power lever. (RES-0020 R5)
- DEC-WA-0005: runtime capability gate mandatory — FEATURE_WIFI_AWARE +
  isAvailable() + ACTION_WIFI_AWARE_STATE_CHANGED; degrade to BLE/Wi-Fi Direct
  when unavailable. (RES-0020 R1/R7)
- DEC-WA-0006: discovery defaults = unsolicited publish + PASSIVE subscribe;
  solicited + ACTIVE in emergency. (WIFI_AWARE.md, RES-0020 R1)
- DEC-WA-0007: peer identity = public-key fingerprint, never NAN MAC/PeerHandle;
  no unauthenticated peer-triggered actions. (RES-0020 R8, SEC-001/IDENT-001)
- DEC-WA-0008: CONFLICT-1 — Apple WiFiAware (iOS 26+) recognized in docs;
  v1 iOS path stays BLE-002; Android↔Apple NDP interop immature. (RES-0020 R9)

---

## 8. Risks / known limitations (recorded, not blockers)

- Physical-device tests + battery BENCH gate (BLK-0005, RESOURCE after DEC-0009);
  battery figures are documented design estimates.
- NDP MTU/throughput is kernel- and OEM-managed (G-WA-1): framing is
  MTU-agnostic (INTERNET-001 1 MiB cap); throughput matrix is a range, not a
  guarantee.
- Android Wi-Fi stack CVEs unpatched-by-app: OS security-patch floor documented
  (G-WA-2; e.g. iwlwifi CVE-2024-49857). NAN MAC randomization built-in but
  sample-period is OEM-variable.
- Android 17 NAN architecture ("new architecture" mode, R1) is a forward risk —
  keep `WifiAwareAdapter` thin so the Kotlin adapter absorbs the migration.
- 6 GHz/6E (G-WA-3): no v1 dependency; opportunistic enable later.
- iOS wifi-awareApi reality (CONFLICT-1): BLE-002 remains the v1 iOS path;
  cross-platform NDP interop TBD at platform nodes.

---
**Next**: IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT scope =
pure-Rust modules (§4); Android Kotlin adapter under ANDROID-001 (BLK-0005).