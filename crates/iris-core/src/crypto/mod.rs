//! Cryptographic layer — CRYPTO-001.
//!
//! Real primitives behind the `CryptoProvider` seam: Ed25519 signatures
//! (RFC 8032), X25519 key agreement (RFC 7748), ChaCha20-Poly1305 AEAD
//! (RFC 8439), HKDF-SHA256 key derivation (RFC 5869), and the at-rest storage
//! key derivation. Swaps the inert [`DevCryptoProvider`] backend for an
//! [`IrisCryptoProvider`] with zero engine changes.
//!
//! ## Security model (D4 / DEC-P0003 — FS honesty)
//!
//! v1 provides:
//! - sender authentication (Ed25519 `verify_strict`, weak-key guarded),
//! - confidentiality against passive eavesdroppers (per-message ephemeral
//!   X25519 + ChaCha20-Poly1305),
//! - sender-side forward secrecy (fresh ephemeral key per message).
//!
//! v1 does **NOT** provide forward secrecy against recipient long-term key
//! compromise (ECIES-like scheme; see RES-0015 §5 / ADR-0006 Issue 3). No
//! code or documentation claims recipient-side FS.
//!
//! ## Mandatory hygiene (RES-0015 D5)
//! - RFC 7748 §6.1: an all-zero X25519 shared secret is rejected.
//! - X25519 inputs are validated (32 bytes, clamped) before use.
//! - Ed25519 verification uses `verify_strict()` only — `verify()` is never
//!   called from production paths.

pub mod aead;
pub mod ed25519;
pub mod kdf;
pub mod key_directory;
pub mod keygen;
pub mod x25519;

use thiserror::Error;

/// Errors from the cryptographic primitives.
#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("crypto: invalid X25519 public key length {0} (expected 32)")]
    InvalidPublicKeyLen(usize),
    #[error("crypto: all-zero X25519 shared secret rejected (RFC 7748 §6.1)")]
    AllZeroSharedSecret,
    #[error("crypto: small-order X25519 peer public key rejected (RED-0011)")]
    SmallOrderPeerKey,
    #[error("crypto: X25519 operation failed")]
    DiffieHellman,
    #[error("crypto: Ed25519 signing failed")]
    Sign,
    #[error("crypto: Ed25519 signature verification rejected")]
    BadSignature,
    #[error("crypto: AEAD encryption failed")]
    Encrypt,
    #[error("crypto: AEAD decryption/tag verification failed")]
    Decrypt,
    #[error("crypto: invalid AAD or ciphertext (length {0})")]
    InvalidPayloadLen(usize),
    #[error("crypto: key derivation failed")]
    Kdf,
    #[error("crypto: key unavailable in directory")]
    KeyUnavailable,
    #[error("crypto: {0}")]
    Other(String),
}

/// KDF domain-separation salts (RFC 5869 `salt` parameter, fixed constants).
///
/// Distinct salts guarantee distinct derived keys even from the same IKM —
/// a message key never equals the at-rest storage key.
pub const MESSAGE_KEY_SALT: &[u8] = b"iris-message-key-v1";
pub const STORAGE_KEY_SALT: &[u8] = b"iris-at-rest-v1";
