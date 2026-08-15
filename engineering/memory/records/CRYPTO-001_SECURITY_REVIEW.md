# CRYPTO-001 SECURITY_REVIEW (redteam) — findings + dispositions

- **Record ID**: CRYPTO-001 SECURITY_REVIEW
- **Scope**: CRYPTO-001 (Cryptographic Layer), SECURITY_REVIEW stage, iter 58
- **Date**: 2026-08-15
- **Agent**: redteam (adversarial source review)
- **Files**: `crates/iris-core/src/crypto/*`, `message_engine/{crypto,mod,fragment}.rs`,
  `protocol/codec.rs`, `iris-storage/src/{seal,pg,schema}.rs`, `crypto_e2e.rs`,
  `engine_handle.rs`, `CRYPTO_DESIGN.md`
- **Verdict**: **FAIL-with-condition** → **RESOLVED/TRACKED** after disposition pass
  (see dispositions; RED-0003/0005/0008/0010 tracked to IDENT-001/PROTO-001 v2).

> ⚠️ Numbering note: findings RED-0001..0012 **below are CRYPTO-001-scoped** and are
> distinct from the earlier transport-layer records `records/RED-0001.md`
> (transport redteam, 2026-08-13) and `records/RED-0002.md`. Not a collision.

---

## Findings (adversarial, verified against code)

| ID | Severity | Title |
|---|---|---|
| RED-0001 | **CRITICAL** | Only production binary (`iris-desktop`) still runs inert `DevCryptoProvider`; no key directory, no persisted identity, no sealer wired |
| RED-0002 | HIGH | Missing-key unicast silently fails open to plaintext (unobservable before fix) |
| RED-0003 | HIGH | `encryption_hdr` (field 16) unsigned ⇒ active strip-downgrade on deliver path |
| RED-0004 | HIGH | Fragmentation breaks E2EE: (1) dedup swallows fragment 2, (2) AAD fields not restored ⇒ decrypt fails, (3) reassembled ADU never verified |
| RED-0005 | MEDIUM | Identity (Ed25519) ↔ X25519 static key not bound; directory unauth'd ⇒ substitution MITM |
| RED-0006 | MEDIUM | AC-4 weak-key rejection evidence missing; KAT table misattributed §7.1 as weak-key vector |
| RED-0007 | MEDIUM | Reassembly trusts attacker-controlled fragment fields; no sender/continuity check |
| RED-0008 | MEDIUM | `auth_cert_chain` (field 18) opaque + unvalidated ⇒ unverifiable "authority-certified" SOS |
| RED-0009 | LOW | `crypto::ed25519::verify` naming contradicts "verify() never called" invariant |
| RED-0010 | LOW | 16-byte `sender_id` accepted by codec but rejected by provider (hidden incompat) |
| RED-0011 | LOW | X25519 rejects all-zero only, not full low-order set (defense-in-depth) |
| RED-0012 | INFO | `payload_size` schema comment lies for sealed rows |

## What passed

verify_strict-only on the provider call site; AAD/KDF scope correct for the
non-fragmented path (M7); nonce/ephemeral hygiene (fresh ephemeral + OsRng 12B
nonce per message, §6.1 reject in the single shared DH helper); verify-before-
decrypt ordering; at-rest seal semantics (AAD binding, per-row nonce uniqueness,
wrong-key fails closed, migration drops plaintext identity columns, dump-
resistance); honest D4 no-FS disclosure; RustCrypto pinning only (DISC-0010 no
misattribution — no libcrux anywhere).

## Disposition pass (implemented this iteration)

- **RED-0004 FIXED + PROVEN**: distinct per-fragment wire ids
  (`derive_fragment_message_id`, SHA-256(original‖index)[..16]); fragment header
  now carries `orig_payload_type` + the original whole-ADU signature;
  `FragmentAssembler::feed` restores message_id/payload_type/payload_ref/signature;
  `process_incoming` re-verifies the reassembled envelope; `FragmentSet::bind_sender`
  rejects mixed-signer sets (RED-0007); two latent routing bugs fixed
  (fragmentable transport selection + bottleneck-MTU clamp). Engine-level E2E:
  `m7_fragmented_encrypted_message_reassembles_and_verifies` PASS.
- **RED-0002 MITIGATED**: `msg.sent_unencrypted` warn + `iris.messages.sent_unencrypted_total`
  metric; documented threat model. Fail-closed policy deferred to IDENT-001/v2.
- **RED-0006 FIXED**: `verify_strict_rejects_weak_small_order_key_forgery` test;
  KAT table corrected.
- **RED-0009 FIXED**: `crypto::ed25519::verify` → `verify_strict`; docs corrected.
- **RED-0012 FIXED**: schema.rs comment corrected.

## Tracked (v2 / IDENT-001 / PROTO-001)

- **RED-0001 (CRITICAL)**: **RATIFIED by operator (DEC-0010, 2026-08-15)** —
  tracking to IDENT-001 approved with security control: *no production
  deployment with DevCryptoProvider / no identity / no sealer*. CRYPTO-001
  core (provider + engine + at-rest seal) is complete and tested; ACCEPTED.
- RED-0003 → PROTO-001/ADR-0011 v2: signed encryption-intent flag.
- RED-0005 → IDENT-001 v2: Ed25519 signs its X25519 key (certified key advertisement).
- RED-0008 → IDENT-001 v2: authority-chain verification; v1 UI must not claim
  authority-verified.
- RED-0010 → codec narrowing or explicit unverifiable disposition (tracked).
- RED-0011 → optional small-order point rejection (accepted deviation; AEAD
  tag + §6.1 already fail closed).

## Evidence

- Workspace **290 tests green** (up from 284: +3 fragment, +1 weak-key,
  +1 selection fragmentable, +1 M7 fragment E2E), clippy `-D warnings` 0.
- Design doc updated: `CRYPTO_DESIGN.md` SECURITY_REVIEW dispositions + threat model.
- Records: this file; `execution-log.md` iter 58; CHANGELOG 0.3.6.