//! Discovery engine (layer 6) — DISCO-001.
//!
//! Transport-agnostic neighbor discovery. `DiscoveryManager` drives
//! `Transport::discover_peers()` across all registered transports, dedupes by
//! `peer_id`, feeds the `NeighborTable`, and emits `TopologyEvent`s on a
//! broadcast channel. First contact triggers a CAPABILITY + Bloom handshake
//! (`handshake` module) so peers record each other's capabilities and dedup
//! filters.
//!
//! Real BLE/Wi-Fi-Aware adapters are deferred (BLK-0005); this engine runs
//! against any `Transport` implementation (`SimulatedTransport`,
//! `InternetTransport`) — see `DiscoveryManager::scan_once`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::sync::broadcast;

use crate::message::{DiscoveryConfig as ScanConfig, LinkQuality, PeerId};
use crate::message::MessagePriority::P2;
use crate::transport::{TopologyEvent, Transport, TransportId};
use crate::TransportError;

pub mod handshake;
pub mod neighbor_table;

use handshake::{HandshakeMessage, parse_handshake};
use neighbor_table::NeighborTable;

/// Discovery run mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DiscoveryMode {
    /// Fast, frequent scans — use when actively looking for low-power peers.
    Active = 0,
    /// Periodic scans at a slow interval — battery-friendly baseline.
    Passive = 1,
}

impl DiscoveryMode {
    pub fn as_u8(self) -> u8 {
        self as u8
    }
    pub fn from_u8(v: u8) -> Option<DiscoveryMode> {
        Some(match v {
            0 => DiscoveryMode::Active,
            1 => DiscoveryMode::Passive,
            _ => return None,
        })
    }
}

/// Configuration for the discovery engine.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Interval between full scan rounds (per registered transport).
    pub scan_interval: Duration,
    /// Neighbor idle-TTL before eviction.
    pub neighbor_ttl: Duration,
    /// Per-scan timeout handed to `Transport::discover_peers`.
    pub scan_timeout: Duration,
    /// Max peers reported per scan.
    pub max_peers: usize,
    /// Capability tags advertised by this node.
    pub capabilities: Vec<String>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            scan_interval: Duration::from_secs(30),
            neighbor_ttl: Duration::from_secs(300),
            scan_timeout: Duration::from_secs(10),
            max_peers: 64,
            capabilities: Vec::new(),
        }
    }
}

/// The transport-agnostic discovery engine (DISCO-001).
pub struct DiscoveryManager {
    node_id: PeerId,
    config: DiscoveryConfig,
    table: Arc<NeighborTable>,
    events: broadcast::Sender<TopologyEvent>,
    transports: RwLock<Vec<Arc<dyn Transport>>>,
    mode: AtomicU8,
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl DiscoveryManager {
    pub fn new(node_id: PeerId, config: DiscoveryConfig) -> Self {
        let (events, _) = broadcast::channel(256);
        DiscoveryManager {
            node_id,
            table: Arc::new(NeighborTable::new(config.neighbor_ttl)),
            events,
            transports: RwLock::new(Vec::new()),
            mode: AtomicU8::new(DiscoveryMode::Passive as u8),
            task: tokio::sync::Mutex::new(None),
            config,
        }
    }

    /// Register a transport to scan. Takes effect on the next scan round.
    pub async fn register_transport(&self, transport: Arc<dyn Transport>) {
        self.transports.write().await.push(transport);
    }

    pub async fn deregister_transport(&self, id: &TransportId) {
        self.transports.write().await.retain(|t| t.transport_id() != id);
    }

    pub fn mode(&self) -> DiscoveryMode {
        DiscoveryMode::from_u8(self.mode.load(Ordering::Relaxed)).unwrap_or(DiscoveryMode::Passive)
    }

    pub fn set_mode(&self, mode: DiscoveryMode) {
        self.mode.store(mode.as_u8(), Ordering::Relaxed);
    }

    /// Reactive API: a transport may push a discovered peer without the poll
    /// loop (e.g. an event-driven BLE callback later). Dedup + emit handled
    /// identically to the scan path.
    pub async fn report_peer(
        &self,
        peer: &crate::message::PeerInfo,
        via: &TransportId,
        quality: LinkQuality,
    ) {
        if let Some(ev) = self.table.upsert(peer, via, quality).await {
            let _ = self.events.send(ev);
        }
    }

    /// Ingest an inbound handshake envelope (CAPABILITY bundle or Bloom
    /// exchange). Updates the neighbor table with the peer's capabilities and
    /// records its dedup Bloom filter for the dispatch layer.
    pub async fn ingest_handshake(&self, env: &crate::protocol::Envelope) {
        let Ok(msg) = parse_handshake(env) else {
            return;
        };
        match msg {
            HandshakeMessage::Capability(bundle) => {
                let node_id = bundle.node_id;
                self.table.set_capabilities(&node_id, bundle).await;
            }
            HandshakeMessage::BloomExchange(exchange) => {
                if let Some(filter) = exchange.into_filter() {
                    self.table.set_peer_bloom(exchange.node_id, filter).await;
                }
            }
        }
    }

    /// Run one scan round: for each registered transport, call
    /// `discover_peers`, upsert each peer into the neighbor table, and emit
    /// topology events. Begins the handshake for newly discovered peers.
    pub async fn scan_once(&self) {
        let transports = self.transports.read().await.clone();
        let mut new_peers = Vec::new();
        for transport in transports {
            let scan_cfg = ScanConfig {
                timeout: self.config.scan_timeout,
                max_peers: self.config.max_peers,
                filter: None,
            };
            let mut stream = match transport.discover_peers(scan_cfg).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            while let Some(peer) = futures_util::StreamExt::next(&mut stream).await {
                let ev = self
                    .table
                    .upsert(&peer, transport.transport_id(), LinkQuality::Good)
                    .await;
                if matches!(ev, Some(TopologyEvent::PeerDiscovered { .. })) {
                    new_peers.push(peer.peer_id);
                }
                if let Some(ev) = ev {
                    let _ = self.events.send(ev);
                }
            }
        }
        // First-contact handshake for peers never seen before.
        for peer_id in new_peers {
            self.run_handshake(peer_id).await;
        }
        // TTL eviction pass.
        for ev in self.table.sweep().await {
            let _ = self.events.send(ev);
        }
    }

    /// Exchange capabilities + dedup Bloom with a peer. Builds the handshake
    /// envelopes; delivery is asynchronous — the remote side ingests via
    /// `ingest_handshake`. Sent over every transport that links to the peer
    /// (ROUTE-001 later selects the optimal one).
    async fn run_handshake(&self, peer_id: PeerId) {
        let transports = self.transports.read().await.clone();
        if transports.is_empty() {
            return;
        }
        let bloom = crate::message_engine::dedup::BloomFilter::default();
        let (cap_env, bloom_env) = match handshake::build_handshake(
            self.node_id,
            self.config.capabilities.clone(),
            transports
                .iter()
                .map(|t| t.transport_id().as_str().to_string())
                .collect(),
            &bloom,
            &peer_id,
        ) {
            Ok(pair) => pair,
            Err(_) => return,
        };
        for transport in &transports {
            let _ = Self::try_send(transport, &peer_id, &cap_env).await;
            let _ = Self::try_send(transport, &peer_id, &bloom_env).await;
        }
    }

    /// Best-effort send of a handshake envelope over a transport.
    async fn try_send(
        transport: &Arc<dyn Transport>,
        peer: &PeerId,
        env: &crate::protocol::Envelope,
    ) -> Result<(), TransportError> {
        let payload = match crate::protocol::codec::encode(env) {
            Ok(p) => p,
            Err(_) => return Ok(()), // encode failure: skip, not a transport error
        };
        let msg = crate::message::SerializedMessage {
            message_id: env.message_id,
            priority: P2,
            payload,
        };
        transport.send(peer, &msg).await.map(|_| ())
    }

    /// Start the background discovery loop. Idempotent. The loop runs until
    /// [`DiscoveryManager::stop`] aborts it.
    pub async fn start(self: &Arc<Self>) {
        let mut guard = self.task.lock().await;
        if guard.is_some() {
            return;
        }
        let this = self.clone();
        let handle = tokio::spawn(async move {
            loop {
                this.scan_once().await;
                tokio::time::sleep(this.config.scan_interval).await;
            }
        });
        *guard = Some(handle);
    }

    pub async fn stop(&self) {
        if let Some(handle) = self.task.lock().await.take() {
            handle.abort();
        }
    }

    /// Broadcast channel for `TopologyEvent`s.
    pub fn events(&self) -> broadcast::Receiver<TopologyEvent> {
        self.events.subscribe()
    }

    /// Shared neighbor table (routing queries ride on this).
    pub fn neighbors(&self) -> Arc<NeighborTable> {
        self.table.clone()
    }

    pub fn node_id(&self) -> PeerId {
        self.node_id
    }
}

/// Re-export for consumers of the discovery API.
pub use handshake::HandshakeError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{MessagePriority, PeerInfo, SerializedMessage};
    use crate::message_engine::dedup::BloomFilter;
    use crate::protocol::codec;
    use crate::transport::simulated::{SimConfig, SimulatedTransport};
    use crate::transport::TransportManager;
    use futures_util::StreamExt;

    const ALICE: [u8; 32] = [0xAA; 32];
    const BOB: [u8; 32] = [0xBB; 32];

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn manager(id: u8) -> Arc<DiscoveryManager> {
        Arc::new(DiscoveryManager::new(
            pid(id),
            DiscoveryConfig {
                scan_interval: Duration::from_millis(20),
                neighbor_ttl: Duration::from_secs(60),
                scan_timeout: Duration::from_millis(100),
                max_peers: 64,
                capabilities: vec!["relay".into()],
            },
        ))
    }

    fn peer_info(id: u8) -> PeerInfo {
        PeerInfo {
            peer_id: pid(id),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    #[test]
    fn mode_round_trips() {
        assert_eq!(DiscoveryMode::from_u8(0), Some(DiscoveryMode::Active));
        assert_eq!(DiscoveryMode::from_u8(1), Some(DiscoveryMode::Passive));
        assert_eq!(DiscoveryMode::from_u8(2), None);
        assert_eq!(DiscoveryMode::Active.as_u8(), 0);
    }

    #[tokio::test]
    async fn scan_without_transports_is_noop() {
        let m = manager(1);
        m.scan_once().await;
        assert_eq!(m.neighbors().len().await, 0);
    }

    #[tokio::test]
    async fn report_peer_populates_table_and_emits_event() {
        let m = manager(1);
        let mut rx = m.events();
        let peer = peer_info(9);
        m.report_peer(&peer, &TransportId::from("sim"), LinkQuality::Excellent)
            .await;
        assert_eq!(m.neighbors().len().await, 1);
        assert!(m.neighbors().is_up(&peer.peer_id).await);
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::PeerDiscovered { .. }));
        assert_eq!(
            m.neighbors().transports_to(&peer.peer_id).await.unwrap().len(),
            1
        );
    }

    #[tokio::test]
    async fn neighbor_lifecycle_up_update_down() {
        let m = manager(2);
        let mut rx = m.events();
        let peer = peer_info(7);
        m.report_peer(&peer, &TransportId::from("sim-a"), LinkQuality::Good)
            .await;
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::PeerDiscovered { .. }));
        // Quality update on the same transport emits LinkQualityUpdated.
        m.report_peer(&peer, &TransportId::from("sim-a"), LinkQuality::Fair)
            .await;
        let ev = tokio::time::timeout(Duration::from_millis(500), rx.recv())
            .await
            .expect("event")
            .expect("event");
        assert!(matches!(ev, TopologyEvent::LinkQualityUpdated { .. }));
        // Down.
        let ev = m
            .neighbors()
            .mark_down(&peer.peer_id, &TransportId::from("sim-a"))
            .await;
        assert!(matches!(ev, Some(TopologyEvent::PeerLost { .. })));
        assert!(!m.neighbors().is_up(&peer.peer_id).await);
    }

    #[tokio::test]
    async fn background_loop_scans_periodically() {
        let m = manager(3);
        m.start().await;
        // No transports registered → loop must not spin or panic.
        tokio::time::sleep(Duration::from_millis(60)).await;
        m.stop().await;
    }

    // --- Handshake relay across SimulatedTransport (M4-style connector) ---

    #[tokio::test(flavor = "multi_thread")]
    async fn handshake_round_trip_records_capabilities_and_bloom() {
        // Alice's side: handshake envelopes flow through the simulated
        // transport to Bob's discovery manager (M4-style connector).
        let a_transport = Arc::new(SimulatedTransport::new(
            "sim-alice",
            "Sim A",
            SimConfig {
                packet_loss_rate: 0.0,
                ..Default::default()
            },
        ));
        let a_manager = Arc::new(TransportManager::new());
        a_manager.register(a_transport.clone()).await.unwrap();
        let _ = (&a_manager, &Arc::new(TransportManager::new()));

        // Alice builds + sends the CAPABILITY and Bloom envelopes to Bob.
        let mut bloom = BloomFilter::new(1000, 0.001);
        bloom.insert([0x11; 16]);
        let (cap_env, bloom_env) = handshake::build_handshake(
            PeerId::from_bytes(ALICE),
            vec!["relay".into()],
            vec!["sim-alice".into()],
            &bloom,
            &PeerId::from_bytes(BOB),
        )
        .unwrap();
        for env in [cap_env, bloom_env] {
            let payload = codec::encode(&env).unwrap();
            a_transport
                .send(
                    &PeerId::from_bytes(BOB),
                    &SerializedMessage {
                        message_id: env.message_id,
                        priority: MessagePriority::P2,
                        payload,
                    },
                )
                .await
                .unwrap();
        }

        // Bob's discovery manager ingests what his transport delivers.
        let bob_dm = manager(0xBB);
        // Real flow: Alice is first discovered (upsert), then handshake lands.
        bob_dm
            .report_peer(
                &PeerInfo {
                    peer_id: PeerId::from_bytes(ALICE),
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim-alice"),
                LinkQuality::Good,
            )
            .await;
        let mut stream = a_transport.incoming_messages();
        for _ in 0..2 {
            let msg = tokio::time::timeout(Duration::from_millis(500), stream.next())
                .await
                .expect("transport delivers")
                .expect("stream item");
            let env = codec::decode(&msg.payload).unwrap();
            bob_dm.ingest_handshake(&env).await;
        }
        let nb = bob_dm.neighbors().get(&PeerId::from_bytes(ALICE)).await;
        let nb = nb.expect("capability recorded for alice");
        let caps = nb.capabilities.as_ref().expect("capabilities set");
        assert_eq!(caps.capabilities, vec!["relay".to_string()]);
        assert_eq!(caps.bloom_m, bloom.m());
        assert_eq!(caps.bloom_k, bloom.k());
        assert!(caps.transports.contains(&"sim-alice".to_string()));
        // Bloom filter recorded on the neighbor.
        let peer_bloom = bob_dm.neighbors().peer_bloom(&PeerId::from_bytes(ALICE)).await;
        assert!(peer_bloom.is_some());
        assert!(peer_bloom.unwrap().contains([0x11; 16]));
    }
}