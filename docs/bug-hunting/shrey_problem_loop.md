# Section 5 (iOS App) — Autonomous Loop Specification

**Reviewer:** Shrey (Section 5 — iOS App)
**Companion:** [`shrey_problem.md`](shrey_problem.md) (state tracker) · [`shrey_problem_log.md`](shrey_problem_log.md) (execution journal)
**Full bug report:** [`shrey_problems.md`](shrey_problems.md) — complete evidence, code, and solution per finding (72 bugs)

---

## §1 Reading this file

This file tells the loop agent **how to execute** Section 5 iOS fixes autonomously. Before starting any run:

1. Read [`shrey_problem.md`](shrey_problem.md) — current fix status for all 72 findings.
2. Read [`shrey_problem_log.md`](shrey_problem_log.md) — what previous runs attempted and verified.
3. Read [`shrey_problems.md`](shrey_problems.md) — full evidence and exact solution per finding.
4. Read the relevant source file(s) from the code listing in §3 before editing — code at HEAD may have drifted from the review snapshot. Note any drift in the log before applying the fix.
5. Never guess from memory. Always read the current file before writing.

---

## §2 Session protocol

### Pre-run checklist (run once at the very start of each session)

- [ ] Read `shrey_problem.md` — identify which tier and batch is currently active.
- [ ] Read `shrey_problem_log.md` — check if a previous run left anything in-progress or blocked.
- [ ] Run the baseline commands (§3) and record their output in the log before any edits.
- [ ] Confirm the working directory is `C:\Users\sohan\Desktop\cd\IRIS-clone` (Windows) / `/c/Users/sohan/Desktop/cd/IRIS-clone` (Git Bash).
- [ ] Confirm git is clean (`git status`) — if there are uncommitted changes from a prior session, stash or commit them before proceeding.

### Per-finding fix protocol

For each finding in the active tier batch:

1. **Read the bug entry** in `shrey_problems.md` (the full `## Detailed Findings` block for that bug number).
2. **Read the target source file(s)** at HEAD — note any drift from the spec.
3. **Apply the fix** using the Edit tool (prefer Edit over Write for existing files).
4. **Verify the fix** using the verification note from the log template or the solution in `shrey_problems.md`.
5. **Record in `shrey_problem_log.md`**: status, commit SHA, verification evidence.
6. **Update `shrey_problem.md`**: change the finding's Fix status from ⬜ to ✅ (or 🔒 if blocked).

### End-of-run checklist

- [ ] Run all applicable baseline commands (§3) — all must pass before closing the run.
- [ ] Update the Progress Tracker table in `shrey_problem.md` (✅ counts).
- [ ] Write the batch closeout section in the log entry.
- [ ] If advancing to the next tier, confirm the tier gate (§2 Tier gate, below).
- [ ] Commit all tracker/log file updates in the same commit as the code changes, or in a clean-up commit immediately after.

### Blocked findings

A finding is blocked (🔒) when:
- A prerequisite finding in a higher tier is not yet fixed (e.g., Bug #43 depends on Bug #31).
- The fix requires a specific Xcode environment or iOS Simulator not available in this session.
- The fix scope crosses a boundary listed in §4 (not this section's work).

When a finding is blocked:
- Record the block reason in the log.
- Do NOT apply a partial fix.
- Attempt remaining findings in the batch; only skip if they depend on the blocked one.
- Leave Fix status as 🔒 in `shrey_problem.md`.

### Tier gate

Do not advance to the next tier until **all findings in the current tier** are either ✅ or 🔒 with a documented reason, AND the build commands for that tier pass.

**Tier 0 batch A → batch B gate:** Bugs #1–#5, #10 all ✅. `xcodebuild build` must be CLEAN (no compile errors).
**Tier 0 → Tier 1 gate:** All 10 Critical findings ✅. `xcodebuild build` CLEAN. `xcodebuild test` produces a test result (even if some tests fail — they can fail for Tier 1 reasons).
**Tier 1 → Tier 2 gate:** All 18 High findings ✅ or 🔒. `xcodebuild build` CLEAN. `cargo build -p iris-ios` CLEAN.
**Tier 2 → Tier 3 gate:** All 30 Medium findings ✅ or 🔒. `xcodebuild test` pass count stable or improving.
**Tier 3 complete gate:** All 14 Low findings ✅ or 🔒. `cargo clippy -p iris-ios -- -D warnings` exit 0. `cargo fmt -p iris-ios -- --check` exit 0.

---

## §3 Build environment

| Command | When to run | Expected outcome |
|---|---|---|
| `cargo build -p iris-ios` | Pre-run baseline + after every Rust edit | CLEAN (exit 0) |
| `cargo clippy -p iris-ios -- -D warnings` | After each Rust batch | Exit 0, 0 warnings |
| `cargo fmt -p iris-ios -- --check` | End-of-run final check | Exit 0 |
| `cargo test -p iris-ios` | After Rust FFI changes | All pass |
| `xcodebuild -workspace ios/IRIS.xcworkspace -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16' build` | After each Swift batch | CLEAN — critical gates at Tier 0 batch A and B |
| `xcodebuild test -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16'` | After Tier 0 complete + after each subsequent tier | Record pass count; must not regress |
| `bash ios/Scripts/build-xcframework.sh` | After any change to `crates/iris-ios/` | CLEAN; produces xcframework artifact |

**Working directory for all commands:** `C:\Users\sohan\Desktop\cd\IRIS-clone` (Windows PowerShell) or `/c/Users/sohan/Desktop/cd/IRIS-clone` (Git Bash).

**Xcode requirement:** Xcode 16.4+ (required by `IPHONEOS_DEPLOYMENT_TARGET 18.0` in `ios/project.yml`). If Xcode is not available, record `PENDING CI` for Swift build/test steps and continue with Rust-only verification.

---

## §4 Scope boundaries

### In scope (editable files)

```
ios/IRIS/App/AppDelegate.swift
ios/IRIS/App/IRISApp.swift
ios/IRIS/BLE/IosBleAdapter.swift
ios/IRIS/BLE/CBManagerCentral.swift
ios/IRIS/BLE/CBManagerPeripheral.swift
ios/IRIS/BLE/IrisBleConstants.swift
ios/IRIS/Background/BGTaskWiring.swift
ios/IRIS/Keychain/KeychainEd25519.swift
ios/IRIS/Session/SessionRecovery.swift
ios/IRIS/LiveActivity/LiveActivityController.swift
ios/IRIS/Notifications/Notifications.swift
ios/IrisLiveActivityWidget/IrisLiveActivityWidget.swift
ios/IrisWidgetExtension/Info.plist
ios/IRIS/Resources/Info.plist
ios/IRIS/Resources/PrivacyInfo.xcprivacy      (new file for Bug #72)
ios/project.yml
ios/G-IOS_SPIKE.md
ios/Scripts/build-xcframework.sh
ios/Tests/KeychainIdentityTests.swift
ios/Tests/SessionRecoveryTests.swift
ios/Tests/ProbeAdmissionTests.swift
ios/Tests/IosBleAdapterTests.swift
ios/Tests/MockCoreBluetooth.swift
crates/iris-ios/src/engine.rs
crates/iris-ios/src/ffi/bridge.rs
crates/iris-ios/src/ffi/ble_adapter.rs
crates/iris-ios/src/ffi/body.rs
crates/iris-ios/src/ffi/error.rs
crates/iris-ios/Cargo.toml
.github/workflows/ios.yml
```

### Read-only context (can read for understanding; do NOT edit)

```
crates/iris-core/                 (all files)
crates/iris-storage/              (all files)
crates/iris-desktop/              (all files — different section)
android/                          (all files — different section)
```

### Never touch (even for reading; do not open, edit, or view)

```
AGENTS.md
docs/implementation/PYTHON_LAYER.md
docs/implementation/TYPESCRIPT_LAYER.md
crates/iris-desktop/tauri.conf.json
crates/iris-desktop/capabilities/
```

---

## §5 Safety limits

- **One finding per Edit call** — do not combine two independent findings into one file write. Exception: coupled findings must be fixed in the same commit (see §6).
- **No partial fixes** — if the fix spec requires two files to be changed atomically, apply both before committing.
- **No scope creep** — if the code shows a nearby issue not listed in the 72 findings, note it in the log and do NOT fix it in this run. Flag it for a future session.
- **No destructive git operations** — `git reset --hard`, `git push --force`, `git clean -f` are prohibited without explicit user instruction.
- **No skipping the baseline** — always record baseline command output in the log before any edits.
- **Commit atomicity** — each batch (not each individual finding) should be one commit. Include the log + tracker update in the same commit or in an immediate follow-up commit.
- **Tier 0 first** — do not touch any Tier 1–3 finding until all 10 Tier 0 findings are ✅ or 🔒. The build does not compile until Tier 0 batch A is complete; running later-tier fixes on a broken build produces misleading results.

---

## §6 Coordinated findings (must fix together)

| Group | Findings | Reason |
|---|---|---|
| SOS / UUID case | #6 + any UUID-comparison test fix | Changing `lowercased()` → `uppercased()` invalidates any test that hardcoded lowercase UUIDs. Fix tests in same commit. |
| Keychain storage format | #31 + #43 | Bug #43 test assertion depends on Bug #31 raw-byte storage format. Fix #31 first; update #43 assertion in same commit. |
| Dev seam gate | #41 + any test using `MemoryStorage`/`DevCryptoProvider` | Gating behind `dev-seams` feature breaks tests that use these types. Add `--features dev-seams` to test invocations in same commit. |
| xcframework path | #54 + `ios.yml` reference | `build-xcframework.sh` and `ios.yml:72` must agree on the output path. Fix both in same commit. |
| `uuid_to_hex` case | #44 + `SimBle` test alignment (#39) | After fixing #44 (uppercase hex), `SimBle` test UUIDs must also be uppercase. Fix together or sequentially in the same PR. |

---

## §7 Sync protocol

Shrey works Section 5 (iOS). Sohan works Section 4 (Android). Dhairya works Section 6 (Desktop). There are no shared file edits between sections except:

- **`Cargo.toml` (root workspace)** — if Bug #64 (`crates/iris-ios/Cargo.toml` crate-type) triggers a workspace dependency rebuild that touches the root `Cargo.toml`, coordinate with Dhairya's Section 6 fixes (F-W1) to avoid merge conflicts.
- **`iris-core` interfaces** — Bug #17 (inbox forwarder) and Bug #47 (`received_at_ms` naming) touch `crates/iris-ios/src/engine.rs` which calls into `iris-core` APIs. Read `iris-core` interfaces for understanding; do NOT edit `iris-core` files.
- **CI file** — `ios.yml` is Section 5 only. `.github/workflows/ci.yml` is Section 6. Do NOT mix edits between the two workflow files.

If a merge conflict occurs on any shared file:
1. Read both diffs.
2. Merge manually — do NOT `git checkout --theirs` or `git checkout --ours`.
3. Run `cargo build -p iris-ios` + `xcodebuild build` to verify the merged result.
4. Record the resolution in the log.

---

## §8 Tier plan

| Tier | Findings (in fix order) | Batch | Gate to next |
|---|---|---|---|
| 0A | #1 → #2 → #3 → #4 → #5 → #10 | Build breaks — Swift + test compilation | `xcodebuild build` CLEAN |
| 0B | #6 → #7 → #8 → #9 | Runtime emergency-path: UUID case, GATT errors, delegate, UUID collision | `xcodebuild build` + tests run |
| 1A | #11 → #12 → #13 → #14 → #15 → #16 | Crashes + AppDelegate: restore, classify, BG task, identity race | Build CLEAN + tests stable |
| 1B | #17 → #18 → #19 → #20 → #21 → #22 | Leaks + races: inbox forwarder, retain cycle, CB queue, maps | Build CLEAN + Rust CLEAN |
| 1C | #23 → #24 → #25 → #26 → #27 → #28 | Remaining high: handles, BG task, AppDelegate shared, mock deadlock, overflow | Build CLEAN + tests stable |
| 2A | #29 → #30 → #31 → #32 → #33 → #34 → #35 → #36 → #37 → #38 → #39 → #40 → #41 → #42 → #43 → #44 | Medium batch A — 16 findings | Rust CLEAN + build CLEAN |
| 2B | #45 → #46 → #47 → #48 → #49 → #50 → #51 → #52 → #53 → #54 → #55 → #56 → #57 → #58 | Medium batch B — 14 findings | Rust CLEAN + build CLEAN + tests stable |
| 3 | #59 → #60 → #61 → #62 → #63 → #64 → #65 → #66 → #67 → #68 → #69 → #70 → #71 → #72 | All 14 low findings | clippy + fmt exit 0; CI green |

**Total actionable:** 72 findings across Tiers 0–3.
**Total no-action:** 0 findings (all 72 have fixes; none are marked N/A or routed).
**Grand total:** 72 findings.
