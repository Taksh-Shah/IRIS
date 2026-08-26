# Fix Loop — autonomous execution plan for `taksh_problems.md`

**Owner:** Taksh Shah
**Companion document:** [`taksh_problems.md`](taksh_problems.md) — the 284-finding report this loop fixes. Read that document's `Progress Tracker` section for current status; this file is the *logic*, that one is the *state*.
**Purpose:** a self-contained specification an autonomous agent (or a human following it by hand) can execute repeatedly, without needing this conversation's context, to work through all 282 actionable findings tier by tier, committing as it goes, and keeping `taksh_problems.md` accurate at every step.

---

## 1. The invariant this loop must never break

**The tree must be green after every commit.** No commit is made unless `cargo build --workspace` succeeds and the relevant test scope passes. If a fix cannot be made to pass, it is reverted in the same wake cycle — the loop never leaves a broken build for the next wake or for a human to find. This is non-negotiable and takes priority over "making progress."

## 2. Tier order and gating

Run tiers **in this order**. Do not skip ahead; do not run Tier 2 early even if it looks tempting — the gate exists on purpose.

1. **Tier -1** (1 finding) — must be fixed first. Nothing in Tiers 0/1/3/4 can be *observed* working until GAP-7 is fixed, because no transport can reach `send()` until something calls `connect()`. Tier 2 findings are latent regardless (see §5 of the report), so Tier -1 doesn't block Tier 2 the same way, but fix it first anyway — it's one finding and it's the report's own headline.
2. **Tier 0, Tier 1, Tier 3, Tier 4** — no ordering dependency on each other. Run them in this sequence: **0 → 1 → 4 → 3**. Reasoning: Tier 0 is the highest-severity live-surface work and the report's second headline; Tier 1 is small and closes the CI-blind-spot (TAK-1) which the loop itself benefits from having closed early (a real Postgres test gate makes every subsequent storage-adjacent fix in Tier 2/4 easier to trust); Tier 4 is bulk mechanical work best done before Tier 3's more involved simulator rewrite; Tier 3 goes last because fixing simulator fidelity changes what "passing tests" means for everything reviewed against it, and doing that once at the end avoids re-basing test expectations twice.
3. **Tier 2 — HARD STOP.** Before starting Tier 2, the loop must stop and require an explicit human go-ahead. Reason: Tier 2 findings change security and correctness *invariants* — routing loop prevention, gateway trust admission, storage exhaustion bounds, DTN eviction ordering. A wrong fix here is worse than the original bug, and several of these need Section 2 (message engine) owner agreement per the report's own "Dependencies / blast radius" fields. The loop may *prepare* Tier 2 (read every finding, draft a fix plan per finding, flag which ones need Section 2 sign-off) without human confirmation, but may not commit code changes for Tier 2 until a human says so.

## 3. Authoritative tier membership

This is the source of truth for "which tier is this finding in." It was computed once by rule (see §7 for the rule, in case the report is amended and this needs regenerating) and is pasted here as a static list so the loop never has to re-derive it.

**Tier -1** (1 finding): GAP-7

**Tier 0** (32 findings): BLE-1, BLE-2, BLE-3, BLE-4, BLE-5, BLE-9, BLE-27, BLE-28, BLE-29, BLE-30, FFI-1, FFI-2, FFI-3, FFI-4, FFI-5, FFI-6, FFI-7, FFI-8, FFI-9, FFI-10, FFI-11, FFI-12, FFI-13, FFI-14, FFI-15, FFI-16, FFI-17, FFI-18, FFI-19, GAP-4, GAP-9, GAP-12

**Tier 1** (11 findings): SYS-1, SYS-2, SYS-3, SYS-4, SYS-5, SYS-6, RF-35, RF-36, RF-37, RF-38, TAK-1

**Tier 2** (80 findings — GATED, human sign-off required before any commit): ROUT-1 through ROUT-36 (all 36), DTN-1 through DTN-25 (all 25), MG-25 through MG-42 (all 18), TAK-2

**Tier 3** (32 findings): SIM-1 through SIM-32 (all 32)

**Tier 4** (126 findings): BLE-6, BLE-7, BLE-8, BLE-10 through BLE-26, BLE-31 through BLE-40 (BLE Medium/Low, 28 total), RF-1 through RF-34, RF-39 through RF-45 (RF Medium/Low, 41 total), MG-1 through MG-24 (MG manager/error Medium/Low, 24 total), TAK-3 through TAK-22, TAK-24, TAK-25 (TAK Medium/Low, 22 total), GAP-1, GAP-2, GAP-3, GAP-5, GAP-6, GAP-8, GAP-10, GAP-11, GAP-13 (GAP remainder, 9 total)

**Not applicable** (2, no fix needed — verified-clean results, skip entirely): TAK-23, GAP-14

Total: 1 + 32 + 11 + 80 + 32 + 126 + 2 = **284**, matching the report.

## 4. Per-finding fix protocol

For every finding, in this exact order:

1. **Read the finding in full** from `taksh_problems.md` — the `What`, `Evidence`, `Why it is wrong`, `Root cause`, `Fix`, and `Dependencies / blast radius` fields. The `Fix` field is a starting sketch, not a spec to copy verbatim — re-derive the correct fix from the `Evidence` and current source, since the source may have drifted since the finding was written.
2. **Re-read the cited source lines before touching anything.** Line numbers drift. Confirm the defect is still present as described before writing a fix for it — if it's already fixed (e.g., by an earlier finding's fix touching the same code), mark it `✅ Fixed (subsumed by <other-id>)` and move on without writing new code.
3. **Check `Dependencies / blast radius`.** If the finding says another finding must land first (e.g., DTN-1's fix depends on DTN-11's eviction policy being wired), and that dependency is not yet `✅`/`🟢`, either fix the dependency first (if it's in the same tier) or mark this one `🔒 Blocked · reason: depends on <id>, not yet fixed` and move to the next finding in the tier.
4. **Check for a hardware/human gate.** If the finding is Tier 0 and its fix cannot be exercised without a physical device (most Tier 0 findings), still write and commit the code fix — mark it `✅ Fixed · PENDING HARDWARE VERIFICATION`, never `🟢`. If a finding anywhere requires cross-section agreement (Section 2/4/5 named in `Dependencies`) and none has been recorded, mark `🔒 Blocked · reason: needs <Section N> sign-off` rather than guessing at the other section's intent.
5. **Write the fix.** Prefer the smallest change that removes the defect described — do not use a fix as an excuse to refactor unrelated code. If the finding explicitly recommends a CI guard (several `SYS-*` and `TAK-*` findings do), add it in the same commit.
6. **Test.** At minimum: `cargo build --workspace`. Then the narrowest test scope that exercises the fix (a single `cargo test -p <crate> <test_name>`, not the whole suite, unless the finding's own area requires it — Tier 2 fixes in particular should re-run the specific test suite named in the finding, e.g. `cargo test -p iris-core --test sim_scenarios` for a `ROUT-*`/`DTN-*` fix once routing is wired). If the finding says "add a regression test" or references a test that should now assert the corrected behaviour, write it — a fix without a test that would have caught the original bug is incomplete.
7. **On success:** commit (format in §6), then update `taksh_problems.md`:
   - Flip the finding's `Fix status` line to `✅ Fixed · Tier N · commit <short-hash> · <date>` (or `🟢` only if hardware-verified by a human, which the loop cannot do itself).
   - Update the Progress Tracker's tier roll-up table (decrement `⬜`, increment `✅/🟢`) and its `Last updated` line.
8. **On failure** (build breaks, test fails, or the fix turns out to be wrong once written): `git checkout` the changed files back to the last good commit — never commit a red tree. Mark the finding `❌ Reverted · reason: <what broke, one sentence>` and move on. Do not retry the same finding more than once per wake cycle; a second failure on the same finding should escalate to `🔒 Blocked · reason: attempted and reverted, needs human review` rather than looping.

## 5. Tier-complete protocol

When every finding in a tier is `✅`/`🟢`/`🔒`/`⚪` (i.e., nothing left `⬜` or `🔵`):

1. Run the full workspace build and the full test suite one more time (`cargo build --workspace --all-targets && cargo test --workspace`), not just the narrow per-fix scopes — this catches interactions between fixes within the tier that per-finding testing could miss.
2. If green: make a **tier-checkpoint commit** (format in §6) that touches only `taksh_problems.md` (the tier roll-up now shows the tier complete) — no code should be in this commit, code was already committed per-finding in step 7 above.
3. If a `🔒 Blocked` count is non-zero, list the blocked findings and their reasons in the checkpoint commit message so a human scanning `git log` sees exactly what needs their attention, without having to open the report.
4. Advance to the next tier per the order in §2. If the next tier is Tier 2, **stop** and do not proceed — see §2.3.

## 6. Commit format

Per-finding commits:
```
fix(<area>): <one-line summary matching the finding title>

<one paragraph: what was wrong, what changed, per the finding's Root cause/Fix fields>

Finding: <ID> (Tier <N>)
Ref: docs/bug-hunting/taksh_problems.md
```

Tier-checkpoint commits (report-only):
```
docs(bug-hunting): Tier <N> complete — <tier name>

<X> fixed, <Y> blocked (listed below), <Z> not applicable.
<blocked-id>: <reason>
<blocked-id>: <reason>

Ref: docs/bug-hunting/taksh_problems.md
```

**Commits are local only.** This loop never runs `git push`. Pushing to a shared branch is a separate, explicit decision for a human to make after reviewing the commit history this loop produces.

## 7. Regenerating the tier table (only needed if the report changes)

If `taksh_problems.md` is re-audited or findings are added/removed, recompute §3 with this rule (evaluated top to bottom, first match wins — `fid` is the finding ID like `ROUT-14`, `sev` is its post-verification severity):

```
tier(-1)  if fid == "GAP-7"
tier(0)   if fid in {"GAP-4", "GAP-9", "GAP-12"}
tier(0)   if prefix(fid) == "FFI"
tier(0)   if prefix(fid) == "BLE" and sev in {"Critical","High"}
tier(1)   if prefix(fid) == "SYS"
tier(1)   if fid == "TAK-1"
tier(1)   if prefix(fid) == "MG" and sev in {"Critical","High"} and number(fid) < 25
tier(1)   if prefix(fid) == "RF" and number(fid) >= 35 and sev in {"Critical","High"}
tier(2)   if prefix(fid) in {"ROUT","DTN"}
tier(2)   if prefix(fid) == "MG" and number(fid) >= 25
tier(2)   if fid == "TAK-2"
tier(3)   if prefix(fid) == "SIM"
tier(4)   otherwise
skip      if sev == "Informational"  (verified-clean, not a defect)
```
This mirrors the boundary between "manager/error-type" (MG-1..24, live, Tier 0/4) and "gateway" (MG-25..42, latent, Tier 2) findings, and between LoRa/satellite (RF-1..34, latent, Tier 4) and internet/simulated (RF-35..45, more live, Tier 1/4) findings — both splits reflect §5 of the report (wiring status), not arbitrary numbering.

## 8. Safety limits for one wake cycle

To keep any single autonomous session bounded and reviewable:

- **Fix at most 8 findings per wake cycle**, then stop and yield (schedule the next wake rather than continuing indefinitely in one session). This keeps each session's diff reviewable and each `git log` entry meaningful, and avoids one runaway session silently burning through an entire tier unattended.
- **Never modify files outside `crates/`, `android/`, `ios/`, and `docs/bug-hunting/`** without it being explicitly named in the finding's `Location` field. If a fix seems to require touching Section 1/2/4/5-owned code not named in the finding, stop and mark `🔒 Blocked · reason: fix requires changes outside Section 3 ownership, needs owner review` rather than proceeding.
- **Never delete a test to make it pass.** If a fix appears to require weakening or removing an existing test, that is itself a red flag — mark `🔒 Blocked · reason: fix conflicts with existing test <name>, needs review` instead.
- **If three consecutive findings in a wake cycle fail (§4 step 8), stop the wake cycle early** rather than continuing to fail through the rest of the batch — this usually means something upstream changed (a dependency, a toolchain issue) that needs a human look, not more attempts.

## 9. How to actually run this

This file is written to be resumable and idempotent: reading `taksh_problems.md`'s Progress Tracker at the start of any wake cycle tells you exactly where to continue — which tier is active, which findings in it are still `⬜`/`🔵`, and whether the previous wake ended mid-finding (in which case §4 step 8's revert logic means the tree is already clean and that finding just gets picked up as `⬜` again).

Recommended invocation: a recurring `/loop` with dynamic self-pacing, re-entering this file's instructions each wake, prompt equivalent to:

> Continue the fix loop specified in `docs/bug-hunting/taksh_problems_loop.md`. Read the Progress Tracker in `docs/bug-hunting/taksh_problems.md` to find the current tier and next unfixed finding. Follow §4 (per-finding protocol), §5 (tier-complete protocol), and the safety limits in §8. Stop and report if you reach the Tier 2 gate (§2.3) or if §8's three-consecutive-failures condition fires.

Do not start this loop unattended for an extended period without first running it for one manual wake cycle and reviewing the diff and commit it produces — confirm the protocol is being followed as written before letting it run over many wakes.
