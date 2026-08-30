//! `DesktopEngine` — in-process host of the iris-core engine for the desktop
//! shell (DESKTOP_DESIGN.md §4, D4/D8/D9).
//!
//! The shell is a *thin adapter*: it owns the `TransportManager`, registers the
//! available transports (loopback `SimulatedTransport` always; `InternetTransport`
//! when `IRIS_RELAY_ADDR` is configured), builds the `MessageEngine` over the
//! STORE-001 storage seam, and wires inbound transport frames into
//! `process_incoming`. Protocol/routing/telemetry/storage all live in iris-core.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::StreamExt;
use rand::RngCore;
use tokio::sync::broadcast;

use iris_core::discovery::{DiscoveryConfig, DiscoveryManager};
use iris_core::message::{MessagePriority, PeerId};
use iris_core::message_engine::crypto::DevCryptoProvider;
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{InboundOutcome, MessageEngine, MessageEngineConfig};
use iris_core::observability::MetricsRegistry;
use iris_core::protocol::{ContentType, Envelope, MessageId, PROTOCOL_VERSION};
use iris_core::transport::internet::{InternetCostParams, InternetTransport};
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::{Transport, TransportId, TransportManager, TransportState};

use crate::identity::DesktopIdentity;
use crate::types::{
    parse_peer_id, EngineMetricsView, IncomingMessageView, MeshStatus, MessageIdView, MetricView,
    TransportStatus,
};

/// Configuration for the desktop engine host.
#[derive(Debug, Clone)]
pub struct DesktopEngineConfig {
    /// This node's 32-byte identity. The production path ignores this and
    /// derives the PeerId from the persisted identity (`DesktopIdentity`);
    /// the field remains for test seams (`with_node_id`) that use a dev
    /// provider.
    pub node_id: [u8; 32],
    /// Optional relay endpoints for the Internet transport (comma-separated
    /// `IRIS_RELAY_ADDR`). Empty = loopback only.
    pub relay_addrs: Vec<SocketAddr>,
    /// Test seam: use the inert dev provider + explicit `node_id` (never set in
    /// production). `with_node_id` / `with_transport` set this.
    pub dev: bool,
}

impl Default for DesktopEngineConfig {
    fn default() -> Self {
        Self {
            node_id: default_node_id(),
            relay_addrs: parse_relay_env(),
            dev: false,
        }
    }
}

/// The desktop shell's engine host (Send + Sync, managed by Tauri State).
pub struct DesktopEngine {
    node_id: [u8; 32],
    engine: Arc<MessageEngine>,
    manager: Arc<TransportManager>,
    transports: Vec<TransportId>,
    identity: Option<Arc<DesktopIdentity>>,
    /// Active UI inbox pump (SEC6_AUDIT D2): repeated `subscribe_inbox` IPC calls
    /// (webview reloads) previously accumulated dormant forwarder tasks forever;
    /// the newest subscription now aborts the previous one.
    inbox_task: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    /// GAP-7: owns the background scan loop that calls `Transport::connect()`
    /// on first contact with a peer — without it, `InternetTransport` in
    /// particular never leaves `Unavailable` (nothing else ever connects it)
    /// and the manager's own eligibility filter excludes it from selection.
    #[allow(dead_code)]
    discovery: Arc<DiscoveryManager>,
}

impl DesktopEngine {
    /// Build the engine host with the default (env-driven) configuration.
    pub async fn new() -> Result<Arc<Self>, String> {
        Self::with_config(DesktopEngineConfig::default()).await
    }

    /// Build the engine host with an explicit node id (tests / determinism).
    #[cfg(feature = "test-seams")]
    pub async fn with_node_id(node_id: [u8; 32]) -> Result<Arc<Self>, String> {
        Self::with_config(DesktopEngineConfig {
            node_id,
            dev: true,
            ..Default::default()
        })
        .await
    }

    /// Build the engine host over a caller-provided transport (loopback test
    /// wiring: two engines sharing one `SimulatedTransport` form a wire).
    #[cfg(feature = "test-seams")]
    pub async fn with_transport(
        node_id: [u8; 32],
        transport: Arc<dyn Transport>,
    ) -> Result<Arc<Self>, String> {
        let config = DesktopEngineConfig {
            node_id,
            relay_addrs: Vec::new(),
            dev: true,
        };
        Self::build(config, Some(transport)).await
    }

    /// Build the engine host from configuration (DESKTOP_DESIGN.md §4).
    pub async fn with_config(config: DesktopEngineConfig) -> Result<Arc<Self>, String> {
        Self::build(config, None).await
    }

    async fn build(
        config: DesktopEngineConfig,
        extra: Option<Arc<dyn Transport>>,
    ) -> Result<Arc<Self>, String> {
        let manager = Arc::new(TransportManager::new());

        let mut transports = Vec::with_capacity(1 + config.relay_addrs.len());

        // D8: loopback transport in dev mode only (proves the mesh path with no
        // hardware without polluting production routing with a self-echo sink).
        // A caller-provided `extra` transport REPLACES it (test wiring).
        match extra {
            Some(t) => {
                let id = t.transport_id().clone();
                manager.register(t).await.map_err(|e| e.to_string())?;
                transports.push(id);
            }
            None if config.dev => {
                let loopback = Arc::new(SimulatedTransport::new(
                    "loopback",
                    "Loopback (simulated)",
                    SimConfig {
                        packet_loss_rate: 0.0,
                        latency_base_ms: 5,
                        latency_spread_ms: 0,
                        ..Default::default()
                    },
                ));
                manager
                    .register(loopback.clone())
                    .await
                    .map_err(|e| e.to_string())?;
                transports.push(loopback.transport_id().clone());
            }
            None => {}
        }

        // D8: One internet transport for all configured relays (tried in order).
        // Constructing one-per-relay collides on the constant transport id.
        if !config.relay_addrs.is_empty() {
            let relay = Arc::new(InternetTransport::new(
                &config.relay_addrs,
                InternetCostParams::default(),
            ));
            manager
                .register(relay.clone())
                .await
                .map_err(|e| format!("register relay: {e}"))?;
            transports.push(relay.transport_id().clone());
        }

        // D9: STORE-001 seam — MemoryStorage by default; PgStorage later.
        let storage: Arc<dyn iris_core::message_engine::storage::MessageStorage> =
            Arc::new(MemoryStorage::new());

        // RED-0001 (IDENT_DESIGN §11): production identity is persisted and
        // drives the real crypto provider + trust-fed key directory. Only the
        // test seams (`with_node_id` / `with_transport`, `config.dev`) fall
        // back to the inert DevCryptoProvider.
        let identity = if config.dev {
            None
        } else {
            match DesktopIdentity::provision_or_load() {
                Ok(id) => Some(Arc::new(id)),
                Err(e) => return Err(e),
            }
        };

        let node_id = identity
            .as_ref()
            .map(|id| id.node_id())
            .unwrap_or(config.node_id);

        let (crypto, key_directory) = match &identity {
            Some(id) => {
                let provider: Arc<dyn iris_core::message_engine::crypto::CryptoProvider> =
                    Arc::new(id.crypto_provider());
                let dir: Arc<dyn iris_core::crypto::key_directory::KeyDirectory> =
                    Arc::new(id.trust_key_directory());
                (provider, Some(dir))
            }
            None => {
                let provider: Arc<dyn iris_core::message_engine::crypto::CryptoProvider> =
                    Arc::new(DevCryptoProvider::new());
                (provider, None)
            }
        };

        let engine = MessageEngine::new_with_telemetry(
            MessageEngineConfig {
                node_id,
                ..Default::default()
            },
            storage,
            crypto,
            manager.clone(),
            MetricsRegistry::new(),
        );
        if let Some(dir) = key_directory {
            engine.set_key_directory(dir);
        }

        // GAP-7: discovery is what actually calls `Transport::connect()` on
        // first contact — without it every send() fails NotConnected forever,
        // and InternetTransport specifically never leaves `Unavailable`.
        let discovery = Arc::new(DiscoveryManager::new(
            PeerId::from_bytes(node_id),
            DiscoveryConfig::default(),
        ));

        let this = Arc::new(DesktopEngine {
            node_id,
            engine,
            manager,
            transports,
            identity,
            inbox_task: std::sync::Mutex::new(None),
            discovery: discovery.clone(),
        });

        // Production receive path: every registered transport's frames feed the
        // engine. Runs on Tauri's async runtime so it dies with the app.
        let registered = this.transports.clone();
        for id in registered {
            if let Some(t) = this.manager.get(&id).await {
                this.spawn_inbox_forwarder(t.clone());
                discovery.register_transport(t).await;
            }
        }
        discovery.start().await;
        Ok(this)
    }

    /// Spawn a task forwarding one transport's inbound frames into the engine.
    /// Broadcast receivers are loss-tolerant by design; a full inbox simply
    /// drops the frame (same guarantee as the transport itself).
    fn spawn_inbox_forwarder(self: &Arc<Self>, transport: Arc<dyn Transport>) {
        let engine = self.engine.clone();
        let mut incoming = transport.incoming_messages();
        tauri::async_runtime::spawn(async move {
            while let Some(msg) = incoming.next().await {
                match engine.process_incoming(msg).await {
                    Ok(InboundOutcome::Delivered) | Ok(InboundOutcome::ReassembledDelivered) => {
                        tracing::debug!("desktop: inbound delivered");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!("desktop: inbound rejected: {e}"),
                }
            }
            // Stream closed — no reconnect path exists; operator must restart.
            tracing::warn!(
                "desktop: transport inbound stream closed; this forwarder will not recover"
            );
        });
    }

    /// Send a text message to `recipient_hex` (64-hex node id).
    pub async fn send_text(
        &self,
        recipient_hex: &str,
        text: &str,
        priority: u8,
    ) -> Result<MessageIdView, String> {
        if text.is_empty() {
            return Err("message text must not be empty".to_string());
        }
        let recipient = parse_peer_id(recipient_hex)?;
        let priority = MessagePriority::from_u8(priority)
            .ok_or_else(|| format!("priority must be 0-7, got {priority}"))?;
        let envelope = build_text_envelope(self.node_id, recipient, text, priority)?;
        self.engine
            .send_message(envelope.clone())
            .await
            .map_err(|e| e.to_string())?;
        Ok(MessageIdView::from_envelope(&envelope))
    }

    /// Process one raw inbound frame (test seam; production uses the
    /// auto-spawned forwarder).
    pub async fn process_incoming(
        &self,
        msg: iris_core::message::IncomingMessage,
    ) -> Result<InboundOutcome, String> {
        self.engine
            .process_incoming(msg)
            .await
            .map_err(|e| e.to_string())
    }

    /// Live delivered-message stream (test seam; UI uses `subscribe_inbox`).
    pub fn subscribe_delivered(&self) -> broadcast::Receiver<Envelope> {
        self.engine.delivered_messages()
    }

    /// Stream delivered messages to a Tauri IPC Channel (D5).
    ///
    /// Lag-tolerant by design (SEC6_AUDIT D1): a broadcast `Lagged` burst SKIPS
    /// the lost envelopes and keeps pumping — the receive path must never die
    /// quietly on a busy mesh. Ends only when the engine shuts down or the
    /// webview goes away. Re-subscribing aborts the previous pump (D2).
    pub fn subscribe_inbox(&self, channel: tauri::ipc::Channel<IncomingMessageView>) {
        let rx = self.engine.delivered_messages();
        let handle =
            tokio::task::spawn(pump_inbox(rx, move |view| channel.send(view).is_ok()));
        let mut slot = self.inbox_task.lock().expect("inbox slot poisoned");
        if let Some(prev) = slot.as_ref() {
            prev.abort();
        }
        *slot = Some(handle.abort_handle());
    }

    /// The node's 32-byte PeerId for outbound messages.
    pub fn node_id(&self) -> [u8; 32] {
        self.node_id
    }

    /// The provisioned debug identity (production path). `None` in dev seams.
    pub fn identity(&self) -> Option<Arc<DesktopIdentity>> {
        self.identity.clone()
    }

    /// Aggregate shell status (AC1/AC5).
    pub async fn mesh_status(&self) -> MeshStatus {
        let mut transports = Vec::with_capacity(self.transports.len());
        let mut online = false;
        for id in &self.transports {
            if let Some(t) = self.manager.get(id).await {
                let state = t.state();
                online |= state.as_u8() >= TransportState::Available.as_u8();
                transports.push(TransportStatus {
                    id: id.as_str().to_string(),
                    display: t.display_name().to_string(),
                    state: format!("{state:?}"),
                });
            }
        }
        MeshStatus {
            node_id_short: PeerIdShort(self.node_id).to_string(),
            transports,
            online,
            metrics: EngineMetricsView::from(self.engine.metrics()),
        }
    }

    /// OBS-001 telemetry snapshot — non-zero `iris.*_total` counters (AC4).
    pub fn telemetry(&self) -> Vec<MetricView> {
        let mut out: Vec<MetricView> = self
            .engine
            .telemetry()
            .snapshot()
            .into_iter()
            .filter(|(_, v)| *v > 0)
            .map(|(name, value)| MetricView {
                name: name.to_string(),
                value,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }
}

/// Build a `Text` envelope from UI input (fields mirror `build_ack`).
pub fn build_text_envelope(
    node_id: [u8; 32],
    recipient: iris_core::message::PeerId,
    text: &str,
    priority: MessagePriority,
) -> Result<Envelope, String> {
    // Enforced post-deserialization: the Tauri IPC layer materializes the
    // String before this check runs (SECURITY_POLICY prefers validate-before-
    // allocate). Accepted for v1 because ui/index.html caps chars at 60000 and
    // iris-core re-validates envelope sizes. NOTE (D7): HTML maxlength counts
    // UTF-16 code units while this measures UTF-8 bytes, so multibyte input
    // can pass the front-end guard and land here — intentional backstop.
    const MAX_TEXT_BYTES: usize = 60_000;
    if text.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "text too large for a single Text envelope ({MAX_TEXT_BYTES} bytes max)"
        ));
    }
    let mut env = Envelope {
        version: PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: node_id.to_vec(),
        recipient_id: recipient.as_bytes().to_vec(),
        priority,
        ttl_seconds: 3600,
        timestamp: iris_core::message_engine::expiry::unix_now(),
        hop_count: 0,
        max_hops: None,
        payload_type: if priority == MessagePriority::P0 {
            ContentType::Sos
        } else {
            ContentType::Text
        },
        payload_size: text.len() as u64,
        payload_hash: [0u8; 32],
        payload: text.as_bytes().to_vec(),
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    };
    env.payload_hash = Envelope::compute_payload_hash(&env.payload);
    Ok(env)
}

/// Minimal 16-hex display helper for the local node id (privacy P2).
struct PeerIdShort([u8; 32]);

impl std::fmt::Display for PeerIdShort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for b in &self.0[..8] {
            write!(f, "{b:02x}")?;
        }
        Ok(())
    }
}

/// `IRIS_NODE_ID` hex override for the *dev seam* only. RED-0001 (§11): in the
/// production path the PeerId comes from the persisted identity and
/// `IRIS_NODE_ID` is display-only (see `identity.rs`).
fn default_node_id() -> [u8; 32] {
    if let Ok(hex) = std::env::var("IRIS_NODE_ID") {
        if let Ok(id) = parse_peer_id(&hex) {
            return *id.as_bytes();
        }
    }
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b
}

/// `IRIS_RELAY_ADDR` = comma-separated `SocketAddr`s. Invalid entries are
/// logged and skipped (never silent — a vanished relay looks like a network
/// outage; SEC6_AUDIT D4).
fn parse_relay_env() -> Vec<SocketAddr> {
    let raw = match std::env::var("IRIS_RELAY_ADDR") {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in raw.split(',') {
        let s = entry.trim();
        if s.is_empty() {
            continue;
        }
        match s.parse::<SocketAddr>() {
            Ok(addr) => out.push(addr),
            Err(e) => tracing::warn!("IRIS_RELAY_ADDR: ignoring invalid endpoint '{s}': {e}"),
        }
    }
    out
}

/// Forward delivered envelopes from the engine's broadcast channel to `send`.
///
/// `Lagged(n)` (receiver fell behind the ring buffer) logs and CONTINUES — the
/// skipped envelopes are gone but the stream lives (SEC6_AUDIT D1). `Closed`
/// (engine dropped its sender) or a `false` from `send` (webview gone) ends
/// the pump. Split out from `subscribe_inbox` so the lag contract has a
/// windowless regression test.
pub async fn pump_inbox(
    mut rx: broadcast::Receiver<Envelope>,
    mut send: impl FnMut(IncomingMessageView) -> bool,
) {
    loop {
        match rx.recv().await {
            Ok(env) => {
                if !send(IncomingMessageView::from_envelope(&env)) {
                    return; // consumer (webview) gone
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("desktop inbox lagged: {n} delivered envelope(s) skipped");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

/// Test hooks for the D1/D2 pump contract (integration tests construct
/// envelopes and observe pump termination without a Tauri runtime).
#[doc(hidden)]
pub mod pump_test_hooks {
    use iris_core::protocol::{ContentType, Envelope, MessageId, PROTOCOL_VERSION};

    pub fn make_envelope() -> Envelope {
        let mut env = Envelope {
            version: PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: vec![0xAA; 32],
            recipient_id: vec![0xBB; 32],
            priority: iris_core::message::MessagePriority::P4,
            ttl_seconds: 3600,
            timestamp: iris_core::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Text,
            payload_size: 4,
            payload_hash: [0u8; 32],
            payload: b"ping".to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);
        env
    }
}

/// Test-only wrapper around [`pump_inbox`] that signals termination via an
/// atomic flag (SEC6_AUDIT D1 integration test support).
#[doc(hidden)]
pub async fn pump_inbox_for_tests(
    rx: broadcast::Receiver<Envelope>,
    send: impl FnMut(IncomingMessageView) -> bool,
    ended: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    pump_inbox(rx, send).await;
    ended.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
mod d2_lifecycle {
    use super::*;

    #[tokio::test]
    async fn resubscribe_aborts_previous_pump() {
        let (_tx, rx) = broadcast::channel::<Envelope>(4);
        let task1 = tokio::spawn(pump_inbox(rx, |_| true));
        let first = task1.abort_handle();
        let first_check = first.clone();

        let (_tx2, rx2) = broadcast::channel::<Envelope>(4);
        let task2 = tokio::spawn(pump_inbox(rx2, |_| true));
        let second = task2.abort_handle();
        let second_check = second.clone();

        // Mirror of the swap performed inside subscribe_inbox.
        let mut slot: Option<tokio::task::AbortHandle> = Some(first);
        if let Some(prev) = slot.as_ref() {
            prev.abort();
        }
        slot = Some(second);

        // Give the aborted task a beat to observe cancellation.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(first_check.is_aborted(), "previous pump must be aborted on re-subscribe");
        assert!(!second_check.is_aborted(), "current pump must stay live");
        let _ = slot;
    }
}
