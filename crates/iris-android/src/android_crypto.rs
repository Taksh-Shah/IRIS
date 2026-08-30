//! `AndroidCryptoProvider` — real Ed25519/X25519/ChaCha20-Poly1305 for Android.
//!
//! Signing is delegated to the Kotlin-side `FfiCryptoSigner` (Android Keystore
//! TEE or software backend). Verification, key-agreement, and AEAD are in Rust.
//! X25519 static keypair is generated once per engine lifetime; AN-6 will wire
//! the Kotlin-side `X25519StaticAd` once AN-1 (this file) lands.

use std::sync::Arc;

use iris_core::crypto::{self, keygen::X25519Keypair};
use iris_core::message_engine::crypto::{CryptoError, CryptoProvider};
use iris_core::protocol::{EncryptionHdr, Envelope};

use crate::ffi::crypto_signer::FfiCryptoSigner;

pub struct AndroidCryptoProvider {
    signer: Arc<dyn FfiCryptoSigner>,
    static_x25519: X25519Keypair,
}

impl AndroidCryptoProvider {
    pub fn new(signer: Arc<dyn FfiCryptoSigner>) -> Self {
        Self {
            signer,
            static_x25519: X25519Keypair::generate(),
        }
    }
}

#[async_trait::async_trait]
impl CryptoProvider for AndroidCryptoProvider {
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
        // Only the 32-byte full-key form is supported here. Abbreviated P0
        // senders require a ShortIdResolver (TrustStore lookup) — wired in
        // AN-6 once the Kotlin trust path is established.
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
        let shared = crypto::x25519::diffie_hellman(&self.static_x25519.secret, &hdr.ephemeral_pubkey)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let key = crypto::kdf::message_key(&shared, info)?;
        crypto::aead::decrypt(&key, &hdr.nonce, ciphertext, aad)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    fn authenticates(&self) -> bool {
        true
    }
}
