//! X25519 static-keypair bridge — the Kotlin-side `X25519StaticAd` owns the
//! key material so the secret never crosses the FFI boundary in raw form.
//!
//! AN-6: wires the complete key-agreement path that was written and tested in
//! isolation but never called from the running engine (see `X25519StaticAd.kt`
//! and `AndroidCryptoProvider::decrypt`).

use crate::ffi::error::IrisFfiError;

/// Kotlin-implemented X25519 static-key provider (wraps `X25519StaticAd`).
///
/// - `static_public_key()` — 32-byte X25519 public key for advertisement.
/// - `diffie_hellman(ephemeral_pubkey)` — compute `X25519(static_secret, eph)`
///   entirely inside Kotlin/JCA so the raw secret bytes never cross the FFI
///   boundary; returns the 32-byte shared secret.
#[uniffi::export(with_foreign)]
pub trait FfiX25519KeyProvider: Send + Sync + 'static {
    /// 32-byte X25519 public key advertised in the static-ad binding.
    fn static_public_key(&self) -> Result<Vec<u8>, IrisFfiError>;
    /// Perform X25519 DH against the given 32-byte ephemeral public key.
    /// Returns the 32-byte raw shared secret.
    fn diffie_hellman(&self, ephemeral_pubkey: Vec<u8>) -> Result<Vec<u8>, IrisFfiError>;
}
