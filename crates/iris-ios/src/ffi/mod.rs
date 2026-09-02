//! UniFFI projection modules for the iOS platform adapters.
//!
//! Mirrors `crates/iris-android/src/ffi` (G-AND-3 spike, ANDROID_DESIGN §10):
//! owned, UniFFI-safe projection types (issue #2263) + `#[uniffi::export(with_foreign)]`
//! foreign traits. The Rust crate stays warning-free because the raw `iris-core`
//! adapter traits use reference params and `MutexGuard` drains that cannot
//! cross the FFI boundary — the projections are owned records / Vec drains.

pub mod ble_adapter;
pub mod body;
pub mod crypto_signer;
pub mod error;

pub use ble_adapter::{
    FfiAdvertisementData, FfiBleAdapter, FfiGattWriteEvent, FfiScanFilter, FfiScanResult,
};
pub use body::{FfiIrisEnvelope, IrisBody};
pub use crypto_signer::FfiCryptoSigner;
pub use error::IrisFfiError;
