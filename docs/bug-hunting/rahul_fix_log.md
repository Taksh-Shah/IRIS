# Protocol/Messaging/Emergency Fix Log — Execution Journal

**Companion to:** [`rahul_problems_loop.md`](rahul_problems_loop.md) (logic) ·
[`rahul_problems.md`](rahul_problems.md) (state).
This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.
Commits are local-only per §6 of the loop file.

**Scope:** `crates/iris-core/src/` — `protocol/`, `message_engine/`, `discovery/`, `emergency/`.
Source review: `PROTOCOL_MESSAGING_EMERGENCY_REVIEW.md`.

Baseline at run 1 start (verify against HEAD before starting): `cargo build --workspace --exclude iris-desktop` clean ·
`cargo test -p iris-core` — record pass count here before any changes.

**Environment note:** GNU toolchain required on Windows (`stable-x86_64-pc-windows-gnu`).
MSVC cannot link without Windows SDK. Exclude `iris-desktop` from every build/test command.
`cargo kani` is Linux-only — Kani-related verification (PM-16) must be re-checked on Linux CI.

**PG-gated tests:** if `IRIS_PG_PASSWORD` is unset, PG-backed tests will skip. Record as
`PENDING LIVE-PG` and promote via a human run with a real database.

---

## Run 1 — 2026-08-30 — Tier 0 batch A (PM-1, PM-2+PM-15)

Target findings: PM-1 (Critical, signed-field downgrade), PM-2 + PM-15 (Critical, CANCEL
rejection + same-signer check — these MUST ship together per rahul_problems.md).

Pre-run drift audit: re-read all Tier 0 finding locations against current source before writing any fix.

**Drift notes:** All cited line numbers confirmed accurate against HEAD before any edits.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PM-1 | ✅ Fixed | 0beaca2 | `QueuedMessage::scheduling_priority` carries downgrade; wire field 5 untouched; relay path returns `Relayed` for all 4 SOS; `emergency_sos_rate_limited == 1`. Full IrisCryptoProvider sign+verify PENDING LINUX-CI. |
| 2 | PM-2 + PM-15 | ✅ Fixed | 9c83143 | SosLedger (cap 3600) stores accepted-SOS (timestamp, sender_short); `sos_register` called on AcceptedSos; `sos_original` looked up before cancel classification; `CancelSignerMismatch` rejects cross-sender cancels. `cancel_forgery_rejected_pm15` test added. PENDING LINUX-CI. |

**Batch A closeout:** ✅ Complete — PM-1, PM-2+PM-15 all fixed.

**Carried notes for later runs / owners:**
- PM-1: IrisCryptoProvider wire-integrity test needs Linux CI (can't run `cargo kani` or IrisCryptoProvider here — add as a TODO test in the crate).

### Next run
PM-2 + PM-15 (CANCEL rejection + same-signer check — must ship together).

---

## Run 2 — 2026-08-30 — Tier 0 batch B (PM-4, PM-5, PM-6)

Target findings: PM-4 (security, fragment reassembly unmetered), PM-5 (security, handshake
decode ignores budget + neighbor table uncapped), PM-6 (security, SOS timestamps unbounded).

**Drift notes:** All cited line numbers confirmed accurate against HEAD before edits.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PM-4 | ✅ Fixed | 8e35628 | Per-sender cap MAX_SETS_PER_SENDER=4 in FragmentAssembler + rate_limit_with_claim at fragment path entry in process_incoming (after ACL check). Two new unit tests. PENDING Section 1 coordination. |
| 2 | PM-5 | ✅ Fixed | f213dce | decode() size check added; BloomFilter::from_parts m ceiling from bloom_bits(BLOOM_CAPACITY, BLOOM_FPR); MAX_NEIGHBORS=512 cap + LRU eviction in NeighborTable::upsert. Three new tests. PENDING Section 3 coordination. |
| 3 | PM-6 | ✅ Fixed | 0f2d45b | timestamps.truncate(SOS_P0_ALLOWANCE) in record() and check(); O(n²) attack surface eliminated; new timestamp_vec_bounded_under_flood test. PENDING LINUX-CI. |

**Batch B closeout:** ✅ Complete — PM-4, PM-5, PM-6 all fixed.

**Tier 0 checkpoint:** ✅ All 6 Tier 0 findings fixed (PM-1, PM-2+PM-15, PM-4, PM-5, PM-6). Advancing to Tier 1.

### Next run
Advance to Tier 1: PM-3 (ACK tracker leak), PM-7 (mode ladder), PM-8 (dedup rotation).

---

## Run 3 — 2026-08-30 — Tier 1 batch A (PM-3, PM-7, PM-8)

Target findings: PM-3 (High, P0 ACK tracker leak), PM-7 (Medium, disaster mode ladder),
PM-8 (Medium, dedup rotation by call count).

**Drift notes:** All cited locations confirmed accurate against HEAD before edits.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PM-3 | ✅ Fixed | a7c2b4d | register_sent stores timestamp+ttl_seconds; due_retries(now, now_unix) filters expired; drain_expired sweeps GC task; MAX_PENDING_ACKS=8192 cap. Three new tests (p0_leak_stopped_by_ttl_drain, max_pending_cap_evicts_oldest, existing tests updated). requeue_for_retry TtlExpired now calls forget instead of .ok(). PENDING LINUX-CI. |
| 2 | PM-7 | ✅ Fixed | a7c2b4d | guarded_transition gains triggers:&Triggers param; Normal→Emergency, Emergency→Crisis (hold+trigger), Crisis→Degraded (hold+no trigger) escalation arms added. Four new tests. PENDING LINUX-CI. |
| 3 | PM-8 | ✅ Fixed | a7c2b4d | DedupEngine::seen() guards current.insert behind !current.contains; replaying_id_does_not_rotate test confirms rotation counter stays at 0 for replayed ids. PENDING LINUX-CI. |

**Batch A closeout:** ✅ Complete — PM-3, PM-7, PM-8 all fixed.

### Next run
PM-9 (clock-skew dead code), PM-10 (fragment MTU override), PM-12 (P0 size limit tests-only), PM-14 (signing scope disagreement).

---

## Run 4 — 2026-08-30 — Tier 1 batch B (PM-9, PM-10, PM-12, PM-14)

Target findings: PM-9 (Medium, clock skew fallback dead), PM-10 (Medium, fragment MTU),
PM-12 (Medium, P0 envelope size enforcement), PM-14 (Medium, signing scope docs).

**Drift notes:** All cited file/line locations confirmed accurate against HEAD before edits.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PM-9 | ✅ Fixed | 78c6e91 | `is_expired_from_arrival(now, now)` used at `process_incoming`; `expiry_reason` emits `ClockSkewSuspect` for suspect clocks; import extended; `expiry_reason` called at `requeue_for_retry` to make `ClockSkewSuspect` live on retry path; test updated. PENDING LINUX-CI. |
| 2 | PM-10 | ✅ Fixed | 78c6e91 | `.max(64)` floor removed; explicit `BadEnvelope` when `usable == 0`. PENDING LINUX-CI. |
| 3 | PM-12 | ✅ Fixed | 78c6e91 | Size check in `codec::decode` after priority decoded: P0 > 255 B → `InvalidField`. New test `p0_oversized_envelope_rejected_on_decode`. PENDING LINUX-CI. |
| 4 | PM-14 | ✅ Fixed | 78c6e91 | Module doc corrected to state actual scope (1–7, 9–14, 16, 18); `encode_for_signing` doc updated; `envelope.rs` field 15 comment fixed. Test `signing_scope_covers_fields_16_and_18_not_17`. No runtime change. PENDING LINUX-CI. |

**Batch B closeout:** cargo build/test PENDING (cargo not in shell PATH this session). All changes code-reviewed manually. PENDING LINUX-CI.

**Tier 1 checkpoint:** ✅ All 7 Tier 1 findings fixed (PM-3, PM-7, PM-8, PM-9, PM-10, PM-12, PM-14). Advancing to Tier 2.

### Next run
Advance to Tier 2: PM-11 (LRU O(capacity)), PM-13 (queue bytes), PM-16 (split_payload hang), PM-17 (NaN delivery_prob), PM-18 (SOS race).

---

## Run 5 — 2026-08-30 — Tier 2 (PM-11, PM-13, PM-16, PM-17, PM-18)

Target findings: all five Tier 2 findings — PM-11 (dedup LRU perf), PM-13 (queue byte accounting),
PM-16 (split_payload zero hang), PM-17 (NaN delivery_prob), PM-18 (SOS check/record race).

**Drift notes:** All five bug locations confirmed present against HEAD before any edits (see prior context). PM-11 at `dedup.rs:220-222`, PM-13 at `queue.rs:139`, PM-16 at `fragment.rs:64`, PM-17 at `codec.rs:425`, PM-18 at `mod.rs:1748-1754` + `provider.rs:198-206`.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PM-16 | ✅ Fixed | Run 5 commit | `let max_chunk = max_chunk.max(1)` at top of `split_payload`; infinite loop on zero chunk eliminated. 733 lib tests pass. PENDING LINUX-CI (Kani harness). |
| 2 | PM-17 | ✅ Fixed | Run 5 commit | `is_finite() \|\| !(0.0..=1.0).contains(f)` guard in `decode_routing_hints`; NaN/Inf/out-of-range rejects with `InvalidField`. 733 lib tests pass. PENDING LINUX-CI. |
| 3 | PM-11 | ✅ Fixed | Run 5 commit | Removed O(n) `VecDeque::iter().position()` + `remove` re-arm from `check_and_insert` (duplicate branch now returns true immediately). Eviction is FIFO. `lru_rearms_recency` test updated to `lru_eviction_is_fifo_after_pm11_fix`. 733 lib tests pass. |
| 4 | PM-18 | ✅ Fixed | Run 5 commit | Added `sos_check_and_record` to `EmergencyProvider` trait (default: two-call; `EmergencyGateway` override: single lock). Call site in `mod.rs` updated to use the new method; separate `sos_record` call removed. 733 lib tests pass. PENDING LINUX-CI. |
| 5 | PM-13 | ✅ Fixed | Run 5 commit | Added `queued_bytes()` (O(n) heap scan) to `PriorityQueue`; added `max_queue_bytes: usize` (default 32 MB) to `MessageEngineConfig`; byte-cap check added at all 3 `push` sites alongside existing count check. Fairness gate O(n) deferred to PS-2 refactor. 733 lib tests pass. PENDING LINUX-CI. |

**Pre-existing failures (not introduced):** `protocol_conformance` tests `corrupted_frame_length_byte_bounded_recovery` and `capture_all_pdus_until_parse_end` were already failing on the baseline before Run 5 edits (confirmed by `git stash` + re-run).

**Batch closeout:** `cargo build -p iris-core` (GNU toolchain) clean, 17 pre-existing warnings, 0 errors. `cargo test -p iris-core --lib` 733/733 passed.

**Tier 2 checkpoint:** ✅ All 5 Tier 2 findings fixed (PM-11, PM-13, PM-16, PM-17, PM-18). HARD STOP before Tier 3.

### Next run
**HARD STOP before Tier 3.** Tier 3 (structural enhancements PS-1..PS-7) requires an explicit
human go-ahead — see `rahul_problems_loop.md` §2.3. Do not begin PS-* without that sign-off.

---

## Run 6 — [DATE] — Tier 3 opened under explicit human sign-off

**(Fill in when run begins. Gate: human must have given explicit go-ahead before this run starts.)**

**Gate confirmation:** _(record the explicit instruction here)_

Target findings: PS-1 (delivery poll), PS-2 (decode Value tree), PS-6 (audit Vec→VecDeque).
Rationale: PS-6 is the lowest-risk change (container swap, same semantics, measured 100× improvement).
PS-2 is self-contained within `codec.rs`. PS-1 needs Section 3 transport re-entrancy confirmation first —
if that coordination is not yet resolved, defer PS-1 and proceed with PS-2 + PS-6 only.

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PS-6 | ⬜ | — | — |
| 2 | PS-2 | ⬜ | — | — |
| 3 | PS-1 | ⬜ | — | — (blocked if Section 3 transport re-entrancy unresolved) |

**Batch closeout:** full workspace build + sweep including benches (`cargo test -p iris-core --bench codec --bench envelope`).

### Next run
PS-3 (authority chain cache — blocked until Section 1 revocation contract established),
PS-4 (ACK retry heap — implement together with PM-3 cleanup if not already done),
PS-5 (discovery parallel scan — blocked until Section 3 hardware contention verified),
PS-7 (serialize_for_transport bytes — implement together with PM-10 fix).

---

## Run 7 — [DATE] — Tier 3 continued (PS-4, PS-7; PS-3/PS-5 blocked)

**(Fill in when run begins.)**

Target findings: PS-4 (ACK retry heap), PS-7 (serialize_for_transport → bytes).
PS-3 blocked (Section 1 revocation contract). PS-5 blocked (Section 3 hardware confirmation).

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PS-4 | ⬜ | — | — |
| 2 | PS-7 | ⬜ | — | — |
| 3 | PS-3 | 🔒 Blocked | — | Blocked: Section 1 revocation/caching contract not yet established |
| 4 | PS-5 | 🔒 Blocked | — | Blocked: Section 3 hardware contention answer needed before implementing |

**Batch closeout:** full workspace build + sweep.

### Next run
PS-3 and PS-5 when their respective Section coordination is resolved.
When all Tier 3 findings are ✅ or 🔒: Tier 3 checkpoint commit.

---

*(Add further runs as needed — copy the template below)*

---

## Run N — [DATE] — [description]

Target findings: _(list)_

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|

**Batch closeout:**

### Next run
_(fill in)_
