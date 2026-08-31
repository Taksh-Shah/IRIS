# Fix Loop — autonomous execution plan for `priyam_problems.md`

**Owner:** Priyam
**Companion documents:**
- [`priyam_problems.md`](priyam_problems.md) — the 30-finding report this loop fixes. Its
  `Progress Tracker` section is the *state*; this file is the *logic*.
- [`priyam_fix_log.md`](priyam_fix_log.md) — the per-run execution journal (one entry per wake).

**Purpose:** a self-contained specification an autonomous agent (or a human following it by
hand) can execute repeatedly, without this conversation's context, to work through all 30
findings tier by tier — committing as it goes and keeping `priyam_problems.md` accurate at
every step.

**Section boundary:** this loop touches **only** `crates/iris-core/src/{crypto,identity,security}/`,
their tests, `crates/iris-core/benches/crypto.rs`, `crates/iris-core/tests/crypto_e2e.rs`, the
`security/` proptest regressions, and the three `docs/bug-hunting/priyam_*.md` files. Anything
else — `message.rs`, `error.rs`, `kani_proofs.rs`, `message_engine/*`, the platform crates —
is another section's territory (§8).

---

## 1. The invariant this loop must never break

**The tree must be green after every commit.** No commit is made unless
`cargo build -p iris-core` succeeds and the relevant test scope passes
(`cargo test -p iris-core crypto:: identity:: security::` at minimum for the touched area). If
a fix cannot be made to pass, it is reverted in the same wake cycle — the loop never leaves a
broken build for the next wake or for a human. This is non-negotiable and takes priority over
"making progress."

**A second invariant, specific to this section:** *a fix must never widen an accept decision
without explicit sign-off.* Section 1 is the trust boundary. If a change would cause the
security layer to accept a message, chain, advertisement, rotation, or SOS that it currently
rejects, that change is Tier 2 and gated (§2.3) — even if it looks like a bug fix. Narrowing
(rejecting more) is lower-risk; widening (accepting more) needs a human.

## 2. Tier order and gating

Run tiers **in this order**:

1. **Tier 0** (7 findings) — invariant-correct fixes with no cross-section dependency. Start
   here. These are: a missing crypto check (PRY-5), a broken arithmetic assumption (PRY-7,
   PRY-14), a panic (PRY-19), an API footgun (PRY-22), a memory leak (PRY-28), and corrupted
   crypto constants (PRY-31). None changes an accept/deny decision in a way that needs
   sign-off; all are net-safer (each only *adds* rejected inputs or fixes an internal bug).
2. **Tier 1** (17 findings) — hardening and hygiene: bounded, self-contained, mostly
   `security/` / `identity/` / doc-level. No ordering dependency among them beyond the
   per-finding `Dependencies` lines. Run smallest-first.
3. **Tier 2 — HARD STOP** (12 findings). Before starting Tier 2 the loop must stop and require
   an explicit human go-ahead recorded in `priyam_fix_log.md`. Reason: every Tier 2 finding
   changes a security *invariant* — what the ACL authorises (PRY-1, PRY-9, PRY-10, PRY-11),
   what the storage layer is told to accept/evict (PRY-2, PRY-3), what the replay engine
   trusts (PRY-6), what chains validate (PRY-17), whether a peer can be permanently locked out
   (PRY-4). Several name **Section 2** (the message engine) or the **EMERG-001 / safety**
   owners in their `Dependencies` field and need that owner's agreement before code lands. The
   loop may *prepare* Tier 2 (read every finding, draft a fix plan per finding, list which
   ones need which owner's sign-off) without confirmation, but may not commit Tier 2 code
   until a human says so in the fix log.

There is no Tier 3/4 — the report is small enough that doc reconciliation is folded into the
relevant findings (PRY-26, and the doc-update clauses in PRY-2/PRY-12/PRY-16).

## 3. Authoritative tier membership

Source of truth for "which tier is this finding in." Computed once by the rule in §7; pasted
here as a static list so the loop never re-derives it.

- **Tier 0** (7): PRY-5, PRY-7, PRY-14, PRY-19, PRY-22, PRY-28, PRY-31
- **Tier 1** (17): PRY-8, PRY-12, PRY-13, PRY-15, PRY-16, PRY-18, PRY-20, PRY-21, PRY-23,
  PRY-24, PRY-25, PRY-26, PRY-27, PRY-29, PRY-30, PRY-35, PRY-36
- **Tier 2** (12 — GATED, human sign-off required before any commit): PRY-1, PRY-2, PRY-3,
  PRY-4, PRY-6, PRY-9, PRY-10, PRY-11, PRY-17, PRY-32, PRY-33, PRY-34

Total: 7 + 17 + 12 = **36**, matching the report (revision 2).

## 4. Per-finding fix protocol

For every finding, in this exact order:

1. **Read the finding in full** from `priyam_problems.md` — `What`, `Evidence`, `Why it is
   wrong`, `Root cause`, `Fix`, and `Dependencies / blast radius`. The `Fix` field is a
   starting sketch, not a spec to copy verbatim — re-derive the correct fix from the
   `Evidence` and current source, since the source may have drifted.
2. **Re-read the cited source lines before touching anything.** Line numbers drift. Confirm
   the defect is still present as described. If an earlier finding's fix already subsumed it,
   mark `✅ Fixed (subsumed by PRY-N)` and move on without new code. (PRY-1↔PRY-12,
   PRY-2↔PRY-3, PRY-3↔PRY-30 are the likely subsumption pairs.)
3. **Check `Dependencies / blast radius`.** If a Tier 2 finding names Section 2 / EMERG-001 /
   safety and no agreement is recorded in `priyam_fix_log.md`, mark
   `🔒 Blocked · reason: needs <owner> sign-off` and move to the next finding. Do not guess the
   other owner's intent.
4. **Check the widen-vs-narrow test.** If the fix would make the security layer accept
   something it currently rejects (a chain, an SOS, an advertisement, a message), and the
   finding is not already Tier 2, **stop and re-tier it** — flag in the fix log, do not
   proceed under Tier 0/1 rules.
5. **Write the fix.** Smallest change that removes the defect. Do not refactor unrelated code.
   If the finding recommends a CI guard (PRY-22) or a `Cargo.toml` feature pin (PRY-25), add
   it in the same commit.
6. **Test.** At minimum `cargo build -p iris-core`. Then the narrowest scope that exercises
   the fix:
   - crypto findings → `cargo test -p iris-core crypto::`
   - identity findings → `cargo test -p iris-core identity::` (+ `--test crypto_e2e` if the
     change touches the DH / key-directory path, e.g. PRY-5)
   - security findings → `cargo test -p iris-core security::`
   - proptest-backed areas (replay, quota, rate_limiter, acl, spam, reputation) →
     `cargo test -p iris-core --features proptest <module>::`
   If the finding's `Fix` says "add a regression test" or names a test, **write it** — a fix
   without a test that would have caught the original bug is incomplete. The test must fail
   against pre-fix code (prove it discriminates) before it passes against the fix.
7. **On success:** commit (format §6), then update `priyam_problems.md`:
   - Flip the finding's `Fix status` line to
     `✅ Fixed · Tier N · commit <short-hash> · <date>`.
   - Update the Progress Tracker tier roll-up table (decrement ⬜, increment ✅) and its
     `Last updated` line.
   - Append a row to `priyam_fix_log.md` for this finding.
8. **On failure** (build breaks, test fails, or the fix is wrong once written):
   `git checkout` the changed files back to the last good commit — never commit a red tree.
   Mark the finding `❌ Reverted · reason: <what broke, one sentence>`. Do not retry the same
   finding more than once per wake; a second failure escalates to
   `🔒 Blocked · reason: attempted and reverted, needs human review`.

## 5. Tier-complete protocol

When every finding in a tier is `✅`/`🔒`/`⚪` (nothing `⬜`/`🔵`):

1. Run the full crate build and test suite once more:
   `cargo build -p iris-core --all-targets && cargo test -p iris-core` **plus**
   `cargo test -p iris-core --features proptest security::` — this catches interactions
   between fixes within the tier that per-finding scopes miss (the four security engines share
   the `SecurityPolicy` facade).
2. If green: make a **tier-checkpoint commit** touching only `priyam_problems.md` /
   `priyam_fix_log.md` (roll-up now shows the tier complete) — no code in this commit.
3. If any `🔒 Blocked` remain, list them and their reasons in the checkpoint commit message so
   a human scanning `git log` sees what needs attention.
4. Advance to the next tier. If the next tier is **Tier 2, STOP** — do not proceed without the
   §2.3 sign-off.

## 6. Commit format

Per-finding commits:
```
fix(<area>): <one-line summary matching the finding title>

<one paragraph: what was wrong, what changed, per the finding's Root cause / Fix fields>

Finding: PRY-<N> (Tier <N>)
Ref: docs/bug-hunting/priyam_problems.md
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
```
`<area>` is one of `crypto`, `identity`, `security`, `acl`, `replay`, `quota`,
`rate-limiter`, `reputation`, `spam`, `trust-store`, `chain`.

Tier-checkpoint commits (report-only):
```
docs(bug-hunting): Priyam Tier <N> complete

<X> fixed, <Y> blocked (listed below).
PRY-<id>: <reason>

Ref: docs/bug-hunting/priyam_problems.md
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
```

**Commits are local only.** This loop never runs `git push`. It never runs on the default
branch without branching first (per the harness git rules). Pushing is a separate, explicit
human decision after reviewing the history this loop produces.

## 7. Regenerating the tier table (only if the report changes)

If `priyam_problems.md` is re-audited or findings are added/removed, recompute §3 with this
rule (top to bottom, first match wins; `fid` = finding id, `sev` = its severity, `widens` =
"does the fix make the security layer accept something it currently rejects?"):

```
tier(2)  if widens(fid) == true
tier(2)  if fid.Dependencies names "Section 2" or "Section 3" or "EMERG-001" or "safety owner"
tier(2)  if sev in {"High","Medium"} and fid changes an accept/deny/evict/send decision
tier(0)  if fid is a panic, a missing/corrupted crypto check, a broken arithmetic assumption,
         an API footgun, or a memory leak — AND does not widen AND has no cross-section dep
tier(1)  otherwise
```
Rationale: Section 1 is the trust boundary, so the split is by *blast radius of being wrong*,
not by severity alone. A High finding whose fix only narrows (rejects more) and is
self-contained could sit in Tier 1; a Medium finding that widens an accept decision is Tier 2.

## 8. Safety limits for one wake cycle

- **Fix at most 6 findings per wake cycle**, then stop and yield (schedule the next wake).
  Keeps each session's diff reviewable and each `git log` entry meaningful. Tier 2 findings
  count as **2** against this budget each (they are riskier and need more verification).
- **Never modify files outside** `crates/iris-core/src/{crypto,identity,security}/`, their
  tests, `benches/crypto.rs`, `tests/crypto_e2e.rs`, `crates/iris-core/proptest-regressions/security/`,
  and `docs/bug-hunting/priyam_*.md`. If a fix seems to require touching `message_engine/`,
  `emergency/`, `message.rs`, `error.rs`, `kani_proofs.rs`, `Cargo.toml` (except the PRY-25
  feature pin, which is explicitly permitted), or a platform crate, **stop** and mark
  `🔒 Blocked · reason: fix requires changes outside Section 1 ownership, needs owner review`.
  PRY-32 / PRY-33 / PRY-34 have their Section 1 half in scope but land their fix at a
  Section 2 call site — Tier 2, blocked until that owner agrees.
- **Never delete a test to make it pass.** If a fix appears to require weakening or removing an
  existing test, that is a red flag — mark
  `🔒 Blocked · reason: fix conflicts with existing test <name>, needs review`. The exception:
  a test that *encodes the vulnerability as expected behaviour* (e.g.
  `huge_message_size_no_overflow` asserting `total_usage() == u64::MAX` for PRY-2,
  `stale_counter_link_rejected` for PRY-17) may be **rewritten** to assert the corrected
  behaviour — but this must be called out explicitly in the fix-log entry, never done quietly.
- **Never widen an accept path under Tier 0/1 rules.** (§4 step 4.) If in doubt, it is Tier 2.
- **If three consecutive findings in a wake fail (§4 step 8), stop the wake early.** Usually
  means something upstream changed (a dependency, a toolchain issue) that needs a human.
- **Environment honesty:** if `cargo` is not on PATH / Rust is not installed in the execution
  environment (as was the case for parts of taksh's loop — see `taksh_fix_log.md` toolchain
  notes), the loop may still write fixes but must mark them
  `✅ Fixed · PENDING LOCAL BUILD` (not plain `✅`) and record in the fix log that the build
  gate was reasoned-through, not run. A human promotes to `✅` after a real
  `cargo test -p iris-core` pass. Proptest/Kani legs that cannot run locally are recorded
  `PENDING PROPTEST` / `PENDING KANI` the same way.

## 9. How to actually run this

This file is resumable and idempotent: reading `priyam_problems.md`'s Progress Tracker at the
start of any wake tells you exactly where to continue — which tier is active, which findings
are still ⬜/🔵, and (via §4 step 8's revert logic) that the tree is already clean if a prior
wake ended mid-finding.

Recommended invocation: a recurring `/loop` with dynamic self-pacing, re-entering this file
each wake, prompt equivalent to:

> Continue the fix loop specified in `docs/bug-hunting/priyam_problems_loop.md`. Read the
> Progress Tracker in `docs/bug-hunting/priyam_problems.md` for the current tier and next
> unfixed finding. Follow §4 (per-finding protocol), §5 (tier-complete), §8 (safety limits).
> Stop and report if you reach the Tier 2 gate (§2.3), if §8's three-consecutive-failures
> condition fires, or if a fix would widen an accept path (§4 step 4).

Do not run this loop unattended for an extended period without first running one manual wake
cycle and reviewing the diff and commit it produces — confirm the protocol is followed as
written before letting it run over many wakes. **Tier 2 must never run unattended.**
