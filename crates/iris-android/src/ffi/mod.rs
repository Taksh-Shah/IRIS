//! UniFFI projection modules for the three IRIS platform adapter traits.
//!
//! G-AND-3 spike (AC-3): proves the UniFFI 0.31 foreign-trait bridge accepts
//! the async/sync adapter shapes with the explicit tokio-handle pattern
//! (issue #2576 workaround). Un-commented-by-design: the raw `iris-core`
//! adapter traits use `MutexGuard` returns and reference params that cannot
//! cross the FFI boundary, so the spike introduces owned projection types.

pub mod ble_adapter;
pub mod error;
pub mod wifi_aware_adapter;
pub mod wifi_direct_adapter;

pub use ble_adapter::{
    FfiAdvertisementData, FfiBleAdapter, FfiGattWriteEvent, FfiScanFilter, FfiScanResult,
};
pub use error::IrisFfiError;
pub use wifi_aware_adapter::{
    FfiIncomingNdpData, FfiPeerDiscovery, FfiPublishConfig, FfiWifiAwareAdapter,
};
pub use wifi_direct_adapter::{
    FfiDirectPeerDiscovery, FfiGroupConfig, FfiGroupInfo, FfiIncomingWifiDirectData,
    FfiOperatingBand, FfiWifiDirectAdapter,
};
