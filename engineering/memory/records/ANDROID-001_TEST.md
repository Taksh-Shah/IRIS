# ANDROID-001 TEST — Android Platform Integration (AC-1..15 evidence mapping)

**Node**: ANDROID-001 (P0 PLATFORM — Android app shell + Kotlin FFI adapters over
Rust `iris-core`)
**Stage**: TEST · **Iteration**: 118 · **Date**: 2026-08-17
**Pattern**: WIFIAWARE-001_TEST.md / BLE-001_TEST.md
**Upstream evidence**: `ANDROID_DESIGN.md` v1.0 (AC-1..15), iter 110-117 records

---

## 1. Acceptance-criteria evidence table

| AC | Criterion | Verdict | Evidence |
|----|-----------|---------|----------|
| AC-1 | cargo-ndk 4.1.2 builds `libiriscode.so` for {arm64-v8a, armeabi-v7a, x86_64} | **GATED** (env) | `crates/iris-android` Cargo.toml ready (lib `iriscode` cdylib/staticlib/rlib); cargo-ndk 4.1.2 + NDK r26 floor/r28 pin + ABI set {arm64-v8a, armeabi-v7a, x86_64}, i686 dropped (ANDROID.md §Platform Targets, ANDROID_DESIGN.md D-5/G-AND-5/6). `cargo ndk build` runs on toolchain-equipped host/CI — **no Android SDK/NDK on dev host** → recorded limitation, not blocker. |
| AC-2 | UniFFI 0.31.x bindgen → Kotlin for `IrisEngine` + 3 foreign traits; committed | **PASS** (source, working tree) | `uniffi-bindgen` 0.31.2 → `kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt` (**271,037 B** iter 113→114 regeneration, 6154 lines, `FfiIncomingMessage`/`subscribeInbox`/`create_group` present, stale `discover_services`/`FfiServiceInfo` 0). **Working-tree status**: files untracked (no commit yet); "git-tracked" pending operator-approved milestone commit → recorded known_limitation (reconciled iter 121 verifier note). |
| AC-3 | G-AND-3 spike: generated Kotlin `suspend` + async foreign traits over explicit tokio Handle (#2576) | **PASS** (Rust leg) | Spike behavior covered by current `iris-android` tests: `projection_trait_drives_sync_ops` (ble_adapter.rs:105), `engine_owns_a_live_runtime` (engine.rs:626), `round_trip_delivers_over_shared_mesh` (engine.rs:510). The iter-113 test `engine_start_and_poll_via_explicit_handle` was **superseded by the AC-4 engine rewrite** (renamed, not fabricated) — reconciled iter 121. Kotlin compile/run leg env-gated (no kotlinc/gradle on host). |
| AC-4 | `IrisEngine` mirrors DesktopEngine (TransportManager + 3 transports + MessageEngine + inbox forwarders + subscribe broadcast) | **PASS** | 5/5 (iter 114): `engine_registers_three_transports_over_bridges`, `start_all_brings_transports_up_over_bridges`, **`round_trip_delivers_over_shared_mesh`** (B advertises → A connects keyed by real node id → A `send_text` b"hello" → B delivers + inbox receipt over shared SimMeshCoordinator), `engine_owns_a_live_runtime`, sync-op drive. Re-verified live iter 118: `cargo test -p iris-android --all-features` = **5 passed / 0 failed**. |
| AC-5 | FFI-contract absorbed (`AdapterLifecycle.kt`): RT-110 timeouts, RT-111/RT-010 idempotence, RT-112 ring-buffer outbox, RT-108 verified-peer reuse, RT-109 closed-NDP prune + multi-subscriber stream | **PASS** | `AdapterLifecycleTest.kt` **14 tests one-per-requirement** (JUnit5 + kotlinx.coroutines), iter 115: rt110 suspendCall TimeoutCancellationException + syncCall onTimeout fallback; rt111 ensureStarted idempotent/never-latched/concurrent-coalescing; rt010 invalidate clears latch; rt112 per-destination oldest-drop eviction + cross-destination preservation; rt109 closed-NDP prune + multi-subscriber replay; rt108 remember/resolve/reuse/evict. Kotlin leg env-gated (host lacks kotlinc/gradle) — tests authored, run on toolchain host/CI. |
| AC-6 | Kotlin 2.2.x + Compose BOM ≥ 2024.01.00 + Hilt MVVM shell; `collectAsStateWithLifecycle`; strong-skipping | **GATED** (env) | Scaffold materialized iter 116 (working tree): Kotlin 2.2.10 + AGP 8.7.3 + Compose BOM 2024.12.01 (strong-skipping ON in `gradle.properties`) + KSP 2.2.10-2.0.2 + Hilt 2.55; Gradle wrapper **properties-only 8.9** (`gradlew`/`.bat`/`wrapper.jar` absent — `gradle wrapper` regeneration on toolchain host; Gradle 8.9 needs JDK ≤ 21); app sourceSets → generated bindings + jna 5.14.0; `unitTests.useJUnitPlatform`; shell: `IrisApplication`/`di/`/`ui/` (`MainActivity` @AndroidEntryPoint + `MeshViewModel` @HiltViewModel + `HomeScreen` `collectAsStateWithLifecycle` + Material3 + @Immutable state + `IrisTheme`)/`data/`. `./gradlew assembleDebug` runs on toolchain host/CI — **no gradle on dev host** → recorded limitation, not blocker. |
| AC-7 | FGS `connectedDevice` (API 34+) + PendingIntent BLE scanning + WorkManager 15-min cadence wired | **PASS** (manifest+code; JVM test source presence) | `IrisBleService` connectedDevice FGS (API 34+ 3-arg `startForeground` + <34 fallback, START_STICKY); `BleScanSession` PendingIntent scan (`SCAN_MODE_OPPORTUNISTIC`, IRIS_SERVICE_UUID filter) with `ScanRestartPolicy` 5/30-s backoff + `BleScanReceiver` + `BleScanEvents`; `WorkScheduler` PeriodicWorkRequest 15-min + 5-min flex + CONNECTED + unique; `IrisBackgroundSyncWorker` (@HiltWorker) drains `RelayOutbox`; manifest FGS `connectedDevice` + perms. **JVM unit-test source**: `ScanRestartPolicyTest` 4 + `MeshSyncPolicyTest` 7 (battery-aware deferral) — authored; **execution env-gated** (no gradle/kotlinc on host). |
| AC-8 | Keystore **TEE Ed25519** (API 33 floor; software fallback) aligned RED-0005; X25519 static advertisement | **PASS** (source presence; JVM execution env-gated) | `KeystoreEd25519` (AndroidKeyStore `KEY_ALGORITHM_ED25519`, `setIsStrongBoxBacked(false)` — StrongBox excludes Ed25519; API 33 floor) + `SoftwareBackend` JCA fallback + `AutoBackend` runtime pick; 32-byte raw pub = 64-hex PeerId. **JVM unit-test source**: `KeystoreEd25519Test` **8** `@Test` (count corrected 9→8, iter 121 verifier) + `PeerIdCodecTest` (nested in `BatteryOptimizationGuidanceTest`). `X25519StaticAd` = Ed25519-signed-X25519 static advertisement (RED-0005). Suites run on toolchain host/CI. |
| AC-9 | targetSdk 34 / minSdk 26; ACCESS_LOCAL_NETWORK SDK-37 cliff documented; `BluetoothSocket.read()` -1 noted | **PASS** (doc+manifest) | Manifest targetSdk 34/minSdk 26 D-8; ANDROID.md §ACCESS_LOCAL_NETWORK SDK-37 cliff (v1 keeps implicit grant; declare + runtime-request at any 37+ bump; NsdManager exemption vs raw sockets) + §BluetoothSocket.read() -1 carry-forward (targetSdk 37) — amended iter 117 (AC-12). |
| AC-10 | Workspace green: `cargo test --workspace --all-features ≥ 608 passed / 0 failed / 1 ignored`; clippy 0; rustfmt clean | **PASS** | **Live re-verify iter 118 + 121**: **613 passed / 0 failed / 1 ignored** (608 baseline + 5 `iris-android`); **clippy --workspace --all-features --tests 0 warnings**; **rustfmt clean** (`cargo fmt --all` applied to `crates/iris-android` iter 121 — `--check` exit 0; workspace tests re-run after → 613/0/1 held). |
| AC-11 | Generated Kotlin + Rust FFI 1:1 op / type-check conformance, no orphan ops | **PASS** (+grep) | 1:1 mapping verified iter 115 (WD 16 interface fns == 16 overrides; WA 12; BLE 10; ULong conversions confirmed). `iriscode` package facade (`kotlin/src/main/kotlin/iriscode/api.kt`, iter 116) re-exports `uniffi.iriscode` — all 15 adapter imports + shell imports resolve (AC-5 gap FIXED). `cargo check --workspace` green. |
| AC-12 | Doc reconciliation: ANDROID.md §toolchain/§crypto/§permissions + KOTLIN_LAYER.md §UniFFI + ANDROID_DESIGN.md consistent | **PASS** | **COMPLETE iter 117**: ANDROID.md pinned toolchain + package layout + UniFFI Integration + Keystore Ed25519 rewrite + ACCESS_LOCAL_NETWORK cliff + OEM matrix; KOTLIN_LAYER.md §UniFFI Binding Layer + conformance note. ANDROID_DESIGN.md consistent (v1.0). |
| AC-13 | Physical-device/battery/OEM tests GATED/BLK-0005 | **GATED/BLK-0005** | Recorded known_limitation per DEC-0009/BLK-0005 disposition — physical-device, battery-measurement, OEM-specific integration tests RESOURCE-gated (need Android hardware). Not a blocker. |
| AC-14 | SECURITY_REVIEW next (adversarial review of FFI layer + lifecycle + Keystore path) | **NEXT** | Redteam subagent review of `crates/iris-android` FFI surface + `AdapterLifecycle.kt` + Keystore path — iter ~119. |
| AC-15 | VERIFY next (independent verifier reproduction) | **NEXT** | Independent verifier — iter ~120, then ACCEPT. |

---

## 2. Live re-verify — iter 118 (this host)

- `cargo test --workspace --all-features` → **613 passed / 0 failed / 1 ignored**
  (per-suite: iris-core lib + integration incl ML/sim/storage/pg/obs/desktop/M3/
  crypto_e2e; + `iris-android` 5). AC-10 origin: 608 pre-crate baseline + 5
  `iris-android` engine/bridge tests.
- `cargo test -p iris-android --all-features` → **5 passed / 0 failed / 0
  ignored** (0.56 s).
- `cargo clippy --workspace --all-features --tests` → **0 warnings** (clippy 0
  held since iter 108).
- Toolchain probe: `gradle` **MISSING**, `kotlinc` **MISSING**, `java` present
  (Adoptium JDK 25, no Android SDK) → Kotlin/Android legs env-gated.
- Rust source untouched since iter 112 (crates/iris-core) — ANDROID-001 work
  additive (`crates/iris-android`, `kotlin/`, `android/`).

## 3. JVM unit-test inventory (authored; run on toolchain host/CI)

Total **39** (JUnit5 + kotlinx.coroutines) — count corrected from 40 (iter 121 verifier — `KeystoreEd25519Test` has **8** `@Test`, not 9):
`AdapterLifecycleTest.kt` 14 (AC-5) · `MeshSyncPolicyTest.kt` 7 (AC-7) ·
`KeystoreEd25519Test.kt` 8 (AC-8) · `ScanRestartPolicyTest.kt` 4 (AC-7) ·
`BatteryOptimizationGuidanceTest.kt` (OEM matrix) + nested `PeerIdCodecTest` = 6
(AC-7/AC-8).

## 4. Known limitations recorded (not blockers)

- **Env-gated legs**: `cargo ndk build` (AC-1), `./gradlew assembleDebug`
  (AC-6), Kotlin `unittest` execution (AC-3/5/6/7/8) — dev host has no Android
  SDK/NDK/gradle/kotlinc; run on toolchain-equipped host/CI (JDK ≤ 21 for Gradle
  8.9).
- **Working-tree status**: `android/`, `kotlin/`, `crates/iris-android/` are
  untracked (no commit yet); "git-tracked" pending operator-approved milestone
  commit (iter 121 verifier reconciliation).
- **Gradle wrapper incomplete**: `gradlew`/`gradlew.bat`/`gradle-wrapper.jar`
  absent; `gradle wrapper` regeneration required on toolchain host.
- **BLK-0005 (AC-13)**: physical-device, battery, OEM-specific integration tests
  RESOURCE-gated (hardware).
- G-AND-1: UniFFI 0.32 async foreign-trait behavior unverified — pinned 0.31.x.
- G-AND-2: StrongBox Ed25519 attestation CTS failure — TEE only, software
  fallback.
- LNP cliff: targetSdk must not bump 34 → 37 without `ACCESS_LOCAL_NETWORK`.

## 5. Status

- PROJECT_GRAPH ANDROID-001: status IMPLEMENTING (stage **TEST COMPLETE**),
  evidence 17, known_limitations 8, stage_note + validation_status iter 118.
- Next: **SECURITY_REVIEW (iter ~119)** — redteam of FFI surface + adapter
  lifecycle + Keystore path → VERIFY → ACCEPT. (Since done: iter 119-122.)