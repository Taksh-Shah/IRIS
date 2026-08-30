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

## Run 1 — [DATE] — Tier 0 batch (AN-1, AN-2)

**(Fill in when run begins.)**

Target findings: AN-1 (Critical, fake crypto stub), AN-2 (High, stale `.so`).

Pre-run drift audit: re-read `engine.rs:127`, `X25519StaticAd.kt`, and `.so` git log against HEAD before writing any fix.

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-1 | ⬜ | — | — |
| 2 | AN-2 | ⬜ | — | — |

**Batch closeout:** `./gradlew assembleDebug` clean · `./gradlew test` pass count · also update `sohan_problems.md` Tier 0 rows and `taksh_problems.md` Tier 0 rows to ✅.

### Next run
Advance to Tier 1: AN-3 (gradle.properties), AN-4 (CI path), AN-5 (SOS content type), AN-6 (X25519 wiring).

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

## Run 3 — [DATE] — Tier 2 batch A (AN-7+AN-12, AN-8, AN-9, AN-10)

**(Fill in when run begins.)**

Target findings: AN-7+AN-12 (BLE scanner timing + sync — fix together), AN-8 (engine init on main thread), AN-9 (exception crash), AN-10 (no reconnect).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-7 + AN-12 | ⬜ | — | — |
| 2 | AN-8 | ⬜ | — | — |
| 3 | AN-9 | ⬜ | — | — |
| 4 | AN-10 | ⬜ | — | — |

**Batch closeout:** `./gradlew assembleDebug` clean · `./gradlew test` pass count. Update both tracker files.

### Next run
Tier 2 batch B: AN-11 (ProGuard), AN-13 (command palette), AN-14 (white flash). Informational review: AN-15, AN-16.

---

## Run 4 — [DATE] — Tier 2 batch B (AN-11, AN-13, AN-14) + informational review

**(Fill in when run begins.)**

Target findings: AN-11 (ProGuard wrong package), AN-13 (command palette headers + nav), AN-14 (launch theme white flash). Also: review AN-15 and AN-16, update N/A status if anything changed.

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | AN-11 | ⬜ | — | — |
| 2 | AN-13 | ⬜ | — | — |
| 3 | AN-14 | ⬜ | — | — |
| 4 | AN-15 | ⚪ N/A | — | _(re-check after AN-7 fix: does the scanner now run for 30 s? If yes and rate-limit guard still missing, escalate to Tier 2.)_ |
| 5 | AN-16 | ⚪ N/A | — | _(confirm resources still dead; file a cleanup PR pointer if team agrees)_ |

**Batch closeout:** `./gradlew assembleDebug` clean · `./gradlew test` pass count.

**Tier 2 checkpoint:** all 8 Tier 2 findings should be ✅ or 🔒. Update both tracker files.

### Next run
All Android Section 4 findings resolved. Perform final sweep: confirm `sohan_problems.md` progress table matches, confirm `taksh_problems.md` is in sync, record green CI run evidence.

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
