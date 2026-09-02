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
| 1 | CROSS-001 | ✅ Fixed · PENDING BUILD VERIFICATION | 9bddad9 | `AndroidBleTransportAdapter.kt:69` `IRIS_SERVICE_UUID` changed `3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0d` → `01000000-0000-0000-0000-000000000000` (matches `iris_core::transport::ble::IRIS_SERVICE_UUID` and `IrisBleConstants.serviceUUID` exactly). Confirmed single definition site via grep across `android/` (7 hits total: 1 definition, 6 references — `BleScanSession.kt:111`, `ensureGattServer()` at `:530`/`:543`, GATT lookup at `:288`, two doc-comment mentions), all reference the constant, none duplicate the literal. Re-grepped whole repo for the old UUID afterward: zero hits in any `.kt`/`.rs`/`.swift` source file; remaining hits are hardware-log evidence files (historical, pre-fix captures) and this report's own bug narrative (also historical, describes the pre-fix state on purpose). No Android SDK/Gradle in this environment — cannot compile-verify per `integration_problem_loop.md` §4 step 6. |

**Run closeout:** Kotlin-only fix — no Rust files touched, so no Rust build was needed (and no `cargo` toolchain exists in this environment regardless). Kotlin side unverified by compiler; verified by exhaustive grep instead.

**Carried notes for later runs:** CROSS-001 needs promotion from `PENDING BUILD VERIFICATION` to plain `✅ Fixed` by a human running the Android Gradle build, then ideally a real two-device BLE discovery test (Android ↔ iOS) per the report's own recommended verification in `taksh_problems.md`'s hardware-verification precedent.

### Next run
Tier 1: CROSS-002 (iOS ContentType::Sos), CROSS-003 (NoopSecurityPolicy wiring), CROSS-004 (iOS DevCryptoProvider), CROSS-005 (Desktop received_at_unix) — no ordering dependency between them, fix in ID order.

---

## Run 2 — 2026-09-02 — Tier 1 (all four findings)

Target findings (4): CROSS-002, CROSS-003, CROSS-004, CROSS-005, in ID order per the loop's tier plan.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | CROSS-002 | ✅ Fixed | 5455985 | `iris-ios/src/engine.rs`'s `build_text_envelope` branches on `priority == MessagePriority::P0` to set `ContentType::Sos`, mirroring `iris-android/src/engine.rs`'s already-shipped identical branch. `MessagePriority`/`ContentType` already in scope; `MessagePriority` derives `PartialEq` (confirmed by reading `message.rs:18-20`). No Rust toolchain to compile-verify — correctness follows from exact structural mirroring of Android's shipped code. |
| 2 | CROSS-003 | ✅ Fixed | 720a9b3 | Added `engine.set_security_policy(Arc::new(FullSecurityPolicy::new(Arc::new(TrustStore::new()))))` after engine construction in all three platform engines (`iris-android/src/engine.rs`, `iris-ios/src/engine.rs`, `iris-desktop/src/engine_handle.rs`). Confirmed `FullSecurityPolicy`/`TrustStore` are root re-exports of `iris-core` (`lib.rs:40,51`) and `set_security_policy`'s signature (`message_engine/mod.rs:355`) before writing the call. Used a fresh per-platform `TrustStore` rather than threading each platform's own identity/trust plumbing through — sufficient to arm the policy; a populated, shared trust store (for `EmergencyAcl` to resolve real authorities) is explicitly out of this finding's scope, noted as a follow-on. |
| 3 | CROSS-004 | ✅ Fixed · PENDING BUILD VERIFICATION | 4e8b13a | Largest fix this run. New `iris-ios/src/ffi/crypto_signer.rs` (`FfiCryptoSigner`) + `iris-ios/src/ios_crypto.rs` (`IosCryptoProvider`), both direct ports of Android's AN-1 equivalents. Added `IrisEngine::new_with_signer` constructor; extracted shared `build()` helper with a runtime `cfg!()`-gated release guard (kept `is_dev_crypto` genuinely used in every build config, avoiding an unused-parameter warning under `-D warnings`). Swift side: new `KeychainCryptoSigner.swift` + both `AppDelegate.swift` engine-construction sites switched to `newWithSigner`. **Self-caught defect**: a first draft of the `engine.rs` edit closed the `#[uniffi::export] impl IrisEngine` block one function too early, which would have silently dropped `node_id()`/`send_text()`/`subscribe_inbox()`/etc. out of the UniFFI-exported surface — not a compile error, so `cargo check` alone would not have caught it even with a toolchain. Found by re-reading the full brace structure before committing; fixed by moving `build()` into the file's pre-existing non-exported `impl IrisEngine` block (same one `spawn_inbox_forwarder` already lives in), matching Android's own layout exactly. Needs both a Rust build and an Xcode/`build-xcframework.sh` Swift-binding regeneration to promote off PENDING. |
| 4 | CROSS-005 | ✅ Fixed | 5f25d7e | `iris-desktop/src/types.rs`: `received_at_unix: env.timestamp` → `received_at_unix: unix_now()`. Confirmed `Envelope::timestamp`'s doc comment ("Unix epoch seconds at origination") matches `unix_now()`'s own return unit before concluding no `*1000` scaling was needed (unlike Android/iOS's millisecond field). |

**Run closeout:** No Rust/Gradle/Xcode toolchain in this environment for any of the four fixes — all verified by source reading (import paths, trait signatures, brace structure, unit consistency) rather than compilation. CROSS-002/003/005 touch only already-battle-tested patterns (mirroring Android's shipped code, or a one-line unit fix) and are marked plain `✅ Fixed`. CROSS-004 introduces substantial new code on both the Rust and Swift sides and is marked `PENDING BUILD VERIFICATION`.

**Carried notes for later runs:**
- CROSS-001 and CROSS-004 both need a human to run the real platform build before their PENDING status can be lifted.
- CROSS-003's fix is deliberately narrow (arms the policy, does not populate a shared trust store) — flagged inline as a follow-on, not filed as a new finding since it's outside what CROSS-003 itself claimed.
- Tier 1 is now fully closed. Tier 2 (CROSS-006/007/008) has no ordering dependency on Tier 0/1 and can start any time.

### Next run
Tier 2: CROSS-006 (RoutingEngine/ScfEngine wiring — the largest remaining fix, cross-cutting into `message_engine/mod.rs`'s delivery loop), CROSS-007 (Android Kotlin FFI import names), CROSS-008 (iOS `performMaintenance` binding regen — same PENDING class as CROSS-004).

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
