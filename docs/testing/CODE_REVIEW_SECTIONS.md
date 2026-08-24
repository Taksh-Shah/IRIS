# IRIS Codebase — 6-Way Independent Review Split

**Team:** 6 reviewers, new to this codebase. **Scope:** research + code review + bug-hunting, each person confined to one section. **Ground rule applied throughout:** every shared file has exactly one owning section; everyone else treats it as read-only and reports issues to the owner instead of fixing it themselves.

---

## STEP 1 — Project map

IRIS is a cross-platform, offline-first mesh-messaging system: a Rust protocol/crypto/routing core, consumed by three native app shells (Android/Kotlin, iOS/Swift, Desktop/Tauri), backed by ~250 design/spec docs and a policy/process scaffold (`engineering/`).

| Subsystem | Location | Language | Approx. size |
|---|---|---|---|
| Crypto primitives | `crates/iris-core/src/crypto/` | Rust | 714 lines |
| Identity & keys | `crates/iris-core/src/identity/` | Rust | 2,854 lines |
| Security (spam/quota/ACL/replay/reputation/rate-limit) | `crates/iris-core/src/security/` | Rust | 4,854 lines |
| Wire protocol (codec/envelope) | `crates/iris-core/src/protocol/` | Rust | 1,493 lines |
| Message lifecycle engine | `crates/iris-core/src/message_engine/` | Rust | 4,914 lines |
| Peer discovery / handshake | `crates/iris-core/src/discovery/` | Rust | 1,271 lines |
| Emergency (SOS/broadcast/authority) | `crates/iris-core/src/emergency/` | Rust | 3,383 lines |
| Routing (flood/opportunistic/PRoPHET/store-carry-forward) | `crates/iris-core/src/routing/` | Rust | 2,990 lines |
| Transport adapters (BLE/WiFi Direct/WiFi Aware/LoRa/Satellite/Internet) | `crates/iris-core/src/transport/` | Rust | 13,424 lines |
| Internet gateway bridging | `crates/iris-core/src/gateway/` | Rust | 1,281 lines |
| Simulation + ML routing predictor | `crates/iris-core/src/sim/` | Rust | 2,325 lines |
| Metrics/telemetry | `crates/iris-core/src/observability/` | Rust | 351 lines |
| Durable storage (eviction/GC/Postgres/seal) | `crates/iris-storage/src/` | Rust | 726 lines |
| Android app | `android/app/src/main/kotlin/` | Kotlin | 5,913 lines |
| Android↔Rust FFI bridge | `crates/iris-android/src/` | Rust | 1,670 lines |
| Shared UniFFI bindings (generated) | `kotlin/src/main/kotlin/` | Kotlin | 6,788 lines |
| iOS app | `ios/IRIS/`, `ios/IrisWidgetExtension/` | Swift | 4,497 lines |
| iOS↔Rust FFI bridge | `crates/iris-ios/src/` | Rust | 1,070 lines |
| Desktop app (Tauri) | `crates/iris-desktop/` | Rust + JS | 877 Rust lines + thin HTML/JS UI |
| Build/dependency/CI infra | root `Cargo.toml`, `deny.toml`, `.cargo/`, `.config/`, `.github/workflows/` | config | small |
| Process/agent scaffolding | `engineering/` | YAML/MD | ~150 files, non-runtime |
| Design & spec docs | `docs/` | Markdown | 236 files, non-runtime |

**Files flagged as shared (used by more than one subsystem)** — each gets exactly one owner below, everyone else references it read-only:

| Shared file | Why it's shared | Owner (Step 3) |
|---|---|---|
| `crates/iris-core/src/message.rs` | Defines `MessagePriority`, `PeerId`, `TransportLink`, `LinkQuality`, `NodeAdvertisement` — the vocabulary `transport`, `routing`, `discovery`, and `message_engine` all compile against | Section 2 |
| `crates/iris-core/src/error.rs` | `TransportError` is returned across transport, routing, and gateway | Section 3 |
| `crates/iris-core/src/lib.rs` | Crate root — re-exports the public surface every platform FFI crate (`iris-android`, `iris-ios`, `iris-desktop`) binds against | Section 2 |
| `crates/iris-core/src/kani_proofs.rs` | Formal-verification harness; individual proofs touch codec (protocol), fragment (message_engine), rate_limiter/quota (security), and prophet (routing) | Section 2 (coordinate with Sections 1 & 3 before changing the proofs that cover their code) |
| `crates/iris-storage` | Consumed by `message_engine/storage.rs` (Section 2) for durable persistence and by `routing/{scf,store}.rs` (Section 3) for store-carry-forward | Section 3 |
| `kotlin/` (generated UniFFI bindings) | Mirrors whatever `crates/iris-android` exposes from `iris-core`; regenerated, not hand-maintained | Section 4 |
| Root `Cargo.toml` / `Cargo.lock` / `deny.toml` | Workspace-wide dependency graph and license/audit policy for all 5 Rust crates | Section 6 |
| `.github/workflows/ci.yml` | Builds/tests every crate | Section 6 (note: `ios.yml` is iOS-specific, owned by Section 5) |

---

## STEP 2 — Why this cuts into 6

The Rust core (`crates/iris-core`, ~39,900 lines) is 65% of all source and its 12 submodules are genuinely interdependent (crypto → identity → protocol → message_engine → routing), so it cannot go to one person. It splits cleanly into **3 slices along the actual data path** — trust/anti-abuse, message lifecycle, and connectivity — each independently reviewable against its own docs without reading the other two. The three native app shells (Android, iOS, Desktop) are naturally independent already: different languages, different toolchains, no shared code between them beyond the Rust core they each bind through FFI. That's a clean **3 core + 3 platform = 6**, with `iris-storage` folded into the connectivity slice (it's only ever called by routing/message_engine, never reviewed standalone) and each platform's generated-bindings/FFI-glue folded into that platform's section rather than split out.

---

## Table of contents

| # | Section | Owns | Difficulty |
|---|---|---|---|
| 1 | [Crypto, Identity & Security](#section-1--crypto-identity--security) | Trust and anti-abuse layer | **Hard** |
| 2 | [Protocol, Messaging & Emergency](#section-2--protocol-messaging--emergency) | Wire format, message lifecycle, discovery, emergency | **Hard** |
| 3 | [Routing, Transport & Storage](#section-3--routing-transport--storage) | Path selection, all radio adapters, gateway, sim, persistence | **Hard** |
| 4 | [Android App](#section-4--android-app) | Kotlin app + Android FFI bridge + generated bindings | **Medium–Hard** |
| 5 | [iOS App](#section-5--ios-app) | Swift app + widget extension + iOS FFI bridge | **Medium–Hard** |
| 6 | [Desktop App & Build Infrastructure](#section-6--desktop-app--build-infrastructure) | Tauri desktop app + workspace/CI/dependency policy + process docs | **Medium** |

---

## Section 1 — Crypto, Identity & Security

**Owns:** cryptographic primitives, key/identity lifecycle, and the anti-abuse pipeline (spam, quota, ACL, replay, reputation, rate-limiting). This is the trust boundary everything else calls into but never needs to understand internally.

**Files/folders:**
```
crates/iris-core/src/crypto/{aead,ed25519,kdf,key_directory,keygen,mod,x25519}.rs
crates/iris-core/src/identity/{advertise,chain,mod,peer_id,provision,rotate,small_order,store,trust_store}.rs
crates/iris-core/src/security/{acl,mod,quota,rate_limiter,replay,reputation,spam}.rs
crates/iris-core/benches/crypto.rs
crates/iris-core/tests/crypto_e2e.rs
crates/iris-core/proptest-regressions/security/{acl,quota,rate_limiter,replay}.txt
crates/iris-core/fuzz/Cargo.toml, fuzz/fuzz_targets/fuzz_security_engines.rs
```
**Read-only reference (owned elsewhere, report issues to that section):** `crates/iris-core/src/message.rs` (Section 2), `crates/iris-core/src/error.rs` (Section 3).

**What to check:**
- Constant-time behavior and zeroization in `crypto/{aead,ed25519,x25519,kdf}.rs` — any secret-dependent branch or `Drop` that doesn't zero key material.
- `identity/rotate.rs` and `identity/chain.rs` — does key rotation correctly invalidate the old key in `trust_store.rs` without a window where both are accepted?
- `identity/small_order.rs` — confirm small-order/low-order point rejection is actually applied on every X25519 key-agreement path, not just at generation.
- `security/replay.rs` — sequence-number derivation and eviction window (recently touched per git history — verify the fix that ties replay sequence to the signed id is complete and the eviction bound can't be bypassed by a crafted id).
- `security/rate_limiter.rs` and `security/quota.rs` — token-bucket refill math for overflow/underflow; cross-check against the Kani proofs `rate_limiter_refill_never_exceeds_burst` and `quota_eviction_bounded_and_positive` in `kani_proofs.rs` (read-only, owned by Section 2) — do the proofs actually cover the current implementation, or has the code drifted?
- `security/acl.rs` and `security/spam.rs` — default-deny vs default-allow on unrecognized message classes; ensure `EmergencyAcl` can't be spoofed by a non-authority peer.
- `security/reputation.rs` — decay/recovery curve for a peer's score; check for a peer being permanently locked out with no recovery path (self-inflicted DoS).

**Where to check from:**
```bash
cargo test -p iris-core crypto:: identity:: security::
cargo test -p iris-core --test crypto_e2e
cargo bench -p iris-core --bench crypto
cargo test -p iris-core --lib -- --include-ignored security   # exercises proptest-regressions
cargo fuzz run fuzz_security_engines                            # from crates/iris-core/fuzz, needs nightly
```
Compare behavior against: `docs/identity/*.md`, `docs/security/*.md`, `docs/implementation/{CRYPTO_DESIGN,CRYPTO_VERIFICATION,IDENT_DESIGN,IDENT_VERIFICATION,SEC_001_DESIGN,SEC_001_VERIFICATION}.md`, `docs/safety/ABUSE_PREVENTION.md`. Prior review history: `engineering/memory/records/{CRYPTO-001_SECURITY_REVIEW,IDENT-001_SECURITY_REVIEW,IDENT-001_TEST,SEC-001_SECURITY_REVIEW,SEC-001_TEST}.md`.

**Interfaces/dependencies:**
- **Sends to Section 2:** `SecurityPolicy`/`FullSecurityPolicy` trait (from `security/mod.rs`) is called by `message_engine` and `emergency` to gate every inbound message.
- **Sends to Section 2:** `emergency/authority.rs` calls `identity::trust_store` to verify authority signatures on emergency broadcasts.
- **Sends to Sections 4 & 5:** the `KeyStore` trait (`identity/store.rs`) is implemented on the platform side by `android/.../identity/KeystoreEd25519.kt` and `ios/IRIS/identity/KeychainEd25519.swift`.
- **Receives from Section 2:** `message.rs` types (owned there) for anything security code needs to inspect (e.g. `MessagePriority` in ACL decisions).

**Difficulty: Hard.** Cryptographic and anti-abuse code is exactly where subtle, hard-to-spot bugs hide (timing leaks, off-by-one eviction windows, bypassable state machines), and a junior team has no tribal knowledge of why a given check exists — lean hard on the proptest/fuzz/Kani harnesses already in the repo rather than eyeballing alone.

---

## Section 2 — Protocol, Messaging & Emergency

**Owns:** the wire format, the full message lifecycle (queue/dedup/fragment/ack/expiry), peer discovery/handshake, and the emergency (SOS/broadcast/authority) subsystem built on top of it. This is "what a message is and how it moves through one node."

**Files/folders:**
```
crates/iris-core/src/protocol/{codec,content_type,envelope,message_id,mod}.rs
crates/iris-core/src/message_engine/{ack,crypto,dedup,expiry,fragment,lifecycle,mod,queue,storage}.rs
crates/iris-core/src/discovery/{handshake,mod,neighbor_table}.rs
crates/iris-core/src/emergency/{audit,authority,broadcast,codec,drill,mod,mode,model,provider,rate_limit,sos}.rs
crates/iris-core/src/message.rs        # owned here (shared type, see Step 1 table)
crates/iris-core/src/lib.rs            # owned here (shared crate root, see Step 1 table)
crates/iris-core/src/kani_proofs.rs    # owned here (shared proof harness, see Step 1 table)
crates/iris-core/benches/{codec,envelope,fragment}.rs
crates/iris-core/tests/{golden_vectors,protocol_conformance,sysval_dtn_multihop,sysval_mesh_integration,sysval_security_flood}.rs
```
**Read-only reference:** `crates/iris-core/src/security/mod.rs` (Section 1, called for message gating), `crates/iris-storage` (Section 3, called by `message_engine/storage.rs`).

**What to check:**
- `protocol/codec.rs` — CBOR encode/decode round-trips exactly for every `ContentType`; check against `docs/protocol/PROTOCOL_TEST_VECTORS.md` for byte-for-byte compatibility, not just "it deserializes."
- `message_engine/fragment.rs` — reassembly budget/timeout logic: can a peer hold a partial fragment set forever and leak memory? Cross-check the Kani proof `fragment_split_payload_conservation_and_budget`.
- `message_engine/dedup.rs` — dedup cache TTL and key derivation; verify it can't be defeated by a 1-bit envelope mutation.
- `message_engine/queue.rs` — priority ordering across `MessagePriority` P0–P7 (SOS must always preempt P7 video); check for starvation of low-priority traffic.
- `message_engine/ack.rs` and `expiry.rs` — retry/backoff schedule vs. TTL expiry: can a message be ack'd after it's already expired and evicted?
- `discovery/handshake.rs` — handshake authentication (recently hardened per git history) — confirm both sides verify identity before any state is created for an unauthenticated peer (resource-exhaustion angle).
- `emergency/sos.rs` + `emergency/rate_limit.rs` — SOS flood protection: does rate-limiting an attacker also rate-limit a genuine mass-casualty event (the one case it must never throttle)?
- `emergency/authority.rs` — authority-signature verification path; confirm it delegates to Section 1's `trust_store` and never rolls its own crypto.

**Where to check from:**
```bash
cargo test -p iris-core --test golden_vectors --test protocol_conformance
cargo test -p iris-core --test sysval_mesh_integration --test sysval_dtn_multihop --test sysval_security_flood
cargo bench -p iris-core --bench codec --bench envelope --bench fragment
```
Compare against: `docs/protocol/*.md`, `docs/implementation/{MSG_DESIGN,MSG_VERIFICATION,DISCO_VERIFICATION,EMERG_DESIGN,EMERG_VERIFICATION}.md`, `docs/emergency/*.md`, `docs/safety/{EMERGENCY_GOVERNANCE,AUTHORITY_VERIFICATION}.md`. Prior review history: `engineering/memory/records/{EMERG-001_SECURITY_REVIEW,EMERG-001_TEST,TEST-001_DISCOVER,TEST-001_TEST,TEST-001_VERIFICATION,TEST-0001,SYSVAL_001_TEST}.md`.

**Interfaces/dependencies:**
- **Receives from Section 1:** `SecurityPolicy` gating decisions before accepting a message into the queue.
- **Sends to Section 3:** `message_engine/storage.rs` calls `iris-storage` for durable persistence; the `Envelope`/`RoutingHints` types (protocol) are consumed by `routing/*` to make forwarding decisions.
- **Sends to Sections 4/5/6:** `lib.rs`'s `pub use` surface is the entire contract that `crates/iris-android`, `crates/iris-ios`, and `crates/iris-desktop` build against — a signature change here breaks all three platform crates simultaneously.
- **Kani proofs:** `kani_proofs.rs` (owned here) contains proofs over Section 1's `rate_limiter`/`quota` and Section 3's `prophet` — coordinate before editing those specific harnesses.

**Difficulty: Hard.** The message lifecycle is a multi-timer state machine (queue, dedup, fragment, ack, expiry all interacting), wire-format changes can silently break interop, and emergency-path bugs have real safety stakes — this needs careful state-machine reasoning, not just line-by-line reading.

---

## Section 3 — Routing, Transport & Storage

**Owns:** how bytes actually move between nodes — path selection, every physical transport adapter, internet gateway bridging, the network simulator/ML routing predictor, telemetry, and durable storage. The largest section by volume, but internally coherent: "given a message ready to send, how does it get to the next hop."

**Files/folders:**
```
crates/iris-core/src/routing/{dedup_cache,direct,flood,known_path,mod,opportunistic,prophet,scf,scf_contact,scf_eviction,store}.rs
crates/iris-core/src/transport/{ble,ble_advert,ble_att,internet,lora,manager,mod,satellite,simulated,wifi_direct,wifi_direct_serv,wifiaware,wifiaware_beacon}.rs
crates/iris-core/src/gateway/mod.rs
crates/iris-core/src/sim/{metrics,mod,scenario}.rs, sim/ml/{experiment,features,mod,predictor}.rs
crates/iris-core/src/observability/mod.rs
crates/iris-core/src/error.rs           # owned here (shared type, see Step 1 table)
crates/iris-core/benches/routing.rs
crates/iris-core/tests/{sim_scenarios,ml_experiments,obs_telemetry,tokio_behavior}.rs
crates/iris-core/loom/loom_models.rs
crates/iris-storage/src/{eviction,gc,lib,pg,schema,seal}.rs
crates/iris-storage/tests/{common/mod,m3_engine,pg_store}.rs
```
**Read-only reference:** `crates/iris-core/src/message.rs` (Section 2), `crates/iris-core/src/protocol/envelope.rs` (Section 2).

**What to check:**
- `transport/lora.rs` — regional duty-cycle/airtime enforcement (recently touched per git history — "bound the duty clock"); confirm the bound actually prevents exceeding regulatory limits under a busy queue, not just under normal load, per `docs/legal/SPECTRUM_CONSIDERATIONS.md`.
- `routing/scf_eviction.rs` — store-carry-forward eviction ordering: is it strictly bounded (can't grow unbounded on a partitioned network), and does eviction pick the *lowest*-priority/oldest message first, never P0/SOS?
- `routing/prophet.rs` — delivery-predictability decay/aging math; cross-check the Kani proof `prophet_floor_intervals_division` (owned by Section 2) still matches this implementation.
- `transport/manager.rs` — transport selection/failover: does losing one transport (e.g. BLE) correctly fail over without dropping in-flight messages?
- `gateway/mod.rs` — gateway selection quality scoring (`compute_gateway_quality`); check for a node falsely advertising gateway capability being trusted without verification.
- `iris-storage/src/eviction.rs` + `gc.rs` — garbage-collection correctness under concurrent writes (this crate is also used from Postgres-backed `pg.rs` — verify GC and PG paths agree on what "expired" means).
- `sim/ml/predictor.rs` — is the ML routing predictor's output ever used to *bypass* a security check, or only to influence path preference? (Should be the latter only.)
- Six independent radio adapters (`ble*.rs`, `wifi_direct*.rs`, `wifiaware*.rs`, `satellite.rs`, `internet.rs`) each need a per-adapter pass for: connection-state handling on unexpected disconnect, and whether errors from the underlying OS API are surfaced or silently swallowed.

**Where to check from:**
```bash
cargo test -p iris-core --test sim_scenarios --test ml_experiments --test obs_telemetry --test tokio_behavior
cargo bench -p iris-core --bench routing
cargo test -p iris-storage
bash engineering/tools/verify-loom.sh    # concurrency model-checking for loom_models.rs
```
Real BLE/WiFi/LoRa/satellite hardware isn't required to review most of this — `transport/simulated.rs` and `sim/scenario.rs` exist specifically to exercise routing/transport logic without hardware; use those first, then only test real adapters against actual devices where available. Compare against: `docs/routing/*.md`, `docs/transports/*.md`, `docs/simulation/*.md`, `docs/intelligence/*.md`, `docs/implementation/{GATEWAY_DESIGN,GW_VERIFICATION,ROUTE_VERIFICATION,ROUTE2_DESIGN,ROUTE2_VERIFICATION,BLE_001_VERIFICATION,BLE_002_DESIGN,BLE_TRANSPORT_DESIGN,WIFI_AWARE_TRANSPORT_DESIGN,WIFI_DIRECT_TRANSPORT_DESIGN,LORA_001_DESIGN,SAT_001_DESIGN,INTERNET_TRANSPORT_VERIFICATION,STORAGE,STORE_REQUIREMENTS,STORE_SECURITY_REVIEW,STORE_STAGE_SKIPS,STORE_VERIFICATION,SIM_VERIFICATION,OBSERVABILITY,OBS_DESIGN,OBS_VERIFICATION}.md`, `docs/performance/{BANDWIDTH,LATENCY,SCALE,BENCHMARKING}.md`. Prior review history: `engineering/memory/records/{BLE-001_*,BLE-002_*,LORA-001_*,SAT-001_*,WIFIAWARE-001_*,WIFIDIRECT-001_*}.md`.

**Interfaces/dependencies:**
- **Receives from Section 2:** `Envelope`/`RoutingHints`/`message.rs` types to make forwarding decisions on.
- **Sends to Section 2:** persistence calls from `message_engine/storage.rs` land in `iris-storage` (owned here).
- **Sends to Sections 4 & 5:** the `Transport` trait (`transport/mod.rs`) is implemented on the platform side — `crates/iris-android/src/ffi/{ble_adapter,wifi_aware_adapter,wifi_direct_adapter}.rs` and `crates/iris-ios/src/ffi/ble_adapter.rs` bridge native radio APIs into it.
- **Kani proofs:** `prophet_floor_intervals_division` lives in Section 2's `kani_proofs.rs` but covers this section's `routing/prophet.rs` — coordinate before changing PRoPHET's math.

**Difficulty: Hard.** Largest surface area of any section (~24,000 lines), six independently-quirky hardware adapters, async/concurrent state (tokio + loom models), and regulatory constraints (LoRa duty cycling) — the combination of size and "can't fully test without hardware" makes this the hardest section to review with confidence for a team new to the codebase.

---

## Section 4 — Android App

**Owns:** the Android application end-to-end — UI, background services, permissions, WorkManager sync, and the Rust FFI bridge + generated bindings that connect it to `iris-core`.

**Files/folders:**
```
android/app/src/main/kotlin/iriscore/**  (IrisApplication.kt, adapter/, command/, data/,
    designsystem/, di/, identity/, service/, ui/, util/, worker/ — full list in repo)
android/app/src/test/kotlin/iriscore/**  (AdapterLifecycleTest, FramedSocketLinkTest,
    CommandEngineTest, DesignTokenParityTest, KeystoreEd25519Test, ScanRestartPolicyTest,
    BatteryOptimizationGuidanceTest, MeshSyncPolicyTest)
android/app/{build.gradle.kts,proguard-rules.pro}, android/{build.gradle.kts,settings.gradle.kts,gradle.properties}
android/app/src/main/AndroidManifest.xml, android/app/src/main/res/**
crates/iris-android/src/{bridge,engine,ffi/ble_adapter,ffi/error,ffi/mod,ffi/wifi_aware_adapter,ffi/wifi_direct_adapter,lib}.rs
kotlin/src/main/kotlin/iriscode/api.kt
kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt   # generated UniFFI bindings, owned here
```
**Read-only reference:** `android/app/src/main/jniLibs/**/*.so` are build artifacts produced from `crates/iris-android` — flag mismatches, don't review the binaries themselves. `crates/iris-core/src/transport/mod.rs` (Section 3, the trait being implemented), `crates/iris-core/src/identity/store.rs` (Section 1, the trait being implemented).

**What to check:**
- `util/MeshPermissions.kt` — runtime permission handling for BLE scan/connect and location (recently touched per git history — "guard permissions"); confirm every code path that starts a scan checks permission *first*, not after.
- `service/{IrisBleService,BleScanSession,BleScanReceiver,ScanRestartPolicy}.kt` — foreground-service lifecycle: does the service correctly restart after the OS kills it, without duplicate scan sessions?
- `worker/{IrisBackgroundSyncWorker,MeshSyncPolicy,WorkScheduler}.kt` — WorkManager constraints and backoff policy; verify sync doesn't silently stop being scheduled after repeated failures.
- `identity/KeystoreEd25519.kt` — Android Keystore usage: is the private key ever exported/loggable, and does it handle `KeyStoreException` (e.g. after a factory reset) without crashing?
- `adapter/{AndroidBleTransportAdapter,AndroidWifiAwareTransportAdapter,AndroidWifiDirectTransportAdapter,SocketDataPath}.kt` — do these correctly implement the `Transport` trait contract (Section 3) for connection-state and error propagation, matching what `crates/iris-android/src/ffi/*_adapter.rs` expects on the Rust side?
- `data/{MeshRepository,RelayOutbox}.kt` — does the outbox correctly persist/replay messages across app restarts?
- `ui/MeshViewModel.kt` — state updates on the main thread only; check for any Compose state mutation from a background dispatcher.

**Where to check from:**
```bash
./gradlew testDebugUnitTest        # from android/
./gradlew assembleDebug
cargo test -p iris-android         # Rust FFI bridge
```
Manual adapter verification (BLE/WiFi Direct/WiFi Aware) needs a physical Android device or two — the unit tests above cover policy/lifecycle logic without hardware. Compare against: `docs/platforms/ANDROID.md`, `docs/implementation/{ANDROID_DESIGN,KOTLIN_LAYER}.md`, `docs/testing/ANDROID_BUILD_STATUS.md`. Prior review history: `engineering/memory/records/{ANDROID-001_SECURITY_REVIEW,ANDROID-001_TEST,ANDROID-001_VERIFICATION}.md`.

**Interfaces/dependencies:**
- **Receives from Section 3:** the `Transport` trait contract that `adapter/*.kt` + `crates/iris-android/src/ffi/*` implement.
- **Receives from Section 1:** the `KeyStore` trait contract that `identity/KeystoreEd25519.kt` implements.
- **Receives from Section 2:** the `lib.rs` public API surface, mirrored into `kotlin/` bindings (owned here) — if that surface changes, regenerate and re-verify these bindings.
- **Sends to Section 6:** any new Cargo dependency pulled in by `crates/iris-android` needs Section 6's `deny.toml` sign-off.

**Difficulty: Medium–Hard.** The UI layer itself is approachable, but background-service lifecycle, Android's permission model, and the JNI/UniFFI boundary are classic sources of subtle bugs (works in the emulator, fails after Doze mode or a permission revocation) that a team unfamiliar with Android specifics will need to test deliberately rather than assume.

---

## Section 5 — iOS App

**Owns:** the iOS application end-to-end — UI shell, Core Bluetooth integration, background tasks, Live Activity widget, Keychain identity, and the Rust FFI bridge.

**Files/folders:**
```
ios/IRIS/App/{AppDelegate,IRISApp}.swift
ios/IRIS/RustFFI/IrisCore.swift
ios/IRIS/Services/{BGTaskWiring,CBManagerCentral,CBManagerPeripheral,CoreBluetoothSeam,
    IosBleAdapter,IrisBleConstants,LiveActivityController,Notifications}.swift
ios/IRIS/identity/KeychainEd25519.swift
ios/IRIS/session/SessionRecovery.swift
ios/IrisWidgetExtension/{Info.plist,IrisLiveActivityWidget.swift}
ios/Tests/{FfiConformanceTests,IosBleAdapterTests,KeychainIdentityTests,MockCoreBluetooth,
    ProbeAdmissionTests,SessionRecoveryTests}.swift
ios/{project.yml,G-IOS_SPIKE.md}, ios/Scripts/build-xcframework.sh
crates/iris-ios/src/{bridge,engine,ffi/ble_adapter,ffi/body,ffi/error,ffi/mod,lib}.rs
.github/workflows/ios.yml
```
**Read-only reference:** `crates/iris-core/src/transport/mod.rs` (Section 3), `crates/iris-core/src/identity/store.rs` (Section 1).

**What to check:**
- `Services/CoreBluetoothSeam.swift` + `CBManagerCentral.swift`/`CBManagerPeripheral.swift` — Core Bluetooth state-restoration handling after the app is suspended/killed by the OS; verify against `MockCoreBluetooth.swift`'s test doubles that every delegate callback path is actually exercised.
- `Services/BGTaskWiring.swift` — background task registration/expiration handling: does the task correctly checkpoint and re-schedule before the OS-imposed time budget expires?
- `identity/KeychainEd25519.swift` — Keychain access-control flags (is the key accessible only when the device is unlocked, and never synced to iCloud Keychain unless explicitly intended?).
- `session/SessionRecovery.swift` — does session recovery after backgrounding correctly re-establish identity without re-running the full handshake (Section 2) unnecessarily, and without accepting a stale/replayed session?
- `Services/LiveActivityController.swift` + `IrisWidgetExtension/IrisLiveActivityWidget.swift` — does the Live Activity ever display stale mesh state after the app updates it, or leak message content into the lock-screen widget beyond what's intended?
- `RustFFI/IrisCore.swift` — memory ownership across the FFI boundary (who frees buffers returned from Rust); cross-check against `ios/Tests/FfiConformanceTests.swift`.
- `Services/IosBleAdapter.swift` — same `Transport`-trait-contract question as Android's adapters (Section 3 boundary): connection-state and error propagation correctness.

**Where to check from:**
```bash
cargo test -p iris-ios                          # Rust FFI bridge
xcodebuild test -project ios/IRIS.xcodeproj      # generated via `xcodegen` from ios/project.yml
bash ios/Scripts/build-xcframework.sh            # verify the xcframework Swift links against builds cleanly
```
`ios/Tests/MockCoreBluetooth.swift` lets Core Bluetooth logic be tested without a physical device; use it before requiring real hardware. Compare against: `docs/platforms/IOS.md`, `docs/implementation/{IOS_DESIGN,SWIFT_LAYER}.md`, `ios/G-IOS_SPIKE.md`. Prior review history: `engineering/memory/records/{IOS-001_DISCOVER,IOS-001_SECURITY_REVIEW,IOS-001_TEST,IOS-001_VERIFICATION}.md`.

**Interfaces/dependencies:**
- **Receives from Section 3:** the `Transport` trait contract that `IosBleAdapter.swift` + `crates/iris-ios/src/ffi/ble_adapter.rs` implement.
- **Receives from Section 1:** the `KeyStore` trait contract that `KeychainEd25519.swift` implements.
- **Receives from Section 2:** the `lib.rs` public API surface via `crates/iris-ios`'s hand-written Swift bindings in `RustFFI/IrisCore.swift`.
- **Sends to Section 6:** `.github/workflows/ios.yml` is owned here specifically (the general `ci.yml` is Section 6's), but any new Cargo dependency in `crates/iris-ios` still needs Section 6's `deny.toml` sign-off.

**Difficulty: Medium–Hard.** Structurally similar risk profile to Android — Core Bluetooth's async delegate/state-restoration model and background-task time budgets are a well-known source of subtle, hard-to-reproduce bugs, and Swift concurrency patterns may be less familiar to a team that hasn't worked in this codebase before.

---

## Section 6 — Desktop App & Build Infrastructure

**Owns:** the Tauri-based desktop app, and the cross-cutting workspace build/dependency/CI policy plus the repo's process/spec documentation that doesn't belong to any single subsystem above.

**Files/folders:**
```
crates/iris-desktop/{Cargo.toml,build.rs,tauri.conf.json,windows-app-manifest.xml}
crates/iris-desktop/src/{commands,engine_handle,identity,lib,main,types}.rs
crates/iris-desktop/tests/{commands_mock,engine_roundtrip}.rs
crates/iris-desktop/{capabilities/default.json,icons/icon.ico}
crates/iris-desktop/ui/{app.js,commands.js,index.html,styles.css,tokens.css}
Cargo.toml, Cargo.lock, deny.toml                    # owned here (shared, see Step 1 table)
.cargo/audit.toml, .config/nextest.toml
.github/workflows/ci.yml                             # owned here (ios.yml is Section 5's)
.gitignore, AGENTS.md, opencode.jsonc, .opencode/**
engineering/**                                       # policy YAML + process/agent memory scaffolding
```
**Read-only reference:** `crates/iris-core/src/identity/{store,chain}.rs` (Section 1, wrapped by `identity.rs` here), `crates/iris-core/src/lib.rs` public surface (Section 2, wrapped by `engine_handle.rs`/`commands.rs` here).

**What to check:**
- `src/commands.rs` — every Tauri IPC command's input validation: can the JS frontend (`ui/app.js`) pass an untrusted value that reaches `iris-core` unchecked?
- `src/identity.rs` — desktop keystore wrapping (OS keychain via Tauri) — same class of check as Android/iOS Sections 4/5's identity files, but for desktop's OS APIs.
- `src/engine_handle.rs` — engine lifecycle (start/stop/restart) from the UI; verify no resource leak on repeated restart from the UI's "reconnect" action.
- `capabilities/default.json` + `tauri.conf.json` — Tauri capability/permission scoping: does the webview have any capability (filesystem, shell) broader than the UI actually needs?
- Root `Cargo.toml` workspace `[workspace.dependencies]` — version pins consistent across all 5 crates; run `cargo deny check` against `deny.toml` for license/advisory violations.
- `.github/workflows/ci.yml` — does CI actually build and test all 5 workspace crates (`iris-core`, `iris-storage`, `iris-desktop`, `iris-android`, `iris-ios`), or has a crate silently fallen out of the matrix?
- `engineering/*.yaml` policy files — are they still consistent with actual practice (e.g. does `SECURITY_POLICY.yaml` still match what Section 1 actually implements), flagging drift rather than rewriting the code they describe.

**Where to check from:**
```bash
cargo test -p iris-desktop
cargo build --workspace && cargo clippy --workspace
cargo deny check                 # against deny.toml
cargo audit                      # against .cargo/audit.toml
cargo nextest run                # against .config/nextest.toml
```
Manual verification: `cargo tauri dev` from `crates/iris-desktop` to exercise the UI end-to-end. Compare against: `docs/platforms/{LINUX,MACOS,WINDOWS,CROSS_PLATFORM,PLATFORM_ARCHITECTURE}.md`, `docs/implementation/{DESKTOP_DESIGN,DESKTOP_VERIFICATION,REPOSITORY_ARCHITECTURE}.md`, `docs/testing/{FUZZING,UNIT_TESTING,TESTING_STRATEGY,SYSTEM_TEST_REPORT}.md`, `docs/decisions/ADR-*.md`, `docs/architecture/*.md` (system-wide — useful background for this section specifically since it touches the whole workspace), top-level `docs/00_PROJECT_CHARTER.md` through `docs/05_GLOSSARY.md`. Prior review history: `engineering/memory/records/{PILOT-001_*,DEC-0002,DEC-0009,DEC-0010,GRAPH-0001,GRAPH-0002,ORCH-0001,RED-0001,RED-0002,TEST-0001,checkpoints/*,discoveries/*,research/*,verification/VER-0001}.md`.

**Note — two docs of unclear provenance:** `docs/implementation/PYTHON_LAYER.md` and `docs/implementation/TYPESCRIPT_LAYER.md` describe layers with no corresponding source in this repo (no Python or TypeScript found anywhere). Flag to the team lead rather than assuming ownership — they may describe a planned/retired component.

**Interfaces/dependencies:**
- **Receives from Section 1:** `identity.rs` wraps the `KeyStore`/`IdentityManager` trait for the desktop OS keychain.
- **Receives from Section 2:** `engine_handle.rs`/`commands.rs`/`types.rs` wrap `lib.rs`'s public API for the Tauri IPC layer.
- **Sends to all sections:** `Cargo.toml`/`deny.toml`/`ci.yml` govern what dependencies any Rust crate (Sections 1–5) may add and whether their tests run in CI — a new dependency anywhere needs this section's sign-off.

**Difficulty: Medium.** The desktop app itself is the smallest application surface in the project (under 1,000 Rust lines plus a thin JS UI), so a junior reviewer can get through it with real confidence — but the build/dependency-policy half of this section has a wide (if shallow) blast radius: a misconfigured CI matrix or an approved-without-review dependency silently weakens every other section's safety net.

---

## Coordination notes — every interface boundary in one place

| # | Boundary | Side A (owns the trait/type) | Side B (implements/consumes it) |
|---|---|---|---|
| 1 | `Transport` trait, `message.rs` types | Section 3 (`transport/mod.rs`) | Section 4 (`crates/iris-android/src/ffi/*_adapter.rs`, `android/.../adapter/*.kt`) |
| 2 | `Transport` trait, `message.rs` types | Section 3 (`transport/mod.rs`) | Section 5 (`crates/iris-ios/src/ffi/ble_adapter.rs`, `ios/IRIS/Services/IosBleAdapter.swift`) |
| 3 | `KeyStore` trait | Section 1 (`identity/store.rs`) | Section 4 (`android/.../identity/KeystoreEd25519.kt`) |
| 4 | `KeyStore` trait | Section 1 (`identity/store.rs`) | Section 5 (`ios/IRIS/identity/KeychainEd25519.swift`) |
| 5 | `KeyStore` trait | Section 1 (`identity/store.rs`) | Section 6 (`crates/iris-desktop/src/identity.rs`) |
| 6 | Crate public API (`lib.rs`) | Section 2 | Sections 4, 5, 6 (each platform's engine-wrapper: `iris-android/src/engine.rs`, `iris-ios/src/engine.rs`, `iris-desktop/src/engine_handle.rs`) — plus Section 4's generated `kotlin/` bindings, which mirror the same surface |
| 7 | `SecurityPolicy` trait | Section 1 (`security/mod.rs`) | Section 2 (`message_engine/*`, `emergency/*` call it to gate messages) |
| 8 | Authority-signature verification | Section 1 (`identity/trust_store.rs`) | Section 2 (`emergency/authority.rs`) |
| 9 | Durable persistence | Section 3 (`crates/iris-storage`) | Section 2 (`message_engine/storage.rs` calls it) |
| 10 | `Envelope`/`RoutingHints`/`message.rs` types | Section 2 (`protocol/`, `message.rs`) | Section 3 (`routing/*` makes forwarding decisions on them) |
| 11 | Kani proof harness (`kani_proofs.rs`) | Section 2 (owns the file) | Section 1 (`rate_limiter`/`quota` proofs), Section 3 (`prophet` proof) — coordinate before editing those specific harnesses |
| 12 | Workspace dependency/CI policy | Section 6 (`Cargo.toml`, `deny.toml`, `ci.yml`) | Sections 1–5 (any new Rust dependency needs sign-off here) |

If any of these 12 boundaries changes shape, the two listed sections should talk before either one merges a fix — that's the only cross-section coordination this split requires.
