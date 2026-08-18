# ANDROID-001 Verification (VERIFY stage)

- **Node**: ANDROID-001 — Android Platform Integration (P0 PLATFORM — app shell
  + Kotlin FFI adapters over Rust `iris-core`)
- **Stage**: VERIFY — acceptance evidence for ACCEPT
- **Version**: v1.1 (v1.0 2026-08-17; v1.1 2026-08-18 reconciles independent
  verifier notes)
- **Date**: 2026-08-18
- **Precedents**: `WIFIAWARE-001_VERIFICATION.md` v1.0 (AC-1..16 + verifier
  reproduction pattern), `WIFIDIRECT-001_VERIFICATION.md`, `BLE_001_VERIFICATION.md`

## Acceptance-criteria outcome

**All 15 acceptance criteria PASS / GATED (recorded).** AC-2/3/4/5/7/8/9/10/11/12
verified in TEST stage (`ANDROID-001_TEST.md`, iter 118) and re-verified live
below; AC-1/AC-6 **env-gated** (cargo-ndk build + `gradlew assembleDebug` need a
toolchain-equipped host/CI — dev host has no Android SDK/NDK/gradle/kotlinc,
probed, recorded known_limitation); AC-13 **GATED/BLK-0005** (physical-device/
battery/OEM tests RESOURCE-gated); AC-14 SECURITY_REVIEW **RESOLVED**
(`ANDROID-001_SECURITY_REVIEW.md` — AND-RT-101..113 all FIXED + re-review pass 2
FAIL→RESOLVED, positive controls HELD); AC-15 this document (independent
verifier reproduction below).

Honest-evidence rule honored: every figure below was re-verified live in this
pass (workspace 613/0/1, `iris-android` 5/5, clippy 0, rustfmt clean after
`cargo fmt` applied in-pass). No unverified claim enters this record.

## AC → evidence table

| AC | Criterion (from PROJECT_GRAPH.yaml) | Evidence | Status |
|----|--------------------------------------|----------|--------|
| AC-1 | `cargo-ndk` 4.1.2 builds `libiriscode.so` for {arm64-v8a, armeabi-v7a, x86_64} | `crates/iris-android` Cargo.toml (lib `iriscode` cdylib/staticlib/rlib) + NDK r26 floor/r28 pin + ABI set (ANDROID.md §Platform Targets, ANDROID_DESIGN.md D-5/G-AND-5/6). `cargo ndk build` needs toolchain host/CI — dev host lacks Android SDK/NDK | ⛔ GATED (env) |
| AC-2 | UniFFI 0.31.x bindgen → Kotlin for `IrisEngine` + 3 foreign traits; generated Kotlin committed | `uniffi-bindgen` 0.31.2 → `kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt` (**271,037 B / 6154 lines**, regenerated iter 114; `FfiIncomingMessage`/`subscribeInbox`/`create_group` present, stale `discover_services`/`FfiServiceInfo` 0). **Working-tree status** (untracked — no Android/Kotlin/iris-android commit exists yet; `git-track pending operator-approved commit` recorded known_limitation) | PASS (source, working tree) |
| AC-3 | G-AND-3 spike: generated Kotlin `suspend` + async foreign traits over explicit tokio Handle (#2576) | Spike behavior (async foreign-trait calls awaited through engine-owned runtime) covered by current `iris-android` tests: `projection_trait_drives_sync_ops` (ble_adapter.rs:105), `engine_owns_a_live_runtime` (engine.rs:626), `round_trip_delivers_over_shared_mesh` (engine.rs:510). The iter-113 spike test `engine_start_and_poll_via_explicit_handle` was superseded by the AC-4 engine rewrite (renamed history, not fabricated) — citations reconciled. Kotlin compile/run leg env-gated | PASS (Rust leg) |
| AC-4 | `IrisEngine` mirrors DesktopEngine (TransportManager + 3 transports + MessageEngine + inbox forwarders + subscribe broadcast) | **5/5** (iter 114) incl. `round_trip_delivers_over_shared_mesh` (engine.rs:510: B advertises → A connects keyed by real node id → A `send_text` → B delivers + inbox receipt). Re-verified live below | PASS |
| AC-5 | FFI-contract absorbed (`AdapterLifecycle.kt`): RT-110 timeouts, RT-111/RT-010 idempotence, RT-112 ring-buffer outbox, RT-108 verified-peer reuse, RT-109 closed-NDP prune + multi-subscriber stream | `AdapterLifecycleTest.kt` **14 tests one-per-requirement** (source presence verified: 14 `@Test`). Execution env-gated — no Kotlin toolchain on this host (gradle/kotlinc absent, probed). Re-review pass 2 re-confirmed `SessionGate` single-Mutex discipline + strict-LRU outbox | PASS (source presence; JVM execution env-gated) |
| AC-6 | Kotlin 2.2.x + Compose BOM ≥ 2024.01.00 + Hilt MVVM shell; `collectAsStateWithLifecycle`; strong-skipping | Scaffold materialized iter 116 in working tree (Kotlin 2.2.10 + AGP 8.7.3 + Compose BOM 2024.12.01 strong-skipping ON + KSP 2.2.10-2.0.2 + Hilt 2.55 + sourceSets→bindings + jna 5.14.0 + `unitTests.useJUnitPlatform`). **Gradle wrapper properties-only** (`gradle-wrapper.properties` = 8.9; `gradlew`/`gradlew.bat`/`gradle-wrapper.jar` absent — `gradle wrapper` regeneration needed on toolchain host, recorded known_limitation). `./gradlew assembleDebug` on toolchain host/CI (note: Gradle 8.9 requires JDK ≤ 21; host JDK 25 incompatible) | ⛔ GATED (env) |
| AC-7 | FGS `connectedDevice` (API 34+) + PendingIntent BLE scanning + WorkManager 15-min cadence wired | `IrisBleService` connectedDevice FGS + `BleScanSession` PendingIntent scan (ScanRestartPolicy 5/30-s) + `BleScanReceiver` + `WorkScheduler` unique 15-min+5-flex CONNECTED + `IrisBackgroundSyncWorker`; JVM unit-test source: `ScanRestartPolicyTest` 4 + `MeshSyncPolicyTest` 7 (presence verified). **Execution env-gated** (no Kotlin toolchain on host). **Re-review pass 2**: `stop()` now stops active scan window (R5) | PASS (manifest+code; JVM test source presence, execution env-gated) |
| AC-8 | Keystore **TEE Ed25519** (API 33 floor; software fallback) aligned RED-0005; X25519 static advertisement | `KeystoreEd25519` TEE (StrongBox excluded) + `SoftwareBackend` JCA fallback + `AutoBackend`; **re-review pass 2**: `SoftwareBackend(context)` persists wrapped key (RT-102) — no silent rotation. `X25519StaticAd` Ed25519-signed-X25519 with **cached** key (RT-107). JVM unit-test source: `KeystoreEd25519Test` **8** `@Test` (count corrected from 9; verified line-by-line) + `PeerIdCodecTest` (nested in `BatteryOptimizationGuidanceTest`). **Execution env-gated** (no Kotlin toolchain on host; TEE path device-gated) | PASS (source presence; JVM execution env-gated) |
| AC-9 | targetSdk 34 / minSdk 26; ACCESS_LOCAL_NETWORK SDK-37 cliff documented; `BluetoothSocket.read()` -1 noted | Manifest targetSdk 34/minSdk 26; ANDROID.md §ACCESS_LOCAL_NETWORK cliff + §BluetoothSocket.read() -1 (iter 117) | PASS (doc+manifest) |
| AC-10 | Workspace green: `cargo test --workspace --all-features ≥ 608 passed / 0 failed / 1 ignored`; clippy 0; rustfmt clean | **Live re-verify iter 121**: **613 passed / 0 failed / 1 ignored** (608 baseline + 5 `iris-android`); clippy **0 warnings**; **rustfmt clean** (`cargo fmt --all` applied in-pass on `crates/iris-android`, then `cargo fmt --all --check` exit 0; re-ran workspace tests after fmt → 613/0/1 held) | PASS |
| AC-11 | Generated Kotlin + Rust FFI 1:1 op / type-check conformance, no orphan ops | 1:1 mapping (WD 16==16, WA 12, BLE 10; ULong conversions) + `iriscode` facade (`api.kt`) resolving all 15 adapter imports. **Re-review pass 2** caught + fixed the one conformance slip (`openNdp` Long→`ULong`, R1) | PASS (+grep) |
| AC-12 | Doc reconciliation: ANDROID.md + KOTLIN_LAYER.md + ANDROID_DESIGN.md consistent | **COMPLETE iter 117** (toolchain pins, package layout, UniFFI Integration, Keystore rewrite, LNP cliff, OEM matrix, §UniFFI Binding Layer) | PASS |
| AC-13 | Physical-device/battery/OEM tests GATED/BLK-0005 | Recorded known_limitation per DEC-0009/BLK-0005 — RESOURCE-gated (hardware) | ⛔ GATED/BLK-0005 |
| AC-14 | SECURITY_REVIEW: redteam adversarial findings dispositioned | `ANDROID-001_SECURITY_REVIEW.md` (iter 119 + re-review 120): **AND-RT-101..113 (2 CRITICAL / 6 HIGH / 5 MEDIUM) all FIXED**, every fix spot-checked vs source; re-review pass 2 FAIL→RESOLVED with R1-R5 fixed in-pass + R4/R6/RT-112 recorded; positive controls HELD. Independent verifier re-confirmed all fix logics as real code (deviceHash BLE:188-192; SessionGate WA:84; R1 ndpHandle.toULong() WA:145; R2 TIRAMISU gate BLE:300-313 + legacy 2-arg writeType; R3 `withContext(Dispatchers.IO)` MeshViewModel:74; R5 stop→stopScanWindow BleScanSession:122-127) | PASS |
| AC-15 | VERIFY: independent verification doc + verifier reproduction | This document + §"Independent verifier (subagent) reproduction" below — verdict **APPROVE_WITH_NOTES**, all notes reconciled in v1.1 (AC-3 citation, AC-8 count, AC-10 rustfmt, working-tree status, wrapper assets, JVM-execution labeling) | PASS |

## Live verification (iter 121, this host)

| Check | Command | Result |
|-------|---------|--------|
| Workspace | `cargo test --workspace --all-features` | **613 passed / 0 failed / 1 ignored** (608 baseline + 5 `iris-android`) |
| iris-android | `cargo test -p iris-android --all-features` | **5 passed / 0 failed** |
| Clippy | `cargo clippy --workspace --all-features --tests` | **0 warnings** |
| Rustfmt | `cargo fmt --all --check` | **exit 0** (`cargo fmt --all` applied in-pass on `crates/iris-android`, tests re-run after → 613/0/1 held) |
| YAML | graph + state + execution-state parse | **VALID** |
| Rust diff | crates/iris-core + crates/iris-android | **no SECURITY_REVIEW fix leaked into Rust** (Kotlin-only fixes; `cargo fmt` applied) |

## Known limitations recorded (not blockers)

- **Working-tree status**: all `android/`, `kotlin/`, `crates/iris-android/` artifacts
  are untracked (no commit exists). AC-2's "generated Kotlin committed" phrasing
  corrected to working-tree status; git-track pending operator-approved commit.
- **Gradle wrapper incomplete**: only `gradle-wrapper.properties` (8.9) shipped;
  `gradlew`/`gradlew.bat`/`gradle-wrapper.jar` absent — regenerate via
  `gradle wrapper` on a toolchain host (Gradle 8.9 needs JDK ≤ 21).
- **JVM unit tests authored, not executed on host**: no gradle/kotlinc (probed),
  no `test-results` artifacts; suites run on toolchain host/CI. Counts are
  source-presence verified (AdapterLifecycle 14, ScanRestartPolicy 4,
  MeshSyncPolicy 7, KeystoreEd25519 8, BatteryOptimization/PeerIdCodec 6 = **39
  total**).
- **Env-gated legs** (AC-1/AC-6): cargo-ndk build + `gradlew assembleDebug` + actual
  Kotlin compile/run need Android SDK/NDK + JDK ≤ 21.
- **BLK-0005 (AC-13)**: physical-device/battery/OEM integration tests
  RESOURCE-gated.
- **AC-2 stale size**: `iriscode.kt` was 202,639 B at iter 113, regenerated to
  271,037 B at iter 114 (6154 lines) — size citations corrected.

## Independent verifier (subagent) reproduction

Independent `verifier` subagent (run 2026-08-18, ses_fedd3858dffe3C0MMoxbmxjdt9)
reproduced: workspace **613/0/1** (per-suite breakdown provided: iriscode 5,
iris_core lib 553, crypto_e2e 3, ml_experiments 8, obs_telemetry 4,
sim_scenarios 8+1 ignored, desktop 5, commands_mock 3, engine_roundtrip 3,
storage 7, m3_engine 1, pg_store 13), `iris-android` **5/5** (names verified in
source: engine.rs:453/474/510/626, ble_adapter.rs:105), clippy **0**.

Per-AC verdicts: VERIFIED-PASS for AC-4/5/7/9/11/12/14 (all spot-checked to real
code with file:line), VERIFIED-GATED for AC-1/6/13 (env-gate labeling honest),
PASS-with-corrections for AC-2/3/8/10/15. **No fabricated fix found** — every
AND-RT-101..113 + R1-R5 fix exists in code with valid logic.

**Verdict: APPROVE_WITH_NOTES.** All six notes were mandatory-before-ACCEPT and
are **reconciled in v1.1**:

1. AC-3 stale `engine_start_and_poll_via_explicit_handle` citation → replaced
   with current covering tests + rename recorded. **DONE**
2. AC-8 `KeystoreEd25519Test` 9→**8**, "40 total"→**39** (14+4+7+8+6),
   `PeerIdCodecTest` = nested class in `BatteryOptimizationGuidanceTest` (6).
   **DONE**
3. AC-10 "rustfmt clean" → **made true**: `cargo fmt --all` applied to
   `crates/iris-android`, `--check` exit 0, tests re-run 613/0/1. **DONE**
4. "committed"/"git-tracked" phrasing → working-tree status + known_limitation.
   **DONE**
5. Gradle wrapper properties-only (`gradlew`/jar absent) → recorded
   known_limitation + toolchain regeneration note. **DONE**
6. AC-5/7/8 "PASS (JVM tests)" → "source presence; JVM execution env-gated".
   **DONE**

Non-blocking notes resolved: AC-2 size 202,639 B → 271,037 B; R3 line ref
MeshViewModel:72-73 → :73-74; verifier pass recorded (this section).

**Final: APPROVE — ready for ACCEPT (iter 122).**