//! Transport abstraction layer — TRANSPORT-001.
//!
//! Implements the `Transport` trait, capability model, state machine, cost
//! models, and the `TransportManager` registry, per
//! `docs/transports/TRANSPORT_ABSTRACTION.md`.
//!
//! Concrete transports in this crate:
//! - [`internet::InternetTransport`] — INTERNET-001 (TCP-framing path, testable)
//! - [`simulated::SimulatedTransport`] — deterministic in-memory channel for tests
//! - [`ble::BleTransport`] — BLE-001 (platform adapter injected via FFI)
//! - [`ble_att`] — BLE-001 ATT segmentation/reassembly (AC-2/AC-6)
//! - [`ble_advert`] — BLE-001 discovery beacon build/parse (AC-3/AC-6)
//! - [`wifiaware::WifiAwareTransport`] — WIAW-001 (Wi-Fi Aware/NAN, adapter injected)
//! - [`wifiaware_beacon`] — WIAW-001 discovery beacon build/parse
//! - [`wifi_direct_serv`] — WIFIDIRECT-001 DNS-SD (Bonjour) TXT-record build/parse
//! - [`wifi_direct::WifiDirectTransport`] — WIFIDIRECT-001 (Wi-Fi Direct P2P, adapter injected)

pub mod ble;
pub mod ble_advert;
pub mod ble_att;
pub mod internet;
pub mod lora;
pub mod manager;
pub mod satellite;
pub mod simulated;
pub mod wifi_direct;
pub mod wifi_direct_serv;
pub mod wifiaware;
pub mod wifiaware_beacon;

pub use manager::{RegistrationError, TransportManager};

use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};

use async_trait::async_trait;
use futures_util::stream::Stream;

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::TransportError;

/// Transport identifier (e.g. `"ble-android"`, `"internet-0"`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct TransportId(pub String);

impl TransportId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TransportId {
    fn from(s: &str) -> Self {
        TransportId(s.to_string())
    }
}

impl fmt::Display for TransportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transport lifecycle state — see state machine in
/// `docs/transports/TRANSPORT_ABSTRACTION.md`.
///
/// Discriminants are ordered so `state() >= TransportState::Available`
/// filters usable transports, and `Connected` sorts above `Degraded`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum TransportState {
    /// Hardware off / permission denied / hardware not present.
    Unavailable = 0,
    /// Ready, can accept connections, no active link.
    Available = 1,
    /// Connection attempt in progress.
    Connecting = 2,
    /// Link exists but quality below threshold.
    Degraded = 3,
    /// Active link with at least one peer.
    Connected = 4,
}

impl TransportState {
    /// Raw discriminant.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// From raw discriminant.
    pub fn from_u8(v: u8) -> Option<TransportState> {
        Some(match v {
            0 => TransportState::Unavailable,
            1 => TransportState::Available,
            2 => TransportState::Connecting,
            3 => TransportState::Degraded,
            4 => TransportState::Connected,
            _ => return None,
        })
    }

    /// A thread-safe in-memory representation used by impls.
    pub fn to_atomic(self) -> AtomicState {
        AtomicState(AtomicU8::new(self.as_u8()))
    }
}

/// Wrapper around `AtomicU8` for lock-free state updates.
#[derive(Debug, Default)]
pub struct AtomicState(pub AtomicU8);

impl AtomicState {
    pub fn load(&self) -> TransportState {
        TransportState::from_u8(self.0.load(Ordering::Acquire))
            .unwrap_or(TransportState::Unavailable)
    }

    pub fn store(&self, state: TransportState) {
        self.0.store(state.as_u8(), Ordering::Release);
    }
}

impl fmt::Display for TransportState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TransportState::Unavailable => "UNAVAILABLE",
                TransportState::Available => "AVAILABLE",
                TransportState::Connecting => "CONNECTING",
                TransportState::Degraded => "DEGRADED",
                TransportState::Connected => "CONNECTED",
            }
        )
    }
}

/// State-change event emitted by transports.
#[derive(Debug, Clone)]
pub struct TransportStateEvent {
    pub transport_id: TransportId,
    pub new_state: TransportState,
}

/// Static capabilities of a transport — reported once at registration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransportCapabilities {
    /// Bytes per `send()` call.
    pub max_message_size: usize,
    pub supports_broadcast: bool,
    pub supports_unicast: bool,
    pub supports_multicast: bool,
    /// Min / Max / Typical range in meters.
    pub range_m_min: u32,
    pub range_m_max: u32,
    pub range_m_typical: u32,
    pub typical_throughput_bps: u64,
    pub typical_latency_ms: u32,
    /// False = P2P capable (no infrastructure).
    pub requires_infrastructure: bool,
    pub supports_background_android: bool,
    pub supports_background_ios: bool,
    pub requires_special_hardware: bool,
    pub cost_class: TransportCostClass,
    /// Regulatory band hint (e.g. LoRa IN865), when applicable.
    pub regulatory_band: Option<String>,
    /// Hardware sharing group: at most one transport per non-None group may be
    /// selected for a single send (TRANSPORT_ABSTRACTION.md §Multi-Transport Concurrency).
    #[serde(default)]
    pub conflict_group: RadioConflictGroup,
}

impl TransportCapabilities {
    /// Maximum message bytes this transport can carry per frame.
    pub fn max_message_size(&self) -> usize {
        self.max_message_size
    }
}

/// Monetary cost classification per `docs/transports/TRANSPORT_ABSTRACTION.md`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransportCostClass {
    /// BLE, Wi-Fi, LoRa — no per-message cost.
    Free,
    /// Cellular / relay metered traffic.
    Metered,
    /// Satellite — only for P0/P1, warned before send.
    Expensive,
}

/// Dynamic cost snapshot used for transport selection.
#[derive(Debug, Clone)]
pub struct TransportCost {
    /// Estimated current draw in milliamps.
    pub estimated_battery_ma: f32,
    /// INR per kilobyte (0 for free transports).
    pub monetary_cost_per_kb: f64,
    /// Current estimated available bandwidth.
    pub bandwidth_available_bps: u64,
    /// 0.0 = no congestion, 1.0 = fully congested.
    pub congestion_level: f32,
}

/// Battery draw model per transport (from TRANSPORT_ABSTRACTION.md).
#[derive(Debug, Clone, Copy)]
pub struct BatteryCostModel {
    pub scan_ma: f32,
    pub advertise_ma: f32,
    pub connected_idle_ma: f32,
    pub tx_ma_per_kbps: f32,
    pub rx_ma_per_kbps: f32,
}

/// BLE current draw (TRANSPORT_ABSTRACTION.md §Battery Cost Model).
pub const BLE_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 8.0,
    advertise_ma: 5.0,
    connected_idle_ma: 3.0,
    tx_ma_per_kbps: 0.02,
    rx_ma_per_kbps: 0.01,
};

/// Wi-Fi Aware current draw (TRANSPORT_ABSTRACTION.md).
pub const WIFI_AWARE_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 25.0,
    advertise_ma: 15.0,
    connected_idle_ma: 12.0,
    tx_ma_per_kbps: 0.005,
    rx_ma_per_kbps: 0.003,
};

/// Wi-Fi Direct current draw (WIFI_DIRECT.md §Battery Impact). High-battery
/// data plane: continuous `discoverPeers()` drains ~80–150 mA additional,
/// sustained transfer ~80–150 mA, GO keeps transmitting beacons. Design
/// estimates only — physical-device battery BENCH is gated (AC-14/BLK-0005).
pub const WIFI_DIRECT_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 80.0,
    advertise_ma: 50.0,
    connected_idle_ma: 40.0,
    tx_ma_per_kbps: 0.01,
    rx_ma_per_kbps: 0.008,
};

/// LoRa current draw (TRANSPORT_ABSTRACTION.md).
pub const LORA_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 1.5,
    advertise_ma: 0.5,
    connected_idle_ma: 1.2,
    tx_ma_per_kbps: 10.0,
    rx_ma_per_kbps: 1.5,
};

/// Events delivered to the routing engine (`docs/routing`).
#[derive(Debug, Clone)]
pub enum TopologyEvent {
    TransportStateChanged {
        transport_id: TransportId,
        new_state: TransportState,
    },
    PeerDiscovered {
        peer: PeerInfo,
        via_transport: TransportId,
        link_quality: crate::message::LinkQuality,
    },
    PeerLost {
        peer_id: PeerId,
        via_transport: TransportId,
    },
    LinkQualityUpdated {
        peer_id: PeerId,
        transport_id: TransportId,
        new_quality: crate::message::LinkQuality,
    },
    GatewayChanged {
        gateway_id: PeerId,
        gateway_type: String,
        available: bool,
    },
}

/// Hardware sharing groups — only one transport per conflicting group may be
/// CONNECTED at a time (TRANSPORT_ABSTRACTION.md §Multi-Transport Concurrency).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize)]
pub enum RadioConflictGroup {
    Bluetooth24GHz,
    WiFi24GHz,
    WiFi5GHz,
    SubGHz,
    Cellular,
    #[default]
    None,
}

/// Adapt a `tokio::sync::broadcast` receiver into a `Stream` of items.
///
/// `broadcast::Receiver` implements `Stream` with `Item = Result<T, RecvError>`;
/// this helper strips errors, reporting `Lagged` and terminating on `Closed`.
///
/// A `Lagged` means inbound messages were dropped because the consumer fell
/// behind the channel. This used to `continue` silently, discarding the skipped
/// count — so a burst could drop traffic, including P0 SOS, with no metric, no
/// log and no retransmit trigger. In a disruption-tolerant mesh that is exactly
/// the failure that must never be invisible.
///
/// `label` is a short `"transport.stream_type"` tag (e.g. `"ble.messages"`)
/// included in the warn event so lagged state streams are distinguishable from
/// lagged message streams in logs (SYS-3 / MG-22).
pub(crate) fn broadcast_stream<T: Clone + Send + 'static>(
    rx: tokio::sync::broadcast::Receiver<T>,
    label: &'static str,
) -> Pin<Box<dyn Stream<Item = T> + Send>> {
    Box::pin(futures_util::stream::unfold(rx, move |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(item) => return Some((item, rx)),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    INBOUND_LAGGED_TOTAL.fetch_add(skipped, std::sync::atomic::Ordering::Relaxed);
                    tracing::warn!(
                        event = "transport.inbound_lagged",
                        transport = label,
                        skipped,
                        "inbound transport buffer overran; messages were dropped"
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    }))
}

/// Count of inbound messages dropped because a receive buffer overran.
static INBOUND_LAGGED_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Total inbound messages lost to receive-buffer overrun since process start.
pub fn inbound_lagged_total() -> u64 {
    INBOUND_LAGGED_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// The core transport abstraction. Every transport must implement this trait.
///
/// All methods are `&self` so a transport can be shared as `Arc<dyn Transport>`
/// and registered/deregistered at runtime (e.g. LoRa dongle hot-plug).
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Unique instance id (e.g. `"ble-android"`).
    fn transport_id(&self) -> &TransportId;

    /// Human-readable name for logging and UI.
    fn display_name(&self) -> &str;

    /// Static capabilities (reported once at registration).
    fn capabilities(&self) -> &TransportCapabilities;

    /// Current dynamic state.
    fn state(&self) -> TransportState;

    /// Stream of state-change events.
    fn state_stream(&self) -> Pin<Box<dyn Stream<Item = TransportStateEvent> + Send>>;

    /// Discover/scan for nearby peers.
    async fn discover_peers(
        &self,
        config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError>;

    /// Stop a running discovery scan.
    async fn stop_discovery(&self) -> Result<(), TransportError>;

    /// Advertise this node's presence.
    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError>;

    /// Stop advertising.
    async fn stop_advertising(&self) -> Result<(), TransportError>;

    /// Open a connection to a peer.
    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError>;

    /// Send a message to a peer (may internally manage connection lifecycle).
    async fn send(
        &self,
        peer: &PeerId,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError>;

    /// Stream of incoming messages from any peer.
    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>;

    /// Current cost snapshot (selection input).
    fn cost_snapshot(&self) -> TransportCost;

    /// Hint about the priority of the next send (may adjust power settings).
    fn set_send_priority_hint(&self, priority: MessagePriority);

    /// Graceful shutdown.
    async fn shutdown(&self) -> Result<(), TransportError>;
}
