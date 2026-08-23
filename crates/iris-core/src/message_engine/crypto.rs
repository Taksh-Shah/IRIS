//! Crypto seam — MSG-001 ↔ CRYPTO-001 (R1 risk mitigation: no hand-rolled crypto).
//!
//! The engine never calls crypto primitives directly. It depends on
//! [`CryptoProvider`]. Two implementations exist:
//! - [`DevCryptoProvider`]: inert test/dev backend — sign returns an all-ones
//!   signature, encrypt/decrypt pass through. NOT for production.
//! - [`IrisCryptoProvider`]: real Ed25519 (RFC 8032, `verify_strict` only),
//!   X25519 (RFC 7748), ChaCha20-Poly1305 (RFC 8439), HKDF-SHA256 (RFC 5869).
//!   Swaps in behind this trait with zero engine changes (CRYPTO-001, D1–D5).
//!
//! ## Message binding (CRYPTO-001 AAD + KDF contracts)
//!
//! `encrypt`/`decrypt` accept `info` (the KDF per-message info,
//! `message_id ‖ direction`) and `aad` (`codec::encode_for_aead(envelope)`
//! — the immutable message-identity fields 1–7, 9–11, 14). The engine
//! computes these from the envelope via `codec` helpers and passes them
//! through; the provider never derives them itself. This binds each
//! ciphertext to its message identity (no cross-message ciphertext replay).

use crate::crypto::{
    self, keygen::IdentityKeypair, keygen::X25519Keypair, CryptoError as PrimCryptoError,
};
use crate::protocol::MessageId;

/// Direction byte for the per-message KDF `info` (v1: 0x00 = MESSAGE).
/// 0x01=SEND / 0x02=RECV reserved for v2 bidirectional session mode.
pub const KDF_DIRECTION_MESSAGE: u8 = 0x00;

/// Per-message KDF `info` = `message_id (16 B) ‖ direction (1 B)`.
///
/// Both sender and recipient derive identical keys via this exact byte string
/// (CRYPTO_DESIGN.md §KDF contracts). The engine and provider tests share this
/// helper — no caller may derive the info differently.
pub fn message_kdf_info(id: MessageId) -> [u8; 17] {
    let mut info = [0u8; 17];
    info[..16].copy_from_slice(&id.to_bytes());
    info[16] = KDF_DIRECTION_MESSAGE;
    info
}
use crate::protocol::{Envelope, EnvelopeError};

/// Error from the crypto seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CryptoError {
    /// Signature verification failed (tampered / wrong key).
    BadSignature,
    /// Decryption failed (wrong key, corrupted ciphertext).
    DecryptionFailed,
    /// Missing key material.
    KeyUnavailable,
    /// A provider is unavailable / not yet implemented.
    NotImplemented(String),
    /// Underlying wire encoding failed while preparing signable bytes.
    Codec(String),
}

impl std::fmt::Display for CryptoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CryptoError::BadSignature => write!(f, "crypto: bad signature"),
            CryptoError::DecryptionFailed => write!(f, "crypto: decryption failed"),
            CryptoError::KeyUnavailable => write!(f, "crypto: key unavailable"),
            CryptoError::NotImplemented(m) => write!(f, "crypto: not implemented: {m}"),
            CryptoError::Codec(m) => write!(f, "crypto: codec: {m}"),
        }
    }
}

impl std::error::Error for CryptoError {}

impl From<EnvelopeError> for CryptoError {
    fn from(e: EnvelopeError) -> Self {
        CryptoError::Codec(e.to_string())
    }
}

impl From<PrimCryptoError> for CryptoError {
    fn from(e: PrimCryptoError) -> Self {
        match e {
            PrimCryptoError::BadSignature => CryptoError::BadSignature,
            PrimCryptoError::KeyUnavailable => CryptoError::KeyUnavailable,
            PrimCryptoError::Decrypt
            | PrimCryptoError::AllZeroSharedSecret
            | PrimCryptoError::DiffieHellman
            | PrimCryptoError::InvalidPublicKeyLen(_)
            | PrimCryptoError::InvalidPayloadLen(_)
            | PrimCryptoError::Encrypt
            | PrimCryptoError::Sign
            | PrimCryptoError::Kdf
            | PrimCryptoError::Other(_) => CryptoError::Codec(e.to_string()),
        }
    }
}

/// The cryptographic operations the message engine needs.
#[async_trait::async_trait]
pub trait CryptoProvider: Send + Sync + 'static {
    /// Sign an envelope for transmission (updates `envelope.signature`).
    async fn sign(&self, envelope: &mut Envelope) -> Result<(), CryptoError>;

    /// Verify an envelope's signature. `Ok(true)` when valid.
    async fn verify(&self, envelope: &Envelope) -> Result<bool, CryptoError>;

    /// Encrypt a payload for a recipient (fills `encryption_hdr`).
    ///
    /// `info` = KDF per-message info (`message_id ‖ direction`); `aad` =
    /// `codec::encode_for_aead(envelope)` (message-identity binding).
    async fn encrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        plaintext: &[u8],
        recipient: &[u8; 32],
    ) -> Result<(Vec<u8>, crate::protocol::EncryptionHdr), CryptoError>;

    /// Decrypt a payload. `info`/`aad` as for [`CryptoProvider::encrypt`].
    async fn decrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
        hdr: &crate::protocol::EncryptionHdr,
    ) -> Result<Vec<u8>, CryptoError>;

    /// Whether this provider actually authenticates (false for dev backend).
    fn authenticates(&self) -> bool;
}

/// Development/test backend — structurally valid, cryptographically inert.
///
/// ⚠️ MUST NOT be used in production. Replaced by [`IrisCryptoProvider`].
#[derive(Debug, Clone, Copy, Default)]
pub struct DevCryptoProvider;

impl DevCryptoProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl CryptoProvider for DevCryptoProvider {
    async fn sign(&self, envelope: &mut Envelope) -> Result<(), CryptoError> {
        envelope.signature = Some([0xFF; 64]);
        Ok(())
    }

    async fn verify(&self, _envelope: &Envelope) -> Result<bool, CryptoError> {
        Ok(true)
    }

    async fn encrypt(
        &self,
        _info: &[u8],
        _aad: &[u8],
        plaintext: &[u8],
        recipient: &[u8; 32],
    ) -> Result<(Vec<u8>, crate::protocol::EncryptionHdr), CryptoError> {
        Ok((
            plaintext.to_vec(),
            crate::protocol::EncryptionHdr {
                ephemeral_pubkey: *recipient,
                nonce: [0u8; 12],
                key_id: None,
            },
        ))
    }

    async fn decrypt(
        &self,
        _info: &[u8],
        _aad: &[u8],
        ciphertext: &[u8],
        _hdr: &crate::protocol::EncryptionHdr,
    ) -> Result<Vec<u8>, CryptoError> {
        Ok(ciphertext.to_vec())
    }

    fn authenticates(&self) -> bool {
        false
    }
}

/// Node identity bundle — the signing (Ed25519) + static key-agreement
/// (X25519) key pair used by [`IrisCryptoProvider`]. Provisioning/attestation
/// is the IDENT-001 concern; v1 generates in-memory keys (tests/desktop).
pub struct NodeIdentity {
    pub identity: IdentityKeypair,
    pub static_x25519: X25519Keypair,
}

impl NodeIdentity {
    pub fn generate() -> Self {
        Self {
            identity: IdentityKeypair::generate(),
            static_x25519: X25519Keypair::generate(),
        }
    }
}

/// Production backend — real Ed25519/X25519/ChaCha20-Poly1305 (CRYPTO-001).
///
/// Swaps in behind the [`CryptoProvider`] seam with zero engine changes.
/// Uses `verify_strict()` only; rejects all-zero X25519 shared secrets
/// (RFC 7748 §6.1); binds every ciphertext to its message identity via the
/// `info`/`aad` passed from the engine (`codec::encode_for_aead`).
pub struct IrisCryptoProvider {
    identity: std::sync::Arc<NodeIdentity>,
    short_id_resolver: Option<std::sync::Arc<dyn ShortIdResolver>>,
}

/// Length of the abbreviated `peer_short` sender form (SHA-256 prefix).
const PEER_SHORT_LEN: usize = 16;

/// Resolves an abbreviated 16-byte `peer_short` sender back to the full 32-byte
/// Ed25519 identity key needed to verify a signature.
///
/// P0 abbreviated envelopes (the 255-byte LoRa SOS form) carry
/// `peer_short = SHA-256(pubkey)[..16]` instead of the full key, because the
/// full key does not fit. Verification therefore needs a directory lookup; a
/// node that has never seen the sender cannot verify it and must fail closed.
pub trait ShortIdResolver: Send + Sync + 'static {
    /// Full identity key for `peer_short`, or `None` when unknown.
    fn resolve(&self, peer_short: &[u8; 16]) -> Option<[u8; 32]>;
}

/// Resolves short ids by scanning the trust store's known identities.
impl ShortIdResolver for crate::identity::trust_store::TrustStore {
    fn resolve(&self, peer_short: &[u8; 16]) -> Option<[u8; 32]> {
        self.entries().into_iter().find_map(|e| {
            (crate::identity::peer_short_from_sender(&e.identity_pubkey) == *peer_short)
                .then_some(e.identity_pubkey)
        })
    }
}

impl IrisCryptoProvider {
    pub fn new(identity: std::sync::Arc<NodeIdentity>) -> Self {
        Self {
            identity,
            short_id_resolver: None,
        }
    }

    /// Attach the directory used to expand abbreviated P0 senders.
    ///
    /// Without one, abbreviated envelopes fail closed with `KeyUnavailable` —
    /// an unverifiable signature must never be accepted.
    pub fn with_short_id_resolver(mut self, resolver: std::sync::Arc<dyn ShortIdResolver>) -> Self {
        self.short_id_resolver = Some(resolver);
        self
    }

    /// Full verifying key for an envelope's `sender_id`.
    ///
    /// The codec accepts both the 32-byte full key and the 16-byte abbreviated
    /// form, and the golden corpus pins an abbreviated P0 SOS vector — but this
    /// used to be a bare `try_into::<[u8; 32]>()`, so every abbreviated
    /// envelope returned `KeyUnavailable` and was hard-rejected before
    /// delivery. The 255-byte LoRa SOS path, the system's primary emergency
    /// use case, could not work end to end.
    fn verifying_key_for(&self, sender_id: &[u8]) -> Result<[u8; 32], CryptoError> {
        match sender_id.len() {
            32 => sender_id
                .try_into()
                .map_err(|_| CryptoError::KeyUnavailable),
            PEER_SHORT_LEN => {
                let mut short = [0u8; PEER_SHORT_LEN];
                short.copy_from_slice(sender_id);
                self.short_id_resolver
                    .as_ref()
                    .and_then(|r| r.resolve(&short))
                    .ok_or(CryptoError::KeyUnavailable)
            }
            _ => Err(CryptoError::KeyUnavailable),
        }
    }
}

#[async_trait::async_trait]
impl CryptoProvider for IrisCryptoProvider {
    async fn sign(&self, envelope: &mut Envelope) -> Result<(), CryptoError> {
        let signable = crate::protocol::codec::encode_for_signing(envelope)?;
        let sig = crypto::ed25519::sign(&self.identity.identity, &signable)?;
        envelope.signature = Some(sig);
        Ok(())
    }

    async fn verify(&self, envelope: &Envelope) -> Result<bool, CryptoError> {
        let Some(sig) = envelope.signature else {
            return Ok(false);
        };
        // IDENT-001: a node's identity IS its 32-byte Ed25519 public key
        // (captured in `sender_id`). Resolve it from the wire, never from the
        // local key store (we may be a relay).
        let key_bytes = self.verifying_key_for(&envelope.sender_id)?;
        let verifying = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes)
            .map_err(|_| CryptoError::KeyUnavailable)?;
        let signable = crate::protocol::codec::encode_for_signing(envelope)?;
        // RFC 8032 verify_strict (weak-key guard). Never verify().
        Ok(crypto::ed25519::verify_strict(&verifying, &signable, &sig)?)
    }

    async fn encrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        plaintext: &[u8],
        recipient: &[u8; 32],
    ) -> Result<(Vec<u8>, crate::protocol::EncryptionHdr), CryptoError> {
        use rand::RngCore;
        // D1 (DEC-P0001): per-message ephemeral X25519 keypair.
        let eph = X25519Keypair::generate();
        let shared = crypto::x25519::diffie_hellman(&eph.secret, recipient)
            .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let key = crypto::kdf::message_key(&shared, info)?;
        let mut nonce = [0u8; crate::crypto::aead::NONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let sealed = crypto::aead::encrypt(&key, &nonce, plaintext, aad)?;
        Ok((
            sealed,
            crate::protocol::EncryptionHdr {
                ephemeral_pubkey: eph.public_bytes(),
                nonce,
                key_id: None, // v1: single long-term key per node
            },
        ))
    }

    async fn decrypt(
        &self,
        info: &[u8],
        aad: &[u8],
        ciphertext: &[u8],
        hdr: &crate::protocol::EncryptionHdr,
    ) -> Result<Vec<u8>, CryptoError> {
        let shared = crypto::x25519::diffie_hellman(
            &self.identity.static_x25519.secret,
            &hdr.ephemeral_pubkey,
        )
        .map_err(|e| CryptoError::Codec(e.to_string()))?;
        let key = crypto::kdf::message_key(&shared, info)?;
        crypto::aead::decrypt(&key, &hdr.nonce, ciphertext, aad)
            .map_err(|_| CryptoError::DecryptionFailed)
    }

    fn authenticates(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessagePriority;
    use crate::protocol::{Envelope, MessageId};

    fn make_node() -> std::sync::Arc<NodeIdentity> {
        std::sync::Arc::new(NodeIdentity::generate())
    }

    fn make_env(node: &NodeIdentity, recipient: &NodeIdentity, body: &[u8]) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: node.identity.verifying_bytes().to_vec(),
            recipient_id: recipient.identity.verifying_bytes().to_vec(),
            priority: MessagePriority::P3,
            ttl_seconds: 3600,
            timestamp: 1_752_000_000,
            hop_count: 0,
            max_hops: None,
            payload_type: crate::protocol::ContentType::Text,
            payload_size: body.len() as u64,
            payload_hash: [0; 32],
            payload: body.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    /// Minimal resolver so the abbreviated-sender path can be exercised
    /// without standing up a full trust store.
    struct FixedResolver([u8; 32]);
    impl ShortIdResolver for FixedResolver {
        fn resolve(&self, peer_short: &[u8; 16]) -> Option<[u8; 32]> {
            (crate::identity::peer_short_from_sender(&self.0) == *peer_short).then_some(self.0)
        }
    }

    #[tokio::test]
    async fn abbreviated_p0_sender_verifies_via_the_resolver() {
        // Regression: `verify` did a bare try_into::<[u8; 32]>() on sender_id,
        // so every 16-byte `peer_short` sender returned KeyUnavailable and was
        // hard-rejected. The codec accepts that form and the golden corpus pins
        // a P0 SOS vector using it, so the 255-byte LoRa SOS path — the primary
        // emergency use case — could not work end to end.
        let node = make_node();
        let recipient = make_node();
        let full_key = node.identity.verifying_bytes();

        let signer = IrisCryptoProvider::new(node.clone());
        let mut env = make_env(&node, &recipient, b"SOS");
        env.priority = MessagePriority::P0;
        // Abbreviate the sender to the on-air P0 form.
        env.sender_id = crate::identity::peer_short_from_sender(&full_key).to_vec();
        signer.sign(&mut env).await.unwrap();

        // A relay that has never seen this peer cannot verify it and must fail
        // closed rather than accept an unverifiable signature.
        let stranger = IrisCryptoProvider::new(make_node());
        assert!(
            matches!(
                stranger.verify(&env).await,
                Err(CryptoError::KeyUnavailable)
            ),
            "unknown abbreviated sender must fail closed"
        );

        // A relay that knows the peer resolves the full key and verifies.
        let knower = IrisCryptoProvider::new(make_node())
            .with_short_id_resolver(std::sync::Arc::new(FixedResolver(full_key)));
        assert!(
            knower.verify(&env).await.unwrap(),
            "abbreviated P0 envelope must verify once the sender is known"
        );

        // Tampering is still caught on the abbreviated path.
        let mut tampered = env.clone();
        tampered.payload = b"NOT SOS".to_vec();
        tampered.payload_size = tampered.payload.len() as u64;
        assert!(!knower.verify(&tampered).await.unwrap());
    }

    #[tokio::test]
    async fn e2e_roundtrip_sign_encrypt_verify_decrypt() {
        let node = make_node();
        let recipient = make_node();
        let provider = IrisCryptoProvider::new(node.clone());

        let mut env = make_env(&node, &recipient, b"hello, resilient world");

        // Encryption first. Field 11 (payload_size) is in the AAD scope and is
        // ciphertext-dependent, so set it to the sealed length BEFORE deriving
        // the AAD (AEAD output is length-preserving: plaintext + tag).
        let sealed_len = crate::crypto::aead::sealed_len(env.payload.len());
        env.payload_size = sealed_len as u64;
        let info = message_kdf_info(env.message_id).to_vec();
        let aad = crate::protocol::codec::encode_for_aead(&env).unwrap();
        let (sealed, hdr) = provider
            .encrypt(
                &info,
                &aad,
                &env.payload,
                &recipient.static_x25519.public_bytes(),
            )
            .await
            .unwrap();
        assert_eq!(sealed.len(), sealed_len);
        env.payload = sealed;
        env.encryption_hdr = Some(hdr);

        // Sign over the signing scope.
        provider.sign(&mut env).await.unwrap();

        // Recipient: verify then decrypt.
        let rp = IrisCryptoProvider::new(recipient.clone());
        assert!(rp.verify(&env).await.unwrap());
        let aad_recv = crate::protocol::codec::encode_for_aead(&env).unwrap();
        let plain = rp
            .decrypt(
                &info,
                &aad_recv,
                &env.payload,
                env.encryption_hdr.as_ref().unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(plain, b"hello, resilient world");
    }

    #[tokio::test]
    async fn tampered_ciphertext_fails_verify_or_decrypt() {
        let node = make_node();
        let recipient = make_node();
        let provider = IrisCryptoProvider::new(node.clone());

        let mut env = make_env(&node, &recipient, b"secret");

        let sealed_len = crate::crypto::aead::sealed_len(env.payload.len());
        env.payload_size = sealed_len as u64;
        let info = message_kdf_info(env.message_id).to_vec();
        let aad = crate::protocol::codec::encode_for_aead(&env).unwrap();
        let (sealed, hdr) = provider
            .encrypt(
                &info,
                &aad,
                &env.payload,
                &recipient.static_x25519.public_bytes(),
            )
            .await
            .unwrap();
        env.payload = sealed;
        env.encryption_hdr = Some(hdr);
        provider.sign(&mut env).await.unwrap();

        // Flip one ciphertext byte → recipient decrypt must fail.
        let mut tampered = env.payload.clone();
        tampered[0] ^= 0x01;
        let rp = IrisCryptoProvider::new(recipient.clone());
        let aad_recv = crate::protocol::codec::encode_for_aead(&env).unwrap();
        let res = rp
            .decrypt(
                &info,
                &aad_recv,
                &tampered,
                env.encryption_hdr.as_ref().unwrap(),
            )
            .await;
        assert!(matches!(res, Err(CryptoError::DecryptionFailed)));

        // Tamper AAD (message identity) → decrypt must fail even with intact ciphertext.
        let mut aad2 = aad_recv.clone();
        aad2[0] ^= 0xFF;
        let res2 = rp
            .decrypt(
                &info,
                &aad2,
                &env.payload,
                env.encryption_hdr.as_ref().unwrap(),
            )
            .await;
        assert!(matches!(res2, Err(CryptoError::DecryptionFailed)));
    }

    #[tokio::test]
    async fn authenticates_is_true() {
        let provider = IrisCryptoProvider::new(make_node());
        assert!(provider.authenticates());
    }
}
