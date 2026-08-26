//! `TransportManager` — transport registry and selection.
//!
//! Implements `docs/transports/TRANSPORT_ABSTRACTION.md` §Transport
//! Registration and §Transport Selection Logic.

use std::collections::HashMap;
use std::sync::Arc;

use futures_util::StreamExt;
use tokio::sync::{broadcast, RwLock};

use crate::message::{MessagePriority, PeerId};
use crate::transport::{
    TopologyEvent, Transport, TransportCapabilities, TransportCost, TransportId, TransportState,
};
use crate::TransportError;

/// Error from registering/deregistering a transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// A transport with the same id is already registered.
    DuplicateId,
    /// Transport was not registered.
    NotFound,
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::DuplicateId => write!(f, "transport id already registered"),
            RegistrationError::NotFound => write!(f, "transport not registered"),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// A transport ranked for a given send request.
#[derive(Debug, Clone)]
pub struct RankedTransport {
    pub transport_id: TransportId,
    pub score: f32,
}

/// Selection request produced by the routing engine (ROUTE-001/002).
#[derive(Debug, Clone)]
pub struct TransportSelectionRequest {
    /// None = broadcast.
    pub target_peer: Option<PeerId>,
    pub message_size: usize,
    pub priority: MessagePriority,
    pub max_latency_ms: Option<u32>,
    pub prefer_low_cost: bool,
    /// True = select multiple transports (multipath for P0/P1).
    pub multipath: bool,
    /// True when the caller will fragment an oversized payload (MSG-001 R8).
    /// Then `max_message_size` no longer gates eligibility — the engine splits
    /// the ADU. Prevents >MTU bulk (P4–P7) from being unroutable over BLE/etc.
    pub fragmentable: bool,
}

/// Registry and selector for all transports.
pub struct TransportManager {
    registry: RwLock<HashMap<TransportId, Arc<dyn Transport>>>,
    /// broadcast of topology events for the routing engine.
    topology_tx: broadcast::Sender<TopologyEvent>,
}

impl TransportManager {
    pub fn new() -> Self {
        let (topology_tx, _) = broadcast::channel(256);
        TransportManager {
            registry: RwLock::new(HashMap::new()),
            topology_tx,
        }
    }

    /// Subscribe to topology events (routing engine).
    pub fn topology_events(&self) -> broadcast::Receiver<TopologyEvent> {
        self.topology_tx.subscribe()
    }

    /// Register a transport. Runtime-safe (LoRa dongle hot-plug).
    pub async fn register(&self, transport: Arc<dyn Transport>) -> Result<(), RegistrationError> {
        let id = transport.transport_id().clone();

        // Forward state changes to topology subscribers before insertion.
        let mut state_stream = transport.state_stream();
        let tx = self.topology_tx.clone();
        let forward_id = id.clone();
        tokio::spawn(async move {
            while let Some(event) = state_stream.next().await {
                tx.send(TopologyEvent::TransportStateChanged {
                    transport_id: forward_id.clone(),
                    new_state: event.new_state,
                }).ok();
            }
        });

        let mut registry = self.registry.write().await;
        if registry.contains_key(&id) {
            return Err(RegistrationError::DuplicateId);
        }
        registry.insert(id.clone(), transport);
        Ok(())
    }

    /// Deregister a transport (e.g. LoRa dongle unplugged).
    pub async fn deregister(&self, id: &TransportId) -> Result<(), RegistrationError> {
        let mut registry = self.registry.write().await;
        if let Some(transport) = registry.remove(id) {
            transport.shutdown().await.ok();
            Ok(())
        } else {
            Err(RegistrationError::NotFound)
        }
    }

    /// Look up a transport by id.
    pub async fn get(&self, id: &TransportId) -> Option<Arc<dyn Transport>> {
        self.registry.read().await.get(id).cloned()
    }

    /// List registered transport ids.
    pub async fn list(&self) -> Vec<TransportId> {
        self.registry.read().await.keys().cloned().collect()
    }

    /// Select transports for a send request, ranked by score.
    ///
    /// Rules (TRANSPORT_ABSTRACTION.md §Selection):
    /// 1. state >= Available
    /// 2. max_message_size >= message_size
    /// 3. score by state, latency, cost, bandwidth, congestion
    /// 4. P0/P1 multipath → all viable; otherwise single best
    pub async fn select_transports(&self, req: &TransportSelectionRequest) -> Vec<RankedTransport> {
        let registry = self.registry.read().await;

        let mut candidates: Vec<RankedTransport> = registry
            .values()
            .filter(|t| t.state() >= TransportState::Available)
            .filter(|t| req.fragmentable || t.capabilities().max_message_size >= req.message_size)
            .map(|t| {
                let score = score_transport(t.as_ref(), req);
                RankedTransport {
                    transport_id: t.transport_id().clone(),
                    score,
                }
            })
            .collect();

        // total_cmp never panics and rejects NaN deterministically
        // (RED-0003-01 replaces partial_cmp().unwrap() panic-on-NaN).
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));

        if req.multipath && req.priority.is_emergency() {
            candidates.retain(|c| c.score > 0.0);
        } else {
            candidates.truncate(1);
        }

        candidates
    }
}

impl Default for TransportManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Score a transport for a request (pure, unit-testable).
pub fn score_transport(t: &dyn Transport, req: &TransportSelectionRequest) -> f32 {
    let caps = t.capabilities();
    let cost = t.cost_snapshot();
    let state = t.state();

    let mut score = match state {
        TransportState::Connected => 40.0,
        TransportState::Available => 20.0,
        TransportState::Degraded => 5.0,
        _ => -1000.0, // eliminates this transport
    };

    // Latency bonus for emergency messages.
    if req.priority.is_emergency() {
        score += 100.0 / (caps.typical_latency_ms as f32 + 1.0);
    }

    // Cost penalty for metered/expensive transports on non-emergency.
    if req.prefer_low_cost || req.priority >= MessagePriority::P4 {
        score -= cost.monetary_cost_per_kb as f32 * 50.0;
        score -= cost.estimated_battery_ma * 0.5;
    }

    // Bandwidth bonus if message is large. Guard against 0 bps → -inf,
    // which corrupts ranking (RED-0003-01 score NaN/-inf).
    if req.message_size > 10_000 {
        let bw = cost.bandwidth_available_bps.max(1) as f32;
        score += bw.log2() * 5.0;
    }

    // Congestion penalty.
    score -= cost.congestion_level * 30.0;

    score
}

/// Build a cost snapshot from a battery model + live inputs.
pub fn cost_from_model(
    model: crate::transport::BatteryCostModel,
    tx_kbps: f32,
    rx_kbps: f32,
    bandwidth_available_bps: u64,
    congestion_level: f32,
    monetary_cost_per_kb: f64,
) -> TransportCost {
    TransportCost {
        estimated_battery_ma: model.connected_idle_ma
            + model.tx_ma_per_kbps * tx_kbps
            + model.rx_ma_per_kbps * rx_kbps,
        monetary_cost_per_kb,
        bandwidth_available_bps,
        congestion_level,
    }
}

/// A no-op transport that reports given caps (used in manager tests).
///
/// Gated behind `#[cfg(any(test, feature = "test-support"))]` (SYS-6):
/// `send()` returns `Ok` while discarding every message — registering it
/// in a production `TransportManager` would produce a node that reports
/// 100% send success while delivering nothing.
#[cfg(any(test, feature = "test-support"))]
pub struct StubTransport {
    id: TransportId,
    caps: TransportCapabilities,
    state: TransportState,
    cost: TransportCost,
}

#[cfg(any(test, feature = "test-support"))]
impl StubTransport {
    pub fn new(
        id: &str,
        caps: TransportCapabilities,
        state: TransportState,
        cost: TransportCost,
    ) -> Self {
        StubTransport {
            id: TransportId::from(id),
            caps,
            state,
            cost,
        }
    }
}

#[async_trait::async_trait]
#[cfg(any(test, feature = "test-support"))]
impl Transport for StubTransport {
    fn transport_id(&self) -> &TransportId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.id.0
    }
    fn capabilities(&self) -> &TransportCapabilities {
        &self.caps
    }
    fn state(&self) -> TransportState {
        self.state
    }
    fn state_stream(
        &self,
    ) -> std::pin::Pin<
        Box<dyn futures_util::stream::Stream<Item = crate::transport::TransportStateEvent> + Send>,
    > {
        Box::pin(futures_util::stream::empty())
    }
    async fn discover_peers(
        &self,
        _config: crate::message::DiscoveryConfig,
    ) -> Result<
        std::pin::Pin<
            Box<dyn futures_util::stream::Stream<Item = crate::message::PeerInfo> + Send>,
        >,
        TransportError,
    > {
        Ok(Box::pin(futures_util::stream::empty()))
    }
    async fn stop_discovery(&self) -> Result<(), TransportError> {
        Ok(())
    }
    async fn start_advertising(
        &self,
        _info: crate::message::NodeAdvertisement,
    ) -> Result<(), TransportError> {
        Ok(())
    }
    async fn stop_advertising(&self) -> Result<(), TransportError> {
        Ok(())
    }
    async fn connect(
        &self,
        _peer: &crate::message::PeerInfo,
    ) -> Result<crate::message::TransportLink, TransportError> {
        Ok(crate::message::TransportLink {
            peer_id: _peer.peer_id,
            transport_id: self.id.0.clone(),
            established_at: std::time::Instant::now(),
        })
    }
    async fn send(
        &self,
        _peer: &PeerId,
        message: &crate::message::SerializedMessage,
    ) -> Result<crate::message::SendReceipt, TransportError> {
        Ok(crate::message::SendReceipt {
            peer_id: *_peer,
            bytes_sent: message.payload.len(),
            sent_at: std::time::Instant::now(),
        })
    }
    fn incoming_messages(
        &self,
    ) -> std::pin::Pin<
        Box<dyn futures_util::stream::Stream<Item = crate::message::IncomingMessage> + Send>,
    > {
        Box::pin(futures_util::stream::empty())
    }
    fn cost_snapshot(&self) -> TransportCost {
        self.cost.clone()
    }
    fn set_send_priority_hint(&self, _priority: MessagePriority) {}
    async fn shutdown(&self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{RadioConflictGroup, TransportCapabilities, TransportCostClass};

    fn caps(max_size: usize, latency: u32, bw: u64) -> TransportCapabilities {
        TransportCapabilities {
            max_message_size: max_size,
            supports_broadcast: false,
            supports_unicast: true,
            supports_multicast: false,
            range_m_min: 0,
            range_m_max: 10_000,
            range_m_typical: 1_000,
            typical_throughput_bps: bw,
            typical_latency_ms: latency,
            requires_infrastructure: true,
            supports_background_android: true,
            supports_background_ios: true,
            requires_special_hardware: false,
            cost_class: TransportCostClass::Free,
            regulatory_band: None,
        }
    }

    fn cost(ma: f32, inr: f64, bw: u64, cong: f32) -> TransportCost {
        TransportCost {
            estimated_battery_ma: ma,
            monetary_cost_per_kb: inr,
            bandwidth_available_bps: bw,
            congestion_level: cong,
        }
    }

    fn free_caps() -> TransportCapabilities {
        caps(65_000, 5, 100_000_000)
    }

    fn req(priority: MessagePriority, size: usize, multipath: bool) -> TransportSelectionRequest {
        TransportSelectionRequest {
            target_peer: None,
            message_size: size,
            priority,
            prefer_low_cost: false,
            max_latency_ms: None,
            multipath,
            fragmentable: false,
        }
    }

    #[tokio::test]
    async fn registers_and_selects_connected_transport() {
        let mgr = TransportManager::new();
        let t = Arc::new(StubTransport::new(
            "internet-0",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 100_000_000, 0.0),
        ));
        mgr.register(t).await.unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P5, 1_000, false))
            .await;
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].transport_id.as_str(), "internet-0");
        assert!(selected[0].score > 0.0);
    }

    #[tokio::test]
    async fn unregistered_transport_is_not_selected() {
        let mgr = TransportManager::new();
        let selected = mgr
            .select_transports(&req(MessagePriority::P5, 1_000, false))
            .await;
        assert!(selected.is_empty());
    }

    #[tokio::test]
    async fn unavailable_transport_excluded() {
        let mgr = TransportManager::new();
        let t = Arc::new(StubTransport::new(
            "ble-android",
            free_caps(),
            TransportState::Unavailable,
            cost(3.0, 0.0, 0, 0.0),
        ));
        mgr.register(t).await.unwrap();
        let selected = mgr
            .select_transports(&req(MessagePriority::P0, 1_000, true))
            .await;
        assert!(selected.is_empty());
    }

    #[tokio::test]
    async fn size_filter_eliminates_small_transport() {
        let mgr = TransportManager::new();
        let small = caps(250, 500, 1_000_000);
        let t = Arc::new(StubTransport::new(
            "lora",
            small,
            TransportState::Connected,
            cost(1.0, 0.0, 1_000_000, 0.0),
        ));
        mgr.register(t).await.unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P3, 5_000, false))
            .await;
        assert!(
            selected.is_empty(),
            "LoRa must not carry >250-byte messages"
        );
    }

    #[tokio::test]
    async fn multipath_returns_all_for_p0() {
        let mgr = TransportManager::new();
        mgr.register(Arc::new(StubTransport::new(
            "ble-android",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 1_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "wifi-direct",
            free_caps(),
            TransportState::Connected,
            cost(5.0, 0.0, 50_000_000, 0.0),
        )))
        .await
        .unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P0, 100, true))
            .await;
        assert_eq!(
            selected.len(),
            2,
            "P0 multipath must use all viable transports"
        );
    }

    #[tokio::test]
    async fn single_best_for_normal_priority() {
        let mgr = TransportManager::new();
        mgr.register(Arc::new(StubTransport::new(
            "ble-android",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 1_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "internet-0",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P5, 20_000, false))
            .await;
        assert_eq!(selected.len(), 1, "P5 uses single best transport");
        assert_eq!(
            selected[0].transport_id.as_str(),
            "internet-0",
            "higher bandwidth wins for >10KB"
        );
    }

    #[tokio::test]
    async fn duplicate_registration_rejected() {
        let mgr = TransportManager::new();
        let t = Arc::new(StubTransport::new(
            "internet-0",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 0, 0.0),
        ));
        mgr.register(t.clone()).await.unwrap();
        assert_eq!(
            mgr.register(t).await.unwrap_err(),
            RegistrationError::DuplicateId
        );
    }

    // RED-0004: a fragmentable (P4–P7 bulk) message larger than every
    // transport MTU must still be selectable — the engine will split it. A
    // BLE-class 512 B transport must be eligible for a 70 KiB bulk ADU.
    #[tokio::test]
    async fn fragmentable_oversize_message_selects_small_mtu_transport() {
        let mgr = TransportManager::new();
        let ble = Arc::new(StubTransport::new(
            "ble-0",
            caps(512, 5, 1_000_000),
            TransportState::Available,
            cost(0.5, 0.0, 1_000_000, 0.0),
        ));
        mgr.register(ble).await.unwrap();

        // 70 KiB > 512 B MTU → not selectable without fragmentation.
        let plain = TransportSelectionRequest {
            target_peer: None,
            message_size: 70_000,
            priority: MessagePriority::P5,
            prefer_low_cost: false,
            max_latency_ms: None,
            multipath: false,
            fragmentable: false,
        };
        assert!(
            mgr.select_transports(&plain).await.is_empty(),
            "oversize non-fragmentable is unroutable"
        );

        let frag = TransportSelectionRequest {
            fragmentable: true,
            ..plain
        };
        let selected = mgr.select_transports(&frag).await;
        assert_eq!(
            selected.len(),
            1,
            "fragmentable oversize selects the BLE transport"
        );
        assert_eq!(selected[0].transport_id.as_str(), "ble-0");
    }

    #[tokio::test]
    async fn deregister_removes_transport() {
        let mgr = TransportManager::new();
        let id = TransportId::from("internet-0");
        let t = Arc::new(StubTransport::new(
            "internet-0",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 0, 0.0),
        ));
        mgr.register(t).await.unwrap();
        mgr.deregister(&id).await.unwrap();
        assert!(mgr.get(&id).await.is_none());
        assert_eq!(
            mgr.deregister(&id).await.unwrap_err(),
            RegistrationError::NotFound
        );
    }

    #[test]
    fn radio_conflict_groups_distinct() {
        use std::collections::HashSet;
        let groups = [
            RadioConflictGroup::Bluetooth24GHz,
            RadioConflictGroup::WiFi24GHz,
            RadioConflictGroup::WiFi5GHz,
            RadioConflictGroup::SubGHz,
            RadioConflictGroup::Cellular,
            RadioConflictGroup::None,
        ];
        let set: HashSet<_> = groups.iter().collect();
        assert_eq!(set.len(), groups.len());
    }

    #[tokio::test]
    async fn multipath_excludes_unavailable_transports() {
        // P0 multipath must return every viable transport (Connected + Degraded)
        // and never include Unavailable/Connecting candidates.
        let mgr = TransportManager::new();
        mgr.register(Arc::new(StubTransport::new(
            "net-a",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "net-b",
            free_caps(),
            TransportState::Degraded,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "net-off",
            free_caps(),
            TransportState::Unavailable,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "net-connecting",
            free_caps(),
            TransportState::Connecting,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P0, 100, true))
            .await;
        let mut ids: Vec<&str> = selected.iter().map(|r| r.transport_id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(
            ids,
            vec!["net-a", "net-b"],
            "P0 multipath must keep all viable and exclude unavailable"
        );
        assert!(selected.iter().all(|r| r.score > 0.0));
    }

    #[tokio::test]
    async fn ranking_prefers_connected_over_degraded() {
        // Identical caps/cost: the only differentiator is state, so Connected
        // (baseline 40) must outrank Degraded (baseline 5).
        let mgr = TransportManager::new();
        mgr.register(Arc::new(StubTransport::new(
            "degraded",
            free_caps(),
            TransportState::Degraded,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();
        mgr.register(Arc::new(StubTransport::new(
            "connected",
            free_caps(),
            TransportState::Connected,
            cost(3.0, 0.0, 100_000_000, 0.0),
        )))
        .await
        .unwrap();

        let selected = mgr
            .select_transports(&req(MessagePriority::P5, 1_000, false))
            .await;
        assert_eq!(selected.len(), 1);
        assert_eq!(
            selected[0].transport_id.as_str(),
            "connected",
            "Connected must beat Degraded when otherwise equal"
        );
        assert!(selected[0].score > 0.0);
    }

    #[tokio::test]
    async fn registers_ten_transports_without_panic() {
        // Stress: a fully-registered mesh node can hold 10 transports. Register
        // must be idempotent for distinct ids and never panic.
        let mgr = TransportManager::new();
        let mut ids = Vec::new();
        for i in 0..10 {
            let id = TransportId(format!("transport-{i}"));
            mgr.register(Arc::new(StubTransport::new(
                &id.0,
                free_caps(),
                TransportState::Available,
                cost(1.0, 0.0, 100_000_000, 0.0),
            )))
            .await
            .unwrap();
            ids.push(id);
        }
        assert_eq!(mgr.list().await.len(), 10);
        for id in &ids {
            assert!(mgr.get(id).await.is_some(), "missing {id}");
        }
    }
}
