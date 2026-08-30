# Section 4 (Android App) — Autonomous Bug-Fix Loop Specification

**Reviewer:** Sohan
**Source review:** `ANDROID_SECTION4_ALL_BUGS.md` (16 confirmed findings, AN-1..AN-16)
**Companion documents:**
- [`sohan_problems.md`](sohan_problems.md) — authoritative bug tracker (status, tiers, detail per finding)
- [`sohan_fix_log.md`](sohan_fix_log.md) — execution journal (one entry per run)
- **Parallel tracker:** [`taksh_problems.md`](taksh_problems.md) · [`taksh_fix_log.md`](taksh_fix_log.md)

**Mandate:** Fix all 16 Android Section 4 bugs in tier order, maintain both the `sohan_*` and `taksh_*` tracker files in sync after every status change, and never break the Android build.

---

## §1 — Reading this file

This file defines the **how** of executing the loop. `sohan_problems.md` defines the **what** (the authoritative list, tier membership, and current status of every finding). `sohan_fix_log.md` records **what happened** in each run.

The three files are a unit: every run reads all three at the start, every run updates all three before it closes.

---

## §2 — Session protocol

### §2.1 — Pre-run checklist (do before writing any code)

1. Read `sohan_problems.md` → identify active tier and all ⬜ Not started findings in it.
2. Read `sohan_fix_log.md` → confirm "Next run" target from the last closed run.
3. Read `taksh_problems.md` and `taksh_fix_log.md` → confirm both are up to date with sohan's tracker; note any drift.
4. Verify the cited file/line for each target finding against HEAD (drift check). Record findings in the run's "Drift notes" section.
5. Run `./gradlew assembleDebug` and `./gradlew test` to establish the pre-run baseline. Record pass counts.

### §2.2 — Per-finding fix protocol

For each target finding in the run:

1. Read the full finding entry in `sohan_problems.md` — understand what, where, why, blast radius.
2. Verify the cited location exists and matches the description.
3. Write the fix. Keep it minimal — no refactors, no cleanup beyond what the finding requires.
4. Run `./gradlew assembleDebug`. If build fails, fix the build before proceeding to the next finding.
5. Run `./gradlew test`. Record pass count. If it regresses vs. baseline, fix the regression before continuing.
6. Update `sohan_problems.md`: change the finding's status from ⬜ to ✅ (or 🔒 if blocked).
7. Update `taksh_problems.md`: mirror the status change to the same AN-* row.
8. Add the run entry row to `sohan_fix_log.md`.
9. Commit. Commit message must reference the AN-* ID(s), e.g. `fix(android): AN-3 gradle.properties split-line`.

### §2.3 — End-of-run checklist

1. Full build sweep: `./gradlew assembleDebug` must be clean.
2. Full test sweep: `./gradlew test` pass count ≥ pre-run baseline.
3. Sync both tracker files (`sohan_problems.md` and `taksh_problems.md`) — progress table rows must match reality.
4. Fill in the run's table in `sohan_fix_log.md` with final status, commit hash, and verification notes.
5. Write the "Next run" target for the following run.
6. Commit documentation changes (tracker + log updates) together or alongside the code change commit.

### §2.4 — Blocked findings

If a finding cannot be fixed in the current run (unresolved dependency, missing NDK toolchain, Section 3 coordination needed), mark it 🔒 in `sohan_problems.md`, mirror to `taksh_problems.md`, add a brief block reason in the log row, and skip it. Do not leave a partial fix uncommitted.

### §2.5 — Tier gate — NO hard stop between any tiers

Unlike the Rust core loop (rahul_problems.md), there is no human gate between Android Section 4 tiers. Proceed Tier 0 → 1 → 2 automatically after each tier's findings are ✅ or 🔒. The only check is: every finding in the tier must be resolved before advancing.

---

## §3 — Build environment

| Command | Purpose |
|---|---|
| `./gradlew assembleDebug` | Full Android debug build — must pass before any commit |
| `./gradlew test` | JVM unit tests (no device needed) — must not regress |
| `./gradlew connectedAndroidTest` | On-device integration tests — run if device available; mark `PENDING DEVICE-TEST` otherwise |
| `./gradlew lint` | Lint sweep — run after each batch; warn on new issues, don't block commit on pre-existing lint |
| `cargo build --release --target aarch64-linux-android` | Rebuild the native `.so` (AN-2) — requires NDK toolchain configured |

**NDK requirement:** AN-2 requires the Android NDK (`ndk-bundle` or standalone NDK) and `cargo-ndk` or equivalent. If the NDK is not available in the current environment, mark AN-2 🔒 and note `PENDING NDK-ENV`.

**CI gap:** AN-4 means no machine-verified baseline until that fix lands. All findings carry `PENDING ANDROID-CI` until a green CI run is confirmed post-AN-4 fix.

---

## §4 — Scope boundaries

**In scope — may edit freely:**
- `android/app/src/main/` (Kotlin, Java, XML, resources)
- `android/gradle.properties`, `android/build.gradle`, `android/app/build.gradle`
- `android/app/proguard-rules.pro`
- `crates/iris-android/src/` (Android-facing Rust)
- `.github/workflows/ci.yml` — only the Android test step (AN-4); coordinate with CI owner before merging

**Read-only context (do not modify):**
- `crates/iris-core/src/` — read to understand interfaces; fixes to core are a separate Section owner's concern
- `crates/iris-desktop/src/` — read for Desktop counterpart patterns (especially crypto wiring for AN-1)

**Never touch:**
- Files outside the above paths
- `security/mod.rs` or any signing/verification core logic directly
- Any file outside `C:\Users\sohan\Desktop\cd\IRIS-clone`

---

## §5 — Safety limits

- **Max findings per run:** 4 (to keep each run reviewable and diffs small)
- **Never break the build:** if `./gradlew assembleDebug` was clean before the run, it must be clean after
- **Never silently discard a regression:** if test count drops, fix it before the next finding
- **Commit granularity:** one commit per finding, or one commit for findings explicitly grouped (e.g., AN-7+AN-12 — the BLE pair)
- **No cross-tier skipping:** do not fix a Tier 2 finding while Tier 0 findings remain ⬜

---

## §6 — Coordinated findings

The following findings have documented dependencies or require careful ordering:

| Finding | Dependency | Rule |
|---|---|---|
| AN-6 (X25519 wiring) | AN-1 (real crypto provider) | Fix AN-1 first; AN-6 wiring only makes sense with real crypto in place |
| AN-2 (.so rebuild) | AN-1 (ideally) | Rebuild with real crypto already in the binary — best if AN-1 lands first |
| AN-7 (BLE start/stop) | AN-12 (BLE sync) | Fix together in one commit — share the same mutex change |
| AN-5 (SOS content type) | — | Affects both Android and Desktop engines — fix both in the same commit |
| AN-4 (CI path) | — | Coordinate with CI owner; flag in PR description that this is a test-infra fix |

---

## §7 — Sync protocol with Taksh's tracker

Every status change in `sohan_problems.md` **must** be mirrored to `taksh_problems.md` in the same commit. The cross-reference header in both files reads:

> "both files must be updated on every status change"

Practically: after marking a finding ✅ in `sohan_problems.md`, open `taksh_problems.md` and update:
1. The finding's **Fix status** line.
2. The **Progress Tracker** table row for the relevant tier.
3. The **Last updated** line at the top.

Do the same in reverse if Taksh's loop fixes something before Sohan does.

---

## §8 — Tier plan (authoritative tier membership in `sohan_problems.md`)

| Tier | Findings | Rationale |
|---|---|---|
| 0 | AN-1, AN-2 | Live safety/security defects — fake crypto, outdated native binary |
| 1 | AN-3, AN-4, AN-5, AN-6 | High correctness + medium security/protocol |
| 2 | AN-7, AN-8, AN-9, AN-10, AN-11, AN-12, AN-13, AN-14 | Medium/low correctness, UX, build polish |
| N/A | AN-15, AN-16 | Informational — no code fix required; track awareness only |

Expected run plan:
- **Run 1:** Tier 0 (AN-1, AN-2)
- **Run 2:** Tier 1 (AN-3, AN-4, AN-5, AN-6)
- **Run 3:** Tier 2 batch A (AN-7+AN-12, AN-8, AN-9, AN-10)
- **Run 4:** Tier 2 batch B (AN-11, AN-13, AN-14) + N/A review

Total expected runs: 4. No human gate required between tiers.
