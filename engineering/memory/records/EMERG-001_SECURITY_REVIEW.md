# EMERG-001 SECURITY_REVIEW (redteam) — findings + dispositions

- **Record ID**: EMERG-001 SECURITY_REVIEW
- **Scope**: EMERG-001 (Autonomous Emergency System), SECURITY_REVIEW stage, iter 70
- **Date**: 2026-08-15
- **Agent**: redteam (adversarial source review) → supervisor disposition pass
- **Files**: `crates/iris-core/src/emergency/{authority,broadcast,codec,provider,
  rate_limit,audit,sos,mode,drill}.rs`, `message_engine/mod.rs`
  (receive-path gate), `EMERG_DESIGN.md`
- **Verdict**: **PASS-with-recorded-deviations** — no CRITICAL/HIGH un-addressed.
  EMERG-RT-001 (CRITICAL, authority-root collapse) and EMERG-RT-002 (HIGH,
  SOS receive-side rate enforcement) **FIXED in-pass**; 2 MEDIUM + 2 LOW
  FIXED; 6 recorded as known_limitations/gated with explicit conditions.

> ⚠️ Numbering note: EMERG-RT-NNN below is **EMERG-001-scoped** and distinct from
> the IDENT-001 `IDENT-RT-NNN` and CRYPTO-001 `RED-NNN` series. No collision.

---

## Findings (adversarial, verified against code)

| ID | Severity | Topic |
|---|---|---|
| EMERG-RT-001 | **CRITICAL** | Authority chain root only needs *any* trusted root (TOFU `Unverified` accepted) — a self-advertising mesh peer could anchor a "verified" CRITICAL emergency chain (PA/panic abuse: alert spoofing, engine surface + relay amplification) |
| EMERG-RT-002 | HIGH | `sos_rate` was never wired: the receive gate recorded SOS (`sos_record`) but never enforced the AC-5 allowance; SOS flood reached route delivery at P0 unbounded |
| EMERG-RT-003 | MEDIUM | CANCEL resolution not wired: gate passes `original_timestamp=None` ⇒ CANCEL always → `UnknownOriginal` → dropped (+audit). Security-positive default but the AC-6 reset path is dead code until a ledger resolves originals |
| EMERG-RT-004 | MEDIUM | `DisasterMode` is a proven guarded state machine but unreachable from the engine path (no provider/trigger seam) — AC-7 evidence is unit-scope only |
| EMERG-RT-005 | LOW | Unarmed gate skips classification; the Noop engineering test asserts forged `EmergencyAlert` **Delivers** unchanged (AC-11 wording reads like a hole) |
| EMERG-RT-006 | MEDIUM | Fresh-`message_id` re-encode of a valid broadcast (`broadcast_id` unchanged) bypasses the engine dedup and re-relays/re-surfaces — broadcast replay not bounded |
| EMERG-RT-007 | MEDIUM | `AreaOutOfScope` embedded **payload bytes** (`area_code` + `geo_scope`) in the verify error → those strings flow into audit `note`s (AC-9 content-free violation) |
| EMERG-RT-008 | LOW | `SosKind::Test` (drill SOS) accepted as a real SOS for rate-limit bookkeeping (no send/receive drill suppression mirror of `EmergencyAlert`) |
| EMERG-RT-009 | LOW | `deliver_or_relay` gated with `.unwrap()` on the emergency `RwLock` — a panicked emergency callback would wedge the whole receive path (poison) rather than recover |
| EMERG-RT-010 | LOW | Bad-signature/expired emergency envelopes fail in `process_incoming` *before* the gate → they exit without an emergency-specific audit/metric row |
| EMERG-RT-011 | LOW | Emergency CDE decoder took the first of duplicate keys (parser-disagreement vs foreign decoders; RFC 8949 §4.2 unique-key requirement) |

## What passed

9-step SPKI/RFC 9804-style chain pipeline (root required, cap ≤4, `Severity::Test`
cap-exempt for drills); leaf-binding of `authority_peer_short` to the chain leaf;
validity-window checks w/ 300 s skew budget (reusing IDENT `DEFAULT_SKEW_BUDGET_SECS`);
drill never surfaces (OS alert layer suppressed, AC-10); SOS ≤84 B never encrypted
(P0 radio-native); rate limiter 3/hr rolling window + CANCEL reset; `DisasterMode`
guarded_transition with ≥15-min challenge hold, wire codes fixed; audit ring bounded
(pseudonymous SHA-256 prefixes, no payload content); Noop default un-arms the whole
emergency horizontal seam (AC-11) — only a node that opted in changes behavior;
ciborium CDE canonical emission (sorted keys, definite lengths); D6 RUSTSEC: no new
crypto crates (`cargo audit` clean in RESEARCH/DESIGN, AC-14 test-side deferred to
VERIFY). Under the intended wiring the "authority" is *provisioned* (NDMA-distributed
root key material), so legitimate NDMA alerts are unaffected by RT-001's tightening.

## Disposition pass (implemented this iteration)

- **EMERG-RT-001 FIXED (CRITICAL)** — `verify_authoritative` now requires the chain
  ROOT to be a **provisioned authority root** (`TrustStore::is_authority_root`,
  i.e. `register_authority_root`), not merely a TOFU-adopted `Unverified` mesh peer
  (`authority.rs` step 3b). `TrustStore` required two plumbing walk-ons:
  `is_authority_root` (level check) + `register_authority_root` (root entry), both
  covered by unit tests. Regression tests:
  - `tofu_adopted_peer_cannot_be_authority_root` — TOFU adoption yields
    `BoundUnverified`; verify → `Err(UntrustedAuthorityRoot)`.
  - `provisioned_authority_root_anchors_self_chain` — registered root self-broadcast
    passes (NDMA root issuing its own alert).
  - All chain builder helpers (`build_chain`, `verified_envelope`,
    `build_four_chain_depth`) now register the root as an authority root, so the
    disabled-verdict test `untrusted_root_chain_rejected` still exercises the
    `Chain(String)` path against a store that never saw the root.
- **EMERG-RT-002 FIXED (HIGH)** — receive gate now enforces the allowance BEFORE
  bookkeeping: `sos_rate` → `Allowed` ⇒ `sos_record` + `Proceed`; `Downgraded` ⇒
  `sos_record` + `EmergencyGateOutcome::SosDowngraded`, which re-enters the delivery
  path at `MessagePriority::P3` (AC-5 4th-in-window degraded delivery, enforced on
  the receive side, not only send-side) and increments
  `emergency_sos_rate_limited` metric + `MSG_EMERGENCY_SOS_RATE_LIMITED` event +
  an `AuditEvent::SosRateLimited` row. Limiter memory bounded:
  `SOS_MAX_BUCKETS` (LRU eviction, `rate_limit.rs`) — hostile fresh-sender floods
  cannot grow memory unbounded. Engine integration test
  `emergency_gate_downgrades_fourth_sos_to_p3` (3× P0 then P3) + unit
  `lru_caps_sender_buckets`.
- **EMERG-RT-006 FIXED (MEDIUM)** — `EmergencyGateway.verify_envelope` now passes the
  decoded `broadcast_id` (or `republish_id`) through a bounded
  `BroadcastReplayGuard` (1024 ids, 5-min retention, LRU eviction); a verified
  replay → `Drop("verified broadcast replay suppressed (RT-006)")` +
  `AuditEvent::BroadcastReplayDropped`. Republish ids are distinct, so legitimate
  UPDATE/republish flows are unaffected. Tests: `replay_guard_suppresses_same_
  broadcast_id_until_retention_elapses`, `replay_guard_bounded_at_max`.
- **EMERG-RT-007 FIXED (MEDIUM)** — `AreaOutOfScope` carries NO payload bytes
  (static discriminant); `geo_scope_denied` test updated. Audit notes stay
  content-free (AC-9). Verify-error strings that cannot leak payload content are
  retained (chain/position/type errors only).
- **EMERG-RT-009 FIXED (LOW)** — `RwLock::read().unwrap_or_else(|e| e.into_inner())`
  poison-recovery on the gate's provider lock (callback panic cannot wedge the
  receive path).
- **EMERG-RT-011 FIXED (LOW)** — all CDE maps (`EmergencyBroadcast`, `SosMessage`,
  `AuthorityMeta`) now reject duplicate integer keys (`no_duplicate_keys`, RFC 8949
  §4.2) — test `known_duplicate_key_rejected`.

## Recorded (known_limitations / gated, with conditions)

- **EMERG-RT-003** — CANCEL resolution deferred: the gate keeps rejecting CANCELs
  with no resolvable original (`UnknownOriginal` → drop + audit), which is
  security-positive. Wired same-signer/ledger resolution is gated on the engine
  ledger/dedup seam (REQ/STORE follow-up). The `sos_reset` AC-6 path is proven by
  unit test but unreachable end-to-end until then.
- **EMERG-RT-004** — `DisasterMode` remains a library state machine; platform/
  ROUTE wiring to the engine trigger seam is a later integration gate (AC-7
  evidence = mode.rs unit tests; engine integration deferred).
- **EMERG-RT-005** — Unarmed+Noop behavior is deliberately byte-identical to
  pre-emergency engine behavior (AC-11): a node that has NOT opted in must not
  change behavior, and forging an alert *there* is irrelevant (nothing armed).
  Arm-before-UI remains an operator/platform responsibility. Documented, accepted.
- **EMERG-RT-008** — `SosKind::Test` reuses the SOS acceptance path (rate-limit
  bookkeeping); drill *surface* suppression for SOS is app-owned (typed
  EmergencyEvent deferral, as recorded in TEST). No security exposure: a Test SOS
  carries the same P0 envelope but never reaches a WEA-style presentation in this
  layer.
- **EMERG-RT-010** — Bad-sig/expired emergency envelopes are still dropped by the
  existing engine defense (drop-first, no reply) before the emergency gate; the
  absence of an emergency-tagged audit row for those early exits is a counter
  observability gap. Tracked; no bypass introduced.

## Evidence

- iris-core unit+integration: **376 passed / 0 failed** (+7 new regression
  tests this disposition: `tofu_adopted_peer_cannot_be_authority_root`,
  `provisioned_authority_root_anchors_self_chain`,
  `emergency_gate_downgrades_fourth_sos_to_p3`, `replay_guard_suppresses_same_
  broadcast_id_until_retention_elapses`, `replay_guard_bounded_at_max`,
  `lru_caps_sender_buckets`, `known_duplicate_key_rejected`).
- Workspace: **431 passed / 0 failed** across suites (424 at TEST end +7 net
  new tests), clippy `--workspace --all-targets` **0 warnings**.
- Engine gate now enforces: unverifiable alert → drop no-reply (anti-probing
  AC-8/10); drill → relay+suppress (AC-10); verified → relay (AC-8);
  SOS allowance → P0, 4th → P3 degraded (AC-5); CANCEL → reset when resolvable
  (AC-6); broadcasts replay-bounded (RT-006); roots authority-anchored (RT-001).

## Known limitations remainder (from TEST stage, unchanged)

BLE 512 B cannot carry a verified `EmergencyAlert` chain (SOS P0 remains
radio-native; EMERG_DESIGN.md §4 measured: 2-elem 690 B > 512 B) · send-side
synthesis is app-owned · typed EmergencyEvent platform panel deferred.

---

**Verdict line**: SECURITY_REVIEW **PASS-with-recorded-deviations** — the CRITICAL
authority-root collapse is closed, the HIGH SOS enforcement gap is closed, MEDIUM
broadcast-replay + privacy leaks closed, LOW poison/duplicate-key closed; 6 findings
recorded as gated/deferred with explicit conditions. Ready for **VERIFY → ACCEPT**.