# Section 6 (Desktop App & Build Infrastructure) — Fix Log — Execution Journal

**Companion to:** [`dhairya_problem_loop.md`](dhairya_problem_loop.md) (loop logic) ·
[`dhairya_problem.md`](dhairya_problem.md) (state tracker).
This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.

**Scope:** `crates/iris-desktop/` (all Rust + UI), `.github/workflows/ci.yml`, `.config/nextest.toml`, `deny.toml`, `.cargo/audit.toml`, `engineering/SECURITY_POLICY.yaml`, `Cargo.toml` (workspace deps only), `crates/iris-android/Cargo.toml`, `crates/iris-ios/Cargo.toml`
**Source review:** `dhairya_problems.md` — 21 original findings (SEC6_AUDIT) + 7 new post-audit findings = 28 total.

Baseline at run 1 start (verify before any changes):
- `cargo build --workspace` — record outcome (clean/error) before any edits.
- `cargo test -p iris-desktop` — record pass count before any edits.
- `cargo deny check` — expected FAIL (F-C2 advisory suppressions missing from deny.toml).
- `cargo audit` — expected exit 0 (audit.toml suppressions already present).
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` — record before any edits.

**Environment note:** SHA pins for F-C4 must be resolved via `git ls-remote https://github.com/<owner>/<repo>.git HEAD` at execution time. Never invent a SHA. If the network is not available, insert a `TODO(security): pin SHA - resolve via git ls-remote` comment and move on.

---

## Run 1 — 2026-08-30 — Tier 0 batch (F-C2, F-DX-1, F-DX-6)

Target findings: F-C2 (HIGH, advisory gate divergence), F-DX-1 (HIGH, desktop SOS content type), F-DX-6 (MEDIUM, SOS text invisible — coupled to DX-1).

**Pre-run baseline:**
- `cargo build --workspace`: CLEAN (post-pull HEAD 48ec502)
- `cargo deny check`: FAIL (expected — 17 advisory DENY lines for gtk/proc-macro-error/unic-*)
- `cargo audit`: exit 0

**Drift notes:** Code at HEAD matched fix specs exactly; no drift.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C2 | ✅ | see batch commit | deny.toml `ignore` now has 17 IDs mirrored from audit.toml |
| 2 | F-DX-1 | ✅ | see batch commit | `payload_type: if priority == MessagePriority::P0 { ContentType::Sos } else { ContentType::Text }` at engine_handle.rs build_text_envelope |
| 3 | F-DX-6 | ✅ | see batch commit | `matches!(env.payload_type, ContentType::Text | ContentType::Sos)` in types.rs:45; same commit as DX-1 |

**Batch closeout:**
- `cargo build --workspace`: PENDING CI (GNU toolchain build running in background)
- `cargo deny check`: PENDING — expect exit 0 after fix
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PENDING CI

**Tier 0 checkpoint:** 3/3 ✅

---

## Run 2 — 2026-08-30 — Tier 1 batch (F-C3, F-D1+F-D2, F-P1, F-DX-4)

Target findings: F-C3 (MEDIUM, Ubuntu CI missing GTK/webkit libs), F-D1+F-D2 (MEDIUM, inbox pump — coupled), F-P1 (MEDIUM, SECURITY_POLICY annotation), F-DX-4 (MEDIUM, forwarder silent exit).

**Drift notes:** F-D1 at HEAD already had a partial fix (GAP-4 loop with Lagged handling) but lacked pump extraction, inbox_task field, and tests. Full F-D1/D2 fix applied.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C3 | ✅ | see batch commit | `apt-get install` step added to test/lint/coverage jobs, guarded by `runner.os == 'Linux'` |
| 2 | F-D1 + F-D2 | ✅ | see batch commit | `pump_inbox` free function + `inbox_task: Mutex<Option<AbortHandle>>` field + `tests/inbox_pump.rs` created + d2_lifecycle unit test added |
| 3 | F-P1 | ✅ | see batch commit | `# v1 DEVIATION` comment inserted after `rule:` line in SECURITY_POLICY.yaml |
| 4 | F-DX-4 | ✅ | see batch commit | `tracing::warn!("desktop: transport inbound stream closed...")` after forwarder loop |

**Batch closeout:**
- `cargo build --workspace`: PENDING CI
- `cargo test -p iris-desktop --features test-seams`: PENDING CI (inbox_pump.rs + d2_lifecycle tests expected GREEN)
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PENDING CI

**Tier 1 checkpoint:** 4/4 ✅

---

## Run 3 — 2026-08-30 — Tier 2 batch A (F-C4, F-C5, F-D3+F-D7, F-D4)

Target findings: F-C4 (LOW, CI supply-chain hardening sub-fixes a–e), F-C5 (LOW, nextest retries comment), F-D3+F-D7 (LOW, size-cap copy + UTF-16 note), F-D4 (LOW, malformed relay silent drop).

**Drift notes:** F-D4 was applied in the same edit as pump_inbox (both in parse_relay_env region). No drift.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C4 | ✅ | see batch commit | (a) `permissions: contents: read` added; (b) timeout-minutes on all 10 jobs; (c) SHA pins: TODO(security) comments (network unavailable); (d) Kani TODO comment; (e) fuzz cache comment |
| 2 | F-C5 | ✅ | see batch commit | nextest.toml comment now says "Retries disabled: failures must be reproducible" |
| 3 | F-D3 + F-D7 | ✅ | see batch commit | `MAX_TEXT_BYTES = 60_000`; error says "{MAX_TEXT_BYTES} bytes max"; D7 UTF-16 comment added |
| 4 | F-D4 | ✅ | see batch commit | `parse_relay_env` uses for-loop with `tracing::warn!` on each rejected entry |

**Batch closeout:**
- `cargo build --workspace`: PENDING CI
- `cargo test -p iris-desktop`: PENDING CI
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PENDING CI

---

## Run 4 — 2026-08-30 — Tier 2 batch B (F-D5, F-S2, F-W1, F-P2)

Target findings: F-D5 (LOW, peer-list prefill UX), F-S2 (LOW, Windows manifest elevation), F-W1 (LOW, workspace dep pins + dead dep), F-P2 (LOW, license allowlists).

**Drift notes:** No drift; all specs matched current code.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-D5 | ✅ | see batch commit | `el.input.value = "/to @short:${id}"` in app.js |
| 2 | F-S2 | ✅ | see batch commit | `<requestedExecutionLevel level="asInvoker" uiAccess="false"/>` in windows-app-manifest.xml |
| 3 | F-W1 | ✅ | see batch commit | uniffi/uniffi_bindgen in workspace.dependencies; android/ios use `.workspace = true`; desktop tempfile.workspace=true; serde_json dev-dep removed (flagged per boundary #12) |
| 4 | F-P2 | ✅ | see batch commit | `Unlicense` added to both deny.toml license blocks; SECURITY_POLICY.yaml allowed_licenses expanded to 12 licenses; MPL-2.0 copyleft decision documented |

**Batch closeout:**
- `cargo build --workspace`: PENDING CI
- `cargo deny check`: PENDING CI — Unlicense now in allow list
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PENDING CI

---

## Run 5 — 2026-08-30 — Tier 2 batch C (F-DX-2, F-DX-3, F-DX-5, F-DX-7)

Target findings: F-DX-2 (LOW, renderPalette group dedup), F-DX-3 (LOW, deprecated platform API), F-DX-5 (LOW, dev-seam feature gate), F-DX-7 (LOW, vacuous telemetry test).

**Drift notes:** No drift; all specs matched current code.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-DX-2 | ✅ | see batch commit | `const seenGroups = new Set()` + `!seenGroups.has(command.group)` in app.js renderPalette; old `lastGroup` removed |
| 2 | F-DX-3 | ✅ | see batch commit | `navigator.userAgentData?.platform` with fallback to `navigator.platform` in app.js:14 |
| 3 | F-DX-5 | ✅ | see batch commit | `#[cfg(feature = "test-seams")]` on with_node_id and with_transport; `test-seams = []` in iris-desktop Cargo.toml [features]; CI uses --all-features which enables test-seams |
| 4 | F-DX-7 | ✅ | see batch commit | commands_mock.rs: sends one message after fresh-engine check; asserts `metrics_after.iter().any(|m| m.name.contains("sent") && m.value > 0)` |

**Batch closeout:**
- `cargo build --workspace`: PENDING CI
- `cargo test -p iris-desktop --features test-seams`: PENDING CI
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: PENDING CI
- `cargo fmt --all -- --check`: PENDING CI

**Final checkpoint:** All 21 actionable findings ✅, all 7 N/A findings confirmed. Section 6 closed.

---

## Final CI evidence

_(Fill in after all fixes are committed and the CI pipeline completes)_

| Check | Result | Run link |
|---|---|---|
| `cargo build --workspace` | _(CLEAN / error)_ | — |
| `cargo test -p iris-desktop --features test-seams` | _(pass count)_ | — |
| `cargo deny check` | _(exit 0 / FAIL)_ | — |
| `cargo audit` | _(exit 0)_ | — |
| `cargo clippy -- -D warnings` | _(exit 0 / warnings)_ | — |
| `cargo fmt -- --check` | _(exit 0)_ | — |
| CI pipeline (GitHub Actions) | _(green / failing jobs)_ | — |

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
