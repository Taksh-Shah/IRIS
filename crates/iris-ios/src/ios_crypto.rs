//! `IosCryptoProvider` — real Ed25519 signing (+ in-process X25519/AEAD) for iOS.
//!
//! CROSS-004: mirrors `crates/iris-android/src/android_crypto.rs`'s
//! `AndroidCryptoProvider` — signing is delegated to the Swift-side
//! `FfiCryptoSigner` (Keychain-backed Ed25519), closing the gap where
//! `IrisEngine::new()` hardcoded `DevCryptoProvider` (a no-op stub) with no
//! production crypto path at all.
//!
//! Unlike Android's provider, X25519 key agreement is NOT yet delegated to a
//! platform-side `FfiX25519KeyProvider` (that is Android's AN-6 follow-on,
//! itself a separate, larger finding not in this fix's scope) — `encrypt`/
//! `decrypt` use an in-process ephemeral/static X25519 keypair, same as
//! `AndroidCryptoProvider::new()`'s (non-`with_x25519_provider`) fallback
//! path. The defect this finding closes is specifically "no real signing, no
//! production crypto path at all, release build crashes at startup" — real
//! signatures make every envelope authenticatable, which was categorically
//! absent before.

use std::sync::Arc;

use iris_core::crypto::{self, keygen::X25519Keypair};
use iris_core::message_engine::crypto::{CryptoError, CryptoProvider};
use iris_core::protocol::{EncryptionHdr, Envelope};

use crate::ffi::crypto_signer::FfiCryptoSigner;

pub struct IosCryptoProvider {
    signer: Arc<dyn FfiCryptoSigner>,
    x25519: X25519Keypair,
}

impl IosCryptoProvider {
    pub fn new(signer: Arc<dyn FfiCryptoSigner>) -> Self {
        Self {
            signer,
            x25519: X25519Keypair::generate(),
        }
    }
}

#[async_trait::async_trait]
impl CryptoProvider for IosCryptoProvider {
    async fn sign(&self, envelope: &mut Envelope) -> Result<(), CryptoError> {
        let signable = iris_core::protocol::codec::encode_for_signing(envelope)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let sig_bytes = self
            .signer
            .sign(signable)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let sig: [u8; 64] = sig_bytes
            .try_into()
            .map_err(|_| CryptoError::Codec("signer returned != 64 bytes".into()))?;
        envelope.signature = Some(sig);
        Ok(())
    }

    async fn verify(&self, envelope: &Envelope) -> Result<bool, CryptoError> {
        let Some(sig) = envelope.signature else {
            return Ok(false);
        };
        // Only the 32-byte full-key form is supported here, same as
        // AndroidCryptoProvider — abbreviated P0 senders require a
        // ShortIdResolver (TrustStore lookup), not yet wired for iOS either.
        let key_bytes: [u8; 32] = envelope
            .sender_id
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::KeyUnavailable)?;
        let verifying = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| CryptoError::KeyUnavailable)?;
        let signable = iris_core::protocol::codec::encode_for_signing(envelope)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        Ok(crypto::ed25519::verify_strict(&verifying, &signable, &sig)?)
    }

    async fn encrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        plaintext: &[u8],
        recipient: &[u8; 32],
    ) -> Result<(Vec<u8>, EncryptionHdr), CryptoError> {
        use rand::RngCore;
        let eph = X25519Keypair::generate();
        let shared = crypto::x25519::diffie_hellman(&eph.secret, recipient)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let key = crypto::kdf::message_key(&shared, info)?;
        let mut nonce = [0u8; crypto::aead::NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let sealed = crypto::aead::encrypt(&key, &nonce, plaintext, aad)?;
        Ok((
            sealed,
            EncryptionHdr {
                ephemeral_pubkey: eph.public_bytes(),
                nonce,
                key_id: None,
            },
        ))
    }

    async fn decrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
        hdr: &EncryptionHdr,
    ) -> Result<Vec<u8>, CryptoError> {
        let shared = crypto::x25519::diffie_hellman(&self.x25519.secret, &hdr.ephemeral_pubkey)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let key = crypto::kdf::message_key(&shared, info)?;
        crypto::aead::decrypt(&key, &hdr.nonce, ciphertext, aad)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    fn authenticates(&self) -> bool {
        true
    }
}
