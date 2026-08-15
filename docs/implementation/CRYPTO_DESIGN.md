# CRYPTO-001 Design — Cryptographic Layer (v1)

**Document ID**: IRIS-CRYPTO-DESIGN-001
**Version**: 1.0
**Node**: CRYPTO-001 (P0 SECURITY, deps PROTO-001 COMPLETE)
**Date**: 2026-08-14
**Research basis**: RES-0015 (RUSTSEC audit 2026-08-14 + 2026 constrained-node SOTA +
DEC-P0001..0004); RFC 7748, RFC 5869, RFC 8032, RFC 8439 (Level 1); ADR-0002
(Ed25519 identity), ADR-0006 (ChaCha20-Poly1305, corrected), ADR-0011 (envelope
signature scope, issue 2 resolved); MESSAGE_ENVELOPE.md (encryption_hdr wire
contract); DISC-0001..0003, DISC-0010.
**Supersedes/extends**: `crates/iris-core/src/message_engine/crypto.rs`
(`DevCryptoProvider` placeholder — this design replaces the inert backend with a
real `IrisCryptoProvider` behind the same seam); ADR-0006 Implementation Notes
(crate + KDF pins, per-message nonce).

---

## Scope

Implement the real cryptographic layer behind the existing `CryptoProvider`
seam: Ed25519 signing/verification (RFC 8032), X25519 key agreement (RFC 7748),
ChaCha20-Poly1305 AEAD (RFC 8439), HKDF-SHA256 key derivation (RFC 5869), and
the at-rest row seal for `iris-storage` (STORE-001). Swaps the inert
`DevCryptoProvider` for a real provider with **zero engine changes** (the seam
contract is preserved; the engine keeps its R1 no-hand-rolled-crypto guarantee).

**Out of scope (v1)**: recipient-side forward secrecy (v2 — X3DH-style prekeys /
Double Ratchet, BLK-0002); group encryption (type 15 GROUP_KEY); P0 SOS
broadcast encryption (no single recipient key — broadcast is signed + authority-
certified, not E2EE, per MESSAGE_ENVELOPE.md field 18); Android Keystore key
provisioning/attestation (IDENT-001); key distribution beyond an in-memory
`KeyDirectory` seam (IDENT-001 + DISCO-001 follow-up); HPKE (RFC 9180) envelope
replacement (would change wire format — v2 candidate, RES-0015 §Gaps).

## Design decisions (RES-0015 D1–D5, ratified)

1. **D1 (DEC-P0001) — per-message ephemeral X25519, ECIES-like.** Fresh
   ephemeral X25519 keypair + random 96-bit nonce per message; no session state,
   no counter persistence, nonce collision risk bounded per-message-key.
   ADR-0006 "per-session" wording already corrected to per-message.
2. **D2 (DEC-P0002) — RFC 5869 HKDF-SHA256** via RustCrypto `hkdf` + `sha2`;
   HKDF-BLAKE3 rejected (FAIL-0003/DISC-0002). BLAKE3 = non-crypto hashing only.
3. **D3 (DEC-P0004) — at-rest row seal** in `iris-storage`: storage key
   HKDF-SHA256(master, "iris-at-rest-v1"); ChaCha20-Poly1305 per-row random
   12-byte nonce; content/metadata sealed, index columns plaintext.
4. **D4 (DEC-P0003) — FS honesty.** Security-model statement: v1 provides
   sender-authentication + confidentiality against passive eavesdroppers +
   sender-side FS; **NO recipient-side FS** (no claim in code or docs).
   Recipient-FS is v2 (BLK-0002).
5. **D5 — crate pins** (RUSTSEC-clean at 2026-08-14): `x25519-dalek = "2.0.1"`
   (→ curve25519-dalek ≥4.1.3, RUSTSEC-2024-0344), `ed25519-dalek = "2"`
   (2.x line, MSRV 1.60, edition 2021 — matches workspace; 3.0.0 deferred:
   edition 2024 + MSRV 1.85), `chacha20poly1305 = "0.10.1"` (RustCrypto, RFC
   8439; libcrux RUSTSEC-2026-0124 is NOT this crate — DISC-0010),
   `hkdf = "0.12"`, `sha2 = "0.10"`. Mandatory hygiene: RFC 7748 §6.1 all-zero
   shared-secret reject (hpke-rs RUSTSEC-2026-0072 lesson); validate X25519
   input bytes (32 bytes, clamped — libcrux-ecdh RUSTSEC-2026-0023 lesson);
   Ed25519 `verify_strict()` ONLY (weak-key guard, RUSTSEC-2022-0093 context).

## Module layout

```
crates/iris-core/src/crypto/
  mod.rs        — module docs, re-exports, CryptoError taxonomy, KDF domain strings
  keygen.rs     — keypair generation: X25519 static, Ed25519 identity, ephemeral X25519
  x25519.rs     — DiffieHellman: 32-byte input validation + RFC 7748 §6.1 all-zero reject
  ed25519.rs    — sign() over encode_for_signing(); verify_strict() only (no verify())
  aead.rs       — ChaCha20-Poly1305 encrypt/decrypt with AAD (RFC 8439 §2.8.2)
  kdf.rs        — message_key() (per-message) + storage_key() (at-rest), RFC 5869
  key_directory.rs — KeyDirectory seam: NodeId (recipient_id) → X25519 pubkey
crates/iris-core/src/protocol/codec.rs   — ADD encode_for_aead() (AAD seam; additive)
crates/iris-storage/src/seal.rs          — RowSealer trait + PgRowSealer (uses iris_core::crypto)
crates/iris-storage/src/pg.rs            — wire optional sealer into persist/load/get_queue
crates/iris-storage/src/schema.rs        — sealed envelope_cbor column semantics
crates/iris-core/src/message_engine/crypto.rs — add IrisCryptoProvider (keeps trait + DevCryptoProvider)
```

No new crates. `iris-storage` already depends on `iris-core`; the seal primitives
live in `iris_core::crypto` and the `RowSealer` glue in `iris-storage`.

## Wire-format contract (MUST NOT break PROTO-001)

`encryption_hdr` (field 16) — **unchanged**, already implemented + byte-tested in
`codec.rs`:

```
encryption_hdr = {
    1: bytes(32),  ephemeral_pubkey   -- X25519 ephemeral public key
    2: bytes(12),  nonce              -- ChaCha20-Poly1305 nonce (random per message)
    3: bytes(4)?   key_id             -- recipient key id (None in v1; reserved for prekey/rotation)
}
```

### Encryption flow (send path)

1. Resolve `recipient_id` → recipient X25519 public key via `KeyDirectory`
   (IDENT-001 populates; CRYPTO-001 tests use an in-memory directory).
2. Generate fresh ephemeral X25519 keypair; `shared = DH(eph_priv, recipient_x25519_pub)`.
3. **RFC 7748 §6.1**: if `shared` is all-zero → abort (error, message not sent).
4. `key = kdf::message_key(shared, message_id)` — see KDF contract.
5. `nonce = random 12 bytes` (os_rng).
6. `payload_ciphertext = aead::encrypt(key, nonce, plaintext, aad)` where
   `aad = codec::encode_for_aead(envelope_without_payload_13_hash_12)` — see AAD contract.
7. Set `envelope.payload = ciphertext`, `payload_size = ciphertext.len()`,
   `payload_hash = BLAKE3(ciphertext)` (field 12, non-crypto hash), `encryption_hdr`.
8. `crypto.sign(envelope)` → Ed25519 over `encode_for_signing()` (fields 1–7, 9–14;
   hop_count 8 excluded per ADR-0011).

### Decryption flow (deliver path)

1. Verify Ed25519 `verify_strict()` over `encode_for_signing()` — already the
   engine seam (`crypto.verify`). Tampered ciphertext or header fails here.
2. `shared = DH(local_recipient_x25519_priv, hdr.ephemeral_pubkey)`; all-zero reject.
3. `key = kdf::message_key(shared, message_id)` (same derivation as sender).
4. `plaintext = aead::decrypt(key, hdr.nonce, ciphertext, aad)` where
   `aad = codec::encode_for_aead(envelope)` — recomputed on the received envelope.
5. Deliver plaintext to the app layer.

### AAD contract (new seam `encode_for_aead`)

- **Fields**: 1–7, 9–11, 14 — i.e., the signing-scope fields **minus** 12
  (`payload_hash`, ciphertext-dependent) and 13 (`payload`, the ciphertext
  itself). Canonical CDE (RFC 8949 §4.2), same encoder as `encode_for_signing`.
- **Excluded**: 8 `hop_count` (relays increment it — must not break recipient
  AAD), 12 `payload_hash`, 13 `payload`, 15 `signature`, 16+ extensions.
- **Rationale**: AAD binds the ciphertext to the immutable message identity
  (version, message_id, sender, recipient, priority, ttl, timestamp, max_hops,
  payload_type, payload_size, payload_ref) so a ciphertext cannot be swapped
  between messages even if signature verification were bypassed locally.
  Computable BEFORE encryption (no circular dependency).
- **Test**: `aad_changes_on_message_identity_fields()` + tamper test
  (flipping message_id/recipient_id must fail decryption).

### Signature scope (unchanged, ADR-0011)

`encode_for_signing()` covers fields 1–7, 9–14 (excludes 8 hop_count, 15
signature, 16+). PROTO-001 byte-exact vectors must stay green — `encode_for_aead`
is additive and does NOT modify `encode_for_signing`.

## KDF contracts (RFC 5869)

### Per-message key (`message_key`)

```
HKDF::new(Some(b"iris-message-key-v1"), shared_secret)   // extract
  .expand(&info, 32)                                     // expand → AEAD key
info = message_id (16 bytes) ‖ direction (1 byte: 0x00 = MESSAGE)
```

- Both sides derive identical keys: same salt, same IKM (ECDH symmetric), same
  `info`. The `direction` byte is 0x00 for all v1 messages (reserved 0x01=SEND /
  0x02=RECV for v2 bidirectional session mode); documented, not yet exercised.
- KAT: RFC 5869 Appendix A cases 1–3 (crate-level) + integration:
  `sender_and_recipient_derive_same_key`.

### At-rest storage key (`storage_key`)

```
HKDF::new(Some(b"iris-at-rest-v1"), master_key)   // extract
  .expand(&[], 32)                                 // expand → row AEAD key
master_key = IDENT-001 provisioned (Android Keystore TEE/StrongBox; desktop OS keychain)
```

- Salt distinct from the message-key salt (domain separation).
- The master key NEVER touches the wire; only derived storage keys are used.

## At-rest row seal (D3 / DEC-P0004)

### Threat model

DB-level compromise (dump, backup theft, SQLi on admin path) must not expose
payload ciphertext or the local contact graph. In-app compromise is out of
scope (the app can decrypt anyway). Payload is already E2EE end-to-end (D1);
at-rest additionally protects metadata + ciphertext-at-dump.

### Sealed vs plaintext columns

| Column | Treatment | Reason |
|---|---|---|
| `message_id` (PK) | plaintext | routing/reap key, equality lookup only |
| `priority`, `status`, `created_at`, `expires_at` | plaintext | GC/quota/queue ordering (operational, relays see equivalents) |
| `payload_size`, `hop_count`, `max_hops` | plaintext | operational routing metadata (wire-visible anyway) |
| `envelope_cbor` | **sealed** | contains full envelope: payload ciphertext + sender/recipient/timestamp |
| `sender_id`, `recipient_id`, timestamp columns | **removed from plaintext schema** | contact-graph leakage; now inside the sealed blob |

Schema note: the current `sender_id`/`recipient_id`/`created_at` columns are
redundant with `envelope_cbor` (which holds the complete envelope). Under the
seal they become **sealed columns** (no plaintext BYTEA); `created_at` remains
plaintext only as the operational timestamp (derived from envelope `timestamp`).
`idx_messages_recipient` is dropped (recipient_id is inside the sealed blob; no
plaintext recipient queries exist in `pg.rs` today).

### RowSealer contract

```rust
pub trait RowSealer: Send + Sync + 'static {
    /// Seal one row. Returns opaque bytes: nonce(12) ‖ ciphertext ‖ tag(16).
    fn seal_row(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, StorageError>;
    fn unseal_row(&self, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, StorageError>;
}
```

- `PgRowSealer` wraps `iris_core::crypto::{kdf::storage_key, aead}` with a
  per-row **random 12-byte nonce** (os_rng) — never reused, never derived.
- **AAD** = canonical bytes of the plaintext index columns `message_id ‖
  priority ‖ expires_at` — binds the seal to its row (row-swap defense).
- `PgStorage` gains `Option<Arc<dyn RowSealer>>` (default `None` = current
  behavior; all existing STORE-001 tests keep passing). When `Some`:
  `persist` seals `envelope_cbor`; `load`/`get_queue` unseal before
  `codec::decode`. Quota accounting uses the **unsealed** projected size
  (unchanged eviction semantics).

## CryptoProvider integration (engine seam)

`message_engine::crypto::CryptoProvider` trait is **preserved unchanged**
(`sign`, `verify`, `encrypt`, `decrypt`, `authenticates`). New implementation:

```rust
pub struct IrisCryptoProvider {
    identity: Arc<NodeIdentity>,        // Ed25519 signer + X25519 static keys
    directory: Arc<dyn KeyDirectory>,   // recipient_id → X25519 pubkey
}
```

- `authenticates() -> true`.
- `sign`/`verify` delegate to `crypto::ed25519` (verify_strict only) over
  `codec::encode_for_signing`.
- `encrypt(plaintext, recipient)` — `recipient` is the **recipient X25519 public
  key** (32 bytes), resolved by the caller via `KeyDirectory` (engine/IDENT-001
  seam). Returns `(ciphertext, EncryptionHdr { ephemeral_pubkey, nonce, key_id })`.
- `decrypt(ciphertext, hdr)` — local static X25519 private key, `message_key`,
  AAD = `encode_for_aead`, tag check on every call.
- `KeyDirectory` seam in `crypto/key_directory.rs`:
  `fn x25519_pubkey(&self, node_id: &[u8; 32]) -> Option<[u8; 32]>`.
  v1: in-memory `MemoryKeyDirectory` (test/dev + desktop local contacts);
  IDENT-001/DISCO-001 deliver the real distribution.
- Engine changes: wire the send path to call `crypto.encrypt` before `sign`,
  and the deliver path to `crypto.decrypt` after `verify` (engine owns the
  policy: `encryption_hdr.is_some()` → decrypt; absent → plaintext, e.g. P0
  broadcast). No `CryptoProvider` trait change — `DevCryptoProvider` remains for
  non-crypto tests.

## P0 SOS broadcast (explicit non-encryption)

P0 SOS to `EMERGENCY_BROADCAST` has no single recipient X25519 key → **never
E2EE-encrypted**. Integrity/authenticity via Ed25519 signature + optional
`auth_cert_chain` (field 18). Documented; test `p0_sos_broadcast_not_encrypted`.

## KAT line-up (node acceptance evidence)

| Standard | Vector | What it proves |
|---|---|---|
| RFC 7748 §5.2 | X25519 Alice/Bob KAT | DH matches spec; clamping correct |
| RFC 7748 §6.1 | all-zero shared-secret | `diffie_hellman` with low-order/all-zero pubkey → **error** (not silent) |
| RFC 8439 §2.8.2 | ChaCha20-Poly1305 AEAD vector | encrypt matches spec byte-exactly |
| RFC 8032 §7.1 | Ed25519 TEST 1 vector | sign/verify_strict match spec |
| RFC 8032 weak-key | small-order (all-zero) verifying key | `verify_strict` rejects a forged signature under a weak key (weak-key guard) |
| RFC 5869 App A | cases 1–3 | HKDF-SHA256 extract/expand vectors |

Plus structural tests: ciphertext/header tamper → decrypt fails; nonce
uniqueness per row; AAD binding; sender/recipient same-key derivation;
`DevCryptoProvider` regression (engine tests unchanged); workspace green.

## Acceptance criteria (IMPLEMENT stage)

- **AC-1** `crypto` module compiles; `x25519-dalek 2.0.1`, `ed25519-dalek 2`,
  `chacha20poly1305 0.10.1`, `hkdf 0.12`, `sha2 0.10` in
  `iris-core/Cargo.toml`; clippy 0 warnings; workspace tests green.
- **AC-2** RFC 7748 §5.2 KAT PASS; §6.1 all-zero shared-secret → error PASS.
- **AC-3** RFC 8439 §2.8.2 KAT PASS; AAD tamper + ciphertext tamper → decrypt
  error PASS; `encode_for_aead` excludes fields 8/12/13/15/16+ (scope test).
- **AC-4** RFC 8032 §7.1 KAT PASS; `verify_strict` rejects a forged signature
  under a weak small-order key (`ed25519::tests::verify_strict_rejects_weak_*`);
  `verify()` is NOT callable — the only production verifier wraps `verify_strict`.
- **AC-5** RFC 5869 App A cases 1–3 PASS; sender/recipient derive identical
  `message_key`; distinct salts → distinct keys (message vs storage).
- **AC-6** End-to-end round trip through the seam: `IrisCryptoProvider`
  sign→encrypt→codec→verify→decrypt recovers plaintext; envelope
  byte-exact through `codec::encode/decode` with `encryption_hdr`; tampered
  wire → verify/decrypt fails.
- **AC-7** At-rest seal: `PgRowSealer` seal→unseal round-trip; tamper (any byte
  of sealed blob or AAD) → error; per-row nonces unique; sealed blob does NOT
  contain plaintext `sender_id`/`recipient_id`/envelope substring
  (dump-resistance test); index columns remain plaintext (schema test);
  STORE-001 existing tests green (sealer default `None`).
- **AC-8** Engine integration: send→store→route→deliver with `IrisCryptoProvider`
  (M-series test); P0 broadcast not encrypted; `DevCryptoProvider` still passes
  all existing engine tests.
- **AC-9** Security model: no code, doc, or comment claims recipient-side FS;
  SECURITY_REVIEW (redteam) checkpoint verifies D4 honesty + no misattribution
  of libcrux advisories (DISC-0010).

## SECURITY_REVIEW dispositions (redteam, iter 58)

Findings recorded in `engineering/memory/records/` (RED-0001..0012). Verdict was
**FAIL-with-condition**; the disposition below resolves or tracks each:

| Finding | Sev | Disposition |
|---|---|---|
| RED-0001 desktop ships inert `DevCryptoProvider` | **CRITICAL** | **Operator gate.** CRYPTO-001 delivers the provider + engine integration + at-rest seal, all test-backed. Wiring the *deployable binary* (identity persistence + key provisioning + sealer) is IDENT-001 scope; until then the desktop v1 is documented as unprotected across launches (identity regenerates per process). **ACCEPT depends on the operator ratifying this tracking.** |
| RED-0002 missing-key unicast fails open | HIGH | Fixed in-pass: high-visibility `msg.sent_unencrypted` warning + dedicated metric `iris.messages.sent_unencrypted_total`; documented in threat model (below). P0 broadcast remains intentionally plaintext. |
| RED-0003 unsigned `encryption_hdr` strip-downgrade | HIGH | Tracked to PROTO-001/ADR-0011 v2 (signed encryption-intent flag). AEAD + signature already reject payload tamper; the residual is an active attacker presenting ciphertext as plaintext. Accepted risk for v1, recorded. |
| RED-0004 fragmentation breaks E2EE | HIGH | **Fixed + proven in-pass**: distinct per-fragment wire ids (`derive_fragment_message_id`) so dedup no longer swallows fragment 2; fragment header carries `orig_payload_type` + whole-ADU signature; reassembly restores AAD fields; `process_incoming` re-verifies the reassembled envelope; two latent routing bugs fixed (fragmentable selection + bottleneck MTU). E2E: `m7_fragmented_encrypted_message_reassembles_and_verifies`. |
| RED-0005 identity↔X25519 not bound in key dir | MEDIUM | Tracked to IDENT-001 v2 (identity cert-signed key advertisement). |
| RED-0006 weak-key KAT evidence gap | MEDIUM | **Fixed in-pass**: `verify_strict_rejects_weak_small_order_key_forgery` test + KAT table corrected (§7.1 is a normal vector; weak-key row added). |
| RED-0007 mixed-signer fragment sets | MEDIUM | **Fixed in-pass**: `FragmentSet::bind_sender` refuses mixed-signer sets. |
| RED-0008 `auth_cert_chain` (field 18) unverified | MEDIUM | Tracked to IDENT-001 v2 (authority-chain verification); v1 UI must not claim authority-verified (note in DESKTOP_DESIGN.md). |
| RED-0009 naming `verify` ↔ `verify_strict` | LOW | **Fixed in-pass**: renamed to `verify_strict`; module docs corrected (RED-0009). |
| RED-0010 16-byte `sender_id` acceptance | LOW | Tracked: codec keeps 16|32 tolerance, provider explicitly rejects non-32 (`KeyUnavailable`). |
| RED-0011 X25519 low-order defense-in-depth | LOW | Accepted: RFC 7748 §6.1 all-zero + AEAD tag fail-closed; small-order set hardening is optional (hpke RUSTSEC-2026-0072 covered). |
| RED-0012 `payload_size` schema comment | INFO | **Fixed in-pass** (schema.rs comment). |

### Threat model (RED-0002 fail-open, explicit)

Addressed unicast whose recipient X25519 key is absent from `KeyDirectory`
**transits relays in plaintext** (signed, unwrapped). v1 engine policy makes
the downgrade **observable** (`msg.sent_unencrypted` + metric) but does not
fail closed (would break unkeyed desktop/relay operation). v2/IDENT-001 must
choose fail-closed or explicit per-message override. This is NOT the P0 SOS
case (which is intentionally plaintext).

## Known limitations / v2 lines

1. **No recipient-side forward secrecy** (D4/DEC-P0003) — documented, redteam-checked.
2. **Key distribution** minimal in v1 (`KeyDirectory` in-memory + desktop local
   contacts); real distribution via IDENT-001 + DISCO-001 extension (v2).
3. **P0 SOS broadcast unencrypted** by design (no recipient key).
4. ed25519-dalek 3.0.0 (edition 2024, MSRV 1.85) deferred — workspace stays
   edition 2021/MSRV 1.60-compatible; revisit on workspace-wide edition bump.
5. HPKE (RFC 9180) and XChaCha20-Poly1305 (192-bit nonce) are v2 candidates
   (RES-0015 §Gaps, ADR-0006 Option B note) — wire-format changes, deferred.
6. Android Keystore/StrongBox master-key provisioning + attestation → IDENT-001.

## References

- RES-0015 (research record: RUSTSEC audit table, D1–D5, KAT line-up)
- RFC 7748 (§5.2, §6.1) · RFC 8439 (§2.8.2) · RFC 8032 (§7.1) · RFC 5869 (App A)
- ADR-0002 (Ed25519 identity) · ADR-0006 (ChaCha20-Poly1305, corrected) ·
  ADR-0011 (signature scope)
- MESSAGE_ENVELOPE.md (encryption_hdr field 16; P0 broadcast field 18)
- DEC-P0001..0004 (DECISIONS.md) · DISC-0010 (libcrux ≠ RustCrypto) ·
  FAIL-0003/DISC-0002 (HKDF-BLAKE3 rejected) · DEC-0009 (gate authorization)
