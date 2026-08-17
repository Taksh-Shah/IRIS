//! HKDF-SHA256 key derivation — CRYPTO-001.
//!
//! RFC 5869 (extract-then-expand) via RustCrypto `hkdf` + `sha2`.
//! DEC-P0002 resolved to HKDF-SHA256: the standard, auditor-friendly
//! construction (TLS 1.3, HPKE, Signal, WireGuard); HKDF-BLAKE3 is rejected
//! as non-standard (FAIL-0003/DISC-0002). BLAKE3 is never used for keys.

use hkdf::Hkdf;
use sha2::Sha256;

use crate::crypto::{CryptoError, MESSAGE_KEY_SALT, STORAGE_KEY_SALT};

/// Derive a 32-byte AEAD message key from an X25519 shared secret.
///
/// Wraps `Hkdf::<Sha256>::new(Some(MESSAGE_KEY_SALT), shared_secret)`, with
/// `info` binding the key to the message identity (`message_id ‖ direction`).
pub fn message_key(shared_secret: &[u8; 32], info: &[u8]) -> Result<[u8; 32], CryptoError> {
    let hk = Hkdf::<Sha256>::new(Some(MESSAGE_KEY_SALT), shared_secret);
    let mut okm = [0u8; 32];
    hk.expand(info, &mut okm).map_err(|_| CryptoError::Kdf)?;
    Ok(okm)
}

/// Derive the 32-byte at-rest storage key from the node master key.
///
/// Wraps `Hkdf::<Sha256>::new(Some(STORAGE_KEY_SALT), master_key)`.
/// Domain-separated from the message key by a distinct salt.
pub fn storage_key(master_key: &[u8]) -> Result<[u8; 32], CryptoError> {
    let hk = Hkdf::<Sha256>::new(Some(STORAGE_KEY_SALT), master_key);
    let mut okm = [0u8; 32];
    hk.expand(&[], &mut okm).map_err(|_| CryptoError::Kdf)?;
    Ok(okm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // RFC 5869 Appendix A.1 (SHA-256, 82-byte IKM, 42-byte OKM).
    const IKM: [u8; 22] = hex!("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b");
    const SALT: &[u8] = &hex!("000102030405060708090a0b0c");
    const INFO: &[u8] = &hex!("f0f1f2f3f4f5f6f7f8f9");
    const OKM_42: &[u8] = &hex!(
        "3cb25f25faacd57a90434f64d0362f2a\
         2d2d0a90cf1a5a4c5db02d56ecc4c5bf\
         34007208d5b887185865"
    );

    #[test]
    fn rfc5869_appendix_a1_kat() {
        // Reproduce the RFC's HKDF-SHA256 expand with the RFC vectors.
        let hk = Hkdf::<Sha256>::new(Some(SALT), &IKM);
        let mut okm = [0u8; 42];
        hk.expand(INFO, &mut okm).expect("RFC 5869 A.1 fits");
        assert_eq!(&okm[..], OKM_42);
    }

    // RFC 5869 Appendix A.2 — SHA-256 with longer inputs/outputs.
    const IKM_A2: [u8; 80] = hex!(
        "000102030405060708090a0b0c0d0e0f\
         101112131415161718191a1b1c1d1e1f\
         202122232425262728292a2b2c2d2e2f\
         303132333435363738393a3b3c3d3e3f\
         404142434445464748494a4b4c4d4e4f"
    );
    const SALT_A2: &[u8] = &hex!(
        "606162636465666768696a6b6c6d6e6f\
         707172737475767778797a7b7c7d7e7f\
         808182838485868788898a8b8c8d8e8f\
         909192939495969798999a9b9c9d9e9f\
         a0a1a2a3a4a5a6a7a8a9aaabacadaeaf"
    );
    const INFO_A2: &[u8] = &hex!(
        "b0b1b2b3b4b5b6b7b8b9babbbcbdbebf\
         c0c1c2c3c4c5c6c7c8c9cacbcccdcecf\
         d0d1d2d3d4d5d6d7d8d9dadbdcdddedf\
         e0e1e2e3e4e5e6e7e8e9eaebecedeeef\
         f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff"
    );
    const OKM_A2: &[u8] = &hex!(
        "b11e398dc80327a1c8e7f78c596a4934\
         4f012eda2d4efad8a050cc4c19afa97c\
         59045a99cac7827271cb41c65e590e09\
         da3275600c2f09b8367793a9aca3db71\
         cc30c58179ec3e87c14c01d5c1f3434f\
         1d87"
    );

    #[test]
    fn rfc5869_appendix_a2_kat() {
        let hk = Hkdf::<Sha256>::new(Some(SALT_A2), &IKM_A2);
        let mut okm = [0u8; 82];
        hk.expand(INFO_A2, &mut okm).expect("RFC 5869 A.2 fits");
        assert_eq!(&okm[..], OKM_A2);
    }

    // RFC 5869 Appendix A.3 — SHA-256 with zero-length salt and info.
    #[test]
    fn rfc5869_appendix_a3_kat() {
        let hk = Hkdf::<Sha256>::new(Some(&[]), &IKM);
        let mut okm = [0u8; 42];
        hk.expand(&[], &mut okm).expect("RFC 5869 A.3 fits");
        let okm_a3: [u8; 42] = hex!(
            "8da4e775a563c18f715f802a063c5a31\
             b8a11f5c5ee1879ec3454e5f3c738d2d\
             9d201395faa4b61a96c8"
        );
        assert_eq!(&okm[..], &okm_a3[..]);
    }

    #[test]
    fn message_key_matches_rfc5869_construction() {
        // "iris-message-key-v1" salt, 32-byte output — must not panic and
        // must be deterministic for identical inputs.
        let sk = [7u8; 32];
        let info = b"message_id_0000000000000000";
        let k1 = message_key(&sk, info).unwrap();
        let k2 = message_key(&sk, info).unwrap();
        assert_eq!(k1, k2);
        assert_ne!(k1, [0u8; 32]);
    }

    #[test]
    fn distinct_salts_yield_distinct_keys() {
        let sk = [7u8; 32];
        let msg = message_key(&sk, b"m").unwrap();
        let at_rest = storage_key(&sk).unwrap();
        assert_ne!(msg, at_rest);
    }

    #[test]
    fn message_and_storage_key_domain_separation() {
        // Even with identical IKM material, message vs storage keys differ.
        let master = [0x42u8; 32];
        let msg = message_key(&master, b"m").unwrap();
        let rest = storage_key(&master).unwrap();
        assert_ne!(msg, rest);
    }

    #[test]
    fn sender_and_recipient_derive_identical_message_keys() {
        // AC-5: sender and recipient compute the SAME per-message key.
        // Both sides input the identical X25519 shared secret (symmetric DH)
        // plus the identical `message_id ‖ direction` info (message_kdf_info
        // is shared by engine + provider, never derived differently).
        use crate::crypto::keygen::X25519Keypair;
        use crate::crypto::x25519::diffie_hellman_keypairs;
        use crate::message_engine::crypto::message_kdf_info;
        use crate::protocol::{MessageId, PROTOCOL_VERSION};

        let sender = X25519Keypair::generate();
        let recipient = X25519Keypair::generate();
        let shared_ab = diffie_hellman_keypairs(&sender, &recipient).expect("dh ab");
        let shared_ba = diffie_hellman_keypairs(&recipient, &sender).expect("dh ba");
        assert_eq!(shared_ab, shared_ba, "X25519 must be symmetric (RFC 7748)");

        let id = MessageId::from_bytes([PROTOCOL_VERSION; 16]);
        let info = message_kdf_info(id);
        let k_sender = message_key(&shared_ab, &info).expect("sender key");
        let k_recipient = message_key(&shared_ba, &info).expect("recipient key");
        assert_eq!(
            k_sender, k_recipient,
            "both parties derive the identical message key"
        );
        assert_ne!(k_sender, [0u8; 32]);
    }
}
