//! Keypair generation — CRYPTO-001.
//!
//! X25519 static/ephemeral keypairs (RFC 7748) and Ed25519 identity keypairs
//! (RFC 8032), generated from the OS CSPRNG. Key **storage/provisioning** is
//! the IDENT-001 concern (Android Keystore TEE/StrongBox, OS keychain); this
//! module only produces in-memory key material.

use ed25519_dalek::{SigningKey as Ed25519SigningKey, VerifyingKey as Ed25519VerifyingKey};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519StaticSecret};

/// An X25519 keypair (static recipient or ephemeral).
#[derive(Clone)]
pub struct X25519Keypair {
    pub secret: X25519StaticSecret,
    pub public: X25519PublicKey,
}

impl X25519Keypair {
    /// Generate a fresh X25519 keypair from the OS CSPRNG.
    pub fn generate() -> Self {
        let secret = X25519StaticSecret::random_from_rng(rand::rngs::OsRng);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    /// Reconstruct a keypair from an existing 32-byte secret (e.g. a
    /// provisioned master key from IDENT-001). Never logs the secret.
    pub fn from_secret_bytes(secret: [u8; 32]) -> Self {
        let secret = X25519StaticSecret::from(secret);
        let public = X25519PublicKey::from(&secret);
        Self { secret, public }
    }

    pub fn public_bytes(&self) -> [u8; 32] {
        self.public.to_bytes()
    }
}

/// An Ed25519 identity keypair (node identity signing — IDENT-001).
pub struct IdentityKeypair {
    pub signing: Ed25519SigningKey,
    pub verifying: Ed25519VerifyingKey,
}

impl IdentityKeypair {
    /// Generate a fresh Ed25519 identity keypair from the OS CSPRNG.
    pub fn generate() -> Self {
        let signing = Ed25519SigningKey::generate(&mut rand::rngs::OsRng);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    /// Reconstruct an identity keypair from an existing 32-byte RFC 8032 seed
    /// (e.g. a provisioned identity seed from IDENT-001). Never logs the seed.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing = Ed25519SigningKey::from_bytes(&seed);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    pub fn verifying_bytes(&self) -> [u8; 32] {
        self.verifying.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x25519_keypair_is_32_bytes_and_valid() {
        let kp = X25519Keypair::generate();
        assert_eq!(kp.public_bytes().len(), 32);
        // Public key must not be all zeros.
        assert_ne!(kp.public_bytes(), [0u8; 32]);
    }

    #[test]
    fn x25519_from_secret_is_deterministic() {
        let kp_a = X25519Keypair::from_secret_bytes([7u8; 32]);
        let kp_b = X25519Keypair::from_secret_bytes([7u8; 32]);
        assert_eq!(kp_a.public_bytes(), kp_b.public_bytes());
    }

    #[test]
    fn ed25519_identity_is_32_bytes() {
        let kp = IdentityKeypair::generate();
        assert_eq!(kp.verifying_bytes().len(), 32);
    }

    #[test]
    fn ed25519_from_seed_is_deterministic() {
        let a = IdentityKeypair::from_seed([0x42u8; 32]);
        let b = IdentityKeypair::from_seed([0x42u8; 32]);
        assert_eq!(a.verifying_bytes(), b.verifying_bytes());
        let c = IdentityKeypair::from_seed([0x43u8; 32]);
        assert_ne!(a.verifying_bytes(), c.verifying_bytes());
    }

    /// PRY-25: `X25519Keypair` is `Clone` and has no explicit `Drop` — it
    /// relies entirely on `x25519-dalek`'s `zeroize` cargo feature to wipe the
    /// static secret on drop. This test fails if that feature is ever dropped
    /// (e.g. a `default-features = false` or a dependency bump), which is why
    /// `Cargo.toml` now pins `x25519-dalek` with `features = [..., "zeroize"]`.
    /// Same alloc/drop_in_place/read technique as
    /// `provision::tests::seeds_are_zeroized_on_drop`.
    #[test]
    fn x25519_static_secret_is_zeroized_on_drop() {
        use std::alloc::{self, Layout};
        use x25519_dalek::StaticSecret;

        let layout = Layout::new::<StaticSecret>();
        let ptr = unsafe { alloc::alloc(layout) } as *mut StaticSecret;
        assert!(!ptr.is_null(), "alloc failed");
        unsafe { ptr.write(StaticSecret::from([0x5Au8; 32])) };

        let view = ptr as *const u8;
        let before = unsafe { std::slice::from_raw_parts(view, 32) };
        assert!(before.iter().any(|&b| b != 0), "secret should be non-zero");

        unsafe { std::ptr::drop_in_place(ptr) };
        let after = unsafe { std::slice::from_raw_parts(view, 32) }.to_vec();
        unsafe { alloc::dealloc(ptr as *mut u8, layout) };

        assert!(
            after.iter().all(|&b| b == 0),
            "X25519 static secret must be zeroed by Drop (zeroize feature missing?)"
        );
    }
}
