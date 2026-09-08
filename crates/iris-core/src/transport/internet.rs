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
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use ed25519_dalek::{Signature, VerifyingKey};
use futures_util::stream::Stream;
use rand::rngs::OsRng;
use rand::RngCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{broadcast, oneshot, Mutex, RwLock, Semaphore};

use crate::transport::relay_protocol::{self, RelayAckStatus, RelayFrame, RELAY_HEADER_LEN};
use crate::transport::tls::{
    relay_server_name as parse_relay_server_name, verified_relay_connector, RELAY_ALPN,
};

/// Owned, heap-allocated asynchronous write half — used for both plain TCP
/// (`OwnedWriteHalf`) and TLS (`WriteHalf<TlsStream<TcpStream>>`).
type BoxWriter = Box<dyn tokio::io::AsyncWrite + Send + Unpin>;
type SharedWriter = Arc<Mutex<BoxWriter>>;
/// Owned, heap-allocated asynchronous read half.
type BoxReader = Box<dyn tokio::io::AsyncRead + Send + Unpin>;
const MAX_INBOUND_LAN_SESSIONS: usize = 64;

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

/// Identity-key signing seam for the relay's server-challenge registration.
/// Android implements this with the same Keystore Ed25519 key whose public
/// half is the node's PeerId.
pub trait RelaySigner: Send + Sync + 'static {
    fn sign(&self, message: &[u8]) -> Result<[u8; 64], TransportError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternetAvailability {
    pub validated_wan: bool,
    pub local_network: bool,
    pub relay_configured: bool,
    pub relay_mode: bool,
    pub lan_listener_port: u16,
    pub last_failure: Option<String>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ConnectionScope {
    /// A LAN socket is authenticated to exactly one remote identity.
    Lan(PeerId),
    /// A relay socket is registered as exactly one local identity and may
    /// carry routes for multiple remote peers on that relay.
    Relay(PeerId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ConnectionKey {
    addr: SocketAddr,
    scope: ConnectionScope,
}

/// A pooled TCP connection. The key includes the authenticated identity scope;
/// a socket can therefore never be reused across LAN peers or local relay
/// identities merely because their socket address is the same.
struct PooledConnection {
    key: ConnectionKey,
    write: SharedWriter,
    reader_abort: tokio::task::AbortHandle,
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
        self.reader_abort.abort();
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

    /// Clone a live connection bound to exactly `key`. The writer is internally
    /// serialized, so keeping the pool entry present coalesces concurrent sends
    /// instead of opening duplicate identity sessions.
    /// Reaps expired entries first — their Drop aborts orphaned reader tasks.
    fn acquire(
        &mut self,
        now: Instant,
        key: ConnectionKey,
    ) -> Option<(SharedWriter, tokio::task::AbortHandle)> {
        self.connections.retain(|c| !c.is_expired(now));
        self.connections.iter_mut().find(|c| c.key == key).map(|c| {
            c.last_used = now;
            (c.write.clone(), c.reader_abort.clone())
        })
    }

    fn release(
        &mut self,
        write: SharedWriter,
        reader_abort: tokio::task::AbortHandle,
        key: ConnectionKey,
        now: Instant,
    ) {
        self.connections.retain(|c| !c.is_expired(now));
        if let Some(existing) = self.connections.iter_mut().find(|c| c.key == key) {
            existing.last_used = now;
            return;
        }
        let per_addr = self
            .connections
            .iter()
            .filter(|c| c.key.addr == key.addr)
            .count();
        let conn = PooledConnection {
            key,
            write,
            reader_abort,
            last_used: now,
            idle_timeout: self.idle_timeout,
        };
        if per_addr < self.max_per_relay {
            self.connections.push(conn);
        }
        // else: conn drops here; Drop aborts the orphaned reader task.
    }

    fn remove(&mut self, key: ConnectionKey) {
        self.connections.retain(|c| c.key != key);
    }
}

/// Exponential backoff with ±25% jitter (pure, unit-tested).
pub fn backoff_ms(attempt: u32, seed: u64) -> u64 {
    let exponent = attempt.saturating_sub(1).min(31);
    let base = 1_u64 << exponent; // attempt 1 => 1s, then 2,4,8,...
    let base_ms = (base * 1000).min(BACKOFF_MAX_MS);
    let jitter_range = (base_ms as f64 * BACKOFF_JITTER) as u64;
    let offset = seed % (jitter_range.saturating_mul(2) + 1);
    // Shift [0, 2*jitter] to [-jitter, +jitter].
    base_ms.saturating_add(offset).saturating_sub(jitter_range)
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
    /// Per-key dial locks coalesce concurrent first-use and reconnect attempts.
    connection_locks: Mutex<HashMap<ConnectionKey, Arc<Mutex<()>>>>,
    cost_params: InternetCostParams,
    priority_hint: AtomicU8,
    /// Address of the relay established at connect(); used to reconnect if the
    /// pooled connection idles out before the next send.
    relay_addr: Mutex<Option<SocketAddr>>,
    /// Candidate relay endpoints supplied by configuration or platform discovery.
    /// This is mutable because Android NSD may resolve a relay after engine start.
    relay_candidates: RwLock<Vec<SocketAddr>>,
    /// Serializes configuration replacement against connect/send operations.
    config_gate: RwLock<()>,
    /// Connectivity is platform-owned. A configured relay alone must not make
    /// this transport selectable while Android reports no validated network.
    network_available: std::sync::atomic::AtomicBool,
    /// Local infrastructure network viability is distinct from public WAN
    /// validation (an isolated disaster-site Wi-Fi LAN is still useful).
    local_network_available: AtomicBool,
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
    relay_signer: StdMutex<Option<std::sync::Arc<dyn RelaySigner>>>,
    local_identity: StdMutex<Option<(PeerId, Arc<dyn RelaySigner>)>>,
    pending_acks: Arc<Mutex<HashMap<([u8; 16], PeerId), oneshot::Sender<RelayAckStatus>>>>,
    last_failure: StdMutex<Option<String>>,
    /// Bound LAN listener port (0 = not started). Set once by start_lan_listener().
    lan_listener_port: AtomicU16,
    /// Abort handle for the LAN accept loop (shutdown() tears it down).
    lan_listener_abort: StdMutex<Option<tokio::task::AbortHandle>>,
    /// Authenticated or authenticating inbound LAN tasks. Explicitly bounded
    /// and aborted when the listener is stopped so network loss cannot leave
    /// detached sessions running.
    lan_sessions: Arc<StdMutex<Vec<tokio::task::AbortHandle>>>,
    /// NSD-discovered LAN peers: PeerId → direct plain-TCP address.
    /// Sends to these peers bypass the relay pool and always use plain TCP,
    /// regardless of the current ConnectionMode.
    lan_peer_addrs: StdMutex<HashMap<PeerId, SocketAddr>>,
    /// Test-only: a custom TlsConnector that trusts a self-signed test cert,
    /// injected so integration tests can exercise the TLS relay path without
    /// a WebPKI-signed certificate.
    #[cfg(test)]
    tls_connector_override: StdMutex<Option<tokio_rustls::TlsConnector>>,
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
            connection_locks: Mutex::new(HashMap::new()),
            cost_params,
            priority_hint: AtomicU8::new(MessagePriority::P5.as_u8()),
            relay_addr: Mutex::new(None),
            relay_candidates: RwLock::new(relay_endpoints.to_vec()),
            config_gate: RwLock::new(()),
            // Non-Android callers historically construct this transport with a
            // concrete endpoint and no platform callback. Preserve that usable
            // core default; Android constructs it empty and explicitly supplies
            // its validated-network signal before configuration.
            network_available: std::sync::atomic::AtomicBool::new(!relay_endpoints.is_empty()),
            local_network_available: AtomicBool::new(false),
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
            relay_signer: StdMutex::new(None),
            local_identity: StdMutex::new(None),
            pending_acks: Arc::new(Mutex::new(HashMap::new())),
            last_failure: StdMutex::new(None),
            lan_listener_port: AtomicU16::new(0),
            lan_listener_abort: StdMutex::new(None),
            lan_sessions: Arc::new(StdMutex::new(Vec::new())),
            lan_peer_addrs: StdMutex::new(HashMap::new()),
            #[cfg(test)]
            tls_connector_override: StdMutex::new(None),
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

    /// Configure the authenticated relay registration signer. Relay connect
    /// fails closed when this is absent.
    pub fn set_relay_signer(&self, signer: std::sync::Arc<dyn RelaySigner>) {
        if let Ok(mut guard) = self.relay_signer.lock() {
            *guard = Some(signer);
        }
    }

    /// Install the identity used for mutual authentication of LAN sockets.
    /// The same signer may also be used for relay registration.
    pub fn set_local_identity(&self, peer_id: PeerId, signer: Arc<dyn RelaySigner>) {
        if let Ok(mut guard) = self.local_identity.lock() {
            *guard = Some((peer_id, signer));
        }
    }

    pub fn availability(&self) -> InternetAvailability {
        InternetAvailability {
            validated_wan: self.network_available.load(Ordering::Acquire),
            local_network: self.local_network_available.load(Ordering::Acquire),
            relay_configured: self.has_relay_endpoint.load(Ordering::Acquire),
            relay_mode: matches!(self.current_mode(), ConnectionMode::Relay { .. }),
            lan_listener_port: self.lan_listener_port(),
            last_failure: self.last_failure.lock().ok().and_then(|v| v.clone()),
        }
    }

    /// Replace relay identity and endpoints as one externally visible change.
    pub async fn configure_relay(
        &self,
        endpoints: Vec<SocketAddr>,
        server_name: Option<String>,
        own_peer_id: [u8; 32],
    ) {
        let _configuration = self.config_gate.write().await;
        self.invalidate_relay_sessions().await;
        if let Ok(mut mode) = self.mode.lock() {
            *mode = match server_name {
                Some(server_name) if !endpoints.is_empty() => ConnectionMode::Relay {
                    server_name,
                    own_peer_id,
                },
                _ => ConnectionMode::Plain,
            };
        }
        self.has_relay_endpoint
            .store(!endpoints.is_empty(), Ordering::Release);
        *self.relay_candidates.write().await = endpoints;
        self.refresh_availability().await;
    }

    fn record_failure(&self, reason: impl Into<String>) {
        if let Ok(mut guard) = self.last_failure.lock() {
            *guard = Some(reason.into());
        }
    }

    fn clear_failure(&self) {
        if let Ok(mut guard) = self.last_failure.lock() {
            *guard = None;
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
        let identity = self
            .local_identity
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or(TransportError::PolicyDenied(
                "LAN identity signer unavailable",
            ))?;
        // IPV6_V6ONLY defaults differ by OS (notably true on Windows), so set
        // it explicitly before bind. One listener then accepts native IPv6 and
        // IPv4-mapped clients on the same NSD-advertised port.
        let socket = socket2::Socket::new(
            socket2::Domain::IPV6,
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )
        .map_err(|e| TransportError::Io {
            kind: e.kind(),
            msg: e.to_string(),
        })?;
        socket
            .set_only_v6(false)
            .and_then(|_| socket.set_nonblocking(true))
            .and_then(|_| socket.bind(&"[::]:0".parse::<SocketAddr>().unwrap().into()))
            .and_then(|_| socket.listen(128))
            .map_err(|e| TransportError::Io {
                kind: e.kind(),
                msg: e.to_string(),
            })?;
        let std_listener: std::net::TcpListener = socket.into();
        let listener =
            tokio::net::TcpListener::from_std(std_listener).map_err(|e| TransportError::Io {
                kind: e.kind(),
                msg: e.to_string(),
            })?;
        let port = listener
            .local_addr()
            .map_err(|e| TransportError::Io {
                kind: e.kind(),
                msg: e.to_string(),
            })?
            .port();
        self.lan_listener_port.store(port, Ordering::Release);
        self.refresh_availability().await;

        let incoming_tx = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        let session_slots = Arc::new(Semaphore::new(MAX_INBOUND_LAN_SESSIONS));
        let lan_sessions = self.lan_sessions.clone();
        let accept_loop = tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((mut stream, _peer_addr)) => {
                        let Ok(session_slot) = session_slots.clone().try_acquire_owned() else {
                            tracing::warn!("LAN inbound session limit reached");
                            continue;
                        };
                        stream.set_nodelay(true).ok();
                        let incoming = incoming_tx.clone();
                        let tid = transport_id.clone();
                        let identity = identity.clone();
                        let task = tokio::spawn(async move {
                            let _session_slot = session_slot;
                            match Self::authenticate_lan_server(&mut stream, identity).await {
                                Ok(remote_peer) => {
                                    let (read, _write) = stream.into_split();
                                    Self::plain_reader_task(
                                        remote_peer,
                                        Box::new(read),
                                        incoming,
                                        tid,
                                    )
                                    .await;
                                }
                                Err(error) => {
                                    tracing::warn!(%error, "LAN mutual authentication rejected");
                                }
                            }
                        });
                        if let Ok(mut sessions) = lan_sessions.lock() {
                            sessions.retain(|handle| !handle.is_finished());
                            sessions.push(task.abort_handle());
                        }
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

    pub async fn stop_lan_listener(&self) {
        if let Ok(mut guard) = self.lan_listener_abort.lock() {
            if let Some(abort) = guard.take() {
                abort.abort();
            }
        }
        if let Ok(mut sessions) = self.lan_sessions.lock() {
            for session in sessions.drain(..) {
                session.abort();
            }
        }
        self.lan_listener_port.store(0, Ordering::Release);
        if let Ok(mut peers) = self.lan_peer_addrs.lock() {
            peers.clear();
        }
        self.refresh_availability().await;
    }

    async fn write_control<W: tokio::io::AsyncWrite + Unpin>(
        write: &mut W,
        frame: RelayFrame,
    ) -> Result<(), TransportError> {
        let bytes = relay_protocol::encode(&frame)
            .map_err(|error| TransportError::Protocol(error.to_string()))?;
        tokio::time::timeout(Duration::from_secs(5), async {
            write.write_all(&bytes).await?;
            write.flush().await
        })
        .await
        .map_err(|_| TransportError::ConnectionFailed)??;
        Ok(())
    }

    fn verify_lan_register(
        frame: RelayFrame,
        challenge: &[u8; 32],
        expected: Option<PeerId>,
    ) -> Result<PeerId, TransportError> {
        let RelayFrame::Register { peer_id, signature } = frame else {
            return Err(TransportError::Protocol("LAN Register missing".into()));
        };
        if expected.is_some_and(|value| value != peer_id) {
            return Err(TransportError::PolicyDenied(
                "LAN endpoint identity does not match discovered contact",
            ));
        }
        let verifying = VerifyingKey::from_bytes(&peer_id.0)
            .map_err(|_| TransportError::PolicyDenied("invalid LAN peer identity"))?;
        let signable = relay_protocol::lan_registration_signable(challenge, &peer_id);
        verifying
            .verify_strict(&signable, &Signature::from_bytes(&signature))
            .map_err(|_| TransportError::PolicyDenied("LAN identity proof rejected"))?;
        Ok(peer_id)
    }

    async fn authenticate_lan_server(
        stream: &mut TcpStream,
        (own_peer, signer): (PeerId, Arc<dyn RelaySigner>),
    ) -> Result<PeerId, TransportError> {
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut challenge);
        Self::write_control(stream, RelayFrame::Challenge { nonce: challenge }).await?;
        let register = Self::read_relay_frame(stream, Duration::from_secs(5))
            .await
            .ok_or_else(|| TransportError::Protocol("LAN client Register missing".into()))?;
        let remote_peer = Self::verify_lan_register(register, &challenge, None)?;
        let client_challenge = match Self::read_relay_frame(stream, Duration::from_secs(5)).await {
            Some(RelayFrame::Challenge { nonce }) => nonce,
            _ => {
                return Err(TransportError::Protocol(
                    "LAN client challenge missing".into(),
                ))
            }
        };
        let signature = signer.sign(&relay_protocol::lan_registration_signable(
            &client_challenge,
            &own_peer,
        ))?;
        Self::write_control(
            stream,
            RelayFrame::Register {
                peer_id: own_peer,
                signature,
            },
        )
        .await?;
        Ok(remote_peer)
    }

    async fn authenticate_lan_client(
        &self,
        stream: &mut TcpStream,
        expected_peer: PeerId,
    ) -> Result<(), TransportError> {
        let (own_peer, signer) = self
            .local_identity
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or(TransportError::PolicyDenied(
                "LAN identity signer unavailable",
            ))?;
        let server_challenge = match Self::read_relay_frame(stream, Duration::from_secs(5)).await {
            Some(RelayFrame::Challenge { nonce }) => nonce,
            _ => {
                return Err(TransportError::Protocol(
                    "LAN server challenge missing".into(),
                ))
            }
        };
        let signature = signer.sign(&relay_protocol::lan_registration_signable(
            &server_challenge,
            &own_peer,
        ))?;
        Self::write_control(
            stream,
            RelayFrame::Register {
                peer_id: own_peer,
                signature,
            },
        )
        .await?;
        let mut challenge = [0u8; 32];
        OsRng.fill_bytes(&mut challenge);
        Self::write_control(stream, RelayFrame::Challenge { nonce: challenge }).await?;
        let register = Self::read_relay_frame(stream, Duration::from_secs(5))
            .await
            .ok_or_else(|| TransportError::Protocol("LAN server Register missing".into()))?;
        Self::verify_lan_register(register, &challenge, Some(expected_peer))?;
        Ok(())
    }

    /// The LAN listener's bound port, or 0 if not started.
    pub fn lan_listener_port(&self) -> u16 {
        self.lan_listener_port.load(Ordering::Acquire)
    }

    /// Register a NSD-discovered LAN peer so `send()` can reach it via plain
    /// TCP directly, without going through the relay.  Idempotent — repeated
    /// calls for the same peer update the address.
    pub fn register_lan_peer(&self, peer_id: PeerId, addr: SocketAddr) {
        if let Ok(mut map) = self.lan_peer_addrs.lock() {
            map.insert(peer_id, addr);
        }
    }

    /// Remove a stale NSD mapping and any pooled socket to its exact address.
    pub async fn remove_lan_peer(&self, peer_id: &PeerId) {
        let addr = self
            .lan_peer_addrs
            .lock()
            .ok()
            .and_then(|mut map| map.remove(peer_id));
        if let Some(addr) = addr {
            self.pool.lock().await.connections.retain(|c| {
                c.key
                    != (ConnectionKey {
                        addr,
                        scope: ConnectionScope::Lan(*peer_id),
                    })
            });
        }
    }

    /// Bind and authenticate a trusted peer through the configured relay.
    /// Trust authorization is deliberately enforced by the caller (Android's
    /// engine owns the TrustStore); this method only establishes transport state.
    pub async fn connect_relay_peer(
        &self,
        peer_id: PeerId,
    ) -> Result<TransportLink, TransportError> {
        let addresses = {
            let _configuration = self.config_gate.read().await;
            if !self.network_available.load(Ordering::Acquire)
                || !self.has_relay_endpoint.load(Ordering::Acquire)
                || !matches!(self.current_mode(), ConnectionMode::Relay { .. })
            {
                return Err(TransportError::NotConnected);
            }
            self.relay_candidates.read().await.clone()
        };
        if addresses.is_empty() {
            return Err(TransportError::PeerNotFound);
        }
        let mut last_error = TransportError::PeerNotFound;
        for (index, address) in addresses.into_iter().enumerate() {
            if index > 0 {
                // One explicit activation attempt probes every configured relay;
                // backoff applies between activation cycles, not between candidates.
                self.record_successful_attempt();
            }
            let peer = PeerInfo {
                peer_id,
                addresses: vec![address],
                transport_addresses: Vec::new(),
                last_seen: Some(Instant::now()),
            };
            match self.connect(&peer).await {
                Ok(link) => return Ok(link),
                Err(error) => last_error = error,
            }
        }
        Err(last_error)
    }

    /// Override the TLS connector used for relay connections.  Only available
    /// in test builds so integration tests can inject a connector that trusts
    /// a self-signed test certificate without shipping an insecure verifier.
    #[cfg(test)]
    pub fn set_test_tls_connector(&self, c: tokio_rustls::TlsConnector) {
        if let Ok(mut guard) = self.tls_connector_override.lock() {
            *guard = Some(c);
        }
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
        own_peer_id: PeerId,
        mut read: BoxReader,
        incoming: broadcast::Sender<IncomingMessage>,
        transport_id: TransportId,
        pending_acks: Arc<Mutex<HashMap<([u8; 16], PeerId), oneshot::Sender<RelayAckStatus>>>>,
        response_writer: SharedWriter,
    ) {
        loop {
            let Some(frame) = Self::read_relay_frame(&mut read, Duration::from_secs(120)).await
            else {
                break;
            };
            match frame {
                RelayFrame::Route {
                    route_id: _,
                    source,
                    target,
                    payload,
                } if target == own_peer_id => {
                    incoming
                        .send(IncomingMessage {
                            peer_id: source,
                            transport_id: transport_id.0.clone(),
                            payload,
                            received_at: Instant::now(),
                        })
                        .ok();
                }
                RelayFrame::Ack {
                    route_id,
                    target,
                    status,
                } => {
                    if let Some(waiter) = pending_acks.lock().await.remove(&(route_id, target)) {
                        waiter.send(status).ok();
                    }
                }
                RelayFrame::Heartbeat => {
                    let heartbeat =
                        relay_protocol::encode(&RelayFrame::Heartbeat).expect("bounded heartbeat");
                    let mut writer = response_writer.lock().await;
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(5), async {
                            writer.write_all(&heartbeat).await?;
                            writer.flush().await
                        })
                        .await,
                        Ok(Ok(()))
                    ) {
                        break;
                    }
                }
                RelayFrame::Route { .. }
                | RelayFrame::Challenge { .. }
                | RelayFrame::Register { .. } => {
                    tracing::warn!(peer = ?peer_id, "relay reader: unexpected or misaddressed frame");
                    break;
                }
            }
        }
    }

    async fn read_relay_frame<R: tokio::io::AsyncRead + Unpin>(
        read: &mut R,
        timeout: Duration,
    ) -> Option<RelayFrame> {
        let mut header = [0u8; RELAY_HEADER_LEN];
        if !matches!(
            tokio::time::timeout(timeout, read.read_exact(&mut header)).await,
            Ok(Ok(_))
        ) {
            return None;
        }
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&header[66..70]);
        let payload_len = u32::from_le_bytes(len_bytes) as usize;
        if payload_len > relay_protocol::MAX_RELAY_PAYLOAD + relay_protocol::RELAY_SIGNATURE_LEN {
            return None;
        }
        let mut full = vec![0u8; RELAY_HEADER_LEN + payload_len];
        full[..RELAY_HEADER_LEN].copy_from_slice(&header);
        if payload_len > 0
            && !matches!(
                tokio::time::timeout(timeout, read.read_exact(&mut full[RELAY_HEADER_LEN..])).await,
                Ok(Ok(_))
            )
        {
            return None;
        }
        relay_protocol::decode(&full).ok()
    }

    /// Replace relay candidates from a trusted configuration or platform
    /// discovery result. Empty input intentionally leaves the transport
    /// unavailable: there is no safe implicit public relay.
    pub async fn set_relay_endpoints(&self, endpoints: Vec<SocketAddr>) {
        self.invalidate_relay_sessions().await;
        self.has_relay_endpoint
            .store(!endpoints.is_empty(), Ordering::Release);
        *self.relay_candidates.write().await = endpoints;
        self.refresh_availability().await;
    }

    /// Report whether the platform currently has a validated usable network.
    /// This is an availability hint, not a peer-reachability assertion.
    pub async fn set_network_available(&self, available: bool) {
        let was_available = self.network_available.swap(available, Ordering::AcqRel);
        if was_available && !available {
            self.invalidate_relay_sessions().await;
        }
        self.refresh_availability().await;
    }

    /// Report whether a local infrastructure network is usable for direct LAN
    /// sockets. This is intentionally independent of public Internet validation.
    pub async fn set_local_network_available(&self, available: bool) {
        let was_available = self
            .local_network_available
            .swap(available, Ordering::AcqRel);
        if was_available && !available {
            self.lan_peer_addrs
                .lock()
                .ok()
                .map(|mut peers| peers.clear());
            self.invalidate_connections().await;
        }
        self.refresh_availability().await;
    }

    async fn invalidate_connections(&self) {
        self.pool.lock().await.connections.clear();
        self.connection_locks.lock().await.clear();
        self.pending_acks.lock().await.clear();
        if let Ok(mut readers) = self.readers.lock() {
            for reader in readers.drain(..) {
                reader.abort();
            }
        }
    }

    async fn invalidate_relay_sessions(&self) {
        self.invalidate_connections().await;
        self.peer_relays.lock().await.clear();
        *self.relay_addr.lock().await = None;
    }

    /// Reconcile externally-owned connectivity/configuration with live sockets.
    /// A stale TCP connection is not usable after Android withdraws its validated
    /// default network (nor after its relay configuration is removed). Drop both
    /// the pool and the per-peer bindings before publishing `Unavailable`, so a
    /// later recovery must establish a fresh, explicitly configured link.
    async fn refresh_availability(&self) {
        let relay_usable = self.network_available.load(Ordering::Acquire)
            && self.has_relay_endpoint.load(Ordering::Acquire)
            && matches!(self.current_mode(), ConnectionMode::Relay { .. });
        let lan_usable = self.local_network_available.load(Ordering::Acquire)
            && self.lan_listener_port.load(Ordering::Acquire) != 0;
        if !relay_usable && !lan_usable {
            self.invalidate_relay_sessions().await;
            self.set_state(TransportState::Unavailable);
            self.record_failure(
                if !self.network_available.load(Ordering::Acquire)
                    && !self.local_network_available.load(Ordering::Acquire)
                {
                    "no validated WAN or local network"
                } else if !self.has_relay_endpoint.load(Ordering::Acquire) && !lan_usable {
                    "relay not configured"
                } else {
                    "relay identity/mode not configured"
                },
            );
            return;
        }
        // A blocking read is deliberately avoided here; an empty/unknown
        // endpoint list is represented by the existing Unavailable state and
        // `connect()` remains the final authority.
        if self.state.load() == TransportState::Unavailable {
            self.set_state(TransportState::Available);
        }
        self.clear_failure();
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        let _ = self.state_tx.send(TransportStateEvent {
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
    /// Returns `(write_half, abort_handle, key)` so the caller has the
    /// address before writing — avoids resolve-after-write for RF-40.
    async fn get_connection(
        &self,
        peer: &PeerInfo,
    ) -> Result<(SharedWriter, tokio::task::AbortHandle, ConnectionKey), TransportError> {
        let addr = self
            .resolve_relay(peer)
            .await
            .ok_or(TransportError::PeerNotFound)?;

        let mode = self.current_mode();
        let key = ConnectionKey {
            addr,
            scope: match &mode {
                ConnectionMode::Plain => ConnectionScope::Lan(peer.peer_id),
                ConnectionMode::Relay { own_peer_id, .. } => {
                    ConnectionScope::Relay(PeerId(*own_peer_id))
                }
            },
        };

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

        let now = Instant::now();
        if let Some((write, abort)) = self.pool.lock().await.acquire(now, key) {
            return Ok((write, abort, key));
        }

        let dial_lock = {
            let mut locks = self.connection_locks.lock().await;
            locks
                .entry(key)
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _dial = dial_lock.lock().await;
        if let Some((write, abort)) = self.pool.lock().await.acquire(Instant::now(), key) {
            return Ok((write, abort, key));
        }

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
        tcp.set_nodelay(true).ok();

        match mode {
            ConnectionMode::Plain => {
                let (r, w) = tcp.into_split();
                let writer: SharedWriter = Arc::new(Mutex::new(Box::new(w)));
                let abort = self.spawn_reader(peer.peer_id, Box::new(r), None);
                self.record_successful_attempt();
                self.pool
                    .lock()
                    .await
                    .release(writer.clone(), abort.clone(), key, Instant::now());
                Ok((writer, abort, key))
            }
            ConnectionMode::Relay {
                ref server_name,
                own_peer_id,
            } => {
                let sn = parse_relay_server_name(server_name)?;
                #[cfg(not(test))]
                let connector = verified_relay_connector();
                #[cfg(test)]
                let connector = self
                    .tls_connector_override
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .unwrap_or_else(verified_relay_connector);
                let signer = self
                    .relay_signer
                    .lock()
                    .ok()
                    .and_then(|value| value.clone())
                    .ok_or_else(|| {
                        self.record_failure("relay identity signer unavailable");
                        TransportError::PolicyDenied("relay identity signer unavailable")
                    })?;
                let mut tls_stream =
                    match tokio::time::timeout(Duration::from_secs(5), connector.connect(sn, tcp))
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
                if tls_stream.get_ref().1.alpn_protocol() != Some(RELAY_ALPN) {
                    self.record_failed_attempt();
                    self.record_failure("relay ALPN mismatch");
                    return Err(TransportError::Protocol(
                        "relay did not negotiate iris-relay/1".into(),
                    ));
                }
                let challenge =
                    match Self::read_relay_frame(&mut tls_stream, Duration::from_secs(5)).await {
                        Some(RelayFrame::Challenge { nonce }) => nonce,
                        _ => {
                            self.record_failed_attempt();
                            self.record_failure("relay registration challenge missing");
                            return Err(TransportError::Protocol(
                                "relay registration challenge missing".into(),
                            ));
                        }
                    };
                let own_peer = PeerId(own_peer_id);
                let signature = signer.sign(&relay_protocol::registration_signable(
                    &challenge, &own_peer,
                ))?;
                let reg = relay_protocol::encode(&RelayFrame::Register {
                    peer_id: own_peer,
                    signature,
                })
                .map_err(|e| TransportError::Protocol(e.to_string()))?;
                tokio::time::timeout(Duration::from_secs(5), async {
                    tls_stream.write_all(&reg).await?;
                    tls_stream.flush().await
                })
                .await
                .map_err(|_| TransportError::ConnectionFailed)??;
                match Self::read_relay_frame(&mut tls_stream, Duration::from_secs(5)).await {
                    Some(RelayFrame::Heartbeat) => {}
                    _ => {
                        self.record_failed_attempt();
                        self.record_failure("relay registration not acknowledged");
                        return Err(TransportError::Protocol(
                            "relay registration not acknowledged".into(),
                        ));
                    }
                }
                let (r, w) = tokio::io::split(tls_stream);
                let writer: SharedWriter = Arc::new(Mutex::new(Box::new(w)));
                let abort =
                    self.spawn_reader(peer.peer_id, Box::new(r), Some((own_peer, writer.clone())));
                self.record_successful_attempt();
                self.clear_failure();
                self.pool
                    .lock()
                    .await
                    .release(writer.clone(), abort.clone(), key, Instant::now());
                Ok((writer, abort, key))
            }
        }
    }

    fn record_failed_attempt(&self) {
        self.reconnect_attempts.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_attempt.lock() {
            *guard = Some(Instant::now());
        }
    }

    fn record_successful_attempt(&self) {
        self.reconnect_attempts.store(0, Ordering::Relaxed);
        if let Ok(mut guard) = self.last_attempt.lock() {
            *guard = None;
        }
    }

    /// Spawn a reader task for a new connection and return its abort handle.
    /// `relay_identity` selects relay-frame vs plain-frame decoding.
    /// A clone is stored in `self.readers` for shutdown (RF-37); the returned
    /// handle is given to the pool so over-cap eviction kills the task.
    fn spawn_reader(
        &self,
        peer_id: PeerId,
        read: BoxReader,
        relay_identity: Option<(PeerId, SharedWriter)>,
    ) -> tokio::task::AbortHandle {
        let incoming = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        let pending_acks = self.pending_acks.clone();
        let handle = tokio::spawn(async move {
            if let Some((own_peer_id, response_writer)) = relay_identity {
                Self::relay_reader_task(
                    peer_id,
                    own_peer_id,
                    read,
                    incoming,
                    transport_id.clone(),
                    pending_acks,
                    response_writer,
                )
                .await;
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
            guards.retain(|guard| !guard.is_finished());
            guards.push(abort.clone());
        }
        abort
    }

    /// Send directly to a NSD-discovered LAN peer over plain TCP, bypassing the
    /// relay path even when the transport is in Relay mode.
    async fn send_lan_direct(
        &self,
        peer: PeerId,
        addr: SocketAddr,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        let frame = encode_frame(message)?;
        let now = Instant::now();
        let key = ConnectionKey {
            addr,
            scope: ConnectionScope::Lan(peer),
        };
        let existing = {
            let mut pool = self.pool.lock().await;
            pool.acquire(now, key)
        };
        let (write, abort) = if let Some(pair) = existing {
            pair
        } else {
            let dial_lock = {
                let mut locks = self.connection_locks.lock().await;
                locks
                    .entry(key)
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            };
            let _dial = dial_lock.lock().await;
            let pooled_after_lock = {
                let mut pool = self.pool.lock().await;
                pool.acquire(Instant::now(), key)
            };
            if let Some(pair) = pooled_after_lock {
                pair
            } else {
                let tcp = tokio::time::timeout(Duration::from_secs(3), TcpStream::connect(addr))
                    .await
                    .map_err(|_| TransportError::ConnectionFailed)?
                    .map_err(TransportError::from)?;
                tcp.set_nodelay(true).ok();
                let mut tcp = tcp;
                self.authenticate_lan_client(&mut tcp, peer).await?;
                let (r, w) = tcp.into_split();
                let abort = self.spawn_reader(peer, Box::new(r), None);
                let write = Arc::new(Mutex::new(Box::new(w) as BoxWriter));
                self.pool
                    .lock()
                    .await
                    .release(write.clone(), abort.clone(), key, Instant::now());
                (write, abort)
            }
        };
        let write_res: Result<(), std::io::Error> = {
            let mut writer = write.lock().await;
            tokio::time::timeout(Duration::from_secs(5), async {
                writer.write_all(&frame).await?;
                writer.flush().await
            })
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "LAN write timeout"))?
        };
        if let Err(e) = write_res {
            abort.abort();
            self.pool.lock().await.remove(key);
            return Err(e.into());
        }
        let now = Instant::now();
        self.ewma.record_send(frame.len());
        self.pool.lock().await.release(write, abort, key, now);
        Ok(SendReceipt {
            peer_id: peer,
            bytes_sent: frame.len(),
            sent_at: now,
        })
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
        let _configuration = self.config_gate.read().await;
        self.set_state(TransportState::Connecting);
        let addr = self.resolve_relay(peer).await.ok_or_else(|| {
            // Restore the pre-call state so a failed connect never leaves the
            // transport stuck in Connecting (RED-0001-13).
            self.set_state(TransportState::Available);
            TransportError::PeerNotFound
        })?;
        *self.relay_addr.lock().await = Some(addr);
        self.peer_relays.lock().await.insert(peer.peer_id, addr);
        let (conn, abort, conn_key) = match self.get_connection(peer).await {
            Ok(triple) => triple,
            Err(e) => {
                let mut bindings = self.peer_relays.lock().await;
                if bindings.get(&peer.peer_id) == Some(&addr) {
                    bindings.remove(&peer.peer_id);
                }
                self.set_state(TransportState::Available);
                return Err(e);
            }
        };
        // Park the connection in the pool (keyed by relay) for send() reuse.
        self.pool
            .lock()
            .await
            .release(conn, abort, conn_key, Instant::now());
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
        let _configuration = self.config_gate.read().await;
        // LAN-direct: NSD-discovered peers bypass the relay pool and always use
        // plain TCP, regardless of whether the transport is in Relay mode.
        let lan_addr = self
            .lan_peer_addrs
            .lock()
            .ok()
            .and_then(|g| g.get(peer).copied());
        if let Some(addr) = lan_addr {
            match self.send_lan_direct(*peer, addr, message).await {
                Ok(receipt) => return Ok(receipt),
                Err(error) => {
                    tracing::warn!(peer = ?peer, %addr, error = %error, "LAN send failed; evicting stale mapping and trying relay");
                    self.remove_lan_peer(peer).await;
                    self.record_failure(format!("LAN direct failed: {error}"));
                }
            }
        }

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
        let (write, reader_abort, key) = self.get_connection(&peer_info).await?;
        // In relay mode, wrap the IRIS envelope in a RelayFrame::Route.
        // In plain mode, use the simple [version][len][payload] wire format.
        let frame = match self.current_mode() {
            ConnectionMode::Relay { own_peer_id, .. } => {
                relay_protocol::encode(&RelayFrame::Route {
                    route_id: *message.message_id.as_bytes(),
                    source: PeerId(own_peer_id),
                    target: *peer,
                    payload: message.payload.clone(),
                })
                .map_err(|e| TransportError::Protocol(e.to_string()))?
            }
            ConnectionMode::Plain => encode_frame(message)?,
        };
        let ack_rx = if matches!(self.current_mode(), ConnectionMode::Relay { .. }) {
            let (tx, rx) = oneshot::channel();
            self.pending_acks
                .lock()
                .await
                .insert((*message.message_id.as_bytes(), *peer), tx);
            Some(rx)
        } else {
            None
        };
        let write_res: Result<(), std::io::Error> = {
            let mut writer = write.lock().await;
            tokio::time::timeout(Duration::from_secs(5), async {
                writer.write_all(&frame).await?;
                writer.flush().await
            })
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "relay write timeout"))?
        };
        // On write failure the link is dead: abort the reader immediately
        // (don't wait for its 10 s timeout), mark Degraded, and discard the
        // socket so the reader marks the transport Available on its exit
        // (RED-0001-02 zombie state).
        if let Err(e) = write_res {
            self.pending_acks
                .lock()
                .await
                .remove(&(*message.message_id.as_bytes(), *peer));
            reader_abort.abort();
            self.pool.lock().await.remove(key);
            self.set_state(TransportState::Degraded);
            return Err(e.into());
        }
        if let Some(rx) = ack_rx {
            let ack_result = tokio::time::timeout(Duration::from_secs(5), rx).await;
            self.pending_acks
                .lock()
                .await
                .remove(&(*message.message_id.as_bytes(), *peer));
            let status = match ack_result {
                Ok(Ok(status)) => status,
                Ok(Err(_)) | Err(_) => {
                    self.record_failure("relay acknowledgement timed out");
                    reader_abort.abort();
                    self.pool.lock().await.remove(key);
                    self.set_state(TransportState::Degraded);
                    return Err(TransportError::ConnectionFailed);
                }
            };
            let now = Instant::now();
            self.ewma.record_send(frame.len());
            self.pool
                .lock()
                .await
                .release(write, reader_abort, key, now);
            match status {
                RelayAckStatus::ForwardedToLiveSession => {}
                RelayAckStatus::TargetOffline => {
                    self.record_failure("relay target offline");
                    return Err(TransportError::NotConnected);
                }
                RelayAckStatus::TargetBackpressured => {
                    self.record_failure("relay target backpressured");
                    return Err(TransportError::Busy);
                }
            }
        } else {
            let now = Instant::now();
            self.ewma.record_send(frame.len());
            self.pool
                .lock()
                .await
                .release(write, reader_abort, key, now);
        }
        self.clear_failure();
        let sent_at = Instant::now();
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: frame.len(),
            sent_at,
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
        if let Ok(mut sessions) = self.lan_sessions.lock() {
            for session in sessions.drain(..) {
                session.abort();
            }
        }
        self.pool.lock().await.connections.clear();
        self.connection_locks.lock().await.clear();
        self.peer_relays.lock().await.clear();
        self.pending_acks.lock().await.clear();
        *self.relay_addr.lock().await = None;
        self.lan_listener_port.store(0, Ordering::Release);
        self.local_network_available.store(false, Ordering::Release);
        if let Ok(mut peers) = self.lan_peer_addrs.lock() {
            peers.clear();
        }
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

    struct TestIdentitySigner(ed25519_dalek::SigningKey);

    impl RelaySigner for TestIdentitySigner {
        fn sign(&self, message: &[u8]) -> Result<[u8; 64], TransportError> {
            Ok(ed25519_dalek::Signer::sign(&self.0, message).to_bytes())
        }
    }

    fn install_test_identity(transport: &InternetTransport, seed: u8) -> PeerId {
        let signing = ed25519_dalek::SigningKey::from_bytes(&[seed; 32]);
        let peer = PeerId(signing.verifying_key().to_bytes());
        let signer = Arc::new(TestIdentitySigner(signing));
        transport.set_local_identity(peer, signer.clone());
        transport.set_relay_signer(signer);
        peer
    }

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
            // Non-decreasing: the 30s cap flattens the sequence at attempts 6+.
            assert!(ms >= last, "must not decrease: {last} -> {ms}");
            last = ms;
        }
        assert_eq!(
            backoff_ms(10, 0),
            22_500,
            "capped base 30s - the full 25% jitter with seed 0"
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
        assert_eq!(transport.state(), TransportState::Unavailable);
        transport.set_relay_mode("relay.example".into(), [7; 32]);
        transport.refresh_availability().await;
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

    #[tokio::test]
    async fn connection_pool_never_cross_reuses_authenticated_identity_scope() {
        let addr: SocketAddr = "127.0.0.1:7890".parse().unwrap();
        let peer_a = PeerId([0xA1; 32]);
        let peer_b = PeerId([0xB2; 32]);
        let key_a = ConnectionKey {
            addr,
            scope: ConnectionScope::Lan(peer_a),
        };
        let key_b = ConnectionKey {
            addr,
            scope: ConnectionScope::Lan(peer_b),
        };
        let relay_key = ConnectionKey {
            addr,
            scope: ConnectionScope::Relay(peer_a),
        };
        let (_read, write) = tokio::io::duplex(64);
        let writer: SharedWriter = Arc::new(Mutex::new(Box::new(write)));
        let reader = tokio::spawn(std::future::pending::<()>());
        let mut pool = ConnectionPool::new(3, Duration::from_secs(90));
        pool.release(writer, reader.abort_handle(), key_a, Instant::now());

        assert!(pool.acquire(Instant::now(), key_a).is_some());
        assert!(pool.acquire(Instant::now(), key_b).is_none());
        assert!(pool.acquire(Instant::now(), relay_key).is_none());
        assert_eq!(pool.connections.len(), 1);
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
                    (750..=37_500).contains(&ms),
                    "attempt {attempt} seed {seed}: {ms} out of [750, 37500]"
                );
                assert!(ms >= last, "seed {seed}: backoff decreased {last} -> {ms}");
                last = ms;
            }
        }
        // Cap-at-attempt: base saturates at BACKOFF_MAX_MS from attempt 6 on, so
        // every later attempt must yield the identical jittered value per seed.
        for seed in 0..100u64 {
            let capped = backoff_ms(6, seed);
            assert_eq!(
                backoff_ms(7, seed),
                capped,
                "seed {seed}: attempt 7 != capped"
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
                (22_500..=37_500).contains(&capped),
                "seed {seed}: capped {capped} out of jitter band"
            );
        }
    }

    // ---- Tier-5 integration tests ----

    #[tokio::test]
    async fn lan_listener_accepts_inbound_plain_tcp_frame() {
        use futures_util::StreamExt;

        let transport = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let server_id = install_test_identity(&transport, 0x31);
        transport.set_local_network_available(true).await;

        let port = transport.start_lan_listener().await.unwrap();
        assert!(port > 0, "listener must bind a non-zero port");

        // A second transport must authenticate before sending the plain frame.
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let client = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let client_id = install_test_identity(&client, 0x32);
        client.register_lan_peer(server_id, addr);

        let mut inbox = transport.incoming_messages();
        client
            .send(
                &server_id,
                &sample_message(b"hello from lan", MessagePriority::P3),
            )
            .await
            .unwrap();
        let arrived = tokio::time::timeout(Duration::from_secs(2), async { inbox.next().await })
            .await
            .expect("message must arrive within 2s")
            .expect("stream must yield a message");

        assert_eq!(arrived.payload, b"hello from lan");
        assert_eq!(arrived.peer_id, client_id);
        client.shutdown().await.unwrap();
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn lan_listener_accepts_authenticated_ipv6_loopback() {
        use futures_util::StreamExt;

        let server = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let server_id = install_test_identity(&server, 0x35);
        server.set_local_network_available(true).await;
        let port = server.start_lan_listener().await.unwrap();

        let client = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let client_id = install_test_identity(&client, 0x36);
        client.register_lan_peer(server_id, format!("[::1]:{port}").parse().unwrap());
        let mut inbox = server.incoming_messages();
        client
            .send(
                &server_id,
                &sample_message(b"ipv6 lan", MessagePriority::P2),
            )
            .await
            .unwrap();
        let arrived = tokio::time::timeout(Duration::from_secs(2), inbox.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(arrived.peer_id, client_id);
        assert_eq!(arrived.payload, b"ipv6 lan");
        client.shutdown().await.unwrap();
        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn lan_claim_with_wrong_expected_identity_is_rejected() {
        let server = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        install_test_identity(&server, 0x37);
        server.set_local_network_available(true).await;
        let port = server.start_lan_listener().await.unwrap();

        let client = InternetTransport::new(&[], InternetCostParams::default());
        install_test_identity(&client, 0x38);
        let forged_claim = PeerId([0x39; 32]);
        client.register_lan_peer(forged_claim, format!("127.0.0.1:{port}").parse().unwrap());
        assert!(matches!(
            client
                .send(
                    &forged_claim,
                    &sample_message(b"must not arrive", MessagePriority::P1),
                )
                .await,
            Err(TransportError::NotConnected)
        ));
        assert!(!client
            .lan_peer_addrs
            .lock()
            .unwrap()
            .contains_key(&forged_claim));
        client.shutdown().await.unwrap();
        server.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn lan_listener_is_idempotent() {
        let transport = InternetTransport::new(&[], InternetCostParams::default());
        install_test_identity(&transport, 0x33);
        transport.set_local_network_available(true).await;
        let p1 = transport.start_lan_listener().await.unwrap();
        let p2 = transport.start_lan_listener().await.unwrap();
        assert_eq!(p1, p2, "second call must return same port");
        assert_eq!(transport.state(), TransportState::Available);
        transport.shutdown().await.unwrap();
        assert_eq!(transport.lan_listener_port(), 0);
        assert_eq!(transport.state(), TransportState::Unavailable);

        transport.set_local_network_available(true).await;
        let restarted = transport.start_lan_listener().await.unwrap();
        assert!(restarted > 0);
        assert_eq!(transport.state(), TransportState::Available);
        transport.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn relay_server_routes_frame_between_two_plain_clients() {
        use crate::transport::relay_protocol::RelayFrame;
        use std::collections::HashMap as HMap;
        use tokio::sync::Mutex as TokMutex;

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
                    let RelayFrame::Register { peer_id, .. } = frame else {
                        return;
                    };
                    rmap.lock().await.insert(peer_id.0, w);
                    loop {
                        let Some(frame) = read_relay_frame(&mut r).await else {
                            break;
                        };
                        if let RelayFrame::Route {
                            route_id,
                            source,
                            target,
                            payload,
                        } = frame
                        {
                            let out = relay_protocol::encode(&RelayFrame::Route {
                                route_id,
                                source,
                                target,
                                payload,
                            })
                            .unwrap();
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
        stream_a
            .write_all(
                &relay_protocol::encode(&RelayFrame::Register {
                    peer_id: PeerId(id_a),
                    signature: [1; 64],
                })
                .unwrap(),
            )
            .await
            .unwrap();
        stream_a.flush().await.unwrap();

        // Register B.
        let mut stream_b = TcpStream::connect(relay_addr).await.unwrap();
        stream_b
            .write_all(
                &relay_protocol::encode(&RelayFrame::Register {
                    peer_id: PeerId(id_b),
                    signature: [2; 64],
                })
                .unwrap(),
            )
            .await
            .unwrap();
        stream_b.flush().await.unwrap();

        // Let relay register both.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // A → B.
        stream_a
            .write_all(
                &relay_protocol::encode(&RelayFrame::Route {
                    route_id: [3; 16],
                    source: PeerId(id_a),
                    target: PeerId(id_b),
                    payload: b"cross-relay delivery".to_vec(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
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
                route_id: [3; 16],
                source: PeerId(id_a),
                target: PeerId(id_b),
                payload: b"cross-relay delivery".to_vec(),
            }
        );
    }

    async fn read_relay_frame(
        r: &mut tokio::net::tcp::OwnedReadHalf,
    ) -> Option<relay_protocol::RelayFrame> {
        use crate::transport::relay_protocol::{MAX_RELAY_PAYLOAD, RELAY_HEADER_LEN};
        let mut hdr = [0u8; RELAY_HEADER_LEN];
        r.read_exact(&mut hdr).await.ok()?;
        let mut lb = [0u8; 4];
        lb.copy_from_slice(&hdr[66..70]);
        let plen = u32::from_le_bytes(lb) as usize;
        if plen > MAX_RELAY_PAYLOAD {
            return None;
        }
        let mut full = vec![0u8; RELAY_HEADER_LEN + plen];
        full[..RELAY_HEADER_LEN].copy_from_slice(&hdr);
        if plen > 0 {
            r.read_exact(&mut full[RELAY_HEADER_LEN..]).await.ok()?;
        }
        relay_protocol::decode(&full).ok()
    }

    async fn read_relay_frame_from_stream(s: &mut TcpStream) -> Option<relay_protocol::RelayFrame> {
        use crate::transport::relay_protocol::{MAX_RELAY_PAYLOAD, RELAY_HEADER_LEN};
        let mut hdr = [0u8; RELAY_HEADER_LEN];
        s.read_exact(&mut hdr).await.ok()?;
        let mut lb = [0u8; 4];
        lb.copy_from_slice(&hdr[66..70]);
        let plen = u32::from_le_bytes(lb) as usize;
        if plen > MAX_RELAY_PAYLOAD {
            return None;
        }
        let mut full = vec![0u8; RELAY_HEADER_LEN + plen];
        full[..RELAY_HEADER_LEN].copy_from_slice(&hdr);
        if plen > 0 {
            s.read_exact(&mut full[RELAY_HEADER_LEN..]).await.ok()?;
        }
        relay_protocol::decode(&full).ok()
    }

    // ---- Gap 1+2: TLS relay end-to-end test ----

    #[tokio::test]
    async fn relay_tls_roundtrip_via_internet_transport() {
        use crate::transport::test_certs::{TEST_RELAY_CERT_DER, TEST_RELAY_KEY_DER};
        use crate::transport::tls::{test_relay_connector_for_cert, RELAY_ALPN};
        use futures_util::StreamExt;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
        use rustls::ServerConfig;
        use std::collections::HashMap as HMap;
        use tokio::sync::Mutex as TokMutex;
        use tokio_rustls::TlsAcceptor;

        // 1. Build TLS server config from hardcoded test cert.
        let cert_der = CertificateDer::from(TEST_RELAY_CERT_DER.to_vec());
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(TEST_RELAY_KEY_DER.to_vec()));
        let mut server_config =
            ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
                .with_no_client_auth()
                .with_single_cert(vec![cert_der], key_der)
                .unwrap();
        server_config.alpn_protocols = vec![RELAY_ALPN.to_vec()];
        let acceptor = TlsAcceptor::from(Arc::new(server_config));

        // 2. Start in-process TLS relay server.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let relay_addr = listener.local_addr().unwrap();

        type TlsRouter = Arc<TokMutex<HMap<[u8; 32], BoxWriter>>>;
        let router: TlsRouter = Arc::new(TokMutex::new(HMap::new()));
        let router_srv = router.clone();

        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                tcp.set_nodelay(true).ok();
                let acc = acceptor.clone();
                let rmap = router_srv.clone();
                tokio::spawn(async move {
                    let mut tls = acc.accept(tcp).await.unwrap();
                    let challenge = [generation_byte(&rmap); 32];
                    let challenge_frame =
                        relay_protocol::encode(&RelayFrame::Challenge { nonce: challenge })
                            .unwrap();
                    tls.write_all(&challenge_frame).await.unwrap();
                    tls.flush().await.unwrap();
                    let (mut r, mut w) = tokio::io::split(tls);
                    // Use read_relay_frame_dyn for TLS split halves (not OwnedReadHalf).
                    let frame = read_relay_frame_dyn(&mut r).await.unwrap();
                    let RelayFrame::Register { peer_id, .. } = frame else {
                        return;
                    };
                    let registered = relay_protocol::encode(&RelayFrame::Heartbeat).unwrap();
                    w.write_all(&registered).await.unwrap();
                    w.flush().await.unwrap();
                    rmap.lock().await.insert(peer_id.0, Box::new(w));
                    loop {
                        let Some(f) = read_relay_frame_dyn(&mut r).await else {
                            break;
                        };
                        if let RelayFrame::Route {
                            route_id,
                            source,
                            target,
                            payload,
                        } = f
                        {
                            let out = relay_protocol::encode(&RelayFrame::Route {
                                route_id,
                                source,
                                target,
                                payload,
                            })
                            .unwrap();
                            let mut map = rmap.lock().await;
                            if let Some(w) = map.get_mut(&target.0) {
                                w.write_all(&out).await.ok();
                                w.flush().await.ok();
                            }
                            if let Some(w) = map.get_mut(&source.0) {
                                let ack = relay_protocol::encode(&RelayFrame::Ack {
                                    route_id,
                                    target,
                                    status: RelayAckStatus::ForwardedToLiveSession,
                                })
                                .unwrap();
                                w.write_all(&ack).await.ok();
                                w.flush().await.ok();
                            }
                        }
                    }
                });
            }
        });

        struct TestRelaySigner(ed25519_dalek::SigningKey);
        impl RelaySigner for TestRelaySigner {
            fn sign(&self, message: &[u8]) -> Result<[u8; 64], TransportError> {
                Ok(ed25519_dalek::Signer::sign(&self.0, message).to_bytes())
            }
        }
        let key_a = ed25519_dalek::SigningKey::from_bytes(&[0xAA; 32]);
        let key_b = ed25519_dalek::SigningKey::from_bytes(&[0xBB; 32]);
        let id_a = PeerId(key_a.verifying_key().to_bytes());
        let id_b = PeerId(key_b.verifying_key().to_bytes());

        // Custom connector that trusts the embedded test cert.
        let test_conn = test_relay_connector_for_cert(TEST_RELAY_CERT_DER);

        // Two transports in Relay mode.
        let ta = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        ta.set_network_available(true).await;
        ta.set_relay_signer(Arc::new(TestRelaySigner(key_a)));
        ta.set_test_tls_connector(test_conn.clone());
        ta.configure_relay(
            vec!["127.0.0.1:1".parse().unwrap(), relay_addr],
            Some("localhost".into()),
            id_a.0,
        )
        .await;

        let tb = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        tb.set_network_available(true).await;
        tb.set_relay_endpoints(vec![relay_addr]).await;
        tb.set_relay_mode("localhost".into(), id_b.0);
        tb.set_relay_signer(Arc::new(TestRelaySigner(key_b)));
        tb.set_test_tls_connector(test_conn.clone());

        let peer_a = PeerInfo {
            peer_id: id_a,
            addresses: vec![relay_addr],
            transport_addresses: Vec::new(),
            last_seen: None,
        };

        // Both transports connect (register) to the relay.
        ta.connect_relay_peer(id_b)
            .await
            .expect("ta must fail over from the first dead endpoint");
        tb.connect(&peer_a).await.expect("tb connect");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Subscribe to tb's inbox before sending.
        let mut b_inbox = tb.incoming_messages();

        // Install a stale LAN route. send() must evict it and fall back to the
        // already-authenticated relay binding rather than failing permanently.
        ta.register_lan_peer(id_b, "127.0.0.1:9".parse().unwrap());

        // ta sends to id_b through the relay after direct-LAN failure.
        let msg = sample_message(b"tls relay delivery", MessagePriority::P2);
        ta.send(&id_b, &msg).await.expect("ta send");

        let received = tokio::time::timeout(Duration::from_secs(3), async { b_inbox.next().await })
            .await
            .expect("message must arrive within 3s")
            .expect("stream must yield a message");

        assert_eq!(received.payload, b"tls relay delivery");
        ta.shutdown().await.unwrap();
        tb.shutdown().await.unwrap();
    }

    fn generation_byte<T>(_value: &T) -> u8 {
        0x5a
    }

    // Generic relay-frame reader for BoxReader (used in TLS relay test).
    async fn read_relay_frame_dyn<R: tokio::io::AsyncRead + Unpin>(
        read: &mut R,
    ) -> Option<relay_protocol::RelayFrame> {
        use crate::transport::relay_protocol::{MAX_RELAY_PAYLOAD, RELAY_HEADER_LEN};
        let mut hdr = [0u8; RELAY_HEADER_LEN];
        read.read_exact(&mut hdr).await.ok()?;
        let mut lb = [0u8; 4];
        lb.copy_from_slice(&hdr[66..70]);
        let plen = u32::from_le_bytes(lb) as usize;
        if plen > MAX_RELAY_PAYLOAD {
            return None;
        }
        let mut full = vec![0u8; RELAY_HEADER_LEN + plen];
        full[..RELAY_HEADER_LEN].copy_from_slice(&hdr);
        if plen > 0 {
            read.read_exact(&mut full[RELAY_HEADER_LEN..]).await.ok()?;
        }
        relay_protocol::decode(&full).ok()
    }

    // ---- Gap 3: LAN-direct send bypasses relay pool ----

    #[tokio::test]
    async fn lan_direct_send_bypasses_relay_mode() {
        use futures_util::StreamExt;

        let server = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let lan_peer_id = install_test_identity(&server, 0xCC);
        server.set_local_network_available(true).await;
        let port = server.start_lan_listener().await.unwrap();
        let lan_addr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut inbox = server.incoming_messages();

        // Transport is in Relay mode but the peer is registered as a LAN peer.
        let transport = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));
        let own_id = install_test_identity(&transport, 0xDD);
        transport.set_network_available(true).await;
        transport
            .set_relay_endpoints(vec!["127.0.0.1:1".parse().unwrap()])
            .await;
        transport.set_relay_mode("unreachable-relay.invalid".into(), own_id.0);
        transport.register_lan_peer(lan_peer_id, lan_addr);

        let msg = sample_message(b"lan direct", MessagePriority::P1);
        // send() should succeed even though the relay is unreachable.
        transport
            .send(&lan_peer_id, &msg)
            .await
            .expect("LAN direct send");

        let received = tokio::time::timeout(Duration::from_secs(2), inbox.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(received.payload, b"lan direct");
        assert_eq!(received.peer_id, own_id);
        transport.shutdown().await.unwrap();
        server.shutdown().await.unwrap();
    }
}
