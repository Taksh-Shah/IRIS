# Tier 4 Fix Log — Section 3 bug-hunt execution journal

**Companion to:** [`taksh_problems_loop.md`](taksh_problems_loop.md) (logic) ·
[`taksh_problems.md`](taksh_problems.md) (state).
This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.
Commits are local-only per §6 of the loop file.

Baseline at run 1 start (`cd3bb01`, clean tree): `cargo build --workspace` clean ·
`cargo test --workspace` = **717 passed / 0 failed / 1 ignored**.

Drift audit (run 1): all 20 Wave-1 findings re-verified present at HEAD against
current sources; zero subsumed; citations accurate.

**Environment honesty:** no Postgres/Docker on this host and `IRIS_PG_PASSWORD`
unset — every PG-backed test skips (this is TAK-1's subject, Tier 1). Storage
SQL changes are verified by compile + clippy + non-PG unit tests + source
reasoning; live-PG regression legs are recorded below as PENDING LIVE-PG and
must be promoted by a human run with a real database, same discipline as
Tier 0's PENDING HARDWARE VERIFICATION.

---

## Run 1 — 2026-08-26 — Wave 1 batch A

Target findings (8): TAK-25, TAK-24, TAK-12, TAK-13, TAK-16, TAK-17, TAK-21,
TAK-22 (smallest/most self-contained first; dependency-safe order).
Deferred within Wave 1 to later runs: TAK-3 → then TAK-15 (declared dependent),
TAK-4, TAK-5, TAK-6, TAK-9, TAK-10, TAK-11, TAK-14, TAK-18, TAK-19 (+TAK-20).

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | TAK-25 | ✅ Fixed | ee7abdb | 2 new unit tests (`threshold_defaults_when_absent_or_invalid`, `threshold_accepts_in_range_values`); iris-storage lib 9/9; clippy 0; fmt clean. Doc: STORAGE.md §Quota now documents `IRIS_EVICTION_THRESHOLD` (named in finding's Dependencies). |
| 2 | TAK-24 | ✅ Fixed · PENDING LIVE-PG | 1cda4a9 | `created_at DESC`→`ASC`; regression test `eviction_tie_break_removes_oldest_first` (old-big/new-small full-tie construction — buggy order provably empties store, fixed order survives newest row). Compiles + suite-skips locally (no Postgres). |
| 3 | TAK-12 | ✅ Fixed · PENDING LIVE-PG | 5960080 | Loop rewritten: batch 256/pass, ≤64 passes, ERROR-level exhaustion trace (metrics registry not plumbed into iris-storage — recorded as carried limitation), P0-only residue break retained. Build+clippy clean. |
| 4 | TAK-13 | ✅ Fixed | 9f3a526 | `MissedTickBehavior::Delay` pinned; cadence extracted to DB-free `gc_loop`. Tests: `missed_gc_ticks_are_delayed_not_burst` (production Delay raced vs Burst control on identical virtual timeline) + `gc_loop_survives_job_failures`. tokio-test added to dev-deps. |
| 5 | TAK-16 | ✅ Fixed | 868f42a | Hand-written Debug for `StorageKeySealer` + `PgStorageConfig`, `[redacted]` secrets. Tests assert no key byte / password fragment renders. |
| 6 | TAK-17 | ✅ Fixed | 87c4af9 | `Drop` zeroizes the at-rest key (zeroize w/ derive added crate-side). Compile-level assurance by construction. |
| 7 | TAK-21 | ✅ Fixed | 3e22a34 | `FeatureVec::new` sanitises non-finite → 0.0 (total, deterministic, allocation-free); debug_assert removed. Regression test pins NaN/±inf handling + bounded score. Division sites in sim/mod.rs left to Tier 3 (SIM scope) — sanitisation makes them non-cascading. |
| 8 | TAK-22 | ✅ Fixed | ff9e9e4 | `predict_next` saturates; fit clamps period to documented `MAX_PERIOD_MS` (1 y of sim-ms). Median sort moved to `total_cmp` — **pre-closes predictor.rs:90 of TAK-8** (that entry will mark this site subsumed when Wave 2 reaches it). |

**Batch A closeout:** full workspace build + sweep green — **726 passed / 0 failed /
1 ignored** vs baseline 717 (+9 net-new tests, zero regressions).

**Carried notes for later runs / owners:**
- PG-gated regression legs for TAK-12/24 (and upcoming storage findings) are
  PENDING LIVE-PG — promote with a human run with a real database.
- Pre-existing fmt drift in `ble.rs`/`wifiaware.rs` from committed Tier 0 work
  (not touched by this loop — those files belong to the operator's Tier 0
  pass; a `cargo fmt` belongs in its ACCEPT step).
- TAK-8's predictor.rs:90 site is already closed by TAK-22.

### Next run (batch B)
TAK-3 (quarantine) → then TAK-15 (declared dependent on TAK-3), plus
TAK-4/5/6/9/10/11/14 and the test-infra trio TAK-18/19/20 (TAK-19 pairs with
TAK-8's remaining sites per its Dependencies field).

**Commit convention (from TAK-24 onward):** per-finding §6 code commit first
(stable hash), report/fix-log status flips accumulate and land in one
`docs(bug-hunting)` commit at batch end citing the real hashes — this avoids
the amend fixed-point problem (editing a hash into a commit changes the hash).
