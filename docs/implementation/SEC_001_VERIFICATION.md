# SEC-001 Verification (VERIFY stage)

- **Node**: SEC-001 — General Security Hardening (P0 SECURITY, 4th P0 SECURITY node)
- **Stage**: VERIFY — acceptance evidence for ACCEPT
- **Version**: v1.0
- **Date**: 2026-08-16
- **Precedents**: EMERG_VERIFICATION.md v1.0 (AC-1..14 PASS), IDENT_VERIFICATION.md
  v1.0 (AC-1..11 PASS), CRYPTO_VERIFICATION.md v1.0 (AC-1..9 PASS)

## Acceptance-criteria outcome

**All 16 acceptance criteria PASS** (AC-1..13 verified in TEST stage
`SEC-001_TEST.md`; AC-14 re-measured honestly iter 80; AC-15 operator-ratified
gated deviation iter 82; AC-16 finalized iter 81).

Honest-evidence rule honored throughout: the SEC-001 history includes a state
correction (iter 77 — module did not compile despite claimed "464 green") and
every figure (tests / coverage / fuzz / clippy) below was **re-verified live in
this pass or the immediately preceding pass with recorded evidence**; no
unverified claim enters this record.

## AC → evidence table

| AC | Criterion (from PROJECT_GRAPH.yaml) | Evidence | Status |
|----|--------------------------------------|----------|--------|
| AC-1 | srTCM token bucket per (sender,class); P0/P1 drop-exempt; silent-drop | `rate_limiter.rs` `rate_limit_exempts_p0_p1`/`rate_limit_silently_drops_on_overflow`/`burst_refill_semantics`/`class_isolation`/`unknown_sender_own_bucket`/`independent_senders`/`refill_after_interval`/`max_sender_buckets_eviction`; **SEC-RT-09/11 FIXED** (unknown-sender burst, div-by-zero `.max(1)`) | PASS |
| AC-2 | Per-sender storage quota + priority-reserved pool; P0/P1 never quota-evicted; TTL ordering | `quota.rs` `within_limit`/`reject`/`p0_p1_exempt`/`remove_frees_space`/`eviction`/`reset`/`metrics`; **SEC-RT-04/05/12 FIXED** (TOCTOU single-gate, terminal-point refunds, saturating eviction); **iter-78 fuzz-found+fixed quota overflow/wraparound bypass** + `huge_message_size_no_overflow` regression | PASS |
| AC-3 | Inbound freshness window + per-source skew; too-old/too-future rejected | `replay.rs` `too_old_rejected`/`too_future_rejected`/`freshness_within_window`/per-source skew tests; `replay_freshness_too_future` (corrected range) | PASS |
| AC-4 | Per-sender high-water (ts,seq): <= drop / strictly-higher advance; reboot covered | `replay.rs` `highwater_in_order`/`out_of_order_dropped`/`reboot_persistence`/`cross_reboot`; **SEC-RT-01/03/14 FIXED** (RwLock self-deadlock, future-poison clamp, bounded regression) | PASS |
| AC-5 | Cross-reboot persistence (dedup Bloom+LRU snapshot + high-water batched crash-safe) | `replay.rs` `snapshot_round_trip`/`load_snapshot_and_reset`/`into_internal_filters_bad_keys`/`snapshot_scheduled_after_interval`; `message_engine/mod.rs` promotes WP-2 deferral (`mod.rs:24` line now covered) | PASS |
| AC-6 | Msg-id 128-bit SHA-256 truncation; NIST SP 800-107 collision analysis | `security` id derivation reuses SHA-256 truncation (CRYPTO-001 pattern); collision analysis recorded RES-0018 (NIST SP 800-107 L1) | PASS |
| AC-7 | Reassembled fragments re-enter dedup/freshness/high-water (no replay-after-reassembly gap) | message_engine fragment reassembly path bound by engine checks; documented in DESIGN §AC-7 (integration covered by engine tests) | PASS |
| AC-8 | Reputation bounded per-peer Bayesian = local watchdog + verified-secondhand positives-only (CORE) + iTrust audits; routing-weight only | `reputation.rs` `positive_increases`/`negative_floored_at_zero`/`decay_toward_neutral`/`bounded_in_range`/`never_negative_propagation`/`max_peers_evicts_lowest`; **SEC-RT-07 FIXED** (`verified_peers` gate + `MAX_VERIFIED_VOUCHERS=64`) | PASS |
| AC-9 | Reputation never an admission gate (no message dropped purely for reputation) | `reputation.rs` `reputation_not_a_gate` regression | PASS |
| AC-10 | Receiver-side 'likely_spam' annotation; relay never hard-drops; P0-P3 disjoint | `spam.rs` `small_payload_scores`/`high_volume_penalty`/`never_drops_only_annotates`/`p0_p3_disjoint`/`sender_stats_capped_at_max`; **SEC-RT-06 FIXED** (`max_sender_stats` cap) | PASS |
| AC-11 | ACL-1 emergency send authorization (per-class allowlists; spoofed/revoked/expired/replayed rejected; closes RED-0002/RED-0003/ADR-0011) | `acl.rs` `authorized_with_valid_authority_chain`/`missing_chain_invalid`/`untrusted_chain_invalid`/`root_not_allowlisted`/`root_not_authority_root`/`sos_*` identity gates; **SEC-RT-02/08/15 FIXED** (chain really crypto-verified, SOS empty-store gate, root decode `KeyAdvertisementV1::from_bytes`) | PASS |
| AC-12 | Un-armed Noop == byte-identical behavior; workspace stays green | `mod.rs` `noop_all_default_methods_are_permissive`/`full_policy_configs`; `noop_disarmed`/`full_arms`/`disarm`; **AC-12 regression** — full workspace **532 passed / 0 failed** with Noop default | PASS |
| AC-13 | Doc reconciliation (DISC-0013 C-pattern: SYBIL_RESISTANCE/DOS_RESISTANCE claimed-vs-implemented) | SEC_001_DESIGN.md §6 reconciliation matrix; C1-C6 resolved; docs corrected per C-pattern | PASS |
| AC-14 | Adversarial coverage >=95% on security module | **tarpaulin (iter 80, honestly re-measured): 590/617 = 95.6%** (target >=95% MET); 40+ targeted coverage tests across all 7 files; per-file acl 95.6% / mod.rs 74.7% (async_trait attribution artifact) / rate_limiter 98.0% / quota·replay·reputation·spam 100%; remaining uncovered = defensive-unreachable + artifact, recorded `known_limitations`; evidence `%TEMP%\opencode\tarpaulin-ac14c\cobertura.xml` (root-package aggregate 77.44%, verifier-reproduced) | PASS |
| AC-15 | Fuzzing >24h across rate-limit/replay/reputation per safety_critical_additional (**or recorded gated deviation with property-based adversarial suite**) | cargo-fuzz harness `crates/iris-core/fuzz/` proven; persistent `fuzz_security_engines` across all 6 engines; **found+fixed 2 real bugs** (quota overflow/wraparound bypass iter 78; acl Noop violation iter 77); **36 M+ executions clean** (19.4 M + 16.9 M+ background run, still counting); literal >=24h aggregate NOT reached → **recorded gated deviation, operator-ratified 2026-08-16**, substitute = property-based adversarial suite (**68 unit + 33 property functions across all engines**) + accumulated clean fuzz; recorded `SEC-001_SECURITY_REVIEW.md` | PASS (gated deviation) |
| AC-16 | Security_Agent review dispositioned (redteam SEC-RT); no secrets in log output (CI grep guard); known_limitations recorded | `SEC-001_SECURITY_REVIEW.md` — **PASS-with-recorded-deviations**; redteam **SEC-RT 12 findings (2 CRITICAL / 8 HIGH / 2 MEDIUM) ALL FIXED in-pass** + **SEC-RT-15** (coverage-discovered) FIXED; **grep guard CLEAN** (`security::` test output grep payload|signature|secret → only test-NAME match `spam_small_payload_clean`, no log-line leak); known_limitations recorded (BLE 512B, Sybil per-identity, sequential reputation, cadence-not-adopted, AC-15 deviation) | PASS |

## Live verification (this pass)

| Check | Command | Result |
|-------|---------|--------|
| Workspace | `cargo test --workspace --all-features` | **532 passed / 0 failed / 1 ignored** (iris-core **477** incl. security + proptest suites; +3 crypto_e2e + 8 ml + 4 obs + 8 sim + 5 desktop + 3 commands + 3 engine_roundtrip + 7 storage + 1 m3 + 13 pg_store) |
| Security lib | `cargo test -p iris-core --features proptest --lib security::` | **101 passed** (68 unit + 33 property fns) |
| Security lib (default) | `cargo test -p iris-core --lib security::` | **68 passed** |
| Clippy | `cargo clippy --workspace --all-features --tests` | **0 warnings** |
| Build | `cargo build --workspace` | clean |
| Coverage | tarpaulin `--features proptest --lib` | security **590/617 = 95.6%** (AC-14 MET) |
| Log hygiene | `cargo test -p iris-core --lib security:: 2>&1 | grep -E "payload|signature|secret"` | CLEAN (test-name only) |
| Fuzz | background `fuzz_security_engines` (log `%TEMP%\opencode\fuzz-ac15-iter80.log`) | 16.9 M+ execs clean this run (36 M+ total) |

## Independent verifier (subagent) reproduction

Independent `verifier` subagent dispatched against the workspace (iter 82) with
instructions to reproduce: workspace test totals, clippy, security test count,
tarpaulin coverage figure, grep-guard result, presence + coverage of the
redteam regression tests (SEC-RT-01..15 fix set), fuzz-harness existence, and
absence of any AC row citing a nonexistent test.

Verdict: **APPROVE_WITH_NOTES**. All headline numbers reproduced live — workspace
**532 passed / 0 failed / 1 ignored**; `--features proptest --lib security::` =
**101 passed (68 unit + 33 property fns)** — larger than documented "68"; clippy
**0**; tarpaulin cobertura exists and security package line-rate **0.95624 =
95.62%** (per-file matches: acl 95.60, mod 74.70, rate_limiter 97.98,
quota/replay/reputation/spam 100); grep guard clean (only test-NAME match); all
13 SEC-RT rows + the iter-78 quota CAS fix present as real code with passing
regression tests; every sampled AC test-name verified to exist (2 exact, 1
rename, 5 behavior-equivalent). Aggregate-coverage figure in this doc corrected
from 77.14% to **77.44%** (root-package scope) — AC-14 gate metric (security
module 95.6%) unaffected. One evidence-column naming cleanup applied (AC-1/3/4/5/10/12
cite behavior-equivalent real test names). No inflated claims found; the
iter-77 honest-state-correction account is consistent with repo history.

## Known limitations (recorded, not blockers)

- BLE 512 B verified-chain ceiling (security chains need >=690 B for 2-element).
- General per-identity rate limits remain Sybil-defeatable by free identity churn
  (RES-0018; Douceur IPTPS 2002) — mitigated, not eliminated.
- Reputation converges slowly in sparse contact; sequential by design; never a gate.
- Meshtastic-style fixed global cadence not adopted (priority-qualified buckets).
- **AC-15 gated deviation**: >=24 h literal wall-clock fuzz not reached; property-based
  adversarial suite (68 unit + 33 property fns) + 36 M+ clean executions substitute;
  operator-ratified 2026-08-16; accumulation continues in background for the record.

## Sign-off

- **AC-1..16 — PASS** (evidence above).
- SECURITY_REVIEW record: PASS-with-recorded-deviations (all findings dispositioned).
- Verifier reproduction: **APPROVE_WITH_NOTES** — reconciled, all noted items applied
  (101-suite count, 33-property-fn phrasing, aggregate 77.44%, evidence-column renames).
- **Ready for ACCEPT → node transition to transports.**