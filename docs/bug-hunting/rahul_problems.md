# Protocol, Messaging & Emergency — Bug-Hunt Problem Tracker

**Reviewer:** Rahul Hadiyal
**Source review:** `PROTOCOL_MESSAGING_EMERGENCY_REVIEW.md`
**Scope:** `crates/iris-core/src/` — `protocol/`, `message_engine/`, `discovery/`, `emergency/`, `message.rs`, `lib.rs`, `kani_proofs.rs`; benches and integration tests in scope; `security/mod.rs`, `iris-storage`, platform crates read-only context.
**Companion documents:**
- [`rahul_fix_log.md`](rahul_fix_log.md) — execution journal (what each run did, evidence, commits)
- [`rahul_problems_loop.md`](rahul_problems_loop.md) — autonomous loop spec (how to execute this tracker)

**Baseline (review start):** All in-scope tests pass — `golden_vectors` 6/6, `protocol_conformance` 10/10, `sysval_dtn_multihop` 1/1, `sysval_mesh_integration` 4/4, `sysval_security_flood` 2/2.

**Build note:** GNU toolchain required on Windows (`stable-x86_64-pc-windows-gnu`). MSVC cannot link without Windows SDK. `iris-desktop` excluded from all build/test commands. `cargo kani` Linux-only — Kani findings (PM-16) must be re-verified on Linux CI.

---

## Progress Tracker

| Tier | Name | Total | ✅ Fixed | 🔒 Blocked | ❌ Reverted | ⬜ Not started |
|---|---|---|---|---|---|---|
| 0 | Critical/High safety+security | 6 | 3 | 0 | 0 | 3 |
| 1 | High correctness + Medium safety/protocol | 7 | 0 | 0 | 0 | 7 |
| 2 | Medium/Low correctness, performance, protocol | 5 | 0 | 0 | 0 | 5 |
| 3 | Structural enhancements | 7 | 0 | 0 | 0 | 7 |
| **Total** | | **25** | **3** | **0** | **0** | **22** |

**Last updated:** 2026-08-30 · **Active tier:** 0

---

## Tier membership (authoritative)

**Tier 0** (6 findings — fix before anything else; live safety/security defects):
PM-1, PM-2, PM-4, PM-5, PM-6, PM-15

**Tier 1** (7 findings — high/medium correctness and protocol; no human gate):
PM-3, PM-7, PM-8, PM-9, PM-10, PM-12, PM-14

**Tier 2** (5 findings — medium/low severity; no human gate):
PM-11, PM-13, PM-16, PM-17, PM-18

**Tier 3 — GATED** (7 structural enhancements — requires explicit human go-ahead before any commit):
PS-1, PS-2, PS-3, PS-4, PS-5, PS-6, PS-7

**Not applicable:** none.

**Total: 6 + 7 + 5 + 7 = 25 findings.**

---

## Tier 0 — Critical/High safety + security

---

### PM-1 — SOS P3 downgrade destroys the signature it needs to relay

- **Fix status:** ✅ Fixed · Tier 0 · commit 0beaca2 · 2026-08-30 · PENDING LINUX-CI (IrisCryptoProvider wire-integrity test)
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:798`; signing scope `crates/iris-core/src/protocol/codec.rs:161`
- **Category:** safety · **Severity:** Critical
- **Tier:** 0

**What:** The 4th+ SOS from a sender is downgraded to P3 by assigning `envelope.priority = MessagePriority::P3` on a verified envelope. Priority is wire field 5, inside the Ed25519 signing scope. The envelope relays with the original signature over the original priority. The next hop's `crypto.verify()` fails and the message is dropped as `BadSignature`. "Degraded delivery" silently becomes "no delivery" on multi-hop paths.

**Evidence:** Probe confirmed: signing a P0 SOS with `IrisCryptoProvider`, applying the line-798 mutation, re-verifying → `VERIFY AFTER PRIORITY DOWNGRADE = false`. Engine tests do not catch this because `DevCryptoProvider::verify()` ignores its argument and returns `Ok(true)` (`crypto.rs:148`).

**Root cause:** `build_map` (`codec.rs:161`) emits field 5 into all three scopes (Full, Signing, Aead). The downgrade site at `mod.rs:798` mutates the field in place with no re-sign afterward.

**Fix:** Do not mutate signed wire fields in transit. Carry the degrade as node-local scheduling metadata: add `scheduling_priority: MessagePriority` field to `QueuedMessage` (defaulting to `envelope.priority`), set that to P3 on the downgrade path, have `PriorityQueue::cmp` order on `scheduling_priority`. The wire envelope then relays byte-identical and verifiable.

**Dependencies / blast radius:**
- `QueuedMessage` and `PriorityQueue::cmp` change (both in `message_engine/`).
- Engine tests that exercise the downgrade (`emergency_gate_downgrades_fourth_sos_to_p3`, `mod.rs:2032`) assert on `envelope.priority` and must be rewritten.
- Engine test suite uses `DevCryptoProvider` — at least one integration test should run with `IrisCryptoProvider`.
- **Section 1 coordination:** confirm no `SecurityPolicy` implementation keys off `envelope.priority` after the gate — `MessageClass::from(envelope.priority)` at `mod.rs:924` shifts class when the downgrade fires.

---

### PM-2 — Every SOS CANCEL is rejected; a false alarm can never be withdrawn

- **Fix status:** ✅ Fixed · Tier 0 · commit 9c83143 · 2026-08-30 · PENDING LINUX-CI (IrisCryptoProvider wire-integrity test)
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:1503`; policy `crates/iris-core/src/emergency/sos.rs:60-71`
- **Category:** safety · **Severity:** Critical
- **Tier:** 0
- **Must ship together with:** PM-15

**What:** `classify_sos` takes `original_timestamp: Option<u64>`. The engine always passes literal `None` at the one non-test call site. For `SosKind::Cancel`, `None` falls into `None => Err(SosError::UnknownOriginal)`, which `emergency_gate` maps to `EmergencyGateOutcome::EmergencyDropped`. Every SOS CANCEL is dropped unconditionally on every node. The `Ok(SosOutcome::AcceptedCancel)` branch at `mod.rs:1535` is unreachable dead code.

**Evidence:** Probe: engine-shape CANCEL → `Err(UnknownOriginal)`. Same payload with resolved `original_timestamp` → `Ok(AcceptedCancel)`. `grep` for `classify_sos(` returns exactly one non-trait, non-test call site at `mod.rs:1503`.

**Root cause:** The engine-side ledger lookup presumed by `sos.rs`'s doc comment was never built. `classify_sos` is a correct pure policy function being fed an argument the caller cannot supply.

**Fix:** Maintain a bounded `HashMap<MessageId, (u64, sender_id)>` of accepted-SOS entries capped and evicted on the 60-minute `SOS_CANCEL_WINDOW_SECS` horizon. On a Cancel, look up `msg.original_message_id` and pass the stored timestamp + sender_id. Wire through `EmergencyProvider` trait (testable behind the trait). Also implement PM-15's same-signer check in the same change.

**Dependencies / blast radius:**
- `EmergencyProvider` trait gains a lookup seam.
- `NoopEmergencyProvider` and `EmergencyGateway` both need updating.
- New tests required for the accept path (zero coverage today at engine level).
- **PM-15 must ship in the same commit** — fixing this without PM-15 converts a latent cancel-forgery hole into an exploitable one.
- **Section 1 coordination (PM-15):** same-signer comparison should reuse `identity::peer_short_from_sender`.

---

### PM-4 — Fragment reassembly runs before rate limiting, with no per-sender quota

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:695-726` (fragment path) vs `:913-945` (rate limit); `crates/iris-core/src/message_engine/fragment.rs:207`, `:268-277`
- **Category:** security · **Severity:** High
- **Tier:** 0

**What:** In `process_incoming`, the fragment branch returns `Ok(InboundOutcome::FragmentBuffered)` at line 724 — before `enqueue_relay` (the only place `rate_limit_with_claim` runs). An incomplete fragment is completely unmetered. One authenticated peer can hold all 256 reassembly slots indefinitely and deny fragmented delivery to every other peer.

**Evidence:** Call-graph analysis. `rate_limit_with_claim` appears exactly once in `message_engine/mod.rs`, inside `enqueue_relay` (line 931); the fragment branch returns at line 724 without reaching it. `MAX_ACTIVE_SETS` eviction selects the global minimum deadline with no sender term. No test exercises multi-sender contention for reassembly slots.

**Root cause:** `MAX_ACTIVE_SETS` was added for memory bounding with FIFO eviction. The rate limiter was placed on the relay path only; the fragment buffer was not recognised as a consumable resource.

**Fix (two independent changes — do both):**
1. Per-sender quota in `FragmentAssembler`: track sets per `sender_id`, cap at 4–8. On overflow, evict from the offending sender's own set first.
2. Meter the fragment path: hoist `rate_limit_with_claim` from `enqueue_relay` up into `process_incoming` so every inbound envelope is metered exactly once regardless of branch.

**Dependencies / blast radius:**
- `FragmentSet` already stores `sender`, so the quota needs only an index.
- Hoisting the rate limiter changes where `MESSAGES_RATE_LIMITED_TOTAL` is incremented.
- New tests needed for per-sender eviction fairness.
- **Section 1 coordination:** hoisting changes when `SecurityPolicy::rate_limit_with_claim` is invoked relative to the emergency ACL check. Section 1 owns that trait and must confirm the reordering does not weaken the emergency exemption. Do not change `security/mod.rs` from this section.

---

### PM-5 — Handshake decode ignores the size budget encode enforces; neighbor table uncapped

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/discovery/handshake.rs:65-76`, `:136-148`; `crates/iris-core/src/discovery/neighbor_table.rs:73-77`
- **Category:** security · **Severity:** High
- **Tier:** 0

**What:** `CapabilityBundle::encode` rejects anything over `CAPABILITY_MAX_BYTES` (256). `CapabilityBundle::decode` performs no such check — it is a bare `ciborium::from_reader`. `NeighborTable` is a plain `HashMap` with no capacity bound.

**Evidence:** Probe: `encode()` rejects a 12912-byte structure with `BudgetExceeded`; `decode()` accepts the same 12.9 KB (50× budget overrun) without complaint.

**Root cause:** Validation was written on the send side as a correctness property of our own encoder, not mirrored on the receive side as a security property.

**Fix:**
1. Enforce the budget in `decode` before parsing: `if bytes.len() > CAPABILITY_MAX_BYTES { return Err(HandshakeError::BudgetExceeded(bytes.len())); }`.
2. Add an explicit `m` ceiling to `BloomFilter::from_parts` derived from `BLOOM_CAPACITY`.
3. Give `NeighborTable` a capacity cap with LRU-by-`last_seen` eviction.

**Dependencies / blast radius:**
- `handshake_sizes` documentation and `bloom_exchange_round_trip` test should assert the new bounds.
- **Section 3 coordination:** `NeighborTable` is consumed by the routing layer for path selection. Section 3 must agree the cap and eviction key rather than this section choosing unilaterally.

---

### PM-6 — SOS rate-limiter timestamps are unbounded within the window

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/emergency/rate_limit.rs:36-39`, `:82-117`; exemption `crates/iris-core/src/message_engine/mod.rs:1430-1436`
- **Category:** security · **Severity:** High
- **Tier:** 0

**What:** `SenderWindow.timestamps` is a `Vec<u64>` appended on every recorded SOS (including downgraded ones) and trimmed only by *age*, never by *count*. SOS bypasses the general rate limiter, so nothing bounds arrival rate. One sender sustaining SOS traffic for an hour accumulates one `u64` per message with no ceiling — at 1000 msg/s that is 3.6M entries (~29 MB) for a single bucket. `retain()` runs over the whole vector on every `check()` and `record()`, making cost O(n) per message and O(n²) over the window.

**Evidence:** Code analysis — `timestamps` has no size cap; both entry points call `retain` over the full vector; `emergency_claim` (`mod.rs:1430`) returns `Emergency` for `ContentType::Sos`, granting the rate-limit exemption.

**Root cause:** The module correctly bounds *sender count* (`SOS_MAX_BUCKETS`, LRU) but not *per-bucket entry count*. The per-bucket time bound only bounds size if arrival rate is bounded, and for SOS it is not.

**Fix:** Replace `Vec<u64>` with a fixed `[u64; SOS_P0_ALLOWANCE]` ring buffer plus a count. The allowance decision needs at most 3 in-window entries; `retry_after_secs` needs the oldest of those 3. Entries beyond the allowance carry no information. Makes both `check` and `record` O(1) and per-bucket footprint constant. Also replace `touch`'s `retain` with an epoch-counter or intrusive LRU so bucket touching is O(1).

**Dependencies / blast radius:**
- `window_slides_over_time` asserts allowance recovery as individual entries age out; with a 3-entry ring that behaviour is preserved but the test should be re-derived.

---

### PM-15 — AC-6 same-signer requirement for SOS CANCEL is unimplemented

- **Fix status:** ✅ Fixed · Tier 0 · commit 9c83143 · 2026-08-30 · PENDING LINUX-CI (shipped together with PM-2)
- **File(s):** `crates/iris-core/src/emergency/sos.rs:60-71`; spec `docs/implementation/EMERG_DESIGN.md:239-241`, `:489`
- **Category:** safety · **Severity:** Medium
- **Tier:** 0
- **Must ship together with:** PM-2

**What:** `EMERG_DESIGN.md` requires a Cancel to be signed by the same sender as the original SOS. `classify_sos` implements only the 60-minute window. It never receives the original sender's identity and cannot compare it — the signature is verified as *valid* but nothing checks it belongs to the *same* identity.

**Evidence:** `classify_sos`'s full signature is `(payload: &[u8], now_unix: u64, original_timestamp: Option<u64>)` — no identity parameter exists. The `SosError` enum has no signer-mismatch variant.

**Root cause:** Same root cause as PM-2 — the engine-side ledger that would supply the original's identity does not exist.

**Fix:** Extend the ledger from PM-2 to store `(message_id → (timestamp, sender_id))`. Change `classify_sos`'s third parameter to `Option<OriginalSos { timestamp, sender_id }>`. Compare the cancel envelope's `sender_id` against the stored one; return `SosError::CancelSignerMismatch` on failure. Implement as one change with PM-2.

**Dependencies / blast radius:**
- Shares the ledger and `EmergencyProvider` seam change with PM-2 — implement as one change.
- A `cancel_forgery` test is required; the spec already calls for one and none exists.
- **Section 1 coordination:** compare sender identity using `identity::peer_short_from_sender` to normalise across 16-byte abbreviated and 32-byte full sender forms.
- **WARNING:** Today this is latent (masked by PM-2 dropping all cancels). Fixing PM-2 without PM-15 converts the latent hole into an exploitable one. These two must ship together.

---

## Tier 1 — High correctness + Medium safety/protocol

---

### PM-3 — Expired P0 messages are never released from the ACK tracker

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/ack.rs:183-188`, `:156-164`; retry loop `crates/iris-core/src/message_engine/mod.rs:1308-1344`
- **Category:** correctness · **Severity:** High
- **Tier:** 1

**What:** `AckTracker.pending` grows monotonically. Nothing removes an entry when a P0 message's TTL expires. The retry loop wakes, finds the expired P0 still "due", loads it from storage, calls `requeue_for_retry`, gets `TtlExpired`, discards the error (`let _ = ...`), leaves the entry in place, repeats every retry window forever.

**Evidence:** Probe: 200 retry windows on an expired P0 → `pending_len = 1`, `exhausted() = 0`, `due_retries()` still returns it at `now + 100_000 s`.

**Root cause:** Four independently-reasonable decisions compose into a leak: `max_attempts_remaining()` returns `1` for P0 so it never appears in `exhausted()`; `evict_expired` is contractually forbidden from evicting P0 (INV-ROUTE-003); `requeue_for_retry` error is discarded; GC never touches `acks`.

**Fix (two changes):**
1. Store the envelope's `timestamp` and `ttl_seconds` in `PendingAck` at `register_sent`. Have `due_retries` skip — and a new `drain_expired(now)` remove — entries past their TTL.
2. Stop swallowing the error: `if let Err(MsgEngineError::TtlExpired) = this.requeue_for_retry(env).await { this.acks.lock().await.forget(id); }`.
3. Add a hard cap on `pending` as defence in depth.

**Dependencies / blast radius:**
- `PendingAck` gains two fields; `register_sent` gains parameters (internal to this section).
- GC task should also call the new `drain_expired`.
- **Section 3 coordination:** the `evict_expired` P0 exemption (INV-ROUTE-003) is `iris-storage`'s contract and is *correct* — do not change it. Confirm with Section 3 that storage-side P0 retention is itself TTL-bounded.
- See also: PS-4 (ACK retry scheduling overhaul) — implement PS-4 together with this finding if Tier 3 has been gated open.

---

### PM-7 — The disaster-mode ladder cannot escalate

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/emergency/mode.rs:87-115`, `:53-68`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 1

**What:** `guarded_transition` implements only two transitions — `authority_deactivate → Normal` and `Degraded → Normal`. There is no `Normal → Emergency` or `Emergency → Crisis` path. `Triggers` struct and its three thresholds are defined but never read. `ESCALATION_HOLD_SECS` is used only on the de-escalation branch.

**Evidence:** `grep -rn "guarded_transition\|Triggers\|DisasterMode\|ModeTransition" crates/ --include=*.rs` returns, outside `mode.rs` itself, only the re-export line in `emergency/mod.rs:48` and an unrelated `AuditEvent::ModeTransition` variant. Nothing in the workspace calls any of it.

**Root cause:** Partial implementation. The module doc describes the full ladder; the tests only cover the two implemented transitions.

**Fix:** Add the escalation arm and take `Triggers` as a parameter. Escalate `Normal → Emergency` when any trigger crosses its threshold and the hold has elapsed; `Emergency → Crisis` on sustained trigger breach; add the `Crisis → Degraded` de-escalation step. Until engine wiring lands, at minimum make the function total with respect to its documented state machine.

**Dependencies / blast radius:**
- `EMERG_VERIFICATION.md`'s AC-7 row should be corrected to describe the actual state (currently overstates what `guarded_transition` implements).
- `ModeTransition` is re-exported from `emergency/mod.rs` — a signature change is technically a public-API change, but grep confirms no platform crate consumes it.

---

### PM-8 — Dedup rotation is driven by call count, so replay traffic shrinks the dedup window

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/dedup.rs:292-308`, `:177-183`; tombstone call `crates/iris-core/src/message_engine/mod.rs:533`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 1

**What:** `DedupEngine::seen()` unconditionally inserts into the current Bloom generation and increments `inserted`, even when the id is already known to be a duplicate. Generation rotation triggers on `inserted() >= capacity`. Duplicate traffic forces rotation without adding any information, discarding real dedup history early.

**Evidence:** Probe: replaying ONE id 1024 times into a capacity-256 engine → 3 full rotations (three generations of dedup history discarded by traffic that should have been a no-op).

**Root cause:** `inserted` was introduced to drive rotation (correctly). `BloomFilter::insert` increments the counter on every call regardless; `seen()` computes duplicate status *before* inserting but does not use it to gate the insert.

**Fix:** Skip the insert when the digest is already present in the current generation: `if !self.current.contains(digest) { self.current.insert(digest); }`. Makes `inserted` an approximate distinct-element count, which is what the rotation threshold assumes.

**Dependencies / blast radius:**
- `bloom_rotation_bounds_the_false_positive_rate` drives distinct ids and still passes; a new test should assert that replaying a single id does NOT rotate.
- **Section 3 coordination:** `routing/dedup_cache.rs` maintains a separate dedup structure — check whether it shares this counting bug (do not change it from here).

---

### PM-9 — The clock-skew fallback is dead code; skewed devices lose all traffic

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/expiry.rs:24-31`, `:69-113`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 1

**What:** Three pieces of the skew-handling design are unreachable: `is_expired_from_arrival` (never called outside its own tests), `expiry_reason` (never called outside its own tests), `ExpiryReason::ClockSkewSuspect` (never constructed anywhere). Every real caller uses `is_expired`, which fails closed — a skew-suspect timestamp is reported expired. A device with a genuinely wrong RTC has every message dropped at every hop.

**Evidence:** `grep -rn` across `src/` and `tests/`: `is_expired_from_arrival` appears only in its definition, its own doc comment, and two lines of its own unit test. `is_expired` is called at `mod.rs:532`, `:994`, and `:1376`. `is_expired` and `expiry_reason` currently disagree: `is_expired` says expired; `expiry_reason` says no reason to expire.

**Root cause:** `is_expired` was correctly changed to fail closed to kill a real immortal-message bug. The intended mitigation (`is_expired_from_arrival`) was written but never adopted by any caller.

**Fix:** Record an arrival instant when an envelope is first seen (`process_incoming` has a natural capture point). Switch the drop-path callers to `is_expired_from_arrival`. Bring `expiry_reason` back in line with `is_expired` and emit `ClockSkewSuspect` on that branch, or delete both the function and the variant if the reason is not wanted.

**Dependencies / blast radius:**
- `QueuedMessage` or the storage record gains an arrival field; `TtlCheck` should carry it too.
- **Section 3 coordination:** if arrival time is persisted, `iris-storage`'s schema is affected — agree the field with Section 3. `evict_expired(before_unix)` makes a wall-clock judgement and may need the same treatment.

---

### PM-10 — Fragment chunk sizing overrides the MTU and ignores envelope overhead

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:1096-1107`
- **Category:** correctness · **Severity:** Medium
- **Tier:** 1
- **See also:** PS-7 (the API-level structural fix that makes this unrepresentable)

**What:** The chunk budget is `max_mtu - FRAGMENT_HEADER_LEN`, then floored at 64 with `.max(64)`. Two problems: (1) when `max_mtu < FRAGMENT_HEADER_LEN + 64`, `.max(64)` overrides the MTU and produces a fragment of 150 bytes regardless of transport capacity; (2) the budget subtracts only the fragment header, not the CBOR envelope encoding (~200 bytes), so emitted fragments systematically exceed `max_mtu`.

**Evidence:** Code analysis. `FRAGMENT_HEADER_LEN = 86` (`fragment.rs:90`). `lora.rs:1227` reports `max_message_size: MAX_PAYLOAD_BYTES` (237) while `internet.rs:50` reports `max_message_size: MAX_FRAME_BYTES` (1 048 576) — confirming the two transports report different quantities through the same field.

**Root cause:** The size check at the top of the function uses the fully-encoded envelope (`codec::encode(envelope)?.len() <= max_mtu`), which is correct. But the *split* budget is computed against the raw payload with only the fragment header subtracted — two different accounting systems.

**Fix:** Compute the budget from a *measured* envelope overhead: build the fragment envelope once with an empty chunk, encode it, use `max_mtu - encoded_len` as the chunk budget. If the resulting budget is non-positive, return `BadEnvelope` explicitly. Ideally implement as part of PS-7 (change `serialize_for_transport` to return `Vec<Vec<u8>>`).

**Dependencies / blast radius:**
- More payloads will legitimately exceed the two-fragment limit on small-MTU links — confirm against `MSG_DESIGN.md` R8.
- **Section 3 coordination (most important):** `max_message_size` is reported inconsistently: `lora.rs` gives a payload ceiling (237), `internet.rs` gives a frame total (1 MB). Must be reconciled before any MTU-budget fix is correct.

---

### PM-12 — The P0 envelope size limit is enforced only in tests

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/protocol/envelope.rs:9`; decode `crates/iris-core/src/protocol/codec.rs:459-579`
- **Category:** protocol · **Severity:** Medium
- **Tier:** 1

**What:** `P0_MAX_ENVELOPE_BYTES = 255` is asserted in three unit tests and one golden-vector test but never enforced on the decode path. An oversized P0 SOS (up to 1 MB on internet transport) is accepted, queued, and stored. P0 status confers: exempt from general rate limiter, `max_attempts: None`, never evicted from storage (INV-ROUTE-003), top-of-heap in priority queue, and (per PM-3) never released from the ACK tracker. An oversized P0 is a permanent, unmetered, un-evictable resource commitment that will never traverse LoRa.

**Evidence:** `grep -rn "P0_MAX_ENVELOPE_BYTES"` across `src/` and `tests/` returns the declaration, one `lora.rs` comment, three assertions inside `codec.rs`'s own `#[cfg(test)]` module, and one in `golden_vectors.rs`. No non-test enforcement site.

**Root cause:** Same encode/decode asymmetry as PM-5. The constant exists and is tested on the encode side; not enforced on the receive side.

**Fix:** Enforce in `codec::decode`: if the decoded envelope is the P0/SOS form, reject when the input slice exceeds `P0_MAX_ENVELOPE_BYTES`. Also add a general per-`ContentType` maximum payload table — `content_type.rs` documents intended ceilings in doc comments but none are enforced.

**Dependencies / blast radius:**
- Golden corpus must be checked to confirm no fixture exceeds the new limits.
- **Section 3 coordination:** transport frame caps (`lora.rs` 255, `internet.rs` 1 MB) are correct and are not a replacement for this protocol-level invariant — agree the layering.

---

### PM-14 — Spec, module doc, and implementation disagree on the signing scope

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/protocol/codec.rs:14-22` (module doc), `:145-154` (build_map doc), `:185-220` (implementation); `docs/protocol/MESSAGE_ENVELOPE.md:128`
- **Category:** protocol · **Severity:** Medium
- **Tier:** 1

**What:** Three sources give three different answers to "what does the Ed25519 signature cover?": `MESSAGE_ENVELOPE.md:128` says "all fields except field 15"; `codec.rs` module doc says fields 1–7 and 9–14 (fields 16+ unsigned); implementation covers 1–7, 9–14, **16**, **18** (excluding 8, 15, 17). The module doc actively describes the exploitable pre-fix design. The spec is wrong in a way that breaks interoperability.

**Evidence:** Direct comparison of the three sources. `build_map`'s own doc comment (`codec.rs:149-151`) claims `Scope::Signing` covers "fields 1–7, 9–14, 16, 18" while the module doc twenty lines earlier says 16+ are not covered — the file contradicts itself internally.

**Root cause:** Fields 16 and 18 were moved into the signing scope by a security fix (inline comment at `codec.rs:199-211` explains it well). The fix updated the inline comment but not the module doc.

**Fix:** Update the `codec.rs` module doc and the `build_map`/`encode_for_signing` doc comments to state the real scope (1–7, 9–14, 16, 18; with one-line reasons for each exclusion). Raise a spec correction for `MESSAGE_ENVELOPE.md:128`. Add a test that asserts the scope by construction — mutate each field on a signed envelope and assert verification fails for signed fields and succeeds for unsigned ones.

**Dependencies / blast radius:**
- No runtime change.
- `MESSAGE_ENVELOPE.md` and `ADR-0011` may both need amendment — raise with spec owner, do not silently edit.
- **Sections 4/5/6 coordination:** confirm no platform crate re-implements envelope signing from the spec (they appear to delegate to `iris-core`).

---

## Tier 2 — Medium/Low correctness, performance, protocol

---

### PM-11 — Duplicate dedup hits are O(capacity) on the receive hot path

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/dedup.rs:212-233`
- **Category:** performance · **Severity:** Medium
- **Tier:** 2

**What:** `LruCache::check_and_insert` re-arms recency on a duplicate by linear-scanning a `VecDeque` for the id and then removing from the middle — both O(n) in `LRU_CAPACITY` (10 000). In a flood-routed mesh, the duplicate branch is the *common* path. The call sits inside `self.dedup.lock().await` on the receive path (`mod.rs:548`), serialising all inbound processing. Tuning up `LRU_CAPACITY` to reduce false positives makes throughput worse.

**Evidence:** Probe: 2000 worst-case duplicate hits @cap=1000 → 1156 µs; @cap=10000 → 8183 µs. Scaling factor 7.1× for 10× capacity.

**Fix:** Replace the `HashSet` + `VecDeque` pair with an O(1) LRU — either the generational approach already used for the Bloom tier (two `HashSet`s, swap and clear when the current one fills, matching the existing `DedupEngine` idiom), or an intrusive `HashMap<MessageId, NodeIndex>` over a doubly-linked list.

**Dependencies / blast radius:**
- `lru_evicts_oldest` and `lru_rearms_recency` assert strict LRU ordering; a generational cache provides approximate rather than exact recency — re-specify against the weaker (but sufficient) guarantee.
- Confirm against `DEDUPLICATION.md` that exact LRU ordering is not itself a requirement.

---

### PM-13 — Queue depth is bounded by message count, not bytes; fairness gate is O(n)

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/queue.rs:125-128`, `:147-201`; config `crates/iris-core/src/message_engine/mod.rs:96`
- **Category:** performance · **Severity:** Medium
- **Tier:** 2

**What:** Two related issues: (1) `max_queue_depth` is 50 000 messages with no byte accounting — worst-case memory ~50 GB. Storage has a byte quota; the in-memory queue does not. (2) `dequeue()`'s fairness gate drains the entire `BinaryHeap` into a `Vec`, partitions into two more `Vec`s, does a linear `max_by`, removes an element, re-extends the heap — O(n) with several allocations every `p0_budget = 60` dispatches. `remove_by_id` and `drain_expired` use the same full drain-and-rebuild pattern.

**Fix:**
1. Track `queued_bytes` alongside `len()`, reject in `push` when either count or a new `max_queue_bytes` limit is exceeded.
2. Replace the single heap with an array of eight FIFO deques indexed by priority. Dequeue is O(1), fairness gate is O(1), `drain_expired`/`remove_by_id` become per-bucket operations.

**Dependencies / blast radius:**
- `QueuedMessage`'s `Ord`/`PartialEq` impls become unnecessary if bucketed deques replace the heap.
- The `delivery_prob` tiebreak would need an explicit decision (see PM-17, which argues it is attacker-influenced anyway).
- **Sections 4/5/6 coordination:** adding `max_queue_bytes` to `MessageEngineConfig` is source-compatible via `Default`, but confirm no platform crate constructs the struct literally without `..Default::default()`.

---

### PM-16 — `split_payload` loops forever on a zero chunk budget

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/fragment.rs:60-72`; proof `crates/iris-core/src/kani_proofs.rs:79-107`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2

**What:** With `max_chunk == 0` and a non-empty payload, the loop makes no progress — `take = rest.len().min(0) = 0`, an empty `Vec` is pushed, `rest = &rest[0..]` leaves `rest` unchanged. Spins forever while growing `out` without bound.

**Evidence:** Probe: `split_payload(b"abc", 0)` on a worker thread with a 600 ms timeout → thread never returned (`hung = true`). The Kani harness explicitly assumes this away: `kani::assume(max_chunk >= 1 && max_chunk <= 16)`.

**Root cause:** The loop's progress condition depends on `max_chunk >= 1` — precondition is neither documented nor checked. The one in-tree caller guards with `.max(64)` (`mod.rs:1102`), but PM-10's recommended fix removes that guard.

**Fix:** Make the precondition explicit: take a `NonZeroUsize`, or clamp internally with `let max_chunk = max_chunk.max(1);`. Widen the Kani harness to drop the lower-bound assumption so termination at zero is actually proved.

**Dependencies / blast radius:**
- If `NonZeroUsize` is chosen, the call site in `serialize_for_transport` changes — coordinate with PM-10's fix (same line).
- **Unverified:** cannot run Kani on Windows; re-confirm on Linux CI that the harness's assumption is what this finding claims.

---

### PM-17 — `delivery_prob` accepts NaN/±∞ from an untrusted relay and wins every queue tiebreak

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/protocol/codec.rs:414-419`; consumption `crates/iris-core/src/message_engine/queue.rs:47-51`, `:93`
- **Category:** protocol · **Severity:** Low
- **Tier:** 2

**What:** `decode_routing_hints` accepts any `Value::Float` for `delivery_prob` with no range validation. `QueuedMessage::new` reads it straight into the queue comparator, where `total_cmp` orders NaN above +∞ and thus above every finite value. Any on-path relay can set `delivery_prob = NaN` and win every FIFO tiebreak within a priority class.

**Fix:** Validate at the decode boundary: reject the envelope with `EnvelopeError::InvalidField("routing_hints.delivery_prob")` when the value is not finite or outside `0.0..=1.0`. Validate in the codec (not at the queue) so every consumer of the field gets the guarantee.

**Dependencies / blast radius:**
- **Section 3 coordination (important):** `routing/prophet.rs` maintains delivery-predictability values; if any Section 3 code ingests the wire `delivery_prob` into PRoPHET's probability arithmetic, an unvalidated NaN would propagate into routing decisions. Section 3 should check — validating at the codec boundary protects them either way.

---

### PM-18 — SOS allowance check and record are not atomic

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:1509-1515`; provider `crates/iris-core/src/emergency/provider.rs:157-165`
- **Category:** correctness · **Severity:** Low
- **Tier:** 2

**What:** `emergency_gate` calls `provider.sos_rate(...)` and then `provider.sos_record(...)` as two separate mutex acquisitions. Two concurrent SOS messages from the same sender can both observe `Allowed` before either records, and both keep P0. The allowance is a soft limit rather than the hard 3/hour the spec states.

**Evidence:** Code analysis of the two adjacent call sites and the two independent `self.limiter.lock()` acquisitions backing them. `BroadcastReplayGuard::check_and_record` (`provider.rs:284`) demonstrates the correct single-lock pattern in the same file.

**Fix:** Collapse the two into one trait method — `sos_check_and_record(&self, sender_id, now_unix) -> RateLimitDecision` — that takes the lock once and performs both under it. Also removes a redundant second `touch()` call, helping PM-6's cost profile.

**Dependencies / blast radius:**
- `EmergencyProvider` trait signature changes — both implementations (`NoopEmergencyProvider`, `EmergencyGateway`) need updating. Engine-internal, no platform crate impact.

---

## Tier 3 — Structural Enhancements (GATED — human go-ahead required)

---

### PS-1 — Outbound delivery is a fixed-rate poll that dispatches one message at a time

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:1142-1154` (`spawn_delivery_loop`); `:1233-1254` (`deliver_outbound`)
- **Category:** architecture · **Priority:** Blocking for scale
- **Tier:** 3

**What:** Hard throughput ceiling of `1 / poll_interval = 20 msg/s` independent of queue depth. Head-of-line blocking on the slowest transport. Multipath P0 emergency fan-out pays the *sum* of all transport latencies instead of the *minimum*.

**Measured:** 40-message backlog, instant transport → 15.5 msg/s against a 20/s theoretical ceiling.

**Fix:** Replace the poll with `tokio::sync::Notify` signalled by every `queue.push` site. Replace the inline `await` with a bounded worker pool (`tokio::sync::Semaphore`, ~16 permits). Fan out across `ranked` transports with `futures_util::future::join_all` so multipath latency becomes `max(...)` not `sum(...)`. Keep per-transport in-flight cap so one radio cannot monopolise permits.

**Dependencies / blast radius:**
- **Section 3 (blocks PS-1):** concurrent `send()` calls to the same transport are only safe if implementations are internally synchronised. Per-adapter answer needed for BLE, LoRa, Wi-Fi Direct, Wi-Fi Aware, satellite, internet.
- **Section 3 (LoRa duty cycle):** concurrent dispatch must not defeat the duty-cycle bound in `transport/lora.rs`.
- Engine tests that rely on timing need to await conditions rather than fixed sleeps.
- `MessageEngineConfig` gains a concurrency limit.

---

### PS-2 — Envelope decode builds a throwaway `Value` tree with 29 linear-scan field lookups

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/protocol/codec.rs:459-579`
- **Category:** architecture · **Priority:** Blocking for scale
- **Tier:** 3

**What:** `codec::decode` parses into a `ciborium::value::Value` tree, then extracts each field through 29 independent linear scans. Byte fields are copied twice (once into the `Value` tree, once into the `Envelope`). The path is *uncapped* — inbound processing is driven directly by transport streams, not the 20/s poll loop. Flood traffic pays full decode before dedup rejects it.

**Measured:** `codec/envelope_decode_222B = 2.22 µs` vs `codec/envelope_encode_222B = 932 ns` (2.4× slower to decode same structure).

**Fix:** Single-pass drain of the map into a field array using `std::mem::take` on owned `Value::Bytes(v)` to eliminate the second copy. Track seen-required-fields in a small bitmask.

**Dependencies / blast radius:**
- Entirely within `protocol/codec.rs`; public signature does not change.
- `golden_vectors` and `protocol_conformance` are the regression gate.
- Duplicate-key handling changes (first vs. last match on malformed input) — decide explicitly.

---

### PS-3 — Authority chains are re-transmitted and re-verified on every alert

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/emergency/authority.rs`, `broadcast.rs`, `provider.rs`
- **Category:** architecture · **Priority:** High
- **Tier:** 3

**What:** Every `EmergencyAlert` carries its full authority chain inline in field 18. A 2-element chain → 690 B, a 4-element chain → 1277 B, against BLE MTU of 512 B. `EmergencyAlert` is explicitly non-fragmentable. An authority emergency broadcast structurally cannot traverse a BLE-only link at any chain depth.

**Fix:** Add a `VerifiedChainCache` keyed by `authority_peer_short`. On a hit, skip chain re-verification. Make field 18 optional on the wire for alerts (dropping ~195 B per chain element). Distribute chains proactively over discovery handshakes.

**Dependencies / blast radius:**
- **Section 1 (blocks PS-3):** cache stores the result of Section 1's verification. Section 1 must specify how revocation invalidates a cached verification result before this ships.
- **Section 3:** if chain distribution gets its own content type, it must be fragmentable and routable.
- `EMERG_DESIGN.md` §4 known-limitation entry substantially revised.

---

### PS-4 — ACK retry scheduling scans every pending message once per second

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/ack.rs:156`; `crates/iris-core/src/message_engine/mod.rs:1308-1363`
- **Category:** architecture · **Priority:** Would help, not urgent
- **Tier:** 3

**What:** `spawn_ack_task` wakes every second and runs two full-map scans, allocating two fresh `Vec`s per tick, whether or not anything is due. P4–P7 retry timeouts are 600 s to 3600 s; expected due entries per tick is near zero while the scan visits every entry. 86 400 full scans per day to discover almost always nothing. Compounds PM-3's leaked entries.

**Fix:** Make the retry schedule a priority queue (`BinaryHeap<Reverse<(Instant, MessageId)>>`). Task sleeps until the actual head deadline. Handle stale heap entries lazily. `exhausted` stops being a scan — move to a `failed` list at the moment `record_retry` crosses its cap.

**Dependencies / blast radius:**
- Implement together with PM-3's TTL-expiry fix; both touch `PendingAck`'s fields and lifecycle.
- `AckTracker` entirely internal, no cross-boundary coordination.

---

### PS-5 — Discovery scans transports serially inside an interval shorter than the worst case

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/discovery/mod.rs:187-221`
- **Category:** architecture · **Priority:** Would help, not urgent — verify premise first
- **Tier:** 3

**What:** `scan_once` loops over transports serially, each able to consume the full 10 s timeout. Six transports × 10 s = 60 s worst case inside a 30 s interval. Effective interval silently becomes "however long the slowest radios took." Two nodes passing within range for 20 s may never discover each other.

**Fix:** Replace the serial `for` with `futures_util::future::join_all` over per-transport scan futures. Wrap each in `tokio::time::timeout(scan_timeout, ...)`. Round takes `max(per-transport)` rather than `sum(...)`, bounded at 10 s.

**Dependencies / blast radius:**
- **Section 3 (verify before implementing — blocks PS-5):** whether concurrent scans contend for shared radio hardware or OS scan arbiter. BLE and Wi-Fi commonly share a physical radio on real phones — concurrent scans may serialise anyway or interfere. Safe intermediate: concurrency within radio-independent groups only.
- **Section 3:** concurrent scanning interleaves `TopologyEvent`s that are currently grouped per transport — confirm routing does not depend on that grouping.

---

### PS-6 — The emergency audit "ring" is a shifting `Vec` with O(n) append at steady state

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/emergency/audit.rs:59-78`
- **Category:** architecture · **Priority:** Would help, not urgent
- **Tier:** 3

**What:** `AuditLog` documents itself as "an in-memory ring" but uses `Vec::drain(..overflow)` — a full-buffer memmove per append at steady state. Cost is O(capacity). Audit rows are written on exactly the paths that spike during an incident, while holding the `EmergencyGateway` audit mutex.

**Measured:** 20k appends @cap=256: 2489 µs (0.12 µs/append); @cap=4096: 68 447 µs (3.42 µs/append) — scaling 27.5× for 16× capacity. `VecDeque` would make this ~30 ns/append at any capacity (100× improvement at 4096 capacity).

**Fix:** Use `VecDeque<EmergencyAuditRecord>` with `push_back` and `pop_front`. `snapshot()` returns `&[T]` — call `make_contiguous()` in snapshot (a diagnostics/UI call, not a hot path).

**Dependencies / blast radius:**
- Only `emergency/audit.rs` and callers of `snapshot()` (`EmergencyGateway::audit_snapshot` at `provider.rs:135`).
- `ring_bounds_capacity` test asserts eviction order and should pass unchanged.
- `snapshot`'s return type change — `make_contiguous()` requires `&mut self`.

---

### PS-7 — `serialize_for_transport` returns `Envelope`s, forcing triple encode per outbound message

- **Fix status:** ⬜ Not started
- **File(s):** `crates/iris-core/src/message_engine/mod.rs:1091`, `:1188`, `:1239`
- **Category:** architecture · **Priority:** Would help, not urgent — implement as part of PM-10's fix
- **Tier:** 3

**What:** `deliver_outbound` encodes to measure, calls `serialize_for_transport` which encodes again to check fit, then encodes each returned envelope a third time to actually send. The return type (`Vec<Envelope>`) is the root problem — handing back structures when the caller needs bytes.

**Measured:** encode(222 B envelope) = 843 ns; clone(222 B envelope) = 115 ns; redundant work floor ≈ 1.8 µs/message.

**Fix:** Change `serialize_for_transport` to return `Vec<(MessageId, Vec<u8>)>` — encoded frames ready to send. The fragmentation budget is then computed from the *encoded fragment size* directly, making PM-10's bug unrepresentable by construction.

**Dependencies / blast radius:**
- Implement as part of PM-10's fix — they touch the same lines; doing PM-10 without this leaves the same shape in place.
- Fragment signing must remain sign-then-encode in that order.
- **Section 3 coordination:** `TransportCapabilities::max_message_size` inconsistency (`lora.rs` payload ceiling vs `internet.rs` frame total) must be resolved first.

---

## Cross-section coordination summary

| Finding | Section | What they must decide |
|---|---|---|
| PM-1 | Sec 1 | Whether any `SecurityPolicy` impl keys off `envelope.priority` after the emergency gate. |
| PM-2, PM-15 | Sec 1 | Same-signer comparison should reuse `identity::peer_short_from_sender`. |
| PM-4 | Sec 1 | Confirm rate-limit reordering does not weaken emergency ACL exemption. |
| PM-3 | Sec 3 | Confirm storage-side P0 retention is itself TTL-bounded (INV-ROUTE-003 correct, do not change). |
| PM-5 | Sec 3 | Agree `NeighborTable` capacity cap and eviction key (routing consumes this). |
| PM-8 | Sec 3 | Check `routing/dedup_cache.rs` for the same count-vs-distinct rotation bug. |
| PM-9 | Sec 3 | If arrival time persisted, `iris-storage` schema changes; `evict_expired` also affected. |
| PM-10 | Sec 3 | **Most important:** `max_message_size` inconsistency between `lora.rs` (237) and `internet.rs` (1 MB) — must be reconciled before any MTU fix is correct. |
| PM-12 | Sec 3 | Protocol-level P0 size invariant is additive to transport frame caps — agree layering. |
| PM-17 | Sec 3 | Whether `routing/prophet.rs` ingests wire `delivery_prob` into PRoPHET arithmetic. |
| PM-13 | Sec 4/5/6 | Confirm no platform crate constructs `MessageEngineConfig` without `..Default::default()`. |
| PM-14 | Sec 4/5/6 | Confirm no platform crate re-implements envelope signing from the spec. |
| PS-1 | Sec 3 | Whether transport adapters are re-entrant under concurrent `send()` — per-adapter answer required. Also LoRa duty-cycle bounds. |
| PS-3 | Sec 1 | How revocation and key rotation invalidate a cached `VerifiedAuthority` — **blocks PS-3**. |
| PS-5 | Sec 3 | Whether concurrent discovery scans contend for shared radio hardware — **verify before implementing**. |

---

## Appendix — Finding severity reference

| ID | Category | Severity | One-line description |
|---|---|---|---|
| PM-1 | safety | Critical | SOS P3 downgrade mutates signed field → dropped at next hop |
| PM-2 | safety | Critical | `classify_sos` always called with `original_timestamp: None` → every CANCEL rejected |
| PM-3 | correctness | High | TTL-expired P0 never leaves `AckTracker` — unbounded growth + eternal retry |
| PM-4 | security | High | Fragment reassembly before rate limit, no per-sender quota |
| PM-5 | security | High | `CapabilityBundle::decode` ignores 256 B budget; `NeighborTable` uncapped |
| PM-6 | security | High | `SenderWindow.timestamps` unbounded → O(n²) CPU, unbounded memory per sender |
| PM-7 | correctness | Medium | `guarded_transition` has no escalation path; `Triggers` never read |
| PM-8 | correctness | Medium | Bloom rotation counts duplicate calls → replay traffic shrinks dedup window |
| PM-9 | correctness | Medium | `is_expired_from_arrival`, `expiry_reason`, `ClockSkewSuspect` all dead code |
| PM-10 | correctness | Medium | `.max(64)` overrides MTU; envelope overhead not subtracted from chunk budget |
| PM-11 | performance | Medium | `LruCache` duplicate hits are O(capacity) — 7.1× slowdown for 10× capacity |
| PM-12 | protocol | Medium | `P0_MAX_ENVELOPE_BYTES` asserted in tests only, never enforced |
| PM-13 | performance | Medium | Queue depth bounded by count not bytes; fairness gate O(n) drain-and-rebuild |
| PM-14 | protocol | Medium | Spec, module doc, implementation disagree on signing scope |
| PM-15 | safety | Medium | AC-6 same-signer for SOS CANCEL unimplemented (latent — masked by PM-2) |
| PM-16 | correctness | Low | `split_payload` loops forever on `max_chunk == 0` |
| PM-17 | protocol | Low | `delivery_prob` accepts NaN/±∞ → wins every queue tiebreak |
| PM-18 | correctness | Low | `sos_rate` + `sos_record` are two separate lock acquisitions — race on allowance |
| PS-1 | architecture | — | Fixed-rate delivery poll (20 msg/s ceiling, serial multipath) |
| PS-2 | architecture | — | Decode builds throwaway Value tree, 29 linear scans, double-copy |
| PS-3 | architecture | — | Authority chains inline in every alert — cannot traverse BLE at any depth |
| PS-4 | architecture | — | ACK retry scans full map every second; 86 400 mostly-empty scans/day |
| PS-5 | architecture | — | Discovery scans transports serially (sum of timeouts vs. max) |
| PS-6 | architecture | — | Audit "ring" is shifting Vec — O(n) memmove per append at steady state |
| PS-7 | architecture | — | `serialize_for_transport` returns Envelopes — triple encode per outbound message |
