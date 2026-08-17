# SEC-001 Security Review — Redteam SEC-RT Findings & Dispositions

**Authority**: AC-16 (Security_Agent review + log hygiene)
**Stage**: TEST / SECURITY_REVIEW (iter 79; AC-14 coverage pass iter 80, AC-15 deviation ratified iter 82, AC-16 finalized iter 81)
**Date**: 2026-08-16
**Verdict**: PASS-WITH-RECORDED-DEVIATIONS (all in-scope findings FIXED in-pass; AC-15 gated deviation operator-ratified; independent verifier iter 82 **APPROVE_WITH_NOTES** — reconciled in SEC_001_VERIFICATION.md)

---

## Summary

Adversarial review of the entire `crates/iris-core/src/security/` module plus
its `message_engine` integration points (incl. the iter-78 fuzz-discovered
quota overflow fix). **12 findings** — 2 CRITICAL / 8 HIGH / 2 MEDIUM —
**all fixed in-pass** with regression tests (iter 79). The iter-80 AC-14
coverage pass discovered **+1 real defect, SEC-RT-15 (HIGH)**, fixed with a
regression test. Zero CRITICAL/HIGH findings remain open. 3 test-harness
defects (proptest time-dependence) corrected.

## Findings Table

| ID | Severity | Location | Finding | Disposition |
|----|----------|----------|---------|-------------|
| SEC-RT-01 | CRITICAL | `security/replay.rs` | tokio `RwLock` self-deadlock: `check()` held `highwater.write()` then called `maybe_schedule_snapshot` which re-took `highwater.read()` (RwLock is NOT reentrant). | FIXED: compute `accepted` in a block-scoped write guard, **drop guard**, then schedule; signature → `maybe_schedule_snapshot(&self)`. |
| SEC-RT-02 | CRITICAL | `security/acl.rs` | `check_authority_chain` never called the chain crypto verifier (no `verify_chain`) and treated `chain.last()` as root (RED-0008 root = `element[0]`). | FIXED: calls `identity::chain::verify_chain(chain_raw, &trust_store, &sender_id)` + requires decoded `element[0]` root allowlisted + `is_authority_root`; `verify_chain_against_allowlist` removed. |
| SEC-RT-03 | HIGH | `security/replay.rs` | Future-poison: attacker `(ts=now+599, seq=u64::MAX)` stored **unclamped** poisoned the sender high-water mark for ~10 min. | FIXED: `stored_ts = ts.min(now)` before `check_and_advance`. |
| SEC-RT-04 | HIGH | `message_engine/mod.rs` | Quota TOCTOU: `check_quota` then `add_message` with the decision discarded → concurrent sends could exceed quota. | FIXED: `add_message` is the single authoritative gate; `remove_message` refund on enqueue-full/persist-fail; returns `Err(MsgEngineError::Storage)`. |
| SEC-RT-05 | HIGH | `message_engine/mod.rs` | Quota never released at terminal points (messages left storage consumed quota forever). | FIXED: `remove_message` refunds at TTL-expiry, requeue budget-exhaust, ack-task exhausted loop, acknowledge (`PriorityQueue::remove_by_id` added). |
| SEC-RT-06 | HIGH | `security/spam.rs` | `sender_stats` map unbounded (per-sender state grows without limit). | FIXED: `SpamConfig.max_sender_stats` (default 100_000) + evict-one at cap. |
| SEC-RT-07 | HIGH | `security/reputation.rs` | Second-hand positives propagated without a verified-sender gate. | FIXED: `verified_peers` registry + `MAX_VERIFIED_VOUCHERS=64`; `reset()` clears both. |
| SEC-RT-08 | HIGH | `security/acl.rs` | SOS identity gate wrong when store populated: `Unknown|Unverified` was authorized unconditionally. | FIXED: authorized **only when** `TrustStore::is_empty()` (true Noop), else `Unauthorized`; `TrustStore::is_empty()` added. |
| SEC-RT-09 | HIGH | `security/rate_limiter.rs` | Unknown-sender aggregate bucket capped at *per-sender* burst, not `unknown_sender_burst`. | FIXED: `refill(burst)` param; `check_unknown` passes `unknown_sender_burst`. |
| SEC-RT-11 | HIGH | `security/rate_limiter.rs` | Div-by-zero panic: `refill_interval.as_secs()` when interval < 1 s. | FIXED: `.max(1)` guards in `check_bucket` + `refill`. |
| SEC-RT-12 | MEDIUM | `security/quota.rs` | `freed = freed + to_evict` could overflow in extreme accounting states. | FIXED: `freed.saturating_add(to_evict)`. |
| SEC-RT-14 | MEDIUM | `security/replay.rs` | Future-poison regression coverage. | FIXED: `replay_future_poison_is_bounded_not_persistent` (crosses a 2nd boundary with sleep). |
| SEC-RT-15 | HIGH | `security/acl.rs` | **Authorized-chain path UNREACHABLE**: `check_authority_chain` decoded the root element with `ciborium::de::from_reader::<KeyAdvertisementV1>`, which **always errors** against the wire bytes (wire form = CBOR sequence + appended 64B signature; the full-struct decoder expects a map). Effect: any valid allowlisted chain returned `InvalidAuthority` — emergency senders could never be authorized even when correctly provisioned (functionality/availability regression, fail-closed). | FIXED (iter 80): canonical `KeyAdvertisementV1::from_bytes(root_bytes)`; regression `acl_authorized_with_valid_authority_chain`. |

## Iter-80 Coverage-Pass Defect Discovery (SEC-RT-15)

Discovered **not by fuzzing but by writing an authorized-chain coverage test**
(from the missing `acl_authorized_with_valid_authority_chain` case). A
temporary debug test (`crates/iris-core/tests/zz_acldebug_tmp.rs`, removed
after use) proved it empirically: `KeyAdvertisementV1::from_bytes(wire)` = OK,
`ciborium::de::from_reader::<KeyAdvertisementV1>(wire)` = Err
(`invalid type: sequence, expected a map`). Lesson recorded: **coverage tests
are also adversarial** — forcing a path to be covered surfaces dead/broken
paths the happy-path suite never touches.

## Test-Harness Corrections (not engine defects)

- 3 replay proptests sampled `base_ts` into the future region
  (`..=unix_now()+500`) which became time-dependent under the `ts.min(now)`
  clamp → re-pinned to present/past windows
  (`(unix_now()-3600)..=unix_now()` / `..=(unix_now()-60)`).

## Regression Evidence

- `replay_highwater_in_order` (3rd case updated to `(now+1, 3)` — a
  strict-monotonic violation under clamping).
- `replay_future_poison_is_bounded_not_persistent` (NEW).
- `SecurityPolicy::FullSecurityPolicy` fuzz target still green over all 6 engines.

## Verification

- `cargo test --workspace --all-features` = **532 passed / 0 failed / 1 ignored**
  (iris-core **477** incl. security + proptest suites; iter-81 honest refresh —
  supersedes the iter-79/80 "499/444" seed recorded before the 40+ coverage tests).
- `cargo test -p iris-core --features proptest --lib security::` = **101 passed** (68 unit + 33 property functions; verifier-reproduced iter 82).
- `cargo test -p iris-core --lib security::` = **68 passed** (iter 80).
- `cargo clippy --workspace --all-features --tests` = **0 warnings** (verifier-reproduced iter 82).
- `cargo build --workspace` clean.
- **Independent verifier (iter 82): APPROVE_WITH_NOTES** — workspace 532/0/1 reproduced, tarpaulin cobertura present with security line-rate 0.95624, all 13 SEC-RT fix rows + iter-78 CAS fix verified as real code with passing regression tests, every sampled AC test-name exists (2 exact / 1 rename / 5 behavior-equivalent); notes reconciled in `SEC_001_VERIFICATION.md` (101-suite count, 33-property-fn phrasing, aggregate 77.44%).
- **AC-14 tarpaulin (iter 80)**: security module **590/617 = 95.6%** (target
  >=95% MET), overall 77.14% (4194/5437); per-file acl 95.6%, mod.rs 74.7%,
  rate_limiter 98.0%, quota/replay/reputation/spam 100%. Evidence:
  `%TEMP%\opencode\tarpaulin-ac14c\cobertura.xml`.
- Prior FUZZ evidence (iter 78): 19.4 M executions clean (quota overflow found+fixed).
- **AC-16 log-hygiene grep guard (iter 81)**: `cargo test -p iris-core --lib
  security:: 2>&1 | grep -E "payload|signature|secret"` → the only match is the
  test identifier `test security::spam::tests::spam_small_payload_clean ... ok`
  (**no log line emits payload/signature/secret content**). Guard CLEAN.

## Recorded Deviations / Known Limitations (deferred, intentional)

- **AC-14 known_limitations (iter 80)**: the remaining uncovered lines are NOT
  genuinely coverable — acl.rs 211/212/218/219 (defensive branches unreachable
  after `verify_chain` success), mod.rs ~21 lines (async_trait default trait
  method bodies — exercised by `noop_all_default_methods_are_permissive` but
  tarpaulin attributes hits elsewhere: measurement artifact), rate_limiter.rs
  46 (unreachable `_ => Unknown` arm — MessagePriority is P0..P7 only) and 183
  (unreachable `_ => {}`).
- **AC-14 MET**: 590/617 = 95.6% >= 95% target (measured honestly iter 80).
- **AC-15 CLOSED via RECORDED GATED DEVIATION (operator-ratified 2026-08-16)**: literal >=24h aggregate wall-clock fuzz not reached; per AC-15 text ("…or **recorded gated deviation with property-based adversarial suite**") the substitute is the **property-based adversarial suite** (**68 unit + 33 property functions across all 6 engines** — `--features proptest --lib security::` = **101 passed**) PLUS accumulated clean fuzz: **36 M+ executions** (19.4 M iter 78 + 16.9 M+ iter-80/81 background run) that already found+fixed **2 real engine bugs** (quota overflow/wraparound bypass, acl Noop violation). Fuzz log: `%TEMP%\opencode\fuzz-ac15-iter80.log`. Background accumulation continues for the record.
- Log-hygiene CI grep guard: **CLEAN (iter 81)** — `cargo test -p iris-core --lib security:: | grep -E "payload|signature|secret"` → only test-NAME match `spam_small_payload_clean`; no log line leaks payload/signature/secret.
- Reputation is routing-weight-only by design (DEC-SEC-0004); never an
  admission gate (AC-9 regression test in place).
- Sybil defeatability: per-identity limits are necessary-not-sufficient
  (RES-0018; Douceur IPTPS 2002) — recorded, not a SEC-001 blocker.