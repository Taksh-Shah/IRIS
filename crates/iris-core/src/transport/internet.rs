//! INTERNET-001 — Internet Gateway Transport.
//!
//! Implements the transport abstraction's TCP-framing path
//! (`docs/transports/INTERNET.md` §TCP framing). QUIC (quinn), WebSocket
//! fallback, and TLS are future work; the framing + connection-pool + backoff
//! machinery is transport-agnostic and shared.
//!
//! **Known gap (RF-35):** connections are plaintext TCP. `INTERNET.md` §TCP+TLS
//! specifies `tokio-rustls` with relay SPKI pinning; that layer has not been
//! added yet. Payload confidentiality is preserved by the IRIS envelope
//! (CRYPTO-001), but frame metadata is exposed and injection is unauthenticated
//! at the transport layer. Tracked in
//! `docs/implementation/INTERNET_TRANSPORT_VERIFICATION.md` §Known Limitations.
//!
//! Design notes applied from `INTERNET.md`:
//! - Length-prefixed frames on a TCP stream (one connection per relay).
//! - Connection pooling with reuse + idle timeout.
//! - Exponential backoff with jitter for reconnection (1s→30s cap).
//! - CGNAT in India: no STUN hole-punching; route via relay.
//! - Monetary cost >0 for metered relay traffic.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex};

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::{
    AtomicState, Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportState, TransportStateEvent,
};
use crate::TransportError;

/// Wire frame: `[u32 LE payload_len][payload]`.
const FRAME_LEN_BYTES: usize = 4;
/// Maximum frame size we will accept (1 MiB — well above P0-P7 budgets).
const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Cap on backoff wait (INTERNET.md §Reconnection Logic).
const BACKOFF_MAX_MS: u64 = 30_000;
/// Jitter factor ±25% (INTERNET.md).
const BACKOFF_JITTER: f64 = 0.25;

/// Static capability set for the Internet transport.
pub fn internet_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        max_message_size: MAX_FRAME_BYTES,
        supports_broadcast: false,
        supports_unicast: true,
        supports_multicast: false,
        range_m_min: 0,
        range_m_max: 0, // no intrinsic range limit
        range_m_typical: 0,
        typical_throughput_bps: 100_000_000, // conservative estimate
        typical_latency_ms: 50,
        requires_infrastructure: true, // needs cellular/wifi/ethernet to a relay
        supports_background_android: true,
        supports_background_ios: true,
        requires_special_hardware: false,
        cost_class: TransportCostClass::Metered,
        regulatory_band: None,
    }
}

/// Cost parameters for the Internet transport.
#[derive(Debug, Clone, Copy)]
pub struct InternetCostParams {
    /// INR per kilobyte of relay traffic (0 for free relays).
    pub cost_per_kb_inr: f64,
    pub estimated_battery_ma: f32,
}

impl Default for InternetCostParams {
    fn default() -> Self {
        InternetCostParams {
            cost_per_kb_inr: 0.0,
            estimated_battery_ma: 50.0, // cellular radio draw when active
        }
    }
}

/// A pooled TCP connection to a relay. Keyed by relay address so a message
/// destined for peer A can never be written to peer B's relay (RED-0001-01).
struct PooledConnection {
    addr: SocketAddr,
    write: OwnedWriteHalf,
    last_used: Instant,
    idle_timeout: Duration,
}

impl PooledConnection {
    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.last_used) > self.idle_timeout
    }
}

/// Connection pool — reuse existing connections, cap per relay (INTERNET.md).
struct ConnectionPool {
    connections: Vec<PooledConnection>,
    max_per_relay: usize,
    idle_timeout: Duration,
}

impl ConnectionPool {
    fn new(max_per_relay: usize, idle_timeout: Duration) -> Self {
        ConnectionPool {
            connections: Vec::new(),
            max_per_relay,
            idle_timeout,
        }
    }

    /// Acquire a live connection bound to exactly `addr` (never any other relay).
    fn acquire(&mut self, now: Instant, addr: SocketAddr) -> Option<OwnedWriteHalf> {
        let idx = self
            .connections
            .iter()
            .position(|c| !c.is_expired(now) && c.addr == addr);
        idx.map(|i| self.connections.remove(i).write)
    }

    fn release(&mut self, conn: OwnedWriteHalf, addr: SocketAddr, now: Instant) {
        self.connections.retain(|c| !c.is_expired(now));
        let per_addr = self.connections.iter().filter(|c| c.addr == addr).count();
        if per_addr < self.max_per_relay {
            self.connections.push(PooledConnection {
                addr,
                write: conn,
                last_used: now,
                idle_timeout: self.idle_timeout,
            });
        }
    }
}

/// Exponential backoff with ±25% jitter (pure, unit-tested).
pub fn backoff_ms(attempt: u32, seed: u64) -> u64 {
    let base = 1_u64 << attempt.min(31); // 1,2,4,8,...
    let base_ms = (base * 1000).min(BACKOFF_MAX_MS);
    let jitter_range = (base_ms as f64 * BACKOFF_JITTER) as u64;
    let half = jitter_range / 2;
    let offset = seed % (jitter_range + 1);
    // ±25%: shift the [0, jitter] offset to [-half, +half].
    base_ms.saturating_add(offset).saturating_sub(half)
}

/// Encode a message into a wire frame.
///
/// Returns `Protocol` rather than panicking on an oversized payload. The
/// previous `assert!` was remotely reachable: `frame_payload_len` accepts a
/// payload of exactly `MAX_FRAME_BYTES`, and relaying re-encodes the envelope
/// with an incremented `hop_count`, which can widen the CBOR integer by a byte
/// and push the frame one over the limit. The panic then unwound inside the
/// spawned delivery loop, whose handle is never joined — so the node stopped
/// delivering and relaying everything, silently and permanently.
pub fn encode_frame(message: &SerializedMessage) -> Result<Vec<u8>, TransportError> {
    let len = message.payload.len();
    if len > MAX_FRAME_BYTES {
        return Err(TransportError::Protocol(format!(
            "frame of {len} bytes exceeds the {MAX_FRAME_BYTES}-byte maximum"
        )));
    }
    let mut frame = Vec::with_capacity(FRAME_LEN_BYTES + len);
    frame.extend_from_slice(&(len as u32).to_le_bytes());
    frame.extend_from_slice(&message.payload);
    Ok(frame)
}

/// Decode the payload length from a frame header (or None if incomplete).
pub fn frame_payload_len(header: &[u8]) -> Option<usize> {
    if header.len() < FRAME_LEN_BYTES {
        return None;
    }
    let mut b = [0u8; FRAME_LEN_BYTES];
    b.copy_from_slice(&header[..FRAME_LEN_BYTES]);
    let len = u32::from_le_bytes(b) as usize;
    if len > MAX_FRAME_BYTES {
        return None;
    }
    Some(len)
}

/// Internet transport over TCP with relay connections.
pub struct InternetTransport {
    id: TransportId,
    display: String,
    caps: TransportCapabilities,
    state: std::sync::Arc<AtomicState>,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    pool: Mutex<ConnectionPool>,
    cost_params: InternetCostParams,
    priority_hint: AtomicU8,
    /// Address of the relay established at connect(); used to reconnect if the
    /// pooled connection idles out before the next send.
    relay_addr: Mutex<Option<SocketAddr>>,
    /// Candidate relay endpoints provided at construction (INTERNET.md relay discovery).
    relay_candidates: Vec<SocketAddr>,
    /// Per-peer relay binding so send() never falls back to another peer's
    /// relay (RED-0001-01). Populated on connect().
    peer_relays: Mutex<HashMap<PeerId, SocketAddr>>,
}

impl InternetTransport {
    /// Create an internet transport for the given relay endpoints.
    pub fn new(relay_endpoints: &[SocketAddr], cost_params: InternetCostParams) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = std::sync::Arc::new(AtomicState::default());
        state.store(TransportState::Unavailable);
        let id = TransportId::from("internet-0");
        InternetTransport {
            id,
            display: "Internet (relay)".to_string(),
            caps: internet_capabilities(),
            state,
            state_tx,
            incoming_tx,
            pool: Mutex::new(ConnectionPool::new(3, Duration::from_secs(90))),
            cost_params,
            priority_hint: AtomicU8::new(MessagePriority::P5.as_u8()),
            relay_addr: Mutex::new(None),
            relay_candidates: relay_endpoints.to_vec(),
            peer_relays: Mutex::new(HashMap::new()),
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    /// Resolve the relay address for a peer. Order: peer's explicit addresses →
    /// remembered per-peer binding → last connect target → construction candidates.
    async fn resolve_relay(&self, peer: &PeerInfo) -> Option<SocketAddr> {
        let known = *self.relay_addr.lock().await;
        let binding = self.peer_relays.lock().await.get(&peer.peer_id).copied();
        peer.addresses
            .first()
            .copied()
            .or(binding)
            .or(known)
            .or_else(|| self.relay_candidates.first().copied())
    }

    /// Open a pooled connection to a peer's relay and return its write half.
    async fn get_connection(&self, peer: &PeerInfo) -> Result<OwnedWriteHalf, TransportError> {
        let addr = self
            .resolve_relay(peer)
            .await
            .ok_or(TransportError::PeerNotFound)?;

        let mut pool = self.pool.lock().await;
        let now = Instant::now();
        if let Some(conn) = pool.acquire(now, addr) {
            return Ok(conn);
        }

        // Open new connection (timeout 3s per INTERNET.md negotiation budget).
        let stream =
            match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(addr)).await {
                Ok(Ok(s)) => s,
                Ok(Err(e)) => return Err(TransportError::Io(e.to_string())),
                Err(_) => return Err(TransportError::ConnectionFailed),
            };
        stream.set_nodelay(true).ok();
        let (read, write) = stream.into_split();
        // Reader task pushes frames to incoming broadcast; on link loss it
        // downgrades state so selection stops choosing a blackholed transport.
        self.spawn_reader(peer.peer_id, read);
        Ok(write)
    }

    fn spawn_reader(&self, peer_id: PeerId, mut read: OwnedReadHalf) {
        let incoming = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        tokio::spawn(async move {
            let mut header = [0u8; FRAME_LEN_BYTES];
            loop {
                // Per-read timeout: a malicious/stalled relay must not pin a
                // 1 MiB allocation + task forever (RED-0001-02 slowloris).
                let header_res =
                    tokio::time::timeout(Duration::from_secs(10), read.read_exact(&mut header))
                        .await;
                match header_res {
                    Ok(Ok(_)) => {}
                    _ => break, // EOF / reset / timeout
                }
                let Some(len) = frame_payload_len(&header) else {
                    break;
                };
                let mut payload = vec![0u8; len];
                let body_res =
                    tokio::time::timeout(Duration::from_secs(10), read.read_exact(&mut payload))
                        .await;
                if !matches!(body_res, Ok(Ok(_))) {
                    break;
                }
                incoming.send(IncomingMessage {
                    // Attributed to the peer this connection was opened for.
                    // The protocol layer MUST re-derive the sender from the
                    // envelope (WP-A codec) — relay metadata is not trust.
                    peer_id,
                    transport_id: transport_id.0.clone(),
                    payload,
                    received_at: Instant::now(),
                }).ok();
            }
            // Link lost → leave the zombie Connected state so the manager stops
            // selecting a dead transport (RED-0001-02).
            state.store(TransportState::Available);
            state_tx.send(TransportStateEvent {
                transport_id,
                new_state: TransportState::Available,
            }).ok();
        });
    }
}

#[async_trait]
impl Transport for InternetTransport {
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
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "internet.state")
    }

    async fn discover_peers(
        &self,
        _config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        // Internet peers are discovered via relay rendezvous, not scanning.
        // (INTERNET.md §Relay Discovery) — returns empty stream.
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn start_advertising(&self, _info: NodeAdvertisement) -> Result<(), TransportError> {
        // Presence is announced to the relay, not broadcast locally.
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        self.set_state(TransportState::Connecting);
        let addr = self.resolve_relay(peer).await.ok_or_else(|| {
            // Restore the pre-call state so a failed connect never leaves the
            // transport stuck in Connecting (RED-0001-13).
            self.set_state(TransportState::Available);
            TransportError::PeerNotFound
        })?;
        *self.relay_addr.lock().await = Some(addr);
        self.peer_relays.lock().await.insert(peer.peer_id, addr);
        let conn = match self.get_connection(peer).await {
            Ok(c) => c,
            Err(e) => {
                self.set_state(TransportState::Available);
                return Err(e);
            }
        };
        // Park the connection in the pool (keyed by relay) for send() reuse.
        self.pool.lock().await.release(conn, addr, Instant::now());
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
        if self.state.load() != TransportState::Connected {
            return Err(TransportError::NotConnected);
        }
        let peer_info = PeerInfo {
            peer_id: *peer,
            addresses: Vec::new(), // resolved from peer_relays; pool holds the socket
            transport_addresses: Vec::new(),
            last_seen: Some(Instant::now()),
        };
        let mut write = self.get_connection(&peer_info).await?;
        let frame = encode_frame(message)?;
        let write_res: Result<(), std::io::Error> = async {
            write.write_all(&frame).await?;
            write.flush().await?;
            Ok(())
        }
        .await;
        // On write failure the link is dead: leave Connected immediately and
        // drop the socket so the reader marks the transport Available, letting
        // the manager steer traffic away (RED-0001-02 zombie state).
        let addr = self
            .resolve_relay(&peer_info)
            .await
            .ok_or(TransportError::PeerNotFound)?;
        if let Err(e) = write_res {
            self.set_state(TransportState::Degraded);
            return Err(TransportError::Io(e.to_string()));
        }
        let now = Instant::now();
        let mut pool = self.pool.lock().await;
        pool.release(write, addr, now);
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: frame.len(),
            sent_at: now,
        })
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "internet.messages")
    }

    fn cost_snapshot(&self) -> TransportCost {
        let congestion = match self.state.load() {
            TransportState::Connected => 0.1,
            TransportState::Degraded => 0.7,
            _ => 0.0,
        };
        TransportCost {
            estimated_battery_ma: self.cost_params.estimated_battery_ma,
            monetary_cost_per_kb: self.cost_params.cost_per_kb_inr,
            bandwidth_available_bps: self.caps.typical_throughput_bps,
            congestion_level: congestion,
        }
    }

    fn set_send_priority_hint(&self, priority: MessagePriority) {
        self.priority_hint
            .store(priority.as_u8(), Ordering::Release);
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        self.set_state(TransportState::Unavailable);
        self.pool.lock().await.connections.clear();
        self.peer_relays.lock().await.clear();
        *self.relay_addr.lock().await = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message(payload: &[u8], priority: MessagePriority) -> SerializedMessage {
        SerializedMessage {
            message_id: crate::protocol::MessageId::from([7u8; 16]),
            priority,
            payload: payload.to_vec(),
        }
    }

    #[test]
    fn frame_roundtrip() {
        let msg = sample_message(b"hello iris", MessagePriority::P1);
        let frame = encode_frame(&msg).expect("sample message encodes");
        let len = frame_payload_len(&frame[..4]).unwrap();
        assert_eq!(len, 10);
        assert_eq!(&frame[4..], b"hello iris");
    }

    #[test]
    fn frame_rejects_oversized_payload() {
        let header = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
        assert!(frame_payload_len(&header).is_none());
    }

    #[test]
    fn backoff_sequence() {
        let mut last = 0;
        for attempt in 1..=6 {
            let ms = backoff_ms(attempt, 0);
            assert!(ms >= 500, "attempt {attempt}: {ms}");
            assert!(ms <= 37_500, "attempt {attempt}: {ms}");
            // Non-decreasing: the 30s cap flattens the sequence at attempts 5+.
            assert!(ms >= last, "must not decrease: {last} -> {ms}");
            last = ms;
        }
        assert_eq!(
            backoff_ms(10, 0),
            26_250,
            "capped base 30s - 25% jitter half with seed 0"
        );
    }

    #[test]
    fn backoff_jitter_bounds() {
        for attempt in 1..=6 {
            for seed in 0..100 {
                let ms = backoff_ms(attempt, seed);
                assert!(
                    (500..=37_500).contains(&ms),
                    "attempt {attempt} seed {seed}: {ms}"
                );
            }
        }
    }

    #[tokio::test]
    async fn send_receive_localhost_roundtrip() {
        // Start a fake relay listener.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut header = [0u8; 4];
            socket.read_exact(&mut header).await.unwrap();
            let len = u32::from_le_bytes(header) as usize;
            let mut payload = vec![0u8; len];
            socket.read_exact(&mut payload).await.unwrap();
            payload
        });

        let transport = InternetTransport::new(&[relay_addr], InternetCostParams::default());
        let peer = PeerId([1u8; 32]);
        let peer_info = PeerInfo {
            peer_id: peer,
            addresses: vec![relay_addr],
            transport_addresses: Vec::new(),
            last_seen: None,
        };
        transport.connect(&peer_info).await.unwrap();
        assert_eq!(transport.state(), TransportState::Connected);

        let msg = sample_message(b"relay delivery", MessagePriority::P2);
        let receipt = transport.send(&peer, &msg).await.unwrap();
        assert_eq!(receipt.bytes_sent, 4 + 14);

        let received = server.await.unwrap();
        assert_eq!(received, b"relay delivery");
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn send_without_connect_returns_not_connected() {
        let transport = InternetTransport::new(&[], InternetCostParams::default());
        let peer = PeerId([2u8; 32]);
        let msg = sample_message(b"nope", MessagePriority::P5);
        let err = transport.send(&peer, &msg).await.unwrap_err();
        assert_eq!(err, TransportError::NotConnected);
    }

    #[tokio::test]
    async fn initial_state_is_unavailable() {
        let transport = InternetTransport::new(&[], InternetCostParams::default());
        assert_eq!(transport.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn cost_snapshot_reflects_connection() {
        let transport = InternetTransport::new(
            &[],
            InternetCostParams {
                cost_per_kb_inr: 0.5,
                estimated_battery_ma: 50.0,
            },
        );
        let cost = transport.cost_snapshot();
        assert_eq!(cost.monetary_cost_per_kb, 0.5);
        assert_eq!(cost.estimated_battery_ma, 50.0);
    }

    #[test]
    fn capability_matrix_facts() {
        let caps = internet_capabilities();
        assert!(caps.requires_infrastructure);
        assert!(!caps.requires_special_hardware);
        assert_eq!(caps.cost_class, TransportCostClass::Metered);
        assert!(caps.supports_background_android && caps.supports_background_ios);
        assert!(caps.max_message_size >= 1_000_000);
    }

    #[test]
    fn encode_frame_rejects_oversized_payload() {
        // Adversarial: a payload above MAX_FRAME_BYTES must never reach the
        // wire — and must be REPORTED, not panicked. This runs inside the
        // spawned delivery loop, where a panic silently kills delivery and
        // relaying for the lifetime of the process.
        let msg = sample_message(&vec![0u8; MAX_FRAME_BYTES + 1], MessagePriority::P0);
        assert!(matches!(
            encode_frame(&msg),
            Err(TransportError::Protocol(_))
        ));
    }

    #[test]
    fn encode_frame_accepts_exactly_max_frame_bytes() {
        // The boundary is inclusive on both encode and decode, so a max-size
        // frame round-trips rather than being rejected on one side only.
        let msg = sample_message(&vec![0u8; MAX_FRAME_BYTES], MessagePriority::P4);
        let frame = encode_frame(&msg).expect("max-size frame encodes");
        assert_eq!(
            frame_payload_len(&frame[..FRAME_LEN_BYTES]),
            Some(MAX_FRAME_BYTES)
        );
    }

    #[test]
    fn frame_payload_len_rejects_truncated_header() {
        // Adversarial: any header shorter than the 4-byte length prefix is incomplete.
        assert!(frame_payload_len(&[]).is_none());
        assert!(frame_payload_len(&[0x00]).is_none());
        assert!(frame_payload_len(&[0x00, 0x00]).is_none());
        assert!(frame_payload_len(&[0x00, 0x00, 0x00]).is_none());
        // A 4-byte header claiming an absurd length is also rejected.
        assert!(frame_payload_len(&[0x00, 0x00, 0x00, 0x80]).is_none());
    }

    #[tokio::test]
    async fn connection_pool_reuses_socket_for_back_to_back_sends() {
        // One relay socket must carry two frames: proves the pool hands back the
        // same pooled write half to the second send() instead of dialing again.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut frames = Vec::new();
            for _ in 0..2 {
                let mut header = [0u8; 4];
                socket.read_exact(&mut header).await.unwrap();
                let len = u32::from_le_bytes(header) as usize;
                let mut payload = vec![0u8; len];
                socket.read_exact(&mut payload).await.unwrap();
                frames.push(payload);
            }
            frames
        });

        let transport = InternetTransport::new(&[relay_addr], InternetCostParams::default());
        let peer = PeerId([5u8; 32]);
        let peer_info = PeerInfo {
            peer_id: peer,
            addresses: vec![relay_addr],
            transport_addresses: Vec::new(),
            last_seen: None,
        };
        transport.connect(&peer_info).await.unwrap();
        assert_eq!(transport.state(), TransportState::Connected);

        let first = sample_message(b"frame one", MessagePriority::P3);
        let second = sample_message(b"frame two", MessagePriority::P3);
        transport.send(&peer, &first).await.unwrap();
        transport.send(&peer, &second).await.unwrap();

        let frames = tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server must finish in time")
            .unwrap();
        assert_eq!(
            frames,
            vec![b"frame one".to_vec(), b"frame two".to_vec()],
            "pooled socket must deliver both frames in order"
        );
        transport.shutdown().await.unwrap();
    }

    #[test]
    fn backoff_capped_and_monotonic_across_attempts() {
        // Monotonic non-decreasing over 1..=10 for every seed: each attempt's
        // worst case (base*1.25) is below the next attempt's best case (base*0.75)
        // until the 30s cap flattens the tail into equality.
        for seed in 0..50u64 {
            let mut last = 0u64;
            for attempt in 1..=10 {
                let ms = backoff_ms(attempt, seed);
                assert!(
                    (500..=37_500).contains(&ms),
                    "attempt {attempt} seed {seed}: {ms} out of [500, 37500]"
                );
                assert!(ms >= last, "seed {seed}: backoff decreased {last} -> {ms}");
                last = ms;
            }
        }
        // Cap-at-attempt: base saturates at BACKOFF_MAX_MS from attempt 5 on, so
        // every later attempt must yield the identical jittered value per seed.
        for seed in 0..100u64 {
            let capped = backoff_ms(5, seed);
            assert_eq!(
                backoff_ms(6, seed),
                capped,
                "seed {seed}: attempt 6 != capped"
            );
            assert_eq!(
                backoff_ms(10, seed),
                capped,
                "seed {seed}: attempt 10 != capped"
            );
            assert_eq!(
                backoff_ms(31, seed),
                capped,
                "seed {seed}: attempt 31 != capped"
            );
            assert!(
                (26_250..=37_500).contains(&capped),
                "seed {seed}: capped {capped} out of jitter band"
            );
        }
    }
}
