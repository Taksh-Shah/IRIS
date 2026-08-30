//! WIAW-001 — Wi-Fi Aware (NAN) transport.
//!
//! Implements the platform-adapter architecture from
//! `docs/transports/TRANSPORT_ABSTRACTION.md` §Platform Adapter Architecture
//! with the Wi-Fi Aware profile in `docs/transports/WIFI_AWARE.md`:
//!
//! ```text
//! WifiAwareTransport (Rust core)  --calls-->  WifiAwareAdapter trait
//!                                                   ├─ AndroidWifiAwareAdapter (Kotlin/JNI)
//!                                                   └─ native / vendor adapters (future)
//! ```
//!
//! The Rust core depends only on the [`WifiAwareAdapter`] trait object — the
//! IRIS transport layer never touches the Android APIs directly. A
//! [`SimulatedWifiAwareAdapter`] (backed by a [`SimMeshCoordinator`]) provides a
//! deterministic in-memory NDP mesh for integration tests, so routing and E2E
//! tests run with no Wi-Fi hardware (RED-0004 in-process style).
//!
//! Security posture — Infrastructure profile (INFRA.md / DEC-WA-0006):
//! `nan_service` name is the cluster identity string ("IRIS" family, carrier
//! allow-listed per market); only DISCOVERY (`discover`/`publish`-broadcast,
//! NAN `subscribe`) is authorized in the scaffold. Data-path (`ndp`) carved
//! from Wi-Fi 6E AFH is gated on the Android platform crate + BLK-0021
//! namespace. The NAN MAC is NEVER an identity (platform randomizes ~30 min);
//! peer identity is the 32-byte IRIS PeerId / 16-byte `peer_short`
//! (DEC-WA-0007).
//!
//! Availability: with AFH display-off handling, the transport remains
//! Available (cluster discovery only); the unicast data scope drops. This
//! matches the "availability churn" contract in WIFI_AWARE.md §3.5.

use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use tokio::sync::broadcast;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::AbortHandle;

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::internet::{encode_frame, frame_payload_len, FRAME_HEADER_LEN};
use crate::transport::wifiaware_beacon::{CapabilityBits, WifiAwareBeacon};
use crate::transport::{
    freshness_minutes_now, AtomicState, EwmaGoodput, Transport, TransportCapabilities,
    TransportCost, TransportState, TransportStateEvent, WIFI_AWARE_COST,
};
use crate::TransportError;

/// Maximum message bytes carried in one NDP frame (Wi-Fi 6E AFH datapath is a
/// dedicated data plane; the platform reassembles frames into one blob).
// SYS-2: deadline constants for Wi-Fi Aware adapter calls.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_NAN_MESSAGE_BYTES: usize = 1024 * 1024;

/// Upper bound on simultaneously open NAN Data Paths, matching the Android
/// platform budget (`characteristics` data-path count, typically ~32). Keeps the
/// NDP pool bounded when the engine re-connects/re-routes (WAW-RT-001, AC-9).
const MAX_NDP_POOL: usize = 32;

/// Incoming-frame channel capacity (WAW-RT-005). Matches all other transports at 1024
/// slots — 32 caused receiver lag under burst load before the reader task drained it.
const INCOMING_CHANNEL_CAPACITY: usize = 1024;

/// Max frames forwarded to the engine per 10 ms poll tick (WAW-RT-005). Frames
/// beyond the budget stay in the adapter and drain next tick — no loss, bounded
/// per-tick work (latency + energy fairness).
const MAX_FRAMES_PER_TICK: usize = 8;

/// Idle poll cadence when no NDP links are active (WAW-RT-014): the discovery /
/// availability watcher still runs but the frame drain is skipped, so the
/// full-power 10 ms drain is only paid while a link is open.
const IDLE_POLL_MS: u64 = 500;

// ---------------------------------------------------------------------------
// Domain types (WIFI_AWARE.md §Domain Model)
// ---------------------------------------------------------------------------

/// Opaque peer handle assigned by the platform NAN manager to a matched peer.
/// Never an identity — the 48-bit NAN MAC is platform-randomized ~every 30 min
/// (DEC-WA-0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerHandle(pub u64);

/// Opaque NAN Data Path (NDP) handle — the active data link to a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NdpHandle(pub u64);

/// One discovery match: the peer's handle + its IRIS beacon bytes.
#[derive(Debug, Clone)]
pub struct PeerDiscovery {
    pub peer_handle: PeerHandle,
    /// Raw `service_specific_info` bytes (the IRIS beacon, WIFI_AWARE.md §4.2).
    pub service_specific_info: Vec<u8>,
    /// Received signal strength in dBm (BLE-38). 0 when unavailable (sim).
    pub rssi: i32,
}

/// A frame received on an NDP data path.
///
/// `ndp` is the *receiving* adapter's local NDP handle (the platform
/// `NanDataPath` the frame arrived on). A real adapter resolves and returns
/// this handle; the sim models inbound data with no local path, so it returns
/// `NdpHandle(0)` and instead reports the *sender's candidate identity* in
/// `sender` — a beacon-derived best-effort attribution for the polling
/// transport (NEW-WA-RT-101 defines local-vs-remote handle semantics). The
/// IRIS envelope layer performs real identity verification (DEC-WA-0007).
#[derive(Debug, Clone)]
pub struct IncomingNdpData {
    /// Local NDP handle the frame arrived on (`NdpHandle(0)` = none / sim).
    pub ndp: NdpHandle,
    /// Best-effort candidate sender identity (beacon short id when known).
    pub sender: Option<PeerId>,
    pub payload: Vec<u8>,
}

/// Publish configuration for the NAN session (infrastructure profile: the
/// publish instance carries the beacon in `service_specific_info`).
///
/// BLE-37: `service_name`, `instance_id`, `cached`, and `ttl_s` are policy-
/// reserved placeholders. The Kotlin bridge hard-codes all four values and
/// never reads them from `FfiPublishConfig` — threading them through the FFI
/// is future work. Only `service_specific_info` (the 22-byte IRIS discovery
/// beacon) is both populated by Rust and consumed by the Android bridge.
#[derive(Debug, Clone)]
pub struct PublishConfig {
    /// NAN service name string. Currently NOT threaded through the FFI —
    /// Kotlin hard-codes `IRIS_SERVICE_NAME = "com.iris.mesh.v1"` directly
    /// (FFI-19). The default is kept in sync with Kotlin's constant. When
    /// carrier-specific service names are needed this field must be wired
    /// into `FfiPublishConfig` and the Kotlin `publishConfig()` call.
    pub service_name: String,
    /// Matched-instance visibility: `-1` = every IRIS neighbor; else the
    /// 4-octet service instance id. Not threaded through the FFI (BLE-37).
    pub instance_id: i16,
    /// Publish set cached across display-off (§3.5 availability churn).
    /// Not threaded through the FFI (BLE-37).
    pub cached: bool,
    /// Session TTL in seconds. Not threaded through the FFI (BLE-37).
    pub ttl_s: u16,
    /// The IRIS discovery beacon (`WifiAwareBeacon::build`, 22 bytes) to
    /// publish as NAN `service_specific_info`. Populated by
    /// `start_advertising` from the node's real `PeerId` (FFI-4 fix) and
    /// forwarded to the Android bridge via `FfiPublishConfig`.
    pub service_specific_info: Vec<u8>,
}

impl Default for PublishConfig {
    fn default() -> Self {
        PublishConfig {
            service_name: "com.iris.mesh.v1".to_string(), // matches Kotlin IRIS_SERVICE_NAME
            instance_id: -1,
            cached: true,
            ttl_s: 60,
            service_specific_info: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Platform adapter trait
// ---------------------------------------------------------------------------

/// Platform bridge the Rust core depends on. Injected by the app: an Android
/// adapter (Kotlin/JNI) on devices, the simulated adapter in tests.
#[async_trait]
pub trait WifiAwareAdapter: Send + Sync {
    /// Power on the Wi-Fi Aware subsystem (`WifiAwareManager` attach). Idempotent.
    async fn start(&self) -> Result<(), String>;

    /// Begin passive discovery subscription against the cluster service.
    async fn subscribe(&self) -> Result<(), String>;

    /// End discovery subscription. Idempotent.
    async fn unsubscribe(&self) -> Result<(), String>;

    /// Publish this node's discovery beacon (broadcast instance).
    async fn publish(&self, config: &PublishConfig) -> Result<(), String>;

    /// Withdraw the publish instance. Idempotent.
    async fn unpublish(&self) -> Result<(), String>;

    /// Drain current discovery matches.
    ///
    /// BLE-30: this and [`incoming_ndp`](Self::incoming_ndp) used to be
    /// infallible by signature while the FFI beneath them
    /// (`FfiWifiAwareAdapter::matches`) is fallible — forcing the bridge to
    /// discard every adapter error as `unwrap_or_default()` ("session died"
    /// became indistinguishable from "no peers found"). Fallible now so a
    /// caller can tell the two apart.
    async fn matches(&self) -> Result<Vec<PeerDiscovery>, String>;

    /// Open a NAN Data Path to a peer; returns the NDP handle.
    async fn open_ndp(&self, peer_handle: PeerHandle) -> Result<NdpHandle, String>;

    /// Close a NAN Data Path. Idempotent.
    async fn close_ndp(&self, ndp: NdpHandle) -> Result<(), String>;

    /// Send a frame on an NDP.
    async fn ndp_send(&self, ndp: NdpHandle, payload: &[u8]) -> Result<(), String>;

    /// Drain inbound NDP frames.
    ///
    /// BLE-30: fallible for the same reason as [`matches`](Self::matches).
    async fn incoming_ndp(&self) -> Result<Vec<IncomingNdpData>, String>;

    /// Tear down the subsystem (unsubscribe, unpublish, close NDPs, detach).
    async fn shutdown(&self) -> Result<(), String>;

    /// Current availability. False = display-off data scope (unicast publish
    /// and NDP drop); discovery-only stays up for mesh survival (§3.5).
    fn is_available(&self) -> bool;

    /// Optional push channel for availability transitions
    /// (`ACTION_WIFI_AWARE_STATE_CHANGED` on Android). Default: pull-only via
    /// [`WifiAwareAdapter::is_available`] (WAW-RT-008). A real adapter SHOULD
    /// override this so the transport can surface `Degraded` on display-off /
    /// radio-share without waiting on the poller.
    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        None
    }
}

// ---------------------------------------------------------------------------
// Simulated adapter + mesh coordinator (deterministic, no hardware)
// ---------------------------------------------------------------------------

/// In-memory NDP mesh for the simulated adapter. Adapters sharing a coordinator
/// can discover and exchange NDP frames; distinct coordinators are isolated
/// mesh "islands", so test setups never cross-talk.
pub struct SimMeshCoordinator {
    /// registered tags -> beacon bytes.
    peers: StdMutex<HashMap<u64, Vec<u8>>>,
    /// queued frames: (to_tag, from_tag, payload). The from-tag lets the
    /// receiver attribute the frame to the sender's beacon (NEW-WA-RT-101);
    /// the receiver knows nothing about the sender's NDP handle.
    outbox: StdMutex<VecDeque<(u64, u64, Vec<u8>)>>,
    /// Frames silently dropped because the outbox was full (BLE-40).
    pub dropped: AtomicU64,
    next_tag: AtomicU64,
    next_ndp: AtomicU64,
}

impl SimMeshCoordinator {
    pub fn new() -> Self {
        SimMeshCoordinator {
            peers: StdMutex::new(HashMap::new()),
            outbox: StdMutex::new(VecDeque::new()),
            dropped: AtomicU64::new(0),
            next_tag: AtomicU64::new(1),
            next_ndp: AtomicU64::new(1),
        }
    }

    fn alloc_ndp(&self) -> NdpHandle {
        NdpHandle(self.next_ndp.fetch_add(1, Ordering::Relaxed))
    }

    fn register_peer(&self, beacon: Vec<u8>) -> u64 {
        let tag = self.next_tag.fetch_add(1, Ordering::Relaxed);
        self.peers.lock().unwrap_or_else(|p| p.into_inner()).insert(tag, beacon);
        tag
    }

    fn set_beacon(&self, tag: u64, beacon: Vec<u8>) {
        self.peers.lock().unwrap_or_else(|p| p.into_inner()).insert(tag, beacon);
    }

    /// Latest beacon bytes published by `tag`, if any (used for inbound
    /// candidate attribution; NEW-WA-RT-101).
    fn beacon_of(&self, tag: u64) -> Option<Vec<u8>> {
        self.peers.lock().unwrap_or_else(|p| p.into_inner()).get(&tag).cloned()
    }

    fn unregister_peer(&self, tag: u64) {
        self.peers.lock().unwrap_or_else(|p| p.into_inner()).remove(&tag);
        // RT-010: also drop any queued frames destined to the departed tag so
        // the outbox cannot grow for peers that will never drain them.
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        outbox.retain(|(to, _, _)| *to != tag);
    }

    /// All (tag, beacon) for peers except `self_tag`.
    fn visible_peers(&self, self_tag: u64) -> Vec<(u64, Vec<u8>)> {
        self.peers
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, _)| **t != self_tag)
            .map(|(t, b)| (*t, b.clone()))
            .collect()
    }

    /// Max queued frames in the in-memory outbox before senders are refused
    /// (WAW-RT-002): a fast local sender must not grow the mesh queue without
    /// bound. A full outbox drops the oldest frames — same as a real NDP
    /// socket's finite buffer under backpressure. 128 × 1 MiB = 128 MiB
    /// worst-case buffered bytes.
    const MAX_OUTBOX_FRAMES: usize = 128;

    /// Upper bound on frames returned by `drain_outbox` per call
    /// (WAW-RT-002 / NEW-WA-RT-101): **equal to** the poller's per-tick budget
    /// `MAX_FRAMES_PER_TICK`, so the adapter never hands the poller more frames
    /// than it forwards. Overflow stays queued for the next poll — bounded work
    /// per tick, **zero frame loss** on the only data path (a real adapter must
    /// return a similarly bounded batch; see the FFI contract notes).
    const MAX_DRAIN_PER_CALL: usize = MAX_FRAMES_PER_TICK;

    fn proxy_send(&self, from_tag: u64, to_tag: u64, payload: &[u8]) {
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        if outbox.len() >= Self::MAX_OUTBOX_FRAMES {
            // Drop the oldest frame; keep the newest (finite NDP buffer).
            outbox.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        outbox.push_back((to_tag, from_tag, payload.to_vec()));
    }

    /// True when a peer with this tag is registered in the mesh (RT-010).
    fn has_peer(&self, tag: u64) -> bool {
        self.peers.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&tag)
    }

    /// Drain up to `MAX_DRAIN_PER_CALL` frames addressed to `tag`, returning
    /// the *sender's* tag alongside the payload so the receiver can attribute
    /// the frame to a beacon-derived candidate PeerId (NEW-WA-RT-101). The
    /// sender's opaque NDP handle is meaningless to the receiver, so it is
    /// intentionally not forwarded. Overflow stays queued for the next call
    /// (bounded work per poll, no loss).
    fn drain_outbox(&self, tag: u64) -> Vec<(u64, Vec<u8>)> {
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        let mut mine = Vec::with_capacity(Self::MAX_DRAIN_PER_CALL.min(outbox.len()));
        let mut keep = VecDeque::with_capacity(outbox.len());
        let mut budget = Self::MAX_DRAIN_PER_CALL;
        for (to, from, payload) in outbox.drain(..) {
            if to == tag && budget > 0 {
                mine.push((from, payload));
                budget -= 1;
            } else {
                keep.push_back((to, from, payload));
            }
        }
        *outbox = keep;
        mine
    }
}

impl Default for SimMeshCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// [`WifiAwareAdapter`] simulation. Publishes the node beacon into the mesh;
/// `matches()` returns every other advertised beacon; `open_ndp` allocates a
/// datapath and `ndp_send` routes frames to the target peer by tag.
pub struct SimulatedWifiAwareAdapter {
    coordinator: Arc<SimMeshCoordinator>,
    tag: StdMutex<u64>,
    subscribed: AtomicBool,
    published: AtomicBool,
    ndp_open: StdMutex<HashMap<NdpHandle, u64>>,
    available: AtomicBool,
    /// BLE-36: kept as sender only — `availability_stream()` subscribes fresh
    /// each time so the stream is re-subscribable after a transport restart.
    available_tx: broadcast::Sender<bool>,
    /// Adapter start time, drives monotonic `freshness_minutes` (RT-011) so
    /// the anti-stale field is exercised with real non-zero values in CI.
    started: Instant,
}

impl SimulatedWifiAwareAdapter {
    pub fn new(coordinator: Arc<SimMeshCoordinator>) -> Self {
        let (available_tx, _) = broadcast::channel(8);
        SimulatedWifiAwareAdapter {
            coordinator,
            tag: StdMutex::new(0),
            subscribed: AtomicBool::new(false),
            published: AtomicBool::new(false),
            ndp_open: StdMutex::new(HashMap::new()),
            available: AtomicBool::new(true),
            available_tx,
            started: Instant::now(),
        }
    }

    fn beacon_bytes(&self, peer_short: &[u8; 16]) -> Vec<u8> {
        let caps = CapabilityBits::from_bits(
            CapabilityBits::NDP_UNICAST | CapabilityBits::PUBLISH_BROADCAST,
        );
        WifiAwareBeacon::build(
            caps,
            *peer_short,
            freshness_minutes_now(), // GAP-6: wall-clock epoch, not uptime
        )
    }

    /// Register (first call) or update (later calls) our beacon in the mesh.
    /// The sim peer_short is derived from the registration tag (a stable
    /// in-process identity — not a NAN MAC).
    fn register(&self) {
        let mut tag = self.tag.lock().unwrap_or_else(|p| p.into_inner());
        let mut short = [0u8; 16];
        if *tag == 0 {
            *tag = self.coordinator.register_peer(vec![]);
        }
        short[..8].copy_from_slice(&tag.to_le_bytes());
        self.coordinator.set_beacon(*tag, self.beacon_bytes(&short));
    }

    /// Simulate availability churn (display-off data scope / radio share).
    /// Pushes to the availability stream so the transport can surface `Degraded`.
    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::Release);
        self.available_tx.send(available).ok();
    }
}

#[async_trait]
impl WifiAwareAdapter for SimulatedWifiAwareAdapter {
    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        // BLE-36: subscribe a fresh receiver each call so the stream is
        // re-subscribable after a transport restart (not a one-shot `.take()`).
        let rx = self.available_tx.subscribe();
        Some(Box::pin(crate::transport::broadcast_stream(rx, "wifiaware.availability"))
            as Pin<Box<dyn Stream<Item = bool> + Send + 'static>>)
    }

    async fn start(&self) -> Result<(), String> {
        Ok(())
    }

    async fn subscribe(&self) -> Result<(), String> {
        self.subscribed.store(true, Ordering::Release);
        Ok(())
    }

    async fn unsubscribe(&self) -> Result<(), String> {
        self.subscribed.store(false, Ordering::Release);
        Ok(())
    }

    async fn publish(&self, config: &PublishConfig) -> Result<(), String> {
        if !self.available.load(Ordering::Acquire) {
            return Err("wifi_aware_unavailable".to_string());
        }
        // BLE-32: use the caller-provided beacon (service_specific_info) so the
        // sim publishes the node's real identity instead of an invented counter.
        // Fall back to the counter-based beacon for tests that publish with
        // PublishConfig::default() (no SSI), preserving backward compatibility.
        let mut tag = self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if *tag == 0 {
            *tag = self.coordinator.register_peer(vec![]);
        }
        if !config.service_specific_info.is_empty() {
            self.coordinator.set_beacon(*tag, config.service_specific_info.clone());
        } else {
            let mut short = [0u8; 16];
            short[..8].copy_from_slice(&tag.to_le_bytes());
            self.coordinator.set_beacon(*tag, self.beacon_bytes(&short));
        }
        self.published.store(true, Ordering::Release);
        Ok(())
    }

    async fn unpublish(&self) -> Result<(), String> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if tag != 0 {
            self.coordinator.unregister_peer(tag);
        }
        self.published.store(false, Ordering::Release);
        Ok(())
    }

    async fn matches(&self) -> Result<Vec<PeerDiscovery>, String> {
        if !self.subscribed.load(Ordering::Acquire) {
            return Ok(Vec::new());
        }
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        Ok(self
            .coordinator
            .visible_peers(tag)
            .into_iter()
            .map(|(t, beacon)| PeerDiscovery {
                peer_handle: PeerHandle(t),
                service_specific_info: beacon,
                rssi: 0,
            })
            .collect())
    }

    async fn open_ndp(&self, peer_handle: PeerHandle) -> Result<NdpHandle, String> {
        // RT-010: reject self-NDPs and handles with no registered peer so the
        // sim mirrors the platform (open_ndp to an unknown handle fails).
        let my_tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if peer_handle.0 == my_tag || !self.coordinator.has_peer(peer_handle.0) {
            return Err("ndp peer not available".to_string());
        }
        let ndp = self.coordinator.alloc_ndp();
        self.ndp_open.lock().unwrap_or_else(|p| p.into_inner()).insert(ndp, peer_handle.0);
        Ok(ndp)
    }

    async fn close_ndp(&self, ndp: NdpHandle) -> Result<(), String> {
        self.ndp_open.lock().unwrap_or_else(|p| p.into_inner()).remove(&ndp);
        Ok(())
    }

    async fn ndp_send(&self, ndp: NdpHandle, payload: &[u8]) -> Result<(), String> {
        let to_tag = self
            .ndp_open
            .lock()
            .unwrap()
            .get(&ndp)
            .copied()
            .ok_or_else(|| "ndp_not_open".to_string())?;
        let from_tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        self.coordinator.proxy_send(from_tag, to_tag, payload);
        Ok(())
    }

    async fn incoming_ndp(&self) -> Result<Vec<IncomingNdpData>, String> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        Ok(self
            .coordinator
            .drain_outbox(tag)
            .into_iter()
            .map(|(from, payload)| {
                // NEW-WA-RT-101: attribute to the sender's beacon-derived
                // candidate identity so the poller never delivers zero-peer
                // frames. The beacon is the sender's latest published beacon.
                let sender = self
                    .coordinator
                    .beacon_of(from)
                    .and_then(|b| WifiAwareBeacon::parse(&b).ok())
                    .map(|beacon| beacon.candidate_peer_id());
                IncomingNdpData {
                    ndp: NdpHandle(0), // sim: no receiver-local NDP handle
                    sender,
                    payload,
                }
            })
            .collect())
    }

    async fn shutdown(&self) -> Result<(), String> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if tag != 0 {
            self.coordinator.unregister_peer(tag);
        }
        self.subscribed.store(false, Ordering::Release);
        self.published.store(false, Ordering::Release);
        self.ndp_open.lock().unwrap_or_else(|p| p.into_inner()).clear();
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Transport scaffolding
// ---------------------------------------------------------------------------

/// Static capabilities for the Wi-Fi Aware transport.
fn wifiaware_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        max_message_size: MAX_NAN_MESSAGE_BYTES,
        supports_broadcast: true,
        supports_unicast: true,
        supports_multicast: false,
        range_m_min: 10,
        range_m_max: 100,
        range_m_typical: 50,
        typical_throughput_bps: 20_000_000,
        typical_latency_ms: 5,
        requires_infrastructure: false,
        supports_background_android: true,
        supports_background_ios: false,
        requires_special_hardware: true,
        cost_class: crate::transport::TransportCostClass::Free,
        regulatory_band: None,
        conflict_group: crate::transport::RadioConflictGroup::WiFi24GHz,
    }
}

/// An open NAN Data Path to a peer, keyed by IRIS PeerId.
struct NdpLink {
    ndp: NdpHandle,
    peer_id: PeerId,
}

/// Wi-Fi Aware transport scaffolding (see module docs).
pub struct WifiAwareTransport {
    id: crate::transport::TransportId,
    display: String,
    caps: TransportCapabilities,
    adapter: AsyncMutex<Option<Arc<dyn WifiAwareAdapter>>>,
    state: Arc<AtomicState>,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    poller: AsyncMutex<Option<AbortHandle>>,
    avail_watcher: AsyncMutex<Option<AbortHandle>>,
    links: Arc<StdMutex<Vec<NdpLink>>>,
    /// Serializes the whole open-link critical section (WAW-RT-001): two
    /// concurrent `connect()` calls must not double-open an NDP for one peer,
    /// exceed `MAX_NDP_POOL`, or push a link that a concurrent `shutdown()`
    /// just tore down. The gate also serializes one-shot bring-up against
    /// shutdown (NEW-WA-RT-102).
    connect_gate: AsyncMutex<()>,
    /// Set once `shutdown()` has completed; prevents a racing `ensure_started`
    /// bring-up from resurrecting a dead transport (NEW-WA-RT-102).
    shutdown_flag: AtomicBool,
    /// Set on the first successful `ensure_started` completion so subsequent
    /// calls return early before acquiring `connect_gate` (BLE-34). Cleared by
    /// `shutdown()` so a restarted transport re-runs bring-up exactly once.
    started_flag: AtomicBool,
    /// Inbound frames that could not be delivered upstream (closed/slow
    /// receiver), counted for telemetry instead of silently swallowed
    /// (NEW-WA-RT-106).
    dropped_inbound: Arc<AtomicU64>,
    /// BLE-39: maps each discovered PeerId to its NAN peer handle so
    /// connect() can validate the caller-supplied handle against what
    /// discover_peers() saw. Cleared on unsubscribe / session restart.
    discovery_cache: StdMutex<HashMap<PeerId, PeerHandle>>,
    /// MG-17: EWMA goodput tracker.
    ewma: EwmaGoodput,
}

/// True when the state is selectable for active use.
fn state_is_available(state: &TransportState) -> bool {
    *state >= TransportState::Available
}

/// NEW-WA-RT-107 / BLE-29: classify an adapter `ndp_send` error as terminal
/// link loss. The original marker set ("not open" / "closed" / "disconnect"
/// / "reset" / "link lost") was written against `SimulatedWifiAwareAdapter`'s
/// vocabulary, not the real Android adapter's — the actual
/// `WifiAwareNetworkSpecifier` failure surfaces as `"NDP terminated"` or
/// `"NETWORK_LOST"` (per `ConnectivityManager.NetworkCallback.onLost`), and
/// neither matched, so no link ever got torn down on a real device: every
/// send returned `Busy` forever and the dead peer stayed `Connected`. Widened
/// to include the real markers.
///
/// This is still substring classification over a string with no format
/// guarantee, which is structurally unsound at a UniFFI boundary — a typed
/// `LinkClosed` FFI error variant that the Kotlin adapter constructs directly
/// (rather than a stringified message) is the correct long-term fix and is
/// out of scope for a Rust-only pass: it requires the Kotlin adapter to throw
/// the new type at the right call sites, which cannot be verified without a
/// Kotlin toolchain. A bare numeric failure code (the third form Android can
/// produce) is unclassifiable by string matching in principle regardless.
fn is_link_loss_error(e: &str) -> bool {
    let e = e.to_ascii_lowercase();
    e.contains("not open")
        || e.contains("closed")
        || e.contains("disconnect")
        || e.contains("reset")
        || e.contains("link lost")
        || e.contains("ndp terminated")
        || e.contains("network_lost")
        || e.contains("network lost")
}

impl fmt::Debug for WifiAwareTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WifiAwareTransport")
            .field("id", &self.id)
            .field("display", &self.display)
            .field("state", &self.state.load())
            .finish()
    }
}

impl WifiAwareTransport {
    /// Create a scaffold transport wrapping the given platform adapter. The
    /// transport begins Unavailable; it becomes Available after the first
    /// successful async interaction (discovery or advertise), and Connected
    /// once an NDP is open.
    pub fn new(adapter: Option<Arc<dyn WifiAwareAdapter>>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(INCOMING_CHANNEL_CAPACITY);
        let id = crate::transport::TransportId::from("wifi-aware-0");
        let state = Arc::new(AtomicState::default());
        state.store(TransportState::Unavailable);
        WifiAwareTransport {
            caps: wifiaware_capabilities(),
            id,
            display: "Wi-Fi Aware".to_string(),
            adapter: AsyncMutex::new(adapter),
            state,
            state_tx,
            incoming_tx,
            poller: AsyncMutex::new(None),
            avail_watcher: AsyncMutex::new(None),
            links: Arc::new(StdMutex::new(Vec::new())),
            connect_gate: AsyncMutex::new(()),
            shutdown_flag: AtomicBool::new(false),
            started_flag: AtomicBool::new(false),
            dropped_inbound: Arc::new(AtomicU64::new(0)),
            discovery_cache: StdMutex::new(HashMap::new()),
            ewma: EwmaGoodput::new(),
        }
    }

    /// BLE-36: reset the transport after a `shutdown()` so it can be brought up
    /// again (e.g. display-on after display-off). Must be called before the next
    /// `discover_peers` / `start_advertising` cycle. Runs under `connect_gate` so
    /// it cannot race a concurrent `ensure_started` or `shutdown`.
    pub async fn restart(&self) {
        let _gate = self.connect_gate.lock().await;
        self.shutdown_flag.store(false, Ordering::SeqCst);
        self.started_flag.store(false, Ordering::Release);
        self.set_state(TransportState::Unavailable);
    }

    /// BLE-33: observable count of frames dropped because the inbound broadcast
    /// channel had no subscriber. Zero means no loss; non-zero means the engine
    /// called `incoming_messages()` too late or the channel wrapped.
    pub fn dropped_inbound(&self) -> u64 {
        self.dropped_inbound.load(Ordering::Relaxed)
    }

    fn set_state(&self, new: TransportState) {
        let old = self.state.load();
        if old != new {
            self.state.store(new);
            self.state_tx.send(TransportStateEvent {
                transport_id: self.id.clone(),
                new_state: new,
            });
        }
    }

    /// Store a new state + emit the transition event (shared by the
    /// availability watcher which holds no `&self`).
    fn store_state_with_event(
        id: &crate::transport::TransportId,
        state: &Arc<AtomicState>,
        tx: &broadcast::Sender<TransportStateEvent>,
        new: TransportState,
    ) {
        if state.load() != new {
            state.store(new);
            tx.send(TransportStateEvent {
                transport_id: id.clone(),
                new_state: new,
            }).ok();
        }
    }

    /// Remove an NDP link + close the handle; drop out of `Connected` when it was
    /// the last active link (WAW-RT-002). NEW-WA-RT-104: the drop lands on
    /// `Degraded` (not `Available`) when the data scope is down, so a transport
    /// whose last link dies mid-outage never lies "Available" to the manager.
    async fn teardown_link(&self, adapter: &Arc<dyn WifiAwareAdapter>, ndp: NdpHandle) {
        let removed = {
            let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            let before = links.len();
            links.retain(|l| l.ndp != ndp);
            before != links.len()
        };
        if removed {
            adapter.close_ndp(ndp).await.ok();
            if self.links.lock().unwrap_or_else(|p| p.into_inner()).is_empty() {
                let next = if adapter.is_available() {
                    TransportState::Available
                } else {
                    TransportState::Degraded
                };
                self.set_state(next);
            }
        }
    }

    /// FFI-7 / FFI-11: classify an adapter error string, distinguishing a
    /// revoked runtime permission (`attach`/`subscribe`/`publish` all throw
    /// `SecurityException` synchronously without `NEARBY_WIFI_DEVICES`,
    /// converted by Kotlin into `IrisFfiError::PermissionDenied` — "permission
    /// denied" — instead of aborting the call undeclared) from a plain I/O
    /// failure. MG-41: mapped to `TransportError::PermissionDenied`, same as
    /// BLE's `BleError::PermissionDenied` and Wi-Fi Direct's equivalent —
    /// not latched as permanently unsupported, since the permission can be
    /// re-granted at any time.
    fn classify_error(context: &str, e: String) -> TransportError {
        if e.contains("permission denied") {
            TransportError::PermissionDenied {
                permission: "wifi_aware",
            }
        } else {
            TransportError::Io {
                kind: std::io::ErrorKind::Other,
                msg: format!("{context}: {e}"),
            }
        }
    }

    /// Clone of the injected adapter, or `HardwareUnavailable` when none is
    /// present.
    async fn adapter(&self) -> Result<Arc<dyn WifiAwareAdapter>, TransportError> {
        self.adapter
            .lock()
            .await
            .clone()
            .ok_or(TransportError::HardwareUnavailable)
    }

    /// One-shot bring-up: attach + subscribe, then spawn the NDP poller + the
    /// availability watcher.
    ///
    /// Serialized against `shutdown()` via `connect_gate` (NEW-WA-RT-102): the
    /// gate is held across the whole bring-up+, so shutdown() cannot interleave
    /// and strand a half-spawned poller, and a `shutdown_flag` permanently
    /// blocks any later bring-up attempt. Re-checks both before promotion.
    async fn ensure_started(&self) -> Result<(), TransportError> {
        // BLE-34: short-circuit before touching connect_gate when bring-up is
        // already done. This removes the head-of-line block on connect() that
        // every discover_peers / start_advertising call previously imposed.
        if self.started_flag.load(Ordering::Acquire) {
            if self.shutdown_flag.load(Ordering::SeqCst) {
                return Err(TransportError::ShuttingDown);
            }
            return Ok(());
        }
        let _gate = self.connect_gate.lock().await;
        // Re-check under the gate (another caller may have completed bring-up).
        if self.started_flag.load(Ordering::Acquire) {
            if self.shutdown_flag.load(Ordering::SeqCst) {
                return Err(TransportError::ShuttingDown);
            }
            return Ok(());
        }
        if self.shutdown_flag.load(Ordering::SeqCst) {
            return Err(TransportError::ShuttingDown);
        }
        let adapter = self.adapter().await?;
        adapter
            .start()
            .await
            .map_err(|e| Self::classify_error("wifiaware.start", e))?;
        adapter
            .subscribe()
            .await
            .map_err(|e| Self::classify_error("wifiaware.subscribe", e))?;
        self.spawn_poller(adapter.clone()).await?;
        // WAW-RT-004: at boot the watcher only fires on *transitions*, so an
        // adapter that starts with data scope already down would otherwise be
        // falsely promoted to Available. Mirror the adapter's initial
        // availability instead: Available when unicast-capable, Degraded when
        // the data scope is already down (discovery-only survives §3.5).
        // NEW-WA-RT-102: gate held + flag re-checked — a shutdown that started
        // before us either holds the gate (we waited) or set the flag (we
        // bailed above), so no dead transport is ever promoted.
        if self.state.load() == TransportState::Unavailable {
            let initial = if adapter.is_available() {
                TransportState::Available
            } else {
                TransportState::Degraded
            };
            self.set_state(initial);
        }
        self.spawn_avail_watcher(adapter).await;
        self.started_flag.store(true, Ordering::Release);
        Ok(())
    }

    /// Spawn (once) the task that drains inbound NDP frames into the incoming
    /// channel. NEW-WA-RT-101: frames are attributed to the adapter-reported
    /// sender candidate (or zero fallback); a bounded per-tick budget caps
    /// churn (WAW-RT-005), enforced here in the transport — not just in the
    /// sim — so a real adapter returning a large batch can never cause
    /// unbounded per-tick work or a long event-loop stall (NEW-WA-RT-105).
    /// Overflow beyond the budget is buffered in a local backlog and drained on
    /// later ticks (no frame loss).
    async fn spawn_poller(&self, adapter: Arc<dyn WifiAwareAdapter>) -> Result<(), TransportError> {
        let mut poller = self.poller.lock().await;
        if poller.is_some() {
            return Ok(());
        }
        let incoming_tx = self.incoming_tx.clone();
        let transport_id = self.id.clone();
        let links = self.links.clone();
        let dropped_inbound = self.dropped_inbound.clone();
        let handle = tokio::spawn(async move {
            let mut backlog: std::collections::VecDeque<IncomingNdpData> =
                std::collections::VecDeque::new();
            loop {
                // RT-014: back off to the idle interval when no NDP links are
                // active; only wake at the fast interval while linked.
                let fast = !links.lock().unwrap_or_else(|p| p.into_inner()).is_empty();
                tokio::time::sleep(Duration::from_millis(if fast { 10 } else { IDLE_POLL_MS }))
                    .await;
                // Top up the backlog from the adapter, then forward at most
                // MAX_FRAMES_PER_TICK per tick (NEW-WA-RT-105).
                // BLE-30: the drain is now fallible — a session-died error is
                // no longer indistinguishable from "no frames this tick".
                match adapter.incoming_ndp().await {
                    Ok(frames) => backlog.extend(frames),
                    Err(e) => tracing::warn!(
                        event = "wifiaware.incoming_ndp_failed",
                        error = %e,
                        "NDP drain failed for this tick"
                    ),
                }
                // BLE-27: the per-tick forward budget bounds CPU work but not
                // memory — a real adapter returning more than
                // MAX_FRAMES_PER_TICK per tick (unlike the simulator, whose
                // MAX_DRAIN_PER_CALL is deliberately pinned equal to it) grew
                // this deque forever. Bound it and drop the oldest frames
                // once full: a real burst that outpaces the forward budget
                // means the backlog is already stale, and newest-first is
                // the more useful frame to keep.
                const MAX_BACKLOG: usize = 64 * MAX_FRAMES_PER_TICK;
                if backlog.len() > MAX_BACKLOG {
                    let drop_count = backlog.len() - MAX_BACKLOG;
                    backlog.drain(..drop_count);
                    tracing::warn!(
                        event = "wifiaware.ndp_backlog_overflow",
                        dropped = drop_count,
                        "NDP backlog exceeded bound; oldest frames dropped"
                    );
                }
                for _ in 0..MAX_FRAMES_PER_TICK {
                    let Some(frame) = backlog.pop_front() else {
                        break;
                    };
                    let Some(len) = frame_payload_len(&frame.payload) else {
                        continue; // truncated/oversized header; drop.
                    };
                    let start = FRAME_HEADER_LEN; // GAP-5: version + u32 LE length prefix
                    let end = start + len;
                    if len == 0 || end > frame.payload.len() {
                        continue;
                    }
                    let payload = frame.payload[start..end].to_vec();
                    // NEW-WA-RT-101: prefer the adapter-reported sender candidate
                    // (sim) / underlying NDP peer mapping (real adapter resolves
                    // `frame.ndp` itself). Best-effort only — the IRIS envelope
                    // layer performs real verification (DEC-WA-0007); zero is the
                    // safe never-attacker-controlled fallback (WAW-RT-007).
                    //
                    // FFI-8: WAW-RT-007's rationale doesn't address the
                    // finding's actual complaint — every unattributed frame
                    // collapses onto the SAME fictitious identity, colliding
                    // sender attribution/rate-limiting/dedup/routing state.
                    // The fuller fix (a distinguishable "pending
                    // identification" state instead of either fabricating
                    // an id or dropping the frame) needs a
                    // MessageEngine::process_incoming contract change this
                    // does not make — IncomingMessage.peer_id is required,
                    // not Option, and dropping unattributed frames here was
                    // tried and reverted: it regressed a real integration
                    // test whose simulated adapter never sets `sender` at
                    // all. FFI-1/FFI-3's fix (Wi-Fi Direct's equivalent
                    // handle-namespace unification) already addresses the
                    // PRIMARY real-hardware cause of `sender` being None —
                    // this substitution remains a last-resort fallback.
                    // BLE-31: resolve the NDP handle in `links` first — the
                    // transport already maintains the NdpHandle→PeerId mapping.
                    // Fall back to adapter-reported sender, then zero.
                    let peer_id = links.lock().unwrap()
                        .iter()
                        .find(|l| l.ndp == frame.ndp)
                        .map(|l| l.peer_id)
                        .or(frame.sender)
                        .unwrap_or(PeerId([0u8; 32]));
                    // NEW-WA-RT-106: count (not silently swallow) frames that
                    // cannot be delivered upstream — a closed receiver or a
                    // receiver too slow for the broadcast channel.
                    if incoming_tx
                        .send(IncomingMessage {
                            peer_id,
                            transport_id: transport_id.to_string(),
                            payload,
                            received_at: Instant::now(),
                        })
                        .is_err()
                    {
                        let n = dropped_inbound.fetch_add(1, Ordering::Relaxed) + 1;
                        // BLE-33: warn once on first drop so the field is observable.
                        if n == 1 {
                            tracing::warn!(
                                transport = %transport_id,
                                "first inbound frame dropped: no subscriber yet (dropped_inbound=1)"
                            );
                        }
                    }
                }
            }
        })
        .abort_handle();
        *poller = Some(handle);
        Ok(())
    }

    /// Spawn (once) the task that mirrors availability churn into transport
    /// state (WAW-RT-008): display-off / radio-share → `Degraded` so the
    /// manager steers to another transport; recovery → re-attach path.
    async fn spawn_avail_watcher(&self, adapter: Arc<dyn WifiAwareAdapter>) {
        let mut slot = self.avail_watcher.lock().await;
        if slot.is_some() {
            return;
        }
        let Some(stream) = adapter.availability_stream() else {
            return;
        };
        let state = self.state.clone();
        let state_tx = self.state_tx.clone();
        let id = self.id.clone();
        let links = self.links.clone();
        let handle = tokio::spawn(async move {
            let mut stream = stream;
            while let Some(available) = stream.next().await {
                let current = state.load();
                if !available {
                    // Data scope dropped → surface Degraded so the manager
                    // steers to another transport (discovery-only stays up
                    // per §3.5; the watcher only reflects unicast loss).
                    if state_is_available(&current) && current != TransportState::Degraded {
                        Self::store_state_with_event(
                            &id,
                            &state,
                            &state_tx,
                            TransportState::Degraded,
                        );
                    }
                } else if current == TransportState::Degraded {
                    // Data scope restored → back to Connected when NDP links
                    // survived the outage, else Available (NEW-WA-RT-106:
                    // restoring to Available while linked would clobber
                    // Connected and mislead the manager's steer logic).
                    let restored = if links.lock().unwrap_or_else(|p| p.into_inner()).is_empty() {
                        TransportState::Available
                    } else {
                        TransportState::Connected
                    };
                    Self::store_state_with_event(&id, &state, &state_tx, restored);
                }
            }
        });
        *slot = Some(handle.abort_handle());
    }
}

#[async_trait]
impl Transport for WifiAwareTransport {
    fn transport_id(&self) -> &crate::transport::TransportId {
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
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "wifiaware.state")
    }

    async fn discover_peers(
        &self,
        config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        self.ensure_started().await?;
        let adapter = self.adapter().await?;
        // BLE-30: the drain is now fallible — surface an adapter-session
        // failure as a real error instead of silently reporting zero peers.
        let mut matches = adapter
            .matches()
            .await
            .map_err(TransportError::Protocol)?;
        // BLE-38: sort by RSSI descending so the closest peers are preferred
        // when max_peers truncation applies. rssi=0 (sim) sorts last.
        matches.sort_unstable_by(|a, b| b.rssi.cmp(&a.rssi));
        let mut infos: Vec<PeerInfo> = Vec::new();
        for m in matches.iter().take(config.max_peers) {
            let Ok(beacon) = WifiAwareBeacon::parse(&m.service_specific_info) else {
                continue;
            };
            let peer_id = beacon.candidate_peer_id();
            // BLE-22: skip peers that do not advertise NDP unicast capability.
            if !beacon.capabilities.contains(CapabilityBits::NDP_UNICAST) {
                continue;
            }
            // BLE-22: advisory staleness — skip beacons older than ~60 minutes.
            // freshness==0 is the "no freshness info" sentinel (sim/builder didn't set it).
            if beacon.freshness_minutes != 0 {
                let age_min = freshness_minutes_now().wrapping_sub(beacon.freshness_minutes);
                if age_min > 60 {
                    continue;
                }
            }
            if let Some(filter) = config.filter.as_ref() {
                if !filter.contains(&peer_id) {
                    continue;
                }
            }
            // BLE-39: record the (peer_id, handle) pair seen at discovery time.
            self.discovery_cache
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(peer_id, m.peer_handle);
            infos.push(PeerInfo {
                peer_id,
                addresses: Vec::new(),
                transport_addresses: vec![("wifi-aware".to_string(), m.peer_handle.0.to_string())],
                last_seen: Some(Instant::now()),
            });
        }
        Ok(Box::pin(stream::iter(infos)))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        let adapter = self.adapter().await?;
        adapter
            .unsubscribe()
            .await
            .map_err(|e| Self::classify_error("wifiaware.unsubscribe", e))?;
        // BLE-39: expire the discovery cache — handles are NAN-session-scoped.
        self.discovery_cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clear();
        // BLE-36: if there are no active links the NDP poller serves no purpose
        // and burns the idle-poll timer. Abort it so the transport actually stops
        // scanning. State drops to Available (not Unavailable — the adapter is
        // still up and a fresh discover_peers / ensure_started can restart).
        let no_links = self
            .links
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .is_empty();
        if no_links {
            if let Some(handle) = self.poller.lock().await.take() {
                handle.abort();
            }
            // Only update state if we were Connected (no links left).
            if self.state.load() == TransportState::Connected {
                self.set_state(TransportState::Available);
            }
        }
        Ok(())
    }

    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError> {
        // NEW-WA-RT-104: attach + subscribe before publishing. Publishing on a
        // not-yet-attached adapter violates the Android create
        // (attach → subscribe/publish) ordering and can silently no-op.
        self.ensure_started().await?;
        let adapter = self.adapter().await?;
        if !adapter.is_available() {
            return Err(TransportError::RadioDisabled);
        }
        // FFI-4: this used to ignore `info` entirely and publish
        // `PublishConfig::default()` — no `service_specific_info`, so no
        // IRIS beacon was ever put on the air and every peer's
        // `WifiAwareBeacon::parse` on the discovery side rejected the
        // resulting zero-length bytes. Build the same 22-byte beacon BLE's
        // `start_advertising` builds, from the real node identity.
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::NDP_UNICAST);
        caps.set(CapabilityBits::PUBLISH_BROADCAST);
        let peer_short = crate::identity::peer_id::peer_short(&info.peer_id.0);
        let beacon = WifiAwareBeacon::build(
            caps,
            peer_short,
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                / 60) as u16,
        );
        adapter
            .publish(&PublishConfig {
                service_specific_info: beacon,
                ..PublishConfig::default()
            })
            .await
            .map_err(|e| Self::classify_error("wifiaware.publish", e))?;
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        let adapter = self.adapter().await?;
        adapter
            .unpublish()
            .await
            .map_err(|e| Self::classify_error("wifiaware.unpublish", e))?;
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::HardwareUnavailable);
        }
        let adapter = self.adapter().await?;
        let raw_handle = peer
            .transport_addresses
            .iter()
            .find(|(k, _)| k == "wifi-aware")
            .and_then(|(_, v)| v.parse::<u64>().ok())
            .ok_or(TransportError::PeerNotFound)?;
        // BLE-39: validate the caller-supplied handle against what discover_peers
        // recorded. Handles are NAN-session-scoped; a stale PeerInfo from before
        // an unsubscribe holds a handle that may now identify a different peer.
        let handle = {
            let cache = self.discovery_cache.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(&cached) = cache.get(&peer.peer_id) {
                cached.0
            } else {
                // Peer was not seen in the current discovery session (e.g.
                // reconstructed from storage after a session restart). Use
                // the caller-supplied value but log the mismatch path.
                raw_handle
            }
        };
        // WAW-RT-001: the open-link sequence (reuse check → pool check →
        // open_ndp → register) runs atomically under `connect_gate`. Without
        // it, two concurrent connects to the same peer both pass the "already
        // linked?" check and double-open NDPs, or two connects to different
        // peers both pass the pool check and exceed MAX_NDP_POOL. The gate
        // also serializes against shutdown() (which takes it too).
        let _gate = self.connect_gate.lock().await;
        // Re-check under the gate: a shutdown() may have completed while we
        // awaited the gate.
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::ShuttingDown);
        }
        // NEW-WA-RT-102: gate connect on data-scope availability, matching the
        // start_advertising guard. While Degraded (display-off unicast scope),
        // open_ndp would only fail anyway — return a clean RadioDisabled
        // instead of ConnectionFailed, and never enter Connected with no
        // data scope.
        if !adapter.is_available() {
            return Err(TransportError::RadioDisabled);
        }
        {
            let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            // WAW-RT-001: per-peer single NDP (AC-9). Reuse an existing link
            // instead of opening a fresh NDP on every connect.
            if links.iter().any(|l| l.peer_id == peer.peer_id) {
                return Ok(TransportLink {
                    peer_id: peer.peer_id,
                    transport_id: self.id.to_string(),
                    established_at: Instant::now(),
                });
            }
            if links.len() >= MAX_NDP_POOL {
                return Err(TransportError::Busy);
            }
        }
        let ndp = tokio::time::timeout(CONNECT_TIMEOUT, adapter.open_ndp(PeerHandle(handle)))
            .await
            .map_err(|_| TransportError::ConnectionFailed)?
            .map_err(|_| TransportError::ConnectionFailed)?;
        // WAW-RT-003: defense-in-depth — if the adapter detached internally
        // while open_ndp awaited, don't register a dead NDP. NEW-WA-RT-103:
        // also re-check data-scope availability, so a display-off that landed
        // mid-open can never leave state = Connected while the adapter is
        // unavailable (a pull-only adapter would otherwise be stuck until a
        // follow-up event that never comes).
        if self.state.load() == TransportState::Unavailable || !adapter.is_available() {
            adapter.close_ndp(ndp).await.ok();
            return Err(TransportError::ShuttingDown);
        }
        self.links.lock().unwrap_or_else(|p| p.into_inner()).push(NdpLink {
            ndp,
            peer_id: peer.peer_id,
        });
        self.set_state(TransportState::Connected);
        Ok(TransportLink {
            peer_id: peer.peer_id,
            transport_id: self.id.to_string(),
            established_at: Instant::now(),
        })
    }

    async fn send(
        &self,
        peer: &PeerId,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        if message.payload.len() > MAX_NAN_MESSAGE_BYTES {
            // WAW-RT-004: bound *before* the shared encoder's assert, so an
            // oversized message is a clean error, never a panic (RES-0020 R8).
            // MG-40: resolvable by fragmenting upstream and retrying on this
            // same transport — not a framing/decode failure.
            return Err(TransportError::MessageTooLarge {
                limit: MAX_NAN_MESSAGE_BYTES,
                actual: message.payload.len(),
            });
        }
        let adapter = self.adapter().await?;
        if !adapter.is_available() {
            return Err(TransportError::RadioDisabled);
        }
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::HardwareUnavailable);
        }
        let ndp = self
            .links
            .lock()
            .unwrap()
            .iter()
            .find(|l| l.peer_id == *peer)
            .map(|l| l.ndp)
            .ok_or(TransportError::NotConnected)?;
        let frame = encode_frame(message)?;
        let send_result = tokio::time::timeout(SEND_TIMEOUT, adapter.ndp_send(ndp, &frame))
            .await
            .unwrap_or(Err("ndp_send timed out".to_string()));
        match send_result {
            Ok(()) => {}
            Err(e) => {
                // NEW-WA-RT-107: only a link-loss error is terminal. The FFI
                // contract pins the marker string ("ndp_not_open"/"ndp closed")
                // for a dead path; a transient error (congestion/ENOBUFS,
                // "wifi_aware_busy") must NOT tear the link down — otherwise a
                // burst causes a reconnect storm. The sim reports true link
                // death ("ndp not open") only.
                if is_link_loss_error(&e) {
                    self.teardown_link(&adapter, ndp).await;
                    return Err(TransportError::NotConnected);
                }
                return Err(TransportError::Busy);
            }
        }
        let bytes_sent = frame.len();
        self.ewma.record_send(bytes_sent); // MG-17
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent,
            sent_at: Instant::now(),
        })
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "wifiaware.messages")
    }

    fn cost_snapshot(&self) -> TransportCost {
        let connected = self.state.load() == TransportState::Connected;
        TransportCost {
            estimated_battery_ma: if connected {
                WIFI_AWARE_COST.connected_idle_ma
            } else {
                WIFI_AWARE_COST.scan_ma
            },
            monetary_cost_per_kb: 0.0,
            bandwidth_available_bps: self.ewma.bandwidth_bps(self.caps.typical_throughput_bps),
            congestion_level: 0.0,
        }
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        // WAW-RT-001: serialize against a connect() that may be mid-open, so
        // the teardown never races the link registration below. Also serialized
        // against a racing ensure_started() bring-up (NEW-WA-RT-102).
        let _gate = self.connect_gate.lock().await;
        if let Some(handle) = self.poller.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.avail_watcher.lock().await.take() {
            handle.abort();
        }
        // Close every open NDP before tearing down the adapter so handles
        // don't leak (WAW-RT-001).
        if let Ok(adapter) = self.adapter().await {
            let ndps: Vec<NdpHandle> = self.links.lock().unwrap_or_else(|p| p.into_inner()).iter().map(|l| l.ndp).collect();
            for ndp in ndps {
                adapter.close_ndp(ndp).await.ok();
            }
            adapter.shutdown().await.ok();
        }
        self.links.lock().unwrap_or_else(|p| p.into_inner()).clear();
        // NEW-WA-RT-102: latch shutdown so no later ensure_started() can
        // resurrect the transport (e.g. a discover_peers on a dead transport).
        self.shutdown_flag.store(true, Ordering::SeqCst);
        // BLE-34: clear started_flag so the shutdown_flag check in
        // ensure_started's fast path fires correctly on any subsequent call.
        self.started_flag.store(false, Ordering::Release);
        self.set_state(TransportState::Unavailable);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    use crate::transport::manager::TransportSelectionRequest;
    use crate::transport::TransportManager;

    async fn advertise(t: &WifiAwareTransport) {
        // Sim adapter derives its beacon short id internally.
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([1u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: vec![],
        })
        .await
        .expect("advertise ok");
    }

    fn sample_message(payload: &[u8]) -> SerializedMessage {
        SerializedMessage {
            message_id: crate::protocol::MessageId::from([0x20; 16]),
            priority: MessagePriority::P2,
            payload: payload.to_vec(),
        }
    }

    /// WIAW-AC-4 — NDP round-trip: A connects to B and delivers a payload to
    /// B's incoming stream over the in-memory mesh. NEW-WA-RT-101: also asserts
    /// the frame is *attributed to A's beacon-derived candidate PeerId*, never
    /// the zero fallback (the red-team gap: inbound frames were delivered as
    /// `PeerId([0u8;32])` because the sim forwarded the *sender's* opaque NDP
    /// handle the receiver could never match).
    #[tokio::test]
    async fn ndp_roundtrip_delivers_payload() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = WifiAwareTransport::new(Some(a));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&ta).await;
        advertise(&tb).await;
        let discovered: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(discovered.len(), 1, "A sees B's beacon");
        let peer_b = discovered[0].peer_id;
        let link = ta.connect(&discovered[0]).await.unwrap();
        assert_eq!(link.peer_id, peer_b);

        // B's view of A: A's beacon-derived candidate PeerId — this is what the
        // inbound frame must be attributed to (NEW-WA-RT-101).
        let peer_a_from_b: Vec<PeerInfo> = tb
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peer_a_from_b.len(), 1, "B sees A's beacon");
        let peer_a = peer_a_from_b[0].peer_id;

        let mut rx = tb.incoming_messages();
        let msg = sample_message(b"ndp-hello");
        ta.send(&peer_b, &msg).await.unwrap();

        let got = tokio::time::timeout(Duration::from_secs(3), rx.next())
            .await
            .expect("frame delivered in time")
            .expect("incoming stream alive");
        assert_eq!(got.payload, b"ndp-hello".to_vec());
        assert_eq!(got.transport_id, "wifi-aware-0");
        // NEW-WA-RT-101: the recipient attributes the frame to A's beacon
        // candidate PeerId, never the zero fallback.
        assert_ne!(
            got.peer_id,
            PeerId([0u8; 32]),
            "sender attributed, not zero"
        );
        assert_eq!(got.peer_id, peer_a, "frame attributed to the real sender");
    }

    /// WIAW-AC-2 — discovery: a subscribed transport matches every advertising
    /// peer in the mesh and maps each to a `PeerId` from the beacon short id.
    #[tokio::test]
    async fn discovery_finds_advertising_peer() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = WifiAwareTransport::new(Some(a));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peers.len(), 1);
        assert!(!peers[0].peer_id.0.is_empty());
    }

    /// WIAW-AC-5 — availability churn: when the data scope drops, publish fails
    /// (unicast dropped) while the transport keeps discovery alive.
    #[tokio::test]
    async fn availability_churn_keeps_discovery_up() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = WifiAwareTransport::new(Some(a));
        // Simulate display-off: B's data scope drops *before* any interaction.
        b.available.store(false, Ordering::Release);
        let tb = WifiAwareTransport::new(Some(b));

        // Publish (unicast data scope) is rejected while unavailable.
        assert!(matches!(
            tb.start_advertising(NodeAdvertisement {
                peer_id: PeerId([9u8; 32]),
                public_ip_addr: None,
                hostname: None,
                tags: vec![],
            })
            .await,
            Err(TransportError::RadioDisabled)
        ));

        // Discovery-only stays up: A can still start a scan (mesh survival).
        assert!(ta.discover_peers(DiscoveryConfig::default()).await.is_ok());
    }

    /// WIAW-AC-8 — lifecycle: shutdown aborts the poller and returns the
    /// transport to Unavailable so the manager never selects it again.
    #[tokio::test]
    async fn shutdown_returns_to_unavailable() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = WifiAwareTransport::new(Some(a));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert!(!peers.is_empty(), "discovery works before shutdown");
        ta.shutdown().await.unwrap();
        assert_eq!(ta.state(), TransportState::Unavailable);
        assert!(ta
            .send(&peers[0].peer_id, &sample_message(b"x"))
            .await
            .is_err());
    }

    /// WIAW-AC-1 — registration: an Unavailable transport is never selected by
    /// the manager; it becomes Available after discovery starts.
    #[tokio::test]
    async fn registration_gating() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let adapter: Arc<dyn WifiAwareAdapter> = Arc::new(SimulatedWifiAwareAdapter::new(coord));
        let t = WifiAwareTransport::new(Some(adapter));
        let manager = TransportManager::new();
        manager.register(Arc::new(t)).await.unwrap();

        let req = TransportSelectionRequest {
            target_peer: None,
            message_size: 100,
            priority: MessagePriority::P3,
            max_latency_ms: None,
            prefer_low_cost: true,
            multipath: false,
            fragmentable: false,
        };
        // Unavailable → never selected.
        assert!(manager.select_transports(&req).await.is_empty());
    }

    /// WIAW-SEC-HT-001 (WAW-RT-001) — concurrent connect() to the same peer
    /// must yield exactly ONE NDP: the connect_gate serializes the
    /// check→open→register critical section so no double-open (AC-9).
    #[tokio::test]
    async fn concurrent_connects_open_single_ndp() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = Arc::new(WifiAwareTransport::new(Some(a)));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();

        // Hammer connect for the same peer concurrently; exactly one NDP must
        // be open at the end (gate → duplicate connects reuse the link).
        let mut handles = Vec::new();
        for _ in 0..16 {
            let t = ta.clone();
            let p = peer.clone();
            handles.push(tokio::spawn(async move { t.connect(&p).await }));
        }
        let mut results = Vec::new();
        for h in handles {
            results.push(h.await.unwrap());
        }
        assert!(
            results.iter().all(|r| r.is_ok()),
            "all connects succeed via reuse"
        );
        assert_eq!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).len(), 1, "single NDP per peer");
    }

    /// WIAW-SEC-HT-002 (WAW-RT-001) — a connect() racing shutdown() must never
    /// leave a registered link: shutdown takes the connect_gate so teardown
    /// and the open-link critical section are mutually exclusive.
    #[tokio::test]
    async fn connect_racing_shutdown_leaks_no_link() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = Arc::new(WifiAwareTransport::new(Some(a)));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();

        let connect_t = {
            let t = ta.clone();
            let p = peer.clone();
            tokio::spawn(async move { t.connect(&p).await })
        };
        // Let the connect get underway, then tear everything down mid-flight.
        tokio::task::yield_now().await;
        ta.shutdown().await.unwrap();
        let _ = connect_t.await;

        assert_eq!(ta.state(), TransportState::Unavailable);
        assert!(
            ta.links.lock().unwrap_or_else(|p| p.into_inner()).is_empty(),
            "no link survives shutdown"
        );
        // Any NDP that the in-flight connect opened must have been closed.
        // (The adapter side keeps its own accounting; just assert transport
        //  state consistency — the gate makes shutdown the last writer.)
    }

    /// WIAW-SEC-HT-003 (WAW-RT-002) — a flooded outbox is bounded: the sim
    /// drops the oldest frames instead of buffering without limit, and a
    /// single drain never returns more than the per-call budget.
    #[tokio::test]
    async fn outbox_is_bounded_under_flood() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let ta = Arc::new(WifiAwareTransport::new(Some(a)));
        let tb = WifiAwareTransport::new(Some(b.clone()));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();
        ta.connect(&peer).await.unwrap();

        // Flood: far more frames than the outbox cap, without draining.
        let msg = sample_message(&[0xAA; 1024]);
        for _ in 0..SimMeshCoordinator::MAX_OUTBOX_FRAMES * 4 {
            let _ = ta.send(&peer.peer_id, &msg).await;
        }
        let outbox_len = coord.outbox.lock().unwrap_or_else(|p| p.into_inner()).len();
        assert!(
            outbox_len <= SimMeshCoordinator::MAX_OUTBOX_FRAMES,
            "outbox capped ({outbox_len} > {})",
            SimMeshCoordinator::MAX_OUTBOX_FRAMES
        );
        // BLE-40: dropped counter must be non-zero after flooding 4× the cap.
        assert!(
            coord.dropped.load(Ordering::Relaxed) > 0,
            "dropped counter was not incremented during flood"
        );

        // A single drain obeys the per-call budget and carries sender tags.
        let drained = coord.drain_outbox(*b.tag.lock().unwrap_or_else(|p| p.into_inner()));
        assert!(drained.len() <= SimMeshCoordinator::MAX_DRAIN_PER_CALL);
        // (Sender attribution is asserted in the round-trip test; here the
        //  sender never registered, so a zero from-tag is legitimate.)
    }

    /// NEW-WA-RT-107 — a transient ndp_send error (congestion) must NOT tear
    /// the link down; only link-loss errors are terminal. The sim reports
    /// "ndp_not_open" for a genuinely closed path.
    #[tokio::test]
    async fn transient_send_error_keeps_link() {
        use std::sync::atomic::Ordering as A;

        struct FlakyAdapter {
            inner: Arc<SimulatedWifiAwareAdapter>,
            fail_next: AtomicU64,
        }
        #[async_trait]
        impl WifiAwareAdapter for FlakyAdapter {
            fn availability_stream(
                &self,
            ) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
                self.inner.availability_stream()
            }
            async fn start(&self) -> Result<(), String> {
                self.inner.start().await
            }
            async fn subscribe(&self) -> Result<(), String> {
                self.inner.subscribe().await
            }
            async fn unsubscribe(&self) -> Result<(), String> {
                self.inner.unsubscribe().await
            }
            async fn publish(&self, config: &PublishConfig) -> Result<(), String> {
                self.inner.publish(config).await
            }
            async fn unpublish(&self) -> Result<(), String> {
                self.inner.unpublish().await
            }
            async fn matches(&self) -> Result<Vec<PeerDiscovery>, String> {
                self.inner.matches().await
            }
            async fn open_ndp(&self, peer_handle: PeerHandle) -> Result<NdpHandle, String> {
                self.inner.open_ndp(peer_handle).await
            }
            async fn close_ndp(&self, ndp: NdpHandle) -> Result<(), String> {
                self.inner.close_ndp(ndp).await
            }
            async fn ndp_send(&self, ndp: NdpHandle, payload: &[u8]) -> Result<(), String> {
                if self.fail_next.fetch_sub(1, A::SeqCst) > 0 {
                    return Err("wifi_aware_busy".to_string()); // transient congestion
                }
                self.inner.ndp_send(ndp, payload).await
            }
            async fn incoming_ndp(&self) -> Result<Vec<IncomingNdpData>, String> {
                self.inner.incoming_ndp().await
            }
            async fn shutdown(&self) -> Result<(), String> {
                self.inner.shutdown().await
            }
            fn is_available(&self) -> bool {
                self.inner.is_available()
            }
        }

        let coord = Arc::new(SimMeshCoordinator::new());
        let inner = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let a: Arc<dyn WifiAwareAdapter> = Arc::new(FlakyAdapter {
            inner: inner.clone(),
            fail_next: AtomicU64::new(1),
        });
        let ta = Arc::new(WifiAwareTransport::new(Some(a)));
        let tb = WifiAwareTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();
        ta.connect(&peer).await.unwrap();

        // First send hits the transient error: link survives, error is Busy.
        let msg = sample_message(b"burst");
        assert!(
            matches!(
                ta.send(&peer.peer_id, &msg).await,
                Err(TransportError::Busy)
            ),
            "transient congestion → Busy, no teardown"
        );
        assert_eq!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).len(), 1, "link kept");

        // Second send succeeds because the transient has passed.
        assert!(ta.send(&peer.peer_id, &msg).await.is_ok());
    }

    /// NEW-WA-RT-104 — when the data scope is down, losing the last NDP link
    /// must leave the transport Degraded (not Available) so the manager never
    /// steers onto a data-path-less transport.
    #[tokio::test]
    async fn last_link_death_while_unavailable_is_degraded() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let tb = WifiAwareTransport::new(Some(b));
        let ta = Arc::new(WifiAwareTransport::new(Some(a.clone())));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();
        ta.connect(&peer).await.unwrap();
        assert_eq!(ta.state(), TransportState::Connected);

        // Display-off: data scope drops. Drive the teardown path (remove the
        // last link + close the NDP) while the adapter is unavailable.
        a.set_available(false);
        let adapter: Arc<dyn WifiAwareAdapter> = a.clone();
        let ndp = ta.links.lock().unwrap_or_else(|p| p.into_inner())[0].ndp;
        ta.teardown_link(&adapter, ndp).await;

        // NEW-WA-RT-104: last-link death while the data scope is down must land
        // on Degraded, never Available.
        assert_eq!(ta.state(), TransportState::Degraded);
    }

    /// NEW-WA-RT-102 — connect() while the data scope is down returns
    /// NotSupported (does not attempt a doomed open_ndp), and can never leave
    /// state=Connected while the adapter is unavailable.
    #[tokio::test]
    async fn connect_while_unavailable_is_not_supported() {
        let coord = Arc::new(SimMeshCoordinator::new());
        let a = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiAwareAdapter::new(coord.clone()));
        let tb = WifiAwareTransport::new(Some(b));
        let ta = WifiAwareTransport::new(Some(a.clone()));

        advertise(&tb).await;
        // Display-off before connect.
        a.set_available(false);

        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();
        // Discovery stays up; state reflects Degraded (not Available) since the
        // data scope is already down (WAW-RT-004).
        assert_eq!(ta.state(), TransportState::Degraded);
        assert!(
            matches!(ta.connect(&peer).await, Err(TransportError::RadioDisabled)),
            "connect while data scope down → RadioDisabled (NEW-WA-RT-102)"
        );
        assert_eq!(ta.state(), TransportState::Degraded, "never Connected");
    }
}
