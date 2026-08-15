//! ChaCha20-Poly1305 AEAD — CRYPTO-001.
//!
//! RFC 8439 via the RustCrypto `chacha20poly1305` crate (0.10.1, audited by
//! NCC Group / MobileCoin, no significant findings; constant-time on ARM).
//! Every call takes associated data (AAD) bound to the message identity —
//! see `protocol::codec::encode_for_aead`.
//!
//! Wire contract: 12-byte nonce (MESSAGE_ENVELOPE.md `encryption_hdr` field 2
//! is `bytes(12)`). Standard `ChaCha20Poly1305`, not XChaCha20 (192-bit nonce)
//! — that stays a v2 candidate (RES-0015 §Gaps). Nonce reuse is bounded
//! per-message by the fresh ephemeral X25519 keypair (D1/DEC-P0001).

use chacha20poly1305::aead::{Aead, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, KeyInit, Nonce};

use crate::crypto::CryptoError;

/// AEAD nonce length (RFC 8439 = 96 bits = 12 bytes).
pub const NONCE_LEN: usize = 12;

/// Poly1305 tag length (RFC 8439 = 128 bits = 16 bytes).
pub const TAG_LEN: usize = 16;

/// Length of a sealed blob for `plaintext_len` bytes: ciphertext is
/// length-preserving, so a sealed value is `plaintext_len + TAG_LEN`.
pub fn sealed_len(plaintext_len: usize) -> usize {
    plaintext_len + TAG_LEN
}

/// Encrypt `plaintext` with `key`, binding `aad` to the ciphertext.
///
/// Returns `ciphertext ‖ tag(16)`.
pub fn encrypt(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let out = cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload { msg: plaintext, aad },
        )
        .map_err(|_| CryptoError::Encrypt)?;
    Ok(out) // ciphertext ‖ tag
}

/// Decrypt `ciphertext ‖ tag` with `key`, requiring `aad` to match.
pub fn decrypt(
    key: &[u8; 32],
    nonce: &[u8; NONCE_LEN],
    sealed: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let out = cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload { msg: sealed, aad },
        )
        .map_err(|_| CryptoError::Decrypt)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    // RFC 8439 §2.8.2 AEAD test vector.
    const KEY: [u8; 32] =
        hex!("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    const NONCE: [u8; 12] = hex!("070000004041424344454647");
    const PLAINTEXT: &[u8] = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
    const AAD: &[u8] = &hex!("50515253c0c1c2c3c4c5c6c7");
    const CIPHERTEXT_AND_TAG: &[u8] = &hex!(
        "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b6116\
         1ae10b594f09e26a7e902ecbd0600691"
    );

    #[test]
    fn rfc8439_282_encrypt_matches_vector() {
        let sealed = encrypt(&KEY, &NONCE, PLAINTEXT, AAD).unwrap();
        assert_eq!(sealed, CIPHERTEXT_AND_TAG);
    }

    #[test]
    fn encrypt_decrypt_round_trip_with_aad() {
        let nonce = [9u8; NONCE_LEN];
        let sealed = encrypt(&KEY, &nonce, PLAINTEXT, AAD).unwrap();
        let open = decrypt(&KEY, &nonce, &sealed, AAD).unwrap();
        assert_eq!(open, PLAINTEXT);
    }

    #[test]
    fn wrong_aad_fails_decrypt() {
        let nonce = [9u8; NONCE_LEN];
        let sealed = encrypt(&KEY, &nonce, PLAINTEXT, AAD).unwrap();
        assert!(decrypt(&KEY, &nonce, &sealed, b"WRONG-AAD").is_err());
    }

    #[test]
    fn tampered_ciphertext_fails_tag() {
        let nonce = [9u8; NONCE_LEN];
        let mut sealed = encrypt(&KEY, &nonce, PLAINTEXT, AAD).unwrap();
        sealed[0] ^= 0x01;
        assert!(decrypt(&KEY, &nonce, &sealed, AAD).is_err());
    }

    #[test]
    fn nonce_uniqueness_produces_distinct_ciphertexts() {
        let a = encrypt(&KEY, &[1u8; NONCE_LEN], PLAINTEXT, AAD).unwrap();
        let b = encrypt(&KEY, &[2u8; NONCE_LEN], PLAINTEXT, AAD).unwrap();
        assert_ne!(a, b);
    }
}