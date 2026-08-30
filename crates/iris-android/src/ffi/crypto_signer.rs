//! Ed25519 signing bridge — the Android Keystore owns the signing key;
//! the Rust engine delegates to Kotlin via this foreign-trait projection.

use crate::ffi::error::IrisFfiError;

/// Kotlin-implemented Ed25519 signer (wraps `KeystoreEd25519`).
///
/// `sign(data)` must return exactly 64 raw Ed25519 signature bytes over
/// `data` (the `codec::encode_for_signing()` byte string), or an error.
#[uniffi::export(with_foreign)]
pub trait FfiCryptoSigner: Send + Sync + 'static {
    fn sign(&self, data: Vec<u8>) -> Result<Vec<u8>, IrisFfiError>;
}
