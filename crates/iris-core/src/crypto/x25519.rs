//! X25519 Diffie-Hellman — CRYPTO-001.
//!
//! RFC 7748 with the mandatory hygiene from RES-0015 D5:
//! - validate the peer public key is exactly 32 bytes,
//! - reject an all-zero shared secret (§6.1) — an invalid/low-order peer key
//!   must error, never silently yield a weak secret (hpke-rs RUSTSEC-2026-0072
//!   lesson).

use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};

use crate::crypto::{keygen::X25519Keypair, CryptoError};

/// Compute `X25519(local_secret, peer_public)`.
///
/// Returns [`CryptoError::AllZeroSharedSecret`] if the shared secret is all
/// zero (RFC 7748 §6.1) and [`CryptoError::InvalidPublicKeyLen`] if `peer_public`
/// is not exactly 32 bytes. The shared secret is **never** exposed on error.
pub fn diffie_hellman(
    local_secret: &X25519StaticSecret,
    peer_public: &[u8; 32],
) -> Result<[u8; 32], CryptoError> {
    // PRY-5: RED-0011 rejection previously lived only at trust-boundary
    // adoption points (advertise/chain/rotate/trust_store). Any path that
    // reaches DH with a key that did not pass those — a `MemoryKeyDirectory`
    // contact, an ephemeral sender key from a message header — agreed against
    // an unchecked point, and the §6.1 all-zero output check does not catch a
    // *non-zero* low-order secret. Filter at the choke point so the guarantee
    // holds by construction regardless of the key's provenance.
    if crate::identity::small_order::is_small_order(peer_public) {
        return Err(CryptoError::SmallOrderPeerKey);
    }
    let peer = X25519PublicKey::from(*peer_public);
    let shared = local_secret.diffie_hellman(&peer);
    let bytes = shared.to_bytes();
    if bytes.iter().all(|&b| b == 0) {
        return Err(CryptoError::AllZeroSharedSecret);
    }
    Ok(bytes)
}

/// Compute the shared secret for a pair of keypairs (test/dev helper).
pub fn diffie_hellman_keypairs(
    a: &X25519Keypair,
    b: &X25519Keypair,
) -> Result<[u8; 32], CryptoError> {
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
        // An all-zero peer public key must be rejected, not silently used.
        // PRY-5: it is now caught as a small-order point (u = 0) before the DH,
        // ahead of the §6.1 all-zero-output guard — either rejection is fine.
        let alice = X25519Keypair::generate();
        let zero_peer = [0u8; 32];
        let err = diffie_hellman(&alice.secret, &zero_peer).unwrap_err();
        assert!(matches!(
            err,
            CryptoError::SmallOrderPeerKey | CryptoError::AllZeroSharedSecret
        ));
    }

    #[test]
    fn diffie_hellman_rejects_every_eight_torsion_point() {
        // PRY-5: no path to DH may agree against a small-order point. Exercise
        // the whole E[8] subgroup via the curve's own torsion table.
        use curve25519_dalek::constants::EIGHT_TORSION;
        let alice = X25519Keypair::generate();
        for tp in EIGHT_TORSION {
            let u = tp.to_montgomery().to_bytes();
            let err = diffie_hellman(&alice.secret, &u).unwrap_err();
            assert!(
                matches!(
                    err,
                    CryptoError::SmallOrderPeerKey | CryptoError::AllZeroSharedSecret
                ),
                "torsion point u={u:02x?} must be rejected, got {err:?}"
            );
        }
    }

    #[test]
    fn diffie_hellman_rejects_non_canonical_identity_encodings() {
        // PRY-31: `p` (≡ 0) and `p+1` (≡ 1) are non-canonical field encodings
        // that x25519-dalek does NOT reduce — they agree to the same weak
        // secret as u = 0 / u = 1 and were absent from the blocklist.
        let alice = X25519Keypair::generate();
        let mut p = [0xffu8; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        let mut p_plus_1 = p;
        p_plus_1[0] = 0xee;
        for u in [p, p_plus_1] {
            assert!(
                diffie_hellman(&alice.secret, &u).is_err(),
                "non-canonical identity encoding u={u:02x?} must be rejected"
            );
        }
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
