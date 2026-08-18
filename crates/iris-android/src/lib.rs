//! IRIS Android integration layer — UniFFI binding crate (ANDROID-001).
//!
//! AC-4 (ANDROID_DESIGN §4): the three UniFFI foreign-trait projections
//! (`ffi::*`) are bridged onto the `iris-core` transport traits
//! (`bridge::*`), and `IrisEngine` wires the real `TransportManager` +
//! `MessageEngine` over them (explicit tokio runtime-handle pattern, issue
//! #2576 workaround). G-AND-3 (AC-3) proved the projections compile; AC-4
//! makes the engine real.

uniffi::setup_scaffolding!();

pub mod bridge;
pub mod engine;
pub mod ffi;

pub use engine::{FfiInboxListener, FfiIncomingMessage, IrisEngine};
pub use ffi::ble_adapter::{
    FfiAdvertisementData, FfiBleAdapter, FfiGattWriteEvent, FfiScanFilter, FfiScanResult,
};
pub use ffi::error::IrisFfiError;
pub use ffi::wifi_aware_adapter::{
    FfiIncomingNdpData, FfiPeerDiscovery, FfiPublishConfig, FfiWifiAwareAdapter,
};
pub use ffi::wifi_direct_adapter::{
    FfiDirectPeerDiscovery, FfiGroupConfig, FfiGroupInfo, FfiIncomingWifiDirectData,
    FfiOperatingBand, FfiWifiDirectAdapter,
};
