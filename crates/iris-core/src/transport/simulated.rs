//! Deterministic simulated transport — routing/simulation experiments.
//!
//! Mirrors `docs/transports/TRANSPORT_ABSTRACTION.md` §SimulatedTransport.
//! All loss/delay decisions are driven by a seeded ChaCha8 RNG, so a fixed
//! seed yields reproducible behavior (SIM-001 acceptance criterion).

use std::pin::Pin;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::sync::broadcast;

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::{
    AtomicState, Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportState, TransportStateEvent,
};
use crate::TransportError;

/// Configuration for a simulated transport.
#[derive(Debug, Clone)]
pub struct SimConfig {
    pub packet_loss_rate: f32,
    pub bandwidth_bps: u64,
    /// Base latency + spread (uniform).
    pub latency_base_ms: u64,
    pub latency_spread_ms: u64,
    /// Seed for deterministic reproduction.
    pub seed: u64,
    /// Override for cost_snapshot().estimated_battery_ma (TAK-19): lets
    /// tests inject adversarial cost inputs - e.g. NaN - to prove the
    /// manager's comparator stays total and deterministic (RED-0003-01).
    /// None keeps the fixed default of 3.0 mA.
    pub battery_ma_override: Option<f32>,
}

impl Default for SimConfig {
    fn default() -> Self {
        SimConfig {
            packet_loss_rate: 0.0,
            bandwidth_bps: 1_000_000,
            latency_base_ms: 10,
            latency_spread_ms: 5,
            seed: 42,
            battery_ma_override: None,
        }
    }
}

/// A simulated peer that echoes received messages back.
#[derive(Debug, Clone)]
pub struct SimulatedPeer {
    pub peer_id: PeerId,
    pub echo: bool,
    pub drop_rate: f32,
}

/// In-memory transport that applies configured loss/latency deterministically.
pub struct SimulatedTransport {
    id: TransportId,
    display: String,
    caps: TransportCapabilities,
    config: SimConfig,
    rng: tokio::sync::Mutex<ChaCha8Rng>,
    state: AtomicState,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
}

impl SimulatedTransport {
    pub fn new(id: &str, display: &str, config: SimConfig) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = AtomicState::default();
        state.store(TransportState::Available);
        let caps = TransportCapabilities {
            max_message_size: 64 * 1024,
            supports_broadcast: true,
            supports_unicast: true,
            supports_multicast: true,
            range_m_min: 0,
            range_m_max: 100,
            range_m_typical: 50,
            typical_throughput_bps: config.bandwidth_bps,
            typical_latency_ms: config.latency_base_ms as u32,
            requires_infrastructure: false,
            supports_background_android: true,
            supports_background_ios: true,
            requires_special_hardware: false,
            cost_class: TransportCostClass::Free,
            regulatory_band: None,
            conflict_group: crate::transport::RadioConflictGroup::None,
        };
        let seed = config.seed;
        SimulatedTransport {
            id: TransportId::from(id),
            display: display.to_string(),
            caps,
            config,
            rng: tokio::sync::Mutex::new(ChaCha8Rng::seed_from_u64(seed)),
            state,
            state_tx,
            incoming_tx,
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    /// Deterministic loss decision for a packet.
    async fn is_lost(&self) -> bool {
        let mut rng = self.rng.lock().await;
        let r: f32 = rand::Rng::gen_range(&mut *rng, 0.0..1.0);
        r < self.config.packet_loss_rate
    }
}

#[async_trait]
impl Transport for SimulatedTransport {
    fn transport_id(&self) -> &TransportId {
        &self.id
    }
    fn display_name(&self) -> &str {
        &self.display
    }
    fn capabilities(&self) -> &TransportCapabilities {
        &self.caps
    }
    fn state(&self) -> TransportState {
        self.state.load()
    }
    fn state_stream(&self) -> Pin<Box<dyn Stream<Item = TransportStateEvent> + Send>> {
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "sim.state")
    }

    async fn discover_peers(
        &self,
        _config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn start_advertising(&self, _info: NodeAdvertisement) -> Result<(), TransportError> {
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        self.set_state(TransportState::Connected);
        Ok(TransportLink {
            peer_id: peer.peer_id,
            transport_id: self.id.as_str().to_string(),
            established_at: Instant::now(),
        })
    }

    async fn send(
        &self,
        peer: &PeerId,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        if self.is_lost().await {
            return Ok(SendReceipt {
                peer_id: *peer,
                bytes_sent: message.payload.len(),
                sent_at: Instant::now(),
            });
        }
        let latency = self.config.latency_base_ms + {
            let mut rng = self.rng.lock().await;
            rand::Rng::gen_range(&mut *rng, 0..self.config.latency_spread_ms + 1)
        };
        let delay = Duration::from_millis(latency);
        let incoming = self.incoming_tx.clone();
        let transport_id = self.id.as_str().to_string();
        let peer_id = *peer;
        let payload = message.payload.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            incoming.send(IncomingMessage {
                peer_id,
                transport_id,
                payload,
                received_at: Instant::now(),
            }).ok();
        });
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: message.payload.len(),
            sent_at: Instant::now(),
        })
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "sim.messages")
    }

    fn cost_snapshot(&self) -> TransportCost {
        TransportCost {
            estimated_battery_ma: self.config.battery_ma_override.unwrap_or(3.0),
            monetary_cost_per_kb: 0.0,
            bandwidth_available_bps: self.config.bandwidth_bps,
            congestion_level: 0.0,
        }
    }

    fn set_send_priority_hint(&self, _priority: MessagePriority) {}

    async fn shutdown(&self) -> Result<(), TransportError> {
        self.set_state(TransportState::Unavailable);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn zero_loss_delivers() {
        let t = SimulatedTransport::new("sim-0", "Sim", SimConfig::default());
        let peer = PeerId([9u8; 32]);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([1u8; 16]),
            priority: MessagePriority::P3,
            payload: b"data".to_vec(),
        };
        t.send(&peer, &msg).await.unwrap();
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_millis(200), incoming.next()).await;
        assert!(
            got.is_ok() && got.unwrap().is_some(),
            "must deliver at loss=0"
        );
    }

    #[tokio::test]
    async fn full_loss_drops() {
        let cfg = SimConfig {
            packet_loss_rate: 1.0,
            ..Default::default()
        };
        let t = SimulatedTransport::new("sim-0", "Sim", cfg);
        let peer = PeerId([9u8; 32]);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([1u8; 16]),
            priority: MessagePriority::P3,
            payload: b"data".to_vec(),
        };
        t.send(&peer, &msg).await.unwrap();
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_millis(200), incoming.next()).await;
        assert!(got.is_err(), "must drop at loss=1.0");
    }

    #[tokio::test]
    async fn state_transitions_flow() {
        let t = SimulatedTransport::new("sim-0", "Sim", SimConfig::default());
        assert_eq!(t.state(), TransportState::Available);
        let peer_info = PeerInfo {
            peer_id: PeerId([1u8; 32]),
            addresses: Vec::new(),
            transport_addresses: Vec::new(),
            last_seen: None,
        };
        t.connect(&peer_info).await.unwrap();
        assert_eq!(t.state(), TransportState::Connected);
        t.shutdown().await.unwrap();
        assert_eq!(t.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn same_seed_produces_identical_loss_outcomes() {
        // SIM-001 determinism: two transports with the same seed must drop the
        // SAME set of messages. Loss 0.5 over 20 sends exercises both outcomes.
        let cfg = SimConfig {
            packet_loss_rate: 0.5,
            seed: 7,
            latency_spread_ms: 0, // deterministic latency; no scheduling skew
            ..Default::default()
        };
        let a = SimulatedTransport::new("sim-a", "A", cfg.clone());
        let b = SimulatedTransport::new("sim-b", "B", cfg);

        let in_a = a.incoming_messages();
        let in_b = b.incoming_messages();

        let peer = PeerId([9u8; 32]);
        let n = 20usize;
        for i in 0..n {
            let msg = SerializedMessage {
                message_id: crate::protocol::MessageId::from([i as u8; 16]),
                priority: MessagePriority::P3,
                payload: format!("msg-{i}").into_bytes(),
            };
            a.send(&peer, &msg).await.unwrap();
            b.send(&peer, &msg).await.unwrap();
        }

        let collect = async |mut rx: Pin<Box<dyn Stream<Item = IncomingMessage> + Send>>| {
            let mut delivered = Vec::new();
            while delivered.len() < n {
                match tokio::time::timeout(Duration::from_millis(400), rx.next()).await {
                    Ok(Some(msg)) => delivered.push(msg.payload),
                    _ => break, // stream quiet → all remaining were dropped
                }
            }
            delivered
        };

        let (delivered_a, delivered_b) = tokio::join!(collect(in_a), collect(in_b));
        assert!(
            delivered_a.len() < n && !delivered_a.is_empty(),
            "loss 0.5 must drop some and deliver some (got {} of {n})",
            delivered_a.len()
        );

        let mut a_sorted = delivered_a.clone();
        a_sorted.sort_unstable();
        let mut b_sorted = delivered_b.clone();
        b_sorted.sort_unstable();
        assert_eq!(
            a_sorted, b_sorted,
            "same seed must drop the SAME messages: a={a_sorted:?} b={b_sorted:?}"
        );
    }
}
