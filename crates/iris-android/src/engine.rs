//! `IrisEngine` — the Android engine host, mirroring `DesktopEngine`
//! (ANDROID_DESIGN §4, AC-4).
//!
//! AC-4 wiring: the engine owns a `tokio::runtime::Runtime` (explicit handle
//! pattern, issue #2576 workaround, D-2), a `TransportManager` with the three
//! transports registered over the injected FFI adapters (via `crate::bridge`),
//! and a `MessageEngine` over `MemoryStorage` + `AndroidCryptoProvider` (AN-1:
//! real Ed25519/X25519/ChaCha20-Poly1305; signing delegates to the Kotlin-side
//! `FfiCryptoSigner` so the Android Keystore TEE key never leaves Kotlin).
//! Inbound frames from every registered transport are forwarded into
//! `process_incoming` by auto-spawned tasks; delivered messages surface to the
//! Kotlin shell through `subscribe_inbox` (FfiInboxListener).

use std::sync::Arc;

use futures_util::StreamExt;
use tokio::runtime::{Handle, Runtime};

use iris_core::discovery::{DiscoveryConfig, DiscoveryManager};
use iris_core::message::{MessagePriority, NodeAdvertisement, PeerId};
use iris_core::message_engine::crypto::CryptoProvider;
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{
    expiry::unix_now, InboundOutcome, MessageEngine, MessageEngineConfig,
};
use iris_core::observability::MetricsRegistry;
use iris_core::protocol::{ContentType, Envelope, MessageId, PROTOCOL_VERSION};
use iris_core::transport::ble::BleTransport;
use iris_core::transport::internet::{InternetCostParams, InternetTransport};
use iris_core::transport::wifi_direct::WifiDirectTransport;
use iris_core::transport::wifiaware::WifiAwareTransport;
use iris_core::transport::{Transport, TransportId, TransportManager};

use crate::android_crypto::AndroidCryptoProvider;
use crate::bridge::{BleBridge, WifiAwareBridge, WifiDirectBridge};
use crate::ffi::ble_adapter::FfiBleAdapter;
use crate::ffi::crypto_signer::FfiCryptoSigner;
use crate::ffi::error::IrisFfiError;
use crate::ffi::wifi_aware_adapter::FfiWifiAwareAdapter;
use crate::ffi::wifi_direct_adapter::FfiWifiDirectAdapter;
use crate::ffi::x25519_provider::FfiX25519KeyProvider;

/// A delivered message handed to the Kotlin shell (`subscribe_inbox`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiIncomingMessage {
    pub sender_id: Vec<u8>,
    pub message_id: Vec<u8>,
    pub recipient_id: Vec<u8>,
    pub payload: Vec<u8>,
    /// 0 = P0 … 7 = P7.
    pub priority: u8,
    /// Unix epoch milliseconds at **local receipt** on this node (GAP-9): set
    /// from `unix_now()` when the delivered message is drained, *not* the
    /// sender's self-declared origination time (that field is peer-controlled
    /// and unsafe as an inbox sort key). The Kotlin side (`MeshRepository.toUi`)
    /// uses it for `receivedAtMs` ordering. There is currently no field
    /// carrying the true origination time — the UI cannot tell a freshly
    /// relayed hour-old message from a new one.
    pub received_at_ms: u64,
}

/// HV-3: one-shot structured diagnostic of what the mesh is doing right now.
/// Rendered by the shell's `/diag` command and also dumped to logcat (tag
/// `iriscore`, `iris.diag` event) on every call. This is the single readout
/// the hardware-verification loop and the Mobly `iris_bench` harness (HV-2)
/// consume — keep the field set stable.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiMeshSnapshot {
    /// This node's 64-hex identity.
    pub node_id_hex: String,
    /// One entry per registered transport, in registration order.
    pub transports: Vec<FfiTransportDiag>,
    /// Known neighbours from the `NeighborTable` (discovered peers + their
    /// per-transport links). Empty until discovery has seen a peer.
    pub neighbors: Vec<FfiNeighborDiag>,
    /// Message-engine lifetime counters (since process start / last reset).
    pub messages: FfiMessageMetrics,
    /// HV-6: the full raw `MetricsRegistry` counter set, sorted by name.
    pub counters: Vec<FfiCounter>,
    /// HV-86: the transport-event ring, oldest first (last ~128 events).
    pub recent_events: Vec<FfiLogEvent>,
}

/// Per-transport slice of [`FfiMeshSnapshot`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiTransportDiag {
    pub id: String,
    pub display_name: String,
    /// `TransportState` debug form: `Unavailable` | `Degraded` | `Available` |
    /// `Connected`.
    pub state: String,
    pub estimated_battery_ma: f32,
    pub bandwidth_available_bps: u64,
    /// 0.0 (idle) … 1.0 (saturated).
    pub congestion_level: f32,
}

/// Per-neighbour slice of [`FfiMeshSnapshot`].
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiNeighborDiag {
    pub peer_id_hex: String,
    /// `LinkedUp` (≥1 live link) or `LinkedDown` (all dropped, kept for TTL).
    pub state: String,
    /// One `"<transport-id>:<quality>"` string per live link, e.g.
    /// `"ble-android:Good"`.
    pub links: Vec<String>,
}

/// Message-engine counters in [`FfiMeshSnapshot`] (`MessageEngine::metrics()`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiMessageMetrics {
    pub sent: u64,
    pub delivered: u64,
    pub relayed: u64,
    pub dropped_duplicates: u64,
    pub expired: u64,
    pub delivery_failed: u64,
}

/// HV-6: one raw `MetricsRegistry` counter (name → lifetime value). The full
/// set is `metric::ALL` — message/route/SCF/gateway/security counters. Rendered
/// by `/stats` (non-zero only) and emitted as the `iris.kpi` logcat line so a
/// bench session's `adb logcat` capture is an analysable KPI trace.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiCounter {
    pub name: String,
    pub value: u64,
}

/// HV-86: one entry from the transport-event ring (`observability::ring`).
/// The last ~128 taxonomy events (`discovery.*` / `msg.*` / `engine.*` / …),
/// oldest first — so a `/diag` after a failure shows the run-up to it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLogEvent {
    /// Unix epoch milliseconds at capture.
    pub unix_ms: u64,
    /// `ERROR` | `WARN` | `INFO` | `DEBUG` | `TRACE`.
    pub level: String,
    /// Taxonomy name, e.g. `discovery.connect_failed`.
    pub event: String,
    /// Compact `message key=value …` detail; never payloads or full ids.
    pub detail: String,
}

/// Foreign-trait callback the Kotlin shell installs to receive delivered
/// messages. `#[async_trait]`-free: `on_message` is a synchronous notify.
#[uniffi::export(with_foreign)]
pub trait FfiInboxListener: Send + Sync + 'static {
    fn on_message(&self, message: FfiIncomingMessage);
}

/// HV-89 (interim): a manually-fed X25519 key directory.
///
/// `MessageEngine::seal_outbound` fails closed (PRY-33) when an addressed
/// recipient has no X25519 key. Android has no handshake-based key exchange
/// yet, so — until that lands — this accepts keys pushed in over
/// [`IrisEngine::register_peer_key`] (the `iris_bench` harness wires the two
/// bench phones' keys out-of-band; a future `/addkey` command could use it
/// too). Same shape as the desktop `TrustKeyDirectory`, just fed by hand.
struct BenchKeyDirectory(std::sync::Mutex<std::collections::HashMap<[u8; 32], [u8; 32]>>);

impl iris_core::crypto::key_directory::KeyDirectory for BenchKeyDirectory {
    fn x25519_pubkey(&self, node_id: &[u8; 32]) -> Option<[u8; 32]> {
        self.0.lock().ok()?.get(node_id).copied()
    }
}

/// The Android app's engine host. Owns the three injected platform adapters,
/// the transport manager, and the message engine — all driven by a tokio
/// runtime whose handle is threaded through every poll path.
#[derive(uniffi::Object)]
pub struct IrisEngine {
    runtime: Runtime,
    handle: Handle,
    manager: Arc<TransportManager>,
    engine: Arc<MessageEngine>,
    transports: Vec<TransportId>,
    internet: Arc<InternetTransport>,
    node_id: [u8; 32],
    /// HV-89 interim: the hand-fed X25519 key directory (see [`BenchKeyDirectory`]).
    keydir: Arc<BenchKeyDirectory>,
    /// AN-6: the Kotlin X25519 provider, kept so the shell can print this
    /// node's static public key (`/x25519`) for a peer to `/addkey`.
    x25519_provider: Option<Arc<dyn FfiX25519KeyProvider>>,
    /// GAP-7: owns the background scan loop that calls `Transport::connect()`
    /// on first contact with a peer. Without this, `deliver_outbound` can
    /// select a transport but every `send()` fails with `NotConnected`
    /// forever — nothing else in the engine ever establishes a link.
    #[allow(dead_code)]
    discovery: Arc<DiscoveryManager>,
}

#[uniffi::export]
impl IrisEngine {
    /// Build the engine host with the injected Kotlin adapters (D-2: the tokio
    /// runtime is constructed here and its handle drives every poller). The
    /// three transports are registered over FFI bridges; the MessageEngine and
    /// the per-transport inbox forwarders are wired before the engine returns.
    #[uniffi::constructor]
    pub fn new(
        ble: Arc<dyn FfiBleAdapter>,
        aware: Arc<dyn FfiWifiAwareAdapter>,
        direct: Arc<dyn FfiWifiDirectAdapter>,
        node_id: Vec<u8>,
        signer: Arc<dyn FfiCryptoSigner>,
    ) -> Result<Arc<Self>, IrisFfiError> {
        // HW-3: install the logcat bridge before anything else can emit a
        // tracing event worth seeing — every transport/discovery/message-
        // engine construction below this line is exactly the kind of
        // startup activity that was previously invisible.
        crate::logging::init();
        let node_id: [u8; 32] = node_id
            .try_into()
            .map_err(|_| IrisFfiError::InvalidArgument("node_id must be 32 bytes".into()))?;
        Self::build(
            ble,
            aware,
            direct,
            node_id,
            Arc::new(AndroidCryptoProvider::new(signer)),
            None,
        )
    }

    /// AN-6: constructor variant that wires the Kotlin-side `FfiX25519KeyProvider`
    /// (wraps `X25519StaticAd`) so DH is performed inside JCA and the raw static
    /// secret never crosses the FFI boundary. Use this constructor when the
    /// X25519 static ad is available; prefer `new` only for development/testing.
    #[uniffi::constructor]
    pub fn new_with_x25519(
        ble: Arc<dyn FfiBleAdapter>,
        aware: Arc<dyn FfiWifiAwareAdapter>,
        direct: Arc<dyn FfiWifiDirectAdapter>,
        node_id: Vec<u8>,
        signer: Arc<dyn FfiCryptoSigner>,
        x25519_provider: Arc<dyn FfiX25519KeyProvider>,
    ) -> Result<Arc<Self>, IrisFfiError> {
        crate::logging::init();
        let node_id: [u8; 32] = node_id
            .try_into()
            .map_err(|_| IrisFfiError::InvalidArgument("node_id must be 32 bytes".into()))?;
        Self::build(
            ble,
            aware,
            direct,
            node_id,
            Arc::new(AndroidCryptoProvider::with_x25519_provider(
                signer,
                x25519_provider.clone(),
            )),
            Some(x25519_provider),
        )
    }
}

impl IrisEngine {
    /// HV-1: the real construction path, parameterised on the crypto backend so
    /// the `#[cfg(test)]` module can build an engine over the inert
    /// `DevCryptoProvider` (via [`Self::new_for_test`]) without a Kotlin
    /// `FfiCryptoSigner`. Production always goes through [`Self::new`] with the
    /// real `AndroidCryptoProvider`. Kept out of the `#[uniffi::export]` block:
    /// `Arc<dyn CryptoProvider>` is not an FFI type.
    fn build(
        ble: Arc<dyn FfiBleAdapter>,
        aware: Arc<dyn FfiWifiAwareAdapter>,
        direct: Arc<dyn FfiWifiDirectAdapter>,
        node_id: [u8; 32],
        crypto: Arc<dyn CryptoProvider>,
        x25519_provider: Option<Arc<dyn FfiX25519KeyProvider>>,
    ) -> Result<Arc<Self>, IrisFfiError> {
        let runtime = Runtime::new().map_err(|e| IrisFfiError::IoError(e.to_string()))?;
        let handle = runtime.handle().clone();
        let keydir = Arc::new(BenchKeyDirectory(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        )));
        let keydir_for_engine = keydir.clone();

        // The engine's background tasks and the transport registrations all
        // need a live tokio context — build them inside `block_on` on the
        // engine's own runtime (the explicit-handle pattern, issue #2576).
        let (manager, engine, transports, discovery, internet) = runtime.block_on(async {
            let manager = Arc::new(TransportManager::new());

            let ble_t = Arc::new(BleTransport::new(Some(Arc::new(BleBridge::new(ble)))));
            let aware_t = Arc::new(WifiAwareTransport::new(Some(Arc::new(
                WifiAwareBridge::new(aware),
            ))));
            let direct_t = Arc::new(WifiDirectTransport::new(Some(Arc::new(
                WifiDirectBridge::new(direct),
            ))));
            let internet_t = Arc::new(InternetTransport::new(&[], InternetCostParams::default()));

            manager
                .register(ble_t.clone())
                .await
                .map_err(|e| IrisFfiError::Transport(e.to_string()))?;
            manager
                .register(aware_t.clone())
                .await
                .map_err(|e| IrisFfiError::Transport(e.to_string()))?;
            manager
                .register(direct_t.clone())
                .await
                .map_err(|e| IrisFfiError::Transport(e.to_string()))?;
            manager
                .register(internet_t.clone())
                .await
                .map_err(|e| IrisFfiError::Transport(e.to_string()))?;

            let transports = vec![
                ble_t.transport_id().clone(),
                aware_t.transport_id().clone(),
                direct_t.transport_id().clone(),
                internet_t.transport_id().clone(),
            ];

            let engine = MessageEngine::new_with_telemetry(
                MessageEngineConfig {
                    node_id,
                    ..Default::default()
                },
                Arc::new(MemoryStorage::new()),
                crypto,
                manager.clone(),
                MetricsRegistry::new(),
            );

            // HV-89 interim: give the engine a key directory so `seal_outbound`
            // has somewhere to look (still fails closed until a key is fed).
            engine.set_key_directory(keydir_for_engine);

            // CROSS-003: the engine defaults to NoopSecurityPolicy (all checks
            // permissive) until a real policy is installed. No platform ever
            // called set_security_policy(), so rate limiting, replay
            // protection, quota, and the emergency ACL were silently disabled
            // in production on every platform. Arm it here.
            engine.set_security_policy(std::sync::Arc::new(iris_core::FullSecurityPolicy::new(
                std::sync::Arc::new(iris_core::TrustStore::new()),
            )));

            let all: [Arc<dyn Transport>; 4] = [ble_t, aware_t, direct_t, internet_t.clone()];
            for t in &all {
                Self::spawn_inbox_forwarder(engine.clone(), t.clone());
            }

            // GAP-7: discovery is what actually calls `Transport::connect()`
            // on first contact — without it every send() fails NotConnected
            // forever. Register the same three transports it just registered
            // with the manager, then start its background scan loop.
            let discovery = Arc::new(DiscoveryManager::new(
                PeerId::from_bytes(node_id),
                DiscoveryConfig::default(),
            ));
            for t in &all {
                discovery.register_transport(t.clone()).await;
            }
            discovery.start().await;

            // MG-8: subscribe to topology events so the broadcast channel has
            // at least one live receiver and tx.send() stops returning Err.
            // Without this, every TransportStateChanged event sent by the
            // register() forwarder is silently discarded.
            let mut topo_rx = manager.topology_events();
            tokio::spawn(async move {
                loop {
                    match topo_rx.recv().await {
                        Ok(event) => {
                            tracing::debug!(event = ?event, "topology event");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(skipped = n, "topology_events lagged");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            });

            Ok::<_, IrisFfiError>((manager, engine, transports, discovery, internet_t))
        })?;

        Ok(Arc::new(Self {
            runtime,
            handle,
            manager,
            engine,
            transports,
            internet,
            node_id,
            keydir,
            x25519_provider,
            discovery,
        }))
    }
}

#[uniffi::export]
impl IrisEngine {
    /// Tier-5: install trusted relay endpoints supplied by Android settings or
    /// NSD resolution. Values must be numeric socket addresses; DNS resolution
    /// stays in the platform layer so network binding remains Android-aware.
    pub fn set_internet_relay_endpoints(&self, endpoints: Vec<String>) -> Result<(), IrisFfiError> {
        let parsed: Result<Vec<std::net::SocketAddr>, _> = endpoints
            .iter()
            .map(|value| value.parse::<std::net::SocketAddr>())
            .collect();
        let parsed = parsed.map_err(|_| {
            IrisFfiError::InvalidArgument("internet relay endpoints must be IP:port values".into())
        })?;
        self.handle
            .block_on(self.internet.set_relay_endpoints(parsed));
        Ok(())
    }

    /// Tier-5: apply Android's validated-network signal. `true` means an
    /// active validated network; it never implies a peer or relay is reachable.
    pub fn set_internet_network_available(&self, available: bool) {
        self.handle
            .block_on(self.internet.set_network_available(available));
    }

    /// Tier-5 / HV-43: start the local LAN TCP listener and return its bound
    /// port. The port is advertised via Android NSD (`_iris._tcp`) so that
    /// peers on the same infrastructure Wi-Fi can connect directly (plain-TCP
    /// path, no relay). Idempotent — repeated calls return the same port.
    pub fn start_internet_lan_listener(&self) -> Result<u16, IrisFfiError> {
        self.handle
            .block_on(self.internet.start_lan_listener())
            .map_err(|e| IrisFfiError::Transport(e.to_string()))
    }

    /// The node's 32-byte PeerId for outbound messages.
    pub fn node_id(&self) -> Vec<u8> {
        self.node_id.to_vec()
    }

    /// This node's 32-byte X25519 static public key as 64-hex — the value a
    /// peer must `register_peer_key` before it can send us addressed mail
    /// (HV-89 interim, until the discovery handshake carries it). `/x25519`.
    pub fn static_x25519_pubkey(&self) -> Result<Vec<u8>, IrisFfiError> {
        let provider = self
            .x25519_provider
            .as_ref()
            .ok_or_else(|| IrisFfiError::InvalidArgument("no x25519 provider wired".into()))?;
        provider.static_public_key()
    }

    /// HV-89 (interim): register a peer's 32-byte X25519 static public key so
    /// `seal_outbound` can encrypt addressed messages to it. Both args are
    /// 64-hex. Used by the `iris_bench` harness to wire the bench phones'
    /// keys out-of-band until handshake-based key exchange lands. Idempotent.
    pub fn register_peer_key(
        &self,
        peer_id_hex: &str,
        x25519_pub_hex: &str,
    ) -> Result<(), IrisFfiError> {
        let peer = decode_hex32(peer_id_hex)
            .ok_or_else(|| IrisFfiError::InvalidArgument("peer id must be 64 hex chars".into()))?;
        let key: [u8; 32] = decode_hex32(x25519_pub_hex).ok_or_else(|| {
            IrisFfiError::InvalidArgument("x25519 key must be 64 hex chars".into())
        })?;
        self.keydir
            .0
            .lock()
            .map_err(|_| IrisFfiError::IoError("keydir poisoned".into()))?
            .insert(peer, key);
        Ok(())
    }

    /// HV-3: point-in-time diagnostic of the mesh — registered transports and
    /// their state/cost, known neighbours and their live links, and the
    /// message-engine counters. Also emitted to logcat (`iris.diag`) so a
    /// failure can be inspected from a plain `adb logcat` capture.
    pub fn snapshot(&self) -> FfiMeshSnapshot {
        let manager = self.manager.clone();
        let discovery = self.discovery.clone();
        let ids = self.transports.clone();
        let (transports, neighbors) = self.handle.block_on(async move {
            let mut transports = Vec::with_capacity(ids.len());
            for id in &ids {
                if let Some(t) = manager.get(id).await {
                    let cost = t.cost_snapshot();
                    transports.push(FfiTransportDiag {
                        id: t.transport_id().as_str().to_string(),
                        display_name: t.display_name().to_string(),
                        state: format!("{:?}", t.state()),
                        estimated_battery_ma: cost.estimated_battery_ma,
                        bandwidth_available_bps: cost.bandwidth_available_bps,
                        congestion_level: cost.congestion_level,
                    });
                }
            }
            let neighbors = discovery
                .neighbors()
                .neighbor_summaries()
                .await
                .into_iter()
                .map(|n| FfiNeighborDiag {
                    peer_id_hex: n.peer_id.to_string(),
                    state: format!("{:?}", n.state),
                    links: n
                        .links
                        .iter()
                        .map(|l| format!("{}:{:?}", l.transport.as_str(), l.quality))
                        .collect(),
                })
                .collect();
            (transports, neighbors)
        });

        let m = self.engine.metrics();
        let mut counters: Vec<FfiCounter> = self
            .engine
            .telemetry()
            .snapshot()
            .into_iter()
            .map(|(name, value)| FfiCounter {
                name: name.to_string(),
                value,
            })
            .collect();
        counters.sort_by(|a, b| a.name.cmp(&b.name));
        let recent_events = iris_core::observability::ring::recent()
            .into_iter()
            .map(|e| FfiLogEvent {
                unix_ms: e.unix_ms,
                level: e.level.to_string(),
                event: e.event,
                detail: e.detail,
            })
            .collect();
        let snapshot = FfiMeshSnapshot {
            node_id_hex: PeerId::from_bytes(self.node_id).to_string(),
            transports,
            neighbors,
            messages: FfiMessageMetrics {
                sent: m.sent,
                delivered: m.delivered,
                relayed: m.relayed,
                dropped_duplicates: m.dropped_duplicates,
                expired: m.expired,
                delivery_failed: m.delivery_failed,
            },
            counters,
            recent_events,
        };
        tracing::info!(
            event = "iris.diag",
            node = %snapshot.node_id_hex,
            transports = ?snapshot.transports,
            neighbors = ?snapshot.neighbors,
            messages = ?snapshot.messages,
            "mesh snapshot",
        );
        // HV-6: a second, KPI-shaped line — just the non-zero counters, one
        // field each — so a bench `adb logcat | grep iris.kpi` is a time series.
        let kpi: Vec<String> = snapshot
            .counters
            .iter()
            .filter(|c| c.value > 0)
            .map(|c| format!("{}={}", c.name, c.value))
            .collect();
        tracing::info!(event = "iris.kpi", node = %snapshot.node_id_hex, counters = %kpi.join(" "), "mesh kpi");
        snapshot
    }

    /// Bring the three mesh transports up: each `start_advertising` triggers
    /// its adapter bring-up (BLE scan+advertise, Wi-Fi Aware attach+subscribe+
    /// publish, Wi-Fi Direct attach+DNS-SD) and spawns the inbound poller.
    pub fn start_all(&self) -> Result<(), IrisFfiError> {
        let handle = self.handle.clone();
        let manager = self.manager.clone();
        let node_id = self.node_id;
        let engine = self.engine.clone();
        let discovery = self.discovery.clone();
        handle.block_on(async move {
            // HV-29: `stop_all` (the app's RETRY) calls `engine.shutdown()`,
            // which aborts the delivery / ack / gc loops. Bring them back before
            // the radios, or a RETRY leaves the UI RUNNING with a dead engine.
            engine.restart().await;
            // and kick discovery so a reconnect scan runs now, not after the
            // current `scan_interval` sleep.
            discovery.wake();
            // Was `?`-propagated per transport in a loop: the first failure
            // aborted start_all() entirely, even if it was the LAST transport
            // tried and the other two had already succeeded. Confirmed on a
            // real device: BLE started fine, then wifi-aware-0 failed with
            // "not supported on this device" (no NAN hardware - a normal,
            // expected condition on many devices, not a bug) and the whole
            // mesh refused to come up, silently discarding the working BLE
            // link. Try every transport independently; succeed if at least
            // one did, so a device missing one radio still gets a mesh.
            let mut failures = Vec::new();
            let mut started = Vec::new();
            for id in ["ble-android", "wifi-aware-0", "wifi-direct-0"] {
                let result = async {
                    let t = manager.get(&TransportId::from(id)).await.ok_or_else(|| {
                        IrisFfiError::Transport(format!("transport {id} missing"))
                    })?;
                    t.start_advertising(NodeAdvertisement {
                        peer_id: PeerId::from_bytes(node_id),
                        public_ip_addr: None,
                        hostname: None,
                        tags: vec!["android".into()],
                    })
                    .await
                    .map_err(|e| IrisFfiError::Transport(format!("{id}.start_advertising: {e}")))
                }
                .await;
                match result {
                    Ok(()) => started.push(id),
                    Err(e) => {
                        tracing::warn!(
                            event = "engine.start_transport_failed",
                            transport = id,
                            error = %e,
                            "transport failed to start; trying the others"
                        );
                        failures.push(format!("{id}: {e}"));
                    }
                }
            }
            if started.is_empty() {
                return Err(IrisFfiError::Transport(format!(
                    "no transport started: {}",
                    failures.join("; ")
                )));
            }
            if !failures.is_empty() {
                tracing::warn!(
                    event = "engine.start_all_partial",
                    started = ?started,
                    failed = ?failures,
                    "mesh started with a subset of transports"
                );
            }
            Ok(())
        })
    }

    /// Send a text message to `recipient_hex` (64-hex node id). Returns the
    /// 16-byte wire message id on acceptance.
    pub fn send_text(
        &self,
        recipient_hex: &str,
        text: &str,
        priority: u8,
    ) -> Result<Vec<u8>, IrisFfiError> {
        if text.is_empty() {
            return Err(IrisFfiError::InvalidArgument(
                "message text must not be empty".into(),
            ));
        }
        let recipient = parse_peer_id_hex(recipient_hex)?;
        let priority = MessagePriority::from_u8(priority).ok_or_else(|| {
            IrisFfiError::InvalidArgument(format!("priority must be 0-7, got {priority}"))
        })?;
        let envelope = build_text_envelope(self.node_id, Some(recipient), text, priority)?;
        let message_id = envelope.message_id;
        let engine = self.engine.clone();
        self.handle
            .block_on(async move { engine.send_message(envelope).await })
            .map_err(|e| IrisFfiError::Transport(e.to_string()))?;
        Ok(message_id.to_bytes().to_vec())
    }

    /// HV-41: send to every peer in range rather than one addressed recipient.
    /// The core (`deliver_or_relay`/`is_broadcast`) already treats an
    /// empty `recipient_id` as "deliver locally AND relay to everyone" — this
    /// was simply never reachable from the Android shell, which always built
    /// an envelope with a concrete 32-byte recipient. Returns the 16-byte wire
    /// message id on acceptance, exactly like `send_text`.
    pub fn broadcast_text(&self, text: &str, priority: u8) -> Result<Vec<u8>, IrisFfiError> {
        let priority = MessagePriority::from_u8(priority).ok_or_else(|| {
            IrisFfiError::InvalidArgument(format!("priority must be 0-7, got {priority}"))
        })?;
        let envelope = build_text_envelope(self.node_id, None, text, priority)?;
        let message_id = envelope.message_id;
        let engine = self.engine.clone();
        self.handle
            .block_on(async move { engine.send_message(envelope).await })
            .map_err(|e| IrisFfiError::Transport(e.to_string()))?;
        Ok(message_id.to_bytes().to_vec())
    }

    /// Stream delivered messages to a Kotlin `FfiInboxListener`. The listener
    /// is invoked from a task on the engine's runtime.
    pub fn subscribe_inbox(&self, listener: Arc<dyn FfiInboxListener>) -> Result<(), IrisFfiError> {
        let mut rx = self.engine.delivered_messages();
        let listener = listener.clone();
        self.runtime.spawn(async move {
            // GAP-4: `Lagged` is recoverable (the consumer merely fell behind
            // the 1024-slot channel) — it is not `Closed`. Treating it as
            // Closed silently and permanently kills the user's inbox: no
            // error is surfaced, `subscribe_inbox` already returned `Ok(())`,
            // and delivered messages keep being decrypted and dropped for
            // the rest of the process. Skip the count and keep draining.
            loop {
                match rx.recv().await {
                    Ok(env) => {
                        // GAP-9: `received_at_ms` must be the local receipt
                        // time, not the sender's self-declared origination
                        // time — that field is peer-controlled and, used as
                        // a sort key, lets a peer pin or bury its own
                        // messages in every recipient's inbox.
                        let view = FfiIncomingMessage {
                            sender_id: env.sender_id,
                            message_id: env.message_id.to_bytes().to_vec(),
                            recipient_id: env.recipient_id,
                            payload: env.payload,
                            priority: env.priority.as_u8(),
                            received_at_ms: unix_now().saturating_mul(1000),
                        };
                        listener.on_message(view);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            event = "inbox.lagged",
                            skipped,
                            "inbox listener fell behind; delivered messages were dropped"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(())
    }

    /// Shut down the message engine and all transports (async teardown
    /// through the handle).
    pub fn stop_all(&self) -> Result<(), IrisFfiError> {
        let handle = self.handle.clone();
        let manager = self.manager.clone();
        let engine = self.engine.clone();
        let ids = self.transports.clone();
        handle.block_on(async move {
            engine.shutdown().await;
            for id in ids {
                if let Some(t) = manager.get(&id).await {
                    let _ = t.shutdown().await;
                }
            }
            Ok(())
        })
    }

    /// HV-97: drop every live link on every transport WITHOUT shutting the mesh
    /// down — advertising/scanning stay up, so the discovery loop re-forms the
    /// links. A test hook for the `iris_bench` harness to deterministically
    /// exercise the reconnect path (HV-14/HV-15).
    pub fn drop_all_links(&self) -> Result<(), IrisFfiError> {
        let handle = self.handle.clone();
        let manager = self.manager.clone();
        let ids = self.transports.clone();
        let discovery = self.discovery.clone();
        handle.block_on(async move {
            for id in ids {
                if let Some(t) = manager.get(&id).await {
                    t.drop_all_links().await;
                }
            }
            // HV-97: wake discovery so the reconnect scan runs now, not after
            // the current `scan_interval` sleep (up to 30 s).
            discovery.wake();
            Ok(())
        })
    }

    /// HV-21: tell every transport this device's own Wi-Fi Direct (P2P) MAC,
    /// once the platform's `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION` broadcast
    /// has delivered it. Only the BLE transport acts on this (default no-op
    /// elsewhere, same shape as `poll_health`/`drop_all_links`) — it folds
    /// the address into its discovery beacon so Wi-Fi Direct's own
    /// `discoverPeers()` results can be matched back to an already-BLE-known
    /// IRIS peer, replacing the DNS-SD service discovery this hardware never
    /// resolves (DEC-BLE-0008). `mac_str` accepts the platform's own
    /// colon-separated form ("aa:bb:cc:dd:ee:ff") or bare 12-hex.
    pub fn set_local_wifi_direct_mac(&self, mac_str: &str) -> Result<(), IrisFfiError> {
        let mac = decode_mac_hex(mac_str).ok_or_else(|| {
            IrisFfiError::InvalidArgument("mac must be 6 bytes, hex or colon-separated".into())
        })?;
        let handle = self.handle.clone();
        let manager = self.manager.clone();
        let ids = self.transports.clone();
        handle.block_on(async move {
            for id in ids {
                if let Some(t) = manager.get(&id).await {
                    t.set_local_wifi_direct_mac(mac).await;
                }
            }
            Ok(())
        })
    }
}

impl IrisEngine {
    /// Spawn a task forwarding one transport's inbound frames into the engine.
    fn spawn_inbox_forwarder(engine: Arc<MessageEngine>, transport: Arc<dyn Transport>) {
        let mut incoming = transport.incoming_messages();
        tokio::spawn(async move {
            while let Some(msg) = incoming.next().await {
                match engine.process_incoming(msg).await {
                    Ok(InboundOutcome::Delivered) | Ok(InboundOutcome::ReassembledDelivered) => {
                        tracing::debug!("android: inbound delivered");
                    }
                    Ok(_) => {}
                    Err(e) => tracing::debug!("android: inbound rejected: {e}"),
                }
            }
        });
    }
}

/// Parse a 64-hex node id into a `PeerId`.
/// Decode exactly 64 hex chars into 32 bytes, or `None`.
fn decode_hex32(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let mut b = [0u8; 32];
    for (i, pair) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        b[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(b)
}

/// Decode a Wi-Fi Direct MAC, accepting either colon-separated
/// ("aa:bb:cc:dd:ee:ff") or bare 12-hex form.
fn decode_mac_hex(s: &str) -> Option<[u8; 6]> {
    let stripped: String = s.chars().filter(|c| *c != ':').collect();
    if stripped.len() != 12 {
        return None;
    }
    let mut b = [0u8; 6];
    for (i, pair) in stripped.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        b[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(b)
}

fn parse_peer_id_hex(hex: &str) -> Result<PeerId, IrisFfiError> {
    if hex.len() != 64 {
        return Err(IrisFfiError::InvalidArgument(
            "peer id must be 64 hex characters".into(),
        ));
    }
    let mut b = [0u8; 32];
    for (i, pair) in hex.as_bytes().chunks(2).enumerate() {
        let s = std::str::from_utf8(pair)
            .map_err(|_| IrisFfiError::InvalidArgument("peer id is not valid hex".into()))?;
        b[i] = u8::from_str_radix(s, 16)
            .map_err(|_| IrisFfiError::InvalidArgument("peer id is not valid hex".into()))?;
    }
    Ok(PeerId::from_bytes(b))
}

/// Build a `Text` envelope from shell input (mirrors DesktopEngine).
/// `recipient = None` builds a broadcast envelope (empty `recipient_id`) —
/// the core's `is_broadcast`/`recipient_matches`/`recipient_peer` all treat an
/// empty `recipient_id` as "everyone" (message_engine/mod.rs), so this is the
/// only change needed here; encryption skip and local-delivery-plus-relay are
/// already handled inside `MessageEngine::send_message`.
fn build_text_envelope(
    node_id: [u8; 32],
    recipient: Option<PeerId>,
    text: &str,
    priority: MessagePriority,
) -> Result<Envelope, IrisFfiError> {
    if text.len() > 60_000 {
        return Err(IrisFfiError::InvalidArgument(
            "text too large for a single Text envelope (64 KB max)".into(),
        ));
    }
    let mut env = Envelope {
        version: PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: node_id.to_vec(),
        recipient_id: recipient.map(|p| p.as_bytes().to_vec()).unwrap_or_default(),
        priority,
        ttl_seconds: 3600,
        timestamp: unix_now(),
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

/// HV-1: test-only constructor over the inert `DevCryptoProvider`. Keeps the
/// engine's FFI/transport/mesh wiring tests (`round_trip_delivers_over_shared_mesh`
/// et al.) building and running without a Kotlin `FfiCryptoSigner` — those tests
/// exercise the bridge + transport + `SimMeshCoordinator` path, not the crypto
/// backend. Kept out of the `#[uniffi::export]` block so it is never part of the
/// generated FFI surface.
#[cfg(test)]
impl IrisEngine {
    fn new_for_test(
        ble: Arc<dyn FfiBleAdapter>,
        aware: Arc<dyn FfiWifiAwareAdapter>,
        direct: Arc<dyn FfiWifiDirectAdapter>,
        node_id: Vec<u8>,
    ) -> Result<Arc<Self>, IrisFfiError> {
        let node_id: [u8; 32] = node_id
            .try_into()
            .map_err(|_| IrisFfiError::InvalidArgument("node_id must be 32 bytes".into()))?;
        Self::build(
            ble,
            aware,
            direct,
            node_id,
            Arc::new(iris_core::message_engine::crypto::DevCryptoProvider::new()),
            None,
        )
    }

    /// HV-1: register a recipient X25519 key so `seal_outbound` does not fail
    /// closed with `KeyUnavailable` (PRY-33 / RED-0002). `DevCryptoProvider`'s
    /// encrypt/decrypt are pass-through, so the key value is irrelevant to the
    /// round-trip — only its presence in the directory matters.
    fn register_test_key(&self, node_id: [u8; 32], x25519_pubkey: [u8; 32]) {
        let mut dir = iris_core::crypto::key_directory::MemoryKeyDirectory::new();
        dir.insert(node_id, x25519_pubkey);
        self.engine.set_key_directory(Arc::new(dir));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    use async_trait::async_trait;

    use iris_core::message::{DiscoveryConfig, PeerInfo};
    use iris_core::transport::wifiaware::{
        NdpHandle, PeerHandle, PublishConfig, SimMeshCoordinator, SimulatedWifiAwareAdapter,
        WifiAwareAdapter,
    };
    use iris_core::transport::TransportState;

    use crate::ffi::ble_adapter::tests::SimBle;
    use crate::ffi::wifi_aware_adapter::{
        tests::SimAware, FfiIncomingNdpData, FfiPeerDiscovery, FfiPublishConfig,
        FfiWifiAwareAdapter,
    };
    use crate::ffi::wifi_direct_adapter::tests::SimDirect;
    use crate::ffi::IrisFfiError;

    use super::*;

    fn hex_node(b: [u8; 32]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// FFI-trait test sim that delegates to the iris-core `SimulatedWifiAwareAdapter`
    /// over a shared `SimMeshCoordinator` — a real in-memory NDP mesh. This is
    /// the Rust twin of the Kotlin `AndroidWifiAwareTransportAdapter`; the
    /// round-trip test drives two engines through it.
    struct SimAwareWire {
        inner: Arc<SimulatedWifiAwareAdapter>,
    }

    #[async_trait]
    impl FfiWifiAwareAdapter for SimAwareWire {
        async fn start(&self) -> Result<(), IrisFfiError> {
            self.inner.start().await.map_err(IrisFfiError::Transport)
        }
        async fn subscribe(&self) -> Result<(), IrisFfiError> {
            self.inner
                .subscribe()
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn unsubscribe(&self) -> Result<(), IrisFfiError> {
            self.inner
                .unsubscribe()
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn publish(&self, c: FfiPublishConfig) -> Result<(), IrisFfiError> {
            self.inner
                .publish(&PublishConfig {
                    service_name: c.service_name,
                    instance_id: c.instance_id,
                    cached: c.cached,
                    ttl_s: c.ttl_s,
                    service_specific_info: c.service_specific_info,
                })
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn unpublish(&self) -> Result<(), IrisFfiError> {
            self.inner
                .unpublish()
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn matches(&self) -> Result<Vec<FfiPeerDiscovery>, IrisFfiError> {
            Ok(self
                .inner
                .matches()
                .await
                .map_err(IrisFfiError::Transport)?
                .into_iter()
                .map(|m| FfiPeerDiscovery {
                    peer_handle: m.peer_handle.0,
                    service_specific_info: m.service_specific_info,
                })
                .collect())
        }
        async fn open_ndp(&self, peer_handle: u64) -> Result<u64, IrisFfiError> {
            self.inner
                .open_ndp(PeerHandle(peer_handle))
                .await
                .map(|h| h.0)
                .map_err(IrisFfiError::Transport)
        }
        async fn close_ndp(&self, ndp_handle: u64) -> Result<(), IrisFfiError> {
            self.inner
                .close_ndp(NdpHandle(ndp_handle))
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn ndp_send(&self, ndp_handle: u64, payload: Vec<u8>) -> Result<(), IrisFfiError> {
            self.inner
                .ndp_send(NdpHandle(ndp_handle), &payload)
                .await
                .map_err(IrisFfiError::Transport)
        }
        async fn incoming_ndp(&self) -> Result<Vec<FfiIncomingNdpData>, IrisFfiError> {
            Ok(self
                .inner
                .incoming_ndp()
                .await
                .map_err(IrisFfiError::Transport)?
                .into_iter()
                .map(|d| FfiIncomingNdpData {
                    sender: d.sender.map(|s| s.as_bytes().to_vec()),
                    ndp_handle: d.ndp.0,
                    payload: d.payload,
                })
                .collect())
        }
        async fn shutdown(&self) -> Result<(), IrisFfiError> {
            self.inner.shutdown().await.map_err(IrisFfiError::Transport)
        }
        fn is_available(&self) -> bool {
            self.inner.is_available()
        }
    }

    /// Recording inbox listener — the Kotlin `FfiInboxListener` twin.
    struct RecordingListener(Arc<Mutex<Vec<FfiIncomingMessage>>>);

    impl FfiInboxListener for RecordingListener {
        fn on_message(&self, message: FfiIncomingMessage) {
            self.0.lock().unwrap().push(message);
        }
    }

    /// Tier-5: the engine wires the three radio transports plus the initially
    /// unavailable IP transport before it returns.
    #[test]
    fn engine_registers_radio_and_internet_transports() {
        let engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAware::default()),
            Arc::new(SimDirect),
            vec![7u8; 32],
        )
        .expect("engine builds");
        assert_eq!(engine.node_id(), vec![7u8; 32]);
        let ids = engine
            .handle
            .block_on(async { engine.manager.list().await });
        let ids: Vec<String> = ids.iter().map(|id| id.as_str().to_string()).collect();
        assert!(ids.contains(&"ble-android".to_string()));
        assert!(ids.contains(&"wifi-aware-0".to_string()));
        assert!(ids.contains(&"wifi-direct-0".to_string()));
        assert!(ids.contains(&"internet-0".to_string()));
    }

    #[test]
    fn internet_transport_requires_valid_endpoint_and_network_signal() {
        let engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAware::default()),
            Arc::new(SimDirect),
            vec![9u8; 32],
        )
        .expect("engine builds");
        engine.set_internet_network_available(true);
        assert_eq!(engine.internet.state(), TransportState::Unavailable);
        engine
            .set_internet_relay_endpoints(vec!["127.0.0.1:9000".into()])
            .expect("endpoint accepted");
        assert_eq!(engine.internet.state(), TransportState::Available);
        assert!(engine
            .set_internet_relay_endpoints(vec!["not-an-endpoint".into()])
            .is_err());
    }

    /// AC-4: `start_all` drives the transports through the bridges; the
    /// Wi-Fi Aware transport must reach `Available` (bridge → FFI sim → state).
    #[test]
    fn start_all_brings_transports_up_over_bridges() {
        let aware = Arc::new(SimAware {
            available: true,
            ..Default::default()
        });
        let engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            aware.clone(),
            Arc::new(SimDirect),
            vec![7u8; 32],
        )
        .expect("engine builds");

        engine.start_all().expect("start_all drives the bridges");
        assert!(aware.subscribed(), "WA subscribe reached the adapter");
        assert!(aware.published(), "WA publish reached the adapter");

        let state = engine.handle.block_on(async {
            engine
                .manager
                .get(&TransportId::from("wifi-aware-0"))
                .await
                .expect("WA transport registered")
                .state()
        });
        assert!(
            state >= TransportState::Available,
            "WA transport must be Available after start_all, got {state:?}"
        );
    }

    /// AC-4 round trip: two engines share one in-memory mesh (through the FFI
    /// bridge + Wi-Fi Aware transport + iris-core `SimMeshCoordinator`). A
    /// text message A sends to B's real node id is delivered to B's engine and
    /// surfaced to the `FfiInboxListener`.
    #[test]
    fn round_trip_delivers_over_shared_mesh() {
        let coordinator = Arc::new(SimMeshCoordinator::new());
        let a_inner = Arc::new(SimulatedWifiAwareAdapter::new(coordinator.clone()));
        let b_inner = Arc::new(SimulatedWifiAwareAdapter::new(coordinator.clone()));
        let node_a = [1u8; 32];
        let node_b = [2u8; 32];

        let a_engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAwareWire {
                inner: a_inner.clone(),
            }),
            Arc::new(SimDirect),
            node_a.to_vec(),
        )
        .expect("engine A builds");
        let b_engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAwareWire {
                inner: b_inner.clone(),
            }),
            Arc::new(SimDirect),
            node_b.to_vec(),
        )
        .expect("engine B builds");

        // B advertises (publishes its beacon, spawning B's inbound poller).
        b_engine.handle.block_on(async {
            let b_wa = b_engine
                .manager
                .get(&TransportId::from("wifi-aware-0"))
                .await
                .expect("B WA transport");
            b_wa.start_advertising(NodeAdvertisement {
                peer_id: PeerId::from_bytes(node_b),
                public_ip_addr: None,
                hostname: None,
                tags: vec!["android".into()],
            })
            .await
            .expect("B advertises");
        });

        // A discovers B, then connects keyed by B's REAL node id (the app's
        // candidate→verified reconcile step); the transport address carries
        // B's opaque mesh handle.
        a_engine.handle.block_on(async {
            let a_wa = a_engine
                .manager
                .get(&TransportId::from("wifi-aware-0"))
                .await
                .expect("A WA transport");
            let mut stream = a_wa
                .discover_peers(DiscoveryConfig::default())
                .await
                .expect("A discovers");
            let first = stream.next().await.expect("B is discoverable");
            let tag = first
                .transport_addresses
                .iter()
                .find(|(k, _)| k == "wifi-aware")
                .expect("B advertises a wifi-aware address")
                .1
                .clone();
            let target = PeerInfo {
                peer_id: PeerId::from_bytes(node_b),
                addresses: Vec::new(),
                transport_addresses: vec![("wifi-aware".to_string(), tag)],
                last_seen: None,
            };
            a_wa.connect(&target).await.expect("A connects to B");
        });

        // HV-1 / PRY-33: A must know B's recipient key to seal the message.
        a_engine.register_test_key(node_b, [0xBB; 32]);

        // B installs the inbox listener before the message arrives.
        let inbox = Arc::new(Mutex::new(Vec::new()));
        b_engine
            .subscribe_inbox(Arc::new(RecordingListener(inbox.clone())))
            .expect("subscribe_inbox installs listener");

        // A sends text addressed to B's real node id.
        let msg_id = a_engine
            .send_text(&hex_node(node_b), "hello", 4)
            .expect("send_text accepted");
        assert_eq!(msg_id.len(), 16, "send_text returns the 16-byte wire id");

        // B delivers on its own runtime: poll both the delivered stream and
        // the inbox listener until the payload arrives (B's poller runs at the
        // 500 ms idle cadence — B has no open links).
        let mut rx = b_engine.engine.delivered_messages();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut delivered = None;
        while Instant::now() < deadline {
            if let Ok(env) = rx.try_recv() {
                delivered = Some(env);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let env = delivered.expect("B must deliver within timeout");
        assert_eq!(env.payload, b"hello".to_vec(), "payload is the text");
        assert_eq!(env.recipient_id, node_b.to_vec(), "addressed to B");

        let got = inbox
            .lock()
            .unwrap()
            .iter()
            .find(|m| m.payload == b"hello".to_vec())
            .cloned();
        assert!(
            got.is_some(),
            "FfiInboxListener must receive the delivered message"
        );
    }

    /// HV-3: `snapshot()` reports every registered transport, the node id, and
    /// the message counters — the contract the `/diag` shell command and the
    /// HV-2 Mobly harness read.
    #[test]
    fn snapshot_reports_transports_and_identity() {
        let engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAware::default()),
            Arc::new(SimDirect),
            vec![7u8; 32],
        )
        .expect("engine builds");

        let snap = engine.snapshot();
        assert_eq!(snap.node_id_hex, "07".repeat(32));
        let ids: Vec<&str> = snap.transports.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"ble-android"), "got {ids:?}");
        assert!(ids.contains(&"wifi-aware-0"), "got {ids:?}");
        assert!(ids.contains(&"wifi-direct-0"), "got {ids:?}");
        // Fresh engine: no neighbours, no traffic.
        assert!(snap.neighbors.is_empty());
        assert_eq!(snap.messages.sent, 0);
        assert_eq!(snap.messages.delivered, 0);
    }

    /// The engine's handle is a *real* tokio handle, not the uniffi attribute.
    #[test]
    fn engine_owns_a_live_runtime() {
        let engine = IrisEngine::new_for_test(
            Arc::new(SimBle),
            Arc::new(SimAware::default()),
            Arc::new(SimDirect),
            vec![9u8; 32],
        )
        .expect("engine builds");
        // A full-fidelity async op (sleep + join) must complete on the engine's
        // runtime — proves the future is driven, not merely ffi-deposited.
        engine.handle.block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        });
    }
}
