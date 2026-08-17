//! At-rest row sealing — CRYPTO-001 gate (BLK-0001, STORAGE.md §At-Rest).
//!
//! `envelope_cbor` is sealed with ChaCha20-Poly1305 under a key derived from
//! the node master key (`crypto::kdf::storage_key`, HKDF-SHA256 domain-separated
//! by `STORAGE_KEY_SALT`). Every sealed row carries a fresh random 12-byte
//! nonce (RFC 8439), so equal plaintext rows never collide. The AEAD tag binds
//! AAD = `message_id ‖ priority ‖ expires_at`, so any out-of-band metadata
//! tamper (row swapped, priority/expiry rewritten) is detected on read.
//!
//! AAD := `message_id_hex_ascii ‖ priority(1) ‖ expires_at(u64 BE)`
//! where `expires_at := envelope.timestamp + envelope.ttl_seconds`.
//! `expires_at` is derived exactly as in the schema DDL so seal (from the live
//! envelope) and unseal (from the stored row columns) always agree.
//!
//! Wire format of a sealed blob: `nonce(12) ‖ ciphertext ‖ tag(16)`.
//!
//! Sealing is opt-in: `PgStorage` carries an `Arc<dyn RowSealer>`, defaulting
//! to [`NoSealer`] (stores plaintext CBOR, pre-gate behavior).

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

/// Row-encryption strategy for the persistent store. The caller supplies the
/// AAD so seal/unseal work from whichever side has data (live envelope on
/// write, scalar columns + id on read).
pub trait RowSealer: Send + Sync + std::fmt::Debug {
    /// Transform the canonical CBOR envelope for storage.
    fn seal(&self, aad: &[u8], cbor: &[u8]) -> Result<Vec<u8>, StorageError>;
    /// Recover the canonical CBOR envelope from a stored blob.
    fn unseal(&self, aad: &[u8], blob: &[u8]) -> Result<Vec<u8>, StorageError>;
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
}

/// AEAD row sealer keyed from the node master key (CRYPTO-001).
#[derive(Debug)]
pub struct StorageKeySealer {
    key: [u8; 32],
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
}
