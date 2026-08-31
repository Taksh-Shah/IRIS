# Section 1 — Crypto, Identity & Security: Deep Bug-Hunt Report

**Owner:** Priyam
**Section:** 1 — Crypto, Identity & Security (`docs/testing/CODE_REVIEW_SECTIONS.md`)
**Date:** 2026-08-31
**Commit reviewed:** `23ad303` (branch `main`, clean tree)
**Toolchain:** cargo 1.97.1 · Windows 11
**Revision:** 2 — second deeper pass added PRY-31…PRY-36 (see §9 for what the second pass covered)

---

## 1. What this document is

A defect inventory for **Section 1 only** — the cryptographic primitives, the key/identity
lifecycle (provisioning, advertisement, chains, rotation, revocation, trust store), and the
anti-abuse pipeline (rate limiter, quota, replay, reputation, spam, emergency ACL). Roughly
8,800 lines across `crates/iris-core/src/{crypto,identity,security}/`, plus the crypto bench,
`crypto_e2e.rs`, and the `security/` proptest regressions.

`crates/iris-core/src/message.rs` (Section 2) and `crates/iris-core/src/error.rs` (Section 3)
were read as **read-only reference**. `crates/iris-core/src/kani_proofs.rs` and
`crates/iris-core/src/message_engine/*` were read as read-only reference (Section 2) and appear
here only where Section 1 code creates a defect on **our** side of the boundary, or where a
proof no longer covers the code it claims to.

Every finding carries: what the problem is, why it is a problem, exactly where it lives (file
and line), why it came to exist, what it causes in production, how to fix it, and what else
must change or be re-verified alongside it.

## Headline — read this first

Three results dominate this report.

**First: the emergency ACL is effectively inert for broadcast/medical traffic in the standard
armed configuration.** `FullSecurityPolicy::new()` builds `EmergencyAcl::default(...)`, whose
`AclConfig.allowlists` is an empty `HashMap`. `check_authority_chain` treats an empty allowlist
as **`Authorized`** ("Noop mode: empty allowlist = permissive"). `add_authority` takes
`&mut self`, but the ACL is stored behind `Arc`, so there is **no way to populate the allowlist
after construction**. An armed policy therefore authorizes every P0 broadcast, P1 medical
alert, and drill message it sees — the exact opposite of AC-11 / DEC-SEC-0006 ("P0/broadcast
requires a verified authority role chain"). See PRY-1.

**Second: the storage-quota invariants that protect the node against exhaustion are
unimplemented.** `priority_reserved_pool_bytes` and `total_quota_bytes` are never read anywhere
in the crate. P0/P1 messages are accounted with *no cap at all* (`PriorityExempt` returns
before any check). `select_eviction_candidates` — the only eviction path — has **no production
caller**, ignores message priority (so it can evict P0/P1, violating AC-2), and emits
meaningless one-byte eviction amounts. "Bounded by app quota (default 500 MB)"
(`NODE_MODEL.md`) is currently false for the security layer. See PRY-2, PRY-3.

**Third: replay protection runs against unauthenticated data with a non-monotonic sequence.**
`check_replay` is called with `sender_id` bytes straight off the wire, before signature
verification, and with `seq = message_id.sequence_hint()` — the first 8 bytes of the message
id, i.e. a timestamp-derived value, **not** the per-sender monotonic counter that `replay.rs`
and AC-4 assume. This lets an on-path attacker advance/poison a victim's high-water mark with a
bad-signature packet, and it makes two legitimate messages minted in the same millisecond
race, the loser rejected as `Replay`. The per-sender high-water table (10 000 entries) is also
not adversary-resistant: a free Sybil flood displaces genuine marks for up to 24 h. See PRY-6,
PRY-7, PRY-8.

These interact: the parts of Section 1 that *are* reachable today (the crypto primitives, the
trust store, the advertisement/chain/rotation state machine) are largely sound; the
anti-abuse pipeline is where the defects cluster, and most of it activates in one future
wiring PR (`FullSecurityPolicy` into the platform engine constructors, `reputation` into
routing weight, `quota` eviction into the store GC) — the commit at which nobody will be
re-reading this code.

**Second pass (revision 2) added six findings**, two of which are High/Medium and belong with
the headline: **PRY-31** — the RED-0011 small-order blocklist `SMALL_ORDER_U` has a *corrupted*
constant (`[2]` shows an internal byte-repeat and does not match the little-endian encoding of
the value in its own doc comment) and is *incomplete* (missing the non-canonical `p−1/p/p+1`
encodings of 0 and 1), with no test cross-checking the constants — the explicit fast-path list
provides no reliable protection and the only real guard is the `to_edwards().is_small_order()`
fallback, whose coverage of non-canonical field encodings is untested. **PRY-33** — the engine's
`seal_outbound` sends an addressed message **as plaintext** when the recipient's X25519 key is
absent from the directory (logs `MSG_SENT_UNENCRYPTED`, proceeds); `key_directory::require_key`,
the fail-closed helper, has zero non-test callers. Confidentiality fails open.

## Progress Tracker — living section, updated as fixes land

**This section is rewritten by the fix loop after every finding and every tier.** Individual
findings carry their own `Fix status` line (just under **Severity**); this table is the
roll-up. The loop logic, tier rules, per-finding protocol and safety gates are specified in
[`priyam_problems_loop.md`](priyam_problems_loop.md). The per-run execution journal is
[`priyam_fix_log.md`](priyam_fix_log.md).

**Last updated:** 2026-08-31 — Run 8: fixed **PRY-13** — all four security engines
(`rate_limiter`/`quota`/`replay`/`spam`) sharded into 64 independent `RwLock` shards
(`security/sharded.rs`); `replay` snapshot clone moved off the accept path; new
`benches/security_contention.rs` (~3× throughput, spread vs one hot sender). 764 lib tests +
security proptests green.
**Only PRY-11 remains** — 🔒 deferred by product scope (SOS is a future update).
**34 ✅ · 1 🔒 · 1 ⚪ of 36.**

---

### Operator authorization & scope (2026-08-31)

The operator (repo owner) has recorded the following. **No code has been changed on the
strength of this block — it is authorization for the *next* work session, which starts only
on the operator's explicit go-signal.**

**1. Full authorization — all §8 boundaries lifted.**
- The loop's file-ownership boundary (`priyam_problems_loop.md` §8 — "never modify files
  outside `crates/iris-core/src/{crypto,identity,security}/`") is **waived**. The fix agent
  may edit `message_engine/`, `emergency/`, `message.rs`, `error.rs`, `kani_proofs.rs`,
  `Cargo.toml`, and any other file a Tier-2 fix genuinely requires.
- The widen-an-accept-path gate (§1 second invariant) remains **lifted** (recorded Run 2).
- Cross-section "needs another owner's sign-off" items (PRY-2/3/4 storage & GC-cadence
  questions, PRY-9 engine routing, PRY-32 canonical authority path) are **delegated to the
  fix agent to decide on engineering merit** — the operator is the sole owner here.
- This unblocks, for the next session: **PRY-2, PRY-3, PRY-4, PRY-6, PRY-9, PRY-32, PRY-33,
  PRY-34**, and the **PRY-7 behavioural tail** (real per-sender counter / bounded seen-set in
  `message_id` + the inbound engine path).

**2. Product scope — SOS and LoRa are FUTURE, not now.**
- **In scope now:** Wi-Fi Direct, Wi-Fi Aware, Bluetooth, Internet transports only.
- **Deferred to a future update:** the **SOS service** (originating / relaying / ACL-gating
  user SOS messages) and the **LoRa** transport. Not being built in this cycle.
- Effect on this report:
  - **PRY-11** (armed node rejects unpaired peers' SOS) → **🔒 Deferred · product scope:
    SOS service is a future update.** Revisit when SOS ships; the safety-charter decision
    ("any correctly-signed SOS is authorized regardless of trust tier") is deferred with it.
  - **PRY-10** (SOS ACL signature check) — **already fixed** (`ccee0fc`); kept as defensive
    hardening, but the SOS delivery path it guards is dormant until the SOS service ships.
  - **PRY-9 / PRY-32** — the emergency-**broadcast** (P0/P1 authority) path is still in
    scope; only the user-**SOS** path is deferred. Fix the broadcast side; leave SOS-only
    branches marked future.
  - No LoRa-specific findings exist in this report (Section 1 is transport-agnostic).

### Status legend
⬜ Not started · 🔵 In progress · ✅ Fixed & tested · 🟢 Fixed & verified · 🔒 Blocked (reason recorded) · ❌ Attempted, reverted (reason recorded) · ⚪ Not applicable

### Tier roll-up

| Tier | Name | Findings | ⬜ | 🔵 | ✅/🟢 | 🔒 | ❌ | Gate to enter |
|---|---|---|---|---|---|---|---|---|
| **0** | Invariant-correct, no cross-section dependency | 7 | 0 | 0 | 6 | 0 | 0 | none — start here |
| **1** | Hardening & hygiene (bounded, self-contained) | 17 | 0 | 0 | 17 | 0 | 0 | **complete** — PRY-13 sharding landed |
| **2** | changes accept/deny security semantics — operator gate **LIFTED** + all §8 file boundaries **WAIVED** (see *Operator authorization & scope*) | 12 (+PRY-7 tail) | 0 | 0 | 11 | 1 | 0 | **Tier 2 complete** — 11 ✅, PRY-11 🔒 (SOS product-scope deferral). PRY-7 tail also ✅. |
| **Total** | | **36** | **0** | **0** | **34** | **1** | **0** | |

**Only 1 finding remains unfixed:**
- **PRY-11** (Tier 2) — deferred by **product scope**: the SOS service is a future update
  (see *Operator authorization & scope*). The safety-charter call it hinges on defers with it.

PRY-13 landed in Run 8 (per-shard `RwLock` × 64 in all four engines + a `benches/`
concurrency benchmark). PRY-29 & PRY-30 were fixed in Run 5 / Run 6 once the §8 boundary
was waived.

Tier 0 note: PRY-28 is ⚪ **Not applicable** (not counted above as ⬜/✅ — premise disproved, see its entry). PRY-7's Tier-0-scoped part (honest docs + a pinning regression test) is ✅ commit `bdac72b`; its behavioural half **widens an accept path** and is now a 🔒 Tier 2 item (Section 2 `message_id` owner). So Tier 0 is materially complete: 6 ✅ + 1 ⚪.

### Authoritative tier membership

- **Tier 0** (7): PRY-5, PRY-7, PRY-14, PRY-19, PRY-22, PRY-28, PRY-31
- **Tier 1** (17): PRY-8, PRY-12, PRY-13, PRY-15, PRY-16, PRY-18, PRY-20, PRY-21, PRY-23, PRY-24, PRY-25, PRY-26, PRY-27, PRY-29, PRY-30, PRY-35, PRY-36
- **Tier 2** (12 — GATED): PRY-1, PRY-2, PRY-3, PRY-4, PRY-6, PRY-9, PRY-10, PRY-11, PRY-17, PRY-32, PRY-33, PRY-34

### What "done" means, per tier

- **Tier 0 / Tier 1:** ✅ means the fix compiled (`cargo build -p iris-core`), the targeted
  test(s) passed, and any regression test named in the finding's *Fix* field was added.
  Fully verifiable in-repo.
- **Tier 2:** no ✅ without (a) the human sign-off recorded in `priyam_fix_log.md`, and
  (b) for findings whose `Dependencies` name Section 2, a recorded agreement from that
  owner — a wrong fix here is worse than the bug.

## 2. Severity scale

- **Critical** — a security control that does not hold, silent loss of P0/SOS traffic, remote
  DoS, or a crash on a normal path.
- **High** — message loss, unbounded resource growth, a documented security requirement that is
  unimplemented, or a permanent self-inflicted partition.
- **Medium** — degraded delivery, wrong-but-recoverable behaviour, a bypassable check that a
  second layer currently covers, or unwired logic that will fire once connected.
- **Low** — diagnosability, hygiene, defense-in-depth gaps, latent defects in code with no
  live caller.

## 3. How the review was carried out

Every file in `crypto/`, `identity/`, `security/` was read line by line and diffed against the
governing design and security docs (`docs/implementation/{CRYPTO,IDENT,SEC_001}_{DESIGN,VERIFICATION}.md`,
`docs/identity/*.md`, `docs/security/*.md`, `docs/safety/ABUSE_PREVENTION.md`, `docs/architecture/FAILURE_ARCHITECTURE.md`,
`docs/emergency/*`). A spec that says X where the code does Y is treated as a first-class
defect. The Kani proofs (`kani_proofs.rs`) and the property-test suites were read to establish
what is *actually* covered. Every "this is never called" claim was proved with a repo-wide
grep that included `crates/iris-android`, `crates/iris-ios`, `crates/iris-desktop`. Arithmetic
claims (`refill_tokens`, `evict_amount`, `sequence_hint`) were checked by hand-evaluation.
Every finding was then re-read and re-checked against current source before inclusion.

## 4. How to read a finding

| Field | Meaning |
|---|---|
| **Severity** | Consequence if it fires in production, independent of likelihood. |
| **Confidence** | `Certain` = the cited line is unambiguous. `High` = defect is clear, exact trigger may vary. `Medium` = mechanism is real, reachability not fully established. |
| **Location** | `file.rs:line` — line numbers are against commit `23ad303`. |
| **Root cause** | Why the code came to be this way (several defects here are the same mistake repeated). |
| **Dependencies / blast radius** | What else must change or be re-verified; cross-section dependencies are called out explicitly. |

### A note on "unwired" findings

A recurring category is code that is **defined, looks load-bearing, and is never called**:
`QuotaManager::select_eviction_candidates`, `ReputationEngine::decay`, the
`priority_reserved_pool_bytes` / `total_quota_bytes` config, `EmergencyAcl::add_authority`.
Each fires the moment it is wired up, in the commit where nobody is re-reading it. They are
reported as defects, cheapest to fix now while inert. Severity reflects the latency and the
entry says so.

---

## 5. Master index

All 36 findings, in report order. Severities are as assessed. PRY-31…PRY-36 were added by the
second pass (revision 2).

| ID | Sev | File | Finding |
|---|---|---|---|
| **PRY-1** | High | `security/acl.rs:159` | Armed `FullSecurityPolicy` authorizes all P0/P1/drill emergency traffic — empty allowlist = permissive, and no way to populate it |
| **PRY-2** | High | `security/quota.rs:147` | P0/P1 storage accounting is unbounded; `priority_reserved_pool_bytes` / `total_quota_bytes` never read |
| **PRY-3** | High | `security/quota.rs:225` | `select_eviction_candidates` has no caller, ignores priority (can evict P0/P1), emits 1-byte amounts, never consults total quota |
| **PRY-4** | High | `security/reputation.rs:254` | `decay()` has no production caller — a peer floored to 0.0 never recovers → permanent routing blackhole |
| **PRY-5** | High | `crypto/x25519.rs:18` | `diffie_hellman` never calls `is_small_order` on the peer key — RED-0011 not applied on the agreement path itself |
| **PRY-6** | Medium | `security/replay.rs:351` | replay high-water keyed by unauthenticated `sender_id`, advanced before signature verification; `seq` attacker-chosen |
| **PRY-7** | Medium | `protocol/message_id.rs:47` (used at `message_engine/mod.rs:598`) | `sequence_hint()` is timestamp-derived, not a per-sender monotonic counter — same-ms legit messages falsely `Replay` |
| **PRY-8** | Medium | `security/replay.rs:406` | high-water table not adversary-resistant — free Sybil flood displaces genuine marks for up to 24 h |
| **PRY-9** | Medium | `security/acl.rs:121` | `ContentType::KeyRotation` mapped to `AlertClass::Drill` — a populated Drill allowlist drops every peer's key rotation |
| **PRY-10** | Medium | `security/acl.rs:242` | SOS ACL trusts raw 32-byte `sender_id`, performs no signature check itself |
| **PRY-11** | Medium | `security/acl.rs:242` | armed node with only provisioned roots: every unpaired peer's SOS is `Unauthorized` — safety regression |
| **PRY-12** | Medium | `security/acl.rs:283` | `add_authority`/`reset` unreachable via `Arc`; `sos_rate_limit_per_hour`, `SosRateLimited`, "certified responders" all dead |
| **PRY-13** | Medium | `security/{rate_limiter,quota,replay,spam}.rs` | every hot-path check takes a global write lock — one slow holder stalls all inbound |
| **PRY-14** | Medium | `security/rate_limiter.rs:306` | `refill_interval < 1 s` → `intervals` always 0 → bucket never refills → every sender permanently `SilentDrop` |
| **PRY-15** | Medium | `security/replay.rs:482` | `load_snapshot` replaces the whole map, no version/timestamp check — wipes live marks / rolls them backward |
| **PRY-16** | Medium | `security/spam.rs:149` | duplicate-content hash is over ciphertext → inert once E2EE is on; only compares the immediately previous message |
| **PRY-17** | Medium | `identity/chain.rs:138` | chain rule 5 requires each element's `key_gen_counter` > the previous element's — breaks legitimate multi-level chains |
| **PRY-18** | Medium | `identity/trust_store.rs:186` | a signed rotation advertisement swaps a `Verified` peer's key while leaving the level at `Verified` |
| **PRY-19** | Low | `security/reputation.rs:168` | `partial_cmp(...).unwrap()` panics on a NaN score; `ReputationConfig` is caller-constructible with arbitrary `f64` |
| **PRY-20** | Low | `security/reputation.rs:237` | `register_verified_peer` adds `+0.1` on every call — repeated calls pump a peer to `max_score` |
| **PRY-21** | Low | `security/spam.rs:126` | cap eviction uses `stats.keys().next()` — the non-adversary-resistant pattern already fixed in `replay.rs` |
| **PRY-22** | Low | `security/rate_limiter.rs:202` + `security/mod.rs:39` | `check` / default `rate_limit` hard-code `EmergencyClaim::Emergency` — restores the P0-starvation primitive |
| **PRY-23** | Low | `identity/provision.rs:90` | `from_bytes` reads seeds through non-zeroized `[u8;32]` stack temporaries |
| **PRY-24** | Low | `identity/store.rs:75` | Windows with `APPDATA` unset → key files written world-readable into CWD; version check only byte 0 |
| **PRY-25** | Low | `crypto/keygen.rs:12` | `X25519Keypair: Clone`, no explicit `Drop`/`ZeroizeOnDrop`; zeroization unpinned (no feature assertion) |
| **PRY-26** | Low | `identity/chain.rs:59` | `verify_chain` doc claims it registers the final identity *and* that it is stateless — it does neither registration |
| **PRY-27** | Low | `security/replay.rs:439` | `check_freshness_only` future window omits the skew budget that `check` applies — untrusted senders penalised for fast clocks |
| **PRY-28** | Low | `security/rate_limiter.rs:239` | FIFO `order` deque accumulates tombstones for every evicted key → unbounded growth under churn |
| **PRY-29** | Low | `identity/chain.rs:82` | `verify_chain` accepts a TOFU (`Unverified`) root; only the caller's separate `is_authority_root` check prevents EMERG-RT-001 |
| **PRY-30** | Low | `security/quota.rs:69` | `evict_amount` floors at 1 byte for a below-quota account — instructs the store to "evict 1 byte", never a whole message |
| **PRY-31** | Medium | `identity/small_order.rs:36` | `SMALL_ORDER_U[2]` is corrupted and the array is incomplete (no `p−1/p/p+1`); no test cross-checks the constants |
| **PRY-32** | Medium | `security/acl.rs:153` | the SEC-001 ACL emergency path enforces none of geo scope / severity cap / drill discipline / validity window that `emergency/authority.rs` does |
| **PRY-33** | Medium | `message_engine/mod.rs:353` (S2) + `crypto/key_directory.rs:43` | `seal_outbound` sends plaintext when the recipient key is absent; `require_key` (fail-closed) has no callers |
| **PRY-34** | Medium | `message_engine/mod.rs:995` (S2) | relay path runs rate-limit but never `quota.add_message` — relayed traffic bypasses per-sender storage quota |
| **PRY-35** | Low | `identity/chain.rs:159` | rule 6 can never match a 16-byte abbreviated `sender_id` → abbreviated emergency envelopes with a chain are unconditionally `SenderMismatch` |
| **PRY-36** | Low | `identity/trust_store.rs:246` | `revoke` / `un_revoke` / `adopt_rotation` / `register_authority_root` are `pub` and self-authenticate nothing — one misuse flips trust state |

---

## 6. Findings

---

### PRY-1 — Armed `FullSecurityPolicy` authorizes every emergency broadcast

- **Severity:** High
- **Fix status:** ✅ Fixed · Tier 2 · 2026-08-31 (operator gate lifted)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/acl.rs:159-164`, `:109-115`, `:85-96`; `crates/iris-core/src/security/mod.rs:162-179`

**What.** `FullSecurityPolicy::new()` constructs `EmergencyAcl::default(trust_store)`, i.e.
`AclConfig::default()` with `allowlists: HashMap::new()`. `check_authority_chain` begins:

```rust
let allowed = self.config.allowlists.get(&class);
if allowed.is_none() || allowed.unwrap().is_empty() {
    self.metrics.authorized.fetch_add(1, Ordering::Relaxed);
    return AclDecision::Authorized;
}
```

So for `AlertClass::Broadcast` (P0), `Medical` (P1) and `Drill`, an armed policy returns
`Authorized` unconditionally. The only way to add an entry is `add_authority(&mut self, ...)`,
but `FullSecurityPolicy` holds the ACL as `Arc<EmergencyAcl>` (`security/mod.rs:156`), and
`with_configs` also wraps in `Arc` — there is no `&mut` handle anywhere after construction.

**Evidence.** `grep -n 'add_authority' crates/iris-core/src` → definition + tests only, never a
non-test caller. `FullSecurityPolicy` exposes `acl(&self) -> &Arc<EmergencyAcl>` (shared ref,
cannot call `&mut` methods). `acl.rs` tests that exercise a populated allowlist all build the
`AclConfig` by hand and pass it to `EmergencyAcl::new(config, store)` *before* wrapping.

**Why it is wrong.** AC-11 / DEC-SEC-0006: "P0/broadcast requires verified authority role
chain … verify chain+sig before relay, silent-drop unverifiable." `ABUSE_PREVENTION.md`:
"Only nodes with valid authority certificates can originate emergency broadcast messages …
Cannot be bypassed." The armed policy is meant to be the enforcing configuration; instead it
is permissive for the single highest-impact class (a spoofed public-safety broadcast). The
"empty allowlist = Noop permissive (AC-12)" comment conflates *un-armed* (`NoopSecurityPolicy`)
with *armed but unconfigured* — those are different states and only the first should be
permissive.

**Root cause.** The Noop-compatibility shortcut ("un-armed engine behaviour identical") was
implemented as a data check (`allowlists.is_empty()`) rather than a mode check
(`self.armed`), and the allowlist was made mutable-only through a `&mut` method that the
chosen ownership model (`Arc`) then made unreachable.

**Fix.** (1) Give `EmergencyAcl` interior mutability for the allowlist
(`RwLock<HashMap<AlertClass, Vec<AuthorityShort>>>`) or accept the full config through
`SecurityPolicyConfig` (`security/mod.rs:362` already has an `acl` field — wire it through
`FullSecurityPolicy::new`). (2) In `check_authority_chain`, an **empty allowlist on an armed
policy must be `InvalidAuthority`, not `Authorized`** for `Broadcast`/`Medical` — an armed
node with no configured authorities cannot verify a broadcast, so it must drop it. Keep the
permissive branch only reachable through `NoopSecurityPolicy`. (3) Regression test:
`full_policy_armed_empty_allowlist_denies_broadcast`.

**Dependencies / blast radius.** **Section 2** owns `message_engine/mod.rs:664-714` which turns
`InvalidAuthority` into `InboundOutcome::EmergencyDropped` — confirm that dropping an
unverifiable broadcast on an armed-but-unconfigured node is the intended engine behaviour (it
should be, but it is a behaviour change from "delivered"). Provisioning code that arms the
policy must now also supply the NDMA authority allowlist or every broadcast is dropped —
coordinate with EMERG-001 owner.

---

### PRY-2 — P0/P1 storage accounting is unbounded

- **Severity:** High
- **Fix status:** ✅ Fixed · Tier 2 · commit `6a9ef0b` · 2026-08-31 — reserved-pool (`priority_reserved_pool_bytes`) + global ceiling (`total_quota_bytes`) now enforced for P0/P1 and all classes; a P0 that cannot be stored is `Rejected`. Admission side only — Section 3 owns physical eviction end-to-end.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/quota.rs:147-158` (`check`), `:178-189` (`add_message`), `:52-61` (config defaults)

**What.** Both `check` and `add_message` short-circuit on priority:

```rust
if matches!(priority, MessagePriority::P0 | MessagePriority::P1) {
    account.used_bytes = new_total;          // add_message: no cap check at all
    ...
    atomic_saturating_add(&self.total_bytes, message_size);
    return QuotaDecision::PriorityExempt;
}
```

There is no comparison against `priority_reserved_pool_bytes`, no comparison against
`total_quota_bytes`, no per-sender cap for P0/P1. `grep -n 'priority_reserved_pool_bytes\|total_quota_bytes' crates/iris-core`
shows both fields are referenced **only** in `Default::default()` and in tests — they are read
by nothing.

**Evidence.** `quota.rs` has no code path that reads `self.config.priority_reserved_pool_bytes`
or `self.config.total_quota_bytes`. `huge_message_size_no_overflow` test even asserts
`qm.total_usage() == u64::MAX` after one P0 `add_message(u64::MAX)` — i.e. the test *codifies*
that a single P0 message can drive total accounting to the maximum.

**Why it is wrong.** AC-2: "per-sender quota + priority-reserved pool **enforced** on the
store." `ABUSE_PREVENTION.md`: `emergency_reserved: 100MB` with
`if self.emergency_pool_usage() + size <= self.emergency_reserved { Allow }`.
`FAILURE_ARCHITECTURE.md:127`: "A 500MB quota can hold ~2 million P0 messages. This guarantee
is absolute." `NODE_MODEL.md`: "Storage: bounded by app quota (default 500MB)." The code
provides no bound of any kind for the priority classes, so a single sender (or a free Sybil
fleet) that marks its traffic P0/P1 can consume all memory the accounting is meant to bound.
The P0 abbreviated-envelope size cap (255-700 B) limits per-message size but not count.

**Root cause.** "P0/P1 never *evicted*" was implemented as "P0/P1 never *checked*" — the
reserved-pool concept (a bounded ceiling that P0/P1 draw from and that, once full, still
rejects) was designed (DEC-SEC-0002, module doc) but never coded; the config fields are
vestigial.

**Fix.** Implement the reserved pool: track `priority_used_bytes` separately; a P0/P1 message
is `PriorityExempt` only while `priority_used_bytes + size <= priority_reserved_pool_bytes`
**and** `total_bytes + size <= total_quota_bytes`; beyond that it is `Rejected` like anything
else (a P0 message that cannot be stored must fail loudly to the sender's engine, not silently
inflate accounting). Add `quota_p0_rejected_when_reserved_pool_full` and
`total_quota_ceiling_enforced_for_all_classes` regression tests.

**Dependencies / blast radius.** This changes what the security layer tells the store to
accept. **Section 3** owns the actual message store (`iris-storage`, `message_engine/storage`)
and has its own P0-never-evicted invariant (INV-ROUTE-003, and taksh's TAK-2 in
`docs/bug-hunting/taksh_problems.md`) — the two layers must agree on who enforces the ceiling
and what "reject a P0" means end to end. Do not fix in isolation.

---

### PRY-3 — Quota eviction is entirely unimplemented and priority-blind

- **Severity:** High
- **Fix status:** ✅ Fixed · Tier 2 · commit `780f242` · 2026-08-31 (§8 waived, operator-delegated engineering call) — deleted `select_eviction_candidates`, its two tests (`quota_eviction_min_ttl_and_break`, `eviction_candidates_oldest_first`), and updated the module doc to state "admission only." Section 3 (`iris-storage` / `routing/scf_eviction.rs`) owns physical eviction with full per-message priority knowledge. Resolved together with PRY-30 and the Kani proof.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/quota.rs:225-263` (`select_eviction_candidates`)

**What.** `select_eviction_candidates` is the only eviction entry point. `grep` shows **no
production caller** — only `quota.rs` tests. Beyond being unwired, the implementation is wrong
in three ways:

1. It sorts senders by `oldest_message`/`last_activity` and evicts from every sender in order,
   **with no knowledge of message priority** (`SenderAccount` has no priority breakdown). It
   can and will select P0/P1 bytes for eviction.
2. It calls `evict_amount(account.used_bytes, default_sender_quota_bytes)`, which returns
   `excess.max(1)` — for a below-quota account that is `1`. So under global pressure it tells
   the store to "evict 1 byte" from a sender (see PRY-30).
3. It never consults `total_quota_bytes`; `target_bytes` is a caller argument with no
   defined relationship to the configured ceiling, and there is no caller to define it.

**Evidence.** `grep -n 'select_eviction_candidates' crates/iris-core/src` → definition +
`quota.rs:455,461,521` (tests). The module header says "P0/P1 never evicted by quota action;
eviction by lifetime/TTL + quota violation order" — the function honours neither clause.

**Why it is wrong.** AC-2: "eviction ordering by lifetime + quota criteria tested."
`ROUTING_REQUIREMENTS.md` REQ-ROUTE-NF-003: "bundles are evicted lowest-priority-first,
oldest-first within same priority. P0 bundles are never evicted." The function evicts
oldest-first *across all priorities*, so on a node under storage pressure it will discard a
just-received P0 SOS before an older P5 file transfer from the same or an earlier sender.

**Root cause.** The function was written to a simpler spec ("evict oldest") and the
priority-stratification requirement (which lives in the routing/store docs, a different
section's territory) was never reconciled into it; then it was never wired, so no test caught
the divergence.

**Fix.** Either (a) delete `select_eviction_candidates` and let Section 3's store own eviction
entirely (the store already has priority per message), documenting that the quota layer only
does admission; or (b) if the quota layer must drive eviction, give `SenderAccount` a
per-priority byte breakdown, evict strictly lowest-priority-first / oldest-within-priority,
never touch P0, and return whole-message-sized amounts. Decide with the Section 3 owner which
layer owns eviction — do not have both.

**Dependencies / blast radius.** **Section 3** (`routing/scf_eviction.rs`,
`iris-storage`) owns the store's own eviction. taksh's report already flags the store side
(TAK-2, and a separate flagged task about `scf_eviction.rs` P0 handling). This finding and
those must be resolved together or the mesh gets two disagreeing eviction policies.

---

### PRY-4 — Reputation has no decay caller: a floored peer never recovers

- **Severity:** High
- **Fix status:** ✅ Fixed · Tier 2 · commit `ed43068` · 2026-08-31 — `routing_weight` floors the *selection* weight at `SELECTION_FLOOR` (0.05, AC-9); new `SecurityPolicy::decay_reputation` ticked every engine GC sweep, self-throttled to `decay_interval`. Epsilon value is the operator-delegated call.
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/reputation.rs:253-261` (`decay`), `:146-159` (`routing_weight`), `:194-198` (`LocalDrop`)

**What.** `decay()` moves each peer's score toward `initial_score` (0.5) and is the *only*
recovery mechanism — `LocalForward`/`AuditPositive` also raise it, but a peer that is only
ever observed dropping messages gets no positive events. `grep -n 'decay' crates/iris-core/src`
→ definition + `reputation.rs:414,648` (tests). **No GC task, no routing tick, nothing in
`message_engine` calls `decay()`.** A peer driven to `min_score` (0.0) by ~5 `LocalDrop`
events (default `negative_weight` 0.1, `min_observations` 3) has `routing_weight` return 0.0
**forever**.

**Evidence.** `reputation_not_a_gate` test drives a peer to 0.0 and asserts the weight is
"still usable" — but a routing layer that multiplies candidate scores by `routing_weight`
treats 0.0 as "never select this peer", which on a sparse mesh is indistinguishable from a
blackhole/partition of that peer. Combined with no decay caller, it is permanent.

**Why it is wrong.** The review scope for this section explicitly asks: "check for a peer being
permanently locked out with no recovery path (self-inflicted DoS)." AC-8: reputation includes
a "decay/recovery curve"; AC-9: "a low/missing-reputation relay remains usable as a route."
With `routing_weight → 0.0` and no decay, both are violated the moment reputation is wired to
routing. A single bad radio window, a temporary buffer-full drop, or an adversary who
provokes a few `LocalDrop` observations against an honest peer, permanently removes that peer
from routing consideration for this node.

**Root cause.** `decay()` was written to be "called from GC task" (its own doc comment says
so) but the GC wiring was deferred with the rest of the reputation→routing integration; the
tests call `decay()` directly so its absence in production is invisible.

**Fix.** Wire `decay()` into a periodic task (the engine GC loop, cadence = `decay_interval`),
matching how `replay.rs` batched persistence is meant to be driven. Additionally, clamp
`routing_weight`'s floor for *selection* purposes to a small non-zero epsilon (e.g. 0.05) so a
low-reputation peer is deprioritised but never categorically excluded (AC-9). Regression test:
`floored_peer_recovers_after_decay_intervals` and
`routing_weight_never_returns_exactly_zero`.

**Dependencies / blast radius.** **Section 3** owns the routing weight consumer
(`routing/`). The epsilon-floor decision (does a 0.0-reputation peer get *zero* selection
probability or a tiny one?) is a routing-policy call that needs the Section 3 owner. The GC
cadence wiring touches the engine (Section 2).

---

### PRY-5 — `diffie_hellman` does not reject small-order peer keys

- **Severity:** High
- **Fix status:** ✅ Fixed · Tier 0 · commit 67a2b0a · 2026-08-31
- **Confidence:** Certain (the check is absent); Medium (production reachability)
- **Location:** `crates/iris-core/src/crypto/x25519.rs:18-29`

**What.** `diffie_hellman(local_secret, peer_public)` validates only the RFC 7748 §6.1
all-zero *output*:

```rust
let shared = local_secret.diffie_hellman(&peer);
let bytes = shared.to_bytes();
if bytes.iter().all(|&b| b == 0) { return Err(CryptoError::AllZeroSharedSecret); }
Ok(bytes)
```

It never calls `crate::identity::small_order::is_small_order(peer_public)`. RED-0011 rejection
lives only at trust-boundary adoption points (`advertise.rs`, `chain.rs`, `rotate.rs`,
`trust_store::resolve_x25519`).

**Evidence.** `small_order.rs` module doc explicitly frames itself as catching "the point
substitution itself at the trust/chain boundary … BEFORE a DH is attempted" and says x25519
"keeps only the §6.1 all-zero check." So the design *intends* the agreement primitive to be
unguarded and relies on every caller having filtered the key first.

**Why it is wrong.** The review scope asks precisely this: "confirm small-order/low-order
point rejection is actually applied on every X25519 key-agreement path, not just at
generation." It is not. Any path that reaches `diffie_hellman` with a key that did **not** go
through trust-store validation performs DH against an unchecked point:
- `MemoryKeyDirectory` (`crypto/key_directory.rs`) does no small-order filtering — only
  `TrustKeyDirectory` does. `crypto_e2e.rs` and desktop local contacts use
  `MemoryKeyDirectory`.
- An X25519 key carried in a message's `encryption_hdr` (ephemeral sender key) is consumed by
  the recipient's DH without an advertisement having been adopted for it.
- A low-order point of order 2/4/8 that is **not** all-zero produces a non-zero shared secret
  drawn from a tiny set — the all-zero check does not catch it. x25519-dalek clamps the local
  scalar, which limits (but per the hpke-rs RUSTSEC-2026-0072 lesson does not eliminate) the
  contributory-behaviour / key-confirmation attacks this enables.

**Root cause.** Defense-in-depth was consciously located at the boundary and the primitive was
left bare; the assumption that "every caller filters first" is not enforced by the type system
or by a check at the choke point.

**Fix.** Add `if is_small_order(peer_public) { return Err(CryptoError::SmallOrderPeerKey); }`
as the first line of `diffie_hellman` (new `CryptoError` variant). It is one torsion test per
agreement — negligible cost, and it makes the "every path" guarantee true by construction
regardless of which directory or header the key came from. Keep the all-zero check as the
belt-and-braces final guard. Regression test:
`diffie_hellman_rejects_every_eight_torsion_point` (reuse `curve25519_dalek::EIGHT_TORSION` as
`small_order.rs` tests do).

**Dependencies / blast radius.** `message_engine/crypto.rs` (Section 2) is the main DH caller
via the `CryptoProvider` seam — confirm no test deliberately feeds a low-order key expecting
`AllZeroSharedSecret` specifically (the error variant changes for the non-zero low-order
case). No wire-format impact.

---

### PRY-6 — Replay high-water runs on unauthenticated sender_id, before signature check

- **Severity:** Medium
- **Fix status:** ✅ Fixed (subsumed) · Tier 2 · commit `42eabb5` · 2026-08-31 — re-audited: `process_incoming` now runs `crypto.verify()` (mod.rs:545) **before** `check_replay` (mod.rs:596), so the high-water only ever advances for a signature-verified `sender_id` — the "advance on unverified bytes" exploit is structurally closed (drift since Run 0). The unclamped-`seq` concern is subsumed by PRY-7 (a crafted `seq` is now one entry in a bounded exact set, not a dominator).
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/replay.rs:351-436`; `crates/iris-core/src/message_engine/mod.rs:584-600`

**What.** In `process_incoming`, `check_replay` is called with
`sender_short = peer_short_from_sender(&envelope.sender_id)` and
`envelope.message_id.sequence_hint()` **before the envelope signature is verified** (the
crypto-provider verification happens later in the delivery path). `replay.rs` then does
`highwater.entry(sender).or_default().check_and_advance(stored_ts, seq)` — it *mutates* the
victim's high-water on the strength of attacker-supplied `sender_id` bytes.

The SEC-RT-03 clamp bounds `stored_ts = ts.min(now)`, but `seq` is **not** clamped and is
fully attacker-chosen: `sequence_hint()` returns `u64::from_be_bytes(message_id[0..8])` and the
attacker crafts the message id freely (the id's binding to content is only checked at
signature verification, which this packet fails — after the damage).

**Evidence.** `message_engine/mod.rs:586` computes `sender_short` from raw bytes;
`:588` calls `check_replay`; the dedup and replay checks precede the crypto path. The inline
comment at `:592-598` explains the *previous* fix (stop using `hop_count` as seq) but the
replacement (`sequence_hint`) is still unauthenticated-at-this-point and unclamped.
`ABUSE_PREVENTION.md:51` states rate limits are applied pre-verification by design — but that
is about *cost*, not about *mutating durable anti-replay state* from unverified input.

**Why it is wrong.** AC-4 scopes the high-water mark to "**trusted (verified/authenticated)
senders**." Applying it to unauthenticated senders, and *advancing* it from a packet that is
about to be rejected for a bad signature, lets an on-path attacker: set
`sender_id = victim`, craft `message_id` so `sequence_hint` is large, send with any garbage
signature. The victim's high-water advances to `(now, large_seq)`. Every genuine message from
the victim in the same wall-clock second with a smaller `sequence_hint` is now `Replay`. The
window is ≤ 1 s (next second, `ts > stored.timestamp` wins), matching the SEC-RT-03 bound —
but it is a repeatable, free, per-second denial against any chosen sender, and it also
pollutes the high-water table (PRY-8).

**Root cause.** The replay check was placed early in the pipeline for DoS-cost reasons and
keyed on `sender_id` for simplicity, but "advance durable state" and "cheap early reject" were
not separated — the freshness check can be early and cheap; the high-water *advance* must be
gated on authentication.

**Fix.** Split the check: run the freshness window (`TooOld`/`TooFuture`) early and
statelessly; run `check_and_advance` on the per-sender high-water **only after** the envelope
signature has verified (move that call, or pass a `verified: bool` and only advance when
true). Also clamp `seq` to a sane bound, or better, feed the replay engine a genuine
per-sender counter (see PRY-7) rather than a hash of the id.

**Dependencies / blast radius.** **Section 2** owns the `process_incoming` pipeline ordering
in `message_engine/mod.rs` — moving the high-water advance past signature verification is an
engine-flow change and needs that owner. The freshness-only early check can stay where it is.

---

### PRY-7 — `sequence_hint()` is not a per-sender monotonic counter

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · commit `42eabb5` · 2026-08-31 — behavioural fix landed (option 2): same-second acceptance is a bounded exact seen-set of `sequence_hint` values (`HighwaterTable::admit`, cap 64/sender/sec), not `seq` ordering — an honest same-millisecond burst is no longer ~50 % false-`Replay`. `check_and_advance` kept for the cross-reboot snapshot merge only. Widens an accept path (distinct same-second messages) — under operator sign-off.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/protocol/message_id.rs:47-51` (Section 2, read-only); consumed at `crates/iris-core/src/message_engine/mod.rs:598`; assumed-contract in `crates/iris-core/src/security/replay.rs:44-65`, `:346-351`

**What.** `replay.rs` documents its second argument as `seq = message sequence (per-sender
monotonic counter)` and `HighWaterMark::check_and_advance` requires strictly increasing
`(ts, seq)`. The engine feeds it `message_id.sequence_hint()`, which is
`u64::from_be_bytes(self.0[..8])` — the first 8 bytes of the 16-byte message id. For a
UUIDv7-style id (`MessageId::new_v7()`) those bytes are ~48 bits of millisecond timestamp
followed by ~16 bits of randomness. It is *roughly* increasing across milliseconds but
**random within a millisecond**.

**Evidence.** `message_id.rs:47-51`. `replay.rs:57` — acceptance requires
`ts == self.timestamp && seq > self.sequence`. Two messages from one honest sender minted in
the same millisecond, arriving in the same wall-clock second (`ts` equal after clamp): the
one with the numerically smaller random tail is rejected `Replay` regardless of send order.

**Why it is wrong.** AC-4's replay semantics assume a real monotonic sequence. With a
timestamp-hash, a sender emitting a burst (an SOS retransmit train, a multi-part status
update, fragmented ADUs re-entering the pipeline per AC-7) loses ~50 % of any two messages
that collide in a millisecond, silently, as "replays." It also means the "sequence" carries no
anti-replay information the timestamp does not already carry — the high-water degenerates to a
per-second timestamp check with a coin-flip tiebreak.

**Root cause.** SEC-001 needed a per-sender sequence but the wire format (frozen, "zero
breaking wire-format changes" — D6) has no sequence field, so an existing id substring was
repurposed. The id's timestamp-dominated structure was not reconciled with the
strictly-increasing requirement.

**Fix.** Options, in preference order: (1) derive `seq` from a monotonic per-sender counter
the engine already maintains for outbound (if one exists) and carry it in an existing
optional field; (2) if no field is available, weaken the high-water comparison to
timestamp-only with a bounded per-`(sender, ts)` **set** of seen `sequence_hint` values
(bounded to e.g. 64 per second) so same-second messages are de-duplicated exactly instead of
ordered; (3) at minimum, document `sequence_hint` honestly in `replay.rs` and add a test
proving two same-millisecond ids do not falsely reject.

**Dependencies / blast radius.** `message_id.rs` is **Section 2**. Option (1)/(2) touch the
engine's inbound path. Report the mismatch to the Section 2 owner; the Section 1 fix is the
`replay.rs` comparison change and its test.

---

### PRY-8 — Per-sender high-water table is not Sybil-resistant

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit e748a12 · 2026-08-31 (first-seen-generation eviction; parts 2/3 of the finding — registration rate-limit, time-bucketing — deferred as larger)
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/replay.rs:404-416`

**What.** At `highwater.len() >= max_sender_highwater` (default 10 000) the code evicts
**exactly one** entry, preferring an entry whose `timestamp < min_ts` (outside the 24 h + skew
past window), else the globally oldest:

```rust
let victim = highwater.iter().filter(|(_, hw)| hw.timestamp < min_ts)
    .min_by_key(|(_, hw)| hw.timestamp)
    .or_else(|| highwater.iter().min_by_key(|(_, hw)| hw.timestamp))
    .map(|(k, _)| *k);
```

**Evidence.** Minting an Ed25519 identity is free (`peer_id.rs` doc: "a node's identity IS its
Ed25519 public key"). An attacker generates 10 000 identities and sends one in-window message
from each. The table is now full of marks with `timestamp ≈ now` — none are `< min_ts`, so the
`filter` yields nothing and eviction falls to "globally oldest." Every genuine sender seen
*before* the flood is older → evicted first, one per subsequent attacker message. The
`stale_highwater_marks_are_evicted_before_live_ones` test only floods 32 senders at `now-600`
(still inside the window) and a victim at `now`, so the attackers are "older" and get evicted
— it does not model a same-timestamp flood.

**Why it is wrong.** `SYBIL_RESISTANCE.md` and SEC-001 accept that per-identity limits are
Sybil-defeatable *for admission*, but the anti-replay high-water is a *security control* whose
whole purpose is defeated if a free flood flushes it. After the flush, an old message from a
flushed victim (still inside the 24 h freshness window) reads as a first sighting and is
accepted — a replay. The exposure lasts until the attacker marks age past `min_ts` (up to
24 h + skew).

**Root cause.** The eviction policy was hardened once (against `keys().next()` random
eviction, per the inline comment) but only against a *stale-marks-present* flood; the
*all-fresh* flood case was not considered, and the table size is a fixed constant with no
back-pressure.

**Fix.** (1) Make eviction refuse to drop a mark that is still inside the freshness window
when a stale one exists *anywhere*, and when none are stale, drop the **newest** fresh mark
(the attacker's most recent), not the oldest (the victim's) — a genuine long-lived sender's
old-but-in-window mark is worth more than a just-created one. (2) Rate-limit *new sender
registration* into the table via the unknown-sender aggregate bucket (`check_unknown`) so a
flood cannot register 10 000 identities in a burst. (3) Consider keying the table by
`(sender, coarse_time_bucket)` with per-bucket caps so one time slice cannot be monopolised.

**Dependencies / blast radius.** Self-contained to `replay.rs`. Re-run the replay proptest
suite; add `sybil_flood_does_not_flush_in_window_victim`.

---

### PRY-9 — `ContentType::KeyRotation` routed through the emergency authority ACL as "Drill"

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · commit `3d269d0` · 2026-08-31 — removed `ContentType::KeyRotation => AlertClass::Drill` from `acl.rs::check`; removed `KeyRotation` from the engine's `check_emergency_acl` routing set (`EmergencyAlert | Sos` only). Tests updated: `armed_empty_allowlist_denies_broadcast_and_medical` now asserts `InvalidAuthority` on `P3+KeyRotation` (hits SOS → unsigned → `InvalidAuthority`); proptest `acl_armed_empty_allowlist_class_decision` updated. Drills need a dedicated content-type or payload marker (EMERG-001 follow-up).
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/acl.rs:121-149`; `crates/iris-core/src/message_engine/mod.rs:666-714`

**What.** `EmergencyAcl::check` maps the payload type to an alert class:

```rust
ContentType::KeyRotation => AlertClass::Drill, // Drills use KeyRotation type
```

and `AlertClass::Drill` is dispatched to `check_authority_chain`. But `rotate.rs`'s module doc
states plainly: "Rotation/revocation **rides the existing `ContentType::KeyRotation` (16)
envelope path**", and `advertise.rs`: advertisements are "carried as an application blob inside
a `ContentType::KeyRotation` envelope." The engine (`message_engine/mod.rs:666`) sends every
`EmergencyAlert | Sos | KeyRotation` envelope through `check_emergency_acl`.

**Evidence.** With the default (empty) allowlist this returns `Authorized` (see PRY-1), so it
is currently masked. But the moment an operator populates the `Drill` allowlist — which the
"drills use KeyRotation" comment invites them to do — `check_authority_chain` requires a valid
authority chain rooted in an allowlisted `AuthorityRoot` for *every key-rotation and
advertisement envelope*. An ordinary peer rotating its own X25519 key has no such chain →
`InvalidAuthority` → `InboundOutcome::EmergencyDropped` at `mod.rs:700`.

**Why it is wrong.** Key rotation is a routine identity-hygiene operation for every node, not
an emergency broadcast. Conflating it with "drill" means: (a) configuring drills breaks
rotation mesh-wide; (b) the ACL's `authorized` metric is polluted with every rotation message;
(c) a legitimate rotation can never be distinguished from a spoofed drill because they share a
content type.

**Root cause.** `ContentType` has no dedicated code for "drill", so an existing type was
overloaded; the overload was not checked against the fact that the same type already carries
high-volume routine traffic.

**Fix.** Do not derive `AlertClass::Drill` from `ContentType::KeyRotation`. A drill must be
identified by an explicit marker (a field in the emergency payload, or a distinct
`ContentType`), and `KeyRotation` envelopes must skip the emergency ACL entirely and be
validated by the rotation/advertisement path (`rotate::apply`, `TrustStore::adopt_advertisement`),
which already does signature + monotonicity + small-order checks. Until a drill marker exists,
remove `AlertClass::Drill` handling from `check`.

**Dependencies / blast radius.** **Section 2** owns which content types the engine routes to
`check_emergency_acl` (`mod.rs:666-669`). Removing `KeyRotation` from that set is the cleanest
fix and is an engine change. EMERG-001 owner should confirm the drill-identification
mechanism.

---

### PRY-10 — SOS ACL trusts the raw sender_id and performs no signature check

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · 2026-08-31 (option (a): ACL self-verifies the envelope signature)
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/acl.rs:242-277`

**What.** `check_sos_identity` copies `envelope.sender_id` (must be 32 bytes) into a pubkey,
looks up `self.trust_store.level(&pubkey)`, and returns `Authorized` for
`Verified | AuthorityRoot`. It never verifies that the envelope is actually signed by that
key — it relies entirely on a signature check performed elsewhere in the pipeline.

**Evidence.** The function's own comment at `:266` calls the sender "spoofable 32-byte
sender." Nothing in `acl.rs` calls `verify_strict` or inspects `envelope.signature`.

**Why it is wrong.** AC-11: "spoofed/revoked/expired/replayed authority rejected." The ACL is
a security gate; if it returns `Authorized` based only on `sender_id` matching a `Verified`
entry, then on an armed node an attacker sends an SOS with `sender_id` = any known verified
peer's public key and the ACL passes it. Whether the message is *ultimately* delivered depends
on a separate envelope-signature check — but (a) that coupling is undocumented and fragile,
and (b) for P0 abbreviated / unsigned-emergency envelopes it is not obvious that a full
signature verification runs before delivery. The ACL should not depend on a check it does not
make and cannot see the result of.

**Root cause.** SEC-001 layered the ACL *on top of* the existing message-authentication path
and assumed that path always runs first with a hard failure; the assumption is not asserted
anywhere.

**Fix.** Either (a) `check_sos_identity` verifies the envelope signature itself (it has the
envelope and the trust store's certified keys), returning `InvalidAuthority` on failure; or
(b) `EmergencyAcl::check` takes a `sender_authenticated: bool` produced by the engine's
verification step and refuses to authorize any class when it is false. Document the ordering
contract in `acl.rs` and add `sos_spoofed_verified_sender_without_signature_rejected`.

**Dependencies / blast radius.** **Section 2** owns the envelope signature verification and its
position relative to `check_emergency_acl`. Confirm with that owner whether every emergency
`ContentType` (including P0 abbreviated) is signature-verified before delivery; if not, this
is a larger hole and the severity rises.

---

### PRY-11 — Armed node with only provisioned roots rejects every unpaired peer's SOS

- **Severity:** Medium
- **Fix status:** 🔒 Deferred · product scope (2026-08-31) — the **SOS service is a future update** (operator decision, see *Operator authorization & scope*). The safety-charter call this finding hinges on ("authorize any correctly-signed SOS regardless of `TrustLevel`") is deferred with it. Revisit when SOS ships.
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/acl.rs:252-276`

**What.** `check_sos_identity`:

```rust
match trust {
    TrustLevel::Verified | TrustLevel::AuthorityRoot => Authorized,
    TrustLevel::Unknown | TrustLevel::Unverified if store_empty => Authorized, // Noop
    TrustLevel::Unknown | TrustLevel::Unverified => Unauthorized,              // armed
    _ => Unauthorized,
}
```

`store_empty` is the *only* permissive path for an unpaired sender. A normal armed deployment
provisions at least one NDMA `AuthorityRoot` (`register_authority_root`), so the store is
non-empty. From that point, any sender the local node has not `verify_peer`-ed out of band
(QR/pairing) is `Unverified` or `Unknown` → **`Unauthorized` → SOS dropped**
(`EmergencyDropped` at `message_engine/mod.rs:686`).

**Evidence.** `acl_sos_unknown_identity_populated_store_unauthorized` test codifies exactly
this: an armed node with one registered authority root drops an `Unknown` sender's SOS.

**Why it is wrong.** DEC-SEC-0006 / AC-11: "SOS = **any verified identity** @ 3/hr." The
governing intent (and `SAFETY_CHARTER.md` / `EMERGENCY_GOVERNANCE.md`) is availability-first
for SOS — a person in a genuine emergency who has never done a QR pairing with the relay
in front of them (the overwhelmingly common case in a disaster) cannot get an SOS through.
`ABUSE_PREVENTION.md:108-111`: "A blocked user can still send SOS … if a user unintentionally
blocked someone who then has a real emergency, the SOS still reaches them." The armed ACL is
stricter than the block list.

**Root cause.** "Verified identity" in the spec was read as `TrustLevel::Verified`
(out-of-band paired) rather than "identity whose signature verifies" (self-authenticating —
`peer_id.rs`). On a self-authenticating identity system, *any* correctly-signed SOS is from a
"verified identity" in the sense that matters; `TrustLevel::Verified` is a stronger,
UI-facing tier that should not gate a life-safety message.

**Fix.** For `AlertClass::Sos`, authorize any sender whose envelope signature verifies
(regardless of `TrustLevel`), subject only to the 3/hr origin rate limit (EMERG-001) and the
block list. Reserve `Unauthorized` for `Revoked` and signature-failure. This is a
product/safety decision — confirm with the EMERG-001 / safety owner before changing, but the
current behaviour is very likely a regression against the safety charter.

**Dependencies / blast radius.** EMERG-001 owner + safety owner sign-off required — this
changes who can send an SOS. The 3/hr limiter (EMERG-001, Section 2-adjacent) becomes the
primary abuse control and must be confirmed to be actually enforced.

---

### PRY-12 — `EmergencyAcl` is immutable and half its config is dead

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit f6f36e9 · 2026-08-31 (documented the unwired surface; interior-mutable wiring tracked under PRY-1, gated)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/acl.rs:108-115` (`add_authority`), `:283-285` (`reset`), `:36-53` (config), `:56-66` (`AclDecision::SosRateLimited`)

**What.** `add_authority(&mut self, ...)` and `reset(&mut self)` need `&mut EmergencyAcl`, but
the ACL only ever exists as `Arc<EmergencyAcl>` inside `FullSecurityPolicy`. So:
- The allowlist is fixed at construction (root of PRY-1).
- `AclConfig::sos_rate_limit_per_hour` (default 3) is read by nothing — `check_sos_identity`
  never rate-limits.
- `AclConfig::require_chain_for_broadcast_medical` (default true) is read by nothing.
- `AclDecision::SosRateLimited` is never returned by any function.
- "Medical = authority **+ certified responders**" (`SEC_001_DESIGN.md:84`) — there is no
  "certified responder" concept in the code; Medical is treated identically to Broadcast.

**Evidence.** `grep -n 'sos_rate_limit_per_hour\|require_chain_for_broadcast_medical\|SosRateLimited'`
→ definitions, `Default`, and tests only.

**Why it is wrong.** The config surface advertises capabilities (per-hour SOS limit,
optional chain requirement, responder tier) that do not exist, so an operator tuning
`AclConfig` gets silent no-ops. Dead enum variants and dead config are a maintenance and
audit hazard.

**Root cause.** SEC-001 scoped the ACL data model generously and implemented a subset;
the unimplemented parts were left in the types rather than removed or `todo!()`-marked.

**Fix.** (a) Make the allowlist interior-mutable and wire it (part of PRY-1). (b) Either
implement `sos_rate_limit_per_hour` in `check_sos_identity` (returning `SosRateLimited`, which
`message_engine/mod.rs:702` already handles) or delete the field and the variant and document
that EMERG-001 owns SOS rate limiting. (c) Delete `require_chain_for_broadcast_medical` or
honour it. (d) Delete the "certified responders" language from the design doc or file a
follow-up node for it.

**Dependencies / blast radius.** If SOS rate limiting stays in EMERG-001, no cross-section
work; if it moves here, coordinate with EMERG-001 to avoid double-limiting.

---

### PRY-13 — Every security check serialises on a global write lock

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit `d711f97` · 2026-08-31 — all four engines (`rate_limiter`, `quota`, `replay`, `spam`) now hold per-sender state in a `ShardedLocks<S>` of `SHARD_COUNT = 64` independent `RwLock` shards, routed by a hash of the sender key. Two senders contend only when they hash to the same shard. Count caps (`max_sender_buckets` / `max_sender_highwater` / `max_sender_stats`) become per-shard (`per_shard_cap`, rounding up). `replay::maybe_schedule_snapshot` no longer clones the map on the accept path — the hot path flips a `snapshot_dirty` flag; the all-shard clone runs in `generate_snapshot` off the accept path. New `benches/security_contention.rs` (N tasks × distinct senders vs one hot sender): distinct-sender throughput ~3× the single-hot-sender case on this box. Standard per-shard `RwLock` (not custom atomics), so no `loom` model needed. `ShardedLocks` has its own concurrency stress test.
- **Confidence:** High
- **Location:** `rate_limiter.rs:235` (`self.buckets.write().await` per check), `quota.rs:168` (`self.accounts.write().await`), `replay.rs:391` (`self.highwater.write().await`), `spam.rs:123` (`self.stats.write().await`)

**What.** Each of the four engines takes a single process-wide `tokio::sync::RwLock` in
**write** mode for *every* inbound message — even the read-only `RateLimiter::check` path
takes `buckets.write()` because it mutates a bucket. `replay.rs`'s `check` additionally, once
per persistence interval, clones the entire (up-to-10 000-entry) high-water map under a read
lock inside `maybe_schedule_snapshot` while on the accept path.

**Evidence.** `rate_limiter.rs:235-273` — the whole check body runs under one write guard.
`replay.rs:384-420` — same. There is no sharding, no per-sender lock, no lock-free fast path.

**Why it is wrong.** This is the security seam that *every* inbound message crosses
(`message_engine/mod.rs` calls `check_replay`, `score_spam`, `check_emergency_acl`,
`rate_limit_with_claim` in sequence). A single task holding any of these guards — a slow
allocator during a `HashMap` grow, a preempted task, contention under a message flood — stalls
**all** inbound processing. taksh's report flags the same class of defect on the transport
side (SYS-2 "no timeout", SYS-5 "poisoned-mutex amplification"); this is its security-layer
twin, and it is directly exploitable: flooding a node is how you *create* the contention.

**Root cause.** The engines were written for correctness first with the simplest possible
concurrency model (one lock each) and never revisited for the hot-path implications, because
`FullSecurityPolicy` is not yet wired into the platform engines so the contention has never
been measured.

**Fix.** Shard each map by a prefix of the sender short (e.g. 16 or 256 shards, each its own
lock), or move the per-bucket state behind atomics where possible (`refill_tokens` is already
a pure saturating function — a bucket could be `AtomicU64` tokens + `AtomicU64` last-refill
nanos with a CAS loop, no lock). At minimum, make `maybe_schedule_snapshot` not clone the map
on the accept path — hand a `Weak` / dirty flag to a dedicated persistence task.

**Dependencies / blast radius.** Self-contained to `security/`. Add a concurrency benchmark
(N tasks hammering one node) to `benches/` and a loom model for the sharded map if atomics are
used.

---

### PRY-14 — Sub-second `refill_interval` permanently bricks the rate limiter

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 0 · commit 7471a56 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/rate_limiter.rs:306-316` (`check_bucket`), `:330-343` (`refill`)

**What.** SEC-RT-11 guarded a division-by-zero:

```rust
let period_secs = refill_interval.as_secs().max(1);
let intervals = elapsed.as_secs() / period_secs;
if intervals > 0 { bucket.tokens = refill_tokens(...); bucket.last_refill = now; }
```

If `refill_interval` is, say, `Duration::from_millis(500)`, then `refill_interval.as_secs()`
is `0` → `period_secs = 1`. The outer guard `elapsed >= refill_interval` becomes true after
500 ms, but `elapsed.as_secs()` is still `0` for the whole first second → `intervals = 0` →
**no refill, and `last_refill` is not advanced**. The bucket only ever drains. After the
initial `burst` tokens are consumed, every subsequent check from that sender is
`SilentDrop`, forever.

**Evidence.** `RateLimiterConfig` exposes `refill_interval: Duration` with no lower-bound
validation. `rate_limiter_refill_after_interval` only tests `Duration::from_secs(1)`. Nothing
tests a sub-second interval.

**Why it is wrong.** A config value that is a `Duration` with sub-second resolution silently
produces a limiter that drops all non-exempt traffic from every sender after the first burst
— a self-inflicted total DoS. The `.max(1)` "fix" for the panic converted a crash into a
silent wedge.

**Root cause.** The refill math works in whole seconds (`elapsed.as_secs()` /
`period_secs`), inherited from an early "1 second tick" assumption, and the panic guard
patched the symptom (div-by-zero) without moving the arithmetic to a sub-second unit.

**Fix.** Do the arithmetic in nanoseconds (or milliseconds):
`intervals = elapsed.as_nanos() / refill_interval.as_nanos().max(1)`. Then a 500 ms interval
refills twice a second as intended. Alternatively, validate `RateLimiterConfig` on
construction and reject `refill_interval < Duration::from_secs(1)` with a clear error.
Regression test: `sub_second_refill_interval_actually_refills`.

**Dependencies / blast radius.** Self-contained. Note that `refill_tokens` (the Kani proof
target `rate_limiter_refill_never_exceeds_burst`) is unaffected — the bug is in the *caller's*
`intervals` computation, which the proof does not model (the proof takes `intervals` as a free
input). Worth adding a proof that `check_bucket` advances `last_refill` whenever
`elapsed >= refill_interval`.

---

### PRY-15 — `load_snapshot` blindly replaces the high-water map, no version/time check

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit e748a12 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/replay.rs:481-485`; `ReplaySnapshot` `version` / `snapshot_timestamp` fields `:80-90`

**What.**

```rust
pub async fn load_snapshot(&self, snapshot: ReplaySnapshot) {
    let mut highwater = self.highwater.write().await;
    *highwater = snapshot.into_internal();
}
```

It overwrites the entire in-memory map. `snapshot.version` (a `u32` "for forward
compatibility") is never checked. `snapshot.snapshot_timestamp` is never compared to now.
`FullSecurityPolicy::load_replay_snapshot` (`security/mod.rs:350`) exposes this to any caller.

**Evidence.** `into_internal` (`:109-122`) only filters malformed hex keys; it does no
staleness or version validation. `from_snapshot` (constructor path) has the same issue but is
less dangerous (fresh engine).

**Why it is wrong.** AC-5 is "cross-reboot persistence" — restore *once* at boot. But
`load_snapshot` is a public runtime method with no guard:
- Called at runtime, it **discards every high-water mark accumulated since boot** → the replay
  window reopens for every currently-tracked sender.
- Fed a stale snapshot (hours/days old), it **rolls high-water marks backward** → every
  message sent by every sender since that snapshot can now be replayed (each is `> ` the
  restored mark).
- An unrecognised future `version` is loaded as if it were v1.

**Root cause.** The persistence API was built as a symmetric `generate` / `load` pair without
distinguishing "restore at boot" (replace is fine, map is empty) from "merge at runtime"
(must be a monotonic merge).

**Fix.** (1) `load_snapshot` must **merge**, taking the per-sender max of stored vs incoming
`(ts, seq)` — never regress a mark. (2) Reject a snapshot whose `version` is not a known
value. (3) Optionally warn/skip if `snapshot_timestamp` is older than `freshness_window_past`
(a snapshot older than the window protects nothing). Regression tests:
`load_snapshot_never_regresses_a_live_mark`, `load_snapshot_rejects_unknown_version`.

**Dependencies / blast radius.** `FullSecurityPolicy::load_replay_snapshot` is called by
whatever persistence wiring Section 2/3 adds — confirm it is a boot-only call site, but fix
the merge semantics regardless.

---

### PRY-16 — Spam duplicate-content detection is inert under E2EE

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit c9d545f · 2026-08-31
- **Confidence:** High
- **Location:** `crates/iris-core/src/security/spam.rs:148-155`, `:352-358` (`compute_content_hash`)

**What.** `score` computes `compute_content_hash(&envelope.payload)` (SHA-256 of the payload
bytes) and adds `duplicate_content_penalty` only if it equals `stat.last_content_hash` — the
hash of that sender's **immediately preceding** message.

**Evidence.** For an E2EE message the `payload` is ChaCha20-Poly1305 ciphertext under a
**fresh per-message ephemeral X25519 key** (`crypto/mod.rs` security model: "fresh ephemeral
key per message"). Identical plaintext therefore produces different ciphertext every time →
the hashes never match → the duplicate signal never fires. Separately, comparing only to the
single previous message means an `A, B, A, B, …` flood never triggers it either.

**Why it is wrong.** Duplicate-content is one of only three spam signals
(`unknown_sender_penalty`, `high_volume_penalty`, `duplicate_content_penalty`). Once
encryption is on — which is the normal state for real traffic — one of the three is dead
weight, and the remaining two (first-message penalty, >100-message volume) are crude. The spam
engine's effectiveness drops sharply exactly when it is deployed for real.

**Root cause.** The heuristic was designed against plaintext payloads (Cormack CIKM 2007
reference in the module header is about content classification) and not reconciled with the
CRYPTO-001 per-message-ephemeral design that landed alongside it.

**Fix.** Spam scoring on ciphertext can only use metadata: message rate, payload-size
distribution, recipient fan-out, delivery-failure rate (`ABUSE_PREVENTION.md:216-225` lists
exactly these "content-agnostic abuse signals"). Replace `duplicate_content_penalty` with a
size-bucket-repetition signal (many messages of near-identical *size* to *different*
recipients) and keep a small ring buffer of recent hashes instead of just the last one, for
the plaintext-payload case that still exists (local/unencrypted). Document that duplicate
detection does not work on E2EE payloads.

**Dependencies / blast radius.** Self-contained. `spam.rs` proptests assert `LikelySpam` under
crafted configs — verify they still hold with a size-based signal.

---

### PRY-17 — Chain rule 5 requires monotonically increasing counters *down* the chain

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · 2026-08-31 (option (b): vs trust-store recorded counter — WIDENS, operator sign-off)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/chain.rs:138-141`

**What.**

```rust
// Rule 5: monotonic counter (strictly increasing per hop).
if i > 0 && ad.key_gen_counter <= ads[i - 1].key_gen_counter {
    return Err(ChainError::StaleCounter(i));
}
```

This compares the `key_gen_counter` (each identity's *own rotation generation*) of adjacent
chain elements and requires the child's to be strictly greater than the parent's.

**Evidence.** `chain.rs:19` documents it as "monotonic counter policy: each element's counter
strictly increases from the previous element's." `key_gen_counter` is per-identity
(`advertise.rs:33` "Monotonic rotation counter (D4)") — it counts how many times *that*
identity has rotated its X25519 key. Two different identities' rotation counts have no
ordering relationship.

**Why it is wrong.** A legitimate two-level org chain: NDMA root has rotated its key twice
(`key_gen_counter = 2`); it certifies a freshly-provisioned district authority whose
`key_gen_counter = 0`. Rule 5: `0 <= 2` → `StaleCounter(1)` → chain rejected. To make any
multi-level chain validate, every deeper element must coincidentally have a higher personal
rotation count than its certifier — which is backwards (roots are older and rotate more). The
`authorized_child_chain_passes` test only works because it hand-picks `counter=1` for the
child against `counter=0` for the root.

**Root cause.** The rule was intended as a replay/freshness guard on the chain *as a whole*
(so a stale cached chain can't be replayed) but was implemented as an inter-element comparison
of a field that means "this identity's rotation generation," not "this element's position."

**Fix.** Rule 5 should either (a) be dropped — element freshness is already covered by
`valid_until` expiry (rule 7) and by the fact each link's signature is over the child's
current advertisement; or (b) compare each element's counter against the **trust store's
recorded counter for that same identity** (reject an element that names an older generation
than the store has already seen for that identity), which is the actual anti-replay property.
Do not compare across identities.

**Dependencies / blast radius.** Changes which chains `verify_chain` accepts — could newly
accept chains previously rejected. `emergency/authority.rs` and `security/acl.rs` both call
`verify_chain`; both add an `is_authority_root` gate afterward, so the blast radius is bounded
to "which authorised chains validate," but the EMERG-001 owner must confirm the intended chain
semantics. Update `chain.rs` tests (`stale_counter_link_rejected` encodes the current
behaviour).

---

### PRY-18 — A signed rotation advertisement silently swaps a `Verified` peer's key

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 1 · commit 9786826 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/trust_store.rs:186-197`

**What.** In `adopt_advertisement`, the "different static key, strictly higher counter" branch:

```rust
} else if ad.key_gen_counter > e.key_gen_counter {
    e.key_gen_counter = ad.key_gen_counter;
    e.valid_until = ad.valid_until;
    e.static_x25519_pubkey = ad.static_x25519_pubkey;   // key replaced
    if e.level == TrustLevel::KeyChanged { e.level = TrustLevel::Unverified; }
    AdoptionOutcome::RotationAdopted
}
```

If `e.level == TrustLevel::Verified` (the peer was confirmed out-of-band via QR/pairing), the
level is **left at `Verified`** while the encryption key is replaced with one that was never
confirmed out of band.

**Evidence.** Only `KeyChanged` is cleared. `Verified` and `Unverified` pass through
untouched. `resolve_x25519` returns the new key for a `Verified` entry, and the UI (per
`trust_store.rs` module doc: "UI distinguishes the tiers") shows the peer as still Verified.

**Why it is wrong.** `verify_peer` (`:222-240`) exists precisely so the operator confirms
*"the identity AND the exact static key shown in the QR/pairing payload"* (RT-010 comment).
Adopting a rotation preserves the `Verified` badge but breaks the property it certifies — the
displayed key is no longer the confirmed one. An attacker who compromises only the identity
Ed25519 signing key (not the old X25519 secret) can push a rotation advertisement and
thereafter the victim's contacts encrypt to the attacker's key while their UI still says
"Verified."

**Root cause.** Rotation was designed to be seamless ("older envelopes still decrypt until
`valid_until`") and the trust-level implications of a key change under a `Verified` peer were
not carried through — the `KeyChanged` downgrade was added for the *conflict* case but not the
*monotonic rotation* case.

**Fix.** On any `static_x25519_pubkey` change (rotation or conflict), a `Verified` entry must
drop to `Unverified` (or a new `RotatedPendingReverify` state) so the UI prompts the operator
to re-confirm. The message still decrypts (new key resolves) but the trust badge honestly
reflects "not re-confirmed since last rotation." Regression test:
`verified_peer_drops_to_unverified_on_rotation`.

**Dependencies / blast radius.** UI (platform, Sections 4/5) shows trust tiers — a new state
or a `Verified→Unverified` transition on rotation is a UX change they must handle. IDENT-001
owner should confirm the intended post-rotation trust semantics (KEY_MANAGEMENT.md /
TRUST_MODEL.md).

---

### PRY-19 — Reputation eviction panics on a NaN score

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 0 · commit 0095c8d · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/reputation.rs:168-176`, config `:20-44`, `routing_weight` clamp `:152-155`

**What.**

```rust
if let Some((k, _)) = peers.iter().min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap()) {
```

`f64::partial_cmp` returns `None` (→ `unwrap` panics) if either operand is NaN.
`ReputationConfig` has public `f64` fields (`initial_score`, `decay_factor`, `positive_weight`,
`negative_weight`, `min_score`, `max_score`, …) with **no validation**. A NaN in
`initial_score` (or a `decay_factor` that produces NaN via `initial + diff*factor` with an
infinite `diff`) propagates into `rep.score`, and the next capacity eviction panics.

**Evidence.** `ReputationConfig::default()` uses sane values, but `new(config)` and
`SecurityPolicyConfig` accept any `ReputationConfig`. `.clamp(min_score, max_score)` does not
sanitise NaN (`f64::clamp` with a NaN input returns NaN; with NaN bounds it panics in debug).
taksh's report hit the identical class in the transport comparator (TAK-21, MG-3: "total_cmp
orders NaN ABOVE +inf, so the poisoned transport WON selection").

**Why it is wrong.** A crash on the reputation update path is a DoS, and `ReputationConfig`
being publicly constructible means a misconfiguration (or a future config-from-file path)
turns a typo into a panic. `min_by(...).unwrap()` on floats is exactly the pattern the
codebase has already been burned by elsewhere.

**Root cause.** Float scores without a sanitisation boundary; `unwrap()` on `partial_cmp`
instead of `total_cmp` or a NaN-filtering comparator.

**Fix.** (1) Sanitise `ReputationConfig` on construction — replace any non-finite field with
its default, or return an error. (2) Sanitise `rep.score` after every update: `if
!rep.score.is_finite() { rep.score = self.config.initial_score; }`. (3) Use
`f64::total_cmp` (or drop non-finite scores before `min_by`) so eviction cannot panic.
Regression test: `nan_config_does_not_panic_reputation_engine`.

**Dependencies / blast radius.** Self-contained. Mirror the fix pattern taksh applied in
`FeatureVec::new` (TAK-21) for consistency.

---

### PRY-20 — `register_verified_peer` inflates a peer's score on every call

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit dab45c1 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/reputation.rs:237-251`

**What.**

```rust
pub async fn register_verified_peer(&self, peer: PeerShort) {
    self.verified_peers.write().await.insert(peer);        // idempotent
    let rep = peers.entry(peer).or_insert_with(...);
    rep.score = (rep.score + 0.1).min(self.config.max_score);   // NOT idempotent
}
```

The `HashSet` insert is idempotent, but the `+0.1` score bump runs unconditionally every
call. Calling `register_verified_peer` 10× drives the peer to `max_score`.

**Evidence.** `reputation_register_verified_peer_bumps_score` test calls it once and asserts
`> 0.5`; nothing tests repeated calls.

**Why it is wrong.** If `register_verified_peer` is ever driven by a network event (a
second-hand verification nomination, a re-pairing, a periodic re-sync from the identity
layer), repeated events pump the target's routing weight to maximum for free — a
reputation-boosting primitive. Even operator-driven, re-running provisioning double-counts.

**Root cause.** "Verified peers start with slightly higher trust" was implemented as a
mutation on a method that is otherwise idempotent, without guarding on
"was this peer already in the verified set."

**Fix.** Only apply the `+0.1` on the *first* registration:
`if self.verified_peers.write().await.insert(peer) { rep.score = (rep.score + 0.1).min(max); }`
(`HashSet::insert` returns `false` if the key was present). Regression test:
`repeated_register_verified_peer_does_not_stack_bonus`.

**Dependencies / blast radius.** Self-contained.

---

### PRY-21 — Spam-stats cap eviction uses arbitrary hash order

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit 204f34c · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/spam.rs:126-130`

**What.**

```rust
if stats.len() >= self.config.max_sender_stats && !stats.contains_key(&sender) {
    if let Some(k) = stats.keys().next().copied() { stats.remove(&k); }
}
```

`HashMap::keys().next()` is arbitrary (hash-seed-dependent) order — the exact pattern
`replay.rs:404` was explicitly rewritten away from ("`HashMap::keys().next()` was used here,
which is arbitrary order and not adversary-resistant").

**Evidence.** `replay.rs:398-405` inline comment documents why this pattern is unsafe;
`spam.rs` still uses it.

**Why it is wrong.** At `max_sender_stats` (default 100 000), a Sybil flood of spoofed sender
shorts evicts genuine senders' spam stats at random. That resets a real sender's
`message_count` to 0, so their next message re-triggers `unknown_sender_penalty` (a false
"likely spam" annotation), and it discards accumulated `spam_hits`. Impact is limited because
spam scoring is annotation-only (never drops), but the inconsistency with the hardened
`replay.rs` is a latent trap.

**Root cause.** SEC-RT-06 added the cap but not an adversary-resistant eviction order; the
`replay.rs` hardening was not back-ported.

**Fix.** Evict by an actual policy: lowest `message_count` first (a spoofed one-shot sender vs
an established one), or track insertion order like `rate_limiter.rs`'s `VecDeque` and evict
oldest. Regression test mirroring `stale_highwater_marks_are_evicted_before_live_ones`.

**Dependencies / blast radius.** Self-contained.

---

### PRY-22 — The un-claimed rate-limit path re-opens the P0 starvation primitive

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 0 · commit 7471a56 (+ test 443b7a2) · 2026-08-31 · CI grep-guard deferred (touches `.github/workflows/`, out of Section 1 scope)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/rate_limiter.rs:202-205`; `crates/iris-core/src/security/mod.rs:39-42`

**What.** `RateLimiter::check` delegates to `check_with_claim(sender, class,
EmergencyClaim::Emergency)` — hard-coded `Emergency`. The `SecurityPolicy` trait's default
`rate_limit` method (`mod.rs:39`) also ignores content and returns `Allowed`. `check_with_claim`
exists specifically because granting the P0/P1 exemption on claimed priority alone was "a free
network-starvation primitive" (`rate_limiter.rs:210-215`).

**Evidence.** `message_engine` currently calls `rate_limit_with_claim` everywhere
(`grep` confirms `mod.rs:425,733,1017`), so this is not live today. But `RateLimiter::check`
and `SecurityPolicy::rate_limit` remain public, and `check`'s hard-coded `Emergency` means any
future caller that reaches for the simpler `check` / `rate_limit` name grants P0/P1 exemption
to *all* content, restoring the exact primitive that `check_with_claim` was built to close.

**Why it is wrong.** A security API should not have a shorter-named method that is a strict
downgrade of the safe one. The default trait method and `check` are footguns.

**Root cause.** `check_with_claim` was added alongside `check` for backward compatibility
rather than replacing it; `check` was left defaulting to the permissive claim.

**Fix.** Make `RateLimiter::check` default to `EmergencyClaim::Ordinary` (safe default —
callers who know the content is emergency use `check_with_claim`), or remove `check` entirely
and update the trait so `rate_limit` is not a callable shortcut. Add a CI grep guard against
`\.rate_limit\(` / `\.check\(sender` in `message_engine`.

**Dependencies / blast radius.** Trait change touches `NoopSecurityPolicy` and
`FullSecurityPolicy` impls (both in `security/mod.rs`) and any external implementor — grep
shows none outside the crate.

---

### PRY-23 — `NodeIdentityV1::from_bytes` leaves seed copies on the stack unzeroed

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit a1a9665 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/provision.rs:90-107`

**What.**

```rust
let read_32 = |i: usize| -> [u8; 32] {
    let mut b = [0u8; 32];
    b.copy_from_slice(&blob[i..i + 32]);
    b                       // returned by value → copied again into Zeroizing::new(...)
};
...
identity_ed25519_seed: Zeroizing::new(read_32(13)),
static_x25519_secret: Zeroizing::new(read_32(45)),
```

The closure's local `b` and the return-value move both hold plaintext seed bytes on the stack
and are **not** zeroized when the closure returns / the temporary is consumed. Only the final
`Zeroizing<[u8;32]>` field and the source `blob` (which `KeyStore::load_identity` does return
as `Zeroizing`) are wiped.

**Evidence.** `seeds_are_zeroized_on_drop` test only proves the *field* Drop path works, not
the decode-time temporaries.

**Why it is wrong.** The section's crypto checklist asks for "any `Drop` that doesn't zero key
material." These are transient but real copies of the Ed25519 seed and X25519 secret in
process memory that survive until the frame is reused. Low severity because the window is
short and the same bytes exist in the `Zeroizing` source `blob` anyway — but the decode path
should not widen the exposure.

**Root cause.** Ergonomic closure returning `[u8;32]` by value; `Zeroizing` applied only at
the final resting place.

**Fix.** Have the closure write directly into a `Zeroizing<[u8;32]>` and return that, or
decode into the struct fields in place. Trivial.

**Dependencies / blast radius.** Self-contained.

---

### PRY-24 — Windows key files fall back to CWD with no permission control

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit 0ef2c02 · 2026-08-31 (Windows resolver; unix/macOS `.` fallbacks left — out of this finding's scope)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/store.rs:75-98` (`app_data_dir`), `:239-256` (`set_user_only` / `check_user_only` non-unix), `:171-179` (`with_header_stripped`)

**What.** On Windows, `app_data_dir()` reads `%APPDATA%` and, if unset,
`unwrap_or_else(|| PathBuf::from("."))` — the current working directory. `set_user_only` and
`check_user_only` are `Ok(())` no-ops on non-unix ("std's ACL control is intentionally not
engaged in v1"). So with `APPDATA` unset (a stripped service account, a misconfigured
container, certain CI), `identity.v1` and `master.key` — containing the raw Ed25519 seed and
X25519 secret — are written into whatever directory the process started in, with default
(often group/other-readable) permissions.

**Evidence.** `store.rs:78-81`. `check_user_only` non-unix returns `Ok(())` unconditionally
(`:254-256`).

**Why it is wrong.** IDENT_DESIGN §5.3's Windows posture is "Windows user-appdata isolation" —
that isolation is exactly what the CWD fallback discards. The `identity_file_is_0600` /
`all_key_files_are_0600_from_birth` tests are `#[cfg(unix)]` only, so Windows has no
permission assertion at all.

**Root cause.** A defensive fallback (`"."`) for a missing env var, combined with a
platform where permission enforcement is a documented no-op, produces an insecure default with
no guardrail.

**Fix.** On Windows, if `%APPDATA%` (and `%LOCALAPPDATA%`) are unset, **fail** rather than
writing keys to CWD — the operator must set a data directory. If a fallback is required, use
`%USERPROFILE%` and apply an explicit restrictive ACL via `windows`/`windows-acl` (or refuse
to proceed). Add a Windows-gated test that the file is not world-readable.

**Dependencies / blast radius.** Platform (Section 4/5 desktop) chooses the data dir via
`FileKeyStore::at` in practice, so the fallback is a defense-in-depth gap rather than the
primary path — but it should not silently do the wrong thing.

---

### PRY-25 — X25519 keypair is `Clone` with no explicit zeroization guarantee

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit c38ee98 · 2026-08-31 (feature pin + Drop-observing test; `Clone` retained — removing it is broader than the finding warrants)
- **Confidence:** Medium
- **Location:** `crates/iris-core/src/crypto/keygen.rs:11-37`

**What.** `X25519Keypair { secret: X25519StaticSecret, public: X25519PublicKey }` derives
`Clone`. Neither `X25519Keypair` nor `IdentityKeypair` implements an explicit `Drop` or
`ZeroizeOnDrop`; both rely entirely on the upstream dalek types zeroizing on drop, which is
gated behind those crates' `zeroize` cargo features. Nothing in the repo asserts those
features are enabled.

**Evidence.** `keygen.rs:12` `#[derive(Clone)]` on the secret-bearing struct. `Cargo.toml`
was not inspected for the exact `x25519-dalek` / `ed25519-dalek` feature set; there is no test
or `cargo tree` check pinning `zeroize`.

**Why it is wrong.** The section's crypto checklist calls out "zeroization … any `Drop` that
doesn't zero key material." `Clone` on a secret means every `.clone()` (e.g.
`runtime_node_identity`, `IdentityManager` construction) creates another copy whose
zeroization depends on a transitive feature flag. If a dependency bump ever drops the
`zeroize` feature (or a `default-features = false` is added), the secrets stop being wiped and
no test fails.

**Root cause.** Convenience `Clone` derive; implicit reliance on upstream defaults.

**Fix.** (1) Add a build-time assertion or a test that `X25519StaticSecret` / `SigningKey`
zeroize (a `Drop`-observing test like `provision.rs`'s `seeds_are_zeroized_on_drop`, applied
to `X25519Keypair`). (2) Pin `x25519-dalek`/`ed25519-dalek` with explicit
`features = ["zeroize", ...]` in `Cargo.toml` and forbid `default-features = false` there.
(3) Consider `#[derive(ZeroizeOnDrop)]` on the wrappers or removing `Clone` from
`X25519Keypair` (callers can re-derive from bytes).

**Dependencies / blast radius.** Removing `Clone` would ripple to `runtime_node_identity` and
any test that clones a keypair — grep first. The feature-pin + test is low-risk and should
land regardless.

---

### PRY-26 — `verify_chain` doc contradicts itself on trust-store mutation

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit ea9ba9c · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/chain.rs:59-66`

**What.** The doc comment says:

> On success the final identity is **registered into the trust store** (TOFU for derived
> peers) and its certified X25519 key resolves for encryption. `verify_chain` is
> **stateless** — it never mutates the caller's trust store; the caller decides whether to
> adopt the outcome.

Both sentences in one paragraph. The function body does no registration (confirmed — it only
reads `trust.entries()`, `trust.level()`, `trust.is_trusted_root()`).

**Evidence.** `chain.rs:66-166` — no `adopt_advertisement`, no `insert`, no `&mut`.

**Why it is wrong.** A caller that reads the first sentence will assume the derived peer's key
is now resolvable after a successful `verify_chain` and skip the explicit
`adopt_advertisement` call — so subsequent encryption to that peer fails
`KeyUnavailable`/`KeyDirectory` returns `None`. It is a documentation defect that produces an
integration bug.

**Root cause.** The behaviour changed (from mutating to stateless) and only half the doc was
updated.

**Fix.** Delete the first sentence's "is registered into the trust store" clause. State
clearly: "`verify_chain` is stateless. On success the caller must call
`TrustStore::adopt_advertisement` with the final element to bind the derived peer." Add a doc
example.

**Dependencies / blast radius.** Doc-only. Grep callers (`acl.rs`, `emergency/authority.rs`)
to confirm they already adopt separately where needed.

---

### PRY-27 — `check_freshness_only` omits the skew budget that `check` applies

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit 7b5683a · 2026-08-31 · **WIDENS** an accept path — landed under the operator's recorded Tier-2 sign-off (fix log Run 2)
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/replay.rs:439-463` vs `:346-375`

**What.** `check` (tracked senders) computes the future bound as
`now + freshness_window_future + per_source_skew_budget`. `check_freshness_only` (untrusted
senders) uses only `now + freshness_window_future` — no skew term (comment: "Future direction
stays conservative for untrusted senders (no skew)"). The past bound *does* include skew in
both.

**Evidence.** `:456` `let max_ts = now.saturating_add(self.config.freshness_window_future.as_secs());`
— no `.saturating_add(per_source_skew_budget)`.

**Why it is wrong.** Untrusted senders are precisely the population least likely to have an
NTP-synced clock (a stranger's phone in a disaster). A device whose clock is fast by 4 minutes
(well within the 5-minute default skew budget) has its messages rejected `TooFuture` by an
untrusted-path relay, while the same device is accepted once it becomes a tracked sender. The
asymmetry penalises exactly the messages most likely to be legitimate-but-unsynced, and
`TooFuture` is a hard drop.

**Root cause.** "Conservative for untrusted" was applied to the future direction without
noticing it makes the freshness check *stricter* for the less-trusted party in a way that
harms availability more than it helps security (a future-dated message is not a replay).

**Fix.** Apply `per_source_skew_budget` to the future bound in `check_freshness_only` too, or
document a deliberate, smaller untrusted skew allowance (e.g. half). Match `check`'s past-bound
treatment, which already includes skew for untrusted.

**Dependencies / blast radius.** Self-contained. `replay_freshness_only_untrusted` test uses
`now + 600` (10 min) which stays `TooFuture` either way; add a `now + 240` case.

---

### PRY-28 — Rate-limiter FIFO `order` deque leaks tombstones

- **Severity:** Low
- **Fix status:** ⚪ Not applicable · 2026-08-31 — premise does not hold against current code. Below the bucket cap `map` and `order` grow **together** 1:1 (nothing removes from `map` until eviction); at/above the cap the eviction `while` loop pops every dead front entry, so `order.len() == map.len()` always (verified by simulation: `max(order − map) == 0` over a 64-identity × 40-cycle churn through an 8-bucket cap). No unbounded growth relative to the (cap-bounded) `map`. If the eviction loop is ever changed, revisit.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/rate_limiter.rs:124-139`, `:239-252`

**What.** `BucketState.order: VecDeque<BucketKey>` is `push_back`-ed on every new bucket
insert and only `pop_front`-ed during eviction (skipping keys already removed from `map`).
`map` is capped at `max_sender_buckets`, but `order` is **only drained when the map is at
capacity and an insert happens**. When a bucket is removed for any other reason — and more to
the point, when the cap is high and never hit — `order` accumulates one entry per distinct
`(sender, class)` ever seen and never shrinks.

Actually re-reading: eviction only triggers `while state.map.len() >= max`. So if the cap is
never reached (large `max`, modest sender population that churns *identities* faster than it
fills), `order` grows with total distinct senders ever seen while `map` stays small. Even when
the cap *is* hit, each eviction pops just enough to remove one live key, leaving earlier
tombstones (keys popped-and-were-dead) gone but never compacting for the not-yet-at-cap case.

**Evidence.** `reset()` clears both, but nothing calls `reset()` in production. There is no
periodic compaction of `order` against `map`.

**Why it is wrong.** On a busy relay with identity churn (Sybil or just many transient
peers), `order` is an unbounded `VecDeque<BucketKey>` (18 bytes each) that grows for the life
of the process. It is a slow memory leak on the security hot path — the same "never evicted /
grows without bound" class flagged repeatedly in the other sections' reports
(`shrey_problems.md` #28, `rahul_problems.md` PM-6).

**Root cause.** The FIFO-eviction fix (GAP-3) added an insertion-order queue but no bound on
the queue itself independent of the map.

**Fix.** Compact `order` opportunistically — e.g. when `order.len() > 2 * map.len()`, rebuild
it from `map`'s keys. Or switch to an `IndexMap` / `LinkedHashMap` where insertion order and
membership are the same structure. Add a test asserting `order.len() <= map.len() + K` after a
churn workload.

**Dependencies / blast radius.** Self-contained.

---

### PRY-29 — `verify_chain` accepts a TOFU root; only the caller's extra check stops EMERG-RT-001

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit `3d269d0` · 2026-08-31 (§8 boundary lifted) — added `RootRequirement` enum (`AnyTrusted` / `AuthorityRoot`) as 4th parameter to `verify_chain`. Both callers (`acl.rs::check_authority_chain`, `emergency/authority.rs`) pass `AuthorityRoot`; redundant `is_authority_root` post-checks removed. `emergency/authority.rs` maps `ChainError::NotAuthorityRoot → AuthorityError::UntrustedAuthorityRoot`. New regression test `authority_root_requirement_rejects_tofu_root` (16 chain tests green). All existing callers updated to pass `RootRequirement::AnyTrusted`.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/chain.rs:82-89`; `crates/iris-core/src/identity/trust_store.rs:321-326` (`is_trusted_root`)

**What.** `verify_chain` rule 2 is `if !trust.is_trusted_root(&root.identity_pubkey)`, and
`is_trusted_root` returns `true` for `AuthorityRoot | Unverified | Verified`. So a plain
TOFU-adopted peer satisfies `verify_chain`'s root requirement. The
"only an authority root may anchor an emergency chain" property (EMERG-RT-001) is enforced
**outside** `verify_chain`, by a separate `trust.is_authority_root(...)` call in both
`security/acl.rs:230` and `emergency/authority.rs:129`.

**Evidence.** Both current callers do add the check (verified). But it is a convention, not a
structural guarantee: a third caller of `verify_chain` that makes an authorization decision
and forgets the extra line inherits a bypass where any TOFU peer anchors an "authority" chain.

**Why it is wrong.** A security-critical invariant that lives in every caller rather than in
the function is one refactor away from a hole. `chain.rs` is `pub` and re-exported from
`identity/mod.rs`.

**Root cause.** `verify_chain` was written for the general RED-0008 peer-tree case (where
TOFU roots are legitimate) and the emergency-specific tightening was bolted on at the call
sites when EMERG-RT-001 was found, rather than folded into a parameterised verifier.

**Fix.** Give `verify_chain` a `root_requirement: RootRequirement` parameter
(`AnyTrusted` | `AuthorityRootOnly`) and move the `is_authority_root` check inside for the
strict variant. Update the two callers to pass `AuthorityRootOnly`. Now the invariant cannot
be forgotten. Regression test: `verify_chain_authority_only_rejects_tofu_root`.

**Dependencies / blast radius.** Signature change to a `pub` function — update both callers
and their tests. EMERG-001 owner should review.

---

### PRY-30 — `evict_amount` returns 1 byte for a below-quota account

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit `780f242` · 2026-08-31 (§8 waived) — deleted `evict_amount` (its only caller `select_eviction_candidates` was deleted in PRY-3) and the Kani proof `quota_eviction_bounded_and_positive` (which targeted the deleted function). Updated `kani_proofs.rs` module doc scope comment. Resolved together with PRY-3.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/quota.rs:63-72`

**What.**

```rust
pub(crate) fn evict_amount(used: u64, quota: u64) -> u64 {
    let excess = used.saturating_sub(quota);
    excess.max(1).min(used)      // below quota → excess 0 → returns 1
}
```

The Kani proof `quota_eviction_bounded_and_positive` *proves* this returns `>= 1` for any
non-empty account — i.e. it certifies the surprising behaviour rather than catching it.

**Why it is wrong.** `select_eviction_candidates` (PRY-3) pushes `(sender, to_evict)` where
`to_evict` can be 1, and increments `freed` by 1. The storage layer cannot evict "1 byte" — a
message is the unit of eviction. Any future code that wires `select_eviction_candidates` to a
real store must translate "evict N bytes from sender S" into "evict whole messages until >= N
freed", and with `N = 1` that means "evict one message" from a sender that is *under* quota —
punishing a compliant sender under global pressure. The `.max(1)` floor makes the function's
output not a byte count but a "should this sender be touched at all" flag wearing a byte
count's type.

**Root cause.** The floor-1 was added so a below-quota account "still yields one byte when the
global quota forces eviction" (doc comment) — conflating "select this account for eviction"
with "how many bytes to take."

**Fix.** Separate the two concerns: a selection predicate (is this account eligible for
eviction under global pressure?) and a per-account amount (`used.saturating_sub(quota)`,
which may legitimately be 0 → skip). If a below-quota account must be evicted from under
extreme pressure, the caller decides that policy and evicts whole messages; `evict_amount`
should return the honest excess. Update the Kani proof to match the corrected contract.

**Dependencies / blast radius.** Coupled to PRY-3 — fix together. The Kani proof
(`kani_proofs.rs`, Section 2-owned per the review brief) will need its assertion updated;
flag to that owner.

---

### PRY-31 — The RED-0011 small-order blocklist is corrupted, incomplete, and unverified

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 0 · commit 67a2b0a · 2026-08-31
- **Confidence:** High (the list is wrong and incomplete); Medium (exploitability — the `to_edwards` fallback provides partial cover)
- **Location:** `crates/iris-core/src/identity/small_order.rs:27-47` (`SMALL_ORDER_U`), `:53-64` (`is_small_order`), `:71-90` (tests)

**What.** `SMALL_ORDER_U` is a hand-typed `[[u8; 32]; 4]` of "the four canonical low-order
u-coordinates." Three problems:

1. **`SMALL_ORDER_U[2]` is corrupted.** Its doc comment names the value
   `325606250916557431795983626356110631294008115727848805560023387167927233504`, whose
   canonical little-endian X25519 encoding is
   `e0 eb 7a 7c 3b 41 b8 ae 16 56 e3 fa f1 9f c4 6a da 09 8d eb 9c 32 b1 fd 86 62 05 16 5f 49 b8 00`.
   The code has
   `e0 eb 7a 7c 3b 41 e8 e3 36 1f f0 1a 31 55 38 dc 1f bf 01 a3 15 38 dc 1f bf 01 a3 15 38 dc 0f 5f`
   — divergent from byte 6, and containing the tell-tale internal repeat
   `1f bf 01 a3 15 38 dc` at offsets 16-22 **and** 23-29. It is a mistyped constant.
   (`SMALL_ORDER_U[3]` is correct — this is a single bad entry.)
2. **The array is incomplete.** The full X25519 small-order / blocklist set has seven values,
   not four: `0`, `1`, the two ~2^252 points, and the **non-canonical high-end encodings**
   `p−1` (`ec…7f`), `p` (`ed…7f`), `p+1` (`ee…7f`) — which X25519 reduces to `1`, `0`, `1`.
   `x25519-dalek`'s `MontgomeryPoint` does **not** reduce its input, so a peer key of `p+1`
   agrees to the same weak secret as `u = 1` and is not in `SMALL_ORDER_U`.
3. **No test cross-checks the constants.** `classic_small_order_values_are_rejected` iterates
   `SMALL_ORDER_U` and asserts `is_small_order(&u)` for each — trivially true because
   `is_small_order` checks `SMALL_ORDER_U.iter().any(|u| u == public_key)` first (a value
   always equals itself). So a corrupted entry passes its own test.
   `whole_torsion_subgroup_is_rejected` uses `EIGHT_TORSION[i].to_montgomery()` — canonical
   encodings only; it never exercises the non-canonical `p±1` forms.

**Evidence.** The byte comparison above; `is_small_order`'s fast-path `any(|u| u == public_key)`
at `:54`; the test at `:71-76`.

**Why it is wrong.** RED-0011's whole point is to reject a low-order point *before* DH. The
explicit fast-path list is described as "a second, checkable safety net" — it is neither
checkable (no cross-check) nor correct. The only real protection is the fallback
`mp.to_edwards(sign).is_small_order()` at `:60-63`. For the corrupted `[2]` that fallback may
still catch the *intended* point (if `to_edwards` decompresses it) — but for the missing
`p±1` non-canonical encodings, `to_edwards` is likely to return `None` (treated as twist,
deferred to `x25519.rs`'s all-zero check), and the all-zero check only catches the case where
the DH *output* is zero — `u = p+1 ≡ 1` yields a **non-zero** low-order secret that slips
through both layers. Compounded with PRY-5 (the agreement path doesn't call `is_small_order`
at all), a peer that substitutes `p+1` for its advertised key gets a predictable shared
secret.

**Root cause.** Hand-transcribed cryptographic constants with no automated cross-check against
a vetted source, and a torsion model (`E[8]` membership) that does not account for
non-canonical field-element encodings of the identity/generator.

**Fix.** (1) Replace `SMALL_ORDER_U` with the constants pulled from a vetted source —
`curve25519-dalek` exposes the torsion table; the seven-value X25519 blocklist is in RFC 7748
§6.1 discussion and every mainstream library (libsodium `crypto_scalarmult_curve25519`,
Python `cryptography`). Include `p−1`, `p`, `p+1`. (2) Add a **canonical-form check** to
`is_small_order` (or to `diffie_hellman` per PRY-5): reject any `u` whose value is `>= p`
after masking the high bit, since a canonical X25519 public key is always `< 2^255 - 19`. (3)
Add a real cross-check test: assert each `SMALL_ORDER_U` entry decompresses via `to_edwards`
to an actual `is_small_order()` Edwards point (so a garbled constant fails the test), and add
`p_minus_1`, `p`, `p_plus_1` rejection cases.

**Dependencies / blast radius.** Self-contained to `small_order.rs` + a companion assert in
`x25519.rs` (PRY-5). No wire impact. Re-run every caller's tests (`advertise.rs`, `chain.rs`,
`rotate.rs`, `trust_store.rs` all gate on `is_small_order`) — none should regress since the
change only *adds* rejected values.

---

### PRY-32 — The SEC-001 emergency ACL enforces none of the authority-meta constraints

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · commit `b1d3a61` · 2026-08-31 — `check_authority_chain` decodes the `EmergencyBroadcast` payload and runs `emergency::authority::verify_authoritative` for Broadcast/Medical (geo scope, functional scope, `max_severity` cap, drill discipline, validity window). Closes the "SEC-001 armed without EMERG-001" gap where the engine's `emergency_gate` (the other path) does not run.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/security/acl.rs:153-239` (`check_authority_chain`); contrast `crates/iris-core/src/emergency/authority.rs:99-202` (`verify_authoritative`, called from `emergency/broadcast.rs:48`)

**What.** There are two emergency-authority verification code paths:

- **`emergency/authority.rs::verify_authoritative`** (EMERG-001, 9 steps): chain +
  `is_authority_root` + leaf binding + **geo scope** (`area_code` prefix) + **functional
  scope** + **`max_severity` cap** + **drill discipline** + **validity window**
  (`issued_at`/`expires_at`).
- **`security/acl.rs::check_authority_chain`** (SEC-001, called by the engine at
  `message_engine/mod.rs:672` for every `EmergencyAlert | Sos | KeyRotation` envelope):
  `verify_chain` + allowlist contains root + `is_authority_root`. **That is all.** It never
  decodes the emergency payload, so geo scope, severity cap, drill discipline, and the
  validity window are not checked.

**Evidence.** `check_authority_chain` (`acl.rs:153-239`) takes `envelope: &Envelope` and
never touches `envelope.payload` beyond the chain. `verify_authoritative` is invoked only from
`emergency/broadcast.rs` — whether *that* runs on the engine's inbound path for a given
content type is a Section 2 / emergency question, but `check_emergency_acl` unquestionably
runs (the engine calls it directly).

**Why it is wrong.** If a deployment relies on the SEC-001 ACL as its emergency gate (it is
the one wired into `message_engine`), an authority whose certificate profile caps it at
`max_severity = Moderate`, `geo_scope = "IN-GJ"`, non-drill, can push a `Critical`,
nationwide, drill-flagged broadcast and the ACL returns `Authorized`. The severity cap and
geo scope — the mechanism that stops a district officer from issuing a national CRITICAL alert
— exist only on a path that may not run.

**Root cause.** SEC-001 and EMERG-001 landed as parallel efforts; SEC-001's `acl.rs`
reimplemented a *subset* of EMERG-001's authority verification (the chain part) without
calling into or reproducing the profile-scope checks, and the engine wired the subset.

**Fix.** `check_authority_chain` for `Broadcast`/`Medical`/`Drill` must call
`emergency::authority::verify_authoritative` (decoding the `EmergencyBroadcast` payload) — one
verification path, not two. If the ACL genuinely cannot see the decoded payload at its call
site, the engine must run `verify_authoritative` for emergency broadcasts in addition to the
ACL, and the ACL must document that it is *only* the chain-anchor check.

**Dependencies / blast radius.** **EMERG-001 owner** must confirm which path is authoritative
and whether `verify_authoritative` currently runs on the engine inbound path for
`ContentType::EmergencyAlert`. **Section 2** owns the engine call sites. This is the same
"two parallel verification paths, weaker one is wired" shape as PRY-9.

---

### PRY-33 — `seal_outbound` sends plaintext when the recipient key is missing (fail-open)

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · commit `3d269d0` · 2026-08-31 (§8 boundary lifted) — `seal_outbound`'s absent-key `else` branch now returns `Err(MsgEngineError::Crypto(CryptoError::KeyUnavailable))` instead of logging `MSG_SENT_UNENCRYPTED` and proceeding plaintext. P0 SOS broadcast (intentionally plaintext, returns early before the key-directory lookup) unaffected.
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/message_engine/mod.rs:353-409` (`seal_outbound`, Section 2); `crates/iris-core/src/crypto/key_directory.rs:41-46` (`require_key`, Section 1, unused)

**What.** `seal_outbound` encrypts an addressed message only if the recipient's X25519 key
resolves from the key directory. When it does not, the code logs
`event = event::MSG_SENT_UNENCRYPTED` ("addressed message sent WITHOUT encryption: recipient
key absent from key directory") and **proceeds to sign and send the plaintext envelope**.

`crypto::key_directory::require_key` — the helper that returns
`Err(CryptoError::KeyUnavailable)` when the key is missing — exists for exactly this decision
point. `grep -n 'require_key' crates/iris-core/src` → definition + its own unit test only. No
production caller.

**Evidence.** `message_engine/mod.rs:397-408` — the `else` branch of the key lookup logs and
falls through to `self.crypto.sign(envelope)`. `observability/mod.rs:96` defines the
`msg.sent_unencrypted` event string, confirming this is an anticipated, supported outcome.

**Why it is wrong.** RED-0002 ("do-not-send-unencrypted") was explicitly *absorbed into ACL-1*
per `SEC_001_DESIGN.md:18`. The crypto layer's confidentiality guarantee ("confidentiality
against passive eavesdroppers", `crypto/mod.rs:12`) fails **open**: a missing directory entry
— which happens routinely before a contact's advertisement has been adopted — silently
downgrades a private message to cleartext on the mesh, visible to every relay. The user is
told nothing; only a debug/warn log records it. `key_directory::require_key` is the
fail-closed primitive Section 1 provides for this, and nothing uses it.

**Root cause.** An availability-over-confidentiality default ("send it anyway so the message
gets through") baked into the seal path, with the fail-closed option left as an unused
library function.

**Fix.** `seal_outbound` for an addressed (non-broadcast) message must use `require_key` and
return an error (`MsgEngineError::RecipientKeyUnavailable`) when the key is absent — the
message stays queued / surfaces to the UI as "cannot encrypt to this contact yet," never goes
out in clear. A deliberate plaintext send must be an explicit, per-message opt-in
(`Envelope` flag or a separate API), not the default. Broadcast/emergency content that is
meant to be plaintext is unaffected.

**Dependencies / blast radius.** **Section 2** owns `seal_outbound` and must agree to the
fail-closed default (it is a behaviour change: messages that currently send will now fail
until the contact key is known). Sections 4/5 (platform UI) need to surface the new
"encryption pending" state. This is the crypto/identity boundary's most direct confidentiality
regression, so it is worth the coordination.

---

### PRY-34 — Relayed traffic bypasses per-sender storage quota accounting

- **Severity:** Medium
- **Fix status:** ✅ Fixed · Tier 2 · commit `780f242` · 2026-08-31 (§8 waived) — added `policy.add_message(relay_sender, relay_size, sched_priority)` to `enqueue_relay` after the rate-limit gate; `Rejected` → `InboundOutcome::RateLimited` with metric increment and debug log. Updated the stale inline comment on the relay path.
- **Confidence:** High
- **Location:** `crates/iris-core/src/message_engine/mod.rs:995-1030` (relay path, Section 2); `crates/iris-core/src/security/quota.rs` (Section 1)

**What.** The inbound-relay path calls `policy.rate_limit_with_claim(relay_sender, ...)` (added
per the inline comment: "Relayed traffic was governed by nothing at all") but it **never
calls `policy.add_message(...)`** — the storage-quota gate. The outbound `send_message` path
does call `add_message` (`:451`); the relay path does not.

**Evidence.** `message_engine/mod.rs:995-1030` — only `rate_limit_with_claim` appears; no
`add_message` / `check_quota`. `grep -n 'add_message' crates/iris-core/src/message_engine` →
`:452` only (the outbound path).

**Why it is wrong.** AC-2: "per-sender quota + priority-reserved pool **enforced on the
store**." Relayed messages are persisted (store-carry-forward, DTN buffering) and queued for
onward transmission — they consume exactly the storage the quota is meant to bound — but they
never touch a sender's `SenderAccount`. A remote peer that floods this node with relay traffic
is metered by the token bucket (rate) but not by the 50 MB per-sender storage quota, so a
sustained low-rate flood fills the store without limit. Combined with PRY-2 (P0/P1 unbounded)
and PRY-3 (eviction unwired), the storage exhaustion surface is wide open on the relay path.

**Root cause.** Quota was wired only into the locally-originated send path; the relay path's
security gating was added later (rate limit only) and the storage accounting was not carried
over — plausibly because `quota.rs` is not yet fully wired and the store side (`iris-storage`,
Section 3) has its own accounting that was assumed to cover it.

**Fix.** The relay path must call `policy.add_message(relay_sender, size, priority)` before
persisting/queuing a relayed message and honour a `Rejected` decision (drop the relay,
increment a metric), and call `policy.remove_message` when the relayed message is delivered or
evicted. Decide with the Section 3 owner whether the quota layer or `iris-storage` owns
relay-traffic accounting — but one of them must, and today neither does on this path.

**Dependencies / blast radius.** **Section 2** owns the relay call site; **Section 3** owns
`iris-storage`'s parallel accounting (and taksh's TAK-2 covers the store-side P0 exhaustion).
Reconcile all three so relay traffic is accounted exactly once.

---

### PRY-35 — Chain rule 6 can never match an abbreviated (16-byte) sender_id

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit 56eec04 · 2026-08-31 · WIDENS (abbreviated chains now validate) — under operator sign-off
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/chain.rs:159-163`

**What.**

```rust
let final_ad = ads.last().expect("non-empty by the Empty check");
if final_ad.identity_pubkey.as_slice() != sender_id {
    return Err(ChainError::SenderMismatch);
}
```

`final_ad.identity_pubkey` is always `[u8; 32]`. If `sender_id` is a 16-byte P0
abbreviated-envelope sender id (`SHA-256(pubkey)[..16]`), the slice comparison is always
unequal → `SenderMismatch`, unconditionally.

**Evidence.** `chain.rs:161`. P0 abbreviated envelopes carry a 16-byte `sender_id`
(`peer_id.rs:26-33`, `peer_short`); the BLE 512 B ceiling (`SEC_001_DESIGN.md:224`) forces
abbreviated form for radio-native emergency traffic.

**Why it is wrong.** Any emergency broadcast that (a) must carry an `auth_cert_chain` and
(b) is sent in abbreviated form is rejected before any real check runs. Either this is
intentional (abbreviated envelopes are for individual SOS, never authority broadcasts — in
which case it should be an explicit, documented `ChainError::AbbreviatedSenderUnsupported`
with a clear message, not an incidental byte-length mismatch) or it is a gap that blocks a
legitimate use case.

**Root cause.** `verify_chain` was written against full 32-byte identities; the
abbreviated-envelope case was not considered at the `sender_id` binding step.

**Fix.** Add an explicit early check: if `sender_id.len() == 16`, compare against
`peer_short(&final_ad.identity_pubkey)`; if `sender_id.len() == 32`, compare directly;
otherwise `ChainError::BadSenderIdLength`. Document which forms are supported. Regression
tests for both lengths.

**Dependencies / blast radius.** Self-contained to `chain.rs`. Confirm with EMERG-001 whether
abbreviated authority broadcasts are a real requirement.

---

### PRY-36 — Trust-mutating `TrustStore` methods are `pub` and self-authenticate nothing

- **Severity:** Low
- **Fix status:** ✅ Fixed · Tier 1 · commit 9786826 · 2026-08-31
- **Confidence:** Certain
- **Location:** `crates/iris-core/src/identity/trust_store.rs:246-257` (`un_revoke`), `:259-266` (`revoke`), `:275-295` (`adopt_rotation`), `:334-366` (`register_authority_root`)

**What.** All four methods mutate trust state and each relies entirely on its caller having
performed the authentication:
- `revoke(id)` — marks `Revoked`, zeroes the key. No signature check.
- `un_revoke(id)` — un-revokes back to `Unverified`. No check at all — "operator recovery
  path" by convention only.
- `adopt_rotation(...)` — swaps the key + counter; re-checks small-order but **not** the
  rotation event's signature or counter monotonicity (that is `rotate::apply`'s job).
- `register_authority_root(ad, now)` — verifies the ad's *self-signature* but then
  `m.insert`s unconditionally, **overwriting** any existing entry (a `Verified` peer, a
  higher rotation counter) with `level: AuthorityRoot` and the ad's key/counter.

They are all `pub` and re-exported paths exist (`identity/mod.rs`).

**Evidence.** `rotate::apply` (`rotate.rs:170-245`) correctly gates `revoke` / `adopt_rotation`
behind signature + counter + expiry checks. But `trust_store.rs` exposes them directly;
`un_revoke` has no gated caller at all in the current tree, and `register_authority_root`'s
unconditional `m.insert` can reset a tracked identity's counter (rotation-replay window, see
also the note in `priyam_problems.md` §8).

**Why it is wrong.** A security state machine whose transitions are `pub` and authenticate
nothing is one careless caller (or one future feature that calls `trust.un_revoke` from a
network handler) away from a trust bypass. The invariants ("revocation never auto-heals,"
"only provisioned roots reach `AuthorityRoot`") live in the *callers*, not the type.

**Root cause.** The trust store was built as a thin data layer with policy in `rotate.rs` /
`chain.rs` / provisioning; the boundary between "library primitive" and "authenticated
operation" was never drawn, so every primitive is fully public.

**Fix.** Make `revoke`, `un_revoke`, `adopt_rotation` `pub(crate)` and route all external use
through the authenticated entry points (`rotate::apply`, an operator-recovery API that logs
and requires an explicit confirmation token). `register_authority_root` must refuse to
overwrite an existing entry with a *lower* counter, and should be `pub(crate)` behind a
provisioning-only facade. Add a doc-contract line to each remaining `pub` method stating what
the caller must have verified.

**Dependencies / blast radius.** Grep all callers (`emergency/*`, `security/acl.rs`, tests)
before narrowing visibility. IDENT-001 owner should confirm the intended API surface.

---

## 7. Cross-checks against the Kani proofs

The brief asks whether the proofs `rate_limiter_refill_never_exceeds_burst` and
`quota_eviction_bounded_and_positive` actually cover the current code.

- **`rate_limiter_refill_never_exceeds_burst`** proves `refill_tokens(tokens, intervals, rate,
  burst) <= burst` and monotonicity. That property still holds — `refill_tokens` is unchanged
  and correct. **But the proof takes `intervals` as a free symbolic input**; it does not model
  how the caller (`check_bucket` / `refill`) *computes* `intervals` from elapsed time. PRY-14
  is a bug in that computation (`elapsed.as_secs() / period_secs` collapses to 0 for
  sub-second intervals), entirely outside the proof's scope. The proof is sound but gives
  false confidence that "refill is proven correct."
- **`quota_eviction_bounded_and_positive`** proves `evict_amount(used, quota) <= used` and
  `>= 1` when `used > 0`. It faithfully covers `evict_amount` — but see PRY-30: the `>= 1`
  clause it proves is itself the defect (a byte count that is really a flag). The proof
  certifies the wrong contract.
- Neither proof, nor any other harness, covers: the priority-reserved-pool ceiling (PRY-2,
  unimplemented so nothing to prove), `select_eviction_candidates` ordering (PRY-3), the
  `sequence_hint` monotonicity assumption (PRY-7), or the high-water eviction adversary model
  (PRY-8).

**Recommendation:** after PRY-14 and PRY-30 land, extend the Kani harnesses (Section 2 to
action) to (a) prove `check_bucket` advances `last_refill` whenever `elapsed >= refill_interval`,
and (b) prove the corrected `evict_amount` returns exactly `used.saturating_sub(quota)`.

## 8. What was checked and found clean

- **`crypto/aead.rs`** — RFC 8439 KAT passes; all error paths collapse to a single
  `Encrypt`/`Decrypt` variant (no padding/tag oracle); caller-supplied 12-byte nonce with the
  reuse contract documented as an engine responsibility. No finding.
- **`crypto/ed25519.rs`** — `verify_strict` only; `from_slice` failure and verify failure both
  return `Ok(false)`; no `verify()` re-export. RFC 8032 KAT passes. No finding.
- **`crypto/kdf.rs`** — HKDF-SHA256 RFC 5869 A.1/A.2/A.3 KATs pass; `MESSAGE_KEY_SALT` /
  `STORAGE_KEY_SALT` domain separation verified. No finding.
- **`identity/small_order.rs`** — the `to_edwards().is_small_order()` fallback covers all 8
  `EIGHT_TORSION` points; twist points deferred to the all-zero check. **But** the explicit
  `SMALL_ORDER_U` fast-path list is corrupted and incomplete (PRY-31), and `x25519.rs` doesn't
  call `is_small_order` at all (PRY-5). Revised from "clean" after the second pass.
- **`identity/peer_id.rs`** — key-derived identity, no MAC/serial input (CVE-2025-53627
  lesson); SHA-256 truncation deterministic across processes. No finding.
- **`identity/rotate.rs`** — signature-before-state, monotonic counter, earliest-seen-wins,
  small-order re-check in `adopt_rotation`, revoked-cannot-rotate. The `apply` → `entries()` →
  `adopt_rotation` sequence has a benign TOCTOU under concurrent rotation events (last-write
  wins, no monotonicity re-check in `adopt_rotation`) — noted here as sub-threshold; worth a
  mutex-scoped compare-and-swap if rotation ever becomes concurrent.
- **`identity/trust_store.rs`** — revocation never auto-heals; stale-replay of a different key
  correctly returns `Duplicate` (RT-001) not `KeyChanged`; `resolve_x25519` re-checks
  small-order at the read boundary. The one real finding is PRY-18 (Verified not downgraded on
  rotation).
- **`security/rate_limiter.rs`** — `refill_tokens` saturating and Kani-proven; the
  `check_with_claim` content-vs-priority split correctly closes the P0-starvation primitive
  for the wired callers. Findings are PRY-14, PRY-22, PRY-28.
- **`emergency/authority.rs::verify_authoritative`** (read in the second pass; Section 2
  territory but Section 1 calls into it) — the 9-step check is thorough: chain +
  `is_authority_root` + leaf binding + geo/functional scope + severity cap + drill discipline
  + validity window, with content-free error discriminants. The problem is not this function
  but that the *engine-wired* ACL path (`security/acl.rs`) doesn't use it (PRY-32).

## 9. What the second pass (revision 2) covered

The first pass read every Section 1 source file and the primary docs. The second pass:

- **Re-read in full** `emergency/authority.rs` (previously only grepped), the
  `message_engine` seal/send path (`mod.rs:353-480`), and the `message_engine` relay path
  (`mod.rs:995-1035`) — Section 2 code, read as reference for how Section 1's primitives are
  actually invoked.
- **Hand-verified the `SMALL_ORDER_U` constants** byte-by-byte against the canonical X25519
  small-order values → PRY-31.
- **Traced the two emergency-authority verification paths** to their call sites
  (`security/acl.rs` via `check_emergency_acl`; `emergency/authority.rs` via
  `emergency/broadcast.rs`) → PRY-32.
- **Grepped for callers of every fail-closed / authenticated primitive** Section 1 exposes
  (`require_key`, `verify_authoritative`, `revoke`, `un_revoke`, `adopt_rotation`) → PRY-33,
  PRY-36.
- **Checked the quota call sites** on both the outbound and relay paths → PRY-34.
- **Re-examined `chain.rs` rule 6** against the abbreviated-envelope sender format → PRY-35.

**Still not done** (would need a third pass, a running toolchain, or a parallel audit):
`cargo fuzz run fuzz_security_engines`; `cargo kani`; a line-by-line adversarial re-read of
`advertise.rs` / `provision.rs` CBOR and binary decoders for parser-differential bugs; the
`proptest-regressions/security/*.txt` seed files; and the `emergency/` and `message_engine/`
modules beyond the specific paths listed above (those are Sections 2's to own regardless).
Treat 36 as "what two careful passes found," not a proven ceiling.
