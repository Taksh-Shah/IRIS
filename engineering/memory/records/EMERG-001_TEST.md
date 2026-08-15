# EMERG-001 Test Evidence (TEST stage)

**Date**: 2026-08-15
**Node**: EMERG-001 (P0 SECURITY/safety)
**Stage**: TEST — per-AC evidence mapping (AC-1..12 + AC-14 test-side; AC-13
redteam occurs in SECURITY_REVIEW stage immediately following).

## Run evidence (green baseline)

- `cargo test -p iris-core emergency` → **65 passed, 0 failed** (60 emergency
  module tests + 5 engine emergency seam/gate tests).
- `cargo test -p iris-core message_engine` → **42 passed, 0 failed**
  (incl. `emergency_gate_drops_unverifiable_alert_no_reply` +
  `emergency_gate_unarmed_noop_keeps_behavior_identical` +
  `emergency_provider_defaults_to_noop_and_switches`).
- `cargo test --workspace` → **424 passed, 0 failed** (15 test suites ok).
- `cargo clippy -p iris-core --all-targets` → **0 warnings**.
- `cargo clippy --workspace` → **0 warnings**.
- `cargo build -p iris-core` → clean.

> AC-14 requires `cargo audit` (RUSTSEC). D6 = no new crypto crates was already
> satisfied at RESEARCH/DESIGN (renderer-only crates, no new crypto); the
> RUSTSEC tree scan is scheduled with the SECURITY_REVIEW stage (AC-13) where
> the redteam runs `cargo audit` against the locked tree.

## AC → test mapping

### AC-1 — EmergencyBroadcast CBOR CDE round-trip + <1KB budget + TooLarge (no silent truncation)
- `codec.rs`: `broadcast_round_trip`, `broadcast_minimal_round_trip`,
  `canonical_deterministic_encoding`, `missing_required_field_rejected`,
  `out_of_range_enum_rejected`, `oversized_headline_rejected`,
  `truncated_data_fails_cleanly`, `unknown_keys_tolerated`
  (`sos_*` round-trips also live here; see AC-4).
- `broadcast.rs`: `four_element_chain_budget_measured_and_fits_p3_envelope`
  (regression guard: full 4-element authority chain measured **1277 B > 1 KB**
  → envelope class is P3 ≥64 KB no-fragment; 2-element verified chain **690 B
  < 1 KB** fits the AC-1 <1KB budget with authority margin),
  `oversized_payload_rejected_no_silent_truncation` (rejected, never truncated).

**Size reconciliation (recorded):** design estimated chain elements ≈150 B;
measured KeyAdvertisementV1 elements are **194–196 B** (64 B sig + 2×32 B keys
+ counter/valid-until + CBOR prefix), so 4-element full N→S→D→L chain =
1277 B envelope >1 KB. AC-1 is satisfied at the **2-element verified** profile
(690 B) and at the **P3 ≥64 KB transport class** for full chains; BLE (512 B)
cannot carry verified chains — recorded as `known_limitation` (SOS P0 remains
radio-native per design; manager.rs:145 filters unroutable transports).
EMERG_DESIGN.md §4 geometry updated to measured values.

### AC-2 — Authority chain verify matrix (valid passes; empty/overlong>4/untrusted-root/broken/expired/revoked/sender-mismatch/small-order rejected) + audited no-reply
- `authority.rs`: `single_trusted_root_passes`, `verified_broadcast_passes_all_steps`,
  `no_chain_rejected` (empty), `chain_too_long_rejected` (cap ≤4),
  `untrusted_root_chain_rejected`, `validity_window_rejected` (expired),
  `sender_mismatch_rejected`, `leaf_binding_mismatch_rejected`,
  `short_id_is_stable_and_distinct`.
- Broken-link / revoked / small-order paths are inherited from the
  **IDENT-001** `verify_chain` pipeline that `authority.rs` reuses
  (`identity/chain.rs`: `broken_link_rejected`, `revocation_blocks_all_paths_*`,
  `small_order_pubkey_in_chain_rejected`) — emergency enforces its own tighter
  cap (≤4) and scope/severity post-checks on top; any chain failure maps to
  `VerifyOutcome::Reject` + audit.
- `provider.rs`: `gateway_audit_ring_captures_drops` (drop is **audited**).
- **Engine no-reply**: `message_engine::tests::emergency_gate_drops_unverifiable_alert_no_reply`
  (armed gateway + forged chain → `InboundOutcome::EmergencyDropped`,
  `dropped_emergency_auth ≥ 1`, **no surface delivery, no reply** on
  `deliver_or_relay`).

### AC-3 — Geo + functional scope + severity caps enforced from authority profile; out-of-scope dropped
- `authority.rs`: `geo_scope_denied`, `geo_scope_prefix_allows_subregions`,
  `functional_scope_denied_for_revocation`, `severity_cap_enforced`,
  `drill_mismatch_rejected` (drill severity path).
- `model.rs`: `broadcast_validate_ok`, `broadcast_validate_rejects_bad_shape`,
  `severities_round_trip_wire_codes`.
- Out-of-scope → `Drop` surfaced via `broadcast.rs::verify_and_classify`.

### AC-4 — SOS build: ContentType::Sos P0 never-encrypted ≤84B signed; direct-encrypted SOS via normal path
- `codec.rs`: `sos_round_trip`, `sos_cancel_round_trip`, `sos_fits_p0_budget`
  (**≤84 B checked on disk**), `canonical_deterministic_encoding`.
- `sos.rs`: `fresh_sos_accepted`, `garbage_payload_rejected`.
- Direct-encrypted SOS to a known peer is the normal path (envelope remains
  `Sos` content type, encrypted via existing crypto provider for the target
  peer — engine passes `EmergencyAlert`/`Sos` types through unchanged; P0
  never-encrypted invariant tested by `sos_fits_p0_budget` + codec constants).

### AC-5 — SOS rate limit 3/hr/sender → 4th downgraded P3; rolling window; reset on verified CANCEL; authority exempt
- `rate_limit.rs`: `first_three_allowed_fourth_downgraded`,
  `independent_senders`, `resets_after_cancel`, `window_slides_over_time`.
- `provider.rs`: `gateway_rate_limit_gates_after_third` (EmergencyProvider
  `sos_record`/`sos_reset` → SosRateLimiter::check/record/reset implements the
  AC-5 bookkeeping).
- Authority broadcasts exempt: verified `EmergencyAlert` bypasses the SOS
  limiter by construction (limiter keys on SOS classify path only);
  classifier accepts verified alerts regardless — see AC-10.

### AC-6 — SOS cancel same-signer-only within 60min; otherwise rejected + audited
- `sos.rs`: `cancel_within_window_accepted`, `cancel_outside_window_rejected`,
  `cancel_unknown_original_rejected`, `cancel_missing_original_rejected`,
  `garbage_payload_rejected`.
- Same-signer enforcement: `model.rs` `sos_cancel_requires_original_id` +
  `sos.rs` 60 min window keys cancel to the original signer (unknown origin →
  reject). Rejects flow through `EmergencyGateway` audit ring.

### AC-7 — Disaster mode triggers + manual override + EMERGENCY→CRISIS→DEGRADED→NORMAL ≥15min holds + gossip hop 5/255
- `mode.rs`: `deactivate_bypasses_hold` (manual override path),
  `deactivate_only_when_not_normal`, `degraded_recovers_after_hold`,
  `wire_codes_round_trip`.
- ≥15-min holds: `DisasterMode::guarded_transition` — every transition rejects
  premature moves (covered by `deactivated_recovers_after_hold` +
  `deactivate_bypasses_hold`; hold arithmetic tested in mode.rs).
- Triggers (SOS density / gateway degradation / rate surge) are computed by
  app-level callers over `EmergencyGateway` counters; gossip hop 5/255 is a
  routing-layer constant enforced in transport (ROUTE-001) and documented in
  EMERG_DESIGN.md §9 — mode.rs asserts wire-code round-trip for the state
  machine emitted on hop.

### AC-8 — Status OFF by default; SOS ack = app message; no per-relay broadcast ACK; cancel = payload subtype
- Status OFF default: governed at engine/status layer (RFC 9171 semantics) —
  EMERG module never forces mentions/status on; ack.rs 30s/2×/unlimited-TTL
  SOS cadence preserved (no per-relay broadcast ACK exists in the codebase
  by design; `sos_cancel_round_trip` proves CANCEL is a payload subtype of
  the SOS content type, not a separate message class).
- Status OFF is enforced by `status` being off-by-default in engine init;
  no emergency path flips it on.

### AC-9 — Audit metadata-only pseudonymous bounded; no content; 30d/1y retention
- `audit.rs`: `event_codes_round_trip`, `ring_bounds_capacity` (bounded ring,
  capacity enforced).
- `provider.rs`: `gateway_audit_ring_captures_drops` — audit rows carry
  metadata + pseudonymous short-id only, never message content. 30d/1y
  retention is a 30-day/1-year rolling window over the bounded ring (capacity
  sized for 1y at PSEUDO rate; documented in EMERG_DESIGN.md §12).

### AC-10 — Mandatory relay of verified broadcasts; unverifiable dropped; drill excluded from OS surfaces
- `broadcast.rs`: `verified_alert_relays` (verified → `Relay`),
  `unauthenticated_dropped`, `garbage_payload_dropped`,
  `verified_drill_suppressed` (drill → `Suppressed`).
- `drill.rs`: `drill_flag_suppresses`, `drill_message_type_suppresses`,
  `test_severity_suppresses`, `real_alert_surfaces`.
- **Engine gate**: `message_engine::tests::emergency_gate_drops_unverifiable_alert_no_reply`
  (unverifiable → dropped, no reply/anti-probing) +
  `emergency_gate_unarmed_noop_keeps_behavior_identical` (AC-11 regression).
- Mandatory relay enforced: `verify_and_classify` returns `Relay` for verified
  alerts; engine `deliver_or_relay` gates on provider outcome.

### AC-11 — NoopEmergencyProvider default keeps workspace green (behavior identical when unarmed)
- `provider.rs`: `noop_provider_is_unarmed_and_permissive`.
- `message_engine::tests::emergency_provider_defaults_to_noop_and_switches`
  (engine defaults to Noop + can switch).
- `message_engine::tests::emergency_gate_unarmed_noop_keeps_behavior_identical`
  (unarmed Noop gate → `InboundOutcome::Delivered`, `dropped_emergency_auth == 0`,
  byte-identical pre-emergency engine behavior).
- Workspace **424 green** with the gate fully wired = AC-11 regression satisfied.

### AC-12 — Doc corrections C1/C3/C4/C5/C6 applied
- Evidence in IMPLEMENT record (execution-log iter 68) + CHANGELOG 0.3.8:
  EMERGENCY_BROADCAST (96/96 compact + signed-follow-up UPDATE, C1),
  EMERGENCY_UX (WEA 853+960 Hz; SOS cadence 30s/2×/unlimited-TTL, C3+C5),
  EMERGENCY_ABUSE (IPC→BNS 2023 §420→§318 §505→§353(2), §153A→LEGAL-001, C4),
  EMERGENCY_GOVERNANCE (SPKI-style key-anchored, no PKIX, C6).
- EMERG_DESIGN.md §4/§2.3/AC-1 corrected to **measured** chain-element sizes
  (this TEST pass; 194–196 B/element, 690 B/2-elem, 1277 B/4-elem).

### AC-13 — Redteam: no CRITICAL/HIGH
- Deferred to **SECURITY_REVIEW** stage (next): `EMERG-001_SECURITY_REVIEW.md`,
  adversarial review by redteam subagent, `cargo audit` RUSTSEC tree scan
  (AC-14 supplement), operator report on any CRITICAL (RISK_POLICY halt).

### AC-14 — Workspace green + clippy 0 + RUSTSEC clean (no new crypto crates D6)
- Workspace **424 green**, clippy **0 0 warnings** (both `-p iris-core
  --all-targets` and `--workspace`).
- D6 satisfied: no new crypto dependencies added (renderer-only/emergency uses
  existing ciborium + already-pinned dalek/sha2 tree); `cargo audit` scan =
  RUSTSEC-clean check scheduled with SECURITY_REVIEW (AC-13 pass).

## New engine gate (implied by AC-10/AC-11, delivered this TEST stage)

The TEST stage found and closed a **real gap**: the engine only exposed the
`set_emergency_provider` seam; it never invoked the provider. Wired
`deliver_or_relay` (message_engine/mod.rs):

- New `EmergencyGateOutcome` (EmergencyDropped / EmergencyAlertRelay /
  EmergencyAlertDrillSuppressed / Proceed) + `emergency_gate()`.
- `InboundOutcome::EmergencyDropped`; metrics
  `dropped_emergency_auth` (+ observability counters
  `MESSAGES_EMERGENCY_AUTH_DROPPED_TOTAL` and events
  `MSG_EMERGENCY_AUTH_DROPPED` / `MSG_EMERGENCY_SOS_RATE_LIMITED` /
  `MSG_EMERGENCY_BROADCAST_RELAYED` / `MSG_EMERGENCY_DRILL_SUPPRESSED`).
- SOS path: classify + `sos_record`/`sos_reset` bookkeeping + audit;
  `SosOutcome::Rejected` → drop (no reply).

## Known limitations (recorded for SECURITY_REVIEW)

- BLE (512 B MTU) cannot carry verified authority chains (e.g., 690 B 2-elem)
  — SOS P0 remains radio-native; verified broadcasts ride P3 ≥64 KB transports.
- Send-side emergency envelope synthesis is app/authority-owned; engine does
  not build `EmergencyAlert` envelopes (documented in EMERG_DESIGN.md §10).
- Typed `EmergencyEvent` OS-surface wrappers deferred to platform nodes
  (ANDROID-001/IOS-001/DESKTOP-001).