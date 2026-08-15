# IDENT-001 Verification

Version 1.0 — 2026-08-15

| | |
|---|---|
| **Node** | IDENT-001 — Identity System (P0, COMPONENT) |
| **Status** | ACCEPT (all AC-1..11 PASS with recorded deviations) |
| **Sources** | IDENT_DESIGN.md v1.0 (AC-1..11, §12); RES-0016 (research + DEC-P0005..0009); `engineering/memory/records/IDENT-001_TEST.md` (iter 62b); `engineering/memory/records/IDENT-001_SECURITY_REVIEW.md` (iter 63, IDENT-RT-001..016) |

---

## Summary

All eleven acceptance criteria **PASS** (AC-1..AC-9 functional/test evidence from
`crates/iris-core/src/identity/` + `crates/iris-desktop/src/identity.rs`,
AC-10 redteam review **PASS-with-recorded-deviations**, AC-11 this document).
Workspace **361 tests green** (306 iris-core; `cargo test -p iris-core identity`
passes **68**, of which the identity module itself contributes **67** unit tests
across 8 modules and the 68th is `crypto::keygen::tests::ed25519_identity_is_32_bytes`
caught by the name filter — both counts reproduced by the independent verifier),
3 desktop identity tests, clippy 0 (`--workspace --all-targets`).
**DEC-0010 security control verified satisfied** — no DevCryptoProvider /
no identity / no sealer reachable in the production path.

Four findings are recorded as gated/known_limitations (RT-002/003/004/005 per the
redteam record); two are accepted single-instance/INFO (RT-014, RT-015/016). None
blocks ACCEPT (no CRITICAL/HIGH un-addressed).

---

## Evidence

| AC | Criterion | Status | Evidence |
|----|-----------|--------|----------|
| AC-1 | `identity` module + `peer_id`/`peer_short` derivation compile, table-tested (determinism, distinct-seed, no-MAC rule, P0 16-byte form matches); clippy `-D warnings` 0; workspace green | **PASS** | `crates/iris-core/src/identity/peer_id.rs` (7 tests: `peer_id_is_verifying_key_bytes`, `peer_short_same_key_same_short_id`, `peer_short_is_16_bytes`, `distinct_keys_produce_distinct_short_ids`, `human_uid_is_16_uppercase_base32_chars`, `no_hardware_derived_identity`, `matches_hex_in_p0_abbreviated_envelopes`). PeerId = Ed25519 verifying bytes (32 B, self-authenticating); `peer_short()` = SHA-256(pubkey)[..16] matching P0 abbreviated-envelope hex form. `cargo clippy --workspace --all-targets` = 0 warnings; `cargo test --workspace` = **361 green** |
| AC-2 | Provisioning round-trip: fresh store → `IdentityManager::provision` → restart → `load()` returns identical PeerId + re-derived keys; corrupt blob → loud error (no silent regen); `zeroize` on drop proven by test | **PASS** | `identity/provision.rs` + `identity/store.rs` (14 tests): `provision_then_reload_returns_identical_peer_id`, `provision_is_idempotent_within_session`, `identity_round_trip_and_reload`, `master_key_round_trip`, `seeds_are_zeroized_on_drop` (allocation-wipe proof), `corrupt_blob_fails_loudly_not_silent_regen`, `corrupt_blob_fails_loudly`, `wrong_version_blob_fails_loudly` (x2), `buffer_buffers_zeroed_on_drop`, `serialization_round_trip_matches_layout`. Desktop reload stability: `crates/iris-desktop/src/identity.rs` persisted-PeerId test (3 desktop tests) |
| AC-3 | `KeyAdvertisement` build/verify: valid ad accepted; tampered/bad-sig ad rejected; cross-identity ad (signer ≠ subject) rejected | **PASS** | `identity/advertise.rs` (6 tests): `valid_ad_builds_and_verifies`, `tampered_static_key_fails_verify`, `tampered_counter_fails_verify`, `forged_ad_wrong_signer_rejected` (signer ≠ subject), `small_order_certified_key_rejected_at_build_and_verify`, `serialization_round_trip`. RED-0005 SPKI-style subject block ‖ Ed25519 sig; verify via `verify_strict` (RED-0009 carried) |
| AC-4 | `verify_chain`: trusted-root chain passes; untrusted-root rejected; broken link rejected; expired link rejected; revoked mid-chain rejected; small-order pubkey in chain rejected; length cap enforced | **PASS** | `identity/chain.rs` (13 tests): `trusted_root_self_signed_single_element_passes`, `authorized_child_chain_passes`, `untrusted_root_chain_rejected`, `broken_link_rejected`, `expired_link_rejected`, `stale_counter_link_rejected`, `small_order_pubkey_in_chain_rejected`, `sender_mismatch_rejected`, `empty_chain_rejected`, `overlong_chain_rejected` (cap 8), `undecodable_link_rejected`, `wrong_format_version_link_rejected` (RT-011 gate), `root_certified_key_must_match_store` (RT-011 root-vs-store consistency). RED-0008 key-anchored, trusted-root required, **no PKIX** |
| AC-5 | TrustStore: TOFU bind on first adoption; repeated ad (same key) idempotent; key-change → `KeyChanged` + re-verify required; `verify_peer` promotes to `Verified`; revocation → `Revoked` and blocks all paths | **PASS** | `identity/trust_store.rs` (14 tests): `tofu_bind_then_repeated_ad_is_idempotent`, `resolution_feeds_key_directory`, `verify_peer_promotes_to_verified`, `verify_peer_rejects_key_mismatch` (RT-010 exact-key confirmation), `key_change_warns_and_requires_reverify`, `rotation_adopts_when_counter_strictly_increases`, `stale_counter_earliest_seen_wins` (RT-001: stale replay → Duplicate, never permanent downgrade), `equal_counter_different_key_warns_but_recoverable` (RT-001), `revocation_blocks_all_paths_and_never_auto_heals`, `un_revoke_allows_re_adoption_after_heal` (RT-010 operator healing), `adversarial_ad_rejected`, `small_order_ad_rejected_red_0011`, `expired_ad_rejected` + `expired_within_skew_budget_accepted` (RT-009 skew budget) |
| AC-6 | Rotation: monotonic counter accepted (key_id set), stale counter rejected, equal-counter conflict → earliest-seen-wins; valid-until enforced for old keys; null-rotation revokes + propagates via SCF/gossip seam | **PASS** | `identity/rotate.rs` (9 tests): `rotation_adopts_with_monotonic_counter`, `stale_counter_rejected_earliest_seen_wins`, `equal_counter_conflict_earliest_seen_wins`, `null_rotation_revokes_and_blocks_paths`, `forged_event_rejected`, `small_order_new_key_rejected`, `adopt_rotation_defensive_small_order_recheck` (RT-013), `unknown_peer_rotation_rejected` (RT-012), `serialization_round_trip`. Valid-until enforced at adopt + rotate with `DEFAULT_SKEW_BUDGET_SECS` (RT-009). Propagation: `KeyRotation` envelope type (16) is the wire seam; **inbound key-rotation feed gated to DISCO-001 (RT-003, recorded)** |
| AC-7 | RED-0011: all known small-order X25519 pubkeys rejected at ad/chain boundary (test vectors), valid remote key accepted | **PASS** | `identity/small_order.rs` (4 tests): `classic_small_order_values_are_rejected`, `whole_torsion_subgroup_is_rejected`, `basepoint_is_valid`, `valid_remote_key_accepted`. `is_small_order` guard applied in `advertise::build`/`verify` (AC-3), `chain` (AC-4), `trust_store::adopt`/`rotate` (AC-5/6), and `resolve_x25519` read boundary (RT-011/RED-0011 ride-along) |
| AC-8 | DESKTOP RED-0001: `DesktopEngine` runs with `IrisCryptoProvider`, persisted identity (PeerId stable across engines/restarts), FileKeyStore, `with_sealer` when PG configured; no DevCryptoProvider reachable in production path (compile/test guard) | **PASS** | `crates/iris-desktop/src/identity.rs` (3 tests: real Ed25519/X25519 sign/verify/encrypt/decrypt with `authenticates()==true`, PeerId reload stability, address-to-key resolution from adopted ads). `engine_handle.rs` production path = `IrisCryptoProvider` + `FileKeyStore` identity + `TrustKeyDirectory`; `DevCryptoProvider` confined to `config.dev` test seams (compile/test guard). **Recorded deviation (RT-004)**: `PgStorage::with_sealer` wiring deferred — loopback path uses `MemoryStorage`; `master_key()` exposed for the future PG/StorageKeySealer path (DEC-0010 satisfied either way) |
| AC-9 | Engine E2E with identity: Alice identity → key dir → encrypt to Bob's advertised static key → verify_strict → decrypt; auth_cert_chain validated on authority/P0-style message | **PASS** | `MessageEngine::set_key_directory` seam: recipient-key lookup is directory-backed (`TrustKeyDirectory`); provider E2E encrypt-to-advertised-key → verify_strict → decrypt exercised by desktop provider tests + `resolve_x25519` small-order read filter (RT-011). auth_cert_chain (RED-0008) validation implemented in `identity/chain.rs` (AC-4 matrix). Scope note (RT-002): multi-key rotation decrypt E2E in the engine is deferred to PROTO-001 v2 / RED-0010; single-key E2E satisfied now |
| AC-10 | Redteam SECURITY_REVIEW: identity/key-mgmt/trust/rotation reviewed; no RUSTSEC regressions (NO PKIX family added; keyring NOT added in v1; core2/thin-vec absent); DEC-0010 security control verified satisfied | **PASS** | `engineering/memory/records/IDENT-001_SECURITY_REVIEW.md` (iter 63): findings IDENT-RT-001..016 (**5 HIGH / 7 MEDIUM / 2 LOW / 2 INFO**) — 9 in-scope code findings **FIXED in-pass** (+10 regression tests), 4 gated (RT-002→PROTO-001 v2, RT-003→DISCO-001, RT-004→deviation DEC-0010, RT-005 Windows keystore), RT-014 accepted single-instance, RT-015/016 INFO clean (`verify_strict` everywhere; no PKIX/new crates — RUSTSEC-2026-0104/0098/0099 avoided; keyring deferred; core2/thin-vec absent). **DEC-0010 control VERIFIED satisfied** (no DevCryptoProvider / no identity / no sealer in production deployment path) |
| AC-11 | VERIFY evidence: `IDENT_VERIFICATION.md` v1.0 with AC-1..11 PASS, workspace green count reported, clippy 0, records (RES-0016, DEC-P0010 wire-reconciliation if recorded, CHANGELOG entry) | **PASS** | This document. Workspace **361 green** (306 iris-core incl. 68 identity; 3 desktop identity); clippy 0 (`--workspace --all-targets`). Records: RES-0016 (iter 60), IDENT-001_TEST.md (iter 62b), IDENT-001_SECURITY_REVIEW.md (iter 63), CHANGELOG [0.3.11] (iter 63). Wire-reconciliation: 32 B self-auth v1 kept; 16 B collapse deferred to PROTO-001 v2 / RED-0010 (documented in IDENT_DESIGN.md §wire, no separate DEC-P0010 required) |

---

## Verdict: **IDENT-001 ACCEPTED — all AC-1..11 PASS; node COMPLETE.**

## Independent verifier check

A verifier subagent independently reproduced the AC evidence on 2026-08-15
(iter 63):

- Re-ran `cargo test -p iris-core identity` → **68 passed, 0 failed**
  (identity module inventory: 6 advertise / 13 chain / 7 peer_id / 6 provision /
  9 rotate / 4 small_order / 8 store / 14 trust_store = 67 module tests; the
  68th hit is `crypto::keygen::tests::ed25519_identity_is_32_bytes` via the
  name filter). Verifier verdict: counter-claim confirmed, totals accurate.
- Re-ran `cargo test --workspace` → **361 passed, 0 failed** across 15 suites.
- Re-ran `cargo clippy --workspace --all-targets` → **0 warnings / 0 errors**.
- Inspected the DEC-0010 control claim: production path in `engine_handle.rs::build`
  (lines 153–180) constructs `IrisCryptoProvider` + `TrustKeyDirectory` from the
  persisted `DesktopIdentity` when `config.dev == false`; `DevCryptoProvider` is
  reachable only in the `config.dev == true` test seams. **Claim verified.**
- Independently spot-checked all 9 redteam fixed findings (RT-001/006/007/008/
  009/010/011/012/013) — each code-level fix present with its regression test.
- Compared every AC-1..AC-11 test citation in this document against
  `cargo test -p iris-core identity -- --list`: **no AC row cites a
  non-existent test.**
- Verifier verdict: **APPROVE** (`VERIFIED — APPROVE`). Three documentation-level
  observations recorded (none blocking): (1) "68 identity unit tests" is the
  command-filtered count, module-internal is 67 — documented above; (2)
  `IDENT-001_TEST.md` cites `identity_file_is_0600` which is `#[cfg(unix)]` and
  does not execute on Windows (the 8 store tests that do run match the doc);
  (3) redteam record/CHANGELOG labels "+10 regression tests" while naming 12
  cases — the named-cases count reflects the security-review pass additions and
  the verified net module delta is +9..10 vs the 58-test baseline; totals are
  accurate.

---

## Known limitations (to record at ACCEPT)

Recorded from IDENT_DESIGN.md §14 + IDENT-001_SECURITY_REVIEW.md (RT-series):

- Physical-device key stores (Android Keystore, iOS Keychain, TPM) deferred to
  platform nodes; desktop v1 uses the protected-file store (`FileKeyStore`,
  0600-from-birth / atomic / lock-reclaim / zeroize). OS-keychain = deferred
  RUSTSEC-clean hardening. Windows keystore posture documented (RT-005).
- RT-004: `PgStorage::with_sealer` desktop wiring deferred (loopback v1 =
  `MemoryStorage`); `master_key()` exposed for the future PG/StorageKeySealer
  path. DEC-0010 control satisfied regardless.
- Full KERI/DID event-log + equivocation detection = v2 (BLK-0002).
- 16-byte wire `sender_id` collapse deferred to PROTO-001 v2 / RED-0010.
- TOFU rests on first-contact authenticity; QR/pairing UI deferred to UX nodes
  (verified-tier hook present).
- RT-002: engine multi-key rotation decrypt E2E → PROTO-001 v2 / RED-0010.
- RT-003: inbound `KeyRotation` feed propagation via SCF → DISCO-001.
- RT-014: provision two-file TOCTOU — accepted single-instance risk.
- No formal proof of Ed25519-signed-X25519 in a disconnected mesh (Briar QP cited
  in RES-0016).
- At-rest row sealing remains OPT-IN (`NoSealer` default) until deployments
  configure `with_sealer`.

---

RECORDED_DEVIATIONS: RT-002/003/004/005 gated (no CRITICAL/HIGH un-addressed);
RT-014 accepted single-instance; RT-015/016 INFO clean. Quality process retained
through VERIFY per ACCEPTANCE_POLICY.yaml; human-gate DEC-0009 standing.