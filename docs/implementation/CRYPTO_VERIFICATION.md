# CRYPTO-001 VERIFICATION — Cryptographic Layer

**Node**: CRYPTO-001 (P0 SECURITY, deps PROTO-001 COMPLETE)
**Version**: 1.0
**Date**: 2026-08-15
**Status**: ACCEPT (all AC-1..9 PASS with ratified disposition)
**Sources**: CRYPTO_DESIGN.md v1.0 (AC-1..9, §KAT); RES-0015 (research + DEC-P0001..0004);
  DEC-0009 (gate authorization); CRYPTO-001_SECURITY_REVIEW.md (redteam RED-0001..0012);
  DEC-0010 (operator ratification of RED-0001 tracking); STORAGE.md (at-rest seal);
  ADR-0006/0011 (crypto ADRs); execution-log iter 57/58

---

## Executive Summary

CRYPTO-001 delivers the real cryptographic layer for IRIS: Ed25519 identity
signing, X25519 key agreement, ChaCha20-Poly1305 AEAD, HKDF-SHA256 key
derivation, and at-rest row sealing — implemented behind the existing
`CryptoProvider` seam, wired through the message engine (send-encrypt /
deliver-decrypt-verify) and iris-storage (RowSealer/StorageKeySealer), and
verified against RFC KATs plus engine-level E2E tests.

**Security review (redteam, iter 58)** returned **FAIL-with-condition** with
12 findings (RED-0001..0012). The disposition pass **fixed** RED-0004
(fragmentation broke E2EE — now the strongest part of the design), RED-0002,
RED-0006, RED-0007, RED-0009, RED-0012, **mitigated** RED-0002 fail-open
visibility, and **tracked** RED-0001 (CRITICAL, operator-ratified to IDENT-001),
RED-0003, RED-0005, RED-0008, RED-0010, RED-0011. Workspace **290 green**,
clippy `-D warnings` clean.

All nine acceptance criteria **PASS** (AC-9 = security model verified + review
checkpoint completed + tracked findings accepted).

---

## Acceptance Criteria — Results

| AC | Criterion (CRYPTO_DESIGN §) | Result | Evidence |
|----|------------------------------|--------|----------|
| AC-1 | `crypto` module compiles; crates pinned (x25519-dalek 2.0.1, ed25519-dalek 2.x, chacha20poly1305 0.10.1, hkdf 0.12, sha2 0.10); clippy 0; workspace green | **PASS** | `crates/iris-core/src/crypto/{mod,keygen,x25519,ed25519,aead,kdf,key_directory}.rs`; `Cargo.toml` pins; `cargo clippy --workspace --all-targets -- -D warnings` 0; `cargo test --workspace` **290 green** (2026-08-15) |
| AC-2 | RFC 7748 §5.2 KAT PASS; §6.1 all-zero shared-secret → error | **PASS** | `x25519.rs` KAT (`rfc7748_52_alice_bob_kat`) byte-exact; `diffie_hellman` rejects low-order/all-zero shared secret with error (not silent) |
| AC-3 | RFC 8439 §2.8.2 KAT PASS; AAD tamper + ciphertext tamper → decrypt error; `encode_for_aead` excludes fields 8/12/13/15/16+ | **PASS** | `aead.rs` KAT (`rfc8439_282_vector`); AAD-tamper + ciphertext-tamper tests return `DecryptionFailed`; scope test asserts `encode_for_aead` covers only fields 1–7, 9–11, 14 |
| AC-4 | RFC 8032 §7.1 KAT PASS; `verify_strict` rejects forged sig under weak small-order key; `verify()` not callable | **PASS** | `ed25519.rs` KAT (`rfc8032_71_test1`); `verify_strict_rejects_weak_small_order_key_forgery` (added iter 58, RED-0006); production verifier renamed `verify_strict` — no `verify()` exists (RED-0009) |
| AC-5 | RFC 5869 App A cases 1–3 PASS; sender/recipient identical `message_key`; distinct salts → distinct keys | **PASS** | `kdf.rs` KATs App A.1/.2/.3; `sender_and_recipient_derive_identical_message_keys` (X25519 DH + identical info → identical key); message vs storage salt distinct |
| AC-6 | E2E round trip via seam: sign→encrypt→codec→verify→decrypt recovers plaintext; byte-exact envelope with `encryption_hdr`; tampered wire fails | **PASS** | `tests/crypto_e2e.rs` `m7_encrypted_signed_message_round_trips_e2e` (plaintext recovered, envelope intact on wire); `m7_tampered_wire_frame_never_surfaces_plaintext` (Err, no plaintext) |
| AC-7 | At-rest seal: seal→unseal round-trip; any-byte/AAD tamper → error; per-row nonces unique; no plaintext sender_id/recipient_id/envelope substring in blob (dump-resistance); index columns plaintext; STORE-001 green (default `None`) | **PASS** | `iris-storage/src/seal.rs` + `tests/` (7 seal unit tests) + `tests/pg_store.rs` (3 DB: round-trip, wrong-key `DecryptionFailed`, plaintext-identity-columns-removed); schema v2 migration; STORE-001 suite green |
| AC-8 | Engine integration send→store→route→deliver with IrisCryptoProvider; P0 broadcast not encrypted; DevCryptoProvider still passes existing engine tests | **PASS** | `message_engine/{crypto,mod,fragment}.rs` wiring; M7 suite: round-trip, tampered-frame, and `m7_fragmented_encrypted_message_reassembles_and_verifies` (70 KiB P5 → 2 wire fragments → reassemble → whole-ADU verify → decrypt → plaintext); P0 broadcast path signed-only; DevCryptoProvider regression intact |
| AC-9 | Security model: no recipient-side FS claims anywhere; SECURITY_REVIEW (redteam) checkpoint passes D4 honesty + no libcrux misattribution | **PASS** | `CRYPTO-001_SECURITY_REVIEW.md` (redteam): "honest D4 no-FS disclosure; no misattribution — no libcrux anywhere"; tracked findings accepted per DEC-0010; FS honesty documented in threat model |

---

## Security review disposition (DEC-0010 accepted)

| Finding | Sev | Disposition | Status |
|---------|-----|-------------|--------|
| RED-0001 desktop inert provider / no identity / no sealer | CRITICAL | Operator-ratified tracking to IDENT-001 + security control (DEC-0010): no production deployment with DevCryptoProvider / no identity / no sealer | Ratified |
| RED-0002 missing-key unicast fail-open | HIGH | Mitigated: `msg.sent_unencrypted` warn + `iris.messages.sent_unencrypted_total` metric + threat model; fail-closed deferred to IDENT-001/v2 | Mitigated |
| RED-0003 unsigned `encryption_hdr` strip-downgrade | HIGH | Tracked → PROTO-001/ADR-0011 v2 (signed encryption-intent flag); AEAD+sig already reject payload tamper | Tracked |
| RED-0004 fragmentation breaks E2EE | HIGH | Fixed + proven: distinct wire ids, header carries type+sig, whole-ADU re-verify (M7 fragment E2E) | Fixed |
| RED-0005 identity↔X25519 binding | MEDIUM | Tracked → IDENT-001 v2 (cert-signed key advertisement) | Tracked |
| RED-0006 weak-key KAT evidence gap | MEDIUM | Fixed: genuine verify_strict weak-key forgery test + KAT table corrected | Fixed |
| RED-0007 mixed-signer fragment sets | MEDIUM | Fixed: `FragmentSet::bind_sender` | Fixed |
| RED-0008 `auth_cert_chain` unverified | MEDIUM | Tracked → IDENT-001 v2; v1 UI must not claim authority-verified | Tracked |
| RED-0009 verify/verify_strict naming | LOW | Fixed: renamed `verify_strict` | Fixed |
| RED-0010 16-byte sender_id acceptance | LOW | Tracked: codec tolerant, provider rejects non-32 | Tracked |
| RED-0011 low-order defense-in-depth | LOW | Accepted (accepted deviation; §6.1 + AEAD already fail closed) | Accepted |
| RED-0012 schema comment | INFO | Fixed: schema.rs comment corrected | Fixed |

## Known limitations (accepted at ACCEPT)

1. **RED-0001 security control** (see DEC-0010): desktop v1 remains a development
   harness (per-process identity, no key dir / sealer) until IDENT-001 wires the
   real provider + provisioning + sealer. No production deployment before then.
2. **No recipient-side forward secrecy** in v1 (DEC-P0003 honest disclosure);
   v2 = X3DH/prekeys.
3. **RED-0003**: active attacker can strip the `encryption_hdr` flag (ciphertext
   presented as plaintext) until the signed encryption-intent flag lands
   (PROTO-001 v2). AEAD + signature still reject payload tamper.
4. **RED-0002**: P0 broadcast is intentionally plaintext (no recipient key);
   missing-key unicast signs-only with a high-visibility warning + metric.
5. At-rest row sealing is **opt-in** (`NoSealer` default) — deployments must
   call `PgStorage::with_sealer(...)` once IDENT-001 provisions the master key.

## Verdict: **CRYPTO-001 ACCEPTED — all AC-1..9 PASS; COMPLETE.**

All nine acceptance criteria pass with implementation + test + redteam
evidence. Findings RED-0001..0012 are dispositioned (fixed/mitigated/accepted/
tracked) per CRYPTO-001_SECURITY_REVIEW.md with the operator-ratified security
control (DEC-0010). Workspace 290 green, clippy 0 warnings.