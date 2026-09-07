//! FFI surface over `iris_core::identity::TrustStore` — the trusted-peers
//! feature's key-safety layer (QR-scan / manual-entry pairing, PRY-18
//! key-change protection, TRUST_MODEL.md SAS confirmation).
//!
//! Kotlin already builds a peer's signed advertisement locally with its own
//! Keystore-backed Ed25519 key (`X25519StaticAd.build()`); the private key
//! never crosses this boundary. This module only reconstructs and adopts
//! *received* advertisements (scanned via QR or typed in manually) into the
//! shared [`iris_core::identity::TrustStore`] that also backs the engine's
//! key directory and its `FullSecurityPolicy` emergency-ACL gate — one store,
//! so a trust decision made here is the same one the engine enforces.

use iris_core::identity::{
    advertise::ADVERTISE_FORMAT_VERSION, AdoptionOutcome, KeyAdvertisementV1, TrustLevel,
};

use crate::ffi::error::IrisFfiError;

/// A peer's signed key advertisement, scanned via QR or typed in manually.
/// `key_gen_counter` / `valid_until` are `0` for a first-pairing
/// advertisement — a future rotation flow will carry real values.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPeerAdvertisement {
    /// The peer's 32-byte Ed25519 identity public key (their PeerId).
    pub identity_pubkey: Vec<u8>,
    /// The peer's 32-byte X25519 static key-agreement public key.
    pub x25519_pubkey: Vec<u8>,
    pub key_gen_counter: u64,
    pub valid_until: u64,
    /// 64-byte `Ed25519(identity_pubkey's private key)` over the binding —
    /// built by the peer's own `X25519StaticAd.build()`.
    pub sig: Vec<u8>,
}

/// Mirrors [`iris_core::identity::TrustLevel`] across the FFI boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiTrustLevel {
    Unknown,
    Unverified,
    Verified,
    AuthorityRoot,
    KeyChanged,
    Revoked,
}

impl From<TrustLevel> for FfiTrustLevel {
    fn from(level: TrustLevel) -> Self {
        match level {
            TrustLevel::Unknown => FfiTrustLevel::Unknown,
            TrustLevel::Unverified => FfiTrustLevel::Unverified,
            TrustLevel::Verified => FfiTrustLevel::Verified,
            TrustLevel::AuthorityRoot => FfiTrustLevel::AuthorityRoot,
            TrustLevel::KeyChanged => FfiTrustLevel::KeyChanged,
            TrustLevel::Revoked => FfiTrustLevel::Revoked,
        }
    }
}

/// Mirrors [`iris_core::identity::AdoptionOutcome`] — what the UI reacts to
/// after scanning/typing a peer's advertisement. `KeyChangeWarn` in
/// particular must block sending and prompt re-verification (PRY-18): it
/// means a previously-seen identity is now presenting a *different* static
/// key with no valid rotation proof. Deliberately fieldless (unlike the core
/// `Rejected(AdvertiseError)`): a rejected advertisement collapses to
/// `Rejected` here, and `adopt_peer_advertisement` separately returns
/// `Err(IrisFfiError::InvalidArgument)` for a malformed (wrong-length) input,
/// so the UI never needs to parse a reason string out of this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiAdoptionOutcome {
    BoundUnverified,
    Duplicate,
    Refreshed,
    RotationAdopted,
    KeyChangeWarn,
    Revoked,
    Rejected,
}

impl From<AdoptionOutcome> for FfiAdoptionOutcome {
    fn from(outcome: AdoptionOutcome) -> Self {
        match outcome {
            AdoptionOutcome::BoundUnverified => FfiAdoptionOutcome::BoundUnverified,
            AdoptionOutcome::Duplicate => FfiAdoptionOutcome::Duplicate,
            AdoptionOutcome::Refreshed => FfiAdoptionOutcome::Refreshed,
            AdoptionOutcome::RotationAdopted => FfiAdoptionOutcome::RotationAdopted,
            AdoptionOutcome::KeyChangeWarn => FfiAdoptionOutcome::KeyChangeWarn,
            AdoptionOutcome::Revoked => FfiAdoptionOutcome::Revoked,
            AdoptionOutcome::Rejected(_) => FfiAdoptionOutcome::Rejected,
        }
    }
}

/// Reconstruct a [`KeyAdvertisementV1`] from the FFI record. Verification
/// (signature, small-order rejection, expiry) happens inside
/// `TrustStore::adopt_advertisement`, not here — this only validates shape.
pub(crate) fn to_key_advertisement(
    ad: FfiPeerAdvertisement,
) -> Result<KeyAdvertisementV1, IrisFfiError> {
    Ok(KeyAdvertisementV1 {
        format_version: ADVERTISE_FORMAT_VERSION,
        identity_pubkey: to_arr32(&ad.identity_pubkey, "identity_pubkey")?,
        static_x25519_pubkey: to_arr32(&ad.x25519_pubkey, "x25519_pubkey")?,
        key_gen_counter: ad.key_gen_counter,
        valid_until: ad.valid_until,
        sig: to_arr64(&ad.sig)?.into(),
    })
}

pub(crate) fn to_arr32(bytes: &[u8], field: &str) -> Result<[u8; 32], IrisFfiError> {
    <[u8; 32]>::try_from(bytes)
        .map_err(|_| IrisFfiError::InvalidArgument(format!("{field} must be 32 bytes")))
}

pub(crate) fn to_arr64(bytes: &[u8]) -> Result<[u8; 64], IrisFfiError> {
    <[u8; 64]>::try_from(bytes)
        .map_err(|_| IrisFfiError::InvalidArgument("sig must be 64 bytes".into()))
}
