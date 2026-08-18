//! `WifiDirectAdapter` — UniFFI foreign-trait projection (async, by-value).
//!
//! Mirrors `iris_core::transport::wifi_direct::WifiDirectAdapter` (20-op) with
//! owned params and Result returns (the foreign-trait contract, D-3). AC-4
//! lands the full adapter surface (replacing the 5-op spike subset): the
//! bridge (`crate::bridge`) implements the core trait over this FFI trait, so
//! the `WifiDirectTransport` can drive the Kotlin adapter end-to-end.

use async_trait::async_trait;

use crate::ffi::error::IrisFfiError;

/// Operating band hint (`wifi_direct::OperatingBand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiOperatingBand {
    Auto,
    Ghz24,
    Ghz5,
    Ghz6,
}

/// Owned group-formation config (`wifi_direct::GroupConfig`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiGroupConfig {
    /// GO election intent (0 = never GO, 15 = always GO).
    pub go_intent: u8,
    /// Autonomous persistent GO until `remove_group`.
    pub persistent: bool,
    pub band: FfiOperatingBand,
}

/// Owned group-membership snapshot (`wifi_direct::GroupInfo`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiGroupInfo {
    pub group_id: u64,
    /// The GO's peer handle.
    pub go: u64,
    /// GO-supplied endpoint address (adapter-provided, never hard-coded, G-WD-2).
    pub go_addr: Option<String>,
    pub clients: Vec<u64>,
}

/// One discovery match: the peer's handle + its DNS-SD TXT-record beacon.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiDirectPeerDiscovery {
    pub peer_handle: u64,
    /// Matched p2p service name (must equal `_iris._tcp`).
    pub service_name: String,
    /// TXT-record value bytes (the IRIS beacon, 22-byte prefix).
    pub txt_record: Vec<u8>,
}

/// A frame received in a P2P group (`wifi_direct::IncomingWifiDirectData`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiIncomingWifiDirectData {
    /// Best-effort candidate sender identity (TXT-record short id when known).
    pub sender: Option<Vec<u8>>,
    pub payload: Vec<u8>,
}

/// Foreign-trait projection of `wifi_direct::WifiDirectAdapter` (full 20-op
/// surface, AC-4). `#[async_trait]` is REQUIRED (see wifi_aware_adapter.rs
/// rationale). `availability_stream()` stays pull-only via `is_available()`
/// (mirrors the WA projection decision, proj-WA-1).
#[uniffi::export(with_foreign)]
#[async_trait]
pub trait FfiWifiDirectAdapter: Send + Sync + 'static {
    /// Power on the Wi-Fi Direct subsystem. Idempotent.
    async fn start(&self) -> Result<(), IrisFfiError>;

    /// Register the DNS-SD (Bonjour) service advertisement (`addLocalService`).
    async fn start_dns_sd(&self) -> Result<(), IrisFfiError>;

    /// Withdraw the DNS-SD service advertisement. Idempotent.
    async fn stop_dns_sd(&self) -> Result<(), IrisFfiError>;

    /// Begin a discovery find window (`discoverServices`, 120-s framework).
    async fn start_discovery(&self) -> Result<(), IrisFfiError>;

    /// Stop the discovery find window (`WIFI_P2P_DISCOVERY_CHANGED_ACTION`).
    async fn stop_discovery(&self) -> Result<(), IrisFfiError>;

    /// Drain current DNS-SD service matches (candidates only).
    async fn matches(&self) -> Result<Vec<FfiDirectPeerDiscovery>, IrisFfiError>;

    /// Form an autonomous persistent Group Owner (RES-0021 Q5).
    async fn create_group(&self, config: FfiGroupConfig) -> Result<FfiGroupInfo, IrisFfiError>;

    /// Join an existing peer's group as a client (`connect`, GO intent 0).
    async fn join_group(
        &self,
        go: u64,
        config: FfiGroupConfig,
    ) -> Result<FfiGroupInfo, IrisFfiError>;

    /// GO invites a discovered peer into the current group (`p2p_invite`).
    async fn add_client(&self, client: u64) -> Result<(), IrisFfiError>;

    /// Tear down the current group (`removeGroup`). Idempotent.
    async fn remove_group(&self) -> Result<(), IrisFfiError>;

    /// Snapshot of the current group (if any).
    async fn group_info(&self) -> Option<FfiGroupInfo>;

    /// GO-supplied endpoint address for the current group (adapter-provided).
    fn go_addr(&self) -> Option<String>;

    /// Band hint for group formation (`setGroupOperatingBand`, API 29).
    async fn set_operating_band(&self, band: FfiOperatingBand) -> Result<(), IrisFfiError>;

    /// Send a frame to a peer inside the current group (TCP-over-GO).
    async fn p2p_send(&self, peer: u64, payload: Vec<u8>) -> Result<(), IrisFfiError>;

    /// Drain inbound group frames.
    async fn incoming(&self) -> Result<Vec<FfiIncomingWifiDirectData>, IrisFfiError>;

    /// Tear down the subsystem (unregister DNS-SD, remove group, detach).
    async fn shutdown(&self) -> Result<(), IrisFfiError>;

    /// Current availability (pull-only projection of the push channel).
    fn is_available(&self) -> bool;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Rust-side simulated adapter for the async path (Kotlin twin over FFI).
    pub(crate) struct SimDirect;

    #[async_trait]
    impl FfiWifiDirectAdapter for SimDirect {
        async fn start(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn start_dns_sd(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn stop_dns_sd(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn start_discovery(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn stop_discovery(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn matches(&self) -> Result<Vec<FfiDirectPeerDiscovery>, IrisFfiError> {
            Ok(Vec::new())
        }
        async fn create_group(&self, _c: FfiGroupConfig) -> Result<FfiGroupInfo, IrisFfiError> {
            Ok(FfiGroupInfo {
                group_id: 1,
                go: 1,
                go_addr: Some("192.168.49.1".into()),
                clients: Vec::new(),
            })
        }
        async fn join_group(
            &self,
            go: u64,
            _c: FfiGroupConfig,
        ) -> Result<FfiGroupInfo, IrisFfiError> {
            Ok(FfiGroupInfo {
                group_id: 2,
                go,
                go_addr: None,
                clients: Vec::new(),
            })
        }
        async fn add_client(&self, _c: u64) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn remove_group(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn group_info(&self) -> Option<FfiGroupInfo> {
            None
        }
        fn go_addr(&self) -> Option<String> {
            None
        }
        async fn set_operating_band(&self, _b: FfiOperatingBand) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn p2p_send(&self, _p: u64, _payload: Vec<u8>) -> Result<(), IrisFfiError> {
            Ok(())
        }
        async fn incoming(&self) -> Result<Vec<FfiIncomingWifiDirectData>, IrisFfiError> {
            Ok(Vec::new())
        }
        async fn shutdown(&self) -> Result<(), IrisFfiError> {
            Ok(())
        }
        fn is_available(&self) -> bool {
            true
        }
    }
}
