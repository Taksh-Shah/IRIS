# Section 4 (Android App) — Bug-Hunt Problem Tracker

**Reviewer:** Sohan (Section 4 — Android App)
**Source review:** `ANDROID_SECTION4_ALL_BUGS.md` — the full 16-finding report authored by Taksh.
**Scope:** `crates/iris-android/src/`, `android/app/src/main/`, `android/gradle.properties`, `.github/workflows/ci.yml`; read-only context for `crates/iris-core/` interfaces and Desktop counterparts.
**Companion documents:**
- [`sohan_fix_log.md`](sohan_fix_log.md) — execution journal (what each run did, evidence, commits)
- [`sohan_problems_loop.md`](sohan_problems_loop.md) — autonomous loop spec (how to execute this tracker)
- **Cross-reference:** [`taksh_problems.md`](taksh_problems.md) — Taksh's parallel tracker for the same findings; both files must be updated on every status change.

**Baseline (review start):** Android module builds locally on the reviewer's machine. No automated proof that Android tests compile or pass — CI is pointing at the wrong folder (AN-4). Manual inspection and static analysis only.

**Build note:** Android builds require the Gradle wrapper (`./gradlew`) and a working NDK. The native `.so` rebuild (AN-2) requires `cargo build --release --target aarch64-linux-android` (and x86_64/arm variants) with the NDK toolchain. The CI gap (AN-4) means no machine-verified baseline exists; fixes marked `PENDING ANDROID-CI` until the CI step is corrected and a green run is observed.

---

## Progress Tracker

| Tier | Name | Total | ✅ Fixed | 🔒 Blocked | ❌ Reverted | ⬜ Not started |
|---|---|---|---|---|---|---|
| 0 | Critical/High safety + security | 2 | 0 | 0 | 0 | 2 |
| 1 | High correctness + Medium safety/protocol | 4 | 3 | 1 | 0 | 0 |
| 2 | Medium/Low correctness, UX, build | 8 | 0 | 0 | 0 | 8 |
| N/A | Informational / cleanup (track only) | 2 | 0 | 0 | 0 | 2 |
| **Total** | | **16** | **3** | **1** | **0** | **12** |

**Last updated:** 2026-08-30 · **Active tier:** 1 (Tier 1 complete — 3 ✅, 1 🔒)

---

## Tier membership (authoritative)

**Tier 0** (2 findings — fix before anything else; live safety/security defects):
AN-1, AN-2

**Tier 1** (4 findings — high correctness + medium security/protocol; no human gate after Tier 0):
AN-3, AN-4, AN-5, AN-6

**Tier 2** (8 findings — medium/low correctness, UX, build polish; no human gate after Tier 1):
AN-7, AN-8, AN-9, AN-10, AN-11, AN-12, AN-13, AN-14

**Not applicable** (2 findings — informational/cleanup; track awareness only, no fix required):
AN-15, AN-16

**Total: 2 + 4 + 8 + 2 = 16 findings.**

---

## Tier 0 — Critical + High safety/security

---

### AN-1 — Android engine uses a fake, always-valid crypto stub instead of real encryption

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-android/src/engine.rs:127`
- **Category:** safety · **Severity:** Critical
- **Tier:** 0
- **Original report:** Bug #1 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The Android engine is initialized with a placeholder crypto module that the codebase itself labels "must not be used in production." Every outgoing message gets a static, fixed signature (no actual signing). Every incoming message's signature verification always returns `true` regardless of the actual bytes. No encryption is applied.

**Evidence:** `engine.rs:127` shows construction with the stub provider. Desktop uses `IrisCryptoProvider` (real Ed25519 signing + X25519 key agreement). Android was never switched over.

**Root cause:** Partial port — the Desktop implementation was completed and the Android port left with the development stub.

**Fix:** Replace the stub provider with `IrisCryptoProvider` at the Android engine construction site, the same way `iris-desktop/src/engine_handle.rs` already does.

**Dependencies / blast radius:**
- AN-6 (X25519 key exchange dead code) is likely the missing wiring — fix together if possible; at minimum fix AN-1 first so the engine uses real signing, then AN-6 to wire key agreement.
- Requires the `IrisCryptoProvider` to compile on the Android target (`aarch64-linux-android`). Confirm NDK linking before merging.
- `PENDING ANDROID-CI` — must be re-verified with a real device run after AN-4 is fixed and CI is green.

---

### AN-2 — Pre-built native library predates a critical relay-crash fix

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/jniLibs/*/libiriscode.so`
- **Category:** safety · **Severity:** High
- **Tier:** 0
- **Original report:** Bug #2 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The `.so` binary checked into the repo was built before a same-day fix for a bug that could permanently crash a device's ability to relay messages (single malformed packet → silent relay death). Nothing has triggered a rebuild since that fix landed.

**Evidence:** Git blame on the `.so` files shows a timestamp that predates the relay-crash fix commit in the core crate.

**Root cause:** The native library is not rebuilt automatically on core changes. It was manually checked in and never updated.

**Fix:** Rebuild all ABI variants (`armeabi-v7a`, `arm64-v8a`, `x86`, `x86_64`) from current source using the NDK toolchain, replace the checked-in `.so` files, and add a CI step (or a README gate) that fails if the `.so` hash doesn't match a freshly-built artifact.

**Dependencies / blast radius:**
- Must happen before any device testing. Phones running the current `.so` may experience the relay-death bug on a crafted packet.
- Ideally blocked until AN-1 is fixed so the freshly-built `.so` already has real crypto.
- `PENDING ANDROID-CI` — confirm build pipeline can reproduce the `.so` deterministically.

---

## Tier 1 — High correctness + Medium safety/protocol

---

### AN-3 — Merged `gradle.properties` line silently drops a JDK version setting

- **Fix status:** ✅ Fixed — removed `org.gradle.java.home=C:/IRIS/tools/jdk-17` (Windows-specific hardcoded path overriding CI's `setup-java`)
- **File(s):** `android/gradle.properties:16`
- **Category:** correctness · **Severity:** High
- **Tier:** 1
- **Original report:** Bug #3 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** Two separate settings were merged onto a single line (missing newline between them). The key-value parser reads the second setting's key as part of the first setting's value, so the JDK version directive is silently ignored. The build "works" on machines that already have the expected JDK configured, but fails unpredictably on any other machine.

**Evidence:** `gradle.properties:16` — two `key=value` entries on one line. Gradle's properties parser is newline-delimited; only the first key is recognized.

**Root cause:** Missing newline — likely introduced by a merge conflict resolution or manual edit.

**Fix:** Split the line into two separate lines.

**Dependencies / blast radius:** None. One-character change. Safe to fix independently.

---

### AN-4 — CI Android test step points at a Rust folder; Android tests never run

- **Fix status:** ✅ Fixed — both `jvm` job steps changed to `working-directory: android`; removed `|| true` from `testDebugUnitTest`
- **File(s):** `.github/workflows/ci.yml:227–258`
- **Category:** correctness · **Severity:** High
- **Tier:** 1
- **Original report:** Bug #4 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The CI step meant to run Android tests points at a Rust source folder. No Android tests are in that folder, so the step silently succeeds without running anything. The `continue-on-error: true` flag would hide real failures even if the folder were correct.

**Evidence:** `ci.yml:227` — `working-directory: crates/iris-android/src` (Rust). Android test files live under `android/`. The step completes with exit 0 because Gradle finds nothing to run.

**Root cause:** Wrong path set when the step was written; masked by `continue-on-error`.

**Fix:** Change `working-directory` to `android/` and remove `continue-on-error: true`. This CI file is owned by the CI/build-pipeline section — flag to that owner, but also fix it directly since it's specifically Section 4's tests that are affected.

**Dependencies / blast radius:**
- This file is shared infrastructure. Coordinate with the CI owner before merging to avoid conflicting changes.
- Once fixed, all other AN-* findings should be verified against a green CI run.

---

### AN-5 — SOS messages are typed as plain Text, not as SOS, at the network level

- **Fix status:** ✅ Fixed — `build_text_envelope` in `engine.rs` now sets `payload_type: ContentType::Sos` when `priority == MessagePriority::P0`
- **File(s):** `crates/iris-android/src/engine.rs`, `crates/iris-desktop/src/engine_handle.rs`
- **Category:** protocol · **Severity:** High
- **Tier:** 1
- **Original report:** Bug #5 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** When `/sos` is invoked, the message is correctly given P0 priority, but `content_type` is set to `ContentType::Text` instead of `ContentType::Sos`. Comments in both files acknowledge the intent to set `ContentType::Sos` but it was never wired. Any receiver that dispatches on content type (not just priority) — including a stricter future safety check or a different IRIS-compatible client — will fail to recognize it as an SOS.

**Evidence:** Code comments alongside the SOS path in both engine files say "should be `ContentType::Sos`" but use `ContentType::Text`.

**Root cause:** Incomplete implementation — the comment marks the intent, the wiring was deferred and forgotten.

**Fix:** When `priority == MessagePriority::P0` for a user-initiated SOS command, set `content_type = ContentType::Sos`.

**Dependencies / blast radius:**
- Affects both Android and Desktop engines — fix both in the same commit.
- Any receiver-side logic that branches on `ContentType::Sos` will now start receiving these messages correctly; verify the emergency gate in `message_engine/mod.rs` handles them as expected (it should — this is the intended path).

---

### AN-6 — X25519 key-exchange code is fully written and tested but never called

- **Fix status:** 🔒 Blocked on AN-1 — wiring X25519StaticAd requires the real `IrisCryptoProvider` to be in place first
- **File(s):** `android/app/src/main/kotlin/iriscore/identity/X25519StaticAd.kt`
- **Category:** safety · **Severity:** Medium
- **Tier:** 1
- **Original report:** Bug #6 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** A complete, tested X25519 static-plus-ephemeral key agreement module exists in the identity layer but is never called from the running app. The project's design spec (`ANDROID_DESIGN.md`) lists this as a required numbered item. The historical sign-off recorded it as "done" based on a test that only verifies the module's own output — not that it's connected to the crypto path. This is likely the missing wiring needed for AN-1's real encryption to function end-to-end.

**Evidence:** `grep -rn "X25519StaticAd"` — appears only in its own file and its unit test. No call site in the engine or transport layer.

**Root cause:** The module was completed and test-signed-off in isolation; the wiring into the session-establishment path was never implemented.

**Fix:** Call `X25519StaticAd` from the session key derivation step in the Android identity/crypto path. Coordinate with AN-1 — after the engine is pointed at `IrisCryptoProvider`, confirm which call site should invoke key agreement and wire it in.

**Dependencies / blast radius:**
- Depends on AN-1 being fixed first (real crypto provider must be in place before key agreement makes sense).
- Must be verified on-device: key agreement requires a real handshake between two instances.

---

## Tier 2 — Medium/Low correctness, UX, build

---

### AN-7 — Background BLE scanner stops immediately after starting; burns battery for nothing

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/service/BleScanSession.kt`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 2
- **Original report:** Bug #7 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The secondary BLE scanner starts and stops in the same call — no 30-second window occurs. Additionally, no callback is wired to receive results from this scanner even if it did run. The primary mesh scanner is unaffected.

**Evidence:** `BleScanSession.kt` — `startScan()` is followed immediately by `stopScan()` without any delay. The result listener is never registered.

**Root cause:** Missing async delay and missing result callback registration.

**Fix:** Insert a coroutine delay (30 seconds per spec) between start and stop. Wire the scan result callback. See also AN-12 (timing race in the same file) and AN-15 (rate limit, informational) — fix all three together.

---

### AN-8 — Engine initialization blocks the main (UI) thread on cold start

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/di/IrisCoreModule.kt`, `crates/iris-android/src/engine.rs`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 2
- **Original report:** Bug #8 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** Engine initialization (native library load + background thread creation) happens on the main thread on first access via the DI module. The project's own rule requires this to happen off the main thread. Result: a UI stutter on cold start.

**Evidence:** `IrisCoreModule.kt` — the engine provider is not wrapped in `Dispatchers.IO` or `withContext`. First access is triggered by the startup composable on the main thread.

**Root cause:** DI wiring provides the engine eagerly on main thread; no `Dispatchers.IO` wrapper.

**Fix:** Move engine initialization into a `withContext(Dispatchers.IO)` block, or configure the DI binding as `async`/`lazy` and ensure the first access point is already on an IO coroutine.

---

### AN-9 — Unexpected mesh-start failures crash the app instead of showing an error

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/ui/MeshViewModel.kt`, `android/app/src/main/kotlin/iriscore/data/MeshRepository.kt`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 2
- **Original report:** Bug #9 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The `catch` block around mesh startup only handles expected exception types. An unanticipated exception escapes the catch, propagates up uncaught, and crashes the app. The shutdown path handles this correctly with a broader catch — the same pattern was just not applied to startup.

**Evidence:** `MeshViewModel.kt` mesh-start catch block is narrowly typed; the shutdown equivalent uses a broader catch. A hypothetical new exception from a future code change would crash vs. show "radio unavailable."

**Root cause:** Inconsistent error handling between startup and shutdown paths.

**Fix:** Broaden the catch in the mesh-start path to `catch (e: Exception)` (same as shutdown), emit the existing "radio unavailable" error state.

---

### AN-10 — No reconnect or leave-mesh button; only fix after failure is full app restart

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/ui/MeshViewModel.kt`, `android/app/src/main/kotlin/iriscore/ui/screens/ConsoleScreen.kt`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 2
- **Original report:** Bug #10 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** After a mesh connection failure (e.g., Bluetooth off at startup), there is no UI affordance to retry. The ViewModel has no reconnect action; the screen has no reconnect button. The only path is to kill and reopen the app.

**Evidence:** `MeshViewModel.kt` — no `reconnect()` or `leaveMesh()` method exists. `ConsoleScreen.kt` — no retry/reconnect UI element in the error state.

**Root cause:** Reconnect flow was not implemented.

**Fix:** Add a `reconnectMesh()` action to the ViewModel that tears down the current mesh state and calls start again. Add a "Retry" button to the error state in `ConsoleScreen.kt`.

---

### AN-11 — ProGuard rule uses the wrong package name; native bridge breaks when shrinking is enabled

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/proguard-rules.pro:4`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2
- **Original report:** Bug #11 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The `-keep` rule that protects the JNI bridge class from being renamed/removed by R8 contains a slightly wrong package name. Currently harmless because `minifyEnabled` is off in the build config, but enabling shrinking for a production release would silently break the JNI link.

**Evidence:** `proguard-rules.pro:4` — package in the keep rule does not match the actual package of the JNI class.

**Root cause:** Typo.

**Fix:** Correct the package name in the `-keep` rule to match the actual JNI class path.

---

### AN-12 — Unsynchronized access to scan state in the secondary BLE scanner

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/service/BleScanSession.kt`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2
- **Original report:** Bug #12 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** Two coroutines can read and write the same scan-state variable without synchronization. Worst case today is one redundant call; not a crash. Fix alongside AN-7.

**Evidence:** `BleScanSession.kt` — shared mutable state modified in separate coroutine contexts without `@GuardedBy` or `Mutex`.

**Root cause:** Missing synchronization primitive around scan-state mutation.

**Fix:** Wrap scan-state access in a `Mutex` (or convert to `StateFlow`). Fix alongside AN-7.

---

### AN-13 — Command palette shows duplicate section headers and scroll lands off-target

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/kotlin/iriscore/ui/components/IrisCommandPalette.kt`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2
- **Original report:** Bug #13 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The command palette groups results by category and renders a section header for each group. When results span multiple categories but groups are not contiguous (e.g., search returns a mixed ordering), the same header can appear multiple times. Arrow-key navigation offsets land in the wrong row because the index calculation doesn't account for the extra header rows.

**Evidence:** `IrisCommandPalette.kt` — header insertion logic iterates over results and inserts a header on every category change, but doesn't de-duplicate when the same category appears in non-consecutive positions after filtering. Navigation index math doesn't count header rows.

**Root cause:** Grouping logic assumes pre-sorted, contiguous results.

**Fix:** Sort results by category before rendering, or de-duplicate headers. Recalculate navigation index accounting for header rows.

---

### AN-14 — White flash on app cold start contradicts the intentional all-black design

- **Fix status:** ⬜ Not started
- **File(s):** `android/app/src/main/res/values/themes.xml:6`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2
- **Original report:** Bug #14 in `ANDROID_SECTION4_ALL_BUGS.md`

**What:** The launch theme (shown before the first composable renders) has `android:windowBackground` set to white. The app's design is intentionally pure black. Every cold start shows a white flash.

**Evidence:** `themes.xml:6` — `windowBackground` is `@color/white` (or equivalent). The running app uses a pure black background.

**Root cause:** Launch theme was never updated to match the design spec.

**Fix:** Set `android:windowBackground` in the launch theme to `@android:color/black` (or the app's black color resource).

---

## Not applicable — Informational / cleanup

---

### AN-15 — Secondary BLE scanner has no OS rate-limit guard of its own

- **Fix status:** ⚪ Informational — no fix required
- **File(s):** `android/app/src/main/kotlin/iriscore/service/BleScanSession.kt`
- **Category:** cleanup · **Severity:** Informational
- **Original report:** Bug #15 in `ANDROID_SECTION4_ALL_BUGS.md`

Android limits BLE scan restarts to 5 per 30 seconds. The secondary scanner stays under this limit incidentally due to its own broken timing (AN-7), but does not enforce the limit itself the way the primary scanner does. Worth adding an explicit guard if AN-7 is fixed and the scanner starts running properly.

**Note:** Fix AN-7 first. If AN-7's fix means the scanner now runs continuously for 30 s and stops, the rate limit is naturally respected. Only add an explicit guard if the timing changes.

---

### AN-16 — Several resources and a theme file were built but never actually used

- **Fix status:** ⚪ Informational — no fix required
- **File(s):** `android/app/src/main/res/values/strings.xml` (9 of 13 entries), `android/app/src/main/res/values/colors.xml` (both entries), `android/app/src/main/kotlin/iriscore/ui/theme/Theme.kt`
- **Category:** cleanup · **Severity:** Informational
- **Original report:** Bug #16 in `ANDROID_SECTION4_ALL_BUGS.md`

Dead string resources, two unused color entries, and an entire theme file that the running screens ignore (they hardcode colors instead). Not harmful; four separate instances of "built but never wired" is a pattern worth a team conversation. May indicate more unfinished wiring is hiding similarly.

**Note:** Raise in a team sync. Clean up in a separate "dead code removal" PR if the team agrees nothing plans to use them.

---

## Appendix — Finding severity reference

| ID | Category | Severity | One-line description |
|---|---|---|---|
| AN-1 | safety | Critical | Android engine uses fake always-valid crypto stub — all messages forgeable, unencrypted |
| AN-2 | safety | High | Pre-built `.so` predates relay-crash fix — real phones may still contain the bug |
| AN-3 | correctness | High | `gradle.properties` line merge silently drops JDK setting — breaks cross-machine builds |
| AN-4 | correctness | High | CI Android test step points at wrong folder — zero Android tests ever run |
| AN-5 | protocol | High | SOS messages typed as `ContentType::Text` not `ContentType::Sos` at network level |
| AN-6 | safety | Medium | X25519 key-exchange module written, tested, and never called — real crypto not wired |
| AN-7 | correctness | Medium | Secondary BLE scanner starts and stops instantly — 30-second window never happens |
| AN-8 | correctness | Medium | Engine initialization blocks main thread on cold start — violates project rule |
| AN-9 | correctness | Medium | Unexpected mesh-start exception escapes catch and crashes app |
| AN-10 | correctness | Medium | No reconnect/retry button — mesh failure requires full app restart to retry |
| AN-11 | correctness | Low | ProGuard keep rule has wrong package name — JNI bridge breaks when shrinking enabled |
| AN-12 | correctness | Low | Unsynchronized scan-state access in secondary BLE scanner — harmless today |
| AN-13 | correctness | Low | Command palette shows duplicate headers and arrow-key navigation lands off-target |
| AN-14 | correctness | Low | Launch theme `windowBackground` is white; entire app is designed to be black |
| AN-15 | cleanup | Informational | Secondary BLE scanner lacks its own rate-limit guard (harmless while AN-7 broken) |
| AN-16 | cleanup | Informational | Dead string/color resources and an unused theme file — "built but never wired" pattern |
