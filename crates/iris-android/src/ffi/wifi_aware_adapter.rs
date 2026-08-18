//! `WifiAwareAdapter` — UniFFI foreign-trait projection (async, by-value).
//!
//! Mirrors `iris_core::transport::wifiaware::WifiAwareAdapter` (12-op) with
//! async trait methods and **owned** params (issue #2263: no `&PublishConfig`,
//! no `&[u8]`). The real trait's `is_available()` / `availability_stream()`
//! push channel become an FFI query — recorded spike finding (proj-WA-1).

use async_trait::async_trait;

use crate::ffi::error::IrisFfiError;

/// Owned publish config (mirrors `wifiaware::PublishConfig`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPublishConfig {
    pub service_name: String,
    /// `-1` = broadcast to every IRIS neighbor.
    pub instance_id: i16,
    pub cached: bool,
    pub ttl_s: u16,
}

/// Owned discovery match (`wifiaware::PeerDiscovery`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPeerDiscovery {
    /// Peer handle owned by the adapter.
    pub peer_handle: u64,
    /// Raw `service_specific_info` bytes (the IRIS beacon, WIFI_AWARE.md §4.2).
    /// AC-4 projection fix: the beacon bytes are the ONLY wire identity the
    /// transport parses (`WifiAwareBeacon::parse`), so the projection must
    /// carry them — the earlier `service_instance: String` field dropped them.
    pub service_specific_info: Vec<u8>,
    /// rssi dBm.
    pub rssi: i32,
}

/// Owned inbound NDP frame (`wifiaware::IncomingNdpData`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiIncomingNdpData {
    /// Best-effort candidate sender identity (beacon short id when known).
    /// AC-4 projection fix: the transport attributes the frame via `sender`
    /// (NEW-WA-RT-101); the earlier `peer_handle: u64` carried the sender's
    /// opaque NDP handle, which is meaningless to the receiver.
    pub sender: Option<Vec<u8>>,
    pub ndp_handle: u64,
    pub payload: Vec<u8>,
}

/// Foreign-trait projection of `wifiaware::WifiAwareAdapter`. Implemented by
/// `AndroidWifiAwareTransportAdapter` (Kotlin); simulated in Rust unit tests.
/// Async trait methods over FFI → generated Kotlin `suspend fun` (G-AND-3).
/// `#[async_trait]` is REQUIRED (not optional): UniFFI 0.31 lowers async
/// foreign-trait methods through `Arc<dyn Trait>`, which needs the boxed-future
/// form `async_trait` produces (plain `async fn` is not dyn-compatible).
#[uniffi::export(with_foreign)]
#[async_trait]
pub trait FfiWifiAwareAdapter: Send + Sync + 'static {
    async fn start(&self) -> Result<(), IrisFfiError>;
    async fn subscribe(&self) -> Result<(), IrisFfiError>;
    async fn unsubscribe(&self) -> Result<(), IrisFfiError>;
    async fn publish(&self, config: FfiPublishConfig) -> Result<(), IrisFfiError>;
    async fn unpublish(&self) -> Result<(), IrisFfiError>;
    async fn matches(&self) -> Result<Vec<FfiPeerDiscovery>, IrisFfiError>;
    async fn open_ndp(&self, peer_handle: u64) -> Result<u64, IrisFfiError>;
    async fn close_ndp(&self, ndp_handle: u64) -> Result<(), IrisFfiError>;
    async fn ndp_send(&self, ndp_handle: u64, payload: Vec<u8>) -> Result<(), IrisFfiError>;
    async fn incoming_ndp(&self) -> Result<Vec<FfiIncomingNdpData>, IrisFfiError>;
    async fn shutdown(&self) -> Result<(), IrisFfiError>;
    /// Availability query (projection of `is_available()` push channel).
    fn is_available(&self) -> bool;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Rust-side simulated adapter for the async path (Kotlin twin over FFI).
    /// Tracks subscription/publish state so the spike can assert that async
    /// foreign-trait calls genuinely reached the adapter through the engine's
    /// explicit tokio handle.
    #[derive(Default)]
    pub(crate) struct SimAware {
        pub available: bool,
        pub started: std::sync::atomic::AtomicBool,
        pub subscribed: std::sync::atomic::AtomicBool,
        pub published: std::sync::atomic::AtomicBool,
        pub ndp_sent: std::sync::atomic::AtomicUsize,
    }

    impl SimAware {
        pub fn subscribed(&self) -> bool {
            self.subscribed.load(std::sync::atomic::Ordering::SeqCst)
        }
        pub fn published(&self) -> bool {
            self.published.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl FfiWifiAwareAdapter for SimAware {
        async fn start(&self) -> Result<(), IrisFfiError> {
            self.started
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn subscribe(&self) -> Result<(), IrisFfiError> {
            self.subscribed
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn unsubscribe(&self) -> Result<(), IrisFfiError> {
            self.subscribed
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn publish(&self, _c: FfiPublishConfig) -> Result<(), IrisFfiError> {
            self.published
                .store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn unpublish(&self) -> Result<(), IrisFfiError> {
            self.published
                .store(false, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn matches(&self) -> Result<Vec<FfiPeerDiscovery>, IrisFfiError> {
            Ok(vec![FfiPeerDiscovery {
                peer_handle: 3,
                service_specific_info: vec![0u8; 22],
                rssi: -70,
            }])
        }
        async fn open_ndp(&self, _p: u64) -> Result<u64, IrisFfiError> {
            Ok(5)
        }
        async fn close_ndp(&self, _n: u64) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn ndp_send(&self, _n: u64, p: Vec<u8>) -> Result<(), IrisFfiError> {
            assert_eq!(p, b"ping".to_vec());
            self.ndp_sent
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn incoming_ndp(&self) -> Result<Vec<FfiIncomingNdpData>, IrisFfiError> {
            Ok(Vec::new())
        }
        async fn shutdown(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        fn is_available(&self) -> bool {
            self.available
        }
    }
}
