//! At-rest row sealing — CRYPTO-001 gate (BLK-0001, STORAGE.md §At-Rest).
//!
//! `envelope_cbor` is sealed with ChaCha20-Poly1305 under a key derived from
//! the node master key (`crypto::kdf::storage_key`, HKDF-SHA256 domain-separated
//! by `STORAGE_KEY_SALT`). Every sealed row carries a fresh random 12-byte
//! nonce (RFC 8439), so equal plaintext rows never collide.
//!
//! AUTHENTICATED METADATA — precise statement (TAK-14 supersedes the old
//! blanket claim): rows written since the v2 AAD authenticate message_id,
//! priority, expires_at, **status**, created_at, hop_count, max_hops,
//! payload_size and is_own_message via [`row_aad_v2`]; any out-of-band edit to
//! those columns fails AEAD verification on read. Rows sealed under the v1 AAD
//! (`row_aad`: id‖priority‖expires_at only) remain readable through an
//! explicit fallback in the read path; they are re-sealed under v2 by
//! `update_status`'s read-modify-reseal (optimistic CAS on status) or the
//! `migrate_envelope_format` pass. Wire format of a sealed blob:
//! `nonce(12) ‖ ciphertext ‖ tag(16)`. Sealing is opt-in: `PgStorage` carries
//! an `Arc<dyn RowSealer>`, defaulting to [`NoSealer`].

use iris_core::crypto::aead::{self, NONCE_LEN, TAG_LEN};
use iris_core::crypto::{kdf, CryptoError};
use iris_core::message_engine::storage::StorageError;

use rand::rngs::OsRng;
use rand::RngCore;

/// AAD binding a sealed row to its message identity and lifecycle scalars.
pub fn row_aad(message_id_hex: &str, priority: u8, expires_at: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(40);
    out.extend_from_slice(message_id_hex.as_bytes());
    out.push(priority);
    out.extend_from_slice(&expires_at.to_be_bytes());
    out
}

/// V2 AAD (TAK-14): binds EVERY mutable scalar column — critically `status`,
/// the field that decides whether a row is still queued. Under the v1 AAD an
/// attacker with DB write access could run
/// `UPDATE messages SET status='DELIVERED'` on every pending row (including
/// P0/SOS) and the AEAD verified cleanly: the node believed it had nothing to
/// send. Framing: `0xA2` ‖ v1-triple ‖ status_len ‖ status ‖ created_at ‖
/// hop_count ‖ max_hops-tagged ‖ payload_size ‖ is_own_message.
#[allow(clippy::too_many_arguments)]
pub fn row_aad_v2(
    message_id_hex: &str,
    priority: u8,
    expires_at: u64,
    status: &str,
    created_at: i64,
    hop_count: i16,
    max_hops: Option<i16>,
    payload_size: i64,
    is_own_message: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(96);
    out.push(0xA2);
    out.extend_from_slice(row_aad(message_id_hex, priority, expires_at).as_slice());
    let status_bytes = status.as_bytes();
    out.push(status_bytes.len().min(u8::MAX as usize) as u8);
    out.extend_from_slice(&status_bytes[..status_bytes.len().min(u8::MAX as usize)]);
    out.extend_from_slice(&created_at.to_be_bytes());
    out.extend_from_slice(&hop_count.to_be_bytes());
    match max_hops {
        None => out.push(0),
        Some(h) => {
            out.push(1);
            out.extend_from_slice(&h.to_be_bytes());
        }
    }
    out.extend_from_slice(&payload_size.to_be_bytes());
    out.push(is_own_message as u8);
    out
}

/// Stored-envelope format tags (TAK-15). Every row written by the current
/// code carries a leading 1-byte tag so reads never have to GUESS whether the
/// blob is plaintext CBOR or a sealed container — the old equality heuristic
/// only worked for `NoSealer` and made every pre-existing plaintext row
/// unreadable (`DecryptionFailed`) the moment sealing was enabled.
pub mod envelope_format {
    /// Untagged plaintext canonical CBOR (legacy rows and `NoSealer` writes).
    pub const PLAINTEXT_CBOR: u8 = 0x00;
    /// Sealed container: nonce(12) ‖ ciphertext ‖ tag(16).
    pub const SEALED: u8 = 0x01;
}

/// Split a stored blob into its format tag and payload (TAK-15). Returns
/// `None` for blobs written before the format existed (untagged), which the
/// read path then resolves heuristically for backwards compatibility.
pub fn split_format_tag(blob: &[u8]) -> Option<(u8, &[u8])> {
    blob.split_first().map(|(tag, rest)| (*tag, rest))
}

/// Row-encryption strategy for the persistent store. The caller supplies the
/// AAD so seal/unseal work from whichever side has data (live envelope on
/// write, scalar columns + id on read).
pub trait RowSealer: Send + Sync + std::fmt::Debug {
    /// Transform the canonical CBOR envelope for storage.
    fn seal(&self, aad: &[u8], cbor: &[u8]) -> Result<Vec<u8>, StorageError>;
    /// Recover the canonical CBOR envelope from a stored blob.
    fn unseal(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, StorageError>;
    /// Whether this sealer stores rows verbatim (`NoSealer`). Drives which
    /// format tag new rows carry (TAK-15).
    fn is_identity(&self) -> bool;
}

/// Default: transparent pass-through (pre-gate behavior, tests).
#[derive(Debug, Default)]
pub struct NoSealer;

impl RowSealer for NoSealer {
    fn seal(&self, _aad: &[u8], cbor: &[u8]) -> Result<Vec<u8>, StorageError> {
        Ok(cbor.to_vec())
    }
    fn unseal(&self, _aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, StorageError> {
        Ok(blob.to_vec())
    }
    fn is_identity(&self) -> bool {
        true
    }
}

/// AEAD row sealer keyed from the node master key (CRYPTO-001).
///
/// `Debug` is hand-implemented to redact the key material (TAK-16): the
/// derived form printed all 32 bytes, leaking the at-rest key into any log
/// line, panic payload or tracing capture that formatted the sealer. The key
/// is zeroized when the sealer drops (TAK-17).
pub struct StorageKeySealer {
    key: [u8; 32],
}

impl std::fmt::Debug for StorageKeySealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageKeySealer")
            .field("key", &"[redacted]")
            .finish()
    }
}

/// TAK-17: zero the key material on drop, so the at-rest storage key does not
/// linger in freed heap memory recoverable from core dumps, swap or
/// hibernation images — consistent with how `iris-core` treats key material.
impl Drop for StorageKeySealer {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.key.zeroize();
    }
}

impl StorageKeySealer {
    /// Derive the at-rest key with `kdf::storage_key` (HKDF, domain-separated).
    pub fn from_master_key(master_key: &[u8]) -> Result<Self, CryptoError> {
        Ok(Self {
            key: kdf::storage_key(master_key)?,
        })
    }
}

impl RowSealer for StorageKeySealer {
    fn seal(&self, aad: &[u8], cbor: &[u8]) -> Result<Vec<u8>, StorageError> {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let sealed = aead::encrypt(&self.key, &nonce, cbor, aad)
            .map_err(|e| StorageError::Backend(format!("seal: {e}")))?;
        let mut blob = Vec::with_capacity(NONCE_LEN + sealed.len());
        blob.extend_from_slice(&nonce);
        blob.extend_from_slice(&sealed);
        Ok(blob)
    }

    fn unseal(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, StorageError> {
        if blob.len() < NONCE_LEN + TAG_LEN {
            return Err(StorageError::DecryptionFailed);
        }
        let (nonce, sealed) = blob.split_at(NONCE_LEN);
        let nonce: &[u8; NONCE_LEN] = nonce
            .try_into()
            .map_err(|_| StorageError::DecryptionFailed)?;
        aead::decrypt(&self.key, nonce, sealed, aad).map_err(|_e| StorageError::DecryptionFailed)
    }

    fn is_identity(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris_core::crypto::{kdf, CryptoError};
    use iris_core::message::MessagePriority;
    use iris_core::protocol::{codec, Envelope, MessageId};

    fn sample_envelope() -> Envelope {
        let mut e = Envelope {
            version: 1,
            message_id: MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![2u8; 32],
            priority: MessagePriority::P4,
            ttl_seconds: 60,
            timestamp: 1_752_000_000,
            hop_count: 0,
            max_hops: None,
            payload_type: iris_core::protocol::ContentType::Text,
            payload_size: 11,
            payload_hash: [0; 32],
            payload: b"secret body".to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        e.payload_hash = Envelope::compute_payload_hash(&e.payload);
        e
    }

    fn master_key() -> [u8; 32] {
        [0x42u8; 32]
    }

    /// AAD exactly as `PgStorage` builds it on write.
    fn env_aad(e: &Envelope) -> Vec<u8> {
        row_aad(
            &e.message_id.to_string(),
            e.priority as u8,
            e.timestamp.saturating_add(e.ttl_seconds),
        )
    }

    #[test]
    fn storage_key_is_domain_separated_from_message_key() {
        let key = kdf::storage_key(&master_key()).expect("hkdf");
        // STORAGE_KEY_SALT differs from MESSAGE_KEY_SALT; sanity: non-zero and 32B.
        assert_eq!(key.len(), 32);
        assert_ne!(key, [0u8; 32]);
    }

    #[test]
    fn round_trip_preserves_cbor() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let aad = env_aad(&env);
        let blob = sealer.seal(&aad, &cbor).expect("seal");
        // Blob is nonce(12) + ciphertext(cbor_len) + tag(16).
        assert_eq!(blob.len(), NONCE_LEN + cbor.len() + TAG_LEN);
        assert_ne!(blob, cbor, "ciphertext must not equal plaintext");
        let back = sealer.unseal(&aad, &blob).expect("unseal");
        assert_eq!(back, cbor);
    }

    #[test]
    fn equal_rows_never_collide() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let aad = env_aad(&env);
        let a = sealer.seal(&aad, &cbor).expect("seal a");
        let b = sealer.seal(&aad, &cbor).expect("seal b");
        assert_ne!(a, b, "fresh nonce per row required (RFC 8439)");
    }

    #[test]
    fn aad_tamper_is_detected() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let aad = env_aad(&env);
        let blob = sealer.seal(&aad, &cbor).expect("seal");

        // Row swapped for a different message id.
        let other_id = row_aad(
            &MessageId::new_v7().to_string(),
            env.priority as u8,
            env.timestamp + 60,
        );
        assert!(
            sealer.unseal(&other_id, &blob).is_err(),
            "aad binds message_id"
        );

        // Lifecycle scalar rewritten out-of-band (priority in DB column).
        let tampered = row_aad(
            &env.message_id.to_string(),
            MessagePriority::P0 as u8,
            env.timestamp + 60,
        );
        assert!(
            sealer.unseal(&tampered, &blob).is_err(),
            "aad binds priority"
        );

        // Expiry rewritten out-of-band.
        let expiry_tampered = row_aad(
            &env.message_id.to_string(),
            env.priority as u8,
            env.timestamp + 9999,
        );
        assert!(
            sealer.unseal(&expiry_tampered, &blob).is_err(),
            "aad binds expires_at"
        );

        // Wrong key.
        let wrong = StorageKeySealer::from_master_key(&[0u8; 32]).expect("sealer");
        assert!(wrong.unseal(&aad, &blob).is_err(), "wrong key must fail");

        // Short blob.
        assert!(sealer.unseal(&aad, &[]).is_err());
        assert!(sealer.unseal(&aad, &blob[..NONCE_LEN]).is_err());
    }

    #[test]
    fn plaintext_bit_flip_is_detected() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let aad = env_aad(&env);
        let mut blob = sealer.seal(&aad, &cbor).expect("seal");
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert!(
            sealer.unseal(&aad, &blob).is_err(),
            "poly1305 catches bit flips"
        );
    }

    #[test]
    fn no_sealer_is_transparent() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = NoSealer;
        assert_eq!(sealer.seal(&[], &cbor).unwrap(), cbor);
        assert_eq!(sealer.unseal(&[], &cbor).unwrap(), cbor);
    }

    #[test]
    fn used_only_with_variables() {
        // The compiler/lint guard: from_master_key surfaces CryptoError.
        let _proof: Result<StorageKeySealer, CryptoError> =
            StorageKeySealer::from_master_key(&master_key());
    }

    /// TAK-16: the derived Debug printed all 32 key bytes. The hand-written
    /// redacting impl must never leak a single byte of key material.
    #[test]
    fn debug_output_never_contains_the_key() {
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let rendered = format!("{sealer:?}");
        assert!(rendered.contains("[redacted]"), "rendered: {rendered}");
        // 0x42 is the whole master key; none of it may appear in the output.
        assert!(
            !rendered.contains("66"),
            "no key bytes in debug: {rendered}"
        );
        assert_eq!(
            rendered.matches('4').count(),
            0,
            "hex-free debug: {rendered}"
        );
    }

    /// TAK-16 (config side, exercised here since pg.rs owns the type):
    /// PgStorageConfig::debug must not contain the password.
    #[test]
    fn config_debug_redacts_password() {
        use crate::pg::PgStorageConfig;
        let cfg = PgStorageConfig {
            host: "db.internal".into(),
            port: 5432,
            dbname: "iris".into(),
            user: "postgres".into(),
            password: "S3cr3t-Hunter2".into(),
            max_storage_bytes: 1024,
            eviction_threshold: 0.8,
            node_id: None,
        };
        let rendered = format!("{cfg:?}");
        assert!(rendered.contains("[redacted]"), "rendered: {rendered}");
        assert!(!rendered.contains("Hunter2"), "password leaked: {rendered}");
        assert!(
            rendered.contains("db.internal"),
            "non-secret fields stay useful"
        );
    }

    /// TAK-15: the read dispatcher must handle all three stored shapes —
    /// tagged plaintext, tagged sealed, and UNTAGGED legacy plaintext. The
    /// last one is the headline: the old equality heuristic made every
    /// pre-existing plaintext row unreadable the moment a real sealer was
    /// configured, because its `?` returned before the fallback could run.
    #[test]
    fn decode_dispatch_handles_tagged_and_legacy_shapes() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let aad = env_aad(&env);
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");

        // Tagged sealed round trip.
        let mut tagged_sealed = vec![envelope_format::SEALED];
        tagged_sealed.extend(sealer.seal(&aad, &cbor).expect("seal"));
        assert_eq!(
            crate::pg::decode_stored_blob_with(&sealer, std::slice::from_ref(&aad), &tagged_sealed)
                .expect("decode"),
            cbor
        );

        // Tagged plaintext.
        let mut tagged_plain = vec![envelope_format::PLAINTEXT_CBOR];
        tagged_plain.extend_from_slice(&cbor);
        assert_eq!(
            crate::pg::decode_stored_blob_with(&sealer, std::slice::from_ref(&aad), &tagged_plain)
                .expect("decode"),
            cbor
        );

        // Untagged LEGACY plaintext row with a real sealer configured: must
        // still decode (this is exactly the upgrade scenario that used to
        // strand the whole store).
        assert_eq!(
            crate::pg::decode_stored_blob_with(&sealer, std::slice::from_ref(&aad), &cbor)
                .expect("legacy decode"),
            cbor
        );

        // Untagged legacy SEALED blob (old sealing-era rows) also resolves.
        let legacy_sealed = sealer.seal(&aad, &cbor).expect("seal legacy");
        assert_eq!(
            crate::pg::decode_stored_blob_with(&sealer, std::slice::from_ref(&aad), &legacy_sealed)
                .expect("decode"),
            cbor
        );
    }

    /// TAK-15 helpers: tag splitting is exact and rejects empty input by
    /// returning no payload rather than panicking.
    #[test]
    fn split_format_tag_basics() {
        assert_eq!(split_format_tag(b"\x00hello"), Some((0x00, &b"hello"[..])));
        assert_eq!(split_format_tag(&[0x01]), Some((0x01, &[][..])));
        assert_eq!(split_format_tag(&[]), None);
    }

    /// TAK-14: the v2 AAD authenticates status. Sealing under one status and
    /// reading under another (the exact `SET status='DELIVERED'` at-rest
    /// attack that motivated the finding) must fail verification; matching
    /// scalars verify cleanly.
    #[test]
    fn v2_aad_detects_status_tamper() {
        let env = sample_envelope();
        let cbor = codec::encode(&env).expect("encode");
        let sealer = StorageKeySealer::from_master_key(&master_key()).expect("sealer");
        let base = row_aad_v2(
            &env.message_id.to_string(),
            env.priority as u8,
            env.timestamp + env.ttl_seconds,
            "PENDING_SEND",
            1_752_000_000,
            0,
            None,
            cbor.len() as i64,
            false,
        );
        let blob = sealer.seal(&base, &cbor).expect("seal");
        assert!(
            sealer.unseal(&base, &blob).is_ok(),
            "matching scalars verify"
        );
        let tampered = row_aad_v2(
            &env.message_id.to_string(),
            env.priority as u8,
            env.timestamp + env.ttl_seconds,
            "DELIVERED",
            1_752_000_000,
            0,
            None,
            cbor.len() as i64,
            false,
        );
        assert!(
            sealer.unseal(&tampered, &blob).is_err(),
            "status rewrite must fail AEAD verification"
        );
    }
}
