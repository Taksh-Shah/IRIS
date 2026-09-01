//! `AndroidCryptoProvider` — real Ed25519/X25519/ChaCha20-Poly1305 for Android.
//!
//! Signing is delegated to the Kotlin-side `FfiCryptoSigner` (Android Keystore
//! TEE or software backend). Verification, key-agreement, and AEAD are in Rust.
//! AN-6: X25519 DH is now delegated to the Kotlin-side `FfiX25519KeyProvider`
//! (wraps `X25519StaticAd`) so the raw secret bytes never cross the FFI boundary.

use std::sync::Arc;

use iris_core::crypto::{self, keygen::X25519Keypair};
use iris_core::message_engine::crypto::{CryptoError, CryptoProvider};
use iris_core::protocol::{EncryptionHdr, Envelope};

use crate::ffi::crypto_signer::FfiCryptoSigner;
use crate::ffi::x25519_provider::FfiX25519KeyProvider;

pub struct AndroidCryptoProvider {
    signer: Arc<dyn FfiCryptoSigner>,
    /// AN-6: Kotlin-side X25519 provider (wraps `X25519StaticAd`). When
    /// present, DH for decryption is performed inside Kotlin/JCA so the raw
    /// static secret never crosses the FFI boundary. Falls back to an
    /// internally-generated ephemeral keypair when `None` (dev/test only).
    x25519_provider: Option<Arc<dyn FfiX25519KeyProvider>>,
    /// Fallback keypair used only when `x25519_provider` is `None`.
    fallback_x25519: X25519Keypair,
}

impl AndroidCryptoProvider {
    pub fn new(signer: Arc<dyn FfiCryptoSigner>) -> Self {
        Self {
            signer,
            x25519_provider: None,
            fallback_x25519: X25519Keypair::generate(),
        }
    }

    pub fn with_x25519_provider(
        signer: Arc<dyn FfiCryptoSigner>,
        x25519_provider: Arc<dyn FfiX25519KeyProvider>,
    ) -> Self {
        Self {
            signer,
            x25519_provider: Some(x25519_provider),
            fallback_x25519: X25519Keypair::generate(),
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
        // AN-6: delegate DH to Kotlin so the raw static secret stays in JCA.
        let shared: [u8; 32] = if let Some(provider) = &self.x25519_provider {
            let bytes = provider
                .diffie_hellman(hdr.ephemeral_pubkey.to_vec())
                .map_err(|e| CryptoError::Codec(e.to_string()))?;
            bytes
                .try_into()
                .map_err(|_| CryptoError::Codec("X25519 provider returned != 32 bytes".into()))?
        } else {
            // Fallback: use the internally-generated keypair (dev/test only).
            crypto::x25519::diffie_hellman(&self.fallback_x25519.secret, &hdr.ephemeral_pubkey)
                .map_err(|e| CryptoError::Codec(e.to_string()))?
        };
        let key = crypto::kdf::message_key(&shared, info)?;
        crypto::aead::decrypt(&key, &hdr.nonce, ciphertext, aad)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    fn authenticates(&self) -> bool {
        true
    }
}
