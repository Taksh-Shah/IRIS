# Priyam Fix Log — Section 1 (Crypto, Identity & Security) bug-hunt execution journal

**Companion to:** [`priyam_problems_loop.md`](priyam_problems_loop.md) (logic) ·
[`priyam_problems.md`](priyam_problems.md) (state).

This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. **One entry per run.** Commits
are local-only per §6 of the loop file.

---

## Baseline (run 0) — 2026-08-31

**Commit:** `23ad303` (branch `main`, clean tree).

**Report authored.** 30 findings across `crates/iris-core/src/{crypto,identity,security}/`:

| Tier | Findings | Count |
|---|---|---|
| 0 | PRY-5, PRY-7, PRY-14, PRY-19, PRY-22, PRY-28 | 6 |
| 1 | PRY-8, PRY-12, PRY-13, PRY-15, PRY-16, PRY-18, PRY-20, PRY-21, PRY-23, PRY-24, PRY-25, PRY-26, PRY-27, PRY-29, PRY-30 | 15 |
| 2 (GATED) | PRY-1, PRY-2, PRY-3, PRY-4, PRY-6, PRY-9, PRY-10, PRY-11, PRY-17 | 9 |

**Nothing fixed yet.** All 30 findings `⬜ Not started`. (See Run 0b for +6 from the second pass → 36 total.)

### Drift audit (run 0)

All 30 findings verified present at HEAD `23ad303` against current sources; zero subsumed;
all line citations checked by direct read. Key confirmations:

- **PRY-1:** `grep -n 'add_authority' crates/iris-core/src` → definition (`acl.rs:109`) +
  test call sites only. `FullSecurityPolicy` holds `Arc<EmergencyAcl>` (`security/mod.rs:156`).
  `check_authority_chain` empty-allowlist → `AclDecision::Authorized` (`acl.rs:161-164`).
- **PRY-2 / PRY-3:** `grep -n 'priority_reserved_pool_bytes\|total_quota_bytes\|select_eviction_candidates'
  crates/iris-core/src` → all matches are `Default::default()`, struct definitions, or tests.
  No production reader of either config field; no production caller of
  `select_eviction_candidates`.
- **PRY-4:** `grep -n 'decay' crates/iris-core/src` → `reputation.rs:254` (def),
  `reputation.rs:414` + `:648` (tests). No `message_engine` / GC caller.
- **PRY-5:** `crypto/x25519.rs:18-29` — `diffie_hellman` body contains no `is_small_order`
  call; only the all-zero output check.
- **PRY-7:** `protocol/message_id.rs:47-51` — `sequence_hint` = `u64::from_be_bytes(self.0[..8])`.
  Consumed at `message_engine/mod.rs:598`.
- **PRY-13:** `rate_limiter.rs:235`, `quota.rs:168`, `replay.rs:391`, `spam.rs:123` all take
  `.write().await` on the per-engine map on the inbound path.
- **PRY-29:** both `acl.rs:230` and `emergency/authority.rs:129` currently DO add the
  `is_authority_root` check after `verify_chain` — so this is a defense-in-depth / refactor
  finding, not a live bypass. Confirmed.

### Environment note

Build/test gate status for this environment (`cargo` on PATH?) to be confirmed at the first
fix run. If Rust is not installed here, fixes land as `✅ Fixed · PENDING LOCAL BUILD` per
loop §8 and a human promotes them after a real `cargo test -p iris-core` pass — same
discipline taksh's loop used for its PENDING LIVE-PG / PENDING HARDWARE legs (see
`taksh_fix_log.md`).

### Tier 2 sign-off tracker (populate before any Tier 2 commit)

| Finding | Owner needed | Sign-off recorded? |
|---|---|---|
| PRY-1 | Section 2 (engine drop-on-`InvalidAuthority` behaviour) + EMERG-001 (allowlist provisioning) | ⬜ |
| PRY-2 | Section 3 (`iris-storage` / store ceiling — reconcile with TAK-2) | ⬜ |
| PRY-3 | Section 3 (who owns eviction — quota layer or store) | ⬜ |
| PRY-4 | Section 3 (routing-weight epsilon-floor policy) + Section 2 (GC cadence wiring) | ⬜ |
| PRY-6 | Section 2 (`process_incoming` ordering — high-water advance past signature check) | ⬜ |
| PRY-9 | Section 2 (which `ContentType`s route to `check_emergency_acl`) + EMERG-001 (drill marker) | ⬜ |
| PRY-10 | Section 2 (is every emergency envelope signature-verified before delivery?) | ⬜ |
| PRY-11 | EMERG-001 + safety owner (who can send an SOS) | ⬜ |
| PRY-17 | EMERG-001 (intended chain counter semantics) | ⬜ |
| PRY-32 | EMERG-001 (which authority-verification path is authoritative) + Section 2 (engine call site) | ⬜ |
| PRY-33 | Section 2 (`seal_outbound` fail-closed default) + Sections 4/5 (UI "encryption pending" state) | ⬜ |
| PRY-34 | Section 2 (relay call site) + Section 3 (`iris-storage` accounting — who owns relay quota) | ⬜ |

---

## Run 0b — 2026-08-31 — second deeper pass

**Commit:** still `23ad303`. Report bumped to revision 2. Six findings added: PRY-31…PRY-36.
Total now **36** (Tier 0: 7 · Tier 1: 17 · Tier 2: 12).

### What the second pass did

- Full read of `emergency/authority.rs`, `message_engine/mod.rs:353-480` (seal/send),
  `message_engine/mod.rs:995-1035` (relay) — previously only grepped.
- Byte-by-byte verification of `identity/small_order.rs::SMALL_ORDER_U` against the canonical
  X25519 small-order encodings.
- Caller-grep of every fail-closed / authenticated Section 1 primitive.

### New findings

| ID | Sev | Tier | One-line |
|---|---|---|---|
| PRY-31 | Medium | 0 | `SMALL_ORDER_U[2]` corrupted (internal byte-repeat, ≠ its own doc-comment value); array missing `p−1/p/p+1`; no cross-check test. `to_edwards` fallback is the only real guard and its non-canonical-encoding coverage is untested. |
| PRY-32 | Medium | 2 | `security/acl.rs` emergency path checks only chain+allowlist+`is_authority_root` — no geo scope / severity cap / drill discipline / validity window (all of which `emergency/authority.rs::verify_authoritative` does). Two parallel paths; the wired one is weaker. |
| PRY-33 | Medium | 2 | `seal_outbound` sends **plaintext** when the recipient X25519 key is absent (`MSG_SENT_UNENCRYPTED`, proceeds). `crypto::key_directory::require_key` (fail-closed) — 0 non-test callers. RED-0002 regression. |
| PRY-34 | Medium | 2 | Relay path runs `rate_limit_with_claim` but never `quota.add_message` — relayed traffic bypasses per-sender storage quota entirely. |
| PRY-35 | Low | 1 | `chain.rs` rule 6 (`identity_pubkey.as_slice() != sender_id`) can never match a 16-byte abbreviated `sender_id` → abbreviated emergency envelopes carrying a chain are always `SenderMismatch`. |
| PRY-36 | Low | 1 | `revoke` / `un_revoke` / `adopt_rotation` / `register_authority_root` are `pub` and self-authenticate nothing; `register_authority_root` overwrites existing entries unconditionally. |

### Drift audit (Run 0b)

- **PRY-31:** canonical LE encoding of `325606…233504` is
  `e0 eb 7a 7c 3b 41 b8 ae 16 56 e3 fa f1 9f c4 6a da 09 8d eb 9c 32 b1 fd 86 62 05 16 5f 49 b8 00`;
  code has `…3b 41 e8 e3 36 1f f0 1a 31 55 38 dc 1f bf 01 a3 15 38 dc 1f bf 01 a3 15 38 dc 0f 5f`
  — divergent from byte 6, repeat `1f bf 01 a3 15 38 dc` at offsets 16-22 and 23-29.
  `SMALL_ORDER_U[3]` (`5f 9c 95 bc a3 50 …`) verified **correct**. `is_small_order`'s fast
  path is `SMALL_ORDER_U.iter().any(|u| u == public_key)` so a garbled entry passes
  `classic_small_order_values_are_rejected` (a value always equals itself).
- **PRY-32:** `grep verify_authoritative` → called only from `emergency/broadcast.rs:48`.
  `security/acl.rs::check_authority_chain` never decodes `envelope.payload`.
- **PRY-33:** `grep require_key` → `key_directory.rs:43` (def) + `:63,66` (its own test). Zero
  production callers. `message_engine/mod.rs:397-408` logs `MSG_SENT_UNENCRYPTED` and falls
  through to `self.crypto.sign(envelope)`.
- **PRY-34:** `grep add_message` in `message_engine/` → `:452` only (outbound `send_message`).
  Relay path `:995-1030` has `rate_limit_with_claim` but no quota call.

---

## Run 1 — 2026-08-31 — Tier 0

**Branch:** `main` (per explicit user instruction — the loop's default-branch guard was
waived; all teammates' bug-hunt work lands on `main` in this repo). Baseline commit `86009dd`.

**Environment:** `cargo 1.97.1` / `rustc 1.97.1` on PATH — build + test gates ran for real
(not PENDING). Baseline `cargo build -p iris-core` clean (23 warnings, pre-existing).
`cargo test -p iris-core` baseline: 741 lib tests pass; **2 pre-existing failures** in
`tests/protocol_conformance.rs` (`corrupted_frame_length_byte_bounded_recovery`,
`capture_all_pdus_until_parse_end`) — verified present on clean `86009dd` via `git stash`;
protocol/framing, outside Section 1 scope (§8), not touched by this run.

### Drift notes

- **PRY-3 stub in loop plan** ("deque compaction") — dropped. After reading the eviction
  loop + a Python simulation (64 identities × 40 cycles through an 8-bucket cap), `order` and
  `map` grow/shrink together 1:1 (`max(order − map) == 0`). The "unbounded" premise does not
  hold; below the cap `map` grows identically (nothing evicts). Marked **⚪ Not applicable**.
- **PRY-7** — the good fixes (real counter / bounded seen-set) all **widen** the replay
  accept path and touch `message_id` (Section 2). Per loop §4 step 4 the behavioural fix is
  **re-tiered to Tier 2** (🔒, needs Section 2 sign-off). Only the honest-doc + pinning-test
  part landed under Tier 0.
- **PRY-5** — new `CryptoError::SmallOrderPeerKey` forced a one-line match-arm in
  `message_engine/crypto.rs` `From<PrimCryptoError>` to keep the tree green. This is a
  mechanical compile-glue change placing the variant in the **same rejection group** as
  `AllZeroSharedSecret` (→ `CryptoError::Codec`). It changes **no** accept/deny decision and
  does not widen. Flagged here for the Section 2 owner's awareness; not treated as a §8 stop.
- **PRY-31** — correct `SMALL_ORDER_U[2]` bytes obtained from
  `curve25519_dalek::constants::EIGHT_TORSION[1].to_montgomery()` via a throwaway test (added,
  run, removed), not hand-transcribed. Value matches the report's stated canonical LE
  encoding exactly.

### Tests that encoded old behaviour (§8 — rewritten, called out here)

- `rate_limiter::tests::p0_p1_exempt_from_drops` — asserted bare `check(P0/P1) == Exempt`.
  Rewritten to assert exemption via `check_with_claim(Emergency)` + that bare `check(P0)` is
  now metered.
- `rate_limiter::tests::rate_limiter_respects_exempt_p0_p1` (proptest) — same, rewritten to
  `check_with_claim(Emergency)`.
- `sysval_security_flood::p0_never_dropped_even_under_full_throttle` — same, rewritten.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-19 | ✅ Fixed · Tier 0 | `0095c8d` | `sanitize_config` (non-finite → default), finite-guard after update + decay, `total_cmp` eviction. New `nan_config_does_not_panic_reputation_engine` (discriminates: pre-fix `min_by(partial_cmp().unwrap())` panics on NaN). `cargo test -p iris-core --lib security::reputation` green. |
| 2 | PRY-14 | ✅ Fixed · Tier 0 | `7471a56` | Nanosecond refill arithmetic in `check_bucket` + `refill`. New `sub_second_refill_interval_actually_refills` (discriminates: pre-fix `intervals == 0` at 600 ms → still `SilentDrop`). |
| 3 | PRY-22 | ✅ Fixed · Tier 0 | `7471a56` (+ test `443b7a2`) | `RateLimiter::check` now passes `EmergencyClaim::Ordinary`. New `check_treats_content_as_ordinary_not_emergency`. 3 old-behaviour tests rewritten. CI grep-guard **deferred** — `.github/workflows/` is outside Section 1 scope (§8). |
| 4 | PRY-5 | ✅ Fixed · Tier 0 | `67a2b0a` | `is_small_order(peer_public)` is the first line of `diffie_hellman`; new `CryptoError::SmallOrderPeerKey`. New `diffie_hellman_rejects_every_eight_torsion_point` + non-canonical case. `--test crypto_e2e` + full `security::`/`crypto::`/`identity::` green (174 lib tests). |
| 5 | PRY-31 | ✅ Fixed · Tier 0 | `67a2b0a` | `SMALL_ORDER_U[2]` corrected from `EIGHT_TORSION`; canonical-form guard (`u >= p` → reject) catches `p`/`p+1`; new `small_order_constants_are_genuine` (decompress-and-verify, defeats the tautological old test) + `non_canonical_identity_encodings_are_rejected`. |
| 6 | PRY-7 | ◐ Partial ✅ / 🔒 | `bdac72b` | Doc comments on `HighWaterMark` + `check()` corrected; new `same_timestamp_lower_seq_is_rejected_after_higher` pins current (wrong) behaviour. Behavioural fix **re-tiered to Tier 2** — widens accept path, needs Section 2 `message_id` owner. |
| — | PRY-28 | ⚪ Not applicable | — | Premise disproved (see Drift notes + finding entry). No code change. |

### Batch closeout

- `cargo build -p iris-core --all-targets`: clean (pre-existing warnings only).
- `cargo test -p iris-core`: 741 lib + all security/crypto/identity integration tests pass;
  8 new regression tests pass; the only 2 failures are the pre-existing
  `protocol_conformance.rs` framing tests (baseline, out of scope).
- `cargo clippy` / `cargo fmt --check`: not yet run this wake — deferred to the Tier 0
  checkpoint / next wake.

### Tier 0 status

6 ✅ + 1 ⚪ of 7. **Materially complete.** PRY-7's remaining behavioural work is now a Tier 2
gated item. Next wake: run `cargo clippy -p iris-core -- -D warnings` + `cargo fmt --check`,
make the Tier 0 checkpoint commit, then begin **Tier 1** (smallest-first: PRY-27, PRY-30,
PRY-20, PRY-21, PRY-13, PRY-26 …). **Do NOT enter Tier 2** without the §2.3 sign-off.

### Tier 2 sign-off tracker — addition

| Finding | Owner needed | Sign-off recorded? |
|---|---|---|
| PRY-7 (behavioural) | Section 2 (`message_id` — real per-sender counter vs bounded per-`(sender,ts)` seen-set; inbound engine path) | ⬜ |

---

## Run 2 — 2026-08-31 — Tier 1 (batch 1)

**Branch:** `main`. Baseline commit `5e4d077` (Tier 0 checkpoint).

### ⚠️ Tier 2 gate — human sign-off recorded

The operator (repo owner, this session) explicitly instructed: *"remove the tier 2 gate,
I allow everything."* This is the §2.3 human go-ahead. **Recorded scope and caveats:**

- The **widen-an-accept-path** restriction (§1 second invariant, §4 step 4) is **lifted** —
  fixes that make the security layer accept more may now land, with extra verification.
- The operator **cannot** speak for the other section owners. Findings whose fix must edit
  files **outside Section 1 ownership** (`message_engine/`, `emergency/`, `message.rs`,
  `error.rs`, `kani_proofs.rs`) still hit the §8 hard file-boundary and are recorded
  `🔒 Blocked · needs <owner> — file outside Section 1` until that owner acts. This affects
  **PRY-29** (caller in `emergency/authority.rs`), **PRY-30** (coupled to Tier-2 PRY-3 +
  `kani_proofs.rs` assertion), **PRY-32 / PRY-33 / PRY-34** (fix lands at a Section 2 call
  site), and the **PRY-7 behavioural half** (`message_id`).
- Tier 2 findings that are **fully Section-1-scoped** (PRY-1, PRY-2, PRY-3, PRY-4, PRY-6,
  PRY-9, PRY-10, PRY-11, PRY-17 — subject to per-finding file-scope check) may now proceed
  in a later wake under the normal per-finding protocol + heavier verification.

### Environment

`cargo 1.97.1` on PATH. `cargo fmt --check` fails **crate-wide** on ~240 pre-existing files
(rustfmt-version drift; the repo is not `cargo fmt`-clean and never was on this toolchain) —
not a regression from this loop. My changed files are kept rustfmt-consistent by hand. The
2 pre-existing `protocol_conformance.rs` failures persist (baseline, out of scope).

### Tests that encoded old behaviour (§8 — rewritten, called out here)

- `replay::tests::replay_freshness_only_untrusted` — asserted
  `check_freshness_only(now + 600) == TooFuture`. Post-PRY-27 the future bound is
  `future_window (300) + skew_budget (300) = 600`, so `now + 600` is the boundary. Rewritten
  to assert `now + 450` → `Accepted` (the widened case) and `now + 601` → `TooFuture`.
- `replay::tests::replay_metrics_snapshot` — same `now + 600` assertion, bumped to `now + 601`.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-26 | ✅ Fixed · Tier 1 | `ea9ba9c` | Doc-only. `verify_chain` comment rewritten: stateless, caller must `adopt_advertisement`. No behaviour change; `identity::` tests green. |
| 2 | PRY-20 | ✅ Fixed · Tier 1 | `dab45c1` | `+0.1` bump gated on `HashSet::insert` returning true. New `repeated_register_verified_peer_does_not_stack_bonus` (discriminates: pre-fix score climbs each call, hits max). |
| 3 | PRY-23 | ✅ Fixed · Tier 1 | `a1a9665` | `read_secret_32` closure returns `Zeroizing<[u8;32]>` — transient decode copy wiped on drop. Existing `from_bytes` round-trip + `seeds_are_zeroized_on_drop` cover correctness; transient-copy zeroization not independently observable (finding calls the fix "Trivial"). |
| 4 | PRY-21 | ✅ Fixed · Tier 1 | `204f34c` | Cap eviction picks `min_by_key(message_count)` not `keys().next()`. New `spam_cap_evicts_least_established_sender` — deterministically passes post-fix; pre-fix passes only ~50% (arbitrary hash order), so it discriminates probabilistically. |
| 5 | PRY-25 | ✅ Fixed · Tier 1 | `c38ee98` | `Cargo.toml`: `x25519-dalek` features `["static_secrets","zeroize"]` (the §8-permitted Cargo.toml edit). New `x25519_static_secret_is_zeroized_on_drop` (alloc/`drop_in_place`/read — fails if the zeroize feature is ever removed). `Clone` retained — removing it ripples to `runtime_node_identity` + tests, broader than a Low finding warrants; noted in the report. |
| 6 | PRY-27 | ✅ Fixed · Tier 1 · **WIDENS** | `7b5683a` | `check_freshness_only` future bound now `+ per_source_skew_budget`, matching `check()`. Landed under the operator sign-off recorded above. 2 old-behaviour tests rewritten. `now + 450` (7.5 min fast clock) now `Accepted`; `now + 601` still `TooFuture`; no durable state advanced on this path. |

### Batch closeout

- `cargo build -p iris-core`: clean.
- `cargo test -p iris-core`: **744 lib tests pass** (741 baseline + 3 new) + all
  security/crypto/identity integration tests; only the 2 pre-existing
  `protocol_conformance.rs` framing failures remain (baseline, out of scope).
- `cargo test -p iris-core --features proptest security::`: **109 passed, 0 failed** (451s) — the four security engines' proptests (acl, replay, quota, rate_limiter, spam, reputation) all green after the Tier 0 + Tier 1 batch-1 changes.
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged); my hunks kept consistent.

### Tier 1 status after Run 2

**6 ✅ / 11 ⬜.** Remaining: PRY-8, PRY-12, PRY-13, PRY-15, PRY-16, PRY-18, PRY-24, PRY-29,
PRY-30, PRY-35, PRY-36. Expected blocks: **PRY-29** (caller in `emergency/authority.rs` —
§8), **PRY-30** (coupled to Tier-2 PRY-3 + `kani_proofs.rs` assertion — §8). Next wake:
continue Tier 1 smallest-first (PRY-35, PRY-36, PRY-24, PRY-12, PRY-15, PRY-16).

---

## Run 3 — 2026-08-31 — Tier 1 (batch 2) — **Tier 1 complete**

**Branch:** `main`. Baseline `b35698c`. `cargo 1.97.1`. Operator asked to "complete the whole
tier" — this wake exceeds the §8 6-findings cap (8 fixed) by explicit instruction; each fix
still got its own build + targeted-test + regression test.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-8 | ✅ Fixed · Tier 1 | `e748a12` | `HighwaterTable` now carries a local monotonic `first_seen` generation; eviction: stale-by-window first, else highest `first_seen` (the flood's newest). Unforgeable — an attacker cannot backdate a local counter the way they can a mark `timestamp`. New `sybil_flood_does_not_flush_in_window_victim` (pre-fix: `min_by_key(timestamp)` evicts the victim → replay accepted). Parts 2/3 of the finding (registration rate-limit, `(sender, time-bucket)` keying) deferred — larger. |
| 2 | PRY-15 | ✅ Fixed · Tier 1 | `e748a12` | `load_snapshot` rejects unknown `version` (`SNAPSHOT_VERSION` const) and merges via `check_and_advance` (advance-only). New `load_snapshot_never_regresses_a_live_mark`, `load_snapshot_rejects_unknown_version`. `replay_load_snapshot_and_reset` still green (merge into empty map == replace). |
| 3 | PRY-18 | ✅ Fixed · Tier 1 | `9786826` | `RotationAdopted` branch + `adopt_rotation` drop `Verified` → `Unverified` on any static-key change. New `verified_peer_drops_to_unverified_on_rotation` (both paths). No existing test asserted rotation-keeps-Verified, so none rewritten. UX note for Sections 4/5: a `Verified→Unverified` transition on rotation is new. |
| 4 | PRY-36 | ✅ Fixed · Tier 1 | `9786826` | `revoke` / `un_revoke` / `adopt_rotation` → `pub(crate)` (all callers in-crate: `rotate.rs`, test modules — verified by grep, no external-crate use). `register_authority_root` refuses `key_gen_counter < stored` (rotation-replay). Doc contracts added. New `register_authority_root_refuses_stale_counter`. |
| 5 | PRY-35 | ✅ Fixed · Tier 1 · **WIDENS** | `56eec04` | Rule 6 compares a 16-byte `sender_id` against `peer_short(identity_pubkey)`, 32-byte directly, else new `BadSenderIdLength`. Abbreviated authority chains — previously always `SenderMismatch` — now validate. Under operator sign-off. New `abbreviated_sender_id_matches_via_peer_short`. |
| 6 | PRY-16 | ✅ Fixed · Tier 1 | `c9d545f` | `last_content_hash` → ring of recent hashes (plaintext A,B,A,B) + a content-agnostic signal: same payload size to ≥4 distinct recipients with a 75% size-majority in the last 16. Survives E2EE (fixed AEAD overhead). New `spam_same_size_fanout_flags_but_single_recipient_does_not`. Existing `spam_duplicate_content_penalty` / `spam_high_volume_penalty` still green. |
| 7 | PRY-12 | ✅ Fixed · Tier 1 | `f6f36e9` | Doc-only (like PRY-26). Marked `require_chain_for_broadcast_medical`, `sos_rate_limit_per_hour`, `AclDecision::SosRateLimited`, and the effectively-immutable `allowlists` as reserved / EMERG-001-owned. Deleting the variant would break a `message_engine` match (§8); implementing the limiter needs the interior-mutable allowlist from PRY-1 (gated). |
| 8 | PRY-24 | ✅ Fixed · Tier 1 | `0ef2c02` | Windows `app_data_dir` tries APPDATA → LOCALAPPDATA → `%USERPROFILE%\AppData\Roaming`, then **panics** instead of `PathBuf::from(".")`. Logic in `resolve_windows_app_data(get_fn)` for env-race-free testing. New `#[cfg(windows)] windows_app_data_never_falls_back_to_cwd`. unix/macOS `.` fallbacks left (out of this finding's scope). |

### Blocked (3)

| Finding | Reason |
|---|---|
| PRY-13 | Full fix = shard-by-prefix / lock-free-atomics across all four engines + a `benches/` concurrency benchmark + (if atomics) a loom model. The finding itself says measure first, and `FullSecurityPolicy` is not yet wired into the platform engines so contention is unmeasurable. Disproportionate to a hygiene wake — recommend a dedicated performance pass. The one acute sub-item (`maybe_schedule_snapshot` map-clone) runs once per 30 s, not per message. |
| PRY-29 | The `RootRequirement` parameter must be threaded through both `verify_chain` callers; one is `emergency/authority.rs` — outside Section 1 file ownership (§8). Needs EMERG-001. No live bypass (both callers already gate on `is_authority_root`). |
| PRY-30 | Coupled to Tier-2 PRY-3 and needs the Kani proof `quota_eviction_bounded_and_positive` (`kani_proofs.rs`, Section 2-owned, §8) updated. Fix together under a Section 2 agreement. |

### Batch closeout

- `cargo build -p iris-core --all-targets`: clean (pre-existing warnings only).
- `cargo test -p iris-core`: **752 lib tests pass** (744 + 8 new) + all security/crypto/identity
  integration tests. Only the 2 pre-existing `protocol_conformance.rs` framing failures remain.
- `cargo test -p iris-core --features proptest security::`: _(running — appended)_
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged).

### Tier 1 gate

**14 ✅ / 3 🔒 — Tier 1 complete** (§5: all findings are ✅/🔒 with documented reasons).
Advancing to **Tier 2**. The operator lifted the §2.3 gate (Run 2). Per-finding §8 file-scope
check still applies: PRY-32/33/34 and PRY-6/9/10 (call sites) will likely block on
`message_engine/` / `emergency/`; PRY-1/2/3/4/11/17 need per-finding assessment.

---

## Run 4 — 2026-08-31 — Tier 2 (batch 1)

**Branch:** `main`. Baseline `62b60eb` (Tier 1 complete). `cargo 1.97.1`. Operator lifted the
§2.3 gate (Run 2). This wake: 3 Tier-2 findings, all **fully Section-1 file-scoped** (no
`message_engine/` / `emergency/` edits). Each got build + targeted tests + touched proptests.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-17 | ✅ Fixed · Tier 2 · **WIDENS** | `b062f92` | `chain.rs` rule 5 no longer compares `key_gen_counter` across adjacent (different-identity) elements — it now rejects only an element naming an **older** generation than `TrustStore::recorded_counter()` has already seen for that same identity (real stale-chain replay). New `TrustStore::recorded_counter`. Rewrote `stale_counter_link_rejected` (now: store sees child@5, replayed chain carries child@3 → `StaleCounter(1)`); new `multilevel_chain_with_low_child_counter_passes` (root@2 certifies fresh child@0 — the exact case the old rule broke). Widens (multi-level org chains newly validate) — operator sign-off. EMERG-001 should still confirm intended chain semantics. |
| 2 | PRY-1 | ✅ Fixed · Tier 2 · narrows | `ccee0fc` | `check_authority_chain`: an **empty/missing allowlist on `Broadcast`/`Medical`** now returns `InvalidAuthority`, not `Authorized`. Reaching the ACL at all means the policy is armed (`security/mod.rs::check_emergency_acl` short-circuits un-armed + Noop). `Drill` stays permissive-on-empty (PRY-9: `KeyRotation`→`Drill` mis-map would otherwise drop all routine rotations). Rewrote `acl_noop_default_authorized` (unit + proptest — they encoded the vuln: "verifies the current behavior"); new `armed_empty_allowlist_denies_broadcast_and_medical`. `full_policy_armed_routes_to_engines` / `security::mod` assertion updated (unsigned P4→SOS now `InvalidAuthority`). |
| 3 | PRY-10 | ✅ Fixed · Tier 2 · narrows | `ccee0fc` | `check_sos_identity` now calls `envelope_signature_valid()` (Ed25519 `verify_strict` over `encode_for_signing`, key = the self-authenticating 32-byte `sender_id`) **before** the trust-level match — a missing/wrong/invalid signature → `InvalidAuthority`. Option (a) from the finding (option (b) needs a Section-2 `sender_authenticated` bool). SOS test envelopes now signed via new `sign_env` helper; new `acl_sos_spoofed_verified_sender_without_signature_rejected` (spoofed `sender_id` + no sig / attacker sig → rejected). |

### Not done this wake (remaining Tier 2)

| Finding | Why |
|---|---|
| PRY-11 | **Not a code-gate call** — "authorize any signed SOS regardless of `TrustLevel`" is a life-safety product decision against `SAFETY_CHARTER.md` / `EMERGENCY_GOVERNANCE.md`. The operator lifted the *procedural* gate but cannot stand in for the safety / EMERG-001 owner on who may originate an SOS. 🔒 needs safety-owner sign-off. |
| PRY-9 | `acl.rs` half (drop `ContentType::KeyRotation → AlertClass::Drill`) is doable, but the clean fix removes `KeyRotation` from the engine's `check_emergency_acl` routing set (`message_engine/mod.rs`, §8). Partial-only here would leave `Drill` unreachable-by-content and is risky without the engine half. Deferred to a wake that can coordinate the Section 2 edit. |
| PRY-2 / PRY-3 / PRY-4 | `quota.rs` / `reputation.rs` portions are Section-1, but each needs a Section-3 (`iris-storage`) or Section-2 (GC cadence) decision on ownership/semantics to fix correctly rather than guess. Next wake — read fully + draft per-finding plan. |
| PRY-32 | `acl.rs` half (enforce authority-meta: geo scope / severity cap / drill discipline / validity window) requires decoding the emergency payload — the authoritative path is `emergency/authority.rs::verify_authoritative` (§8). Needs EMERG-001 on which path is canonical. |
| PRY-6 / PRY-33 / PRY-34 | Fix lands in `message_engine/mod.rs` — §8 hard file boundary. 🔒 Section 2. |
| PRY-7 (tail) | 🔒 Section 2 (`message_id`). |

### Batch closeout

- `cargo build -p iris-core`: clean (pre-existing warnings only).
- `cargo test -p iris-core --lib`: **755 pass, 0 fail** (752 + 3 net new regression tests).
- `cargo test -p iris-core --features proptest --lib -- security::replay security::acl identity::chain`: **54 pass, 0 fail**.
- `cargo test -p iris-core --test sysval_security_flood --test crypto_e2e`: pass.
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged); my hunks kept consistent.

### Tier 2 status

**3 ✅ / 9 ⬜ / 1 🔒 (PRY-7 tail).** Of the 9: PRY-11 is really 🔒 (safety owner); PRY-6/33/34
are 🔒 (§8); PRY-2/3/4/9/32 need a per-finding cross-section decision before they can be
fixed right. Next wake: draft those plans; do not guess another owner's intent.

---

## Run 5 — 2026-08-31 — Tier 1 tail + Tier 2 (batch 2)

**Branch:** `main`. Baseline `a91fd8b` (pulled from remote after operator pushed Run 4 + scope
docs). Commit: `3d269d0`. `cargo 1.97.1`. All §8 boundaries waived (Operator auth & scope).

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-29 | ✅ Fixed · Tier 1 (§8 waived) | `3d269d0` | `RootRequirement` enum added to `identity/chain.rs`; both callers (`acl.rs`, `emergency/authority.rs`) pass `AuthorityRoot`; redundant `is_authority_root` post-checks removed. `ChainError::NotAuthorityRoot` → `AuthorityError::UntrustedAuthorityRoot` mapping preserves existing test assertions. All existing `verify_chain` call sites updated (`replace_all`). New regression test `authority_root_requirement_rejects_tofu_root` (TOFU-only root fails `AuthorityRoot`; after `register_authority_root` passes). 16 `identity::chain` tests green. |
| 2 | PRY-9 | ✅ Fixed · Tier 2 | `3d269d0` | Removed `ContentType::KeyRotation => AlertClass::Drill` arm from `acl.rs::check`. Removed `ContentType::KeyRotation` from `message_engine/mod.rs` emergency-ACL routing set (`EmergencyAlert \| Sos` only). `KeyRotation` envelopes now skip the emergency ACL entirely and fall through to the priority-class dispatch. `armed_empty_allowlist_denies_broadcast_and_medical` and proptest `acl_armed_empty_allowlist_class_decision` updated. 14 `security::acl` tests green. |
| 3 | PRY-33 | ✅ Fixed · Tier 2 | `3d269d0` | `seal_outbound` absent-key `else` branch now returns `Err(MsgEngineError::Crypto(CryptoError::KeyUnavailable))`. `Some(rkey)` branch gets explicit `return self.crypto.sign(envelope).await...`. P0 broadcast early-return unaffected. Build fix: type-mismatch on `()` resolved by adding the explicit return inside `Some`. Tests: GNU toolchain (`stable-x86_64-pc-windows-gnu`) — MSVC link.exe systemic failure on this machine. 75 `identity`, 14 `security::acl`, 16 `identity::chain` lib tests green. |

### Batch closeout

- `cargo +stable-x86_64-pc-windows-gnu test -p iris-core --lib -- identity security`: green (75 + 14 + 16 + message engine compile).
- MSVC toolchain has a systemic `link.exe "extra operand"` failure on build scripts (serde, thiserror, getrandom, proc-macro2 …) — pre-existing, not caused by these changes.
- 4 files changed, 138 insertions, 65 deletions.
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged).

### Tier 2 status after Run 5

**6 ✅ / 5 ⬜ / 2 🔒 (PRY-7 tail, PRY-11).** Remaining ⬜: PRY-2, PRY-3, PRY-4, PRY-6, PRY-32, PRY-34. Unblocked Tier-1 tail: PRY-30. Next wake: PRY-30 + PRY-2/3/4 quota/reputation/eviction (cross-section calls delegated to agent; draft per-finding plans).

---

## Run 6 — 2026-08-31 — Tier 1 tail + Tier 2 (batch 3)

**Branch:** `main`. Baseline `91644f8` (Run 5 docs pushed). Commit `780f242`. `cargo 1.97.1`
(GNU toolchain). All §8 file boundaries waived (Operator auth & scope). Budget: 6 pts
(Tier 2 = 2 pts, Tier 1 = 1 pt) → PRY-3 (2) + PRY-30 (1) + PRY-34 (2) = 5 pts.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-3 | ✅ Fixed · Tier 2 (operator-delegated eng. call) | `780f242` | Deleted `select_eviction_candidates` (no production caller, priority-blind, 1-byte amounts); deleted 2 associated tests; updated quota module doc to "admission only — eviction owned by Section 3 (`iris-storage` / `routing/scf_eviction.rs`)." |
| 2 | PRY-30 | ✅ Fixed · Tier 1 (§8 waived) | `780f242` | Deleted `evict_amount` (its only caller was deleted in PRY-3); deleted Kani proof `quota_eviction_bounded_and_positive` and its import from `kani_proofs.rs`; updated Kani module doc scope comment. |
| 3 | PRY-34 | ✅ Fixed · Tier 2 | `780f242` | Added `policy.add_message(relay_sender, relay_size, sched_priority).await` to `enqueue_relay` after the rate-limit gate. `Rejected` → `InboundOutcome::RateLimited` + `MESSAGES_QUOTA_EXCEEDED_TOTAL` metric increment + debug log. Updated stale relay-path comment. |
| + | PRY-33 test fallout | — | `780f242` | Discovered during Run 6 testing. Two sub-issues: (1) 3 message-engine tests + 1 routing test called `send_message(envelope_for(BOB,...))` with no key directory — PRY-33's fail-closed fix now returns `KeyUnavailable`. Fix: added `MemoryKeyDirectory` with fake `BOB=[0xBB;32]` key to all affected test engines. (2) `seal_outbound` pre-set `payload_size = sealed_len = n+16` but `DevCryptoProvider.encrypt()` returns plaintext unchanged (n bytes, no AEAD tag) — `codec::encode` rejected the envelope with `PayloadSizeMismatch`. Fix: set `envelope.payload_size = ciphertext.len() as u64` after `encrypt()` returns (no-op for real AEAD where `sealed_len == ciphertext.len()`). |

### Batch closeout

- `cargo +stable-x86_64-pc-windows-gnu test -p iris-core --lib`: **754 lib tests pass, 0 fail** (finished in 21.09s).
- Only the 2 pre-existing `protocol_conformance.rs` framing failures remain (baseline, out of scope).
- 4 files changed, 58 insertions(+), 126 deletions(-).
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged).

### Tier 2 status after Run 6

**7 ✅ / 4 ⬜ / 2 🔒 (PRY-7 tail, PRY-11).** Remaining ⬜: PRY-2, PRY-4, PRY-6, PRY-32.
Tier 1 tail: PRY-30 → ✅ (was 🔒). PRY-13 remains 🔒 (concurrency/lock refactor).
Overall: 29 ✅, 4 ⬜, 3 🔒. Not pushed — waiting on explicit go-signal.

---

## Operator authorization & scope note — 2026-08-31 (docs-only, no code)

Recorded in `priyam_problems.md` → *Operator authorization & scope*. Summary:

- **All §8 file boundaries waived.** The fix agent may now edit `message_engine/`,
  `emergency/`, `kani_proofs.rs`, `Cargo.toml`, etc. Widen-accept gate stays lifted.
  Cross-section ownership calls (PRY-2/3/4/9/32) delegated to the fix agent on engineering
  merit — operator is sole owner.
- **SOS service + LoRa transport = FUTURE update, out of scope this cycle.** In scope:
  Wi-Fi Direct, Wi-Fi Aware, Bluetooth, Internet.
  - **PRY-11** → 🔒 Deferred (product scope). Safety-charter SOS-authorization call deferred
    with it.
  - **PRY-10** stays ✅ (defensive; guarded path dormant until SOS ships).
  - **PRY-9 / PRY-32**: fix the emergency-**broadcast** (P0/P1 authority) side; leave
    user-**SOS**-only branches marked future.
- **Tier 1 PRY-29 / PRY-30**: the §8 block is lifted — cleared for the next session.
- **Work has NOT started.** Next fix session begins only on the operator's explicit
  go-signal. Roll-up now: Tier 2 = 3 ✅ / 8 ⬜ / 2 🔒 (PRY-7 tail, PRY-11).

---

## Run 7 — 2026-08-31 — Tier 2 (batch 4) — **Tier 2 complete**

**Branch:** `main`. Baseline `91f6ca4` (Run 6 pushed). `cargo 1.97.1` MSVC (the Run 5/6
`link.exe` failure did not recur — MSVC builds cleanly this session). All §8 boundaries
waived. Operator: "complete the whole tier." 4 findings + the PRY-7 tail.

### Per-finding results

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | PRY-2 | ✅ Fixed · Tier 2 | `6a9ef0b` | `QuotaManager` gets `priority_bytes: AtomicU64` + per-account `priority_used_bytes`. `check`/`add_message` P0/P1 branch: `PriorityExempt` only while `priority_bytes + size <= priority_reserved_pool_bytes` AND `total_bytes + size <= total_quota_bytes`; else `Rejected`. Global ceiling now also applies to ordinary classes. `remove_message` attributes freed bytes to the pool first. **Tests rewritten (§8):** `huge_message_size_no_overflow` (asserted `total_usage()==u64::MAX` after a P0 add — the vuln) and proptest `quota_p0_p1_priority_exempt` (unbounded P0/P1). New `quota_p0_rejected_when_reserved_pool_full`, `total_quota_ceiling_enforced_for_all_classes`, `quota_p0_p1_exempt_within_reserved_pool` (proptest). |
| 2 | PRY-4 | ✅ Fixed · Tier 2 | `ed43068` | `routing_weight` → `.max(SELECTION_FLOOR)` (0.05) on the returned selection weight (internal score still floors at `min_score`). New `SecurityPolicy::decay_reputation` (Noop no-op); `FullSecurityPolicy` self-throttles to `reputation.decay_interval()` via a `Mutex<Instant>`; engine `spawn_gc_task` ticks it every sweep. **Tests rewritten (§8):** `reputation_negative_floored_at_zero` (unit + proptest), `reputation_not_a_gate` — all asserted `routing_weight == 0.0`. New `floored_peer_recovers_after_decay_intervals`, `routing_weight_never_returns_exactly_zero`. |
| 3 | PRY-6 | ✅ Fixed (subsumed) · Tier 2 | `42eabb5` | Re-audit: `process_incoming` runs `self.crypto.verify(&envelope)` at mod.rs:545 **before** `check_replay` at mod.rs:596. The only `check_replay` caller is that one site. So the high-water is only ever advanced for a signature-verified `sender_id` — the "advance on attacker-spoofed sender_id + garbage sig" exploit is structurally closed (engine drift since the Run 0 audit, which cited an older line layout). The unclamped-`seq` half is subsumed by PRY-7. |
| 4 | PRY-7 (tail) | ✅ Fixed · Tier 2 · **WIDENS** | `42eabb5` | `HighwaterTable::admit`: timestamp-primary acceptance. Advance `HighWaterMark` across seconds; within the current second dedup `sequence_hint` against a bounded exact seen-set (`recent_seqs`, cap `MAX_SEEN_SEQ_PER_SECOND = 64`) instead of strict `seq` ordering — an honest same-millisecond burst is no longer ~50 % false-`Replay`. `check_and_advance` kept for the `load_snapshot` merge only. **Tests rewritten (§8):** `replay_highwater_replay_detection`, `replay_out_of_order_accepted`, `replay_future_poison_is_bounded_not_persistent`, `replay_cross_reboot_persistence`, `load_snapshot_never_regresses_a_live_mark`, proptest `replay_cross_reboot_restores_highwater` — all encoded strict same-second ordering. New `same_second_descending_seq_burst_all_accepted`, `same_second_seen_set_is_bounded`. Cross-second replay protection fully preserved; a same-second replay in the ≤1 s window right after a reboot is accepted by design (option 2's stated tradeoff). |
| 5 | PRY-32 | ✅ Fixed · Tier 2 | `b1d3a61` | `check_authority_chain` for Broadcast/Medical decodes the `EmergencyBroadcast` payload and runs `emergency::authority::verify_authoritative` — geo scope, functional scope, `max_severity` cap, drill discipline, validity window. Closes the "SEC-001 armed, EMERG-001 provider not armed" gap (the engine's `emergency_gate` runs the same check but only when the EMERG-001 provider is armed — a separate flag). Non-emergency payloads keep the anchor-only result (fail closed on the engine path when armed). Existing acl tests use empty payloads → unaffected. New `acl_rejects_out_of_geo_scope_broadcast` (geo mismatch + over-cap severity). |

PRY-11 → 🔒 Deferred (product scope — SOS service is a future update; the safety-charter
authorization decision defers with it).

### Batch closeout

- `cargo build -p iris-core --all-targets`: clean (pre-existing warnings only).
- `cargo test -p iris-core`: **761 lib tests pass** + all integration; only the 2 pre-existing
  `protocol_conformance.rs` framing failures remain (baseline, out of Section 1 scope).
- `cargo test -p iris-core --features proptest --lib -- security::`: **119 passed, 0 failed** (200s).
- `cargo test -p iris-core --test sysval_security_flood --test crypto_e2e`: pass.
- `cargo fmt --check`: crate-wide pre-existing failure (unchanged); my hunks kept consistent.

### Final state — 34 of 36 findings resolved

- **Tier 0:** 6 ✅ + 1 ⚪ (PRY-28 premise disproved).
- **Tier 1:** 16 ✅ + 1 🔒 (**PRY-13** — concurrency/lock refactor, needs benchmarking + loom; disproportionate to a hygiene pass).
- **Tier 2:** 11 ✅ + 1 🔒 (**PRY-11** — SOS service is a future update, product-scope deferral).
- **Total: 33 ✅ · 2 🔒 · 1 ⚪.**

No further autonomous work: both 🔒 items need either a dedicated performance effort (PRY-13)
or the SOS service to ship (PRY-11).

---

## Run 8 — 2026-08-31 — PRY-13 (engine lock sharding)

**Branch:** `main`. Baseline `e7d28e4` (Run 7 pushed). `cargo 1.97.1` MSVC. Operator: "do
PRY-13 properly." This is the last non-SOS finding.

### What landed

New `crates/iris-core/src/security/sharded.rs` — `ShardedLocks<S>`: `SHARD_COUNT = 64`
independent `tokio::sync::RwLock<S>` shards, addressed by `DefaultHasher` of the sender key.
Deterministic within a process (fixed hasher keys). `per_shard_cap(total)` divides a
configured count cap across shards (rounding up so the aggregate is never materially below
the configured value). 3 unit tests: shard spread, key stability, 32-task concurrent-writer
no-lost-updates.

All four engines converted from one `RwLock<map>` to `ShardedLocks`:

| Engine | Field | Cap → per-shard | Whole-map ops |
|---|---|---|---|
| `rate_limiter` | `buckets: ShardedLocks<BucketState>` | `max_sender_buckets` | `reset` iterates shards |
| `quota` | `accounts: ShardedLocks<HashMap<…>>` | (no count cap; byte totals stay in atomics) | `reset` iterates shards |
| `replay` | `highwater: ShardedLocks<HighwaterTable>` | `max_sender_highwater` | `generate_snapshot` / `from_snapshot` / `load_snapshot` / `reset` iterate/partition shards |
| `spam` | `stats: ShardedLocks<HashMap<…>>` | `max_sender_stats` | `reset` iterates shards |

`replay` also: **PRY-13's "at minimum"** — `maybe_schedule_snapshot` no longer clones the map
on the accept path. The hot path sets a `snapshot_dirty: AtomicBool`; `generate_snapshot`
(the all-shard clone) runs off the accept path and clears the flag. New `snapshot_pending()`
accessor for a persistence driver. `maybe_schedule_snapshot` now only advances the persist
clock + bumps the `persisted` metric.

New `crates/iris-core/benches/security_contention.rs` (registered in `Cargo.toml`): N=64
tasks × 200 checks each through `FullSecurityPolicy` (`rate_limit` → `check_quota` →
`check_replay` → `score_spam`), two shapes — **distinct senders** (spread across shards) vs
**one hot sender** (all on one shard). On this box: distinct ≈ 1.3 Melem/s, hot ≈ 0.42
Melem/s — ~3× when load spreads. Standard per-shard `RwLock` (no custom lock-free atomics),
so **no `loom` model is required**; the `ShardedLocks` concurrency stress test covers the
lock discipline.

### Behaviour change (documented)

Count caps are now **per-shard**. A sender population spread across shards still hits the
same aggregate bound; a flood ground onto one shard (an attacker grinding sender-id hashes)
hits its 1/64 slice sooner — stronger isolation, not weaker. Small-cap eviction-ordering
tests (`spam_cap_evicts_least_established_sender`, `spam_sender_stats_capped_at_max`)
rewritten to force senders into one shard via a `colliding_senders` test helper — the
eviction *ordering* logic (least-established / stale-first / newest-out) is still exercised,
within a shard.

### Batch closeout

- `cargo build -p iris-core --all-targets`: clean.
- `cargo test -p iris-core --lib`: **764 pass** (761 + 3 `sharded` unit tests), 0 fail.
- `cargo test -p iris-core --lib -- security:: message_engine::`: 157 pass.
- `cargo test -p iris-core --features proptest --lib -- security::`: _(running — appended)_
- `cargo bench -p iris-core --bench security_contention`: builds + runs; ~3× spread/hot ratio.
- Only the 2 pre-existing `protocol_conformance.rs` framing failures remain.

### Final state — 35 of 36 findings resolved

- Tier 0: 6 ✅ + 1 ⚪ (PRY-28)
- Tier 1: **17 ✅** (PRY-13 done)
- Tier 2: 11 ✅ + 1 🔒 (**PRY-11** — SOS product-scope deferral)
- **Total: 34 ✅ · 1 🔒 · 1 ⚪.**

The only open item is PRY-11, which cannot be done until the SOS service ships.
