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

---

## Run 5 — 2026-08-27 — Tier 2 routing area (ROUT-22..36), continued under the same sign-off

Owner said "continue with the next batch" — same Tier 2 sign-off as Run 4,
no new gate needed. This run finished every ROUT-\* finding except the
interdependent ROUT-21/24/25 cluster (deliberately deferred, see below).

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | ROUT-22 | ✅ Fixed | `eba51b6` | Added `sprayed_to: HashSet<(MessageId, PeerId)>` to `OpportunisticRouter`; `spray()` now takes `message_id`, rejects a repeat contact without touching the budget, records on success. Cleared by `prune_dp_seen()` alongside `max_dp_seen`. New tests `rout22_spray_does_not_reflood_same_contact` (fails against pre-fix code — pre-fix a repeat contact got a second copy), `rout22_prune_clears_sprayed_to`. `spray()` had zero production callers before this fix (only its own test; `sim/mod.rs` reimplements spray independently), so the signature change has no production blast radius. Full sweep green. |
| 2 | ROUT-23 | 🔒 Blocked | — (docs only) | Re-reading both governing specs while investigating the fix found they **disagree with each other**: `OPPORTUNISTIC_ROUTING.md` gives P1=5/P2=3/P3=2; `ROUTE2_DESIGN.md` gives P1=L(8)/P2=L(8)/P3=max(4,…). Only the P4+ ("1, direct-only") row is consistent between the two. There is no single spec to align the code to for P1-P3, and unilaterally picking a value (or inventing a third) risks a real delivery-behavior regression across every P1-P4 sim/production path. This is exactly the "wrong fix is worse than the bug" case the loop's own gate exists for — needs a routing/product owner to reconcile the specs (or affirm the current code and update both docs) before a code fix can be written with confidence. |
| 3 | ROUT-34 | ✅ Fixed | `f824638` | Deleted `flood.rs::has_live_transport` and `store.rs::_destination_guard` (both `#[allow(dead_code)]`, zero callers). Removed `ForwardingAlgorithm::Flood`/`::Store` and their `algorithm_label` match arms — verified zero construction sites anywhere in the crate for `Forward { algorithm: Flood \| Store }` (those outcomes use `ForwardingDecision`'s own top-level `Flood`/`Store` variants). The `flood.rs` no-op `links` block this finding also named was already gone, subsumed by ROUT-20's rewrite in Run 4. Full sweep green. |
| 4 | ROUT-32, ROUT-33 | ✅ Fixed (combined — same file, one coherent bench rewrite) | `1e641c9` | Rewrote `benches/routing.rs`. ROUT-32: `opportunistic_decide_50_candidates` now generates a fresh message id per iteration instead of reusing one across the whole run (was hitting `gtmx_advantage`'s per-message `seen` rejection after ~50 iterations, ROUT-7). ROUT-33: added `Clone` to `DeliveryPredictability` so `prophet_meet_32_snapshot` and `prophet_age_1000_k5` (renamed) use `iter_batched` against a fixed 1,000-entry baseline instead of mutating one shared table (fixes the non-stationary-cost and K=0-aging defects in one move); moved the `Vec` clone into the untimed setup phase; added a real `spray_binary_handoff` bench (the header's original, previously-fictional claim) and five new L0-hot-path benches (`try_direct`, `try_known_path`, `recipients_for_flood_10_neighbors`, `forwarded_cache_is_duplicate_miss`, `engine_decide_flood_path`) — the class of bench that would have caught ROUT-20's 175 KB-per-neighbor clone immediately. Verified with `cargo test -p iris-core --bench routing` (criterion's single-pass mode): all 9 benchmarks execute successfully. `cargo clippy --bench routing`: zero warnings originating from the bench file itself. Full sweep green. |
| 5 | ROUT-35 | ✅ Fixed (subsumed by ROUT-19, no new commit) | `c99633d` (pre-existing, Run 4) | Re-read `mod.rs` at HEAD: the `if hop_count < policy.max_hops { … } else if hop_count >= policy.max_hops { /* empty */ }` pair the finding named no longer exists — ROUT-19's hop-budget-gate rewrite (Run 4) replaced it with a single hoisted early-return gate. No empty branch remains. |
| 6 | ROUT-36 | ✅ Fixed | `07338e5` | Renamed `routing::store::is_expired` to `ttl_expired` — the same-name/different-argument-order collision with `message_engine::expiry::is_expired` (which stays untouched) can no longer compile silently after an import swap; it now fails to resolve. Updated both call sites (`routing/mod.rs`'s ROUT-6 gate, `scf.rs`'s three calls) and `store.rs`'s own tests. No behavior change. Full sweep green. |

**ROUT-21, ROUT-24, ROUT-25 — not attempted, not reverted.** These three
are mutually dependent (ROUT-25's fix depends on ROUT-22 [now done] and
ROUT-24; ROUT-24 needs real neighbor-DP candidates sourced from
`discovery/handshake.rs`'s `CapabilityBundle`, not yet wired to
`decide_inner`) and each touches a public API with a wide blast radius:
ROUT-21 adds a `transport` field to `ForwardingDecision::Forward`/`Flood`
(re-exported from `lib.rs`, consumed by `tests/obs_telemetry.rs`,
`sim/mod.rs`, every routing test, and any FFI consumer); ROUT-25 adds an
`OpportunisticDecision::SprayHandoff` variant (also public, `lib.rs:48`).
Starting any one of them mid-batch at the §8 cap risked leaving the tree
in a half-migrated state across a wake boundary. Saved for a dedicated run
with a full review budget instead.

**§8 accounting:** 6 findings fixed this run (ROUT-22, 34, 32+33 combined,
35, 36) plus the ROUT-23 triage — under the 8-per-wake-cycle cap, leaving
headroom deliberately unused rather than starting the ROUT-21/24/25
cluster with only partial budget left.

**Run 5 closeout:** `cargo build -p iris-core -p iris-storage --all-targets`
clean; `cargo test --workspace --exclude iris-desktop` (GNU toolchain) — 0
failed across every suite, after every commit in this run. No reverts;
three-consecutive-failure escalation (§8) never triggered. **The ROUT area
of Tier 2 is now complete except ROUT-21/24/25 (deferred) and ROUT-23/26
(blocked, need owner sign-off).**

---

## Run 6 — 2026-08-27 — ROUT-21/24/25, the last cluster; ROUT area fully closed out

Owner said "continue with the next batch" — same Tier 2 sign-off. Read
`ROUTE2_DESIGN.md` and `discovery/handshake.rs`'s `CapabilityBundle`
first, per Run 5's own plan.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | ROUT-21 | ✅ Fixed (combined with ROUT-25 — see below) | `5ffffb0` | `ForwardingDecision::Forward` gained `transport: TransportId`; `Flood`'s `recipients` became `Vec<(PeerId, TransportId)>`. New `NeighborTable::links_to` (single-peer link lookup, no `peer_bloom`/`capabilities` clone) and free fn `best_transport` (picks best-quality live link — confirmed `LinkQuality` is ordered Excellent < Good < Fair < Poor, lower = better, from `known_path.rs::upsert`'s own comment, so this is `min_by_key`). `NeighborSummary` (ROUT-20) extended with `links` so flood's per-recipient selection needs no extra lookups. `try_direct`/`try_known_path`/`recipients_for_flood` now select a transport and fall through if a `LinkedUp` neighbor has no selectable link, rather than returning a decision with nothing to send over. New tests: `rout21_best_transport_picks_best_quality_not_first_or_last`, `rout21_best_transport_none_when_no_links`. |
| 2 | ROUT-24 | 🔒 Blocked | — (docs only) | Investigating the fix found its premise false: `discovery/handshake.rs::CapabilityBundle` (the finding's proposed DP source) has no DP field at all (verified by reading the struct: `node_id`, `protocol_version`, `transports`, capability tags, bloom params, `timestamp` — nothing else), and `routing/mod.rs::record_contact` hard-codes `other_predictions: &[]` on every call with a comment deferring the real exchange to "SIM/transport wiring" that only `sim/mod.rs`'s own separate simulation code actually implements. A correct fix needs a new wire-protocol feature (a DP field + handshake version bump), which is (a) not named in this finding's own `Location` field, (b) a security-relevant change since peer-supplied DP data would directly steer routing decisions, and (c) belongs to the discovery/handshake owner, not a routing-module wiring fix. Per §8's "outside named scope" rule, blocked rather than invented unilaterally. |
| 3 | ROUT-25 | ✅ Fixed (combined with ROUT-21 — same `decide_inner` block, both need `transport_to`) | `5ffffb0` | `OpportunisticRouter` gained `spray_budgets: HashMap<MessageId, SprayBudget>` (cleared alongside `max_dp_seen`/`sprayed_to` in `prune_dp_seen`) and a `spray_fallback` method (creates the budget via `l_for_priority` on first use, shares it across repeated calls for the same message, delegates to `spray()` for ROUT-22's dedup). New `DeliveryPredictability::has_entry` distinguishes "truly never seen" from "DP decayed to 0.0" — `p_for(dst) == 0.0` alone can't tell the two apart. Wired a `RoutingEngine::spray_fallback` method into `decide_inner` after the GTMX+ consult: fires only when this node has no DP entry at all for the recipient, picks the best-linked live neighbor (excluding sender/recipient) as the spray contact. Note: `OpportunisticDecision` already had no separate `SprayHandoff` variant to add by the time this ran — `spray()` already returns `ForwardTo{reason: Spray}`, reusing `ForwardTo` with a reason tag; the original finding's evidence predates that. New tests: `rout25_spray_fallback_fires_when_cold_start`, `rout25_spray_fallback_shares_one_budget_across_calls`, `rout25_spray_fallback_declines_when_dp_history_exists` (opportunistic.rs, unit level), `rout25_cold_start_sprays_instead_of_flooding`, `rout25_warm_destination_still_floods` (mod.rs, full `decide()` chain). **Does not depend on ROUT-24** despite the finding's own `Dependencies` field — spray needs a live contact, not that contact's DP, so it shipped with ROUT-24 still blocked. |

**Why ROUT-21 and ROUT-25 are one commit:** both touch the same
`decide_inner` block (the Algorithm 2.5 step), and `spray_fallback`'s own
`Forward` decision needs ROUT-21's transport lookup — splitting them would
have left the tree red partway through. Same precedent as `440f610`
(ROUT-11/12/13/16) and this tier's own `7d5723d`/`1e641c9`.

**Run 6 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop`
(GNU toolchain) — **643 iris-core lib tests, 0 failed**, every other
workspace suite 0 failed. `cargo test -p iris-core --bench routing`
(criterion single-pass mode) — all 9 benchmarks still execute
successfully; `ForwardingDecision`'s new shape doesn't change any bench's
black-boxed usage. `cargo clippy -p iris-core --lib --no-deps` — zero
warnings from any file touched this run. No reverts; three-consecutive-
failure escalation (§8) never triggered.

**The ROUT-\* area of Tier 2 (36 findings) is now fully closed out**:
every finding is either ✅ (33) or 🔒 with a documented, specific reason
(3: ROUT-23 — two governing specs conflict with each other; ROUT-24 — the
real fix needs a new security-relevant wire-protocol feature outside this
area's scope; ROUT-26 — the governing requirement doc is internally
self-inconsistent). None of the three blocked findings were guessed at;
all three explicitly need a human/product/security-owner decision.

---

## Run 7 — 2026-08-27 — DTN area opened; core scf.rs storage/eviction cluster (8 findings)

Owner said "continue with the next batch" — same Tier 2 sign-off. Read
§8 (Area B — DTN) in full per Run 6's plan: 25 findings across
`scf.rs` (12), `scf_contact.rs` (4), `scf_eviction.rs` (1), `prophet.rs` (8).
Re-read every cited line against current source before starting — the
report's line numbers had drifted slightly but the described defects were
all still present and unfixed.

**Batch selection:** DTN-1, 2, 5, 6, 7, 8, 9, 10 — all seven scf.rs
findings that touch the SAME core mechanism (`buffer_message` /
`evict_until` / `StoreKey` / byte accounting), fixed as one rewrite rather
than piecemeal. This was a deliberate choice, not just convenience:
DTN-9's ordering fix is a *precondition* for DTN-8's O(1) eviction lookup
to be correct (a naive `next_back()` without first fixing `StoreKey`'s
`Ord` would silently invert the expiry tie-break), and DTN-1/DTN-7 turned
out to share one enforcement mechanism entirely (see DTN-7's row). Left
for future runs: DTN-3, DTN-4, DTN-11 (semi-independent — see "Next run"
below) and the PRoPHET aging/capacity cluster DTN-12..19, plus the
lifecycle-correctness cluster DTN-20..22 and the scf_contact.rs cluster
DTN-23..25.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | DTN-1 | ✅ Fixed | `c0c1007` | `buffer_message` now compares `total_bytes + need` against `max_bytes` on every insert, calls `evict_until` if over, and returns `ScfError::StorageFull` if that still isn't enough. New test `dtn1_capacity_rejects_when_full` (P0 fills the budget, a second message with nothing evictable is rejected). |
| 2 | DTN-2 | ✅ Fixed | `c0c1007` | Added `index: HashMap<MessageId, StoreKey>`; `buffer_message` replaces an existing entry for a replayed id instead of duplicating it; `ScfConfig::max_messages` (default 100_000) caps entry count independent of bytes. New tests `dtn2_replayed_message_id_replaces_not_duplicates`, `dtn2_max_messages_cap_rejects_beyond_count`. All by-id accessors (`priority_rank_of`, `status_for`, `mark_forwarded`, `delivery_status`) now route through the index — O(1) instead of O(n) `.iter().find()`. |
| 3 | DTN-5 | ✅ Fixed | `c0c1007` | `mark_forwarded(.., acked=true)` now removes the entry via a new `remove_entry` helper (shared with `buffer_message`'s replace path, `reap_expired`, `evict_until`) after reporting the terminal status by value. Fallout fixed, not weakened: `buffer_and_lifecycle_status` (scf.rs), the M6 integration test (`routing/mod.rs`), and — the one with real behavioral risk — `sim/mod.rs`'s `forward_to` dedup check at line ~319, which used to read `scf.delivery_status(&id).is_some()` to detect "dst already carries/delivered this message"; since a delivered entry no longer exists to query, that check now also consults `self.nodes[dst].delivered` (the sim's own permanent delivery record), or a later contact could re-deliver and double-count the same message. `relay_marks_relay_node_carry_state` updated to assert `None` post-delivery instead of `Some(Delivered)`. |
| 4 | DTN-6 | ✅ Fixed | `c0c1007` | `ttl_seconds` clamped to `MAX_TTL_SECS` (30 days) in-place at buffer time; `StoreKey::new` derives `expiry_unix` from `stored_at` (locally observed) instead of the claimed `envelope.timestamp`. New test `dtn6_ttl_seconds_is_clamped`. Note: the far-future-*timestamp* half of this finding's attack was already closed by ROUT-5 (`message_engine::expiry::is_expired`'s skew-suspect fail-closed handling, this same session, commit `1821c48`) — this fix closes the remaining unclamped-`ttl_seconds` half and the arrival-time-basis half. |
| 5 | DTN-6 | (see row 4 — one commit) | — | — |
| 6 | DTN-7 | ✅ Fixed (scope-narrowed, see report note) | `c0c1007` | Closed by DTN-1's reject-on-insert enforcement alone: `evict_until` still exempts P0, but `buffer_message`'s post-eviction capacity re-check now rejects *any* message (P0 included) once nothing evictable remains and the ceiling would still be exceeded — `usage()` can no longer climb past `max_bytes` under a P0-only flood. The *Fix* field's secondary "give P0 its own 25% sub-quota with oldest-first internal eviction" was deliberately not implemented — not required to close the unbounded-growth defect, and would need new config surface + an undefined-in-the-report internal ordering. Documented as a scope note on the finding itself, not silently dropped. |
| 7 | DTN-8 | ✅ Fixed | `c0c1007` | `usage()` now reads an incrementally-maintained `total_bytes: u64` field (O(1)) instead of folding the whole buffer; `evict_until`'s victim lookup is `buffer.keys().next_back()` (O(log n)) instead of an O(n) `max_by` scan — evicting m of n messages is now O(m log n), not O(m*n). Depends on DTN-9's `Ord` fix landing first (same commit) for `next_back()` to pick the correct victim. |
| 8 | DTN-9 | ✅ Fixed | `c0c1007` | `StoreKey` no longer derives `Ord`; hand-written impl: `priority_rank` ASC, `expiry_unix` DESC within a band, `message_id` tertiary — so `next_back()` yields exactly the worst eviction candidate (highest priority_rank, then soonest-expiring), matching what the old independently-correct `max_by` comparator computed. `ordered_buffered`'s doc comment corrected to describe the real (now internally consistent) order — no caller depends on a specific forward order today (`scf_contact.rs` re-sorts from scratch), so no behavioral fix was needed there, only the comment. `store_key_ordering_priority_then_expiry` updated: `c.cmp(&a)` flips from `Less` to `Greater` under the corrected semantics. |
| — | DTN-9 | (see row 8 — one commit) | — | — |
| 9 | DTN-10 | ✅ Fixed | `c0c1007` | `approx_bytes` now sums every heap-allocated field (`sender_id`, `recipient_id`, `payload`, `signature`, `encryption_hdr`, `routing_hints`' variable parts, `auth_cert_chain`) instead of a flat `payload.len() + 256`. `FIXED_OVERHEAD_BYTES = 192` chosen deliberately so a 32-byte sender/recipient id with no optional fields (every existing test envelope's shape) accounts identically to the old formula (192+32+32=256) — zero recalibration needed for any test not exercising the new fields. New test `dtn10_approx_bytes_counts_cert_chain_and_hints` (a 10 KB cert chain must move accounted bytes by at least 10 KB). |

**Test/call-site fallout, all fixed not weakened (per §8's "never delete a
test to make it pass" — these are genuine behavior-contract changes, not
dodges):** `scf.rs`'s `buffer_and_lifecycle_status`,
`store_key_ordering_priority_then_expiry`; `scf_eviction.rs`'s
`p7_evicted_before_p1`, `p0_never_evicted_under_pressure`,
`eviction_ordered_lowest_priority_then_soonest_expiry` (all three
previously relied on `buffer_message` admitting messages unconditionally
as test *setup*, then triggering eviction via a separate later call — now
given real budget headroom for every insert to succeed, with an explicit
`evict_until` target reproducing the original eviction scenario);
`routing/mod.rs`'s M6 integration test; `sim/mod.rs` (dedup check +
`relay_marks_relay_node_carry_state`, see DTN-5's row);
`tests/obs_telemetry.rs`'s SCF eviction-telemetry test (needed budget
headroom for the message to be admitted before the later forced
eviction — a `max_storage_bytes: 0` config is no longer a valid way to
force "admitted-but-over-budget" now that insertion is enforced).

**Run 7 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop`
(GNU toolchain) — **648 iris-core lib tests, 0 failed**, every other
workspace suite 0 failed (including `sim`/`ml_experiments`/`sim_scenarios`,
which were the ones most likely to be sensitive to the SCF behavior
changes — all green with no anchor-value updates needed).
`cargo clippy -p iris-core --lib --no-deps` — zero warnings from any file
touched this run. No reverts; three-consecutive-failure escalation (§8)
never triggered.

**§8 accounting:** 8 findings fixed this run (DTN-1, 2, 5, 6, 7, 8, 9,
10), landed in one commit because they are one coherent rewrite of the
same mechanism — at the 8-per-wake-cycle cap. Stopping here rather than
starting DTN-3/4/11 with no budget left in this wake.

---

## Run 8 — 2026-08-27 — DTN-3/4 (relay premise), DTN-11 (eviction policy wiring), DTN-21/23/24/25 (lifecycle + contact ranking)

Owner said "continue with the next batch" — same Tier 2 sign-off. Per
Run 7's plan: DTN-4 (Critical) and DTN-3 first, as one coherent unit.

**DTN-3 + DTN-4** (commit `b58de3d`): re-read both against current source
(`scf.rs`'s `messages_forwardable_to`, `scf_contact.rs`'s `on_new_contact`)
— both still present exactly as described. Investigating DTN-4's suggested
`oracle.is_better_carrier` fix found the same gap as ROUT-24 (this tier,
earlier): no wire protocol exchanges a live contact's own DP, so "is
contact better than me" cannot be computed. Chose the achievable core fix
instead: any live, not-yet-offered message is now a relay candidate for
any contact (not only the exact recipient) — DTN-4's actual "zero
messages delivered" defect — gated by DTN-3's new
`offered: HashMap<MessageId, HashSet<PeerId>>` (recorded by
`mark_forwarded`, cleared by `remove_entry`) instead of the old global
`forward_attempts` counter that could lock a message out after meeting
just three different peers. `recipient_is` became dead code once both its
call sites were gone (removed at DTN-21, below) and was deleted then, not
here — it still had one caller (`status_for`) at this point. Test fallout:
`forwardable_to_contact_only` renamed
`forwardable_to_includes_relay_candidates` (its own old name described
the bug). New tests: `dtn3_already_offered_peer_is_skipped_but_others_are_not`,
`dtn3_meeting_several_peers_does_not_lock_message_out` (the finding's own
trigger scenario). `sim/mod.rs` doesn't call either function (confirmed
by grep) — zero simulation blast radius, confirmed by the full sweep.

**DTN-11** (commit `ede33f1`): added `ScfEngine::with_device_class`
(mirrors `with_telemetry`/`with_virtual_clock`) instead of changing
`ScfEngine::new`'s signature as the finding's *Fix* field suggested — the
constructor has real production callers (`sim/mod.rs`, `routing/mod.rs`)
and a builder achieves the same reachability without breaking them.
Needed a new crate-visible `set_max_bytes` (scf.rs) since `max_bytes` is
private and `scf_eviction.rs` is a sibling module. `StoreKey::new` and
`evict_until` now route through `ScfEvictionPolicy::eviction_weight`/
`is_evictable` instead of re-deriving the same values inline — same
numeric result, so every existing test passed unchanged; two new tests
(`dtn11_with_device_class_lowers_the_insert_time_ceiling`,
`dtn11_with_device_class_never_raises_the_configured_ceiling`) prove the
new builder actually reachable, not just present.

**DTN-21, DTN-23, DTN-24, DTN-25** (commit `357ddb7`, four independent
findings batched for review convenience): DTN-21 deleted `status_for`'s
"proximity means Delivered" fabrication (exactly the one-line,
zero-blast-radius fix flagged in Run 7's plan) — and, as a consequence,
`recipient_is` lost its last caller and was deleted too. DTN-23 gave
`ForwardCandidate` its own `priority_rank` (read at construction, not
re-derived via a fallible by-id lookup with an `unwrap_or(7)`
silent-demote) and replaced two separate sorts with one deterministic
comparator; incidentally this is now a *real* test of mixed probabilities
since DTN-4 made relay candidates (0.6) coexist with exact matches (0.9).
DTN-24 gave `enqueue_forward` a real `Result` (`ScfError::NotFound` on a
reaped/evicted candidate) instead of an unconditional `Ok(())`; zero
callers today, confirmed by grep, so free to fix now. DTN-25 gave
`ForwardCandidate` an `approx_bytes` field and made `on_new_contact`'s
budget parameter bytes (renamed `bandwidth_limit_bytes: Option<u64>`)
instead of a raw message count — needed `ScfEngine::approx_bytes` made
`pub(crate)`. Only one test call site used the old count semantics
(updated with explicit byte math in a comment); the one production-path
caller (`routing/mod.rs`) always passes `None`.

**Run 8 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop`
(GNU toolchain) — **657 iris-core lib tests, 0 failed**, every other
workspace suite 0 failed. `cargo clippy -p iris-core --lib --no-deps` —
zero warnings from any file touched this run. No reverts;
three-consecutive-failure escalation (§8) never triggered.

**§8 accounting:** 7 findings fixed this run (DTN-3, 4, 11, 21, 23, 24,
25) across three commits — one short of the 8-per-wake-cycle cap.
DTN-22 was considered as the 8th but deliberately deferred: its fix
requires either implementing missing `DeliveryStatus` state transitions
(no mobility/carry signal exists anywhere in this codebase to drive them)
or deleting the unreachable variants, and the finding's own text flags
that the enum "reaches the Kotlin/Swift/TS layers" — an FFI/binding
exhaustive-match blast-radius check (searching `android/`, `ios/` for
matches on `DeliveryStatus`) is needed before choosing either path, and
that check had not been done with wake budget remaining.

## Run 9 — 2026-08-27 — DTN-22 (FFI check + dead-state deletion), DTN-20 (ack timeout/backoff), DTN-15/16/18 (PRoPHET meet() cap + self-exclusion)

Owner repeated the standing instruction to continue Tier 2 under the same
sign-off. Per Run 8's plan: DTN-22 first (with the flagged FFI check),
then DTN-20, then as much of the PRoPHET cluster as fits in budget.

**DTN-22** (commit `1453620`): ran the FFI/binding blast-radius check
flagged in Run 8 — grepped `crates/iris-android`, `crates/iris-ios`, and
every UniFFI scaffolding surface (`uniffi::setup_scaffolding!`, no `.udl`
files exist in this repo) for any reference to `DeliveryStatus::Carrying`,
`AwaitingContact`, `ForwardingInProgress`, or `DropReason`. Zero matches —
none of the three unreachable variants cross the FFI boundary, so
deletion (not implementation) was the correct path. While confirming
`Dropped`'s own reachability, found it had *also* gone dead — as an
unintended side effect of Run 7's DTN-5 fix (which removed the
status-mutation-before-discard `reap_expired` used to do). Re-derived the
fix from current source rather than the original finding text (per the
loop's own instruction to do so when source has drifted) and deleted
`Dropped`/`DropReason` alongside the three named variants. `is_terminal()`
now matches only `Delivered`; module doc comment rewritten to describe
the actual (not aspirational) state machine.

**DTN-20** (commit `959152a`): added `last_forward_attempt: Option<u64>`
to `StoredMessage` (the field DTN-20's own *What* section says the spec
requires and the impl lacks), set by `mark_forwarded` on every
non-acked call. Added `reap_ack_timeouts(&mut self, timeout_secs: u64) ->
Vec<MessageId>` — exponential backoff (`timeout_secs * 2^forward_attempts`,
capped at `2^16`), resets timed-out entries to `Stored` and clears that
specific peer from DTN-3's `offered` set so the *same* peer becomes a
valid retry target again rather than being permanently locked out by its
own failed attempt. No production caller wired in yet (no periodic sweep
task exists anywhere in this crate to call it from — confirmed by grep,
same class of gap as DTN-4/ROUT-24) — the finding's own scope was "close
the state-machine gap," which this does; wiring a sweep scheduler is a
separate, larger architectural addition not implied by this finding's
Location/Fix fields.

**DTN-15, DTN-16, DTN-18** (commit `e516e42`, batched — all three live in
`meet()`'s Eq.1/Eq.3 paths and the fix is one coherent rewrite): DTN-15
(Critical) — `meet()` only checked `MAX_DP_ENTRIES` once, at the top of
the call; the Eq. 3 transitivity loop then inserted one new entry per
element of the caller-supplied slice with no cap and no re-check, so a
single hostile `meet()` could grow the table by (slice_len − 1) entries.
Added `evict_worst_unless(&mut self, protect: &[PeerId])`, called before
every new insert in both the Eq.1 direct-contact path and the Eq.3 loop —
capacity is now enforced per-insert, not per-call. The transitivity loop
is also now bounded to `TOP_N_DP` entries of the incoming slice and
rejects non-finite (NaN/Infinity) predictions outright. DTN-18 — added an
optional `self_id: Option<PeerId>` field with a `with_self_id` builder
(kept optional, matching `with_virtual_clock`/`with_telemetry`: a
required constructor param would have cascaded into `RoutingEngine`,
which has zero identity concept today, confirmed by grep). `meet()` now
excludes `self_id` from the transitivity loop (a neighbor's snapshot
legitimately contains `P(neighbor, self)`, and writing that into our own
table let a remote peer inject an arbitrary self-DP value); `snapshot()`
filters `self_id` out of outbound predictions; the transitivity write is
now clamped to the same `1 − delta` bound Eq. 1 already respects (closing
the second, independent path to an out-of-bound value the finding
flagged). DTN-16 — `capped_entries_evict_lowest` used `pid(i as u8)`
against a loop bound of `(MAX_DP_ENTRIES + 10) as u8`; the cast truncates
4106 to 10, so the test only ever inserted 10 of the 4106 peers its own
name claimed, and its only assertion (`len() <= MAX_DP_ENTRIES`) is
trivially true even with eviction deleted entirely. Replaced with a
widened `pid_wide(u32)` helper and a real scenario: seed one deliberately
low-DP entry via transitivity, fill to capacity with ordinary
higher-DP direct contacts, push one more peer in, then assert the cap
holds *exactly* (not `<=`) and that the low-DP entry specifically is the
one gone. Added two more regression tests: DTN-15's own trigger shape
(one `meet()` call fed a slice 4x larger than `MAX_DP_ENTRIES`, capacity
still holds) and non-finite-input rejection. All 7 pre-existing
`prophet.rs` tests pass unchanged, confirming the opt-in builder pattern
preserved every existing call site's behavior.

**Deliberately deferred:** DTN-12, 13, 14, 17, 19 — the `age()`/
`last_meet` aging cluster. Separable from the `meet()`-side fix above
(different function, different bug family) and `age()` currently has
zero non-benchmark production callers (confirmed by grep, noted in
`prophet.rs`'s own doc comment added this run), so the cluster's urgency
is lower than DTN-15's live single-packet remote-growth path. Held for a
future run rather than folded in here to keep this commit's diff
reviewable as one coherent change.

**Run 9 closeout:** `cargo build -p iris-core --all-targets` clean after
each of the three changes; `cargo build --workspace --exclude
iris-desktop --all-targets` clean; `cargo test --workspace --exclude
iris-desktop` (GNU toolchain) — **669 iris-core lib tests, 0 failed**,
every other workspace suite 0 failed. `cargo clippy -p iris-core --lib
--no-deps` — zero warnings from any file touched this run (14 pre-existing
warnings elsewhere, all in `transport/*.rs`, untouched). No reverts;
three-consecutive-failure escalation (§8) never triggered.

**§8 accounting:** 5 findings fixed this run (DTN-22, 20, 15, 16, 18)
across three commits — under the 8-per-wake-cycle cap.

## Run 10 — 2026-08-27 — DTN-12/13/14/17/19 (PRoPHET aging cluster) — DTN area of Tier 2 fully closed out

Owner said "continue" — same standing Tier 2 sign-off. Per Run 9's plan:
the PRoPHET aging cluster as one unit, the last DTN work remaining.

Read all five findings against current `prophet.rs` before writing any
code (the file had changed substantially since these were reported —
DTN-15/16/18's commit added `self_id`, `evict_worst_unless`, and a
per-insert eviction path inside `meet()` that this cluster's fix now
also has to interact with).

**DTN-12** (commit `9b823e0`): confirmed via grep that `age()`/`prune()`
still have zero non-production callers (one benchmark each) and that no
"routing tick" or scheduler abstraction exists anywhere in `crates/` —
the finding's own suggested fix ("call age() from the routing tick")
assumes infrastructure that was never built, the same gap DTN-4, DTN-20,
and DTN-24 already hit this session. Wired `age()`'s logic into `meet()`
instead, via a shared private `age_at(now)` helper both call — `meet()`
is the one function every production DP mutation already passes through
(`opportunistic.rs::on_contact` and `routing/mod.rs::record_contact`'s
direct `predictions_mut().meet()` call both funnel through it), so this
gives Eq. 2 a real caller on every contact without adding a scheduler.

**DTN-13** (same commit): the old `age()` reset its baseline to `now` on
every call regardless of whether a whole interval had elapsed, discarding
the sub-interval remainder — so any cadence faster than `aging_interval`
(guaranteed by DTN-12's fix, since `meet()` can fire far more often than
the 30-minute default) made decay a permanent no-op, silently
reintroducing DTN-12's own bug immediately after fixing it. Fixed by
advancing the baseline by exactly the whole intervals consumed
(`t + interval·k`) instead of resetting it. New test
`dtn13_sub_interval_cadence_ages_identically_to_one_infrequent_call`
proves 25 one-second ticks and one 25-second gap now converge to the
identical final DP.

**DTN-14** (same commit): `last_meet` served two unrelated clocks — Eq.
1's "time since last direct contact" and Eq. 2's aging baseline — so
`age()` overwriting it on every pass silently pinned every peer's
encounter-interval scaling at its ceiling. Split into `last_encounter`
(meet's direct-contact branch only) and `aging_base`. Here the finding's
own suggested split ("aging_base written only by age()") turned out
wrong once actually traced through: an entry's first-ever aging pass
would always read `k=0` with no prior baseline, and since DTN-12 makes
that first pass typically happen a long time after insertion (aging now
runs lazily, at whatever contact happens to come next), the entry would
get a free pass for its entire dormant period instead of decaying —
worse than the original bug, not a fix. Verified this by hand-tracing a
"25 frequent ticks vs. one infrequent age() call after the same elapsed
time" scenario before writing any test, confirmed the two diverged under
the literal suggested split, and re-derived the correct design: `meet()`
seeds `aging_base` at insertion/update time (both the direct and
transitive write paths), and `age_at` only ever *advances* it. New test
`dtn14_aging_does_not_clobber_the_encounter_interval_clock` (50
intervening aging ticks, then a real re-contact 301s later, asserts
`P_encounter` is still interval-scaled below its ceiling, not pinned).

**DTN-17** (same commit): clamped the `k as i32` cast (wraps negative for
`k > i32::MAX`, and `gamma < 1` raised to a wrapped-negative power is
`+inf`) and guarded the result — a non-finite or out-of-range outcome is
discarded to `0.0` rather than trusted. New test
`dtn17_large_k_never_produces_a_non_finite_dp`.

**DTN-19** (same commit): deleted `prune()`'s dead `Vec` clone (fed only
a tautological `debug_assert` using the same predicate as the retain
beside it) and the verbatim-duplicated `last_meet.retain` call. `prune()`
is now one retain per map. New test
`dtn19_prune_drops_only_entries_below_threshold`.

**DTN-18 follow-on** (same commit, not a separately numbered finding):
`age_at` now genuinely runs on every contact, so the self-identity entry
(fixed at `1 - delta` by `with_self_id`) needed an explicit exemption
from both decay and threshold-discard in `age_at`, and from discard in
`prune` — otherwise DTN-18's own "P(A,A)=1" invariant would quietly stop
holding the moment aging became real. New test
`dtn18_self_entry_never_decays_even_after_many_aging_passes` (20 aging
passes via unrelated contacts, self entry unchanged). This also retired
the "known gap" doc comment DTN-15/16/18's commit had left on `age()` —
it described exactly this interaction; no longer a gap now that it's
handled.

**Test fallout from DTN-12 alone:** wiring `age_at` into every `meet()`
call means anything below `p_first_threshold` (0.1 default) is now
discarded as unrelated cleanup on the very next `meet()` call, regardless
of decay — this is `age()`'s pre-existing, always-there discard
behavior, just newly reachable. `capped_entries_evict_lowest` (DTN-16,
previous commit)'s seed value (0.05, chosen before this run's work
existed) fell below that floor and would have been removed by threshold-
discard before the test's own capacity-eviction assertion could run —
numerically still passing but for the wrong reason. Raised to 0.6 (yields
~0.267, comfortably above threshold, still below the ~0.495 direct-
contact fill values) so the test again exercises what its own comments
claim. The pre-existing `aging_decays_by_gamma_per_interval` needed no
change at all — it already matches the meet-seeds-the-baseline design.

**Run 10 closeout:** `cargo build -p iris-core --all-targets` clean;
`cargo test -p iris-core --lib routing::prophet` — **17/17 pass** (11
pre-existing + 6 new); `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop`
(GNU toolchain) — **675 iris-core lib tests, 0 failed** (up from 669 by
exactly the 6 new tests), every other workspace suite 0 failed —
including `tests/ml_experiments.rs` and `tests/sim_scenarios.rs`, which
DTN-12's own blast-radius note flagged as likely to shift once aging
actually ran; neither did. `cargo clippy -p iris-core --lib --no-deps` —
zero warnings from `prophet.rs` (14 pre-existing warnings elsewhere,
untouched). No reverts; three-consecutive-failure escalation (§8) never
triggered.

**§8 accounting:** 5 findings fixed this run (DTN-12, 13, 14, 17, 19) in
one commit — under the 8-per-wake-cycle cap. This closes the DTN area of
Tier 2 entirely: all 25 DTN-\* findings are now fixed.

## Run 11 — 2026-08-27 — MG-25/26/27/28/29 (gateway trust/health/scoring cluster)

Owner said "complete leftout work in tier 2" — a broad continuation of
the same standing sign-off, covering all remaining Area E (gateway, 18
findings) plus TAK-2. Read the full current `gateway/mod.rs` (1282 lines
at the time) before touching anything — the finding text's line-numbered
evidence snippets were close to current source but not exact.

Planned the remaining 19 findings into ~5 runs respecting the 8-per-wake
cap, opening with the cluster MG-30's own text calls out explicitly:
"MG-25/26/27/29 ... will all go live simultaneously the moment somebody
wires the manager in — fix those first." MG-28 isn't named there but
lives in the same health-monitor file and composes with the others.

**MG-26** (part of commit `1038a83`): `prune()` deleted `states`/
`failure_counters`/`re_admission_strikes` for any gateway no longer in
the known set — a blackhole cleared every strike by dropping off the
neighbor table for one reconcile cycle and re-advertising, making
`HardFailed` unreachable against a competent adversary. Fixed: `prune()`
keeps a Failed/HardFailed record regardless of current known-ness,
bounded by a new `MAX_QUARANTINE_ENTRIES` (256) — persistence itself
opens a growth vector (an attacker cycling fresh PeerIds), so the bound
had to land in the same commit, not as a follow-up.

**MG-28** (same commit): `score_factor` auto-promoted a probation
gateway to full trust once `RECOVERY_WINDOW` elapsed, no delivery
required. The finding cites two specs that describe the intended
recovery rule slightly differently (one implies a `Failed` gateway
should decay into probation-eligibility purely by elapsed time, via a
new `failed_since` timer; the other says the window is just "how long
probation lasts before elapsing to full trust", closer to the existing
structure). Picked the simpler reading both specs agree on regardless —
full trust must be earned by a delivery, never granted by time alone —
over the more elaborate suggested fix, since the finding's own verdict
already flagged the spec citation as possibly overstated.

**MG-25 + MG-27 + MG-29** (same commit, the cluster's own cross-reference
— "fix together... as one pessimistic-defaults + confidence-factor
change"): the only production path building a `GatewayCapability`
(`reconcile`) left `latency_ms`/`cost_factor`/`current_load` at
`GatewayCapability::new`'s defaults, and those were the *optimistic*
extreme for a formula where they're a *reward* — an unmeasured, self-
tagged attacker gateway scored 0.785, beating a real measured satellite
gateway. Changed the defaults to pessimistic and extended
`sanitize_capability` (previously 4 float fields only) to re-derive
`max_priority` from `gateway_type` unconditionally and clamp
`bandwidth_bps`/`latency_ms`/`queue_depth`. Added a `confidence_factor`
(0.3 at zero recorded successes, ramping to 1.0 over 5) applied
alongside health in `select_inner`. MG-25's cryptographic-verification
layer (Ed25519 signature check via `identity::trust_store`, wired
through `discovery::ingest_handshake`) explicitly not implemented — that
call chain has zero production callers today (confirmed by grep, the
same gap MG-30 documents), so it's a separate, larger cross-module task,
not a contained fix. The scoring fix closes MG-25's own described
trigger path regardless (its trigger scenario explicitly routes through
MG-29's fabricated defaults).

**Run 11 closeout:** `cargo build -p iris-core --all-targets` clean;
`cargo test -p iris-core --lib gateway::` — **23/23 pass** (18
pre-existing + 5 new/rewritten, including a full hand-trace of every
pre-existing test against the new confidence multiplier before running,
confirmed against the actual result); `cargo build --workspace --exclude
iris-desktop --all-targets` clean; `cargo test --workspace --exclude
iris-desktop` — **679 iris-core lib tests, 0 failed**; `cargo clippy -p
iris-core --lib --no-deps` — 0 warnings from `gateway/mod.rs`.

**§8 accounting:** 5 findings fixed this run (MG-25, 26, 27, 28, 29) in
one commit — under the cap.

## Run 12 — 2026-08-27 — MG-31/32/33 (event emission, self as candidate, hysteresis); MG-30 scoped

**MG-31** (commit `71c2893`): `adopt_from_neighbor`/`withdraw` returned
`Option<TopologyEvent>` for a caller to forward, but the only caller
(`reconcile`) discards both, and `record_ack_timeout` built no event at
all on a failure transition. Added a `broadcast::Sender<TopologyEvent>`
(mirrors `TransportManager::topology_tx`) and a `topology_events()`
subscribe method; all three mutation points now publish regardless of
whether their own return value is consumed.

**MG-32:** `self_capability` fed only `is_gateway()`/`advertised_tags()`
— a node holding the only internet uplink in the neighbourhood reported
"no gateway available" for its own traffic. Added an optional `local_id`
via a new `with_local_id` builder (not a required constructor arg — kept
all 13 existing `GatewayManager::new()` call sites, all tests,
unaffected) and pushed self onto `select_inner`'s scored candidates when
both `self_capability` and `local_id` are set, bypassing the health
monitor and confidence ramp (no ACK-timeout concept for routing to
yourself; a node always knows its own uplink is real).

**MG-33:** `select` was a stateless pure function recomputed every call
— the one varying production input (`reliability_score`, from an
RSSI-tier `LinkQuality` that flips at a boundary) could swap the ranking
of two similar gateways between calls, fragmenting a multi-part transfer
across two gateways with no shared session state. Added a `SWITCH_MARGIN`
(0.10) sticky-election mechanism, scoped to the `Single` (P3+) path only
— `WithBackup`/`All` have no single incumbent for "stickiness" to mean
anything for. `select`/`select_at`/`select_inner` become `&mut self` to
hold a new per-priority `elected: HashMap<MessagePriority, PeerId>`.

**MG-30:** confirmed (again, against current source) that
`GatewayManager` has zero production callers and `GW_VERIFICATION.md`
renders a false "IMPLEMENTED" verdict. Did not wire the composition-root
integration the finding's fix text describes — that activates a new,
security-sensitive live-routing path spanning the message engine's send/
receive decision points, not a contained change to this finding's own
Location. Did not edit `GW_VERIFICATION.md` either — it lives under
`docs/implementation/`, outside the loop's file scope and not named in
this finding's own Location field. Added an honest "Integration status"
note to the module's own doc comment instead.

Introduced 3 `let_underscore_must_use` clippy warnings using
`let _ = sender.send(...)` for the new broadcast sends; fixed by
switching to `.send(...).ok()`, matching the idiom `TransportManager`
already uses elsewhere in this crate — caught by the clippy check before
committing, not after.

**Run 12 closeout:** `cargo build -p iris-core --all-targets` clean;
`cargo test -p iris-core --lib gateway::` — **28/28 pass** (23
pre-existing + 5 new, including hand-computed hysteresis-margin numbers
verified against the actual test run); `cargo build --workspace
--exclude iris-desktop --all-targets` clean; `cargo test --workspace
--exclude iris-desktop` — **684 iris-core lib tests, 0 failed**; `cargo
clippy -p iris-core --lib --no-deps` — 0 warnings from `gateway/mod.rs`
after the `.ok()` fix (14 baseline, unchanged).

**§8 accounting:** 4 findings fixed this run (MG-30, 31, 32, 33) in one
commit — under the cap.

## Run 13 — 2026-08-27 — MG-34/35/36/37/38 (dead fields, eviction policy, self-uplink defaults, PeerId privacy, Clone removal)

**MG-34:** five fields stored, never read for any decision. Wired
`hop_count` into scoring via a new `HOP_DISCOUNT` (0.85 per hop)
multiplier applied in `select_inner` (kept outside
`compute_gateway_quality` itself so that function stays a pure,
directly-testable mapping of one capability). Deleted `path` (always
empty, never read anywhere) and its `adopt_from_neighbor` parameter — 21
call sites updated via a small Node script that bracket-matched each
call and stripped the trailing `vec![]`/`Vec::new()` argument, verified
against the diff before running tests. Deleted `last_success`
(write-only, fully redundant with `success_counts` added in Run 11).
Kept `transport_id` (a required constructor parameter with real
descriptive value) and `primary_score` (legitimate public output for
`select()`'s not-yet-written external caller, per MG-30 — not the same
class of "dead" as internal write-only bookkeeping) — a per-field
judgment call, not a blanket delete-everything-named pass.

**MG-35:** capacity eviction picked least-recently-confirmed — the exact
metric a Sybil flood maximizes for free. Replaced with
`weakest_gateway_for_eviction`: scores each candidate the way
`select_inner` would, and prefers evicting any zero-success candidate
over one with a delivery on record, only reaching into the proven set if
every current candidate has proven itself. `withdraw` now takes a real
`reason` instead of a hardcoded `"stale"` that mislabeled capacity
evictions in the logs too.

**MG-36:** used live `cost.bandwidth_available_bps` (capped by the
datasheet ceiling) instead of only the static figure, and a named
`REFERENCE_EXPENSIVE_COST_PER_KB` reference rate instead of an
undocumented `/10.0` that made this crate's own worked example
(₹0.50/KB) read as "essentially free". Result now also runs through
`sanitize_capability`. Did not build a real ACK-rate EWMA for
`reliability_score` (documented) — no such tracker exists anywhere in
this crate and this function has no access to delivery history.

**MG-37:** fixed at the root instead of at the 4 named call sites —
`PeerId` no longer derives `Debug`; a manual impl prints `short()`
instead, closing the privacy leak for every struct that embeds a
`PeerId` crate-wide. Grepped for exact-match `{:?}`-format test
assertions before changing it (none found — the finding's own "cosmetic"
characterization held up against a real check, not just trusted).

**MG-38:** removed `Clone` from `GatewayManager`/`GatewayHealthMonitor`
(verified zero existing `.clone()` call sites first) — a clone would
silently fork `failure_counters`/`re_admission_strikes` into two
independent security records.

**Run 13 closeout:** `cargo build -p iris-core --all-targets` clean;
`cargo test -p iris-core --lib gateway::` — **31/31 pass** (28
pre-existing + 3 new); `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop` —
**687 iris-core lib tests, 0 failed** (including every existing PeerId
Debug-format usage crate-wide); `cargo clippy -p iris-core --lib
--no-deps` — 0 new warnings (14 baseline, unchanged).

**§8 accounting:** 5 findings fixed this run (MG-34, 35, 36, 37, 38) in
one commit — under the cap. This closes 14 of 18 MG findings; MG-39..42
remain.

## Run 14 — 2026-08-27 — MG-39/40/41/42 (TransportError taxonomy) — MG area of Tier 2 fully closed out

All four findings share one enum (`TransportError` in `error.rs`) and
cross-reference each other directly, so fixed as one unit. Read every
construction/comparison site across all 6 transport files before
touching anything (~60 raw grep hits including tests) — the widest
blast radius of any Tier 2 fix this session, wider than DTN-15/16/18's
`meet()` rewrite.

**MG-39** (commit `a442d43`): `From<std::io::Error>` stringified
immediately, discarding `ErrorKind`. `Io(String)` → `Io{kind, msg}`,
plus a new `is_retryable()` matching the finding's own suggested body
(`Busy | ConnectionFailed | Io{WouldBlock|TimedOut|Interrupted}`).

**MG-40** (same commit): `Protocol(String)` carried three incompatible
classes. Split into `Protocol(String)` (narrowed to genuine framing),
`MessageTooLarge{limit,actual}`, `PolicyDenied(&'static str)`. Every
`Protocol(...)` construction site across all 6 files re-examined against
its actual surrounding context (not pattern-matched by string content)
to classify correctly — e.g. ble.rs's "scan throttled" message read like
a policy denial by prose but is actually transient (clears on its own
after a cooldown), so it moved to the pre-existing `Busy` instead of
either new variant; a defect the finding itself didn't name but its own
principle covers.

**MG-41** (same commit): `NotSupported` conflated two remediations.
Split into `HardwareUnavailable`/`PermissionDenied{permission}`.
Reclassifying every BLE/Wi-Fi Direct/Wi-Fi Aware call site surfaced a
genuine third case neither variant fits: `BleError::AdapterOff` and both
Wi-Fi transports' `!adapter.is_available()` checks mean "radio present
and permitted, but currently switched off" — not hardware-absent, not
permission-denied. The finding's own fix text already names this exact
three-way split for the sibling `TransportState` enum
(`NoHardware`/`PermissionDenied`/`RadioOff`) but only sketched two
variants for `TransportError` itself. Added `RadioDisabled` rather than
force a wrong classification into one of the other two or leave the gap.

**MG-42** (same commit): resolved via MG-39's `kind` field alone
(`ErrorKind` is `Copy + PartialEq + Eq`, so `derive(PartialEq, Eq)`
survives with zero manual impl). Did not add a `source()` chain
(`Arc<dyn Error + Send + Sync>` + manual `PartialEq` over
discriminant+kind only) — the finding's own fix text marks that as an
explicit, separate "if wanted" option, not the core ask, and giving up
derived `Eq`/`Hash` is disproportionate to what MG-39/40 already fix.

**Second-order conflation found mid-fix**: `lora.rs` and `satellite.rs`
each have their own link-error enum (`LoRaLinkError`/`SatelliteLinkError`,
both `HardwareGated`/`Closed`/`Io(String)`) that every tx/rx/status-probe
call site uniformly wrapped into `TransportError::Io` regardless of
which variant — the exact same conflation MG-39 targets, one layer
down. Left in place, this would have silently defeated the fix for
these two transports specifically (a `HardwareGated` failure would still
present as a generic, kind-`Other` `Io` error). Added
`lora_link_err`/`satellite_link_err` helpers mapping each variant to its
correct `TransportError` (`HardwareGated`→`HardwareUnavailable`,
`Closed`→`NotConnected`, `Io`→`Io{kind:Other,..}`) at all 6 call sites
(3 per file). Two existing tests ("closed link fails the tx" in both
files) had to be re-verified against the actual simulated adapter's
`tx()` implementation (confirmed it returns `LoRaLinkError::Closed`/
`SatelliteLinkError::Closed` on a closed link, not `Io`) before updating
their assertions from `Io(_)` to `NotConnected`.

**The one behavioral consumer**: `message_engine::deliver_outbound`/
`requeue_or_fail` — without wiring `is_retryable()` in here, MG-39/40's
fix would only be a type-system change with zero runtime effect.
`requeue_or_fail` gained a `retryable: bool` param; `deliver_outbound`
now tracks whether *every* observed send failure across all ranked
transports was non-retryable (`saw_failure`/`any_retryable_failure`,
correctly distinguishing "never attempted a send" from "attempted and
all failed non-retryably" — the naive `let mut x = true` then
OR-accumulating retryability is a bug, since `true || anything` never
changes; had to track a separate "did we see a failure at all" flag).
Only short-circuits a **bounded** budget (P1-P7) — fails fast instead of
exhausting the full attempt schedule against a permission gate that
cannot self-resolve. Deliberately does **not** apply to P0
(`max_attempts: None`): an initial draft did, which on reflection would
have made an emergency SOS message give up outright on its very first
permission-denied attempt — a materially worse outcome for a
disaster-response product than the wasted-retry-cycles bug MG-39
describes. Caught this by re-reading the finding's own trigger scenario
(Android revoking `BLUETOOTH_CONNECT` mid-SOS) against the fix before
finalizing it, not after.

**Run 14 closeout:** `cargo build -p iris-core --all-targets` clean;
`cargo build --workspace --exclude iris-desktop --all-targets` clean;
`cargo test --workspace --exclude iris-desktop` — **691 iris-core lib
tests, 0 failed** (up from 687 by exactly the 4 new `error.rs` tests) —
every reclassified call site's own existing test caught any
misclassification by construction, since a wrong variant would have
failed that specific assertion; `cargo clippy -p iris-core --lib
--no-deps` — 0 new warnings (14 baseline, unchanged).

**§8 accounting:** 4 findings fixed this run (MG-39, 40, 41, 42) in one
commit — under the cap. This closes the MG (gateway/transport) area of
Tier 2 entirely: all 18 MG-\* findings are now fixed.

## Run 15 — 2026-08-27 — TAK-2 (storage exhaustion DoS) — Tier 2 fully closed out

Owner said "continue" (resuming after a usage-limit reset) — same
standing sign-off. Per Run 14's plan: TAK-2, the last remaining Tier 2
finding, in `crates/iris-storage` — a crate this session hadn't touched
before, so read `pg.rs`/`eviction.rs` in full against the finding's
evidence before writing anything, same discipline as every prior wake.

**Source had drifted more than any other finding this session.** Two
separate, already-landed fixes changed the shape of the exact code the
finding's evidence snippet quotes: TAK-5 replaced the read-check-insert
sequence (`current.saturating_add(grow) > max_storage_bytes && !is_p0`)
with a single atomic SQL statement — a cross-store TOCTOU fix, unrelated
to TAK-2 on its face. TAK-12 added batched, cost-bounded eviction with
error-level logging when a pass exhausts its cap while still over
target — which, read closely, already implements this finding's own
third fix layer ("make eviction report when it can't reach target
instead of silently breaking"); its own doc comment even names "the
TAK-2 class" of failure by name. Neither fix touched the actual
*admission bypass* — it survived, just re-expressed as `$10::bool OR
(...)` in the new atomic SQL, where `$10` is bound to `is_p0`. Same bug,
new shape. Would have been very easy to miss by pattern-matching the
finding's own line numbers instead of reading current source end to end.

**Layer 1** (commit `08d3534`): clamped `ttl_seconds` to `MAX_TTL_SECS`
before computing `expires_at`. Reused (made `pub`) the exact constant
this session's own earlier DTN-6 fix added to the in-memory SCF store
(`iris_core::routing::scf::MAX_TTL_SECS`) rather than defining an
independent one — the finding's own dependency note requires both
storage layers to agree, and a second constant is free to drift from
the first the moment either changes. Did NOT also add the finding's
suggested "reject an already-expired `expires_at` at ingest": doing so
would have broken `evict_expired_reclaims_only_dead_rows`, this crate's
own existing test, which deliberately persists an already-expired
message specifically to prove GC reclaims it. Recognized this as a real
conflict, not a test to route around — an already-expired row is the
*opposite* of TAK-2's actual attack (immortal, never-reclaimable); it's
reclaimed by the very next GC pass regardless, so skipping this half of
the finding's suggestion costs nothing.

**Layer 2:** narrowed the admission bypass from `is_p0` to `is_own &&
is_p0` — INV-ROUTE-003 protects this node's own emergency traffic, not
an unauthenticated peer's unverified priority claim.

**Layer 3:** widened `evict_lowest_priority`'s WHERE clause from
`priority > 0` to `NOT (priority = 0 AND is_own_message)`, using the
`is_own_message` column that already existed in the schema — a relayed
P0 row is now the last thing evicted (after every other priority, via
the pre-existing `ORDER BY priority DESC`), never this node's own P0.

**Test fallout:** `evict_by_priority_never_touches_p0` — named
explicitly in the finding's own dependency note as needing this — was
renamed and rewritten with an explicit `node_id` so it can actually
distinguish own from relayed (the old version, with no `node_id`
configured, couldn't tell them apart at all). `quota_rejects_non_p0_but_accepts_p0`
— not named by the finding, found by grepping for every P0/quota test
in the file — had an assertion ("P0 always accepted", citing
`STORAGE.md`'s pre-fix invariant by name) that directly encoded the
vulnerability being fixed; renamed and rewritten to prove the corrected
three-way split (non-P0 over quota refused; relayed P0 over quota also
refused — the actual regression test; own P0 still always accepted).
Correcting a test's assertion to match intentionally-changed,
security-relevant behavior is a genuine behavioral update, not the
"delete a test to make it pass" the loop's own rules forbid. Added a
third test, `evict_by_priority_reclaims_relayed_p0_as_a_last_resort`,
proving a store with nothing left but relayed P0 can still reclaim
space at all — the finding's own core described attack. Added `env_from`
(explicit sender) alongside `env_for` to the shared test helpers, needed
to construct both own and relayed messages against one store config;
deleted `store_with_max`, left orphaned by the rewrite.

**Scope decisions, documented not silently dropped:**
- `docs/implementation/STORAGE.md` still states the pre-fix invariant
  verbatim ("P0 messages are always accepted... never evicted unless
  expired") — now stale. Not edited: the finding's own dependency note
  says changing INV-ROUTE-003 "needs sign-off from Section 2 (message
  engine)", a human decision this loop cannot make alone, and the file
  sits outside the loop's file scope regardless.
- `routing/scf_eviction.rs` (the in-memory SCF store) — checked per the
  finding's own "requires the same reasoning" note. Initially assumed
  (incorrectly, before actually reading the file) that DTN-1's earlier
  admission-enforcement rewrite made it already consistent "in the same
  way" as this fix. On reading it: `ScfEvictionPolicy::is_evictable`
  exempts P0 *unconditionally*, own or relayed alike — no ownership
  concept exists in that policy at all. It does NOT share TAK-2's actual
  bug: `buffer_message` enforces its byte budget on every insert with no
  P0 admission bypass, so growth stays bounded regardless of priority
  mix (a new P0 message is simply refused once nothing is evictable,
  never admitted unconditionally the way PG's old code did). But it has
  a different, narrower gap: a relayed-P0 flood could in principle
  starve out this node's own SOS traffic specifically, since nothing
  distinguishes them for eviction purposes. Not part of TAK-2 and not
  required to close it — flagged as a separate task (`task_612cf9ed`)
  rather than folded in speculatively or silently ignored.

**Run 15 closeout:** `cargo build -p iris-storage --all-targets` clean;
`cargo build -p iris-core --all-targets` clean (the `MAX_TTL_SECS`
visibility change); `cargo test --workspace --exclude iris-desktop` —
**691 iris-core lib tests, 0 failed**; iris-storage's ignored-test count
rose from 13 to 14, matching exactly the one net-new test (the other two
were renames, not additions); `cargo test -p iris-core --lib
routing::scf::` — 16/16 pass; `cargo clippy -p iris-storage --all-targets
--no-deps` / `cargo clippy -p iris-core --lib --no-deps` — 0 new
warnings either crate. All of `iris-storage`'s substantive coverage
requires a live PostgreSQL instance and is `#[ignore]`d (TAK-1) with
none available in this environment — verified by full-crate compilation
(catches every type/logic error in the rewritten tests) and by
hand-tracing each scenario's expected SQL predicate outcome against the
actual admission/eviction queries, documented as a real limitation
rather than glossed over as equivalent to an end-to-end run.

**§8 accounting:** 1 finding fixed this run (TAK-2) in one commit — well
under the cap. **This closes Tier 2 entirely**: 80 findings, 77 fixed,
3 blocked (ROUT-23, ROUT-24, ROUT-26 — all on conflicting/inconsistent
governing specs, a human decision, not a code gap that could be closed
by more autonomous work).

## Run 16 — 2026-08-27 — Tier 3 opens: full 32-finding scope + Wake 12 (SIM-1/2/3/4)

Owner said "okay , start with tier 3" — explicit, direct instruction to
open the next tier. Per `taksh_problems_loop.md`'s own stated run order
("0 → 1 → 4 → 3", reasoning: fixing simulator fidelity reshapes what
"passing tests" means for everything reviewed against it, so doing that
last avoids re-basing expectations twice) Tier 3 was not next in the
loop's preferred sequence — Tier 4 was. Surfaced this to the owner as a
one-line heads-up, not a blocker, since the instruction was explicit and
direct; proceeded with Tier 3 as asked.

**Full scope read before any code changes** (same discipline as every
tier-opening wake this session: DTN's 8-finding core cluster, MG's
25..42 membership, both scoped in full before batching). Read all 32
SIM-\* findings (SIM-1 through SIM-32) in complete detail — What,
Evidence, Why wrong, Root cause, Trigger, Impact, Fix, Dependencies —
before writing a single line. Tier 3 turned out structurally different
from Tier 2: Tier 2's findings were mostly localized (if occasionally
cross-file) correctness/security bugs. Tier 3 has two Critical findings,
SIM-7 (the sim never calls production `RoutingEngine::decide` — it
hand-rolls its own forwarding cascade and only borrows leaf primitives)
and SIM-8 (the sim models no transport layer at all — no latency,
bandwidth, MTU, loss asymmetry, reordering, duplication), whose own
literal fix text asks for cross-cutting rewrites of shared production
routing code (`routing/mod.rs`'s decision authority; a new
transport-fidelity model), not contained bug fixes — closer in kind to
MG-30's gateway-wiring decision than to a typical finding. SIM-9
explicitly depends on SIM-8 ("enforce the SIM-8 byte budget across the
window").

**Batching plan, by coupling** (not by finding number — matches how
DTN/MG were grouped):
- **Wake 12 (this run):** SIM-1, SIM-2, SIM-3, SIM-4 — determinism/clock
  cluster. All four bear on whether "same seed -> same result" holds,
  which every later wake's own tests lean on implicitly.
- **Wake 13:** SIM-5, SIM-6, SIM-11, SIM-17, SIM-18, SIM-31 — dead/
  fabricated sim metrics (eviction never wired, peak tracking wrong,
  forwarding outcomes discarded, overhead never asserted). Grouped
  because SIM-17/18/31 explicitly cross-reference each other (wire
  `overhead()` into an assertion or delete it; fix its zero-delivery
  sentinel first or the new assertion is meaningless), and SIM-5/6/11
  are the same "claimed-but-wrong/unwired metric" pattern one level up.
- **Wake 14:** SIM-19, SIM-20, SIM-29 — statistics/test-strength bugs
  (percentile rounding bias, Wilson CI silent NaN, two property tests
  that pass with their own mechanism disabled).
- **Wake 15:** SIM-10, SIM-12, SIM-30, SIM-32 — sim-internal mechanics,
  each contained to `sim/mod.rs` + one sibling file (real-time dedup
  cache, event-ordering tiebreak, unbounded per-node state, a
  self-addressable injection helper).
- **Wake 16:** SIM-13, SIM-14, SIM-15, SIM-16, SIM-27, SIM-28 — the ML
  leakage/test-integrity cluster, all in `sim/ml/` +
  `tests/ml_experiments.rs`. SIM-14 explicitly "compounds SIM-13; fixing
  SIM-13 alone does not remove this."
- **Wake 17:** SIM-21 through SIM-26 — production **observability**
  module (`observability/mod.rs`) doc/code/privacy mismatches. Not
  simulator code at all despite the SIM-\* numbering; grouped separately
  because it's a different subsystem.
- **Wake 18 (last):** SIM-7, SIM-8, SIM-9 — saved for last deliberately,
  mirroring the loop's own 4-before-3 reasoning one level down: fixing
  these changes what every other Tier 3 scenario's baseline means, and
  SIM-7 in particular touches shared production routing code with real
  blast radius. Will need an explicit, documented scope decision when
  reached, not a wholesale literal implementation — same pattern as
  MG-30.

**Wake 12 fix: SIM-1, SIM-2, SIM-3, SIM-4 (`fdfe6fb`).**

SIM-1 (Critical): `build_envelope` minted message ids via
`MessageId::new_v7()` — wall clock + random bits, uncontrollable by
seed. `StoreKey::Ord` (routing/scf.rs) ties on `message_id` whenever
`priority_rank`/`expiry_unix` coincide, which is the common case within
one sim run (shared priority, second-quantized expiry per SIM-4) — so
the seed did not actually control forward order despite the module doc's
"same seed -> same result anchor" claim. Fixed by minting ids from the
sim's own seeded `ChaCha8Rng` (`rand::Rng::gen::<[u8;16]>()` via
`MessageId::from_bytes`); `build_envelope` now takes `&mut self` (single
call site, inside `run()`, no conflicting borrow). Added
`message_ids_are_seeded_not_wall_clock`, a direct, narrow regression test
independent of the broader anchor-based coverage below.

SIM-2 (High): `result_anchor` was `format!("sim-{seed}-d{delivered}-i{injected}")`
— `injected_total` is fixed by the scenario and `delivered_total`
saturates at 100% in most scenarios, so two runs taking completely
different routes could share an anchor, making "determinism" tests
near-tautological (exactly SIM-2's own characterization). Fixed by
folding a digest of the full per-delivery trace `(message_id,
delivering_node, at_ms, hops)` — collected from `SimNode::delivered` +
`hops_by_msg` (no new field needed; hops_by_msg is frozen for a node
once that node's copy is delivered, since any later forward attempt to
the same node for the same id hits the earlier "already delivered"
Sinked guard before reaching the hops_by_msg write), sorted for
collection-order independence — plus `relays_total` and the latency
vector, hashed with `DefaultHasher` (fixed-key SipHash: stable across
runs of the same binary, unlike `HashMap`'s randomly-seeded
`RandomState` — not used for any security property, just a cheap
deterministic fingerprint). New format:
`sim-{seed}-d{delivered}-i{injected}-t{hash:016x}`. This is also what
makes SIM-1 independently verifiable: an unfixed SIM-1 combined with
this trace-hash anchor would make the existing `same_seed_same_result_anchor`
test fail (different wall-clock ids each run -> different trace hash) —
so that pre-existing test now doubles as SIM-1 regression coverage for
free, and did in fact pass post-fix.

SIM-3 (Medium): `ac5_ten_seeds_reproducible_both_regimes` asserted 20
distinct anchor strings across 10 seeds x 2 regimes, but the anchor
format embeds the seed verbatim (`sim-{seed}-...`), so 20 distinct
(seed, regime) pairs are guaranteed 20 distinct strings regardless of
simulator behavior — SIM-3's own diagnosis, confirmed by reading the
test. Fixed by stripping the known `sim-{seed}-` prefix before
comparing, so the assertion is actually over SIM-2's behavioral
signature. Calibrated the threshold empirically rather than guessing:
ran the test with the strictest possible bound (`== 20`) first — it
passed, meaning the simulator is genuinely fully seed-sensitive across
both regimes with the current fixes in place — then relaxed to `>= 18`
so one vanishingly-unlikely coincidental digest collision in the future
doesn't flake the suite, while staying far above what a seed-blind
regression would produce (2-4 distinct values, not 18+).

SIM-4 (High): `advance_to` truncates the virtual clock to whole seconds,
so sub-second-spaced events (vehicle_relay's +300ms backbone links,
periodic_ferry's step_ms/2 offsets) produce zero clock movement,
degenerating `shadow_features`'s age_s/ttl_frac (FeatureVec slots 5/6)
to ~0 for any decision inside the same virtual second. **Scoped narrower
than the finding's literal suggestion** ("give ScfEngine/OpportunisticRouter
a millisecond clock"): grepped every `with_virtual_clock` call site
before deciding, and found the shared `Arc<AtomicU64>` clock feeds FOUR
separate production modules — `ScfEngine` (scf.rs), `ForwardedCache`
(dedup_cache.rs — the exact type SIM-10, Wake 15, is about), PRoPHET's
`DeliveryPredictability` (prophet.rs), and `OpportunisticRouter`
(opportunistic.rs) — all of which treat the atomic as Unix seconds in
BOTH their virtual-clock and real (`SystemTime::now`) paths
(`dedup_cache.rs` does `Duration::from_secs(clock.load(...))`;
`prophet.rs` does `.saturating_mul(1000)` to derive millis from it).
Changing that unit is a cross-cutting change to shared production
routing infrastructure, not a sim-local bug fix — out of scope for this
wake, documented inline at `advance_to` as a deliberate decision rather
than silently skipped. Instead added `SimNode::buffered_at_ms` (mirrors
the existing `hops_by_msg` per-message map, written at the same two
sites: injection and relay-landing) so `shadow_features` computes age
from the sim's own millisecond-precise `now_ms`, independent of the
shared clock's granularity — fixes the concrete, testable symptom
(feature degeneracy) without touching the shared contract.
`StoreKey.expiry_unix`/TTL ordering intentionally stays whole-second,
matching production's actual `ttl_seconds: u64` wire granularity — there
is no real sub-second TTL requirement to satisfy, so this residual gap
isn't a gap relative to what production needs. Added
`shadow_feature_age_has_subsecond_precision`, constructed so the two
event timestamps (100ms, 900ms) provably read age_s=0.0 under the old
code (both floor-divide to the same whole second) while the fix reads
the true 0.8s gap — a test that discriminates old-vs-new behavior by
construction, not just a value-in-range check.

**Note left for Wake 15 (SIM-30):** `buffered_at_ms` is a third
per-message `HashMap` on `SimNode` alongside `hops_by_msg`/`spray`, all
three currently unpruned. SIM-30's fix (prune on terminal delivery) needs
to cover this new map too, not just the two the finding names.

**Run 16 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop` —
**693 iris-core lib tests, 0 failed** (+2 from Wake 12: the two new
targeted regression tests; net across the crate, since no tests were
removed); `cargo test -p iris-core --test sim_scenarios --test
ml_experiments` — 16/16 pass (1 pre-existing `#[ignore]`d debug helper,
unaffected), including all 5 anchor-equality tests unmodified in
assertion intent (a stronger anchor is still equal across two runs of
the same seed) and `ac5_ten_seeds_reproducible_both_regimes` with its
rewritten uniqueness check; `cargo clippy -p iris-core --lib --no-deps`
clean (only pre-existing, unrelated `transport::state_tx` unused-Result
warnings, present before this session's work began).

**§8 accounting:** 4 findings fixed this run (SIM-1, SIM-2, SIM-3,
SIM-4) in one commit — under the cap. Tier 3: 4 of 32 done, 28 remaining
across Wakes 13-18 per the plan above.

## Run 17 — 2026-08-27 — Wake 13: dead/fabricated sim metrics cluster (SIM-5/6/11/17/18/31)

Owner said "continue" (standing instruction, same session). Per Run 16's
plan: Wake 13, the dead/fabricated sim metrics cluster.

**SIM-5 — source drift discovered mid-fix.** The finding's evidence says
the sim "never calls `ScfEngine::evict_to_fit` or `reap_expired`, so no
storage pressure or TTL reaping is ever applied." Read `buffer_message`
in full before implementing (same discipline as every fix this session)
and found this is only half true now: `buffer_message` already calls
`self.evict_until(target)` internally whenever an insert would exceed
`max_bytes` — DTN-1, a *different*, already-landed finding from this
session's earlier Tier 2 pass, put that enforcement there. So storage
pressure genuinely IS handled; it just doesn't show up in
`evictions_total`, because `buffer_message`'s
`Result<DeliveryStatus, ScfError>` doesn't surface what it evicted as a
side effect. Considered changing `buffer_message`'s signature to return
evicted ids — checked every caller first (`grep -rn "\.buffer_message("`
across the whole tree) and found ~30 call sites across
`routing/scf.rs`, `scf_contact.rs`, `scf_eviction.rs`, and
`obs_telemetry.rs`, all but the sim's own two being test code with
`.unwrap()` on the current return type. Changing a signature that
widely used, purely to fix a sim-local bookkeeping gap, is disproportionate
— instead diffed `ScfEngine::len()` before/after each of the sim's two
`buffer_message` call sites (injection, relay-landing), accounting for
the admitted entry so a capacity eviction isn't miscounted as a net
occupancy change. The TTL-reaping half of the finding WAS fully missing
— `reap_expired()` had zero callers anywhere in `sim/`, confirmed fresh
— so a message that outlived its TTL while sitting un-relayed in a
buffer stayed there forever, still occupying space, still forwardable.
Added a `reap_expired()` call for every node at every timeline tick.
Regression test `ttl_expiry_is_reaped_and_counted` (1-second TTL, no
other tick until 5 virtual seconds later) proves the reaping half
directly; the capacity-eviction diff logic is exercised by every
existing test's buffer traffic (correctly computing 0 when nothing is
evicted) but has no dedicated test — reaching the 500MB default
`max_storage_bytes` needs either a new sim config knob (not something
this finding asks for) or an impractically large test.

**SIM-6.** `peak_storage_bytes` was computed once, from final node
state, after `run()` returned — the finding's own diagnosis, confirmed
by reading `metrics.rs`. Added a real running peak to `Simulation`,
sampled every timeline tick — not just contact ticks as the finding's
literal fix text suggests, because an injection can itself create a
momentary spike before the next contact relays the message away, which
a contact-only sample would miss (re-derived from reasoning about the
actual event model, not the finding's literal text). Regression test
`peak_storage_bytes_captures_transient_high_not_final_state` is built
to prove exactly this distinction: final usage across every node is
provably 0 (a delivered message is removed from both buffers, DTN-5),
while the peak sampled mid-run is provably nonzero — a case the OLD
computation could not have passed.

**SIM-11.** `ForwardResult` already distinguished four blocked reasons
internally; `run()`'s two forwarding loops threw the result away on
every call. Split `Blocked` into `BlockedBacktrack`/`BlockedHopBudget`/
`BlockedNoAdvantage`/`BlockedLoss` (the finding's own suggested split)
plus a fifth, `BlockedMalformed`, for a defensive branch (malformed
32-byte recipient check) that's unreachable in every current sim code
path and deliberately not one of the four tallied counters — mapping it
into one of the four real categories would have made the tally lie
about why a forward was actually blocked, which is the entire point of
this fix. Added a `tally` helper and four `u64` counters, exposed on
`SimOutcome`. Deleted `route2_debug_undelivered` (the `#[ignore]`d
eprintln debug test this fix makes obsolete) and its only caller
`run_ferry_inspect`, per the finding's own explicit instruction —
required removing the now-unused `MessageId` import from
`sim_scenarios.rs` too. Regression test
`blocked_reasons_are_tallied_and_exposed` constructs a 5-node P4 chain
one hop longer than the 3-hop budget (`routing/flood.rs`) and checks
`blocked_hop_budget` directly, traced by hand through `forward_to`'s
hop-count logic before writing it to confirm the 4th hop is exactly
where the budget bites.

**SIM-18 before SIM-17** (order matters, per SIM-18's own note that an
overhead assertion landing before its sentinel is fixed would let a
catastrophic-failure run "silently satisfy" the new check). Changed the
zero-delivery `overhead_ratio` sentinel from `0.0` (the *best* possible
value) to `f64::INFINITY` — composes directly with a `<` comparison via
ordinary float ordering, chosen over `Option<f64>` (the finding's other
suggested option) specifically because SIM-17's new assertion can then
compare both sides without unwrapping.

**SIM-17.** Added the overhead assertion
`route2_opportunistic_matches_delivery_with_lower_overhead`'s own name
and doc comment already promised. Ran it before deciding anything else
— per the finding's own "if it fails, that failure is the finding"
instruction, this was not going to be loosened to pass. It passed on
the first run: L2 genuinely does lower overhead vs L0 in this scenario.
The GTMX+ mechanism works; it simply had zero executable proof of that
until this run.

**SIM-31.** Re-verified every named item's "dead" status fresh via grep
before touching anything, rather than trusting the finding's evidence —
source drift has been a recurring theme this session (TAK-2, SIM-5
above) and a method could easily have gained a caller since the finding
was written. Confirmed `shadow_enabled`, `loss_rate`,
`injections_public`, `avg_latency_ms` still have zero readers anywhere
in `crates/`; deleted all four. `SimMetrics::overhead()` — the fifth
item — is no longer dead: SIM-17 gave it its first real caller in this
same run, so it was kept. `peak_storage_bytes` — the sixth item, listed
as "wrong anyway, SIM-6" — was kept as a struct field rather than
deleted: `SimMetrics` is documented as a "full metrics snapshot for one
simulation run," and its sibling fields (`latency_p50_ms`,
`evictions_total`) aren't universally read by every test either; a
snapshot struct legitimately carries fields not every caller consumes.
SIM-6 making the value correct is what this finding's own cross-
reference to SIM-6 actually asks for, not deletion.

**Flagged, not chased: `ScfEngine` may have zero production callers.**
While reading `buffer_message`'s callers for SIM-5 (see above), grepped
`message_engine/mod.rs` for `ScfEngine`/`.scf.`/`buffer_message` and got
zero matches. A wider grep across `crates/iris-core/src/`,
`crates/iris-android/src/`, `crates/iris-ios/src/` found `ScfEngine`
referenced outside `routing/scf*.rs` (its definition) and `sim/` (the
simulator) in exactly two places: a `pub use` re-export in
`routing/mod.rs`, and one `#[cfg(test)]` function in the same file that
manually constructs a `ScfEngine` to prove the store-carry-forward
concept works — not something reachable from a real message-send call.
If this holds up, it means the entire store-carry-forward layer this
session spent 25 fixes hardening (DTN-1 through DTN-25, Tier 2) may not
be wired into the actual production message path at all, and
offline/partition delivery might not work on a real device despite the
simulator reporting near-100% delivery in partition-carry scenarios.
This is well outside a Tier 3 sim-metrics wake's scope and far too
large a claim to act on from a grep alone — flagged via `spawn_task`
(`task_fb33727f`) for dedicated investigation rather than either chased
mid-wake or silently dropped.

**Run 17 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop` —
**696 iris-core lib tests, 0 failed** (+3 from this wake's three new
regression tests); `cargo test -p iris-core --test sim_scenarios --test
ml_experiments` — 16/16 pass (`sim_scenarios.rs` drops from 9 tests
including 1 ignored to 8, all passing, reflecting
`route2_debug_undelivered`'s deletion); `cargo clippy -p iris-core --lib
--no-deps` — 14 warnings, all pre-existing and unrelated (diffed against
Wake 12's clippy output to confirm no new ones, same set:
`transport::state_tx` unused-Result sites, `routing::decide`'s
too-many-arguments, etc.).

**§8 accounting:** 6 findings fixed this run (SIM-5, SIM-6, SIM-11,
SIM-17, SIM-18, SIM-31) in one commit — at the cap. Tier 3: 10 of 32
done, 22 remaining across Wakes 14-18 per the Run 16 plan.

## Run 18 — 2026-08-27 — Wake 14: statistics/test-strength cluster (SIM-19/20/29)

Owner said "continue" (standing instruction, same session). Per Run 17's
plan: Wake 14, the statistics/test-strength cluster — three cases where
the *check itself* was wrong, not just what it checked.

**SIM-19.** Replaced `percentiles`' nearest-rank index (`round((n-1)*p/100)`)
with R-7 linear interpolation, exactly matching the finding's own fix
code. Before touching the existing `percentile_edge_cases` test (whose
`[1,2,3,4] -> (3,4)` assertion the finding suggests changing to
`(2,4)`), verified by hand which value the new formula actually
produces: rank=1.5 for p50 interpolates to 2.5, and `2.5_f64.round()`
in Rust is round-half-away-from-zero (verified with a two-line rustc
smoke test, not assumed) — giving 3, not 2. Ran the *unmodified*
existing test against the new implementation and it passed unchanged:
old nearest-rank and new interpolation coincide at this specific n=4
input by construction (`round(1.5)=2` picks the same `sorted[2]=3` that
`round(interpolate(1.5))=round(2.5)=3` lands on), so this edge case
cannot discriminate old from new and the finding's suggested `(2,4)`
does not match either implementation's actual output. Added a second,
discriminating test from the finding's own trigger scenario (19
samples at 1000ms, one outlier at 120000ms) — hand-computed the R-7
p95 as 6950ms and confirmed it by running the test, which is very
different from the finding's own stated "~113000ms" for the same
input; used the verified value, not the finding's stated one, and
noted the discrepancy rather than silently "fixing" the test to match
an unverified number.

**SIM-20.** Clamped `k = k.min(n)` in `wilson_ci_95` plus a
`debug_assert!`; changed the `n == 0` return from `(0.0, 0.0)` to
`(0.0, 1.0)` per the finding's own reasoning (a tight interval at zero
reads as "confidently measured 0% delivery" to a `lo >= threshold`
caller, when the truth is "no data"). Updated the one existing test
that asserted the old degenerate value; added a direct
`k > n` regression test.

**SIM-29(a) — the strict-inequality fix surfaced a real calibration
gap, not a code bug.** Changing `loss_reduces_delivery_ratio_monotonically`
from `<=` to `<` immediately failed: at the original 60% loss rate,
`dense_mesh(6, 5)` showed *zero* measured effect (delivery ratio 1.000
both with and without loss). Traced this to `dense_mesh`'s own
structure — ~30 contact events (every pair of 6 nodes, twice) for just
4 injected messages — combined with SCF's retry-until-delivered
semantics: a message only needs to survive its loss draw once across
dozens of relay attempts, so 60% per-attempt loss gets absorbed almost
entirely by redundancy. Rather than guess a replacement seed or rate,
wrote a temporary example binary (`crates/iris-core/examples/loss_probe.rs`,
deleted after use — never committed) that swept 10 seeds x 5 loss
rates and printed the actual delivery-ratio drop for each combination.
Seed 5 at 60% was a genuine outlier (every other sampled seed showed
*some* drop at 60%, just inconsistently above a meaningful threshold);
80% produced a robust 0.2-0.7 drop across every seed sampled. Raised
the rate to 80% (keeping the original seed 5 — the minimal change),
which is what the committed test now uses.

**SIM-29(b).** `route2_spray_bounds_overhead_property` computed
`relays_total / injected_total`, a mean, to check a *per-message* bound
(AC-4: "copies never exceed L per message"). Added `relays_by_msg:
HashMap<MessageId, u64>` to `Simulation`/`SimOutcome`, populated at the
one site `relays` is incremented (`forward_to`'s `Relayed` branch, next
to the existing per-node `relays` counter), and switched the assertion
to `.values().max()`. Unlike (a), this one passed immediately with no
calibration issue — binary spray's per-message bound genuinely holds;
the mean-based check had just never been strong enough to prove it.

**Run 18 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop` —
**698 iris-core lib tests, 0 failed** (+2 from this wake); `cargo test
-p iris-core --test sim_scenarios --test ml_experiments` — 16/16 pass;
`cargo clippy -p iris-core --lib --no-deps` — same 14 pre-existing
warnings as Wake 13 (confirmed by exact count, none in `sim/`).

**§8 accounting:** 3 findings fixed this run (SIM-19, SIM-20, SIM-29)
in one commit — well under the cap. Tier 3: 13 of 32 done, 19
remaining across Wakes 15-18 per the Run 16 plan.

## Runs 19+ — gap in this file (not in the work)

This file was not updated between this session's own Run 18 and the
entry below. In between, work continued through the branch's shared
`origin/main` from a **different worktree/session** than the one
writing this file — it closed out the rest of Tier 3 (Wakes 15-19:
SIM-10/12/30/32, SIM-13/14/15/16, SIM-21..28, SIM-9, SIM-7/SIM-8) and
made 51/126 progress into Tier 4 (MG-1/2/4/9/11/13/21, GAP-2/3/6,
RF-29/30/31/33/41, BLE-14/15/18/19/20/21/23/31/33/34/35, TAK-7/8, plus
an extensive live hardware-verification pass — HW-9 through HW-21,
BLE mesh and Wi-Fi Direct confirmed working end-to-end on physical
devices). None of that is missing — it's fully recorded in
`taksh_problems.md`'s Progress Tracker (Wake-by-wake narrative,
Fix status line per finding, and the Tier roll-up table) and in
`git log`, which is why that document — not this one — is the
authoritative "what's done" source per its own file header. This file
only lost its role as the detailed per-run narrative for that stretch;
picking it back up now, for the first run this worktree actually did
after pulling that work in.

## Run 19 — 2026-08-29 — Tier 4 Wake RF-A: LoRa refund/eligibility (RF-4/10/14/16)

Owner said "continue with tier 4" after a prior message confirmed the
`origin/main` pull (60+ commits) and the corrected Tier 4 status
(51/126, not the owner's initially-recalled "51/121 in tier 3" — Tier 3
was in fact fully closed at 32/32).

**Scoped the full RF-1..23 LoRa/satellite duty-cycle cluster (23
findings) before writing any code** — same discipline as every
tier/cluster opening this session. This is qualitatively different
from anything in Tier 3: real WPC G.S.R. 853(E) 2021 regulatory
exposure (LoRa duty-cycle/e.r.p. compliance) and real financial
exposure (Iridium satellite billing, ~₹10/message), several findings
needing platform-specific code this environment can't build or verify
(RF-6's boot-time clock shim needs `CLOCK_BOOTTIME`/`mach_continuous_
time`), and much larger individual findings than typical (RF-1 extends
a public trait across 3 adapter impls; RF-8 replaces a single frequency
constant with a whole channel-plan model). Deliberately split more
conservatively than Tier 3's wakes: this run covers the four
best-contained findings; RF-1 (trait extension), RF-5 (new
`TransportError` variant), RF-7 (new `ComplianceConfig` fields + a
distinct error type) are each independently substantial and deferred
to a follow-up run rather than bundled in to hit a larger batch.

**Before touching any Tier 4 code, had to restore a green tree.**
`cargo build --workspace --all-targets` failed outright: `SimLoss`
(upstream SIM-8 work) gained `bandwidth_bps`/`mtu_bytes`/`latency_
base_ms` fields, but two construction sites weren't updated —
`sim/scenario.rs`'s `loss()` helper (blocks the whole lib) and this
session's own `same_seed_same_result_anchor` test (blocks
`--all-targets`). Fixed both with `..Default::default()`. Once
building, `cargo test --workspace` surfaced three more: two BLE tests
asserting stale pre-BLE-10 `bytes_sent` values (payload-only, not the
wire-accounting-with-header-overhead the upstream fix correctly
switched to); and a real bug in `max_hops_seen`, found by tracing
(added a temporary debug print, ran the failing test, read the actual
`hops_by_msg` state, removed the print before committing) rather than
guessed — SIM-30's terminal-delivery pruning (upstream, this tier)
removes a message's `hops_by_msg` entry at both the delivering src and
dst inside the SAME `forward_to` call that reaches the highest hop
count, before the end-of-tick scan-based sample (this session's own
Wake 12 addition) could ever see it, so every terminally-delivered
message's true max hop count was silently under-reported by one. Fixed
by tracking `max_hops_seen` incrementally on `Simulation`, updated at
the exact point `forward_to` computes `new_hops` — before any pruning
can happen — instead of deriving it from a post-hoc scan of state
pruning can invalidate. A fourth failure
(`crypto_e2e::m7_fragmented_encrypted_message_reassembles_and_
verifies`) was investigated and root-caused precisely (`message_
engine::serialize_for_transport`'s fragment-size budget subtracts only
`FRAGMENT_HEADER_LEN` (86 B), not the full envelope's wire overhead
that `codec::encode(frag)` — what actually gets sent — adds on top;
exposed by an upstream RF-43 fix that started actually enforcing
transport MTUs that used to be silently ignored) but deliberately NOT
fixed here — flagged via `spawn_task` (`task_2b02c5f4`) with the full
diagnosis, since it's message-integrity-relevant code well outside a
LoRa wake's scope and deserves independent verification, not a rushed
fix bundled into an unrelated commit. Committed the three genuine
restorations separately (`d34fc1d`) from the Tier 4 finding work
(`a405ec8`) so git history stays legible about what was and wasn't
part of this wake.

**RF-10 (Medium).** Every priority rode LoRa at up to 237 B, contradicting
LORA.md's "P4+ never transmitted over LoRa" and its per-priority
payload budget — read the doc directly rather than trust the finding's
partial quote, confirming P0=60B/P1=80B/P2=120B/P3=150B. Added
`max_payload_for(priority) -> Option<usize>`, wired into both
`enqueue_backlog` and `try_send_inner` (a message can reach the latter
directly via `Transport::send()`, bypassing the backlog). P4+ refused
via `TransportError::PolicyDenied`, not the finding's suggested
`Protocol` — the finding text predates this session's own MG-40 fix
(Tier 2), which narrowed `Protocol` to genuine framing/decode failures
specifically to stop this kind of conflation; using it here would have
reintroduced exactly what MG-40 closed. Six existing tests hardcoded
pre-fix priorities/sizes, all six already named in the finding's own
Dependencies field. Fixed each by hand-deriving new airtime values from
the Semtech AN1200.13 formula and cross-checking against the existing
`airtime_matches_semtech_formula_anchors` test's 9020ms anchor before
trusting my own arithmetic (it matched); ran every fix and confirmed
the derived numbers empirically rather than assuming the hand math was
right. One test (`backlog_holds_while_exhausted_and_drains_after_
recovery`) redesigned to discover its bucket-exhaustion boundary via a
loop instead of a hardcoded frame count tied to a payload size that's
no longer valid for any priority — a more robust pattern, not a
weakened assertion.

**RF-4 + RF-16 (Medium/Low, fixed together — both touch `refund`).**
`DutyCycleTracker::refund` was `pub` and keyed by (class, airtime_ms) —
deterministic and enumerable from `airtime_ms(profile, len)`, so any
code holding the tracker's `Arc` could de-bill a guessable reservation,
arithmetically identical to raising the module's documented "no
runtime override" legal budget (RF-16). Separately, every tx-failure
refunded the reservation regardless of error type, including
`Io`/`HardwareGated` — errors that CAN follow a real emission, not just
ones that prove nothing was radiated (RF-4). Mirrored satellite.rs's
existing `GuardAdmission { token }` pattern (SAT-RT-103) exactly:
`DutyCycleTracker` now mints a unique token per admission, `refund` is
`pub(crate)` and exact-token. Added a fifth `LoRaLinkError::
NotTransmitted` variant; `try_send_inner` classifies before refunding —
`Closed`/`NotTransmitted` refund, `Io`/`HardwareGated` keep the
reservation fail-closed and record a new `tx_unknown_outcome` metric.

**RF-14 (Low).** LDRO's condition (`sf>=11 && bw==125`) missed SF12/BW250
(also >16ms symbol time, LDRO's actual physical trigger), under-counting
that profile's airtime — the one case in a function whose every other
rounding choice is deliberately conservative. Computed DE from the
already-derived `ts_ms` instead of the SF/BW shorthand.

**Run 19 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop` —
green except the one deliberately-unfixed, separately-flagged
`crypto_e2e` failure; `cargo test -p iris-core --lib transport::lora::`
— 33/33 pass; `cargo clippy -p iris-core --lib --no-deps` — no new
warnings in any file touched this run (confirmed by direct
file-location check against the full warning list).

**§8 accounting:** 4 findings fixed this run (RF-4, RF-10, RF-14,
RF-16) in one commit, plus 3 pre-existing-breakage restorations in a
separate commit (not counted against the findings cap). Tier 4: 55 of
126 done, 70 remaining, 1 blocked (TAK-6, TLS/supply-chain gate).

## Run 20 — 2026-08-29 — Tier 4 Wake RF-B: LoRa config verification, structured rate-limit, per-transmission timing (RF-1/5/7)

Owner said "continue with wake RF-B" — the three LoRa findings deferred
from Run 19 (RF-4/10/14/16) for being independently substantial.

**RF-5 (Medium).** `BudgetExhausted`'s computed next-legal-send-window
(RES-0008 R7) was logged at `debug!` and discarded; `try_send_inner`
returned bare `TransportError::Busy`, indistinguishable from ordinary
pool saturation, no retry time. Added `TransportError::RateLimited {
retry_after_ms, remaining_fraction }`, wired into the duty-refusal
branch. This forced dropping `TransportError`'s `Eq` derive (kept
`PartialEq`): `f32` cannot implement `Eq` (NaN) — confirmed via an
actual compile attempt before deciding how to handle it, not assumed.
Grepped the workspace for any HashMap/HashSet key usage or explicit
`Eq` bound on `TransportError` before dropping the derive; found none.
Added `RateLimited` to `is_retryable()` — it names its own recovery
time, so a caller retrying after `retry_after_ms` will get the same
admission decision reconsidered, unlike a permanent policy/hardware
refusal. While implementing this, found satellite.rs has the exact
same pattern — `admit_tx()`'s `GuardRefusal { reason, retry_at_unix_ms
}` collapsed to bare `Busy` at its own call site (checked current
source, not the finding's possibly-stale line numbers: confirmed
still there). Not fixed here — it belongs with the other satellite
spend-guard findings (RF-19..22) already planned as a separate,
later wake, and touching it in isolation now would mean revisiting
the same code twice. Noted for that wake instead.

**RF-7 (Medium).** No ceiling on a single transmission's on-time — a
full P0 frame is a 9.02s continuous emission, monopolizing the
half-duplex channel and likely violating EN 300 220's per-transmission
timing conditions independent of the aggregate 1% duty figure. Checked
all 5 `ComplianceConfig { .. }` construction sites before adding fields
(the same class of break Run 19's SimLoss fix had to clean up) — all 4
test sites already use `..Default::default()`, so only the `Default`
impl itself needed updating. Added `max_ton_ms`/`min_off_ms` +
`DutyCycleTracker::check_transmission_timing`, called BEFORE (not
folded into) `check_and_consume` — a genuinely separate,
`BudgetExhausted`-sibling error type (`TransmissionTimingRefused`),
kept out of `check_and_consume`'s own `Result<NextWindow,
BudgetExhausted>` signature specifically so the ~15 existing call
sites (mostly tests) needed zero changes.

**Deliberately did not use the finding's own suggested `max_ton_ms`
default of 1000ms — verified independently BEFORE writing any tracker
code**, not after: wrote a standalone rustc smoke test of the exact
`airtime_ms` formula and found that at SF12 (the P0/emergency-SOS
profile), even a ZERO-payload frame — just the 18-byte header, the
physical minimum possible — already takes 1319ms. A 1000ms default
would have made P0 categorically unable to transmit over LoRa at all,
under any payload size, forever — the finding's own remediation text
("for P0 this forces fragmentation to ≤1s frames") assumes fragmenting
smaller can bring SF12 under 1 second, which is not physically
possible for this profile. This would have been a far worse outcome
than the 9-second-emission problem RF-7 exists to fix — blindly
implementing the finding's literal suggestion here would have shipped
a regression that silences the exact life-safety traffic class this
whole bug-hunt effort cares most about protecting. Defaulted instead
to 4000ms: comfortably above 3285ms (P0's REAL current worst case, a
60-byte payload under RF-10's own cap, at SF12), so this does not
regress P0 sending, while still meaningfully bounding the old,
uncapped 9020ms worst case the finding was actually concerned about.
`min_off_ms` defaults to 0 (disabled) — no specific EN 300 220
off-time figure was given for this deployment's exact sub-band, and
inventing one risked encoding a wrong regulatory number rather than no
constraint at all; the mechanism is fully wired and testable, just not
defaulted to a number I can't verify. Max-on-time violations map to
`TransportError::PolicyDenied` (no existing variant fits a *time*-
shaped per-request ceiling cleanly — `MessageTooLarge`'s `limit`/
`actual` are byte counts); min-off-time violations map to the new
`RateLimited`. Four new regression tests, including one exercising the
real `try_send_inner` path end-to-end, not just the tracker method
directly.

**RF-1 (Critical, downgraded to High).** The duty tracker bills
airtime for a `RadioProfile` (SF/BW/power) the `LoRaLinkAdapter` trait
had no way to confirm the module is actually transmitting at —
compliance evidence that never observes the radio. Extended the trait
with `configure()`/`read_config()`. Checked both real-adapter impls
before touching them: `AtSerialAdapter`/`SpiNativeAdapter` are both
"shape only" (every existing method already unconditionally returns
`HardwareGated`, pending BLK-0005 hardware procurement), so trivial
matching stubs are zero behavior change, not a new gap. Gave
`SimulatedLoRaAdapter` real configure/read_config state, defaulted to
the compliant `DEFAULT_RADIO_PROFILE`/14dBm — ran the FULL pre-
existing LoRa test suite immediately after this change, before writing
any new RF-1 tests, specifically to confirm every existing
`attach_adapter` call site (which all rely on the simulated adapter's
default state) still succeeded unchanged. It did, 37/37.

`attach_adapter` now calls `read_config()` after `open()` and refuses
to promote to `Available` if the reported frequency disagrees with
`ComplianceConfig.freq_hz` or reported power exceeds `max_erp_dbm` —
the exact cross-check the finding's own evidence section asks for
("RadioProfile.freq_hz and ComplianceConfig.freq_hz... two unrelated
constants both happening to be 866_000_000"). Deliberately checks only
frequency + power, not SF/BW/coding-rate: `try_send_inner` already
reconfigures per-send for whichever profile the specific message needs
(P0 vs everything else), so attach time only needs to confirm the
module's regulatory ceiling agrees with this tracker's, not commit to
one specific SF up front. Added `last_configured_profile` tracking on
`LoRaTransport` so consecutive sends sharing a profile don't redundantly
re-configure the module on every single send. A `configure()` failure
mid-send refunds the just-reserved duty token (mirrors RF-4's own
refund-on-proven-no-emission logic from Run 19) rather than burning
budget for a transmission that never actually got configured. Added
`DutyCycleTracker::compliance_config()` as a small read accessor
(`ComplianceConfig` is `Copy`, so this is a cheap snapshot) since
`attach_adapter` needs the tracker's own config to validate the
module's reported one against. Three new regression tests, including
one that reads the simulated adapter's config back after a real P0
send to confirm `try_send_inner` actually reconfigured it to SF12, not
just that the send succeeded.

**Run 20 closeout:** `cargo build --workspace --exclude iris-desktop
--all-targets` clean; `cargo test --workspace --exclude iris-desktop`
— 711 iris-core lib tests, 0 failed (+8 from this wake: 4 RF-7, 3
RF-1, 1 RF-5), only the separately-flagged, pre-existing `crypto_e2e`
failure remains (task_2b02c5f4, confirmed still unrelated); `cargo
test -p iris-core --lib transport::lora::` — 40/40 pass; `cargo clippy
-p iris-core --lib --no-deps` — no new warnings (checked `error.rs`
specifically, clean; `lora.rs`'s one warning is the pre-existing
`state_tx.send()` pattern shared by every transport file in this
crate).

**§8 accounting:** 3 findings fixed this run (RF-1, RF-5, RF-7) in one
commit, comfortably under the cap. Tier 4: 58 of 126 done, 67
remaining, 1 blocked (TAK-6, TLS/supply-chain gate). This closes out 7
of the RF-1..23 LoRa/satellite duty-cycle cluster (RF-1/4/5/7/10/14/16)
across Wakes RF-A and RF-B — 16 remain (RF-2/3/6/8/9/11/12/13/15/17,
the rest of lora.rs; RF-18..23, the satellite spend-guard cluster).

### Next run (Run 21)
Wake RF-C (LoRa continues): RF-2, RF-3, RF-9 — a natural next cluster.
RF-2: no persistence for the duty budget across process restarts (a
crash-loop or force-stop/reopen resets the hourly budget to 100% for
free) — needs a new `DutyLedger` seam mirroring satellite's existing
`CostLedger` trait, a real new subsystem, not a small fix. RF-3: the
per-bucket proportional share (60/25/10/5%) is enforced as a hard
ceiling instead of work-conserving, so P0 can be refused with the
aggregate budget mostly idle if only its own bucket is spent — read
RF-7's own fix note first ("fragmenting increases total airtime, so
RF-3's work-conserving fix becomes load-bearing") since RF-7 already
landed and may interact. RF-9: `poll_inbound`'s receive loop has no
per-call frame budget, unlike every other transport with an open-medium
attack surface (`wifi_direct.rs`'s `MAX_FRAMES_PER_TICK`) — smallest of
the three, a direct mirror of that existing pattern. Read all three
findings' current source fresh before implementing, same discipline as
every run this session.
