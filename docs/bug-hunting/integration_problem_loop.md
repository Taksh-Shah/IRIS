# Fix Loop — autonomous execution plan for `integration_problems.md`

**Owner:** Sohan (cross-section integration sweep)
**Companion document:** [`integration_problems.md`](integration_problems.md) — the 8-finding cross-section integration report this loop fixes. Read that document's `Progress Tracker` section for current status; this file is the *logic*, that one is the *state*.
**Purpose:** a self-contained specification an autonomous agent (or a human following it by hand) can execute repeatedly, without needing this conversation's context, to work through all 8 findings tier by tier, committing as it goes, and keeping `integration_problems.md` accurate at every step.

---

## 1. The invariant this loop must never break

**The tree must be green after every commit.** No commit is made unless `cargo build --workspace` succeeds (and, for Android/iOS-touching findings, the platform side compiles as far as this environment can verify — see §4 step 6) and the relevant test scope passes. If a fix cannot be made to pass, it is reverted in the same wake cycle — the loop never leaves a broken build for the next wake or for a human to find. This is non-negotiable and takes priority over "making progress."

**Extra invariant specific to this report:** every finding here crosses an ownership boundary. A fix is not complete just because "a" side compiles — see §4 step 3 for the cross-section sign-off rule this loop must follow that `taksh_problems_loop.md` did not need (Taksh's findings were single-owner).

## 2. Tier order and gating

Run tiers **in this order**. Do not skip ahead.

1. **Tier 0** (1 finding — CROSS-001) — fix first. It is the report's own headline: until Android and iOS use the same BLE service UUID, no cross-platform mesh scenario can be observed working at all, which makes every other finding's real-world impact unverifiable in practice.
2. **Tier 1** (4 findings — CROSS-002, CROSS-003, CROSS-004, CROSS-005) — security regressions, build breaks, and data-integrity bugs. No ordering dependency between these four; fix in ID order for a reviewable `git log`.
3. **Tier 2** (3 findings — CROSS-006, CROSS-007, CROSS-008) — missing wiring and compile breaks on binding regeneration. No ordering dependency on Tier 0/1 or on each other.

Unlike `taksh_problems_loop.md`, this report has no hard-stop human-sign-off gate on any tier — none of these findings change a security *invariant* by themselves (CROSS-003 re-enables an invariant that was supposed to be on already; it does not introduce a new one). Tiers 1 and 2 may run in the same wake if budget allows (see §8).

## 3. Authoritative tier membership

This is the source of truth for "which tier is this finding in." It matches `integration_problems.md`'s own Progress Tracker exactly — if the two ever disagree, `integration_problems.md` is stale and must be corrected to match this list (which is derived from the report's per-finding Severity fields), not the other way around.

**Tier 0** (1 finding): CROSS-001

**Tier 1** (4 findings): CROSS-002, CROSS-003, CROSS-004, CROSS-005

**Tier 2** (3 findings): CROSS-006, CROSS-007, CROSS-008

Total: 1 + 4 + 3 = **8**, matching the report.

## 4. Per-finding fix protocol

For every finding, in this exact order:

1. **Read the finding in full** from `integration_problems.md` — the `What`, `Why it is wrong`, `Root cause`, and `Fix` fields, plus both code blocks showing what each side of the boundary currently does. Re-derive the correct fix from current source; the `Fix` field is a starting sketch, not a spec to copy verbatim — source may have drifted since the finding was written (this report was written against commit `3b808f5`; check `git log` for anything touching the finding's `Files` list since then).

2. **Re-read the cited source lines on *both* sides of the boundary before touching anything.** This is the rule unique to integration findings: a single-section bug only needs its own file re-checked, but a cross-section bug can be "fixed" on one side while the other side has already drifted further. Confirm both halves of the mismatch are still present as described.

3. **Identify which section owns the fix.** Each finding's `Section ownership cross-reference` table (in `integration_problems.md` §4) marks exactly one side as "fix here" — that is normally the side that deviated from the shared contract, not the side serving as the reference implementation. Do not "fix" the reference-implementation side. If a finding requires changes on both sides (none currently do, but a future finding might), treat it as two sub-fixes and record both commits under the one finding ID.

4. **Check for a cross-section sign-off need.** If the fix touches a file outside the owning section's normal directory boundary (e.g. a Tier 1/CROSS-003 fix that needs to add a `TrustStore` construction inside `iris-android`'s or `iris-ios`'s engine file — those are the owning platform's files, so this is fine), proceed. If a fix would require changing a file under another section's ownership *not* named in the finding's own `Files` list, stop and mark `🔒 Blocked · reason: fix requires changes outside the named Files list, needs owner review` instead of guessing.

5. **Write the fix.** Prefer the smallest change that closes the mismatch — align the deviating side to the reference side's contract; do not redesign the contract itself as part of this loop. If the finding's `Fix` field names a specific target value (e.g. CROSS-001's UUID), use exactly that value — it was cross-checked against the reference implementation when the finding was written.

6. **Test.**
   - For a Rust-only fix (`crates/iris-core`, or a platform crate's `.rs` files): `cargo build --workspace` at minimum, then the narrowest test scope that exercises the change.
   - For a Kotlin-only fix (`android/`): this environment cannot run a Gradle build (no Android SDK/toolchain here — same limitation noted throughout `sohan_problems.md`/`shrey_problem.md`). Verify by careful reading — confirm the changed identifier/value is used consistently everywhere it appears (grep for the old value across the whole `android/` tree to make sure no other call site still expects it) — and mark the fix `✅ Fixed · PENDING BUILD VERIFICATION` rather than a bare `✅ Fixed`.
   - For a Swift-only fix (`ios/`) or a Swift-binding-regeneration fix (CROSS-008): this environment cannot run Xcode or `build-xcframework.sh`'s codegen step. Mark `✅ Fixed · PENDING BUILD VERIFICATION` and note in the commit exactly what a human must run to promote it (e.g. for CROSS-008: "run `ios/Scripts/build-xcframework.sh`, commit the regenerated `IrisCore.swift`").
   - If the finding says "add a regression test" (none currently do explicitly, but a future finding might), write it.

7. **On success:** commit (format in §6), then update `integration_problems.md`:
   - Flip the finding's `Fix status` line to `✅ Fixed · Tier N · commit <short-hash> · <date>` (or the `PENDING BUILD/HARDWARE VERIFICATION` suffix per step 6 above where the platform toolchain isn't available in this environment).
   - Update the Progress Tracker's tier roll-up table (decrement `⬜`, increment `✅/🟢` — or the not-started count for whichever column changed) and its `Last updated` line.
   - Update the `Section ownership cross-reference` table row if its "fix here" marker needs to note completion.

8. **On failure** (build breaks, or the fix turns out to be wrong once written): revert the changed files to the last good commit — never commit a red tree. Mark the finding `❌ Reverted · reason: <what broke, one sentence>` and move on. Do not retry the same finding more than once per wake cycle; a second failure escalates to `🔒 Blocked · reason: attempted and reverted, needs human review`.

## 5. Tier-complete protocol

When every finding in a tier is `✅`/`🟢`/`🔒`/`⚪` (i.e., nothing left `⬜` or `🔵`):

1. Run the full workspace build one more time (`cargo build --workspace --all-targets`), not just the narrow per-fix scopes — this catches interactions between fixes within the tier.
2. If green: make a **tier-checkpoint commit** (format in §6) that touches only `integration_problems.md` (the tier roll-up now shows the tier complete) — no code should be in this commit, code was already committed per-finding in step 7 above.
3. If a `🔒 Blocked` count is non-zero, list the blocked findings and their reasons in the checkpoint commit message so a human scanning `git log` sees exactly what needs their attention.
4. Advance to the next tier per the order in §2.

## 6. Commit format

Per-finding commits:
```
fix(integration): <one-line summary matching the finding title>

<one paragraph: what was wrong, what changed, per the finding's Root cause/Fix fields.
Name explicitly which section's file was the reference implementation and which
side was brought into alignment.>

Finding: CROSS-<NNN> (Tier <N>)
Ref: docs/bug-hunting/integration_problems.md
```

Tier-checkpoint commits (report-only):
```
docs(integration): Tier <N> complete — <tier name>

<X> fixed, <Y> blocked (listed below), <Z> pending build verification.
<blocked-id>: <reason>

Ref: docs/bug-hunting/integration_problems.md
```

**Commits are local only by default.** This loop does not run `git push` on its own — pushing to the shared branch is a separate, explicit decision the operator makes after reviewing the commit history this loop produces, same as every other section's fix loop in this repo.

## 7. Regenerating the tier table (only needed if the report changes)

If `integration_problems.md` is re-audited or findings are added/removed, recompute §3 using the finding's own `Severity` field:

```
tier(0)  if sev == "Critical"
tier(1)  if sev == "High"
tier(2)  if sev == "Medium"
tier(2)  if sev == "Low"   (none currently exist at this severity)
```

This mirrors the report's own stated tier rationale (§2 above) — Critical is the cross-platform-invisible-to-each-other bug, High is security/build-integrity regressions, Medium is wiring/regen breaks.

## 8. Safety limits for one wake cycle

- **Fix at most 8 findings per wake cycle** — this report only has 8 total, so in practice a single well-resourced wake can attempt the entire report; if it does, still commit per-finding (not one giant commit) so the diff stays reviewable per finding.
- **Never modify files outside `crates/`, `android/`, `ios/`, and `docs/bug-hunting/`** without it being explicitly named in the finding's `Files` field.
- **Never delete a test to make it pass.** If a fix appears to require weakening or removing an existing test, mark `🔒 Blocked · reason: fix conflicts with existing test <name>, needs review` instead.
- **If two consecutive findings in a wake cycle fail (§4 step 8), stop the wake cycle early** rather than continuing through the rest of the batch — with only 8 total findings, two failures in a row is a stronger signal here than it would be in a 284-finding report; investigate before continuing.
- **Never "fix" the reference-implementation side of a mismatch** (see §4 step 3) — this is the single most likely mistake in this particular loop, since (unlike a single-section report) every finding here has two plausible-looking targets to change and only one is correct.

## 9. How to actually run this

Reading `integration_problems.md`'s Progress Tracker at the start of any wake cycle tells you exactly where to continue — which tier is active, which findings in it are still `⬜`/`🔵`.

Recommended invocation: a recurring `/loop` with dynamic self-pacing, re-entering this file's instructions each wake, prompt equivalent to:

> Continue the fix loop specified in `docs/bug-hunting/integration_problem_loop.md`. Read the Progress Tracker in `docs/bug-hunting/integration_problems.md` to find the current tier and next unfixed finding. Follow §4 (per-finding protocol) and §5 (tier-complete protocol). Record every run in `docs/bug-hunting/integration_problems_log.md`. Stop and report if §8's two-consecutive-failures condition fires.

Do not start this loop unattended for an extended period without first running it for one manual wake cycle and reviewing the diff it produces.
