# Integration Fix Log — cross-section bug-hunt execution journal

**Companion to:** [`integration_problem_loop.md`](integration_problem_loop.md) (logic) ·
[`integration_problems.md`](integration_problems.md) (state).
This file records each fix run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.

Baseline at run 1 start (`3e70a87`, clean tree): report published with 8
findings, 0 fixed. `cargo build --workspace` clean at this commit (verified
during the original sweep — see `integration_problems.md` §6 "What was
checked").

**Environment honesty:** this environment has no Android SDK/Gradle toolchain
and no Xcode/iOS toolchain — only `cargo` is runnable here. Any finding whose
fix lands in `android/*.kt` or `ios/*.swift` (or requires regenerating UniFFI
Swift bindings) is verified by careful source reading and cross-repository
grep, not by an actual platform build. Those fixes are marked `✅ Fixed ·
PENDING BUILD VERIFICATION` per `integration_problem_loop.md` §4 step 6, and
must be promoted to a bare `✅ Fixed` by a human running the real platform
build, same discipline `taksh_fix_log.md` uses for `PENDING LIVE-PG` and
`PENDING HARDWARE VERIFICATION`.

---

## Run log

No runs yet. The next wake cycle should start at Tier 0 (CROSS-001) per
`integration_problem_loop.md` §2 and record its result here using the format
below.

---

### Run entry format (for the next run to follow)

```
## Run N — <date> — <what this run targeted>

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | CROSS-00X | ✅ Fixed [· PENDING BUILD VERIFICATION] | <short-hash> | <what was checked, what a human still needs to run if pending> |

**Run closeout:** <build/test result summary>.

**Carried notes for later runs:** <anything the next run needs to know>.

### Next run
<which finding(s) are next per the loop's tier order>
```
