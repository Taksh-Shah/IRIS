# IDENT-001 Design — Identity, Key Management, Trust (v1)

**Document ID**: IRIS-IDENT-DESIGN-001
**Version**: 1.0
**Node**: IDENT-001 (P0 SECURITY, deps CRYPTO-001 COMPLETE)
**Date**: 2026-08-15
**Research basis**: RES-0016 (2026 SOTA: identity model, key provisioning, trust,
rotation, key binding, RustSec posture); DEC-P0005..P0009 (ratified in
DECISIONS.md); RFC 9804 (SPKI), RFC 9171/9172 (BPv7/BPSec), RFC 7748 (X25519),
RFC 8032 (Ed25519), ADR-0002; CVE-2025-53627 (Meshtastic NodeNum — never
MAC-derived identity); CRYPTO-001 SECURITY_REVIEW RED-0001/0005/0008/0011.
**Absorbs**: RED-0001 (desktop real-provider wiring + persisted identity + key
dir + sealer), RED-0005 (Ed25519-signed-X25519 key binding), RED-0008
(`auth_cert_chain` key-anchored validation), RED-0011 (small-order X25519
rejection).
**Supersedes/extends**: `crates/iris-core/src/message_engine/crypto.rs`
`NodeIdentity` in-memory-only provisioning; `engine_handle.rs:145`
`DevCryptoProvider` in the desktop production path; `ADDRESSING.md` BLAKE3
address derivation (resolved to key-derived in §3).

---

## 1. Scope

Implement the identity system behind the wire + engine seams with **zero engine
breaking changes**, matching how CRYPTO-001 swapped `DevCryptoProvider`:

1. **Per-device Ed25519 identity keypair**, provisioned once and persisted
   (decides and records the provisioning mechanism per platform).
2. **Address/PeerId derivation from the public key** (DEC-P0005) — never from
   MAC/hardware (CVE-2025-53627).
3. **Key store seam** (`KeyDirectory` extension): sealed X25519 static key +
   bound to identity by signature (RED-0005 / SPKI-style, RFC 9804).
4. **Trust store** (TOFU baseline + verified/pairing tier) and **key
   advertisement** transport vehicle (DEC-P0007).
5. **`auth_cert_chain` (field 18) validation** as key-anchored chain — **no
   PKIX/X.509** (RED-0008, RUSTSEC pressure: rustls-webpki family).
6. **Rotation/revocation** via signed rotation events over the existing
   `ContentType::KeyRotation` (16) path — monotonic counter + valid-until +
   earliest-seen-wins + null-rotation (DEC-P0008).
7. **RED-0011**: small-order X25519 public-key rejection integrated into
   advertisement/chain validation.
8. **Desktop wiring (RED-0001)**: real `IrisCryptoProvider`, persisted
   identity, key directory, `StorageKeySealer` default-on — satisfies the
   DEC-0010 security control (no production deployment with DevCryptoProvider /
   no identity / no sealer).

**Out of scope (v1)**: wire `sender_id` collapse to 16 bytes (RED-0010 — tracked
to PROTO-001 v2 §9.4); full KERI/DID-style event log with equivocation
detection (v2, BLK-0002); XEdDSA/single-key curve signing (DEC-P0009: two
keypairs, no curve conversion); Android Keystore/iOS adapters (ANDROID-001 /
IOS-001); OS-keychain crate integration beyond the protected-key-file design
(decided here, §5.3); QR/pairing UI implementation beyond the trust-store
`TrustLevel` hook (ANDROID/IOS/desktop UX nodes).

## 2. Design decisions (RES-0016 D1–D5, DEC-P0005..P0009, ratified)

1. **D1 (DEC-P0005) — key-derived PeerId, stable UID separate.**
   - `PeerId` (32 B, existing opaque identity in envelope field 3) = the
     **Ed25519 public key** itself (self-authenticating: the verifier derives
     the `VerifyingKey` directly from the wire — CRYPTO-001 verified path
     unchanged, `message_engine/crypto.rs:236-246`).
   - **Key-derived wire short id** (`sender_id` 16 B, for P0 abbreviated
     envelopes / display) = `SHA-256(Ed25519 pubkey)[..16]` — implemented and
     table-tested now (§9.2); the wire collapse of standard envelopes to 16 B
     is **deferred to PROTO-001 v2 / RED-0010** because it requires
     key-directory-based pubkey resolution for verification (breaks the
     self-authenticating relay path).  Full hash = internal PeerId keying.
   - Node UID (e.g. `IRIS_NODE_ID`) = **separate, non-authoritative
     convenience label**; never used for authentication (DESKTOP-001
     `default_node_id` moves to this role).
   - **No MAC/serial/hardware-derived identity** (CVE-2025-53627, CVSS 8.2).
   - did:key-compatible multicodec 0xed derivation preserved as a documented
     derived view, not a v1 requirement.
2. **D2 (DEC-P0006) — per-platform provisioning; master in the strongest
   available store; wrapped-DEK pattern; `StorageKeySealer` default-on.**
   - Android (later, ANDROID-001): Ed25519 identity + master storage key in
     Android Keystore (TEE default, StrongBox-optional for master) — ChaCha
     payload ops stay in-process (StrongBox keygen ≈9.2 s — KeyDroid
     arXiv:2507.07927).
   - iOS (later, IOS-001): identity/X25519 in Keychain
     (`WhenUnlockedThisDeviceOnly`); Secure Enclave is P-256-only — never
     claims Ed25519 there.
   - **Desktop (v1 deliverable, RED-0001)**: **protected key file** (v1 default,
     §5.3) — 0600, atomic-write, advisory lock, zeroize-on-drop buffers; OS
     keychain crate (`keyring`) documented as the RUSTSEC-clean hardening step
     for a later pass. Decision recorded: v1 uses the protected file because it
     is deterministic, testable on all dev platforms, and satisfies
     DEC-P0006's "protected key file" branch; OS-keychain behind a
     `KeyStore` impl remains a drop-in.
   - Pi/relay (later): external TPM (SLB9670/9672) sealing only — documented
     weaker posture; Stronghold optional software vault, never a root of trust.
   - Storage key = `StorageKeySealer::from_master_key(master)` where master =
     32 random bytes stored in the keystore alongside identity; at-rest row
     sealing becomes **default-on** in writing desktop (`PgStorage::with_sealer`).
3. **D3 (DEC-P0007) — trust = TOFU baseline + signed key advertisement
   (RED-0005) + verified/pairing tier.**
   - First-seen key binds by TOFU; **key change → user warning** (Meshtastic
     pattern); earliest-seen-wins on conflict.
   - Higher assurance via QR/pairing sets `TrustLevel::Verified`; UI hook
     distinguishes verified vs unverified (Briar BQP 3-vs-2-bar lesson).
   - RED-0005 advertisement = **SPKI-style (RFC 9804) keyholder certificate**:
     Ed25519 identity signs its X25519 static public key (raw 32-byte form).
   - RED-0008 `auth_cert_chain` = key-anchored chain of authorization, NOT a CA
     hierarchy, NOT PKIX/X.509 (rustls-webpki family excluded).
   - Routing reputation (ROUTE-002/SEC-001 PRoPHET trust) is explicitly NOT an
     identity-trust input.
4. **D4 (DEC-P0008) — rotation/revocation via signed rotation events.**
   - Reuse envelope `ContentType::KeyRotation` (16): rotation payload carries
     new X25519 static pubkey + **key-generation counter** (monotonic) +
     **valid-until** timestamp + signature by the identity keypair.
   - `earliest-seen-wins` on conflicting rotation/replay (Ceramic lesson);
     offline convergence via the gossip/SCF path.
   - **Revocation = null-rotation** (KERI semantics): signed rotation moving to
     a placeholder "revoked" key, propagated as a signed advertisement.
   - v1 does not build a full signed event log; `encryption_hdr.key_id` (optional)
     becomes the routing key for multi-key state (§9.6).
5. **D5 (DEC-P0009) — two distinct keypairs, Ed25519 signs X25519.**
   - Identity Ed25519 (signing, verify_strict-only) + static X25519
     (encryption) are **separate keypairs**, bound by the signed advertisement
     (RED-0005). NO XEdDSA / curve conversion in v1 (LSEG arXiv:2511.07548,
     IACR 2021/509 caveat; Signal X3DH + Meshtastic XEdDSA = precedent for the
     pattern, not for single-key).
   - RED-0011 small-order rejection rides along as defense-in-depth.
6. **D6 — crates / security posture (no new crypto crates).**
   - No new cryptographic primitives. Optional desktop key-store crate
     (`keyring`) NOT added in v1 (file store instead) — avoids OS-keychain
     test flakiness; revisit in OS-integration pass. NO PKIX family
     (rustls-webpki RUSTSEC-2026-0104/0098/0099, pemfile 2025-0134). Watch
     core2 2026-0105 / thin-vec 2026-0103 if they enter the tree. zeroize (in
     tree) for key material. All signing/verifying reuses `crypto::ed25519`
     (`verify_strict` only) and `crypto::x25519` (RFC 7748 §6.1 all-zero reject).

## 3. Wire contract — identity fields (reconciled)

The flat envelope (MESSAGE_ENVELOPE.md) fields touched by identity:

| Field | Use | v1 (this design) |
|-------|-----|-------------------|
| 3 `sender_id` | Sender identity | **32 B Ed25519 public key** (self-authenticating; verifier reconstructs `VerifyingKey::from_bytes` — CRYPTO-001 `verify()`). P0 abbreviated envelope keeps **16 B = SHA-256(pubkey)[..16]**, authority-verifiable via `auth_cert_chain` (field 18). |
| 4 `recipient_id` | Recipient address | Unchanged (PeerId or broadcast constant). |
| 16 `encryption_hdr.key_id` | Key selector | Generic node = None; after rotation, carries the key-generation counter so the recipient selects the right decrypt key. |
| 18 `auth_cert_chain` | Authorization chain | Key-anchored SPKI-style chain, present for P0/authority traffic + carried by signed advertisements; validated in §9.5. NOT PKIX. |

**ADDRESSING.md correction**: the legacy `node_id = BLAKE3(public_key)` derivation
(ADDRESSING.md §I) is superseded — PeerId = Ed25519 pubkey, short id =
SHA-256(pubkey)[..16]. Patch ADDRESSING.md in the same pass.

## 4. Module layout

```
crates/iris-core/src/identity/
  mod.rs          — IdentityManager facade: provision/load, PeerId, key dir
  provision.rs    — on-first-run generation, versioned serialization, store access
  store.rs        — KeyStore trait + FileKeyStore (0600, atomic, lock, zeroize)
  peer_id.rs      — PeerId = Ed25519 pubkey; peer_short() = SHA-256(pubkey)[..16]
  advertise.rs    — RED-0005 signed key advertisement (SPKI-style), build/verify
  trust_store.rs  — TOFU table, TrustLevel (Unverified/Verified), key-change warn,
                    earliest-seen-wins, revocation markers
  chain.rs        — RED-0008 auth_cert_chain validation (key-anchored, no PKIX)
  rotate.rs       — signed rotation events (KeyRotation payload, counter,
                    valid-until, null-rotation)
  small_order.rs  — RED-0011 X25519 small-order pubkey rejection

crates/iris-core/src/crypto/key_directory.rs  — KeyDirectory trait extended:
  X25519 pubkeys returned only after advertisement signature verifies.

crates/iris-desktop/src/
  identity.rs     — RED-0001 wiring: provision-at-start, FileKeyStore, master key,
                    PgStorage::with_sealer, real IrisCryptoProvider
  engine_handle.rs — replace DevCryptoProvider with IrisCryptoProvider(node id
                    from identity); IRIS_NODE_ID → non-authoritative uid label
```

## 5. Key provisioning (D2) — detail

### 5.1 Identity material

`NodeIdentity` (extends CRYPTO-001's in-memory bundle):

```
struct NodeIdentityV1 {            // v1 serialization, version byte prefix
    format_version: u8 = 1,
    created_unix: u64,
    identity_ed25519_seed: [u8; 32],   // Ed25519 signing key (RFC 8032 seed)
    static_x25519_secret: [u8; 32],    // static X25519 encryption key
    key_gen_counter: u64 = 0,          // monotonic rotation counter (D4)
}
```

- `identity_ed25519_seed` + `static_x25519_secret` stored as 32-byte seeds
  (not expanded private/secret scalar) → deterministic re-derivation via the
  existing `keygen`/`x25519` providers. Both are `zeroize::ZeroizeOnDrop`.
- **Note**: v1 stores identity seed + static X25519 secret in the *same* store,
  per DEC-P0006's wrapped-DEK model (master wraps/co-provisions both). Android /
  iOS may later split (identity in Keystore, static key wrapped by master).

### 5.2 Provisioning flow (first run)

1. Verify key store empty (no `identity.v1` blob) — else load (§5.4).
2. `OsRng` → 32-byte master key (storage KEK) + derive both seeds (`OsRng`).
3. Persist `NodeIdentityV1` through the `KeyStore`; persist master key
   adjacent (same keystore entry or sibling file) — both 0600.
4. Derive `PeerId` = identity `VerifyingKey::to_bytes()`.
5. Wire `StorageKeySealer::from_master_key(master)` into writing storage
   (`PgStorage::with_sealer`), passes `StorageKeySealer` to `MessageEngine`
   storage where applicable (row seal default-on).
6. First-run session logs a provisioning event (OBS-001 seam), never key bytes.

### 5.3 Desktop `KeyStore` (v1) — protected key file

- Location: platform app-data dir (`%APPDATA%/iris/` on Windows,
  `~/.local/share/iris/` on Linux, `~/Library/Application Support/iris/` on
  macOS) in `keys/identity.v1` + `keys/master.key`.
- **0600 / user-only ACL**; atomic write (temp + rename) to avoid torn reads;
  advisory `FileLock` for cross-process safety; buffers zeroized on drop;
  file header = version byte + reserved bytes (future re-key/derivation).
- Documented as the weaker desktop posture (per DEC-P0006); OS keychain
  (`keyring`) is the recorded hardening follow-up (no RUSTSEC advisories as of
  2026-08-15 snapshot), kept behind the `KeyStore` trait so the swap is
  drop-in.

### 5.4 Loading semantics

- Corrupt/mismatched-version blob → **fail loudly with identity-error**, never
  silently regenerate (regeneration would change PeerId and orphan storage).
  Recovery path documented: operator re-provisions (accepting the new
  identity + re-pairing).
- `IRIS_NODE_ID` env now maps to the **UID label** only (display); PeerId comes
  exclusively from the persisted identity.

## 6. PeerId derivation (D1)

- `peer_id()` → `VerifyingKey::to_bytes()` (32 B). Existing engine routes on
  this byte string unchanged.
- `peer_short()` → `SHA-256(pubkey)[..16]`. Table-tested:
  - deterministic across calls and processes;
  - distinct seed keys → distinct short ids;
  - matches the hex in P0 abbreviated envelopes.
- `human_uid()` → `base32(SHA-256(pubkey)[..10])` uppercase, per ADDRESSING.md
  human-address convention (display only).
- `IRIS_NODE_ID` env override remains usable as the UID convenience label.

## 7. Key advertisement (RED-0005, SPKI-style)

Signed by the **identity Ed25519 key**, covering the **static X25519 public
key** — this is the "keyholder certifies the key" primitive (RFC 9804): the
signature key (identity) is the subject of record; the certified key (X25519
static) is what peers encrypt to.

```
struct KeyAdvertisementV1 {
    format_version: u8 = 1,
    identity_pubkey: [u8; 32],        // Ed25519 verifying key (subject/keyholder)
    static_x25519_pubkey: [u8; 32],   // certified encryption key
    key_gen_counter: u64,             // D4 monotonic
    valid_until: u64,                 // Unix expiry (0 = no expiry)
    sig: [u8; 64],                    // Ed25519(identity) over the above fields
}
```

- **Transport**: carried as an application-level blob inside a normal
  `ContentType::KeyRotation` envelope (16) and/or piggybacked in the
  `auth_cert_chain` for authority traffic — same bytes, wire unchanged.
- **Verification** (`trust_store.adopt_advertisement`):
  1. Small-order check on `static_x25519_pubkey` (§8).
  2. `verify_strict(identity_pubkey, signable, sig)`. (RFC 8032)
  3. With existing entry: key equal → no-op; key differs → **earliest-seen-wins
     on counter conflict**, else flag **key-change warning** + require re-verify
     (unless counter strictly increases per §9.6 rotation, then adopt).
- On success: `KeyDirectory` stores `identity_pubkey → static_x25519_pubkey`
  (encrypt target). Key material never logged (OBS-001).

## 8. RED-0011 small-order X25519 rejection

- Reject remote X25519 public keys in the small-order subgroup (the 4 classic
  low-order points + 0 / identity elements) before any DH with them. Applies at
  the advertisement-adoption boundary and inside chain validation (§9.5).
- Complements the RFC 7748 §6.1 all-zero shared-secret reject already in
  `crypto::x25519`; RED-0011 catches the point substitution before a harmful DH
  is attempted.
- Reference: RFC 7748 §6; curve25519-dalek `CompressedMontgomeryU` membership
  check; explicit test vectors for the known small-order points.

## 9. RED-0008 `auth_cert_chain` validation (key-anchored, no PKIX)

`auth_cert_chain` (field 18) = ordered array of advertisements (§7) forming a
chain of authorization from a **known, trusted identity root** (TOFU/verified)
to the message signer. No X.509, no `rustls-webpki` — only `ed25519::verify_strict`.

Algorithm (`chain::verify_chain`):

```
1. chain = decode(array of KeyAdvertisementV1)         # structural errors → reject
2. root  = first element;   must already be trusted     # in TrustStore
3. for each consecutive (a, b):
     verify_strict(a.identity_pubkey, b.identity_pubkey, b.sig) or reject
     small_order(b.static_x25519_pubkey) → reject        # RED-0011
     b.key_gen_counter <= root.key_gen_counter + allowed_increment_tolerance
                         (or monotonic policy per §9.6)
4. final.identity_pubkey must equal envelope.sender_id   # anchors the message
5. all valid_until == 0 or valid_until >= now           # no expired links
6. OK → mark final identity trusted (TOFU) and register its KeyDirectory entry
```

Rules:
- Chain must **terminate at a trusted root** — pure ad-hoc self-signed chains
  with no trusted anchor are rejected (the whole point vs PKI).
- Cap chain length (e.g. ≤ 8) to bound work (adversarial deep-chain DoS).
- Revoked/force-rotated identities (null-rotation marker, §9.6) are rejected
  even inside an otherwise-valid chain.

## 9.6 Rotation & revocation (D4) — detail

Rotation/revocation payload rides `ContentType::KeyRotation` (envelope type 16):

```
struct RotationEventV1 {
    format_version: u8 = 1,
    rotation_type: u8,            // 0=Rotation, 1=Revocation (null-rotation)
    new_static_x25519_pubkey: [u8; 32],   // placeholder [0u8;32] for revocation
    key_gen_counter: u64,                 // strictly greater than current
    valid_until: u64,
    sig_identity: [u8; 64],               // signed by CURRENT identity
}
```

Receiving node policy (`rotate::apply`):

1. Verify `sig_identity` under current trusted identity (earliest-seen + TOFU).
2. **Rotation**: counter must be > stored counter (monotonic). Adopt new static
   key, bump counter, `encryption_hdr.key_id = counter` for future TX; older
   envelopes still decrypt until `valid_until`.
3. **Revocation (null-rotation)**: mark identity revoked; KeyDirectory entry
   removed; incoming messages from that identity rejected (incl. inside chains).
   Propagate the revocation advertisement via the existing gossip/SCF seam so
   offline nodes converge.
4. Conflict (two different rotations, same counter): **earliest-seen-wins**,
   log warning, drop the loser.
5. Full KERI-style signed event log with equivocation detection = v2 (BLK-0002).

## 10. Trust store semantics (D3, TOFU + verified tier)

| State | Meaning | Trigger | UI hook |
|-------|---------|---------|---------|
| `Unknown` | Never seen | — | — |
| `Unverified` | TOFU first-seen (ad verified, not out-of-band) | ad adoption | "unverified" (Briar-2-bar analog) |
| `Verified` | QR/pairing out-of-band exchange confirmed | `verify_peer()` API | "verified" (Briar-3-bar analog) |
| `KeyChanged` | same identity, different static key (< monotonic) | ad detect | warning banner |
| `Revoked` | null-rotation received | revocation ad | blocked |

- `TrustLevel` per PeerId surfaced via a read API for UI (desktop/Android/iOS).
- TOFU binding never auto-promotes; revocation never auto-heals.

## 11. Desktop wiring (RED-0001) — engine_handle.rs

Replaces the inert provider at `engine_handle.rs:145`:

```
1. provision_or_load_identity() -> NodeIdentity     // §5.2/5.4 (FileKeyStore)
2. Arc<NodeIdentity> -> IrisCryptoProvider            // real provider
3. MessageEngineConfig.node_id = PeerId (VerifyingKey bytes)
4. master key -> StorageKeySealer; where PgStorage is configured:
     PgStorage::with_sealer(StorageKeySealer::from_master_key(master))
     (row sealing default-on; loopback/MemoryStorage stays NoSealer)
5. KeyDirectory = TrustStoreAdapter (advertisement-fed)
```

- `IRIS_NODE_ID` downgraded to display UID (docs + code comment updated).
- Satisfies DEC-0010 security control: no DevCryptoProvider / no identity / no
  sealer in any production path after this node.

## 12. Acceptance criteria (AC-1..N)

- **AC-1**: `identity` module + `peer_id`/`peer_short` derivation compile, table
  tested (determinism, distinct-seed, no-MAC rule, P0 16-byte form matches);
  clippy `-D warnings` 0; workspace green.
- **AC-2**: provisioning round-trip: fresh store → `IdentityManager::provision`
  → restart → `load()` returns identical PeerId + re-derived keys; corrupt blob
  → loud error (no silent regen); `zeroize` on drop proven by test.
- **AC-3**: `KeyAdvertisement` build/verify: valid ad accepted; tampered/bad-sig
  ad rejected; cross-identity ad (signer ≠ subject) rejected.
- **AC-4**: `verify_chain`: trusted-root chain passes; untrusted-root chain
  rejected; broken link rejected; expired link rejected; revoked mid-chain
  rejected; small-order pubkey in chain rejected; length cap enforced.
- **AC-5**: TrustStore: TOFU bind on first adoption; repeated ad (same key)
  idempotent; key-change → `KeyChanged` + re-verify required; `verify_peer`
  promotes to `Verified`; revocation → `Revoked` and blocks all paths.
- **AC-6**: rotation: monotonic counter accepted (key_id set), stale counter
  rejected, equal-counter conflict → earliest-seen-wins; valid-until enforced
  for old keys; null-rotation revokes + propagates via SCF/gossip seam.
- **AC-7**: RED-0011: all known small-order X25519 pubkeys rejected at
  ad/chain boundary (test vectors), valid remote key accepted.
- **AC-8**: DESKTOP RED-0001: `DesktopEngine` runs with `IrisCryptoProvider`,
  persisted identity (PeerId stable across engines/restarts), FileKeyStore,
  `with_sealer` when PG configured; no DevCryptoProvider reachable in
  production path (compile/test guard).
- **AC-9**: engine E2E with identity: Alice identity → key dir → encrypt to
  Bob's advertised static key → verify_strict → decrypt; auth_cert_chain
  validated on authority/P0-style message.
- **AC-10**: redteam SECURITY_REVIEW: identity/key-mgmt/trust/rotation reviewed;
  no RUSTSEC regressions (NO PKIX family added; keyring NOT added in v1;
  core2/thin-vec absent); DEC-0010 security control verified satisfied.
- **AC-11**: VERIFY evidence: `IDENT_VERIFICATION.md` v1.0 with AC-1..11 PASS,
  workspace green count reported, clippy 0, records (RES-0016, DEC-P0010
  wire-reconciliation if recorded, CHANGELOG entry).

## 13. Test plan map (node stages)

- **IMPLEMENT**: unit tests per module (§4) + `identity` integration tests;
  `DESKTOP` engine round-trip extended with real provider.
- **TEST**: full AC-1..9 suite; P0 abbreviated-envelope short-id matching;
  rotation/replay/conflict cases; chain DoS cap.
- **SECURITY_REVIEW**: redteam on identity/trust/rotation/advert/chain +
  compatibility with CRYPTO-001 (verify self-auth from wire must survive).
- **VERIFY/ACCEPT**: `IDENT_VERIFICATION.md` v1.0 AC-1..11 PASS; workspace
  green; CHANGELOG; PROJECT_GRAPH COMPLETE + evidence + known_limitations.

## 14. Known limitations / open items (to record at ACCEPT)

- Physical-device key stores (Android Keystore, iOS Keychain, TPM) deferred to
  platform nodes; desktop uses the protected-file v1 store (OS keychain =
  followed-up hardening).
- Full KERI/DID event-log + equivocation detection = v2 (BLK-0002).
- 16-byte wire `sender_id` collapse deferred to PROTO-001 v2 / RED-0010.
- TOFU rests on first-contact authenticity; QR/pairing UI deferred to UX nodes
  (verified tier hook present).
- No formal proof of Ed25519-signed-X25519 in a disconnected mesh (Briar
  Tamarin is closest); formality = v2 hardening (RES-0016 gap 4).