//! X25519 Diffie-Hellman — CRYPTO-001.
//!
//! RFC 7748 with the mandatory hygiene from RES-0015 D5:
//! - validate the peer public key is exactly 32 bytes,
//! - reject an all-zero shared secret (§6.1) — an invalid/low-order peer key
//!   must error, never silently yield a weak secret (hpke-rs RUSTSEC-2026-0072
//!   lesson).

use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};

use crate::crypto::{CryptoError, keygen::X25519Keypair};

/// Compute `X25519(local_secret, peer_public)`.
///
/// Returns [`CryptoError::AllZeroSharedSecret`] if the shared secret is all
/// zero (RFC 7748 §6.1) and [`CryptoError::InvalidPublicKeyLen`] if `peer_public`
/// is not exactly 32 bytes. The shared secret is **never** exposed on error.
pub fn diffie_hellman(
    local_secret: &X25519StaticSecret,
    peer_public: &[u8; 32],
) -> Result<[u8; 32], CryptoError> {
    let peer = X25519PublicKey::from(*peer_public);
    let shared = local_secret.diffie_hellman(&peer);
    let bytes = shared.to_bytes();
    if bytes.iter().all(|&b| b == 0) {
        return Err(CryptoError::AllZeroSharedSecret);
    }
    Ok(bytes)
}

/// Compute the shared secret for a pair of keypairs (test/dev helper).
pub fn diffie_hellman_keypairs(a: &X25519Keypair, b: &X25519Keypair) -> Result<[u8; 32], CryptoError> {
    diffie_hellman(&a.secret, &b.public.to_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::X25519Keypair;
    use hex_literal::hex;

    // RFC 7748 §5.2 test vector (X25519): Alice's private key (scalar k) and
    // Bob's public key u-coordinate, expected shared secret u-coordinate.
    const ALICE_SCALAR: [u8; 32] =
        hex!("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
    const BOB_PUBLIC: [u8; 32] =
        hex!("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
    const EXPECTED_SHARED: [u8; 32] =
        hex!("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

    #[test]
    fn rfc7748_52_kat() {
        let alice = X25519StaticSecret::from(ALICE_SCALAR);
        let shared = diffie_hellman(&alice, &BOB_PUBLIC).expect("RFC 7748 §5.2 vector");
        assert_eq!(shared, EXPECTED_SHARED);
    }

    #[test]
    fn rfc7748_61_all_zero_shared_secret_rejected() {
        // A low-order (all-zero outcome) peer public key must be rejected, not
        // silently used. All-zero peer public key yields an all-zero secret.
        let alice = X25519Keypair::generate();
        let zero_peer = [0u8; 32];
        let err = diffie_hellman(&alice.secret, &zero_peer).unwrap_err();
        assert!(matches!(err, CryptoError::AllZeroSharedSecret));
    }

    #[test]
    fn valid_inputs_produce_matching_shared_secrets() {
        let a = X25519Keypair::generate();
        let b = X25519Keypair::generate();
        let ab = diffie_hellman_keypairs(&a, &b).unwrap();
        let ba = diffie_hellman_keypairs(&b, &a).unwrap();
        assert_eq!(ab, ba);
    }
}
