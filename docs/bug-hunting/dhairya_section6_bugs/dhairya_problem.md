# Section 6 (Desktop App & Build Infrastructure) — Bug-Hunt Problem Tracker

**Reviewer:** Dhairya (Section 6 — Desktop App & Build Infrastructure)
**Source review:** `SEC6_AUDIT.md` v1.1 — 21 original findings + 5 additional found in post-audit sweep (2026-08-30) + 2 more found in deep sweep = **28 total findings**.
**Scope:** `crates/iris-desktop/` (all Rust + UI), `.github/workflows/ci.yml`, `.config/nextest.toml`, `deny.toml`, `.cargo/audit.toml`, `engineering/SECURITY_POLICY.yaml`, `Cargo.toml` (workspace deps only), `crates/iris-android/Cargo.toml`, `crates/iris-ios/Cargo.toml`
**Companion documents:**
- [`dhairya_problem_log.md`](dhairya_problem_log.md) — execution journal (what each run did, evidence, commits)
- [`dhairya_problem_loop.md`](dhairya_problem_loop.md) — autonomous loop spec (how to execute this tracker)
- [`dhairya_problems.md`](dhairya_problems.md) — self-contained fix brief with exact code replacements per finding

**Baseline (review start):** Workspace builds locally. `cargo deny check` FAILS on unmaintained gtk-family advisories (expected — F-C2 fixes this). `cargo audit` exits 0. No automated proof that desktop tests pass until F-C3 lands and Ubuntu CI gets system libs.

**Build note:** All fixes require `cargo build --workspace` to stay green. Desktop tests run with `cargo test -p iris-desktop`. The F-D1 fix adds new integration tests that must be green before the run closes. SHA pinning in F-C4 requires resolving real commit SHAs at execution time — never invent a SHA.

---

## Progress Tracker

| Tier | Name | Total | ✅ Fixed/Resolved | 🔮 Future | 🔒 Blocked | ⬜ Not started |
|---|---|---|---|---|---|---|
| 0 | High safety / security / emergency-path | 3 | 3 | 0 | 0 | 0 |
| 1 | Medium correctness + protocol | 4 | 4 | 0 | 0 | 0 |
| 2 | Low correctness, UX, build polish | 14 | 14 | 0 | 0 | 0 |
| N/A | Deferred / routed / pass / no action | 7 | 6 | 1 | 0 | 0 |
| **Total** | | **28** | **27** | **1** | **0** | **0** |

N/A breakdown: ✅ F-D6 (resolved — existing gate), F-S1 (CSP fixed), F-S3 (verified clean), F-W2 (verified clean), F-P3 (provenance banners fixed), F-C1 (✅ fixed 2026-09-01 — `android/gradlew` + `android/gradlew.bat` created) · 🔮 F-S4 (future DESKTOP-002)

**Last updated:** 2026-09-01 · **Active tier:** Complete — all 21 actionable tier findings fixed; 6/7 N/A findings resolved (F-C1 unblocked).

---

## Tier membership (authoritative)

**Tier 0** (3 findings — fix before anything else; emergency-path and supply-chain security):
F-C2, F-DX-1, F-DX-6

**Tier 1** (4 findings — medium correctness + desktop protocol; no human gate after Tier 0):
F-C3, F-D1, F-P1, F-DX-4

**Tier 2** (14 findings — low correctness, UX, build polish; no human gate after Tier 1):
F-C4, F-C5, F-D2, F-D3, F-D7, F-D4, F-D5, F-S2, F-W1, F-P2, F-DX-2, F-DX-3, F-DX-5, F-DX-7

**Not applicable / resolved** (7 findings — deferred / routed / verified pass / no action):
F-C1 (✅ fixed 2026-09-01 — `android/gradlew` and `android/gradlew.bat` created, unblocking the `jvm` CI job), F-D6 (✅ resolved — iris-core MessagePriority::from_u8 + gateway serves_priority already gates all 8 levels), F-S1 (✅ fixed — CSP added to tauri.conf.json), F-S3 (pass — verified clean), F-S4 (info — future DESKTOP-002), F-W2 (pass — empirically settled by F-C2 Step 1), F-P3 (✅ fixed — provenance banners added to both layer docs)

**Total: 3 + 4 + 14 + 7 = 28 findings.**

---

## Tier 0 — High safety / security / emergency-path

---

### F-C2 — Align cargo-deny with cargo-audit suppressions

- **Fix status:** ✅ Fixed · Run 1 · 2026-08-30
- **File(s):** `deny.toml` (advisories block)
- **Category:** supply-chain security · **Severity:** HIGH
- **Tier:** 0
- **Fix brief:** §2 GROUP B → F-C2 in `dhairya_problems.md`

**What:** `deny.toml` has `ignore = []` while `.cargo/audit.toml` suppresses 17 RUSTSEC advisories. The two gates contradict each other — `cargo deny check` fails on every CI run on advisories that the team has already reviewed and explicitly suppressed in `audit.toml`. Any new advisory that IS a real problem is invisible in the noise.

**Root cause:** Suppressions were added to `audit.toml` but never mirrored to `deny.toml`.

**Fix:** Mirror all 17 suppressions (11 gtk-family + proc-macro-error + 5 unic-*) into `deny.toml`'s `ignore` list with the same reason + expiry contract already present in `audit.toml`. Run `cargo deny check` before and after — must go from FAIL to exit 0.

**Dependencies / blast radius:**
- Step 1 (pre-edit `cargo deny check` run) also settles F-W2 empirically.
- Coordinate with the F-P2 fix (license list) — both edit `deny.toml` in the same section, do in the same commit.

---

### F-DX-1 — Desktop `build_text_envelope` always sets `ContentType::Text` even for P0/SOS

- **Fix status:** ✅ Fixed · Run 1 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:398`
- **Category:** protocol / emergency-path · **Severity:** HIGH
- **Tier:** 0
- **Fix brief:** §6 F-DX-1 in `dhairya_problems.md`

**What:** When a user sends `/sos <message>` from the desktop, `build_text_envelope` always sets `payload_type: ContentType::Text` regardless of priority. A P0 emergency message arrives at relay/peer nodes typed as Text, defeating SOS priority routing and emergency-delivery guarantees. The Android AN-5 fix corrected this for the Android engine — the desktop engine was never updated.

**Root cause:** The Android AN-5 fix was not ported to the desktop equivalent in `engine_handle.rs`.

**Fix:** Conditionally set `payload_type: ContentType::Sos` when `priority == MessagePriority::P0`, `ContentType::Text` otherwise. **Must be fixed in the same commit as F-DX-6** — applying DX-1 without DX-6 makes the SOS command worse (correctly tagged at send, but text invisible to recipient on receive).

**Dependencies / blast radius:**
- F-DX-6 MUST be in the same commit (text extraction for SOS payloads in `types.rs`).
- Any receiver-side logic that branches on `ContentType::Sos` will now correctly receive SOS messages from desktop senders.

---

### F-DX-6 — `IncomingMessageView` drops text payload for `ContentType::Sos` messages

- **Fix status:** ✅ Fixed · Run 1 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/types.rs:44`
- **Category:** protocol / emergency-path · **Severity:** MEDIUM (coupled to HIGH F-DX-1)
- **Tier:** 0
- **Fix brief:** §6 F-DX-6 in `dhairya_problems.md`

**What:** `IncomingMessageView::from_envelope` only extracts text for `ContentType::Text` payloads. After F-DX-1 is applied, SOS messages sent from the desktop are tagged `ContentType::Sos`. Any desktop receiving such a message gets `text: None` in the view — the UI shows `[Sos payload not shown]` instead of the actual emergency message text.

**Root cause:** The text-extraction guard was written when only `ContentType::Text` existed on the desktop; `ContentType::Sos` was never added to the match.

**Fix:** Extend the `if` condition to `if matches!(env.payload_type, ContentType::Text | ContentType::Sos)`. Must fix in the same commit as F-DX-1.

**Dependencies / blast radius:**
- Coupled to F-DX-1 — fix together, never separately.

---

## Tier 1 — Medium correctness + desktop protocol

---

### F-C3 — Ubuntu CI jobs lack GTK/webkit system libraries

- **Fix status:** ✅ Fixed · Run 2 · 2026-08-30
- **File(s):** `.github/workflows/ci.yml` (`test`, `lint`, `coverage` jobs)
- **Category:** CI correctness · **Severity:** MEDIUM
- **Tier:** 1
- **Fix brief:** §2 GROUP A → F-C3 in `dhairya_problems.md`

**What:** The `test`, `lint`, and `coverage` jobs compile `iris-desktop`, which depends on Tauri/wry. On Linux, Tauri needs `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, and related system libs. Today correctness depends on whatever GitHub's runner image preinstalls — a runner image update can break all three jobs at once with no repo-side hint.

**Root cause:** System dependency installation step was never added.

**Fix:** Insert one `apt-get install` step immediately after `actions/checkout` in each of the three jobs, guarded by `if: runner.os == 'Linux'`.

**Dependencies / blast radius:**
- Shared CI file — coordinate with CI owner before merging.
- Fixes silently failing Ubuntu matrix cells.

---

### F-D1 — Inbox stream dies silently on broadcast lag

- **Fix status:** ✅ Fixed · Run 2 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:299–325` + new `tests/inbox_pump.rs`
- **Category:** correctness · **Severity:** MEDIUM
- **Tier:** 1
- **Fix brief:** §2 GROUP C → F-D1 in `dhairya_problems.md`

**What:** The `subscribe_inbox` forwarder uses a `while let Ok(env) = rx.recv().await` pattern. When a burst overflows the engine's broadcast ring, `RecvError::Lagged(n)` is returned. This is recoverable — the loop should skip the lost envelopes and continue. Instead it exits the loop, permanently killing the webview inbox with no log and no resubscribe.

**Note (drift):** The current code at HEAD already has a partially-corrected `loop { match rx.recv() ... }` pattern with `Lagged` logged and continued. The remaining F-D1 work is: (a) extract `pump_inbox` as a free function for testability, (b) add the `inbox_task: Mutex<Option<AbortHandle>>` field for F-D2, and (c) write the regression test in `tests/inbox_pump.rs`.

**Root cause:** Recoverable broadcast errors were not distinguished from terminal ones; lag-tolerant pump was not extracted for testing.

**Fix:** Extract `pump_inbox` free function; add `inbox_task` field to `DesktopEngine`; write mandatory regression test. Fix includes F-D2 (abort handle) as it is folded in.

**Dependencies / blast radius:**
- F-D2 is folded into this fix — do together.
- New test must be GREEN before closing the run.

---

### F-P1 — SECURITY_POLICY key-storage annotation missing

- **Fix status:** ✅ Fixed · Run 2 · 2026-08-30
- **File(s):** `engineering/SECURITY_POLICY.yaml` (key_storage area, after `rule:` line)
- **Category:** policy annotation · **Severity:** MEDIUM-drift
- **Tier:** 1
- **Fix brief:** §2 GROUP B → F-P1 in `dhairya_problems.md`

**What:** `SECURITY_POLICY.yaml` asserts "Private keys NEVER in plaintext in storage or logs." The shipped desktop store (`FileKeyStore`) stores seeds plaintext-at-rest. The policy must be annotated to document this v1 deviation so it does not mislead security reviewers into thinking the file's assertion is already true.

**Root cause:** Policy was written to spec; code was not fully implemented to match.

**Fix:** Insert a `# v1 DEVIATION` comment block immediately below the `rule:` line — policy annotation ONLY, no code changes. Hardening is owned by Section 1 under RT-005.

**Dependencies / blast radius:** Zero. Comment-only change. YAML must still parse.

---

### F-DX-4 — `spawn_inbox_forwarder` exits silently when transport stream ends

- **Fix status:** ✅ Fixed · Run 2 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:248`
- **Category:** correctness · **Severity:** MEDIUM
- **Tier:** 1
- **Fix brief:** §6 F-DX-4 in `dhairya_problems.md`

**What:** The inbound forwarder loop `while let Some(msg) = incoming.next().await` exits with no log when the transport stream closes. In practice transport streams are long-lived singletons, but an unexpected stream termination kills the receive path for that transport permanently with zero operator visibility.

**Root cause:** No logging added for the stream-end case.

**Fix:** Add a `tracing::warn!(...)` after the loop body to signal that the forwarder died and will not recover.

**Dependencies / blast radius:** Minimal — adds one log line, no behaviour change.

---

## Tier 2 — Low correctness, UX, build polish

---

### F-C4 — CI supply-chain hardening batch

- **Fix status:** ✅ Fixed · Run 3 · 2026-08-30 (SHA pins: TODO comments added — network unavailable at fix time)
- **File(s):** `.github/workflows/ci.yml`
- **Category:** CI hardening · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP A → F-C4 (sub-fixes a–e) in `dhairya_problems.md`

**What:** Four sub-fixes: (a) missing top-level `permissions: contents: read` — default token is too broad; (b) no job-level `timeout-minutes` — GitHub default is 360 min; (c) third-party Actions pinned by mutable tags, not SHA; (d) Kani version floats to latest; (e) fuzz job nightly cache key rotates daily — add an acceptance comment.

**Fix:** Apply all four sub-fixes in one commit. For SHA pins: resolve real SHAs at execution time via `git ls-remote`; never invent a SHA — if unresolvable, skip and add a `TODO(security):` comment.

---

### F-C5 — nextest config comment contradicts `retries = 0`

- **Fix status:** ✅ Fixed · Run 3 · 2026-08-30
- **File(s):** `.config/nextest.toml:6`
- **Category:** documentation · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP A → F-C5 in `dhairya_problems.md`

**What:** The `[profile.default]` comment says "Retry a test up to 2 times on failure" but `retries = 0`. The value is correct; the comment is wrong.

**Fix:** Replace the comment to match the value. One-line change.

---

### F-D2 — Subscription task lifecycle (abort previous on re-subscribe)

- **Fix status:** ✅ Fixed · Run 2 · 2026-08-30 (folded into F-D1)
- **File(s):** `crates/iris-desktop/src/engine_handle.rs`
- **Category:** correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-D2 in `dhairya_problems.md`

**What:** Repeated `subscribe_inbox` IPC calls (webview reloads) spawn new forwarder tasks without aborting the old one. Old tasks accumulate, each silently consuming CPU and memory.

**Fix:** Add `inbox_task: Mutex<Option<AbortHandle>>` to `DesktopEngine`; abort the previous pump on re-subscribe. **Already folded into the F-D1 fix commit** — do not fix separately.

---

### F-D3 — Size-cap copy says "64 KB" but constant is 60,000 bytes

- **Fix status:** ✅ Fixed · Run 3 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:385–387`
- **Category:** documentation / correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-D3/D7 in `dhairya_problems.md`

**What:** The error string says "64 KB max" but the actual limit is `60_000` bytes. Fix also extracts a named constant and adds the D7 UTF-8-vs-UTF-16 explanatory comment.

**Fix:** Replace inline check with `const MAX_TEXT_BYTES: usize = 60_000` and update error string to match. See also F-D7 (same edit).

---

### F-D7 — HTML `maxlength` counts UTF-16 units; Rust checks UTF-8 bytes

- **Fix status:** ✅ Fixed · Run 3 · 2026-08-30 (combined with F-D3)
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:385` + optional `ui/app.js`
- **Category:** correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-D3/D7 in `dhairya_problems.md`

**What:** `<input maxlength="60000">` counts UTF-16 code units while the Rust size check counts UTF-8 bytes. A message with multibyte characters can pass the front-end guard and hit the Rust error. Fix is a comment documenting the intentional backstop + an optional JS `TextEncoder` byte-count check.

**Fix:** Add the explanatory comment alongside the F-D3 constant. Optionally add a `TextEncoder` byte-count guard in the `app.js` submit path.

---

### F-D4 — Malformed relay addresses dropped silently

- **Fix status:** ✅ Fixed · Run 3 · 2026-08-30
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:438–451`
- **Category:** correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-D4 in `dhairya_problems.md`

**What:** `parse_relay_env()` uses `filter_map(|s| s.parse().ok())` — a malformed entry in `IRIS_RELAY_ADDR` is silently dropped. A misconfigured relay address looks like a network outage.

**Fix:** Replace `filter_map` with a `for` loop that emits `tracing::warn!` on each rejected entry.

---

### F-D5 — Peer-list click prefills an invalid-looking `/to` command

- **Fix status:** ✅ Fixed · Run 4 · 2026-08-30
- **File(s):** `crates/iris-desktop/ui/app.js:494–498`
- **Category:** UX · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-D5 in `dhairya_problems.md`

**What:** Clicking a peer row prefills `/to ${id}` where `id` is a 16-hex short ID — not a valid 64-hex PeerId. The user sees what looks like a working command but it will fail on submit.

**Fix:** Prefill `/to @short:${id}` to make the guaranteed-invalid short-ID shape visually obvious.

---

### F-S2 — Windows manifest lacks `requestedExecutionLevel`

- **Fix status:** ✅ Fixed · Run 4 · 2026-08-30
- **File(s):** `crates/iris-desktop/windows-app-manifest.xml`
- **Category:** security / correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP C → F-S2 in `dhairya_problems.md`

**What:** The manifest has no `trustInfo` / `requestedExecutionLevel` element. Without it, Windows may apply UAC heuristics and prompt for elevation on some user configurations.

**Fix:** Add `<trustInfo>` block with `level="asInvoker" uiAccess="false"` — correct for a user-space messenger.

---

### F-W1 — Centralize uniffi/tempfile pins; remove dead `serde_json`

- **Fix status:** ✅ Fixed · Run 4 · 2026-08-30 (android/ios Cargo.toml changes flagged per boundary #12)
- **File(s):** `Cargo.toml` (workspace), `crates/iris-android/Cargo.toml`, `crates/iris-ios/Cargo.toml`, `crates/iris-desktop/Cargo.toml`
- **Category:** build correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §2 GROUP D → F-W1 in `dhairya_problems.md`

**What:** Three sub-defects: (a) `uniffi = "=0.31.2"` and `uniffi_bindgen = "=0.31.2"` pinned separately in android and ios manifests — one platform diverging silently is a classic trap; (b) `crates/iris-desktop/Cargo.toml` has `tempfile = "3"` duplicating the workspace pin; (c) `serde_json = "1"` in iris-desktop dev-deps has zero usages anywhere.

**Fix:** Add both pins to `[workspace.dependencies]`; switch platform manifests to `uniffi.workspace = true`; switch desktop `tempfile` to `tempfile.workspace = true`; delete the dead `serde_json` line.

---

### F-P2 — Reconcile the two license allowlists

- **Fix status:** ✅ Fixed · Run 4 · 2026-08-30
- **File(s):** `deny.toml` (licenses block), `engineering/SECURITY_POLICY.yaml:99`
- **Category:** policy correctness · **Severity:** LOW-drift
- **Tier:** 2
- **Fix brief:** §2 GROUP B → F-P2 in `dhairya_problems.md`

**What:** `deny.toml` allows 11 licenses; `SECURITY_POLICY.yaml` allows only 6 — the two lists disagree in both directions. `Unlicense` is policy-approved but fails the deny gate; `Zlib`, `WTFPL`, `MPL-2.0`, and others pass the deny gate but are not in the policy.

**Fix:** Add `"Unlicense"` to both `[licenses]` blocks in `deny.toml`; expand `SECURITY_POLICY.yaml:99` to the full 12-license list; document the MPL-2.0 copyleft decision.

---

### F-DX-2 — `renderPalette` adjacent-only group dedup (JS version of AN-13)

- **Fix status:** ✅ Fixed · Run 5 · 2026-08-30
- **File(s):** `crates/iris-desktop/ui/app.js:178–188`
- **Category:** UX correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §6 F-DX-2 in `dhairya_problems.md`

**What:** `renderPalette` uses `let lastGroup = null` — emits a group header only when the current group differs from the previous. When search results have non-contiguous group recurrences (confirmed reproducible with a simple `h` query), the same section header appears twice. JS version of the AN-13 bug.

**Fix:** Replace `let lastGroup` with `const seenGroups = new Set()` and guard with `!seenGroups.has(command.group)`.

---

### F-DX-3 — `navigator.platform` deprecated API

- **Fix status:** ✅ Fixed · Run 5 · 2026-08-30
- **File(s):** `crates/iris-desktop/ui/app.js:14`
- **Category:** correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §6 F-DX-3 in `dhairya_problems.md`

**What:** `navigator.platform` is deprecated since Chrome 101 / Firefox 127. In Tauri's embedded WebView this still works today but is scheduled for removal. Used for Mac keyboard detection (`Cmd` vs `Ctrl`).

**Fix:** Replace with `navigator.userAgentData?.platform` with a safe fallback to `navigator.platform` for WebKit/Safari.

---

### F-DX-5 — Dev-seam builder methods accessible without a feature gate

- **Fix status:** ✅ Fixed · Run 5 · 2026-08-30 (test-seams feature gate; integration tests require --features test-seams or --all-features)
- **File(s):** `crates/iris-desktop/src/engine_handle.rs:82,93` + `crates/iris-desktop/Cargo.toml`
- **Category:** security / correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §6 F-DX-5 (corrected spec) in `dhairya_problems.md`

**What:** `DesktopEngine::with_node_id` and `with_transport` set `dev: true` (routes through `DevCryptoProvider` — fake crypto). They are `pub` with no gate, reachable from production code. **Important:** `#[cfg(test)]` is NOT the right fix — these methods are called from integration tests in `tests/engine_roundtrip.rs` and `tests/commands_mock.rs`, which link against the non-test build of the library and cannot see `#[cfg(test)]`-gated items.

**Fix:** Add a `test-seams = []` Cargo feature to `crates/iris-desktop/Cargo.toml`; gate both methods with `#[cfg(feature = "test-seams")]`; run integration tests with `--features test-seams`.

---

### F-DX-7 — Telemetry command test is vacuously true on a fresh engine

- **Fix status:** ✅ Fixed · Run 5 · 2026-08-30
- **File(s):** `crates/iris-desktop/tests/commands_mock.rs:83`
- **Category:** test correctness · **Severity:** LOW
- **Tier:** 2
- **Fix brief:** §6 F-DX-7 in `dhairya_problems.md`

**What:** `get_telemetry` filters for `value > 0` counters. A fresh engine has all-zero counters, returning an empty `Vec`. The test asserts `.iter().all(|m| m.value > 0)` — vacuously true on an empty slice. The test always passes and proves nothing.

**Fix:** Send one message first to make the `sent` counter non-zero, then assert that at least one counter with `sent` in the name appears with `value > 0`.

---

## Not applicable — Deferred / Routed / Pass / No action

---

### F-C1 — JVM gate repair (FIXED)

- **Fix status:** ✅ Fixed · 2026-09-01
- **Category:** CI correctness · **Severity:** HIGH

`android/gradlew` (POSIX shell, Gradle 8.9) and `android/gradlew.bat` (Windows batch) created. The `jvm` CI job's `if [ -f gradlew ]` gate now finds the wrapper and `./gradlew testDebugUnitTest` can run. The existing `gradle/wrapper/gradle-wrapper.jar` at `android/gradle/wrapper/gradle-wrapper.jar` (Gradle 8.9) was already in the repo.

---

### F-D6 — Priority 0–7 forwarded unchecked (RESOLVED)

- **Fix status:** ✅ Resolved — iris-core already gates priority at the engine boundary
- **Category:** protocol · **Severity:** INFO

`engine_handle.rs:283–284` validates priority via `MessagePriority::from_u8(priority).ok_or_else(...)` and the gateway module's `serves_priority()` enforces the P0→all-gateways / P1–P2→primary+backup / P3+→single selection matrix. Priority is never forwarded unchecked. No desktop-side gate needed.

---

### F-S1 — WebView CSP (FIXED)

- **Fix status:** ✅ Fixed — CSP set in `crates/iris-desktop/tauri.conf.json`
- **Category:** security · **Severity:** INFO

Added `"csp": "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src ipc: http://ipc.localhost"`. Allows only same-origin resources plus Tauri IPC; blocks all external fetches.

---

### F-S3 — Capability scoping least-privilege (PASS)

- **Fix status:** ✅ Verified clean by SEC6_AUDIT
- **Category:** security · **Severity:** PASS

Capability file grants `core:default` only. Custom app commands work via `generate_handler!` and do not require additional ACL entries. No action needed.

---

### F-S4 — Production bundling absent (INFO)

- **Fix status:** 🔮 Future scope — tracked as DESKTOP-002 packaging node
- **Category:** build · **Severity:** INFO

`bundle.active = false` in `tauri.conf.json`. Not a defect — intentional during development phase. No fix in this sprint.

---

### F-W2 — Lock ↔ deny.toml skip consistency (PASS)

- **Fix status:** ✅ Verified PASS by SEC6_AUDIT; fully settled by F-C2 Step-1 run
- **Category:** supply-chain · **Severity:** PASS

Running `cargo deny check` as the first step of F-C2 settles this empirically. No separate action.

---

### F-P3 — Orphan PYTHON/TYPESCRIPT layer docs (FIXED)

- **Fix status:** ✅ Fixed — provenance banners added to both docs
- **Category:** documentation · **Severity:** FLAG

Added explicit `Scope:` metadata to `docs/implementation/PYTHON_LAYER.md` (research/tooling only — not production) and updated `docs/implementation/TYPESCRIPT_LAYER.md` with a status banner clarifying it describes the planned v2 TypeScript/React architecture, not the current plain-JS implementation.

---

## Appendix — Finding severity reference

| ID | Category | Severity | One-line description |
|---|---|---|---|
| F-C2 | supply-chain | HIGH | `deny.toml` ignores 17 advisories already suppressed in `audit.toml` — gates contradict each other |
| F-DX-1 | protocol | HIGH | Desktop P0 SOS messages always tagged `ContentType::Text` — emergency routing bypassed |
| F-DX-6 | protocol | MEDIUM | `IncomingMessageView` drops text for SOS payloads — receiver sees `[Sos payload not shown]` |
| F-C3 | CI | MEDIUM | Ubuntu CI jobs lack GTK/webkit system libs — runner-image change breaks three jobs at once |
| F-D1 | correctness | MEDIUM | Inbox pump must be extracted as testable free function with lag-tolerance regression test |
| F-P1 | policy | MEDIUM | SECURITY_POLICY asserts plaintext-free key storage that v1 desktop does not implement |
| F-DX-4 | correctness | MEDIUM | `spawn_inbox_forwarder` exits with no log when transport stream closes |
| F-C4 | CI hardening | LOW | Missing permissions, timeouts, SHA pins, Kani pin, fuzz-cache comment |
| F-C5 | documentation | LOW | nextest comment says "retry 2 times" but `retries = 0` |
| F-D2 | correctness | LOW | Re-subscribe accumulates dormant forwarder tasks — abort handle needed |
| F-D3 | documentation | LOW | Error copy says "64 KB" but limit is 60,000 bytes |
| F-D7 | correctness | LOW | HTML `maxlength` counts UTF-16 units; Rust checks UTF-8 bytes — no front-end byte guard |
| F-D4 | correctness | LOW | Malformed relay entries in `IRIS_RELAY_ADDR` dropped silently — looks like network outage |
| F-D5 | UX | LOW | Peer-list click prefills short ID that looks valid but always fails on submit |
| F-S2 | security | LOW | Windows manifest missing `requestedExecutionLevel` — UAC heuristics may prompt elevation |
| F-W1 | build | LOW | uniffi pins duplicated across platforms; tempfile literal; dead serde_json dev-dep |
| F-P2 | policy | LOW | deny.toml and SECURITY_POLICY license allowlists disagree in both directions |
| F-DX-2 | UX | LOW | JS palette group dedup is adjacent-only — same-group non-contiguous results emit duplicate headers |
| F-DX-3 | correctness | LOW | `navigator.platform` deprecated; needs `userAgentData` fallback |
| F-DX-5 | security | LOW | Dev-seam methods `pub` without feature gate — DevCryptoProvider reachable from prod builds |
| F-DX-7 | test | LOW | Telemetry command test vacuously true on fresh engine — proves nothing |
| F-C1 | CI | HIGH | JVM gate broken — wrong dir, no gradlew, `\|\| true` masks failures · ✅ fixed 2026-09-01 (gradlew created) |
| F-D6 | protocol | INFO | Priority forwarding unchecked — ✅ resolved (iris-core already gates via MessagePriority + serves_priority) |
| F-S1 | security | INFO | WebView CSP null — ✅ fixed (CSP string set in tauri.conf.json) |
| F-S3 | security | PASS | Capability scoping verified clean |
| F-S4 | build | INFO | Bundle inactive — future DESKTOP-002 |
| F-W2 | supply-chain | PASS | Lock ↔ deny.toml skip consistency verified pass |
| F-P3 | documentation | FLAG | Orphan layer docs — ✅ fixed (provenance banners added to both layer docs) |
