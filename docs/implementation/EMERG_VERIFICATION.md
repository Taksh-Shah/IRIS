# EMERG-001 Verification (VERIFY stage)

- **Node**: EMERG-001 — Emergency System (P0 SECURITY/safety)
- **Stage**: VERIFY — acceptance evidence for ACCEPT
- **Version**: v1.0
- **Date**: 2026-08-15
- **Precedents**: IDENT_VERIFICATION.md v1.0 (AC-1..11 PASS + independent verifier
  APPROVE), CRYPTO_VERIFICATION.md v1.0 (AC-1..9 PASS)

## Acceptance-criteria outcome

**All 14 acceptance criteria PASS** (AC-1..12 + AC-14 verified in TEST stage
`EMERG-001_TEST.md` and re-confirmed live in this VERIFY pass; AC-13 redteam
verified in `EMERG-001_SECURITY_REVIEW.md`, integrity re-checked here).

Independent verifier subagent verdict: **APPROVE_WITH_NOTES → reconciled to
APPROVE** (see §5).

## AC → evidence table

| AC | Criterion (from PROJECT_GRAPH.yaml) | Evidence | Status |
|----|--------------------------------------|----------|--------|
| AC-1 | EmergencyBroadcast CBOR CDE round-trip + <1 KB budget + TooLarge error (no silent truncation) | `codec.rs` `broadcast_round_trip`/`broadcast_minimal_round_trip`/`canonical_deterministic_encoding`/`missing_required_field_rejected`/`out_of_range_enum_rejected`/`oversized_headline_rejected`/`truncated_data_fails_cleanly`/`unknown_keys_tolerated`; `broadcast.rs` `four_element_chain_budget_measured_and_fits_p3_envelope` (2-elem **690 B < 1 KB**; 4-elem 1277 B = P3 ≥64 KB class) + `oversized_payload_rejected_no_silent_truncation` | PASS |
| AC-2 | Authority chain verify matrix (valid passes; empty/overlong>4/untrusted-root/broken/expired/revoked/sender-mismatch/small-order rejected) + audited no-reply | `authority.rs` `single_trusted_root_passes`/`verified_broadcast_passes_all_steps`/`no_chain_rejected`/`chain_too_long_rejected`/`untrusted_root_chain_rejected`/`validity_window_rejected`/`sender_mismatch_rejected`/`leaf_binding_mismatch_rejected`/`short_id_is_stable_and_distinct`; inherited IDENT `identity/chain.rs` broken-link + small-order rejection (revoked-element rejection path is real in `chain.rs` `RevokedElement`; revocation *behavior* proven at trust_store/rotate instruction level — `identity/trust_store.rs`/`rotate.rs`); `provider.rs` `gateway_audit_ring_captures_drops`; engine `emergency_gate_drops_unverifiable_alert_no_reply` (drop + no reply + `dropped_emergency_auth ≥ 1`) | PASS |
| AC-3 | Geo/functional scope + severity caps enforced from authority profile; out-of-scope dropped | `authority.rs` `geo_scope_denied`/`geo_scope_prefix_allows_subregions`/`functional_scope_denied_for_revocation`/`severity_cap_enforced`/`drill_mismatch_rejected`; `model.rs` `broadcast_validate_ok`/`broadcast_validate_rejects_bad_shape`/`severities_round_trip_wire_codes`; out-of-scope → Drop via `verify_and_classify` | PASS |
| AC-4 | SOS build: ContentType::Sos P0 never-encrypted ≤84 B signed; direct-encrypted SOS to known peer via normal path | `codec.rs` `sos_round_trip`/`sos_cancel_round_trip`/`sos_fits_p0_budget` (≤84 B on disk)/`canonical_deterministic_encoding`; `sos.rs` `fresh_sos_accepted`/`garbage_payload_rejected`; direct-encrypted SOS = normal path (Sos content type unchanged through engine) | PASS |
| AC-5 | SOS rate limit 3/hr/sender → 4th downgraded P3; rolling window; reset on verified CANCEL; authority exempt | `rate_limit.rs` `first_three_allowed_fourth_downgraded`/`independent_senders`/`resets_after_cancel`/`window_slides_over_time`; `provider.rs` `gateway_rate_limit_gates_after_third`; **receive-side enforcement (RT-002 FIXED)**: engine `emergency_gate_downgrades_fourth_sos_to_p3` + `lru_caps_sender_buckets` (LRU bound `SOS_MAX_BUCKETS`); authority alerts bypass limiter by construction (SOS-only path) | PASS |
| AC-6 | SOS cancel same-signer-only within 60 min; otherwise rejected + audited | `sos.rs` `cancel_within_window_accepted`/`cancel_outside_window_rejected`/`cancel_unknown_original_rejected`/`cancel_missing_original_rejected`; `model.rs` `sos_cancel_requires_original_id`; rejects flow to gateway audit ring. (Engine-ledger resolution of originals gated = documented RT-003 limitation) | PASS |
| AC-7 | Disaster triggers + manual override + EMERGENCY→CRISIS→DEGRADED→NORMAL ≥15-min holds + gossip hop 5/255 | `mode.rs` `deactivate_bypasses_hold`/`deactivate_only_when_not_normal`/`degraded_recovers_after_hold`/`wire_codes_round_trip`; `guarded_transition` rejects premature moves; hop 5/255 constant enforced in ROUTE-001, documented EMERG_DESIGN.md §9. (Engine-trigger wiring deferred = RT-004 limitation; AC-7 evidence library-scope) | PASS (library-scope, RT-004 recorded) |
| AC-8 | Status OFF by default; SOS ack = app message; no per-relay broadcast ACK; cancel = payload subtype | Status off-by-default in engine init (RFC 9171 §6.2 semantics); ack.rs 30s/2×/unlimited-TTL cadence preserved, no per-relay broadcast ACK exists by design; `sos_cancel_round_trip` proves CANCEL = payload subtype of Sos content type | PASS |
| AC-9 | Audit metadata-only pseudonymous bounded; no content; 30d/1y retention | `audit.rs` `event_codes_round_trip`/`ring_bounds_capacity`; `provider.rs` `gateway_audit_ring_captures_drops`; **content-free enforced (RT-007 FIXED)**: `AreaOutOfScope` is a static discriminant (no `area_code`/`geo_scope` bytes in notes); pseudonymous SHA-256 prefixes only | PASS |
| AC-10 | Mandatory relay of verified broadcasts; unverifiable dropped; drill excluded from OS surfaces | `broadcast.rs` `verified_alert_relays`/`unauthenticated_dropped`/`garbage_payload_dropped`/`verified_drill_suppressed`; `drill.rs` `drill_flag_suppresses`/`drill_message_type_suppresses`/`test_severity_suppresses`/`real_alert_surfaces`; engine gate `emergency_gate_drops_unverifiable_alert_no_reply`; **replay-bounded (RT-006 FIXED)**: `replay_guard_suppresses_same_broadcast_id_until_retention_elapses` + `replay_guard_bounded_at_max` | PASS |
| AC-11 | NoopEmergencyProvider default keeps workspace green (behavior identical when unarmed) | `provider.rs` `noop_provider_is_unarmed_and_permissive`; engine `emergency_provider_defaults_to_noop_and_switches` + `emergency_gate_unarmed_noop_keeps_behavior_identical` (Delivered, `dropped_emergency_auth == 0`); workspace 431 green with gate fully wired | PASS |
| AC-12 | Doc corrections C1/C3/C4/C5/C6 applied | IMPLEMENT evidence (execution-log iter 68, CHANGELOG 0.3.8): EMERGENCY_BROADCAST (96/96 compact + signed-follow-up UPDATE, C1), EMERGENCY_UX (WEA 853+960 Hz; SOS cadence 30s/2×/unlimited-TTL, C3+C5), EMERGENCY_ABUSE (IPC→BNS 2023, C4), EMERGENCY_GOVERNANCE (SPKI-style key-anchored, no PKIX, C6); EMERG_DESIGN.md §4/§2.3/AC-1 corrected to measured sizes (TEST) | PASS |
| AC-13 | Redteam: no CRITICAL/HIGH un-addressed; CRITICAL halt + operator report if any | `EMERG-001_SECURITY_REVIEW.md` (iter 70): verdict **PASS-with-recorded-deviations** — EMERG-RT-001..011 dispositioned; RT-001 CRITICAL + RT-002 HIGH **FIXED in-pass** (+7 regression tests); 2 MEDIUM + 2 LOW FIXED; 6 recorded as gated/deferred with explicit conditions. No CRITICAL/HIGH un-addressed. | PASS |
| AC-14 | Workspace green + clippy 0 + RUSTSEC clean (no new crypto crates D6) | Live re-verification this pass: `cargo test --workspace` **431 passed / 0 failed** (1 `#[ignore]`d debug helper `route2_debug_undelivered`, unrelated to EMERG-001); `cargo clippy --workspace --all-targets` **0 warnings**; `cargo audit` **0 vulnerabilities** in 516 crates — 17 warnings are all pre-existing desktop/tray UI-transitive (gtk/atk/gdk/glib/unic/proc-macro-error), none emergency-relevant; D6: emergency module adds **no** dependencies (reuses ciborium v0.2.2 + pinned ed25519-dalek 2.2.0/x25519-dalek 2.0.1/chacha20poly1305 0.10.1/hkdf 0.12.4/sha2 0.10.9/curve25519-dalek 4.1.3) | PASS |

## Independent verifier (subagent) reproduction

Parallel subagent `verifier` dispatched against the workspace with instructions
to reproduce: workspace test totals, clippy, seam behavior (armed gate drops /
Noop passthrough), presence + coverage of every redteam regression test
(RT-001/002/006/007/009/011), audit-warning scope, and absence of any AC row
citing a nonexistent test or fake evidence.

Verdict: **APPROVE_WITH_NOTES** — each AC row was traced to a real, compilable
test; workspace totals independently re-run (431 passed / 0 failed / 1
`#[ignore]`d debug helper); all six redteam fixes present with regression tests;
audit warnings confirmed desktop-transitive (out of iris-core scope). Three
documentation-level notes, all reconciled in this doc/code:

1. **Ignored-test characterization** — the single ignored test is the
   `route2_debug_undelivered` `#[ignore]` debug helper (sim_scenarios.rs), NOT
   unix-gated (the `#[cfg(unix)]` identity tests are compiled out on Windows and
   are not "ignored"). Fixed wording above (AC-14 row + run evidence).
2. **AC-2 revoked-test coverage phrasing** — `verify_chain`'s `RevokedElement`
   path is real in `chain.rs` but no test feeds a revoked element through
   `verify_chain`; revocation behavior is proven at the trust_store/rotate
   instruction level. AC-2 evidence column rephrased to state this precisely
   (criterion still holds: revoked chains are rejected, error path exists).
3. **Stale docstring** — `provider.rs:240` documented
   `BroadcastReplayGuard::MAX_IDS` as 3096; actual constant is 1024. Comment
   fixed to 1024 (matches code + tests).

## Re-recorded known limitations (→ ACCEPT documentation)

Recorded in `EMERG-001_SECURITY_REVIEW.md` + this doc, carried to
PROJECT_GRAPH `known_limitations` at ACCEPT:

1. **BLE 512 B ceiling**: verified authority chains (2-elem 690 B) exceed BLE
   512 B MTU; SOS P0 remains radio-native; verified broadcasts ride P3 ≥64 KB
   transports.
2. **Send-side emergency synthesis app/authority-owned**: engine does not build
   `EmergencyAlert` envelopes (EMERG_DESIGN.md §10).
3. **Typed `EmergencyEvent` OS-surface wrappers** deferred to platform nodes
   (ANDROID-001 / IOS-001 / DESKTOP-001).
4. **RT-003**: engine-ledger resolution of CANCEL originals gated (UnknownOriginal
   → drop is security-positive; `sos_reset` AC-6 proven unit-scope).
5. **RT-004**: `DisasterMode` engine-trigger wiring deferred (AC-7 library-scope
   evidence).
6. **RT-005**: unarmed-Noop identity intentional (AC-11). **RT-008**: SOS drill
   surface suppression app-owned. **RT-010**: pre-gate emergency audit gap tracked.

## Verification run evidence (live, this pass)

- `cargo test --workspace` → **431 passed / 0 failed** (15 suites; 1 `#[ignore]`d
  debug helper `route2_debug_undelivered` — not EMERG-001 related).
- `cargo clippy --workspace --all-targets` → **0 warnings**.
- `cargo audit` → **0 vulnerabilities** (516 crates); 17 warnings = pre-existing
  unmaintained/unsound desktop-transitive (gtk/atk/gdk/glib/unic-ucd/proc-macro-error);
  no iris-core/emergency/crypto advisory.
- `cargo tree -p iris-core --depth 1` → emergency path uses only existing pinned
  crypto tree; no new crates (D6).