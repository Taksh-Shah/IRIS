//! FFI-compatible error type (ANDROID_DESIGN D-3, Result contract).

/// Errors that can cross the UniFFI boundary from the Kotlin adapters back
/// into the Rust engine. UniFFI requires every foreign-trait method to return
/// `Result<_, CompatibleError>`; this is the compatible error type.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum IrisFfiError {
    #[error("not supported on this device")]
    NotSupported,
    #[error("permission denied")]
    PermissionDenied,
    #[error("adapter off")]
    AdapterOff,
    #[error("device not found")]
    DeviceNotFound,
    #[error("gatt failure: {0}")]
    GattFailure(String),
    #[error("timeout")]
    Timeout,
    #[error("io error: {0}")]
    IoError(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("transport error: {0}")]
    Transport(String),
}
