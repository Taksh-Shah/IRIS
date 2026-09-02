# Integration Fix Log — cross-section bug-hunt execution journal

**Companion to:** [`integration_problem_loop.md`](integration_problem_loop.md) (logic) ·
[`integration_problems.md`](integration_problems.md) (state).
This file records each fix run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.

Baseline at run 1 start (`3e70a87`, clean tree): report published with 8
findings, 0 fixed. `cargo build --workspace` clean at this commit (verified
during the original sweep — see `integration_problems.md` §6 "What was
checked").

**Environment honesty:** this environment has no Android SDK/Gradle toolchain
and no Xcode/iOS toolchain — only `cargo` is runnable here. Any finding whose
fix lands in `android/*.kt` or `ios/*.swift` (or requires regenerating UniFFI
Swift bindings) is verified by careful source reading and cross-repository
grep, not by an actual platform build. Those fixes are marked `✅ Fixed ·
PENDING BUILD VERIFICATION` per `integration_problem_loop.md` §4 step 6, and
must be promoted to a bare `✅ Fixed` by a human running the real platform
build, same discipline `taksh_fix_log.md` uses for `PENDING LIVE-PG` and
`PENDING HARDWARE VERIFICATION`.

---

## Run log

## Run 1 — 2026-09-02 — Tier 0

Target finding (1): CROSS-001.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | CROSS-001 | ✅ Fixed · PENDING BUILD VERIFICATION | (pending commit) | `AndroidBleTransportAdapter.kt:69` `IRIS_SERVICE_UUID` changed `3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0d` → `01000000-0000-0000-0000-000000000000` (matches `iris_core::transport::ble::IRIS_SERVICE_UUID` and `IrisBleConstants.serviceUUID` exactly). Confirmed single definition site via grep across `android/` (7 hits total: 1 definition, 6 references — `BleScanSession.kt:111`, `ensureGattServer()` at `:530`/`:543`, GATT lookup at `:288`, two doc-comment mentions), all reference the constant, none duplicate the literal. Re-grepped whole repo for the old UUID afterward: zero hits in any `.kt`/`.rs`/`.swift` source file; remaining hits are hardware-log evidence files (historical, pre-fix captures) and this report's own bug narrative (also historical, describes the pre-fix state on purpose). No Android SDK/Gradle in this environment — cannot compile-verify per `integration_problem_loop.md` §4 step 6. |

**Run closeout:** Kotlin-only fix — no Rust files touched, so no Rust build was needed (and no `cargo` toolchain exists in this environment regardless). Kotlin side unverified by compiler; verified by exhaustive grep instead.

**Carried notes for later runs:** CROSS-001 needs promotion from `PENDING BUILD VERIFICATION` to plain `✅ Fixed` by a human running the Android Gradle build, then ideally a real two-device BLE discovery test (Android ↔ iOS) per the report's own recommended verification in `taksh_problems.md`'s hardware-verification precedent.

### Next run
Tier 1: CROSS-002 (iOS ContentType::Sos), CROSS-003 (NoopSecurityPolicy wiring), CROSS-004 (iOS DevCryptoProvider), CROSS-005 (Desktop received_at_unix) — no ordering dependency between them, fix in ID order.

---

### Run entry format (for the next run to follow)

```
## Run N — <date> — <what this run targeted>

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | CROSS-00X | ✅ Fixed [· PENDING BUILD VERIFICATION] | <short-hash> | <what was checked, what a human still needs to run if pending> |

**Run closeout:** <build/test result summary>.

**Carried notes for later runs:** <anything the next run needs to know>.

### Next run
<which finding(s) are next per the loop's tier order>
```
