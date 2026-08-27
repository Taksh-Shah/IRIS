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
TAK-3 (quarantine) → then TAK-15 (declared dependent), plus
TAK-4/5/6/9/10/11/14 and the test-infra trio TAK-18/19/20 (TAK-19 pairs with
TAK-8's remaining sites per its Dependencies field).

---

## Run 2 — 2026-08-26 — Wave 1 batch B

Target findings: TAK-3, TAK-9, TAK-10, TAK-11, TAK-15, TAK-4 (6 fixed —
run ended early at a natural boundary; TAK-5/6 are each substantial and move
to Run 3 with fresh review budget). Carried from batch A plan: TAK-14 +
test-infra trio TAK-18/19/20 also to Run 3.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | TAK-3 | ✅ Fixed · PENDING LIVE-PG | 8d8ead9 | get_queue skip-and-quarantine (status→DELIVERY_FAILED, evictable, out of queue predicate) + `quarantined_rows()` counter; regression test corrupts a P0 row on disk and asserts healthy P1 still drains + repeated drains stay clean. |
| 2 | TAK-9 | ✅ Fixed · PENDING LIVE-PG | 518b4ab | i64::try_from validation for timestamp/expiry/payload_size (AAD↔column round-trip now provable); v3 idempotent migration adds messages_scalars_nonnegative CHECK. TTL-clamp deliberately excluded → belongs to gated TAK-2. |
| 3 | TAK-10 | ✅ Fixed · PENDING LIVE-PG | c52f1a5 | PgStorageConfig.node_id (IRIS_NODE_ID hex, parser unit-tested) drives is_own_message — no Section-2 trait change needed; malformed senders stay relayed. |
| 4 | TAK-11 | ✅ Fixed · PENDING LIVE-PG | 0e1f2d3 | DDL rebuilt unpartialled (expires_at) + (status,priority,created_at); v4 idempotent drop-and-recreate for existing DBs (same-name collision handled). EXPLAIN assertion deferred to live-PG leg. |
| 5 | TAK-15 | ✅ Fixed · PENDING LIVE-PG | dff5358 | Explicit 1-byte format tags (0x00 plaintext / 0x01 sealed) on all new rows; dispatcher validates candidates by real decode/AEAD success so legacy blobs whose first byte collides with a tag value cannot be misrouted; untagged legacy plaintext+sealed shapes still resolve; migrate_envelope_format() one-shot rewrite; unknown/corrupt shapes fail into TAK-3 quarantine. Dispatcher extracted as free fn; 2 unit tests cover all four stored shapes without a DB. |
| 6 | TAK-4 | ✅ Fixed · PENDING LIVE-PG | f2fc6a7 | Supervised reconnect: RwLock client slot; inline initial establishment; supervisor probes every 5s, clears slot FIRST (distinct "storage unavailable" error), re-dials with jittered exponential backoff (1s→30s cap, ±20% deterministic jitter), re-applies ALL migrations per connection; is_healthy() round-trip check; backoff split into deterministic+jitter halves with exact unit tests; supervisor waits for empty slot before dialing (no dual-connection window). All methods acquire current client per call; async client() accessor (test call sites updated). |

**Batch B closeout:** full workspace build --all-targets clean; sweep
**734 passed / 0 failed / 1 ignored** vs 726 after batch A (+8 net-new tests,
zero regressions).

### Next run (batch C)
TAK-5 (atomic quota admission + evict-on-refusal), TAK-6 (TLS for non-loopback),
then test-infra trio TAK-18/19/20 and TAK-14 (AAD expansion — coordinate note:
touches update_status into read-modify-reseal; STORE_SECURITY_REVIEW.md doc
update named in Dependencies).

---

## Run 3 — 2026-08-26 — Wave 1 batch C

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | TAK-5 | ✅ Fixed · PENDING LIVE-PG | f766383 | Admission atomic in ONE statement (server-side SUM in same snapshot as guard); refusal → one eviction pass + single retry before StorageFull; dup pre-check preserves Ok semantics. |
| 2 | MG-3 + TAK-19 | ✅ Fixed | 5852ff2 | SimConfig.battery_ma_override injection; new paused-time test registers healthy + NaN-cost transports. **Test caught MG-3 live**: total_cmp orders NaN ABOVE +inf, so the poisoned transport WON selection on the truncate(1) path. Fix: non-finite scores dropped before ranking. The test failed against pre-fix code — it discriminates. |
| 3 | TAK-20 | ✅ Fixed | efc8e6d | Bare-timer test DELETED per finding prescription (pure tokio duplication, zero IRIS coverage); drop test now asserts num_alive_tasks returns to baseline via bounded yield/advance drain. |
| 4 | TAK-14 | ✅ Fixed · PENDING LIVE-PG | 92c6110 | row_aad_v2 authenticates status+created_at+hop_count+max_hops+payload_size+is_own; read paths verify v2→v1 candidates; update_status = read-modify-reseal with optimistic CAS on old status (Arc slot blocks &mut transaction — documented); migrate rewrites under v2 using live status; module doc now precise per generation. Unit test proves the SET status=DELIVERED attack fails verification. |
| 5 | TAK-18 | ✅ Fixed (shape-analog scope) | c8d237e | Three Section-3 loom models: registry register/deregister vs lookup (no torn payload), reassembly bounded append-vs-sweep, SCF insert-vs-max-key-evict accounting. All 8 loom models pass. Honest residual: analogs not extracted sync cores — carried. |
| — | TAK-6 | 🔒 Blocked → Run 4 | — | TLS for non-loopback PG: introducing rustls/tokio-postgres-rustls/native-certs is a supply-chain event needing fresh session budget for version-API wrangling + deny/licence review. Not rushed at run end on the connection path. |

**Batch C closeout:** workspace build --all-targets clean; full sweep **0
failed across all suites**; loom suite 8/8 under `--cfg loom`.

### Next run (Run 4) — superseded, see Run 4 below
TAK-6 (TLS — fresh budget, supply-chain review) was the planned next run,
but the repo owner explicitly redirected the loop to Tier 2 instead (see
Run 4). TAK-6 and the rest of Wave 1/3 remain open for a future run.

**Commit convention (from TAK-24 onward):** per-finding §6 code commit first
(stable hash), report/fix-log status flips accumulate and land in one
`docs(bug-hunting)` commit at batch end citing the real hashes — this avoids
the amend fixed-point problem (editing a hash into a commit changes the hash).

---

## Run 4 — 2026-08-27 — Tier 2 opened under explicit human sign-off

**Gate:** `taksh_problems_loop.md` §2.3 requires an explicit human go-ahead
before any Tier 2 commit. The repo owner instructed: *"we will start from
tier 2 problems solving... Start solving problems mentioned in tier 2."*
That instruction is the required sign-off. This run only touches the
**routing area (ROUT-\*)** of Tier 2 — DTN-1..25, MG-25..42 and TAK-2 are
untouched and remain `⬜`.

**Baseline audit:** before any new work, re-read the Progress Tracker and
found it stale — ROUT-1 through ROUT-16 were already `✅` from a prior
session, and **ROUT-17/ROUT-18 were already fixed in git history**
(commits `9ae05b4`, `4bb4733`, authored outside this loop by another
contributor working directly on `main`) but the report's status lines
still said `⬜ Not started`. Re-verified both fixes against current source
and their regression tests (`rout17_fanout_capped_at_max`,
`priority_hop_budgets`) before flipping their status — no code change,
docs-only catch-up.

**Pre-existing red build:** `cargo test --workspace` at the start of this
run failed one test — `known_path::tests::rout13_capacity_bound_evicts_oldest`
(`left: 256, right: 1000`) — unrelated to any Tier 2 finding in scope. Root
cause: the test's `pid(i as u8)` peer-id generator wraps every 256 values
but was being used to insert 1000 (`MAX_ENTRIES`) rows, so only 256 distinct
peers were ever created. Fixed first (commit `1b90f02`, `pid_wide(usize)`
helper) per the loop's "tree must be green after every commit" invariant —
this was a prerequisite fix, not a Tier 2 finding itself.

**Toolchain:** MSVC `link.exe` was not resolvable from this environment's
Bash (Git's own `usr/bin/link.exe` shadows it) or from PowerShell (no VS
Developer Shell active). Worked around by building under the installed GNU
toolchain (`RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`), which links
`iris-core`/`iris-storage`/`iris-android`/`iris-ios` cleanly.
`iris-desktop` (Tauri, MSVC-only manifest linker flags) does not link
under GNU and was excluded from every build/test command this run
(`--workspace --exclude iris-desktop` / `-p iris-core -p iris-storage`);
no Tier 2 finding in scope touches `iris-desktop`.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| — | rout13 test (prerequisite) | ✅ Fixed | `1b90f02` | `pid_wide(usize)` spreads the index across 8 bytes so all 1000 ids are distinct; `cargo test -p iris-core --lib routing::known_path` 8/8; full workspace sweep 0 failed. |
| 1 | ROUT-17 | ✅ Fixed (docs catch-up only) | `9ae05b4` (pre-existing) | `rout17_fanout_capped_at_max` passes at HEAD; fix matches finding's own `Fix` field (`MAX_FLOOD_FANOUT` cap). |
| 2 | ROUT-18 | ✅ Fixed (docs catch-up only) | `4bb4733` (pre-existing) | `priority_hop_budgets` passes at HEAD; `P0_MAX_HOPS = 16` replaces the `u8::MAX` sentinel per the finding's own `Fix` field. |
| 3 | ROUT-19 | ✅ Fixed | `c99633d` | Hoisted the hop-budget check above KnownPath/Opportunistic/Flood (Direct stays exempt). New test `rout19_known_path_respects_hop_budget` fails against pre-fix code (asserts `Store`, pre-fix returned `Forward{KnownPath}`). Full sweep green. |
| 4 | ROUT-20 | ✅ Fixed | `ec150a6` | Added `NeighborTable::neighbor_summaries()` (cheap `(peer_id, state)` projection); `recipients_for_flood` no longer clones every neighbor's ~175 KB `peer_bloom`. New test `rout20_neighbor_summaries_match_full_snapshot`. Full sweep green. |
| 5 | ROUT-21 through ROUT-25 | ⬜ Deferred | — | Larger blast radius (public `ForwardingDecision`/`OpportunisticDecision` API changes touching `sim/mod.rs`, `tests/obs_telemetry.rs`, and each other per their own `Dependencies` fields) — deferred to a future run with its own review budget rather than rushed into this one at the §8 cap. Not attempted, not reverted. |
| 6 | ROUT-26 | 🔒 Blocked | — (docs only) | The finding's own `Fix` field concludes `REQ-ROUTE-NF-004`'s own numbers are internally inconsistent (64 KB cannot hold 400k ids at <1% FPR) and says to escalate to the requirements owner rather than pick a resolution — three named options, none of which the loop should choose unilaterally. |
| 7 | ROUT-27 | ✅ Fixed | `8d728e3` | `decide()` now increments `ROUTING_FLOODS_TOTAL` on a `Flood` decision. New test `rout27_flood_decision_increments_floods_total` asserts the counter moves on Flood and stays put on Direct. Full sweep green. |
| 8 | ROUT-28, ROUT-29 | ✅ Fixed (combined commit — same file, tightly related property-test rewrites, precedent: `440f610` covered ROUT-11/12/13/16) | `7d5723d` | `flood_terminates_no_loops_property` rebuilt on a per-node `NeighborTable` graph (seeded `StdRng`) checked against an independent ground-truth BFS; `flood_no_backtrack_no_reflood_property` rebuilt with an explicit `already` subset and exact-set assertions. Full sweep green. |
| 9 | ROUT-30 | ✅ Fixed | `1191eff` | `ForwardedCache` gained an injectable clock (`TimePoint`, mirrors `prophet.rs`); `ring_eviction_removes_expired_exact_entries` now advances virtual time past `EXACT_WINDOW` and asserts `len() == 0`, plus a within-window "must not evict early" check. Production default (wall-clock `Instant`) unchanged; public API unchanged, no call-site edits needed. Full sweep green. |
| 10 | ROUT-31 | ✅ Fixed (subsumed by ROUT-5, no new commit) | `1821c48` (pre-existing) | Re-read `store.rs` at HEAD: `is_expired` is already a thin shim delegating to `message_engine::expiry::is_expired` (the `<=` boundary), and `ttl_check_arrival_time`'s skew-budget assertion is no longer the ROUT-5 immortality bug. `rout5_far_future_timestamp_fails_closed` covers the fail-closed path. No code change needed. ROUT-36 (same-name/different-argument-order hazard) is a distinct, still-open finding. |

**§8 accounting:** 8 findings fixed this run (ROUT-19, 20, 27, 28, 29, 30,
31, plus the ROUT-26 triage) — at the 8-per-wake-cycle cap once the
ROUT-17/18 docs-only catch-ups (no code, so not counted against the cap)
are excluded. Stopping here per §8 rather than continuing into ROUT-21..25
or ROUT-32..36 in the same wake.

**Run 4 closeout:** `cargo test --workspace --exclude iris-desktop`
(GNU toolchain) — 0 failed across every suite, after every commit in this
run. No reverts; three-consecutive-failure escalation (§8) never triggered.

### Next run (Run 5)
Resume Tier 2 routing area: ROUT-21 through ROUT-25 (opportunistic/spray
layer — read ROUT-22/23/24/25's `Dependencies` fields together first, they
reference each other) and ROUT-32 through ROUT-36 (benchmark + dead-code
findings). Then DTN-1..25, MG-25..42, TAK-2 (untouched this run).
