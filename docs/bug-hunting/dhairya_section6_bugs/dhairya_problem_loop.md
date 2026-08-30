# Section 6 (Desktop App & Build Infrastructure) — Autonomous Loop Specification

**Reviewer:** Dhairya (Section 6 — Desktop App & Build Infrastructure)
**Companion:** [`dhairya_problem.md`](dhairya_problem.md) (state tracker) · [`dhairya_problem_log.md`](dhairya_problem_log.md) (execution journal)
**Fix brief:** [`dhairya_problems.md`](dhairya_problems.md) — exact code replacements, per finding

---

## §1 Reading this file

This file tells the loop agent **how to execute** Section 6 fixes autonomously. Before starting any run:

1. Read [`dhairya_problem.md`](dhairya_problem.md) — current fix status for all 28 findings.
2. Read [`dhairya_problem_log.md`](dhairya_problem_log.md) — what previous runs attempted and verified.
3. Read [`dhairya_problems.md`](dhairya_problems.md) — exact diff specs for each finding.
4. Read the relevant source file(s) from the code listing in §3 before editing — code at HEAD may have drifted from the review snapshot. Note any drift in the log before applying the fix.
5. Never guess from memory. Always read the current file before writing.

---

## §2 Session protocol

### Pre-run checklist (run once at the very start of each session)

- [ ] Read `dhairya_problem.md` and identify which tier is currently active.
- [ ] Read `dhairya_problem_log.md` — check if a previous run left anything in-progress or blocked.
- [ ] Run the baseline commands (§3) and record their output in the log before any edits.
- [ ] Confirm the working directory is `C:\Users\sohan\Desktop\cd\IRIS-clone` (Windows) / `/c/Users/sohan/Desktop/cd/IRIS-clone` (Git Bash).
- [ ] Confirm git is clean (`git status`) — if there are uncommitted changes from a prior session, stash or commit them before proceeding.

### Per-finding fix protocol

For each finding in the active tier batch:

1. **Read the fix spec** in `dhairya_problems.md` (exact section for the finding).
2. **Read the target source file(s)** at HEAD — note any drift from the spec.
3. **Apply the fix** using the Edit tool (prefer Edit over Write for existing files).
4. **Verify the fix** using the verification command from the fix spec or the log template.
5. **Record in `dhairya_problem_log.md`**: status, commit SHA, verification evidence.
6. **Update `dhairya_problem.md`**: change the finding's Fix status from ⬜ to ✅ (or 🔒 if blocked).

### End-of-run checklist

- [ ] Run all baseline commands (§3) — all must pass before closing the run.
- [ ] Update the Progress Tracker table in `dhairya_problem.md` (✅ counts).
- [ ] Write the batch closeout section in the log entry.
- [ ] If advancing to the next tier, confirm the tier gate (§2 Tier gate, below).
- [ ] Commit all tracker/log file updates in the same commit as the code changes, or in a clean-up commit immediately after.

### Blocked findings

A finding is blocked (🔒) when:
- A prerequisite finding in a higher tier is not yet fixed.
- The fix requires an external tool or environment not available in this session (e.g., NDK toolchain, network-only SHA resolution).
- The fix scope crosses a boundary listed in §4 (not this section's work).

When a finding is blocked:
- Record the block reason in the log.
- Do NOT apply a partial fix.
- Do NOT move to the next finding in the same tier — attempt remaining findings in the batch; only skip if they depend on the blocked one.
- Leave Fix status as 🔒 in `dhairya_problem.md`.

### Tier gate

Do not advance to the next tier until **all findings in the current tier** are either ✅ or 🔒 with a documented reason, AND all baseline build/test commands (§3) pass.

**Tier 0 → Tier 1 gate:** F-C2 ✅, F-DX-1 ✅, F-DX-6 ✅ (or 🔒 with reason). `cargo deny check` must exit 0.
**Tier 1 → Tier 2 gate:** F-C3 ✅, F-D1 ✅, F-P1 ✅, F-DX-4 ✅ (or 🔒 with reason). New inbox pump tests must be GREEN.
**Tier 2 complete gate:** All 14 Tier 2 findings ✅ or 🔒. `cargo build --workspace`, `cargo test -p iris-desktop --features test-seams`, `cargo clippy`, `cargo fmt` all pass.

---

## §3 Build environment

| Command | When to run | Expected outcome |
|---|---|---|
| `cargo build --workspace` | Pre-run baseline + after each batch | CLEAN (exit 0) |
| `cargo test -p iris-desktop` | Pre-run baseline + after Tier 1 | All pass (record count) |
| `cargo test -p iris-desktop --features test-seams` | After F-DX-5 lands | All integration tests pass |
| `cargo deny check` | Pre-run baseline + after F-C2 + after F-P2 | Exit 0 after F-C2 fix |
| `cargo audit` | Pre-run baseline | Exit 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | After each batch | Exit 0, 0 warnings |
| `cargo fmt --all -- --check` | End-of-run final check | Exit 0 |

**Working directory for all commands:** `C:\Users\sohan\Desktop\cd\IRIS-clone` (Windows PowerShell) or `/c/Users/sohan/Desktop/cd/IRIS-clone` (Git Bash).

**SHA pin resolution for F-C4:**
```bash
git ls-remote https://github.com/dtolnay/rust-toolchain.git HEAD
git ls-remote https://github.com/Swatinem/rust-cache.git HEAD
git ls-remote https://github.com/taiki-e/install-action.git HEAD
git ls-remote https://github.com/actions/upload-artifact.git HEAD
git ls-remote https://github.com/actions/checkout.git HEAD
```
If any resolution fails (no network), insert `# TODO(security): pin SHA - resolve via git ls-remote` on the offending line and move on.

---

## §4 Scope boundaries

### In scope (editable files)

```
.github/workflows/ci.yml
.config/nextest.toml
deny.toml
engineering/SECURITY_POLICY.yaml
Cargo.toml                        (workspace.dependencies section only)
crates/iris-android/Cargo.toml    (dependency lines only)
crates/iris-ios/Cargo.toml        (dependency lines only)
crates/iris-desktop/Cargo.toml
crates/iris-desktop/src/engine_handle.rs
crates/iris-desktop/src/types.rs
crates/iris-desktop/tests/
crates/iris-desktop/windows-app-manifest.xml
crates/iris-desktop/ui/app.js
```

### Read-only context (can read for understanding; do NOT edit)

```
crates/iris-core/           (all files)
crates/iris-storage/        (all files)
android/                    (all Kotlin/Gradle files; Cargo.toml lines only via above)
crates/iris-desktop/src/    (files not listed in scope above)
```

### Never touch (even for reading; do not open, edit, or view)

```
crates/iris-desktop/tauri.conf.json
crates/iris-desktop/capabilities/
crates/iris-desktop/build.rs
AGENTS.md
docs/implementation/PYTHON_LAYER.md
docs/implementation/TYPESCRIPT_LAYER.md
```

---

## §5 Safety limits

- **One finding per Edit call** — do not combine two independent findings into one file write. Exception: F-DX-1 + F-DX-6 MUST be in the same commit (they share an emergency-path fix chain).
- **No invented SHAs** — for F-C4 SHA pins, resolve via `git ls-remote` or leave a TODO comment.
- **No partial fixes** — if the fix spec requires two files to be changed atomically (e.g., DX-1 + DX-6, D1 + D2), apply both before committing.
- **No scope creep** — if the code shows a nearby issue not listed in the 28 findings, note it in the log and do NOT fix it in this run. Flag it for a future session.
- **No destructive git operations** — `git reset --hard`, `git push --force`, `git clean -f` are prohibited without explicit user instruction.
- **No skipping the baseline** — always record baseline command output in the log before any edits.
- **Commit atomicity** — each batch (not each individual finding) should be one commit. Include the log + tracker update in the same commit or in an immediate follow-up commit.

---

## §6 Coordinated findings (must fix together)

| Group | Findings | Reason |
|---|---|---|
| SOS chain | F-DX-1 + F-DX-6 | DX-1 changes the ContentType at send; DX-6 fixes the receiver's text extraction. Fixing one without the other makes SOS worse. Same commit. |
| Inbox pump | F-D1 + F-D2 | D2 (abort handle) is inside D1's pump extraction. Fix in the same commit. D2 must NOT be fixed separately. |
| Size cap + UTF-16 | F-D3 + F-D7 | Both edit the same region of engine_handle.rs (size-cap check, ~line 385). Fix in the same commit. |
| Deny + license | F-C2 + F-P2 | Both edit deny.toml. Can be the same commit or sequential; F-C2 must come first (F-P2 adds Unlicense, depends on F-C2 clearing the existing advisory FAIL). |

---

## §7 Sync protocol

Dhairya works Section 6. Sohan works Section 4 (Android). There are no shared file edits between the two sections except for the following cross-section awareness points:

- **F-DX-1 mirrors AN-5** — the Android AN-5 fix corrected `ContentType::Sos` for Android. F-DX-1 applies the same correction to the desktop engine. No shared file; just pattern parity.
- **F-C2 (deny.toml)** — only Section 6 edits `deny.toml`. If Sohan's section produces a new dependency that lands in `Cargo.lock`, confirm `cargo deny check` still passes after merging.
- **F-W1 (Cargo.toml workspace.dependencies)** — editing the root `Cargo.toml`. If Sohan's AN-2 work also adds workspace dependencies in the same sprint, coordinate the Cargo.toml edit to avoid conflicts.
- **CI file (.github/workflows/ci.yml)** — F-C3 and F-C4 both edit `ci.yml`. Apply F-C3 first (system libs — immediate correctness fix), then F-C4 (hardening — SHA pins, permissions). Merge as one PR or in close sequence.

If a conflict occurs with Sohan's section on any of the above files:
1. Read both diffs.
2. Merge manually — do NOT `git checkout --theirs` or `git checkout --ours`.
3. Run `cargo build --workspace` + `cargo deny check` + `cargo clippy` to verify the merged result.
4. Record the resolution in the log.

---

## §8 Tier plan

| Tier | Findings (in fix order) | Gate to next tier |
|---|---|---|
| 0 | F-C2 → F-DX-1 + F-DX-6 (same commit) | `cargo deny check` exit 0; all three ✅ |
| 1 | F-C3 → F-D1 + F-D2 (same commit) → F-P1 → F-DX-4 | inbox_pump tests GREEN; all four ✅ |
| 2A | F-C4 → F-C5 → F-D3 + F-D7 (same commit) → F-D4 | build CLEAN; clippy clean |
| 2B | F-D5 → F-S2 → F-W1 → F-P2 | `cargo deny check` still exit 0 |
| 2C | F-DX-2 → F-DX-3 → F-DX-5 → F-DX-7 | `cargo test -p iris-desktop --features test-seams` GREEN |
| N/A | F-C1, F-D6, F-S1, F-S3, F-S4, F-W2, F-P3 | No action — verified / routed / deferred |

**Total actionable:** 21 findings across Tiers 0–2.
**Total no-action:** 7 findings.
**Grand total:** 28 findings.
