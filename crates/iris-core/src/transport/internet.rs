//! INTERNET-001 — Internet Gateway Transport.
//!
//! Implements the transport abstraction's TCP-framing path
//! (`docs/transports/INTERNET.md` §TCP framing). QUIC (quinn) and WebSocket
//! fallback remain future work; TLS is wired for the relay path.
//!
//! Two operating modes, selected at runtime via [`InternetTransport::set_relay_mode`]
//! or [`InternetTransport::set_plain_mode`]:
//!
//! - **Plain (LAN)**: plaintext TCP, simple `[version][len][payload]` frames,
//!   direct peer-to-peer.  NSD-discovered local endpoints use this path.
//!   A local [`TcpListener`](tokio::net::TcpListener) bound via
//!   [`InternetTransport::start_lan_listener`] accepts inbound LAN peers.
//! - **Relay (WAN)**: TLS (tokio-rustls/WebPKI), `RelayFrame` envelope
//!   (relay_protocol module), sessions registered with the relay on connect.
//!   Payload confidentiality is the IRIS envelope (CRYPTO-001); the relay only
//!   sees source/target PeerIds in the outer relay header.
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
use std::sync::atomic::{AtomicU16, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, Mutex, RwLock};

use crate::transport::relay_protocol::{self, RelayFrame, RELAY_HEADER_LEN};
use crate::transport::tls::{relay_server_name as parse_relay_server_name, verified_relay_connector};

/// Owned, heap-allocated asynchronous write half — used for both plain TCP
/// (`OwnedWriteHalf`) and TLS (`WriteHalf<TlsStream<TcpStream>>`).
type BoxWriter = Box<dyn tokio::io::AsyncWrite + Send + Unpin>;
/// Owned, heap-allocated asynchronous read half.
type BoxReader = Box<dyn tokio::io::AsyncRead + Send + Unpin>;

/// Selects the wire framing and security layer for an [`InternetTransport`] instance.
#[derive(Debug, Clone)]
enum ConnectionMode {
    /// Direct LAN connection — plain TCP, simple `[version][len][payload]` frames.
    Plain,
    /// Relay connection — TLS, `RelayFrame` envelope, peer registration on connect.
    Relay {
        server_name: String,
        own_peer_id: [u8; 32],
    },
}

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::{
    AtomicState, EwmaGoodput, Transport, TransportCapabilities, TransportCost, TransportCostClass,
    TransportId, TransportState, TransportStateEvent,
};
use crate::TransportError;

/// GAP-5: version byte prepended to every frame so future format changes can be
/// rejected cleanly rather than mis-parsed.
pub const FRAME_VERSION: u8 = 1;
/// Wire frame: `[u8 version][u32 LE payload_len][payload]`.
const FRAME_LEN_BYTES: usize = 4;
pub const FRAME_HEADER_LEN: usize = 1 + FRAME_LEN_BYTES; // version + length prefix
/// Maximum frame size we will accept (1 MiB — well above P0-P7 budgets).
const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Cap on backoff wait (INTERNET.md §Reconnection Logic).
const BACKOFF_MAX_MS: u64 = 30_000;
/// Jitter factor ±25% (INTERNET.md).
const BACKOFF_JITTER: f64 = 0.25;

/// Static capability set for the Internet transport.
pub fn internet_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        // PS-7 fix: report payload capacity (frame minus 5-byte header), not total frame size.
        // lora.rs reports MAX_PAYLOAD_BYTES = 237 (payload only); both now report the same
        // semantic: max bytes of IRIS message content, excluding the wire frame header.
        max_message_size: MAX_FRAME_BYTES - FRAME_HEADER_LEN,
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
        cost_class: TransportCostClass::Metered {
            cost_per_kb_inr: 0.0,
        },
        regulatory_band: None,
        conflict_group: crate::transport::RadioConflictGroup::None,
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

/// A pooled TCP connection to a relay or LAN peer. Keyed by address so a
/// message destined for peer A can never be written to peer B's relay
/// (RED-0001-01).
struct PooledConnection {
    addr: SocketAddr,
    write: Option<BoxWriter>,
    /// Abort handle for the paired reader task. Taken out on acquire (so the
    /// reader keeps running while the write half is in use); aborted on Drop
    /// so over-cap and expired evictions kill the orphaned reader task.
    reader_abort: Option<tokio::task::AbortHandle>,
    last_used: Instant,
    idle_timeout: Duration,
}

impl PooledConnection {
    fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.last_used) > self.idle_timeout
    }
}

impl Drop for PooledConnection {
    fn drop(&mut self) {
        if let Some(h) = self.reader_abort.take() {
            h.abort();
        }
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
    /// Reaps expired entries first — their Drop aborts orphaned reader tasks.
    fn acquire(
        &mut self,
        now: Instant,
        addr: SocketAddr,
    ) -> Option<(BoxWriter, tokio::task::AbortHandle)> {
        self.connections.retain(|c| !c.is_expired(now));
        let idx = self.connections.iter().position(|c| c.addr == addr);
        idx.map(|i| {
            let mut conn = self.connections.remove(i);
            let write = conn.write.take().expect("write always present in pool");
            let abort = conn
                .reader_abort
                .take()
                .expect("abort always present in pool");
            (write, abort)
        })
    }

    fn release(
        &mut self,
        write: BoxWriter,
        reader_abort: tokio::task::AbortHandle,
        addr: SocketAddr,
        now: Instant,
    ) {
        self.connections.retain(|c| !c.is_expired(now));
        let per_addr = self.connections.iter().filter(|c| c.addr == addr).count();
        let conn = PooledConnection {
            addr,
            write: Some(write),
            reader_abort: Some(reader_abort),
            last_used: now,
            idle_timeout: self.idle_timeout,
        };
        if per_addr < self.max_per_relay {
            self.connections.push(conn);
        }
        // else: conn drops here; Drop aborts the orphaned reader task.
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
        // MG-40: this is resolvable by fragmenting upstream and retrying on
        // this same transport — not a framing/decode failure.
        return Err(TransportError::MessageTooLarge {
            limit: MAX_FRAME_BYTES,
            actual: len,
        });
    }
    let mut frame = Vec::with_capacity(FRAME_HEADER_LEN + len);
    frame.push(FRAME_VERSION); // GAP-5: version byte first
    frame.extend_from_slice(&(len as u32).to_le_bytes());
    frame.extend_from_slice(&message.payload);
    Ok(frame)
}

/// Decode the payload length from a frame header (or None if incomplete/invalid).
///
/// RF-41: returns `None` for zero-length frames (all call sites should treat them
/// as malformed; previously two of three guarded `len == 0` independently).
pub fn frame_payload_len(header: &[u8]) -> Option<usize> {
    if header.len() < FRAME_HEADER_LEN {
        return None;
    }
    // GAP-5: reject unknown versions cleanly rather than mis-parsing them.
    if header[0] != FRAME_VERSION {
        return None;
    }
    let mut b = [0u8; FRAME_LEN_BYTES];
    b.copy_from_slice(&header[1..FRAME_HEADER_LEN]);
    let len = u32::from_le_bytes(b) as usize;
    if len == 0 || len > MAX_FRAME_BYTES {
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
    /// Candidate relay endpoints supplied by configuration or platform discovery.
    /// This is mutable because Android NSD may resolve a relay after engine start.
    relay_candidates: RwLock<Vec<SocketAddr>>,
    /// Connectivity is platform-owned. A configured relay alone must not make
    /// this transport selectable while Android reports no validated network.
    network_available: std::sync::atomic::AtomicBool,
    has_relay_endpoint: std::sync::atomic::AtomicBool,
    /// Per-peer relay binding so send() never falls back to another peer's
    /// relay (RED-0001-01). Populated on connect().
    peer_relays: Mutex<HashMap<PeerId, SocketAddr>>,
    /// Abort handles for active reader tasks, so shutdown() can stop them before
    /// they resurrect the transport by writing Available on EOF (RF-37).
    readers: StdMutex<Vec<tokio::task::AbortHandle>>,
    /// Consecutive failed dial attempts; drives backoff_ms() in get_connection (RF-38).
    reconnect_attempts: AtomicU32,
    /// When the last dial failure occurred; the gate refuses to dial earlier than
    /// backoff_ms(reconnect_attempts, jitter_seed) after this instant (RF-38).
    last_attempt: StdMutex<Option<Instant>>,
    /// Per-instance jitter seed — derived at construction from SystemTime so
    /// concurrent nodes de-correlate their reconnect storms (RF-38).
    jitter_seed: u64,
    /// MG-17: EWMA goodput tracker — updated on every successful send so
    /// cost_snapshot() returns a live bandwidth estimate rather than the
    /// compile-time constant that was there before.
    ewma: EwmaGoodput,
    /// Connection mode: Plain (LAN, plain TCP) or Relay (WAN, TLS + RelayFrame).
    mode: StdMutex<ConnectionMode>,
    /// Bound LAN listener port (0 = not started). Set once by start_lan_listener().
    lan_listener_port: AtomicU16,
    /// Abort handle for the LAN accept loop (shutdown() tears it down).
    lan_listener_abort: StdMutex<Option<tokio::task::AbortHandle>>,
}

impl InternetTransport {
    /// Create an internet transport for the given relay endpoints.
    ///
    /// Default mode is `Plain` (LAN, plain TCP). Call [`set_relay_mode`] to
    /// switch to TLS relay mode after construction.
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
            relay_candidates: RwLock::new(relay_endpoints.to_vec()),
            // Non-Android callers historically construct this transport with a
            // concrete endpoint and no platform callback. Preserve that usable
            // core default; Android constructs it empty and explicitly supplies
            // its validated-network signal before configuration.
            network_available: std::sync::atomic::AtomicBool::new(!relay_endpoints.is_empty()),
            has_relay_endpoint: std::sync::atomic::AtomicBool::new(!relay_endpoints.is_empty()),
            peer_relays: Mutex::new(HashMap::new()),
            readers: StdMutex::new(Vec::new()),
            reconnect_attempts: AtomicU32::new(0),
            last_attempt: StdMutex::new(None),
            jitter_seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as u64,
            ewma: EwmaGoodput::new(),
            mode: StdMutex::new(ConnectionMode::Plain),
            lan_listener_port: AtomicU16::new(0),
            lan_listener_abort: StdMutex::new(None),
        }
    }

    /// Switch to relay mode: TLS-encrypted connections to a relay server, using
    /// `RelayFrame` framing. `server_name` is the expected TLS SNI/SAN of the
    /// relay (DNS name or IP literal). `own_peer_id` is this node's identity,
    /// sent in the `Register` frame on every new relay connection.
    pub fn set_relay_mode(&self, server_name: String, own_peer_id: [u8; 32]) {
        if let Ok(mut guard) = self.mode.lock() {
            *guard = ConnectionMode::Relay {
                server_name,
                own_peer_id,
            };
        }
    }

    /// Switch back to plain TCP mode (LAN direct).
    pub fn set_plain_mode(&self) {
        if let Ok(mut guard) = self.mode.lock() {
            *guard = ConnectionMode::Plain;
        }
    }

    /// Read the current connection mode.
    fn current_mode(&self) -> ConnectionMode {
        self.mode
            .lock()
            .map(|g| g.clone())
            .unwrap_or(ConnectionMode::Plain)
    }

    /// Start a plain TCP listener on an OS-assigned port for inbound LAN
    /// connections. Returns the bound port. Idempotent — a second call returns
    /// the already-bound port without starting a second listener.
    pub async fn start_lan_listener(&self) -> Result<u16, TransportError> {
        let existing = self.lan_listener_port.load(Ordering::Acquire);
        if existing != 0 {
            return Ok(existing);
        }
        let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
            .await
            .map_err(|e| TransportError::Io { kind: e.kind(), msg: e.to_string() })?;
        let port = listener
            .local_addr()
            .map_err(|e| TransportError::Io { kind: e.kind(), msg: e.to_string() })?
            .port();
        self.lan_listener_port.store(port, Ordering::Release);

        let incoming_tx = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        let accept_loop = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _peer_addr)) => {
                        stream.set_nodelay(true).ok();
                        let (read, _write) = stream.into_split();
                        let incoming = incoming_tx.clone();
                        let tid = transport_id.clone();
                        tokio::spawn(Self::plain_reader_task(
                            PeerId([0u8; 32]), // identity derived from envelope
                            Box::new(read),
                            incoming,
                            tid,
                        ));
                    }
                    Err(_) => break,
                }
            }
            // Listener closed; transition to Available if not already shut down.
            if state.load() != TransportState::Unavailable {
                state.store(TransportState::Available);
                state_tx
                    .send(TransportStateEvent {
                        transport_id,
                        new_state: TransportState::Available,
                    })
                    .ok();
            }
        });
        if let Ok(mut guard) = self.lan_listener_abort.lock() {
            *guard = Some(accept_loop.abort_handle());
        }
        Ok(port)
    }

    /// The LAN listener's bound port, or 0 if not started.
    pub fn lan_listener_port(&self) -> u16 {
        self.lan_listener_port.load(Ordering::Acquire)
    }

    /// Read loop for a plain TCP connection (LAN mode). Reads
    /// `[version][len32][payload]` frames until the peer closes.
    async fn plain_reader_task(
        peer_id: PeerId,
        mut read: BoxReader,
        incoming: broadcast::Sender<IncomingMessage>,
        transport_id: TransportId,
    ) {
        let mut header = [0u8; FRAME_HEADER_LEN];
        loop {
            let header_res =
                tokio::time::timeout(Duration::from_secs(10), read.read_exact(&mut header)).await;
            match header_res {
                Ok(Ok(_)) => {}
                Err(_) => {
                    tracing::warn!(peer = ?peer_id, "plain reader: header timeout");
                    break;
                }
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::debug!(peer = ?peer_id, "plain reader: EOF");
                    break;
                }
                Ok(Err(e)) => {
                    tracing::warn!(peer = ?peer_id, err = %e, "plain reader: TCP error");
                    break;
                }
            }
            let Some(len) = frame_payload_len(&header) else {
                break;
            };
            let mut payload = vec![0u8; len];
            if !matches!(
                tokio::time::timeout(Duration::from_secs(10), read.read_exact(&mut payload)).await,
                Ok(Ok(_))
            ) {
                break;
            }
            incoming
                .send(IncomingMessage {
                    peer_id,
                    transport_id: transport_id.0.clone(),
                    payload,
                    received_at: Instant::now(),
                })
                .ok();
        }
    }

    /// Read loop for a relay TLS connection. Reads `RelayFrame` envelopes;
    /// emits `Route` payloads as `IncomingMessage`, silently swallows `Heartbeat`.
    async fn relay_reader_task(
        peer_id: PeerId,
        mut read: BoxReader,
        incoming: broadcast::Sender<IncomingMessage>,
        transport_id: TransportId,
    ) {
        let mut header = [0u8; RELAY_HEADER_LEN];
        loop {
            let header_res =
                tokio::time::timeout(Duration::from_secs(10), read.read_exact(&mut header)).await;
            match header_res {
                Ok(Ok(_)) => {}
                Err(_) => {
                    tracing::warn!(peer = ?peer_id, "relay reader: header timeout");
                    break;
                }
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    tracing::debug!(peer = ?peer_id, "relay reader: EOF");
                    break;
                }
                Ok(Err(e)) => {
                    tracing::warn!(peer = ?peer_id, err = %e, "relay reader: TCP error");
                    break;
                }
            }
            // Extract payload length from header bytes 66..70.
            let mut len_bytes = [0u8; 4];
            len_bytes.copy_from_slice(&header[66..70]);
            let payload_len = u32::from_le_bytes(len_bytes) as usize;
            if payload_len > relay_protocol::MAX_RELAY_PAYLOAD {
                tracing::warn!("relay reader: oversized payload {payload_len}");
                break;
            }
            let mut full = vec![0u8; RELAY_HEADER_LEN + payload_len];
            full[..RELAY_HEADER_LEN].copy_from_slice(&header);
            if payload_len > 0 {
                if !matches!(
                    tokio::time::timeout(
                        Duration::from_secs(10),
                        read.read_exact(&mut full[RELAY_HEADER_LEN..])
                    )
                    .await,
                    Ok(Ok(_))
                ) {
                    break;
                }
            }
            match relay_protocol::decode(&full) {
                Ok(RelayFrame::Route {
                    source,
                    target: _,
                    payload,
                }) => {
                    incoming
                        .send(IncomingMessage {
                            peer_id: source,
                            transport_id: transport_id.0.clone(),
                            payload,
                            received_at: Instant::now(),
                        })
                        .ok();
                }
                Ok(RelayFrame::Heartbeat) => {} // liveness ping, no action
                Ok(RelayFrame::Register { .. }) => {
                    tracing::warn!("relay reader: unexpected Register from relay");
                }
                Err(e) => {
                    tracing::warn!("relay reader: decode error: {e:?}");
                    break;
                }
            }
        }
    }

    /// Replace relay candidates from a trusted configuration or platform
    /// discovery result. Empty input intentionally leaves the transport
    /// unavailable: there is no safe implicit public relay.
    pub async fn set_relay_endpoints(&self, endpoints: Vec<SocketAddr>) {
        self.has_relay_endpoint
            .store(!endpoints.is_empty(), Ordering::Release);
        *self.relay_candidates.write().await = endpoints;
        self.refresh_availability().await;
    }

    /// Report whether the platform currently has a validated usable network.
    /// This is an availability hint, not a peer-reachability assertion.
    pub async fn set_network_available(&self, available: bool) {
        self.network_available.store(available, Ordering::Release);
        self.refresh_availability().await;
    }

    /// Reconcile externally-owned connectivity/configuration with live sockets.
    /// A stale TCP connection is not usable after Android withdraws its validated
    /// default network (nor after its relay configuration is removed). Drop both
    /// the pool and the per-peer bindings before publishing `Unavailable`, so a
    /// later recovery must establish a fresh, explicitly configured link.
    async fn refresh_availability(&self) {
        if !self.network_available.load(Ordering::Acquire)
            || !self.has_relay_endpoint.load(Ordering::Acquire)
        {
            self.pool.lock().await.connections.clear();
            self.peer_relays.lock().await.clear();
            *self.relay_addr.lock().await = None;
            if let Ok(mut readers) = self.readers.lock() {
                for reader in readers.drain(..) {
                    reader.abort();
                }
            }
            self.set_state(TransportState::Unavailable);
            return;
        }
        // A blocking read is deliberately avoided here; an empty/unknown
        // endpoint list is represented by the existing Unavailable state and
        // `connect()` remains the final authority.
        if self.state.load() == TransportState::Unavailable {
            self.set_state(TransportState::Available);
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
        let candidate = self.relay_candidates.read().await.first().copied();
        peer.addresses
            .first()
            .copied()
            .or(binding)
            .or(known)
            .or(candidate)
    }

    /// Open a pooled connection to a peer's relay or LAN address.
    /// Returns `(write_half, abort_handle, addr)` so the caller has the
    /// address before writing — avoids resolve-after-write for RF-40.
    async fn get_connection(
        &self,
        peer: &PeerInfo,
    ) -> Result<(BoxWriter, tokio::task::AbortHandle, SocketAddr), TransportError> {
        let addr = self
            .resolve_relay(peer)
            .await
            .ok_or(TransportError::PeerNotFound)?;

        // RF-38: backoff gate — refuse to dial before the inter-attempt delay.
        let attempts = self.reconnect_attempts.load(Ordering::Relaxed);
        if attempts > 0 {
            if let Ok(guard) = self.last_attempt.lock() {
                if let Some(t) = *guard {
                    let delay = Duration::from_millis(backoff_ms(attempts, self.jitter_seed));
                    if t.elapsed() < delay {
                        return Err(TransportError::Busy);
                    }
                }
            }
        }

        let mut pool = self.pool.lock().await;
        let now = Instant::now();
        if let Some((write, abort)) = pool.acquire(now, addr) {
            return Ok((write, abort, addr));
        }
        drop(pool);

        let mode = self.current_mode();

        // Open new TCP connection (timeout 3s per INTERNET.md negotiation budget).
        let tcp = match tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(addr)).await
        {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                self.record_failed_attempt();
                return Err(e.into());
            }
            Err(_) => {
                self.record_failed_attempt();
                return Err(TransportError::ConnectionFailed);
            }
        };
        self.reconnect_attempts.store(0, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_attempt.lock() {
            *guard = None;
        }
        tcp.set_nodelay(true).ok();

        match mode {
            ConnectionMode::Plain => {
                let (r, w) = tcp.into_split();
                let abort = self.spawn_reader(peer.peer_id, Box::new(r), false);
                Ok((Box::new(w), abort, addr))
            }
            ConnectionMode::Relay {
                ref server_name,
                own_peer_id,
            } => {
                let sn = parse_relay_server_name(server_name)?;
                let connector = verified_relay_connector();
                let tls_stream = match tokio::time::timeout(
                    Duration::from_secs(5),
                    connector.connect(sn, tcp),
                )
                .await
                {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => {
                        self.record_failed_attempt();
                        return Err(TransportError::Io {
                            kind: e.kind(),
                            msg: e.to_string(),
                        });
                    }
                    Err(_) => {
                        self.record_failed_attempt();
                        return Err(TransportError::ConnectionFailed);
                    }
                };
                let (r, w) = tokio::io::split(tls_stream);
                let abort = self.spawn_reader(peer.peer_id, Box::new(r), true);
                // Send Register frame immediately so the relay can route to us.
                let reg = relay_protocol::encode(&RelayFrame::Register {
                    peer_id: PeerId(own_peer_id),
                })
                .map_err(|e| TransportError::Protocol(e.to_string()))?;
                let mut boxed_w: BoxWriter = Box::new(w);
                boxed_w.write_all(&reg).await?;
                boxed_w.flush().await?;
                Ok((boxed_w, abort, addr))
            }
        }
    }

    fn record_failed_attempt(&self) {
        self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_attempt.lock() {
            *guard = Some(Instant::now());
        }
    }

    /// Spawn a reader task for a new connection and return its abort handle.
    /// `is_relay` selects relay-frame vs plain-frame decoding.
    /// A clone is stored in `self.readers` for shutdown (RF-37); the returned
    /// handle is given to the pool so over-cap eviction kills the task.
    fn spawn_reader(
        &self,
        peer_id: PeerId,
        read: BoxReader,
        is_relay: bool,
    ) -> tokio::task::AbortHandle {
        let incoming = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        let handle = tokio::spawn(async move {
            if is_relay {
                Self::relay_reader_task(peer_id, read, incoming, transport_id.clone()).await;
            } else {
                Self::plain_reader_task(peer_id, read, incoming, transport_id.clone()).await;
            }
            // RF-37: only transition to Available on link-loss if not shut down.
            if state.load() != TransportState::Unavailable {
                state.store(TransportState::Available);
                state_tx
                    .send(TransportStateEvent {
                        transport_id,
                        new_state: TransportState::Available,
                    })
                    .ok();
            }
        });
        let abort = handle.abort_handle();
        if let Ok(mut guards) = self.readers.lock() {
            guards.push(abort.clone());
        }
        abort
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
        let (conn, abort, conn_addr) = match self.get_connection(peer).await {
            Ok(triple) => triple,
            Err(e) => {
                self.set_state(TransportState::Available);
                return Err(e);
            }
        };
        // Park the connection in the pool (keyed by relay) for send() reuse.
        self.pool
            .lock()
            .await
            .release(conn, abort, conn_addr, Instant::now());
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
        // RF-36: require an explicit per-peer binding — no fallback to relay_addr
        // or relay_candidates so a send for peer B can never use peer A's relay.
        let bound_addr = self
            .peer_relays
            .lock()
            .await
            .get(peer)
            .copied()
            .ok_or(TransportError::NotConnected)?;
        let peer_info = PeerInfo {
            peer_id: *peer,
            addresses: vec![bound_addr], // explicit binding; resolve_relay picks first()
            transport_addresses: Vec::new(),
            last_seen: Some(Instant::now()),
        };
        // get_connection returns the addr it resolved, so we know it before
        // writing — a post-write resolution miss returning PeerNotFound on bytes
        // already on the wire (RF-40) is now impossible.
        let (mut write, reader_abort, addr) = self.get_connection(&peer_info).await?;
        // In relay mode, wrap the IRIS envelope in a RelayFrame::Route.
        // In plain mode, use the simple [version][len][payload] wire format.
        let frame = match self.current_mode() {
            ConnectionMode::Relay { own_peer_id, .. } => relay_protocol::encode(
                &RelayFrame::Route {
                    source: PeerId(own_peer_id),
                    target: *peer,
                    payload: message.payload.clone(),
                },
            )
            .map_err(|e| TransportError::Protocol(e.to_string()))?,
            ConnectionMode::Plain => encode_frame(message)?,
        };
        let write_res: Result<(), std::io::Error> = async {
            write.write_all(&frame).await?;
            write.flush().await?;
            Ok(())
        }
        .await;
        // On write failure the link is dead: abort the reader immediately
        // (don't wait for its 10 s timeout), mark Degraded, and discard the
        // socket so the reader marks the transport Available on its exit
        // (RED-0001-02 zombie state).
        if let Err(e) = write_res {
            reader_abort.abort();
            self.set_state(TransportState::Degraded);
            return Err(e.into());
        }
        let now = Instant::now();
        self.ewma.record_send(frame.len()); // MG-17: update goodput estimate
        let mut pool = self.pool.lock().await;
        pool.release(write, reader_abort, addr, now);
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
            bandwidth_available_bps: self.ewma.bandwidth_bps(self.caps.typical_throughput_bps),
            congestion_level: congestion,
        }
    }

    fn set_send_priority_hint(&self, priority: MessagePriority) {
        self.priority_hint
            .store(priority.as_u8(), Ordering::Release);
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        // Set Unavailable before aborting readers so the state-guard in
        // spawn_reader sees Unavailable and skips the Available write (RF-37).
        self.set_state(TransportState::Unavailable);
        if let Ok(mut guards) = self.readers.lock() {
            for h in guards.drain(..) {
                h.abort();
            }
        }
        // Stop LAN listener accept loop.
        if let Ok(mut guard) = self.lan_listener_abort.lock() {
            if let Some(h) = guard.take() {
                h.abort();
            }
        }
        self.pool.lock().await.connections.clear();
        self.peer_relays.lock().await.clear();
        *self.relay_addr.lock().await = None;
        self.reconnect_attempts.store(0, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_attempt.lock() {
            *guard = None;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
        let len = frame_payload_len(&frame[..FRAME_HEADER_LEN]).unwrap();
        assert_eq!(len, 10);
        assert_eq!(&frame[FRAME_HEADER_LEN..], b"hello iris");
    }

    #[test]
    fn frame_rejects_oversized_payload() {
        let mut header = vec![FRAME_VERSION];
        header.extend_from_slice(&(MAX_FRAME_BYTES as u32 + 1).to_le_bytes());
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
            let mut header = [0u8; FRAME_HEADER_LEN];
            socket.read_exact(&mut header).await.unwrap();
            let len = frame_payload_len(&header).expect("valid frame header");
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
        assert_eq!(receipt.bytes_sent, FRAME_HEADER_LEN + 14);

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
    async fn network_and_relay_configuration_gate_availability() {
        let transport = InternetTransport::new(&[], InternetCostParams::default());
        transport.set_network_available(true).await;
        assert_eq!(transport.state(), TransportState::Unavailable);
        transport
            .set_relay_endpoints(vec!["127.0.0.1:9000".parse().unwrap()])
            .await;
        assert_eq!(transport.state(), TransportState::Available);
        transport.set_network_available(false).await;
        assert_eq!(transport.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn network_loss_drops_peer_bindings_before_recovery() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
        });
        let transport = InternetTransport::new(&[relay_addr], InternetCostParams::default());
        let peer = PeerId([0x42; 32]);
        let peer_info = PeerInfo {
            peer_id: peer,
            addresses: vec![relay_addr],
            transport_addresses: Vec::new(),
            last_seen: None,
        };
        transport.connect(&peer_info).await.unwrap();
        assert_eq!(transport.state(), TransportState::Connected);

        transport.set_network_available(false).await;
        assert_eq!(transport.state(), TransportState::Unavailable);
        assert!(
            matches!(
                transport
                    .send(&peer, &sample_message(b"stale", MessagePriority::P2))
                    .await,
                Err(TransportError::NotConnected)
            ),
            "a network-loss transition must not leave a stale peer route usable"
        );
        server.await.unwrap();
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
        assert!(matches!(
            caps.cost_class,
            TransportCostClass::Metered { .. }
        ));
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
            Err(TransportError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn encode_frame_accepts_exactly_max_frame_bytes() {
        // The boundary is inclusive on both encode and decode, so a max-size
        // frame round-trips rather than being rejected on one side only.
        let msg = sample_message(&vec![0u8; MAX_FRAME_BYTES], MessagePriority::P4);
        let frame = encode_frame(&msg).expect("max-size frame encodes");
        assert_eq!(
            frame_payload_len(&frame[..FRAME_HEADER_LEN]),
            Some(MAX_FRAME_BYTES)
        );
    }

    #[test]
    fn frame_payload_len_rejects_truncated_header() {
        // Adversarial: any header shorter than FRAME_HEADER_LEN (5 bytes) is incomplete.
        assert!(frame_payload_len(&[]).is_none());
        assert!(frame_payload_len(&[FRAME_VERSION]).is_none());
        assert!(frame_payload_len(&[FRAME_VERSION, 0x00]).is_none());
        assert!(frame_payload_len(&[FRAME_VERSION, 0x00, 0x00]).is_none());
        assert!(frame_payload_len(&[FRAME_VERSION, 0x00, 0x00, 0x00]).is_none());
        // Unknown version byte — even with sufficient length, must be rejected.
        assert!(frame_payload_len(&[0x00, 0x01, 0x00, 0x00, 0x00]).is_none());
        // Version correct but absurd payload length.
        assert!(frame_payload_len(&[FRAME_VERSION, 0x00, 0x00, 0x00, 0x80]).is_none());
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
                let mut header = [0u8; FRAME_HEADER_LEN];
                socket.read_exact(&mut header).await.unwrap();
                let len = frame_payload_len(&header).expect("valid frame header");
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

    // ---- Tier-5 integration tests ----

    #[tokio::test]
    async fn lan_listener_accepts_inbound_plain_tcp_frame() {
        use futures_util::StreamExt;

        let transport = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        // Trigger manual availability: supply a dummy relay addr + network signal.
        transport.set_network_available(true).await;
        transport
            .set_relay_endpoints(vec!["127.0.0.1:1".parse().unwrap()])
            .await;

        let port = transport.start_lan_listener().await.unwrap();
        assert!(port > 0, "listener must bind a non-zero port");

        // A raw LAN client sends one plain frame to the listener.
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        tokio::spawn(async move {
            let mut s = TcpStream::connect(addr).await.unwrap();
            let msg = sample_message(b"hello from lan", MessagePriority::P3);
            let frame = encode_frame(&msg).unwrap();
            s.write_all(&frame).await.unwrap();
            s.flush().await.unwrap();
        });

        let mut inbox = transport.incoming_messages();
        let arrived = tokio::time::timeout(Duration::from_secs(2), async {
            inbox.next().await
        })
        .await
        .expect("message must arrive within 2s")
        .expect("stream must yield a message");

        assert_eq!(arrived.payload, b"hello from lan");
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn lan_listener_is_idempotent() {
        let transport = InternetTransport::new(&[], InternetCostParams::default());
        let p1 = transport.start_lan_listener().await.unwrap();
        let p2 = transport.start_lan_listener().await.unwrap();
        assert_eq!(p1, p2, "second call must return same port");
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn relay_server_routes_frame_between_two_plain_clients() {
        use std::collections::HashMap as HMap;
        use tokio::sync::Mutex as TokMutex;
        use crate::transport::relay_protocol::RelayFrame;

        // Minimal in-process relay (plain TCP, same wire protocol as relay_server.rs).
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = listener.local_addr().unwrap();
        let router: Arc<TokMutex<HMap<[u8; 32], tokio::net::tcp::OwnedWriteHalf>>> =
            Arc::new(TokMutex::new(HMap::new()));

        let router_srv = router.clone();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                stream.set_nodelay(true).ok();
                let (mut r, w) = stream.into_split();
                let rmap = router_srv.clone();
                tokio::spawn(async move {
                    let frame = read_relay_frame(&mut r).await.unwrap();
                    let RelayFrame::Register { peer_id } = frame else { return };
                    rmap.lock().await.insert(peer_id.0, w);
                    loop {
                        let Some(frame) = read_relay_frame(&mut r).await else { break };
                        if let RelayFrame::Route { source, target, payload } = frame {
                            let out = relay_protocol::encode(&RelayFrame::Route {
                                source, target, payload,
                            }).unwrap();
                            let mut map = rmap.lock().await;
                            if let Some(w) = map.get_mut(&target.0) {
                                w.write_all(&out).await.ok();
                                w.flush().await.ok();
                            }
                        }
                    }
                });
            }
        });

        let id_a = [0xAA_u8; 32];
        let id_b = [0xBB_u8; 32];

        // Register A.
        let mut stream_a = TcpStream::connect(relay_addr).await.unwrap();
        stream_a.write_all(
            &relay_protocol::encode(&RelayFrame::Register { peer_id: PeerId(id_a) }).unwrap(),
        ).await.unwrap();
        stream_a.flush().await.unwrap();

        // Register B.
        let mut stream_b = TcpStream::connect(relay_addr).await.unwrap();
        stream_b.write_all(
            &relay_protocol::encode(&RelayFrame::Register { peer_id: PeerId(id_b) }).unwrap(),
        ).await.unwrap();
        stream_b.flush().await.unwrap();

        // Let relay register both.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // A → B.
        stream_a.write_all(
            &relay_protocol::encode(&RelayFrame::Route {
                source: PeerId(id_a),
                target: PeerId(id_b),
                payload: b"cross-relay delivery".to_vec(),
            }).unwrap(),
        ).await.unwrap();
        stream_a.flush().await.unwrap();

        // B reads the forwarded frame.
        let frame = tokio::time::timeout(
            Duration::from_secs(2),
            read_relay_frame_from_stream(&mut stream_b),
        )
        .await
        .expect("must arrive within 2s")
        .expect("relay must forward frame");

        assert_eq!(
            frame,
            RelayFrame::Route {
                source: PeerId(id_a),
                target: PeerId(id_b),
                payload: b"cross-relay delivery".to_vec(),
            }
        );
    }

    async fn read_relay_frame(
        r: &mut tokio::net::tcp::OwnedReadHalf,
    ) -> Option<relay_protocol::RelayFrame> {
        use crate::transport::relay_protocol::{RELAY_HEADER_LEN, MAX_RELAY_PAYLOAD};
        let mut hdr = [0u8; RELAY_HEADER_LEN];
        r.read_exact(&mut hdr).await.ok()?;
        let mut lb = [0u8; 4];
        lb.copy_from_slice(&hdr[66..70]);
        let plen = u32::from_le_bytes(lb) as usize;
        if plen > MAX_RELAY_PAYLOAD { return None; }
        let mut full = vec![0u8; RELAY_HEADER_LEN + plen];
        full[..RELAY_HEADER_LEN].copy_from_slice(&hdr);
        if plen > 0 { r.read_exact(&mut full[RELAY_HEADER_LEN..]).await.ok()?; }
        relay_protocol::decode(&full).ok()
    }

    async fn read_relay_frame_from_stream(s: &mut TcpStream) -> Option<relay_protocol::RelayFrame> {
        use crate::transport::relay_protocol::{RELAY_HEADER_LEN, MAX_RELAY_PAYLOAD};
        let mut hdr = [0u8; RELAY_HEADER_LEN];
        s.read_exact(&mut hdr).await.ok()?;
        let mut lb = [0u8; 4];
        lb.copy_from_slice(&hdr[66..70]);
        let plen = u32::from_le_bytes(lb) as usize;
        if plen > MAX_RELAY_PAYLOAD { return None; }
        let mut full = vec![0u8; RELAY_HEADER_LEN + plen];
        full[..RELAY_HEADER_LEN].copy_from_slice(&hdr);
        if plen > 0 { s.read_exact(&mut full[RELAY_HEADER_LEN..]).await.ok()?; }
        relay_protocol::decode(&full).ok()
    }
}
