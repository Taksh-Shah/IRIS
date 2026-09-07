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
pub mod relay_protocol;
pub mod satellite;
pub mod simulated;
pub mod tls;
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
///
/// The inner field is `pub(crate)` (MG-21): external callers must use
/// `load()` / `store()`, preventing bypass of future transition guards.
#[derive(Debug, Default)]
pub struct AtomicState(pub(crate) AtomicU8);

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
///
/// Variants carry the figures required for the user-consent cost warning
/// (TRANSPORT_ABSTRACTION.md:462 — MG-19/MG-9).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransportCostClass {
    /// BLE, Wi-Fi, LoRa — no per-message cost.
    Free,
    /// Cellular / relay metered traffic.
    Metered { cost_per_kb_inr: f64 },
    /// Satellite — only for P0–P2, warned before send.
    Expensive {
        /// Estimated per-message charge in INR.
        cost_per_message_inr: f64,
        /// Minimum charge per billing event in INR.
        minimum_cost_inr: f64,
    },
}

impl TransportCostClass {
    /// True if this class imposes real per-message charges (gates the
    /// user-consent warning required by TRANSPORT_ABSTRACTION.md:462).
    pub fn is_expensive(&self) -> bool {
        matches!(self, TransportCostClass::Expensive { .. })
    }
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

/// MG-17: EWMA goodput tracker.
///
/// Records actual bytes sent and maintains an exponential-weighted moving
/// average of observed bandwidth. All transport `send()` implementations
/// call `record_send(bytes)` so `cost_snapshot()` can return a live estimate
/// rather than the compile-time constant that was there before.
pub struct EwmaGoodput {
    state: std::sync::Mutex<EwmaState>,
}

struct EwmaState {
    /// EWMA of goodput in bytes/s. Starts at `None` until the first send.
    ewma_bps: Option<f64>,
    last_send: std::time::Instant,
}

impl EwmaGoodput {
    /// α = 0.2 gives ~5-sample memory — quick to adapt, not too noisy.
    const ALPHA: f64 = 0.2;

    pub fn new() -> Self {
        EwmaGoodput {
            state: std::sync::Mutex::new(EwmaState {
                ewma_bps: None,
                last_send: std::time::Instant::now(),
            }),
        }
    }

    /// Call after each successful send. `bytes` is the frame byte count.
    pub fn record_send(&self, bytes: usize) {
        let Ok(mut s) = self.state.lock() else { return };
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(s.last_send).as_secs_f64().max(1e-6);
        let sample_bps = (bytes as f64 * 8.0) / elapsed; // bits/s for consistency with bps naming
        s.ewma_bps = Some(match s.ewma_bps {
            None => sample_bps,
            Some(prev) => Self::ALPHA * sample_bps + (1.0 - Self::ALPHA) * prev,
        });
        s.last_send = now;
    }

    /// Returns the current EWMA goodput in bits/s, or `fallback_bps` if no
    /// sends have been recorded yet.
    pub fn bandwidth_bps(&self, fallback_bps: u64) -> u64 {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.ewma_bps)
            .map(|v| v as u64)
            .unwrap_or(fallback_bps)
    }
}

impl Default for EwmaGoodput {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// Emitted after the recovery-backoff ladder exhausts all attempts
    /// (TRANSPORT_ABSTRACTION.md:513-519 — MG-12). The supervision task
    /// (to be wired in the composition root) drives the ladder; this variant
    /// is the event it emits on permanent failure.
    TransportPermanentFailure {
        transport_id: TransportId,
        /// Number of recovery attempts made before giving up.
        attempts: u32,
    },
}

/// Hardware sharing groups — only one transport per conflicting group may be
/// CONNECTED at a time (TRANSPORT_ABSTRACTION.md §Multi-Transport Concurrency).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
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
                    // MG-22: separate state-stream lag (topology loss) from
                    // message-stream lag (data loss) — state lag is more severe
                    // because the routing engine may never recover the missed edge.
                    let is_state = label.ends_with(".state") || label.ends_with(".availability");
                    if is_state {
                        STATE_LAGGED_TOTAL.fetch_add(skipped, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        INBOUND_LAGGED_TOTAL
                            .fetch_add(skipped, std::sync::atomic::Ordering::Relaxed);
                    }
                    tracing::warn!(
                        event = if is_state {
                            "transport.state_lagged"
                        } else {
                            "transport.inbound_lagged"
                        },
                        transport = label,
                        skipped,
                        "transport buffer overran; {} were dropped",
                        if is_state {
                            "state transitions"
                        } else {
                            "messages"
                        }
                    );
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    }))
}

/// Count of inbound data messages dropped because a receive buffer overran.
static INBOUND_LAGGED_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Count of transport state-change events dropped due to buffer overrun (MG-22).
static STATE_LAGGED_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Total inbound data messages lost to receive-buffer overrun since process start.
pub fn inbound_lagged_total() -> u64 {
    INBOUND_LAGGED_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Total transport state-change events dropped due to buffer overrun since process start.
/// A non-zero value means the routing engine may have missed topology changes.
pub fn state_lagged_total() -> u64 {
    STATE_LAGGED_TOTAL.load(std::sync::atomic::Ordering::Relaxed)
}

/// Minutes since UNIX epoch mod 2^16 — the shared freshness clock for all
/// discovery beacons (GAP-6). BLE, Wi-Fi Aware, and Wi-Fi Direct all define
/// `freshness_minutes` as "minutes since epoch mod 2^16"; calling this helper
/// rather than a local `Instant::elapsed()` keeps all three transports
/// consistent on the same wall-clock origin.
pub fn freshness_minutes_now() -> u16 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 60) as u16
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
    ///
    /// LoRa and Internet wire this into a stored priority field; all other
    /// transports fall through to this default no-op until TX-power tuning is
    /// implemented (MG-23).
    #[allow(unused_variables)]
    fn set_send_priority_hint(&self, priority: MessagePriority) {}

    /// Graceful shutdown.
    async fn shutdown(&self) -> Result<(), TransportError>;

    /// HV-97: drop every live link on this transport WITHOUT shutting the
    /// transport down — advertising/scanning stay up so discovery re-forms the
    /// links. Used by the bench harness to deterministically exercise the
    /// reconnect path (HV-15) and by the app's RETRY affordance later
    /// (HV-31/HV-33). Default no-op for transports where "drop the link" has no
    /// meaning distinct from `shutdown`.
    async fn drop_all_links(&self) {}

    /// HV-99: a cheap liveness/health tick the discovery loop runs on a short
    /// fixed interval (independent of the scan cadence), so a platform-surfaced
    /// event — a Bluetooth off→on toggle, an advertiser that died — is acted on
    /// in a few seconds even while the mesh is linked and scanning slowly.
    /// Must NOT start a scan or block. Default no-op.
    async fn poll_health(&self) {}

    /// HV-21: tell this transport the local device's Wi-Fi Direct (P2P) MAC,
    /// once the platform has told the app what it is. Only the BLE transport
    /// acts on this (it re-publishes its discovery beacon with the address
    /// included, so a peer can match a plain `discoverPeers()` result back to
    /// an already-BLE-known IRIS peer without depending on Wi-Fi Direct's own
    /// DNS-SD service discovery — see `ble_advert.rs`'s DEC-BLE-0008 doc
    /// comment for why). Default no-op for every other transport.
    async fn set_local_wifi_direct_mac(&self, _mac: [u8; 6]) {}

    /// HV-27: how healthy *this transport's* link to `peer` currently is, from
    /// per-link activity (recent successful traffic = healthy; long quiet =
    /// suspect). `None` when the transport has no per-link signal — discovery
    /// then assumes `Good`, the historical behaviour. Feeds the `NeighborTable`
    /// and `/diag`, and (HV-48) transport selection.
    fn link_quality(&self, _peer: &crate::message::PeerId) -> Option<crate::message::LinkQuality> {
        None
    }
}
