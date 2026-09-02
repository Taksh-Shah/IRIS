//! Ed25519 signing bridge — the iOS Keychain owns the signing key; the Rust
//! engine delegates to Swift via this foreign-trait projection.
//!
//! CROSS-004 / mirrors `crates/iris-android/src/ffi/crypto_signer.rs` exactly
//! (same trait shape, same contract) so `IosCryptoProvider` can reuse
//! `AndroidCryptoProvider`'s verified sign/verify/encrypt/decrypt logic.

use crate::ffi::error::IrisFfiError;

/// Swift-implemented Ed25519 signer (wraps `KeychainEd25519`).
///
/// `sign(data)` must return exactly 64 raw Ed25519 signature bytes over
/// `data` (the `codec::encode_for_signing()` byte string), or an error.
#[uniffi::export(with_foreign)]
pub trait FfiCryptoSigner: Send + Sync + 'static {
    fn sign(&self, data: Vec<u8>) -> Result<Vec<u8>, IrisFfiError>;
}
