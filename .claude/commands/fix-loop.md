---
description: Resume the Section 3 bug-fix loop from wherever it left off and run one bounded, autonomous batch.
---

You are resuming the autonomous fix loop for `docs/bug-hunting/taksh_problems.md`. **Do not ask the user what to do next — the answer is always in the Progress Tracker.** This command is designed to be invoked repeatedly (by hand, or on a schedule via `/loop`); every invocation is independent and picks up exactly where the last one left off, because state lives in the report files, not in this conversation.

Optional argument: `$ARGUMENTS` — if a number is given (e.g. `/fix-loop 3`), fix at most that many findings this run instead of the default 8. If `unblock-tier-2` is given, treat this invocation as the human's explicit go-ahead to begin Tier 2 (see step 3).

## Step 0 — orient (do this every time, trust nothing from prior context)

1. Read `docs/bug-hunting/taksh_problems_loop.md` in full — this is the logic. Do not improvise a different protocol.
2. Read the **Progress Tracker** section near the top of `docs/bug-hunting/taksh_problems.md` — this is the state. It tells you the current tier, how many findings in each status per tier, and the `Last updated` date.
3. Run `git log --oneline -15` and `git status` to confirm the working tree is clean and matches what the tracker claims (no uncommitted fix mid-flight from a prior run that got interrupted — if there is one, finish or revert it per §4 step 8 of the loop file before starting anything new).

## Step 1 — determine current tier

Walk the tier roll-up table top to bottom in run order (**-1 → 0 → 1 → 4 → 3 → [gate] → 2**, per §2 of the loop file). The current tier is the first one with any finding still `⬜ Not started` or `🔵 In progress`.

- If every tier through 3 shows all findings `✅`/`🟢`/`🔒`/`⚪`, and Tier 2 is still all `⬜`: you are at the **Tier 2 gate**. Do NOT proceed into Tier 2 unless `$ARGUMENTS` contains `unblock-tier-2`. Otherwise, stop here and report: "All of Tiers -1/0/1/4/3 complete. Tier 2 (80 findings, routing/DTN/gateway/storage invariants) needs your explicit sign-off — re-run `/fix-loop unblock-tier-2` when ready." Do not touch any Tier 2 code or file in this case.
- If everything including Tier 2 is done: report completion, stop, do nothing further.
- Otherwise: the current tier is identified. List its remaining `⬜`/`🔵` findings by grepping their IDs from §3 of the loop file and cross-referencing their `Fix status` lines in the report.

## Step 2 — fix findings, one at a time

For each finding, up to the batch cap (`$ARGUMENTS` if numeric, else 8), follow **§4 of `taksh_problems_loop.md` exactly**: re-read the cited source (line numbers drift, confirm the defect is still present before fixing it), check `Dependencies / blast radius`, write the smallest correct fix, test narrowly, and:

- **On success:** commit using the format in §6 of the loop file, then update *both* the finding's own `Fix status` line and the Progress Tracker's tier roll-up table and `Last updated` date in `taksh_problems.md`.
- **On failure:** revert the change (never leave a red tree), mark the finding `❌ Reverted` with a one-sentence reason, move to the next finding.
- **If blocked** (needs hardware, needs cross-section sign-off, or already reverted once): mark `🔒 Blocked` with the reason, move on.
- If 3 consecutive findings fail in this run, stop the batch early per §8 of the loop file and report why.

Respect every safety limit in §8 of the loop file (file-ownership boundary, no weakening tests, batch cap).

## Step 3 — tier-complete check

After the batch, check whether the current tier is now fully resolved (nothing left `⬜`/`🔵`). If so, follow **§5 of the loop file**: run the full workspace build + test suite once, then make the tier-checkpoint commit (report-only, format in §6), and let the tracker reflect the tier as closed. Do not auto-advance into starting work on the next tier within this same invocation — that happens on the *next* `/fix-loop` call, which will read the tracker fresh and pick it up per Step 1.

## Step 4 — report back

End every run with a short, concrete summary, not a restatement of this prompt:

```
Tier: <N> (<tier name>)
Fixed this run: <ids>
Blocked: <ids and one-line reasons>
Reverted: <ids and one-line reasons>
Tier status: <X/Y done> — <complete, or still N remaining>
Next: <what the next /fix-loop call will pick up, or "Tier 2 gate reached, needs unblock-tier-2" or "all tiers complete">
```

Commits made this run are **local only** — never `git push` from this command. That remains a separate, explicit human decision.
