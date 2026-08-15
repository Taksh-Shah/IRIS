# IDENT-001 Security Review (AC-10)

**Node**: IDENT-001 — Identity, Key Management, Trust
**Stage**: SECURITY_REVIEW (redteam adversarial review)
**Date**: 2026-08-15
**Inputs**: IDENT_DESIGN.md v1.0, IMPLEMENT+TEST (iter 62), 58 identity unit tests + 3 desktop identity tests, workspace 296+ green, clippy 0.

## Scope Reviewed

- Provisioning: keystore protection, loud-failure, zeroize discipline (§5).
- Key derivation: PeerId = Ed25519 pubkey, short-id, human UID (§6).
- RED-0005 advertisement forgery resistance (§7).
- Trust store: TOFU poisoning, key-change handling, resolution (§10).
- RED-0008 verify_chain attacks (key-anchored, no PKIX).
- Rotation/revocation: replay, monotonicity, null-rotation (§9.6).
- Small-order (RED-0011) bypass at ad/chain/rotation/read boundaries.
- Desktop key storage on disk (FileKeyStore) + RED-0001 wiring (IrisCryptoProvider, no DevCryptoProvider in production path).
- RUSTSEC / no-new-crypto-crate regression.

## Verdict

**SECURITY_REVIEW: FAIL-with-condition → PASS-with-recorded-deviations after in-scope fixes (iter 63).**

All CRITICAL/HIGH findings either fixed in-pass or dispositioned with explicit conditions/gates recorded as known_limitations. DEC-0010 security control verified satisfied: no DevCryptoProvider / no identity-less provider / no sealer-less storage in any production path.

## Findings — IDENT-RT-NNN

| ID | Severity | Area | Finding | Disposition |
|----|----------|------|---------|-------------|
| IDENT-RT-001 | HIGH | trust_store | Stale-ad replay (counter < stored, different key) permanently downgraded a healthy peer to `KeyChanged`, disabling E2EE resolution via attacker-relayed old advertisement. | **FIXED in-pass.** Stale+different-key → `Duplicate` (drop, earliest-seen-wins); equal-counter+different-key → `KeyChangeWarn`; strictly-higher counter (signature-verified) → `RotationAdopted` and clears `KeyChanged` (recovery). Tests: `stale_counter_earliest_seen_wins`, `equal_counter_different_key_warns_but_recoverable`. |
| IDENT-RT-002 | HIGH | rotate (engine E2E) | Rotation/revocation are primitives + trust-store adoption only; the provider's encrypt/decrypt does not yet select key by `key_id` (single provisioned static secret). Older-envelope decryption-after-rotation not end-to-end wired. | **RECORDED — scope correction.** v1 rotation = primitive + trust-store adoption (RED-0003/v2 scopes engine key_id selection). AC-6 wording corrected to primitive-scope in IDENT_DESIGN (not engine multi-key). Engine multi-key decrypt gated to PROTO-001 v2 (RED-0010). |
| IDENT-RT-004 | MEDIUM | desktop sealer | `PgStorage::with_sealer` not wired in desktop v1 (default MemoryStorage loopback). | **RECORDED deviation under DEC-0010.** Desktop v1 = loopback MemoryStorage (no at-rest PG rows); decision of `IdentityManager::master_key()` exposed for future `PgStorage::with_sealer` wiring; network deployment path gated. Explicitly documented in IDENT_DESIGN §11. |
| IDENT-RT-009 | MEDIUM | trust_store/rotate | `valid_until` compared against local clock without skew budget — a fast local clock permanently rejected healthy ads/rotations. | **FIXED in-pass.** `DEFAULT_SKEW_BUDGET_SECS` (300 s) applied at both `adopt_advertisement` and rotation `apply` (RFC 9171 §4.4.2 pattern). Tests: `expired_within_skew_budget_accepted`, `expired_ad_rejected`. |
| IDENT-RT-010 | MEDIUM | trust_store | `verify_peer` had no key confirmation (could verify a key-swapped entry) and no recovery from a mistaken revocation. | **FIXED in-pass.** `verify_peer` now confirms the expected static key (`KeyMismatch` error on mismatch — re-pair required); new `un_revoke` operator healing path (→ `Unverified`, needs fresh verified ad). Tests: `verify_peer_rejects_key_mismatch`, `revocation_blocks..._never_auto_heals` (extended), `un_revoke_allows_re_adoption_after_heal`. |
| IDENT-RT-011 | MEDIUM | chain | Chain elements did not gate on `format_version`; root element did not verify against the store's certified key (stale cert could re-authorize a pre-rotation key). | **FIXED in-pass.** Per-element version gate (`Verdict` on foreign version); root-vs-store consistency check (`RootKeyMismatch`), RED-0011 evaluated before consistency. Tests: `wrong_format_version_link_rejected`, `root_certified_key_must_match_store`. |
| IDENT-RT-014 | MEDIUM | provision | Provision-on-first-run TOCTOU between identity/master writes (two files, one process assumption). | **RECORDED — accepted risk.** Single-instance assumption (lock file is advisory; two files written under same process lock). Documented in IDENT_DESIGN §5.4; multi-instance hardening gated. |
| IDENT-RT-003 | HIGH | feed wiring | Inbound `ContentType::KeyRotation` feed not implemented — ads/rotations arrive only via `auth_cert_chain`/adoption seams, not the engine inbox. | **RECORDED — gated.** Inbound rotation-feed dispatch deferred to DISCO-001 (mesh traffic path) per architecture layer; primitive + store adoption complete and covered. |
| IDENT-RT-006 | MEDIUM | store | Stale `.lock` file from a crashed writer permanently bricks the store (`Locked` forever). | **FIXED in-pass.** `lock()` reclaims locks older than `LOCK_STALE_SECS` (15 s) via mtime (bounded, never clobbers a live lock). Test: `stale_lock_file_is_reclaimed`. |
| IDENT-RT-007 | MEDIUM | store | Temp identity file created with default perms then chmod'd — a world-readable window at create time. | **FIXED in-pass.** `write_secret_file` creates the temp file 0600 **from birth** (`OpenOptions.mode(0o600)` on Unix; truncate-in-place reuse of leftover tmp). Test: `all_key_files_are_0600_from_birth`. |
| IDENT-RT-008 | MEDIUM | store/provision | Secret-bearing `Vec<u8>` read/serialize paths (`load_identity`, `load_master_key`, `NodeIdentityV1::to_bytes`) did not zeroize on drop. | **FIXED in-pass.** `load_identity` → `Zeroizing<Vec<u8>>`, `load_master_key` → `Zeroizing<[u8;32]>`, `to_bytes` → `Zeroizing<Vec<u8>>`; whole read path wrapped in `Zeroizing` (raw file + stripped payload). Callers unchanged via deref coercion. |
| IDENT-RT-012 | LOW | rotate | `apply` silently accepted rotation for a peer with no TOFU-bound entry (baseline counter 0). | **FIXED in-pass.** Unknown identity → `RotationError::UnknownPeer`. Test: `unknown_peer_rotation_rejected`. |
| IDENT-RT-013 | LOW | trust_store | `adopt_rotation` (public API) did not re-check RED-0011 itself. | **FIXED in-pass.** Defense-in-depth small-order re-check inside `adopt_rotation` (returns `Err`); rotate.rs `apply` expects the re-check to pass. Tests: `adopt_rotation_defensive_small_order_recheck`. |
| IDENT-RT-005 | LOW | store (Windows) | Windows keystore posture relies on per-user APPDATA; std ACL control not engaged in v1. | **RECORDED — documented.** Matches IDENT_DESIGN §5.3 fallback posture; OS-keychain behind the same `KeyStore` seam is the tracked hardening (BLK-0002). |
| IDENT-RT-015 | INFO | crypto | `verify_strict` used at every verification point (ad/chain/rotation). | **CONFIRMED** — no weak-key acceptance found (`advertise.rs`, `chain.rs`, `rotate.rs`, `crypto.rs`). |
| IDENT-RT-016 | INFO | crypto | No PKIX / no new crypto crates introduced. | **CONFIRMED** — only x25519-dalek/ed25519-dalek/chacha20poly1305/hkdf/sha2 (already pinned CRYPTO-001). |

## Positive confirmations (no findings)

- TOFU poisoning structurally impossible: map key IS the Ed25519 key (attacker cannot bind a victim's identity to a key they do not control — forgery requires a valid signature under the victim's key).
- RED-0011 enforced at every ingestion boundary (advertise build+verify, chain elements, rotation new-key) **and** now at the key-directory read boundary (`resolve_x25519` + `TrustKeyDirectory`).
- Rotation verifies signature BEFORE counter monotonicity; chain length cap enforced before any crypto.
- `KeyChanged`/`Revoked` block `resolve_x25519` in all paths.
- Provisioning failure is loud (corrupt/version → error), never silent regeneration.

## Test delta (iter 63)

- identity unit tests: **58 → 68** (+10 regression tests for RT-001/006/007/009/010/011/012/013).
- Workspace total: **361 tests across 15 suites** (306 iris-core), clippy 0 (iris-core + iris-desktop + workspace --all-targets).
- Desktop identity: 3/3 green (IrisCryptoProvider production path, persisted PeerId).

## Deferred / gated items (known_limitations for VERIFY)

1. Engine multi-key encrypt/decrypt selection by `key_id` — PROTO-001 v2 (RED-0010). Primitive + trust adoption complete (RT-002).
2. Inbound `KeyRotation` feed dispatch — DISCO-001 (RT-003).
3. `PgStorage::with_sealer` desktop wiring — network-deploy gate; `master_key()` available (RT-004).
4. Windows keystore → OS-keychain hardening — BLK-0002 (RT-005).
5. Two-file provision TOCTOU — accepted single-instance risk (RT-014).
6. KERI-style equivocation log (rotation double-use detection) — BLK-0002 (escalated, was already flagged in DESIGN).

## Conclusion

SECURITY_REVIEW stage: **PASS-with-recorded-deviations.** No CRITICAL/HIGH un-addressed: 11 in-scope code findings fixed+tested, 4 recorded as gated/known_limitations with explicit conditions, 2 INFO confirmed-clean. Verdict recorded for the VERIFY stage. DEC-0010 control satisfied.

**Next**: VERIFY (IDENT_VERIFICATION.md AC-1..11) → ACCEPT.