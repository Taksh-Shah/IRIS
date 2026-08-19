//! FFI-compatible error type (ANDROID_DESIGN D-3 / IOS_DESIGN §4 Result contract).

/// Errors that can cross the UniFFI boundary from the Swift adapters back into
/// the Rust engine. UniFFI requires every foreign-trait method to return
/// `Result<_, CompatibleError>`; this is the compatible error type.
///
/// `Timeout` carries BLE-RT-C003 (IOS_DESIGN §4 / AC-6): the `gatt_read` path
/// in the Swift `IosBleAdapter` enforces a hard call-timeout (default 10 s) and
/// surfaces it here as a typed error — no hang, no leaked task.
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
