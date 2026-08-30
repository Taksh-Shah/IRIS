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

## Run 1 — [DATE] — Tier 0 batch (F-C2, F-DX-1, F-DX-6)

Target findings: F-C2 (HIGH, advisory gate divergence), F-DX-1 (HIGH, desktop SOS content type), F-DX-6 (MEDIUM, SOS text invisible — coupled to DX-1).

**Pre-run baseline:**
- `cargo build --workspace`: _(record outcome)_
- `cargo test -p iris-desktop`: _(record pass count)_
- `cargo deny check`: _(record FAIL reason — expected 17 advisory DENY lines)_
- `cargo audit`: _(record exit code — expected 0)_

**Drift notes:** _(fill in any differences found between the fix spec and actual code at HEAD)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C2 | ⬜ | — | _(pre: `cargo deny check` FAIL; post: exit 0)_ |
| 2 | F-DX-1 | ⬜ | — | _(grep `ContentType::Sos` in engine_handle.rs:398; confirm conditional on P0)_ |
| 3 | F-DX-6 | ⬜ | — | _(grep `matches!` in types.rs:44; confirm Sos in match arm; same commit as DX-1)_ |

**Batch closeout:**
- `cargo build --workspace`: _(must be CLEAN)_
- `cargo test -p iris-desktop`: _(must match or exceed baseline pass count)_
- `cargo deny check`: _(must exit 0)_
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: _(must exit 0)_

**Tier 0 checkpoint:** _/3 ✅ — advance to Tier 1 when all three green._

### Next run
Tier 1: F-C3 (Ubuntu CI system libs), F-D1 (inbox pump extraction + test), F-P1 (SECURITY_POLICY annotation), F-DX-4 (forwarder silent exit log).

---

## Run 2 — [DATE] — Tier 1 batch (F-C3, F-D1, F-P1, F-DX-4)

Target findings: F-C3 (MEDIUM, Ubuntu CI missing GTK/webkit libs), F-D1 (MEDIUM, inbox pump — includes F-D2), F-P1 (MEDIUM, SECURITY_POLICY annotation), F-DX-4 (MEDIUM, forwarder silent exit).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C3 | ⬜ | — | _(check `apt-get install` step in test/lint/coverage jobs; guarded by `runner.os == 'Linux'`)_ |
| 2 | F-D1 + F-D2 | ⬜ | — | _(confirm `pump_inbox` free function; `inbox_task: Mutex<Option<AbortHandle>>` field; new tests/inbox_pump.rs GREEN)_ |
| 3 | F-P1 | ⬜ | — | _(grep `# v1 DEVIATION` in SECURITY_POLICY.yaml after key_storage rule line; YAML must still parse)_ |
| 4 | F-DX-4 | ⬜ | — | _(grep `tracing::warn!` in engine_handle.rs after the forwarder loop body)_ |

**Batch closeout:**
- `cargo build --workspace`: _(must be CLEAN)_
- `cargo test -p iris-desktop`: _(new inbox_pump tests must appear and pass)_
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: _(must exit 0)_

**Tier 1 checkpoint:** _/4 ✅ — advance to Tier 2 when all four green._

### Next run
Tier 2 batch A: F-C4 (CI hardening), F-C5 (nextest comment), F-D3+F-D7 (size-cap copy + UTF-16 note), F-D4 (malformed relay log).

---

## Run 3 — [DATE] — Tier 2 batch A (F-C4, F-C5, F-D3, F-D7, F-D4)

Target findings: F-C4 (LOW, CI supply-chain hardening sub-fixes a–e), F-C5 (LOW, nextest retries comment), F-D3 (LOW, size-cap copy), F-D7 (LOW, UTF-16 note), F-D4 (LOW, malformed relay silent drop).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-C4 | ⬜ | — | _(confirm: permissions block, timeout-minutes, SHA pins or TODO comment, Kani pin, fuzz cache comment)_ |
| 2 | F-C5 | ⬜ | — | _(grep nextest.toml: comment must say retries = 0, not "2 times")_ |
| 3 | F-D3 + F-D7 | ⬜ | — | _(grep `MAX_TEXT_BYTES` in engine_handle.rs; error string says "60,000"; UTF-16 comment present)_ |
| 4 | F-D4 | ⬜ | — | _(grep `tracing::warn!` in parse_relay_env; for-loop instead of filter_map)_ |

**Batch closeout:**
- `cargo build --workspace`: _(must be CLEAN)_
- `cargo test -p iris-desktop`: _(must pass)_
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: _(must exit 0)_

### Next run
Tier 2 batch B: F-D5 (peer-list prefill), F-S2 (Windows manifest), F-W1 (workspace dep pins), F-P2 (license allowlists).

---

## Run 4 — [DATE] — Tier 2 batch B (F-D5, F-S2, F-W1, F-P2)

Target findings: F-D5 (LOW, peer-list prefill UX), F-S2 (LOW, Windows manifest elevation), F-W1 (LOW, workspace dep pins + dead dep), F-P2 (LOW, license allowlists).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-D5 | ⬜ | — | _(grep `/to @short:` in app.js:494–498; old `/to ${id}` pattern must be gone)_ |
| 2 | F-S2 | ⬜ | — | _(grep `requestedExecutionLevel` in windows-app-manifest.xml; `asInvoker` present)_ |
| 3 | F-W1 | ⬜ | — | _(grep uniffi/uniffi_bindgen in workspace.dependencies; android/ios use `.workspace = true`; desktop Cargo.toml tempfile is workspace; serde_json dev-dep gone)_ |
| 4 | F-P2 | ⬜ | — | _(grep Unlicense in deny.toml licenses.allow; grep Unlicense in SECURITY_POLICY.yaml allowed_licenses; cargo deny check still exit 0)_ |

**Batch closeout:**
- `cargo build --workspace`: _(must be CLEAN)_
- `cargo deny check`: _(must exit 0 — Unlicense now in allow list)_
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: _(must exit 0)_

### Next run
Tier 2 batch C: F-DX-2 (palette dedup), F-DX-3 (navigator.platform), F-DX-5 (dev-seam feature gate), F-DX-7 (vacuous telemetry test).

---

## Run 5 — [DATE] — Tier 2 batch C (F-DX-2, F-DX-3, F-DX-5, F-DX-7)

Target findings: F-DX-2 (LOW, renderPalette group dedup), F-DX-3 (LOW, deprecated platform API), F-DX-5 (LOW, dev-seam feature gate), F-DX-7 (LOW, vacuous telemetry test).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | F-DX-2 | ⬜ | — | _(grep `seenGroups` in app.js:178–188; old `lastGroup` pattern must be gone)_ |
| 2 | F-DX-3 | ⬜ | — | _(grep `userAgentData` in app.js:14; old `navigator.platform` used only as fallback)_ |
| 3 | F-DX-5 | ⬜ | — | _(grep `#[cfg(feature = "test-seams")]` on with_node_id and with_transport; `test-seams` in iris-desktop Cargo.toml features; integration tests run with `--features test-seams`)_ |
| 4 | F-DX-7 | ⬜ | — | _(grep commands_mock.rs:83; assert on at least one counter with value > 0 after a send; not vacuously-true)_ |

**Batch closeout:**
- `cargo build --workspace`: _(must be CLEAN)_
- `cargo test -p iris-desktop --features test-seams`: _(must pass; integration tests in tests/ must see the dev-seam methods)_
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: _(must exit 0)_
- `cargo fmt --all -- --check`: _(must exit 0)_

**Final checkpoint:** All 21 actionable findings ✅, all 7 N/A findings confirmed. Section 6 closed.

### Next run
No further autonomous runs needed. All findings resolved or classified. Record final CI evidence below.

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
