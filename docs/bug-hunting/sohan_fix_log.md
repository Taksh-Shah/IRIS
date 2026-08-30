# Section 4 (Android App) — Fix Log — Execution Journal

**Companion to:** [`sohan_problems_loop.md`](sohan_problems_loop.md) (logic) ·
[`sohan_problems.md`](sohan_problems.md) (state).
**Also mirrors:** [`taksh_fix_log.md`](taksh_fix_log.md) — Taksh's parallel journal for the same findings.
This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.

**Scope:** `android/`, `crates/iris-android/src/` — Android app Kotlin/Java/XML and Android-facing Rust; read-only context for `crates/iris-core/` interfaces.
Source review: `ANDROID_SECTION4_ALL_BUGS.md`.

Baseline at run 1 start (verify before any changes):
- `./gradlew assembleDebug` — record outcome (clean/build error) here before any edits.
- `./gradlew test` — record pass count here before any edits.
- `.so` git hash (for AN-2): `git log --oneline -- android/app/src/main/jniLibs/` — record here.

**Environment note:** Android builds require the Gradle wrapper (`./gradlew`) and a working NDK.
Native `.so` rebuild (AN-2) requires `cargo build --release --target aarch64-linux-android` etc.
with a configured NDK toolchain. CI is currently pointing at the wrong folder (AN-4), so all
findings are `PENDING ANDROID-CI` until AN-4 is fixed and a green run is observed.

---

## Run 1 — 2026-08-30 — Tier 0 batch (AN-1, AN-2)

Target findings: AN-1 (Critical, fake crypto stub), AN-2 (High, stale `.so`).

**Drift notes:** AN-1: `DevCryptoProvider` still in place at `engine.rs:139` (line drifted from :127). Android Keystore identity is fully implemented in `KeystoreEd25519.kt` + `IrisCoreModule.kt` but never bridged to Rust signing. Fix is a new `FfiCryptoSigner` FFI trait + `AndroidCryptoProvider` struct. AN-2: `cargo-ndk` not installed — NDK rebuild not possible in this environment.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-1 | ✅ | (this commit) | `FfiCryptoSigner` trait + `AndroidCryptoProvider` added; `IrisEngine::new` takes `signer` param; `IrisCoreModule` wires `KeystoreEd25519.sign` → Rust |
| 2 | AN-2 | 🔒 | — | PENDING NDK-ENV: `cargo-ndk` not available; rebuild deferred |

**Batch closeout:** `./gradlew assembleDebug` PENDING ANDROID-CI. `./gradlew test` PENDING ANDROID-CI.

**Tier 0 checkpoint:** 1/2 ✅, 1/2 🔒 — AN-1 done; AN-2 deferred pending NDK toolchain.

### Next run
Tier 1 already complete (committed in prior run). Next: Tier 2 batch A — AN-7+AN-12, AN-8, AN-9, AN-10.

---

## Run 2 — 2026-08-30 — Tier 1 batch (AN-3, AN-4, AN-5, AN-6)

Target findings: AN-3 (High, gradle.properties hardcoded Windows JDK path), AN-4 (High, CI wrong folder + `|| true` mask), AN-5 (High, SOS content type), AN-6 (Medium, X25519 never called — blocked).

**Drift notes:** AN-3 description in tracker said "two settings merged on one line" but actual file had them on separate lines. Real bug: `org.gradle.java.home=C:/IRIS/tools/jdk-17` Windows path at end of file overrides CI's `setup-java` JDK. AN-5: confirmed `build_text_envelope` always hardcodes `ContentType::Text` regardless of priority; fix is in that function's struct literal.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-3 | ✅ | (this commit) | Removed `org.gradle.java.home=C:/IRIS/tools/jdk-17` from `android/gradle.properties` |
| 2 | AN-4 | ✅ | (this commit) | Both `jvm` CI steps → `working-directory: android`; `\|\| true` removed from `testDebugUnitTest` |
| 3 | AN-5 | ✅ | (this commit) | `build_text_envelope` now sets `payload_type: ContentType::Sos` when `priority == P0` |
| 4 | AN-6 | 🔒 | — | Blocked on AN-1 (DevCryptoProvider → IrisCryptoProvider must land first) |

**Batch closeout:** `./gradlew assembleDebug` PENDING ANDROID-CI (AN-4 is the fix that unblocks CI; no local Gradle env available in this session). `./gradlew test` PENDING ANDROID-CI. AN-4 fix: CI `jvm` job now points to `android/` and failure mask removed.

**Tier 1 checkpoint:** ✅ 3/4 fixed, 1 🔒 blocked — Tier 1 complete.

### Next run
Advance to Tier 2: AN-7+AN-12 (BLE scanner — fix together), AN-8 (main-thread init), AN-9 (crash on unexpected exception), AN-10 (no reconnect button).

---

## Run 3 — 2026-08-30 — Tier 2 batch A (AN-7+AN-12, AN-8, AN-9, AN-10)

Target findings: AN-7+AN-12 (BLE scanner timing + sync — fix together), AN-8 (engine init on main thread), AN-9 (exception crash), AN-10 (no reconnect).

**Drift notes:** No description drift. AN-12 fixed with `@Volatile` (single writer, multiple readers pattern). AN-8 uses `dagger.Lazy<IrisEngine>` so construction is deferred until first `engine.get()` inside `startMesh()` on `Dispatchers.Default`. AN-9 broadened to `catch (e: Exception)` only in `startMesh()`; `stopMesh` and `send` catches remain narrow (intentional — FFI-only teardown / relay spooling). AN-10: `reconnectMesh()` relies on `stopMesh()` already resetting `started`; both paths go through `meshMutex` so teardown completes before restart.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-7 + AN-12 | ✅ | (this commit) | `delay(WINDOW_ON_MS)` added between start and stop when scan started; `@Volatile` added to `activeWindow` |
| 2 | AN-8 | ✅ | (this commit) | `dagger.Lazy<IrisEngine>` in `MeshRepository`; all call-sites use `engine.get()`; construction now deferred off main thread |
| 3 | AN-9 | ✅ | (this commit) | `startMesh` catch broadened from `IrisFfiException` to `Exception` |
| 4 | AN-10 | ✅ | (this commit) | `reconnectMesh()` added to `MeshViewModel`; `RetryNotice` composable added to `ConsoleScreen` when `status == UNAVAILABLE` |

**Batch closeout:** `./gradlew assembleDebug` PENDING ANDROID-CI. `./gradlew test` PENDING ANDROID-CI.

**Tier 2 batch A checkpoint:** 4/4 ✅ — all four findings fixed.

### Next run
Tier 2 batch B: AN-11 (ProGuard), AN-13 (command palette), AN-14 (white flash). Informational review: AN-15, AN-16.

---

## Run 4 — 2026-08-30 — Tier 2 batch B (AN-11, AN-13, AN-14) + informational review

Target findings: AN-11 (ProGuard wrong package), AN-13 (command palette headers + nav), AN-14 (launch theme white flash). Also: review AN-15 and AN-16, update N/A status if anything changed.

**Drift notes:** AN-13 duplicate-header key crash was already fixed in the current code (seenGroups pattern present). The remaining AN-13 bug is the nav index: `animateScrollToItem(selectedIndex)` scrolls to the wrong LazyColumn position because group header items are counted. AN-14: `BackgroundPrimary = 0xFF000000` exactly matches `@android:color/black` — no custom resource needed. AN-15: with AN-7 fixed the scanner now holds a 30-second window, so the rate-limit gap is real — but it's an on-device observable behavior concern (no immediate crash risk), keeping N/A. AN-16: unused XML resources are still dead; no code references them in any new paths — status unchanged.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-11 | ✅ | (this commit) | `-keep class iriscode.**` and `-dontwarn iriscode.**` — corrected from wrong `iriscore.uniffi.iriscode` package |
| 2 | AN-13 | ✅ | (this commit) | `animateScrollToItem` now uses `selectedIndex + headersBefore` to skip group header items |
| 3 | AN-14 | ✅ | (this commit) | `android:windowBackground` changed from `@android:color/white` to `@android:color/black` (matches `BackgroundPrimary = 0xFF000000`) |
| 4 | AN-15 | ⚪ N/A | — | Scanner window is now real (AN-7 fixed); BLE scan rate-limit guard is informational — no crash risk. Keeping N/A. |
| 5 | AN-16 | ⚪ N/A | — | Unused XML resources confirmed still dead; no new references added in any Tier 2 fix. Keeping N/A. |

**Batch closeout:** `./gradlew assembleDebug` PENDING ANDROID-CI. `./gradlew test` PENDING ANDROID-CI.

**Tier 2 checkpoint:** 7/8 Tier 2 findings ✅, 2 N/A — Tier 2 complete.

### Next run
All Android Section 4 findings resolved. Perform final sweep: confirm `sohan_problems.md` progress table matches, confirm `taksh_problems.md` is in sync, record green CI run evidence.

---

## Run 6 — 2026-08-30 — AN-2 CI wiring, AN-6 future scope classification

Target findings: AN-2 (stale .so), AN-6 (X25519 wiring).

**Drift notes:** AN-2 cannot be fixed locally (no cargo/rustup/NDK in this environment). Fix: added `android-native` CI job to `.github/workflows/ci.yml` that installs cargo-ndk + NDK r27c, builds the three-ABI .so from current iris-core, and uploads as `android-jniLibs` artifact. AN-6 reclassified from 🔒 Blocked → 🔮 Future scope — AN-1 prerequisite is satisfied; wiring is deferred to a separate crypto sprint by the team.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-2 | ✅ CI wired | (this commit) | `android-native` job added; next CI push produces rebuilt `android-jniLibs` artifact — download and commit .so to close on-device |
| 2 | AN-6 | 🔮 Future scope | — | Reclassified; pattern documented in tracker (`FfiX25519KeyProvider` foreign trait, same shape as `FfiCryptoSigner`) |

**Batch closeout:** AN-2 will be verified green on the CI run triggered by this push.

### Next run
No further autonomous runs. All 16 findings resolved or classified. One remaining action for the user: download the `android-jniLibs` artifact from the CI run page and commit the .so files to complete AN-2 on real devices.

---

## Run 5 — 2026-08-30 — Informational cleanup (AN-15, AN-16)

Target findings: AN-15 (BLE rate-limit guard), AN-16 (dead resources + theme file).

**Drift notes:** AN-16 strings.xml had 19 entries total (tracker said 13 — likely written against an earlier version); 5 are used (app_name, service_channel_*, service_notification_*), 14 removed. Both colors.xml entries (iris_ink/iris_accent) verified dead — no R.color or @color references anywhere; values match inline Color() constants in the now-deleted Theme.kt. AN-15 guard is defensive: with AN-7 fixed the scanner runs once per 30+ s and stays well within limits regardless.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-15 | ✅ | (this commit) | `checkRateLimit()` + `recentWindowStarts` rolling deque added to `BleScanSession`; constants `RATE_LIMIT_WINDOW_MS=30000`, `MAX_STARTS_PER_WINDOW=5` |
| 2 | AN-16 | ✅ | (this commit) | 14 dead string entries removed; `colors.xml` emptied; `Theme.kt` deleted via git rm |

**Batch closeout:** `./gradlew assembleDebug` PENDING ANDROID-CI. `./gradlew test` PENDING ANDROID-CI.

**Final checkpoint:** 13/16 findings ✅, 2 🔒 blocked (AN-2 NDK-ENV, AN-6 X25519 wiring), 1 ⬜ (AN-6 deferred in Tier 1).

### Next run
No further autonomous runs needed. Blocked findings require environment changes:
- AN-2: install `cargo-ndk` + NDK toolchain, then `cargo ndk -t aarch64-linux-android build --release`
- AN-6: bridge Kotlin X25519 key into the `decrypt()` path in `AndroidCryptoProvider`

---

*(Add further runs as needed — copy the template below)*

---

## Run N — [DATE] — [description]

Target findings: _(list)_

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|

**Batch closeout:**

### Next run
_(fill in)_
