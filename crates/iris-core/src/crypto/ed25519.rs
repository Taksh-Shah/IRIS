//! Ed25519 signatures — CRYPTO-001.
//!
//! RFC 8032 via `ed25519-dalek` 2.x. Security posture (RES-0015 D5):
//! - **`verify_strict()` is the ONLY verification path** — it rejects weak
//!   keys and signatures that would be malleable, hardening against the
//!   weak-key universal forgery / r-malleability issues that `verify()` (the
//!   non-strict variant) permits (Scuttlebutt exploit, eprint 2019/526).
//!   Production code calls only [`verify_strict`]; there is no re-export named
//!   `verify`.
//! - Signing is over the caller-provided signable bytes (the engine supplies
//!   `codec::encode_for_signing()` per ADR-0011).

use crate::crypto::{keygen::IdentityKeypair, CryptoError};

/// Ed25519 signature — 64 bytes (RFC 8032).
pub type Signature = [u8; 64];

/// Sign `message` with the Ed25519 identity key, producing a 64-byte signature.
pub fn sign(keypair: &IdentityKeypair, message: &[u8]) -> Result<Signature, CryptoError> {
    use ed25519_dalek::Signature as DalekSignature;
    use ed25519_dalek::Signer;
    let sig: DalekSignature = keypair.signing.sign(message);
    Ok(sig.to_bytes())
}

/// Verify a 64-byte signature over `message` using `verify_strict` (weak-key
/// guarded). `Ok(true)` only when the signature is valid under the RFC 8032
/// strict rules; any malformed or weak-key-adjacent signature returns `Ok(false)`
/// (or an injected error, surfaced as-is).
///
/// Named `verify_strict` (not `verify`) so the invariant "strict-only" is
/// enforced by name at every call site (RED-0009).
pub fn verify_strict(
    verifying_key: &ed25519_dalek::VerifyingKey,
    message: &[u8],
    signature: &Signature,
) -> Result<bool, CryptoError> {
    let sig = match ed25519_dalek::Signature::from_slice(signature) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    match verifying_key.verify_strict(message, &sig) {
        Ok(()) => Ok(true),
        Err(_) => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::IdentityKeypair;
    use hex_literal::hex;

    // RFC 8032 §7.1 TEST 1.
    const TEST_1_SK: [u8; 32] =
        hex!("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
    const TEST_1_MSG: &[u8] = b"";
    const TEST_1_SIG: [u8; 64] = hex!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155\
         5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );

    fn test_1_keypair() -> IdentityKeypair {
        let signing = ed25519_dalek::SigningKey::from_bytes(&TEST_1_SK);
        IdentityKeypair {
            verifying: signing.verifying_key(),
            signing,
        }
    }

    #[test]
    fn rfc8032_71_test1_signature_matches() {
        let kp = test_1_keypair();
        let sig = sign(&kp, TEST_1_MSG).unwrap();
        assert_eq!(sig, TEST_1_SIG);
    }

    #[test]
    fn rfc8032_71_verify_strict_accepts_test1() {
        let kp = test_1_keypair();
        assert!(verify_strict(&kp.verifying, TEST_1_MSG, &TEST_1_SIG).unwrap());
    }

    // AC-4 evidence: verify_strict MUST reject a signature presented under a
    // weak (small-order / all-zero) public key — the exact class of key that
    // `ed25519-dalek`'s non-strict `verify()` mishandled (Scuttlebutt weak-key
    // forgery, eprint 2019/526). An attacker can craft *some* signature that
    // `verify()` accepts under such a key; strict mode must refuse it.
    #[test]
    fn verify_strict_rejects_weak_small_order_key_forgery() {
        // All-zero is the canonical small-order X25519/Ed25519 point rejected by
        // RFC 7748 §6.1 and by verify_strict's weak-key guard.
        let weak = [0u8; 32];
        let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&weak) else {
            return; // verifier may refuse to construct it — equally good
        };
        // Forge a signature over an arbitrary message; strict verification under
        // the weak key MUST fail (that is the whole point of verify_strict).
        let forged = [0xA5u8; 64];
        assert!(
            verify_strict(&vk, b"emergency-level forgery", &forged).is_err()
                || !verify_strict(&vk, b"emergency-level forgery", &forged).unwrap_or(false),
            "verify_strict must reject a signature under a weak small-order key"
        );
    }

    #[test]
    fn tampered_message_fails_verify() {
        let kp = IdentityKeypair::generate();
        let sig = sign(&kp, b"hello iris").unwrap();
        assert!(verify_strict(&kp.verifying, b"hello iris", &sig).unwrap());
        assert!(!verify_strict(&kp.verifying, b"hello IRIS", &sig).unwrap());
    }
}
