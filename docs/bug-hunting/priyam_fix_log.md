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

## Run 1 — (not yet executed)

_Target: Tier 0 (7) — PRY-19 panic guard → PRY-22 API default → PRY-28 deque compaction →
PRY-31 small-order constants (replace with vetted values + cross-check test) → PRY-14 refill
arithmetic → PRY-5 DH small-order check → PRY-7 replay sequence semantics. PRY-5 and PRY-31
should land together (both touch the small-order guard)._

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| — | — | — | — | _pending first wake_ |
