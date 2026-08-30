# Fix Loop — Autonomous Execution Plan for `rahul_problems.md`

**Owner:** Rahul Hadiyal
**Companion document:** [`rahul_problems.md`](rahul_problems.md) — the 25-finding report this loop fixes. Read that document's `Progress Tracker` section for current status; this file is the *logic*, that one is the *state*.
**Fix journal:** [`rahul_fix_log.md`](rahul_fix_log.md) — records what each run attempted, evidence, commits.
**Source review:** `PROTOCOL_MESSAGING_EMERGENCY_REVIEW.md` — the full finding descriptions, evidence, and root-cause analysis.
**Purpose:** a self-contained specification an autonomous agent (or a human following it by hand) can execute repeatedly, without needing this conversation's context, to work through all 25 findings tier by tier, committing as it goes, and keeping `rahul_problems.md` accurate at every step.

---

## 1. The invariant this loop must never break

**The tree must be green after every commit.** No commit is made unless `cargo build --workspace --exclude iris-desktop` succeeds and the relevant test scope passes. If a fix cannot be made to pass, it is reverted in the same wake cycle — the loop never leaves a broken build. This is non-negotiable and takes priority over "making progress."

**Build environment:** GNU toolchain required on Windows (`stable-x86_64-pc-windows-gnu`). `cargo kani` is Linux-only — Kani-related verification for PM-16 must be re-checked on Linux CI. Record as `PENDING LINUX-CI` for that finding.

---

## 2. Tier order and gating

Run tiers **in this order**. Do not skip ahead.

1. **Tier 0** — fix first. These are live, active safety and security defects. PM-1 and PM-2 are Critical; PM-15 must ship in the same commit as PM-2 (see PM-2's Dependencies field). Fix these before any Tier 1/2/3 work.

2. **Tier 1** — after Tier 0 is complete (all `✅` or `🔒`). High correctness defects plus medium-severity safety/protocol issues. No human gate between Tier 0 and Tier 1.

3. **Tier 2** — after Tier 1 is complete (all `✅` or `🔒`). Medium/Low correctness, performance, and protocol findings. No human gate between Tier 1 and Tier 2.

4. **Tier 3 — HARD STOP.** Before starting Tier 3, the loop must stop and require an explicit human go-ahead. Reason: Tier 3 findings are structural enhancements — they change architectural patterns (dispatch model, decode algorithm, caching model), are larger diffs with wider blast radii, and several have hard blockers on cross-section coordination (see §2.3). A wrong or premature architectural change here can cause regressions harder to find than the originals.

5. **Tier 4** — after human go-ahead. These are new audit findings (GAP-1..GAP-8) added after the original 25-finding review. Tier 4 proceeds in the same per-finding protocol as Tiers 1–2 (§4). Fix GAP-1 (High security) and GAP-6/GAP-8 (Medium) before GAP-2/GAP-4/GAP-5/GAP-7 (Low).

### 2.3 Tier 3 coordination prerequisites

Before the first Tier 3 commit, confirm the following with the relevant section owners:

| Finding | Coordination needed | Status |
|---|---|---|
| PS-1 | Section 3: per-adapter transport re-entrancy under concurrent `send()` — BLE, LoRa, Wi-Fi Direct, Wi-Fi Aware, satellite, internet. If any adapter is not re-entrant, per-transport in-flight cap must be 1. **Blocks PS-1.** | 🔒 Awaiting Section 3 |
| PS-3 | Section 1: how revocation and key rotation invalidate a cached `VerifiedAuthority`. Without this contract, caching Section 1's verification result is a security regression. **Blocks PS-3.** | 🔒 Awaiting Section 1 |
| PS-5 | Section 3: whether concurrent scans contend for shared radio hardware or OS scan arbiter (BLE + Wi-Fi share physical radio on many phones). **Verify before implementing PS-5** — it may need to be per-radio-family concurrency rather than per-transport. | 🔒 Awaiting Section 3 |

---

## 3. Authoritative tier membership

This is the source of truth for "which tier is this finding in."

**Tier 0** (6 findings):
PM-1, PM-2, PM-4, PM-5, PM-6, PM-15

**Tier 1** (7 findings):
PM-3, PM-7, PM-8, PM-9, PM-10, PM-12, PM-14

**Tier 2** (5 findings):
PM-11, PM-13, PM-16, PM-17, PM-18

**Tier 3** (7 findings — GATED):
PS-1, PS-2, PS-3, PS-4, PS-5, PS-6, PS-7

**Tier 4** (8 findings — opened 2026-08-30 under explicit human go-ahead):
GAP-1, GAP-2, GAP-3, GAP-4, GAP-5, GAP-6, GAP-7, GAP-8

**Total: 6 + 7 + 5 + 7 + 8 = 33 findings.**

---

## 4. Per-finding fix protocol

For every finding, in this exact order:

1. **Read the finding in full** from `rahul_problems.md` — the `What`, `Evidence`, `Root cause`, `Fix`, and `Dependencies / blast radius` fields. For the full original analysis, re-read the corresponding finding in `PROTOCOL_MESSAGING_EMERGENCY_REVIEW.md`. The `Fix` field is a starting sketch, not a spec to copy verbatim — re-derive the correct fix from the `Evidence` and current source, since the source may have drifted.

2. **Re-read the cited source lines before touching anything.** Line numbers drift. Confirm the defect is still present as described before writing a fix. If it's already fixed (e.g., by an earlier finding's fix touching the same code), mark it `✅ Fixed (subsumed by <other-id>)` and move on without writing new code.

3. **Check `Dependencies / blast radius`.** If the finding says another finding must land first and that dependency is not yet `✅`/`🔒`, either fix the dependency first (if in the same tier) or mark this one `🔒 Blocked · reason: depends on <id>, not yet fixed` and move to the next finding.

4. **Check for cross-section or human gates.** If a finding requires cross-section agreement (Section 1/3/4/5/6 named in Dependencies) and none has been recorded, mark `🔒 Blocked · reason: needs <Section N> sign-off` rather than guessing. For PM-16, mark `✅ Fixed · PENDING LINUX-CI` after writing the code fix — Kani verification cannot run on Windows.

5. **Write the fix.** Prefer the smallest change that removes the defect described. Do not use a fix as an excuse to refactor unrelated code. If the finding says "add a regression test," add it in the same commit.

6. **Test.** At minimum: `cargo build --workspace --exclude iris-desktop`. Then the narrowest test scope that exercises the fix (a single `cargo test -p iris-core <test_name>`, not the whole suite, unless the finding's area requires it). If the finding says "add a regression test" or references a test that should assert the corrected behaviour, write it — a fix without a test that would have caught the original bug is incomplete.

7. **On success:** commit (format in §6), then update `rahul_problems.md`:
   - Flip the finding's `Fix status` line to `✅ Fixed · Tier N · commit <short-hash> · <date>` (or `🟢` only if hardware-verified by a human on a real device/with live Postgres).
   - Update the Progress Tracker's tier roll-up table (decrement `⬜`, increment `✅/🔒`).
   - Update `rahul_fix_log.md`: fill in the finding's row with Status, Commit, and Verification fields.

8. **On failure** (build breaks, test fails, or the fix turns out to be wrong once written): `git checkout` the changed files back to the last good commit — never commit a red tree. Mark the finding `❌ Reverted · reason: <what broke, one sentence>` and move on. Do not retry the same finding more than once per wake cycle; a second failure on the same finding escalates to `🔒 Blocked · reason: attempted and reverted, needs human review`.

---

## 5. Special handling for PM-2 + PM-15

These two findings **must ship in the same commit**. PM-15 is latent today only because PM-2 drops all cancels. Fixing PM-2 without PM-15 converts a latent cancel-forgery hole into an exploitable one.

When processing PM-2, treat PM-15 as part of the same unit of work. The commit must:
- Build the engine-side SOS ledger: `HashMap<MessageId, (u64, sender_id)>` (timestamp + sender_id), bounded to `SOS_CANCEL_WINDOW_SECS` horizon.
- Wire `EmergencyProvider` trait with a lookup seam.
- Pass `Some(OriginalSos { timestamp, sender_id })` to `classify_sos` on Cancel.
- Implement the signer-mismatch check inside `classify_sos` using `identity::peer_short_from_sender`.
- Add a `cancel_forgery` regression test.

Mark both PM-2 and PM-15 `✅ Fixed` with the same commit hash.

---

## 6. Commit format

Per-finding commits:
```
fix(<area>): <one-line summary matching the finding title>

<one paragraph: what was wrong, what changed, per the finding's Root cause/Fix fields>

Finding: <ID> (Tier <N>)
Ref: docs/bug-hunting/rahul_problems.md
```

Tier-checkpoint commits (docs only, after all findings in a tier are ✅/🔒):
```
docs(bug-hunting): Tier <N> complete — <tier name>

<X> fixed, <Y> blocked (listed below).
<blocked-id>: <reason>
<blocked-id>: <reason>

Ref: docs/bug-hunting/rahul_problems.md
```

Fix-log update commits (accumulated at batch end):
```
docs(bug-hunting): Run <N> — <area description>

<brief summary of what landed, what was blocked>

Ref: docs/bug-hunting/rahul_fix_log.md
```

**Commits are local only.** This loop never runs `git push`. Pushing to a shared branch is a separate, explicit decision for a human to make after reviewing the commit history.

---

## 7. Tier-complete protocol

When every finding in a tier is `✅`/`🟢`/`🔒`/`⚪`:

1. Run the full workspace build and full test suite one more time (`cargo build --workspace --exclude iris-desktop --all-targets && cargo test --workspace --exclude iris-desktop`) to catch interactions between fixes.
2. If green: make a **tier-checkpoint commit** that touches only `rahul_problems.md` and `rahul_fix_log.md` — no code in this commit.
3. If `🔒 Blocked` count is non-zero, list the blocked findings and their reasons in the checkpoint commit message.
4. Advance to the next tier per §2. If the next tier is Tier 3, **stop** — see §2's hard stop rule.

---

## 8. Safety limits for one wake cycle

- **Fix at most 8 findings per wake cycle**, then stop and yield. Keeps each session's diff reviewable.
- **Never modify files outside `crates/iris-core/src/` and `docs/bug-hunting/`** unless the finding's `Location` field explicitly names the file. If a fix seems to require touching code not named in the finding, mark `🔒 Blocked · reason: fix requires changes outside message_engine/protocol/discovery/emergency ownership, needs owner review`.
- **Never delete a test to make it pass.** If a fix appears to require weakening an existing test, mark `🔒 Blocked · reason: fix conflicts with existing test <name>, needs review`.
- **If three consecutive findings in a wake cycle fail (§4 step 8), stop the wake cycle early** — this usually means something upstream changed that needs a human look, not more attempts.
- **For PM-2 + PM-15:** count these as one finding toward the 8-per-wake limit (they are one commit).

---

## 9. Tier 3 special rules

When human go-ahead has been received and Tier 3 is active:

- Before implementing PS-1: confirm Section 3 has signed off on transport re-entrancy per §2.3.
- Before implementing PS-3: confirm Section 1 has established the revocation/caching contract per §2.3.
- Before implementing PS-5: confirm Section 3 has verified hardware contention behavior per §2.3.
- PS-7 should be implemented as part of PM-10's fix (they touch the same lines and PM-10's fix falls naturally out of PS-7's API change). If PM-10 was already fixed in Tier 1, implement PS-7 as a separate structural change but document the coordination.
- PS-4 should be implemented together with PM-3's TTL-expiry fix (they both touch `PendingAck`'s fields and lifecycle). If PM-3 was already fixed in Tier 1, implement PS-4 as a separate change.
- PS-6 (VecDeque audit ring) is the lowest-risk Tier 3 change — no coordination needed, no API change, measured 100× improvement. Implement this first in Tier 3.

---

## 10. How to actually run this

This file is written to be resumable and idempotent: reading `rahul_problems.md`'s Progress Tracker at the start of any wake cycle tells you exactly where to continue — which tier is active, which findings in it are still `⬜`, and whether the previous wake ended mid-finding (in which case §4 step 8's revert logic means the tree is already clean and that finding gets picked up as `⬜` again).

Recommended invocation per wake:

> Continue the fix loop specified in `docs/bug-hunting/rahul_problems_loop.md`.
> Read the Progress Tracker in `docs/bug-hunting/rahul_problems.md` to find the current tier and next unfixed finding.
> For full finding details, cross-reference `PROTOCOL_MESSAGING_EMERGENCY_REVIEW.md`.
> Follow §4 (per-finding protocol), §5 (PM-2+PM-15 special handling), §7 (tier-complete protocol), and §8 (safety limits).
> Stop and report if you reach the Tier 3 gate (§2) without human sign-off, or if §8's three-consecutive-failures condition fires.
> After fixing findings, update both `rahul_problems.md` (fix status + progress tracker) and `rahul_fix_log.md` (run entry).

Do not start this loop unattended for an extended period without first running it for one manual wake cycle and reviewing the diff and commit it produces — confirm the protocol is being followed as written before letting it run over many wakes.

---

## 11. Regenerating the tier table (only needed if the report changes)

If `rahul_problems.md` is re-audited or findings are added/removed, recompute §3 with this rule (first match wins):

```
tier(0)   if category == "safety"   and severity in {"Critical"}
tier(0)   if category == "security" and severity in {"High"}
tier(0)   if category == "safety"   and severity == "Medium" and must_ship_with_tier0_finding
tier(1)   if category == "correctness" and severity == "High"
tier(1)   if category in {"correctness","protocol","safety"} and severity == "Medium"
tier(2)   if category in {"correctness","performance","protocol"} and severity in {"Medium","Low"}
tier(3)   if category == "architecture"  (structural enhancements)
skip      if finding is already fixed or verified-clean
```

The PM-15 exception: it is `safety/Medium` but must be in Tier 0 because it is latently dangerous once PM-2 is fixed and cannot be safely deferred. Override applied.
