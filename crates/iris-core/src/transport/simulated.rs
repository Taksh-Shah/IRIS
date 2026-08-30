//! Deterministic simulated transport — routing/simulation experiments.
//!
//! Mirrors `docs/transports/TRANSPORT_ABSTRACTION.md` §SimulatedTransport.
//! All loss/delay decisions are driven by a seeded ChaCha8 RNG, so a fixed
//! seed yields reproducible behavior (SIM-001 acceptance criterion).
//!
//! ## What this simulator models
//! - Deterministic packet loss (`packet_loss_rate`) — SIM-001 AC-1
//! - Deterministic latency (`latency_base_ms` + `latency_spread_ms`) — SIM-001 AC-2
//! - MTU enforcement (`max_message_size`) — RF-43
//! - Probabilistic connection failure (`connect_failure_rate`) — RF-45-a
//! - Bandwidth throttling via token bucket (`bandwidth_bps`) — RF-45-b
//! - Optional ordering preservation (`preserve_order`) — RF-45-c
//! - Delivery-task abort on shutdown — RF-45-d
//!
//! ## What this simulator does NOT model
//! - Multi-hop routing or mesh topology
//! - Encryption or authentication overhead
//! - Real OS radio arbitration or power-save delays

use std::pin::Pin;
use std::sync::Mutex as StdMutex;
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

/// Token bucket for bandwidth enforcement (RF-45-b).
struct TokenBucket {
    available_bytes: f64,
    last_refill: Instant,
    rate_bytes_per_sec: f64,
    capacity_bytes: f64,
}

impl TokenBucket {
    fn new(bandwidth_bps: u64) -> Self {
        let rate = bandwidth_bps as f64 / 8.0;
        let capacity = rate.max(65536.0); // at least 64 KiB burst
        TokenBucket {
            available_bytes: capacity,
            last_refill: Instant::now(),
            rate_bytes_per_sec: rate,
            capacity_bytes: capacity,
        }
    }

    /// Refill from elapsed time and try to consume `bytes`. Returns false if
    /// the bucket is empty (caller should return `TransportError::Busy`).
    fn try_consume(&mut self, bytes: usize) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.available_bytes =
            (self.available_bytes + elapsed * self.rate_bytes_per_sec).min(self.capacity_bytes);
        self.last_refill = now;
        if self.available_bytes >= bytes as f64 {
            self.available_bytes -= bytes as f64;
            true
        } else {
            false
        }
    }
}

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
    /// Maximum payload bytes `send()` accepts (RF-43). None defaults to 64 KiB.
    /// Set to e.g. 237 to simulate a LoRa-like MTU, or 185 for BLE.
    pub max_message_size: Option<usize>,
    /// Probability that `connect()` returns `ConnectionFailed` (RF-45-a).
    /// 0.0 = always succeed, 1.0 = always fail.
    pub connect_failure_rate: f32,
    /// When true, messages are delivered in send order (RF-45-c).
    /// When false (default), independent per-message sleeps may reorder delivery.
    pub preserve_order: bool,
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
            max_message_size: None,
            connect_failure_rate: 0.0,
            preserve_order: false,
        }
    }
}

/// In-memory transport that applies configured loss/latency deterministically.
pub struct SimulatedTransport {
    id: TransportId,
    /// Stable PeerId derived from the transport id string, used as sender attribution
    /// when this transport delivers a message to a paired peer transport.
    my_peer_id: PeerId,
    display: String,
    caps: TransportCapabilities,
    config: SimConfig,
    rng: tokio::sync::Mutex<ChaCha8Rng>,
    state: AtomicState,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    /// When `Some`, send() routes outbound frames to the paired transport instead of
    /// looping back to self. Set by `connect_pair`.
    peer_tx: StdMutex<Option<broadcast::Sender<IncomingMessage>>>,
    /// Handles for in-flight delivery tasks; aborted on shutdown() (RF-45-d).
    pending_deliveries: StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    /// For preserve_order: the earliest Instant at which the next message may be delivered,
    /// ensuring messages queue behind the previous one (RF-45-c).
    last_delivery_at: StdMutex<Instant>,
    /// Token bucket for bandwidth enforcement (RF-45-b).
    token_bucket: StdMutex<TokenBucket>,
}

impl SimulatedTransport {
    pub fn new(id: &str, display: &str, config: SimConfig) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = AtomicState::default();
        state.store(TransportState::Available);
        let caps = TransportCapabilities {
            max_message_size: config.max_message_size.unwrap_or(64 * 1024),
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
        let mut peer_id_bytes = [0u8; 32];
        let id_bytes = id.as_bytes();
        let copy_len = id_bytes.len().min(32);
        peer_id_bytes[..copy_len].copy_from_slice(&id_bytes[..copy_len]);
        let bw = config.bandwidth_bps;
        SimulatedTransport {
            id: TransportId::from(id),
            my_peer_id: PeerId(peer_id_bytes),
            display: display.to_string(),
            caps,
            config,
            rng: tokio::sync::Mutex::new(ChaCha8Rng::seed_from_u64(seed)),
            state,
            state_tx,
            incoming_tx,
            peer_tx: StdMutex::new(None),
            pending_deliveries: StdMutex::new(Vec::new()),
            last_delivery_at: StdMutex::new(Instant::now()),
            token_bucket: StdMutex::new(TokenBucket::new(bw)),
        }
    }

    /// Wire two transports to each other so that send() on A delivers to B's
    /// incoming stream (and vice-versa), attributed to the sender's own peer id.
    pub fn connect_pair(a: &SimulatedTransport, b: &SimulatedTransport) {
        *a.peer_tx.lock().unwrap() = Some(b.incoming_tx.clone());
        *b.peer_tx.lock().unwrap() = Some(a.incoming_tx.clone());
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
        // RF-45-a: fault injection — probabilistic connection failure.
        if self.config.connect_failure_rate > 0.0 {
            let mut rng = self.rng.lock().await;
            let r: f32 = rand::Rng::gen_range(&mut *rng, 0.0..1.0);
            if r < self.config.connect_failure_rate {
                return Err(TransportError::ConnectionFailed);
            }
        }
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
        // RF-42: respect transport lifecycle — a shut-down transport must not deliver.
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::ShuttingDown);
        }
        // RF-43: enforce the advertised MTU so fragmentation tests are meaningful.
        let mtu = self.caps.max_message_size;
        if message.payload.len() > mtu {
            return Err(TransportError::MessageTooLarge {
                limit: mtu,
                actual: message.payload.len(),
            });
        }
        // RF-45-b: token-bucket bandwidth enforcement.
        if !self.token_bucket.lock().unwrap().try_consume(message.payload.len()) {
            return Err(TransportError::Busy);
        }
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
        // Route to paired peer if connected; otherwise loopback (for standalone tests).
        let (target_tx, sender_peer_id) = {
            let guard = self.peer_tx.lock().unwrap();
            match guard.clone() {
                Some(tx) => (tx, self.my_peer_id),
                None => (self.incoming_tx.clone(), *peer),
            }
        };
        let transport_id = self.id.as_str().to_string();
        let payload = message.payload.clone();
        // RF-45-c: when preserve_order is true, ensure this message's delivery
        // instant is no earlier than the previous message's delivery instant.
        let sleep_until = if self.config.preserve_order {
            let mut last = self.last_delivery_at.lock().unwrap();
            let deliver_at = (*last).max(Instant::now()) + delay;
            *last = deliver_at;
            deliver_at
        } else {
            Instant::now() + delay
        };
        // RF-45-d: track the spawned task so shutdown() can abort it.
        let handle = tokio::spawn(async move {
            let now = Instant::now();
            if sleep_until > now {
                tokio::time::sleep(sleep_until - now).await;
            }
            target_tx.send(IncomingMessage {
                peer_id: sender_peer_id,
                transport_id,
                payload,
                received_at: Instant::now(),
            }).ok();
        });
        {
            let mut pending = self.pending_deliveries.lock().unwrap();
            // Prune finished handles to bound memory.
            pending.retain(|h| !h.is_finished());
            pending.push(handle);
        }
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

    async fn shutdown(&self) -> Result<(), TransportError> {
        // RF-45-d: abort all in-flight delivery tasks so messages sent before
        // shutdown() never arrive after it.
        for handle in self.pending_deliveries.lock().unwrap().drain(..) {
            handle.abort();
        }
        self.set_state(TransportState::Unavailable);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn connect_pair_delivers_cross_transport() {
        let cfg = SimConfig { latency_base_ms: 0, latency_spread_ms: 0, ..Default::default() };
        let a = SimulatedTransport::new("sim-a", "A", cfg.clone());
        let b = SimulatedTransport::new("sim-b", "B", cfg);
        SimulatedTransport::connect_pair(&a, &b);

        let mut in_b = b.incoming_messages();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([1u8; 16]),
            priority: MessagePriority::P3,
            payload: b"hello-from-a".to_vec(),
        };
        a.send(&PeerId([2u8; 32]), &msg).await.unwrap();
        let got = tokio::time::timeout(Duration::from_millis(200), in_b.next()).await;
        let im = got.expect("timed out").expect("stream closed");
        assert_eq!(im.payload, b"hello-from-a");
        // sender attribution must be A's derived peer id, not the destination id
        assert_eq!(&im.peer_id.0[..5], b"sim-a");
    }

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
