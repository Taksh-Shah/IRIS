//! WIFIDIRECT-001 — Wi-Fi Direct (P2P) transport.
//!
//! Implements the platform-adapter architecture from
//! `docs/transports/TRANSPORT_ABSTRACTION.md` §Platform Adapter Architecture
//! with the Wi-Fi Direct profile in `docs/transports/WIFI_DIRECT.md` and the
//! v1 design in `docs/implementation/WIFI_DIRECT_TRANSPORT_DESIGN.md`:
//!
//! ```text
//! WifiDirectTransport (Rust core)  --calls-->  WifiDirectAdapter trait
//!                                                   ├─ AndroidWifiDirectAdapter (Kotlin/JNI)
//!                                                   └─ Linux wpa_supplicant glue (future)
//! ```
//!
//! The Rust core depends only on the [`WifiDirectAdapter`] trait object — the
//! IRIS transport layer never touches the Android APIs directly. A
//! [`SimulatedWifiDirectAdapter`] (backed by a [`SimP2pCoordinator`]) provides a
//! deterministic in-memory P2P rendezvous for integration tests, so routing and
//! E2E tests run with no Wi-Fi hardware.
//!
//! v1 shape (RES-0021 Q1/Q4/Q5/Q7, WIFI_DIRECT_TRANSPORT_DESIGN.md §1): Wi-Fi
//! Direct is the **data plane on the BLE control plane**. Discovery = **DNS-SD
//! (Bonjour) service discovery** (p2p service name [`WIFI_DIRECT_SERVICE_NAME`],
//! TXT record = 22-byte beacon); data path = **TCP socket over the Group Owner**
//! reusing INTERNET-001 framing (1 MiB cap, pool + backoff). WPA2 protects the
//! L2 link only; the app-layer envelope is the trust anchor (DEC-WD-0002/0007).
//!
//! Security posture: the P2P device MAC is NEVER an identity (platform
//! randomized / persistent-group-bound); peer identity is the 32-byte IRIS
//! PeerId / 16-byte `peer_short` from the DNS-SD TXT record (DEC-WD-0007).
//! Group membership produces *candidate/connection* objects only — every IRIS
//! payload flows through the engine envelope-verify seam (AC-7).
//!
//! Availability: band-restricted GO creation / coex channel-avoidance /
//! single-radio STA+P2P churn surface as availability churn → `Degraded`, never
//! a hard failure (DEC-WD-0008). Discovery re-arms on a 30-s app cadence
//! (framework find is 120 s; the 30-s value is the app-level re-arm, G-WD-7).

use std::collections::{HashMap, HashSet};
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
use crate::transport::internet::{encode_frame, frame_payload_len};
use crate::transport::wifi_direct_serv::{
    WifiDirectCapBits, WifiDirectTxtRecord, WIFI_DIRECT_SERVICE_NAME,
};
use crate::transport::{
    freshness_minutes_now, AtomicState, Transport, TransportCapabilities, TransportCost,
    TransportState, TransportStateEvent, WIFI_DIRECT_COST,
};
use crate::TransportError;

/// Maximum message bytes carried in one Wi-Fi Direct frame (INTERNET-001 TCP
/// framing cap, RES-0021 Q7 / WIFI_DIRECT_TRANSPORT_DESIGN.md §2.2).
// SYS-2: deadline constants for Wi-Fi Direct adapter calls.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

const MAX_WIFI_DIRECT_MESSAGE_BYTES: usize = 1024 * 1024;

/// Bounded GO-side connection table (software admission cap, DEC-WD-0006): the
/// platform's actual client ceiling is vendor/HAL-specific (G-WD-1) — this is a
/// bounded in-transport table, never a hard-coded-8 platform assumption.
const MAX_GO_CLIENTS: usize = 8;

/// Incoming-frame channel capacity (RT-009): at most 32 `IncomingMessage`s of
/// up to 1 MiB each → 32 MiB worst-case buffered bytes (the poller forwards up
/// to `MAX_FRAMES_PER_TICK` frames per 10-ms tick, bounded work, no unbounded
/// buffering).
const INCOMING_CHANNEL_CAPACITY: usize = 1024;

/// Max frames forwarded to the engine per poll tick (bounded per-tick work).
const MAX_FRAMES_PER_TICK: usize = 8;

/// Idle poll cadence when no group links are active.
const IDLE_POLL_MS: u64 = 500;

/// App-level discovery re-arm cadence (RES-0021 Q4/G-WD-7): the framework P2P
/// find window is 120 s (`DISCOVER_TIMEOUT_S`); IRIS re-arms on a 30-s window.
const DISCOVERY_WINDOW: Duration = Duration::from_secs(30);

/// GO static address returned by the sim adapter (Android practice-stable
/// `192.168.49.1`; the transport treats `go_addr` as **adapter-supplied** and
/// never hard-codes it — G-WD-2, the sim just mirrors Android practice).
const GO_STATIC_ADDR: &str = "192.168.49.1";

/// Group-owner election intent bias (WIFI_DIRECT.md): fixed infra 14 /
/// battery 7 / low 3; framework default 6.
const GO_INTENT_BALANCED: u8 = 7;

// ---------------------------------------------------------------------------
// Domain types (WIFI_DIRECT.md §Domain Model)
// ---------------------------------------------------------------------------

/// Opaque peer handle assigned by the platform P2P manager to a discovered
/// peer. Never an identity — the P2P device MAC is platform-randomized /
/// persistent-group-bound (DEC-WD-0007).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PeerHandle(pub u64);

/// Operating band hint (RES-0021 Q5: `setGroupOperatingBand` = API 29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OperatingBand {
    #[default]
    Auto,
    Ghz24,
    Ghz5,
    Ghz6,
}

/// Group formation config (WIFI_DIRECT_TRANSPORT_DESIGN.md §2.2/§2.4).
#[derive(Debug, Clone)]
pub struct GroupConfig {
    /// GO election intent (0 = never GO, 15 = always GO). Bias 14/7/3 by role.
    pub go_intent: u8,
    /// Autonomous persistent GO until `remove_group` (RES-0021 Q5).
    pub persistent: bool,
    /// Preferred band hint; band-constrained GO creation fails → AUTO fallback.
    pub band: OperatingBand,
}

impl Default for GroupConfig {
    fn default() -> Self {
        GroupConfig {
            go_intent: GO_INTENT_BALANCED,
            persistent: true,
            band: OperatingBand::Auto,
        }
    }
}

/// One discovery match: the peer's handle + its DNS-SD TXT-record beacon.
#[derive(Debug, Clone)]
pub struct PeerDiscovery {
    pub peer_handle: PeerHandle,
    /// Matched p2p service name (must equal [`WIFI_DIRECT_SERVICE_NAME`]).
    pub service_name: String,
    /// TXT-record value bytes (the IRIS beacon, 22-byte prefix).
    pub txt_record: Vec<u8>,
}

/// Group membership snapshot (Group Owner + clients).
#[derive(Debug, Clone)]
pub struct GroupInfo {
    pub group_id: u64,
    /// The GO's peer handle.
    pub go: PeerHandle,
    /// GO-supplied endpoint address (adapter-provided, never hard-coded, G-WD-2).
    pub go_addr: Option<String>,
    pub clients: Vec<PeerHandle>,
}

/// A frame received in a P2P group (models the TCP-over-GO data plane).
#[derive(Debug, Clone)]
pub struct IncomingWifiDirectData {
    /// Best-effort candidate sender identity (TXT-record short id when known).
    pub sender: Option<PeerId>,
    pub payload: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Platform adapter trait (FFI contract, DESIGN §5)
// ---------------------------------------------------------------------------

/// Platform bridge the Rust core depends on. Injected by the app: an Android
/// adapter (Kotlin/JNI, `WifiP2pManager` + DNS-SD) on devices, the simulated
/// adapter in tests.
#[async_trait]
pub trait WifiDirectAdapter: Send + Sync {
    /// Power on the Wi-Fi Direct subsystem. Idempotent.
    async fn start(&self) -> Result<(), String>;

    /// Register the DNS-SD (Bonjour) service advertisement
    /// (`addLocalService`), carrying `txt_record` (`WifiDirectTxtRecord
    /// ::build`, 22 bytes) as the TXT-record value (FFI-5).
    ///
    /// This used to take no arguments at all: `registerDnsSd()` on the
    /// Kotlin side registered the DNS-SD service with an empty TXT map, and
    /// `dnsSdTxtRecordListener` reconstructed a peer's TXT record as a
    /// UTF-8 "k=v" concatenation — neither is the 22-byte binary beacon
    /// `WifiDirectTxtRecord::parse` requires, so every peer's TXT record
    /// failed to parse and Wi-Fi Direct discovery yielded zero peers,
    /// permanently, independent of FFI-1/2/3.
    async fn start_dns_sd(&self, txt_record: Vec<u8>) -> Result<(), String>;

    /// Withdraw the DNS-SD service advertisement. Idempotent.
    async fn stop_dns_sd(&self) -> Result<(), String>;

    /// Begin a discovery find window (`discoverServices`, 120-s framework).
    async fn start_discovery(&self) -> Result<(), String>;

    /// Stop the discovery find window (`WIFI_P2P_DISCOVERY_CHANGED_ACTION`).
    async fn stop_discovery(&self) -> Result<(), String>;

    /// Drain current DNS-SD service matches (candidates only).
    async fn matches(&self) -> Vec<PeerDiscovery>;

    /// Form an autonomous persistent Group Owner (RES-0021 Q5).
    async fn create_group(&self, config: &GroupConfig) -> Result<GroupInfo, String>;

    /// Join an existing peer's group as a client (`connect`, GO intent 0).
    async fn join_group(&self, go: PeerHandle, config: &GroupConfig) -> Result<GroupInfo, String>;

    /// GO invites a discovered peer into the current group (`p2p_invite`).
    async fn add_client(&self, client: PeerHandle) -> Result<(), String>;

    /// Tear down the current group (`removeGroup`). Idempotent.
    async fn remove_group(&self) -> Result<(), String>;

    /// Snapshot of the current group (if any).
    async fn group_info(&self) -> Option<GroupInfo>;

    /// GO-supplied endpoint address for the current group (adapter-provided).
    fn go_addr(&self) -> Option<String>;

    /// Band hint for group formation (`setGroupOperatingBand`, API 29).
    async fn set_operating_band(&self, band: OperatingBand) -> Result<(), String>;

    /// Send a frame to a peer inside the current group (TCP-over-GO).
    async fn p2p_send(&self, peer: PeerHandle, payload: &[u8]) -> Result<(), String>;

    /// Drain inbound group frames.
    async fn incoming(&self) -> Vec<IncomingWifiDirectData>;

    /// Tear down the subsystem (unregister DNS-SD, remove group, detach).
    async fn shutdown(&self) -> Result<(), String>;

    /// Current availability. False = band-restricted / coex-constrained / radio
    /// shared → data plane unavailable (discovery DNS-SD may stay up).
    fn is_available(&self) -> bool;

    /// Optional push channel for availability transitions
    /// (`WIFI_P2P_STATE_CHANGED_ACTION` on Android). Default: pull-only via
    /// [`WifiDirectAdapter::is_available`].
    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        None
    }
}

// ---------------------------------------------------------------------------
// Simulated adapter + P2P coordinator (deterministic, no hardware)
// ---------------------------------------------------------------------------

/// In-memory P2P mesh for the simulated adapter. Adapters sharing a coordinator
/// can advertise, match, form groups, and exchange frames; distinct
/// coordinators are isolated mesh "islands", so test setups never cross-talk.
pub struct SimP2pCoordinator {
    /// registered tags -> (service_name, TXT-record bytes).
    services: StdMutex<HashMap<u64, (String, Vec<u8>)>>,
    /// active groups: group_id -> {go, members}.
    groups: StdMutex<HashMap<u64, SimGroup>>,
    /// queued frames: (to_tag, from_tag, payload).
    outbox: StdMutex<Vec<(u64, u64, Vec<u8>)>>,
    next_tag: AtomicU64,
    next_group: AtomicU64,
}

struct SimGroup {
    go: u64,
    members: HashSet<u64>,
}

impl SimP2pCoordinator {
    pub fn new() -> Self {
        SimP2pCoordinator {
            services: StdMutex::new(HashMap::new()),
            groups: StdMutex::new(HashMap::new()),
            outbox: StdMutex::new(Vec::new()),
            next_tag: AtomicU64::new(1),
            next_group: AtomicU64::new(1),
        }
    }

    /// Allocate a fresh tag without touching the services map (the caller
    /// immediately follows with a single-lock [`Self::upsert_peer`], so no
    /// other adapter can observe an empty/unattributable advertisement).
    fn alloc_tag(&self) -> u64 {
        self.next_tag.fetch_add(1, Ordering::Relaxed)
    }

    /// Insert (fresh tag) or replace (existing tag) a service advertisement
    /// under one lock — the tag and its TXT record land atomically, closing
    /// the empty-TXT window (WIFIDIRECT-001_RT-001).
    fn upsert_peer(&self, tag: u64, service_name: String, txt_record: Vec<u8>) {
        self.services
            .lock()
            .unwrap()
            .insert(tag, (service_name, txt_record));
    }

    /// Latest TXT-record bytes advertised by `tag`, if any (inbound candidate
    /// attribution).
    fn txt_of(&self, tag: u64) -> Option<Vec<u8>> {
        self.services.lock().unwrap_or_else(|p| p.into_inner()).get(&tag).map(|e| e.1.clone())
    }

    fn unregister_peer(&self, tag: u64) {
        self.services.lock().unwrap_or_else(|p| p.into_inner()).remove(&tag);
        self.groups.lock().unwrap_or_else(|p| p.into_inner()).retain(|_, g| {
            g.members.remove(&tag);
            !g.members.is_empty()
        });
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        outbox.retain(|(to, _, _)| *to != tag);
    }

    /// All (tag, service_name, txt) registered except `self_tag`, filtered to
    /// the IRIS p2p service name (DNS-SD match).
    fn visible_services(&self, self_tag: u64) -> Vec<(u64, String, Vec<u8>)> {
        self.services
            .lock()
            .unwrap()
            .iter()
            .filter(|(t, (name, _))| **t != self_tag && name == WIFI_DIRECT_SERVICE_NAME)
            .map(|(t, (name, txt))| (*t, name.clone(), txt.clone()))
            .collect()
    }

    fn has_peer(&self, tag: u64) -> bool {
        self.services.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&tag)
    }

    fn create_group(&self, go: u64) -> u64 {
        let id = self.next_group.fetch_add(1, Ordering::Relaxed);
        let mut members = HashSet::new();
        members.insert(go);
        self.groups
            .lock()
            .unwrap()
            .insert(id, SimGroup { go, members });
        id
    }

    fn group_of_go(&self, go: u64) -> Option<u64> {
        self.groups
            .lock()
            .unwrap()
            .iter()
            .find(|(_, g)| g.go == go)
            .map(|(id, _)| *id)
    }

    fn add_member(&self, group_id: u64, tag: u64) {
        if let Some(g) = self.groups.lock().unwrap_or_else(|p| p.into_inner()).get_mut(&group_id) {
            g.members.insert(tag);
        }
    }

    fn members_of(&self, group_id: u64) -> Vec<u64> {
        self.groups
            .lock()
            .unwrap()
            .get(&group_id)
            .map(|g| g.members.iter().copied().collect())
            .unwrap_or_default()
    }

    fn go_of(&self, group_id: u64) -> Option<u64> {
        self.groups.lock().unwrap_or_else(|p| p.into_inner()).get(&group_id).map(|g| g.go)
    }

    fn in_same_group(&self, a: u64, b: u64) -> bool {
        self.groups
            .lock()
            .unwrap()
            .values()
            .any(|g| g.members.contains(&a) && g.members.contains(&b))
    }

    fn remove_group(&self, group_id: u64) {
        self.groups.lock().unwrap_or_else(|p| p.into_inner()).remove(&group_id);
    }

    /// Max queued frames in the in-memory outbox before senders are refused;
    /// a full outbox drops the oldest frame (finite TCP buffer, backpressure).
    const MAX_OUTBOX_FRAMES: usize = 128;

    /// Upper bound on frames returned by `drain_outbox` per call — equal to the
    /// poller's per-tick budget, so the adapter never hands the poller more
    /// frames than it forwards. Overflow stays queued (bounded work, no loss).
    const MAX_DRAIN_PER_CALL: usize = MAX_FRAMES_PER_TICK;

    fn proxy_send(&self, from_tag: u64, to_tag: u64, payload: &[u8]) {
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        if outbox.len() >= Self::MAX_OUTBOX_FRAMES {
            // RT-002: drop the oldest frame queued for the *same* destination
            // when full, so one busy sender can never evict frames staged for
            // another peer (per-destination starvation flight). Falls back to
            // dropping the globally-oldest frame when none matches.
            let evict = outbox
                .iter()
                .position(|(to, _, _)| *to == to_tag)
                .unwrap_or(0);
            outbox.remove(evict);
        }
        outbox.push((to_tag, from_tag, payload.to_vec()));
    }

    fn drain_outbox(&self, tag: u64) -> Vec<(u64, Vec<u8>)> {
        let mut outbox = self.outbox.lock().unwrap_or_else(|p| p.into_inner());
        let mut mine = Vec::with_capacity(Self::MAX_DRAIN_PER_CALL.min(outbox.len()));
        let mut keep = Vec::with_capacity(outbox.len());
        let mut budget = Self::MAX_DRAIN_PER_CALL;
        for (to, from, payload) in outbox.drain(..) {
            if to == tag && budget > 0 {
                mine.push((from, payload));
                budget -= 1;
            } else {
                keep.push((to, from, payload));
            }
        }
        *outbox = keep;
        mine
    }
}

impl Default for SimP2pCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// [`WifiDirectAdapter`] simulation. Registers a DNS-SD advertisement into the
/// mesh; `matches()` returns every other IRIS-service advertiser; `create_group`
/// forms a persistent GO; `join_group`/`add_client` build the group; `p2p_send`
/// routes frames within the group (TCP-over-GO model).
pub struct SimulatedWifiDirectAdapter {
    coordinator: Arc<SimP2pCoordinator>,
    tag: StdMutex<u64>,
    dns_sd_on: AtomicBool,
    discovery_on: AtomicBool,
    band: StdMutex<OperatingBand>,
    band_restricted: AtomicBool,
    group_id: StdMutex<Option<u64>>,
    available: AtomicBool,
    available_tx: broadcast::Sender<bool>,
    available_rx: StdMutex<Option<broadcast::Receiver<bool>>>,
    /// Adapter start time, drives monotonic `freshness_minutes`.
    started: Instant,
}

impl SimulatedWifiDirectAdapter {
    pub fn new(coordinator: Arc<SimP2pCoordinator>) -> Self {
        let (available_tx, available_rx) = broadcast::channel(8);
        SimulatedWifiDirectAdapter {
            coordinator,
            tag: StdMutex::new(0),
            dns_sd_on: AtomicBool::new(false),
            discovery_on: AtomicBool::new(false),
            band: StdMutex::new(OperatingBand::Auto),
            band_restricted: AtomicBool::new(false),
            group_id: StdMutex::new(None),
            available: AtomicBool::new(true),
            available_tx,
            available_rx: StdMutex::new(Some(available_rx)),
            started: Instant::now(),
        }
    }

    fn txt_bytes(&self, peer_short: &[u8; 16]) -> Vec<u8> {
        let caps = WifiDirectCapBits::from_bits(
            WifiDirectCapBits::GROUP_OWNER_CAPABLE
                | WifiDirectCapBits::CLIENT_CAPABLE
                | WifiDirectCapBits::PERSISTENT_GO
                | WifiDirectCapBits::BAND_5GHZ,
        );
        WifiDirectTxtRecord::build(
            caps,
            *peer_short,
            freshness_minutes_now(), // GAP-6: wall-clock epoch, not uptime
        )
    }

    /// Register (first call) or update (later calls) our DNS-SD advertisement.
    fn register(&self) {
        let mut tag = self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if *tag == 0 {
            *tag = self.coordinator.alloc_tag();
        }
        let mut short = [0u8; 16];
        short[..8].copy_from_slice(&tag.to_le_bytes());
        // RT-001: tag + TXT land in one atomic map insert, so no peer can ever
        // observe this advertisement with an empty (unattributable) record.
        self.coordinator.upsert_peer(
            *tag,
            WIFI_DIRECT_SERVICE_NAME.to_string(),
            self.txt_bytes(&short),
        );
    }

    /// Simulate availability churn (band restriction / coex / radio share).
    /// Pushes to the availability stream so the transport can surface `Degraded`.
    pub fn set_available(&self, available: bool) {
        self.available.store(available, Ordering::Release);
        self.available_tx.send(available).ok();
    }

    /// Simulate a band-restricted GO formation (5 GHz unavailable). `create_group`
    /// with a non-AUTO band then fails; AUTO falls back (AC-3).
    pub fn set_band_restricted(&self, restricted: bool) {
        self.band_restricted.store(restricted, Ordering::Release);
    }
}

#[async_trait]
impl WifiDirectAdapter for SimulatedWifiDirectAdapter {
    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        self.available_rx.lock().unwrap_or_else(|p| p.into_inner()).take().map(|rx| {
            Box::pin(crate::transport::broadcast_stream(rx, "wifi_direct.availability"))
                as Pin<Box<dyn Stream<Item = bool> + Send + 'static>>
        })
    }

    async fn start(&self) -> Result<(), String> {
        Ok(())
    }

    async fn start_dns_sd(&self, _txt_record: Vec<u8>) -> Result<(), String> {
        // The simulator builds its own synthetic TXT bytes from an internal
        // tag (`txt_bytes`, `register()` below) for round-trip discovery
        // tests — it never had a real platform TXT map to be wrong about,
        // so FFI-5 doesn't reproduce here. The real callers (Android bridge)
        // are what actually forward `txt_record` on to the platform.
        if !self.available.load(Ordering::Acquire) {
            return Err("wifi_direct_unavailable".to_string());
        }
        self.register();
        self.dns_sd_on.store(true, Ordering::Release);
        Ok(())
    }

    async fn stop_dns_sd(&self) -> Result<(), String> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if tag != 0 {
            self.coordinator.unregister_peer(tag);
        }
        self.dns_sd_on.store(false, Ordering::Release);
        Ok(())
    }

    async fn start_discovery(&self) -> Result<(), String> {
        if !self.dns_sd_on.load(Ordering::Acquire) {
            return Err("wifi_direct_dns_sd_off".to_string());
        }
        self.discovery_on.store(true, Ordering::Release);
        Ok(())
    }

    async fn stop_discovery(&self) -> Result<(), String> {
        self.discovery_on.store(false, Ordering::Release);
        Ok(())
    }

    async fn matches(&self) -> Vec<PeerDiscovery> {
        if !self.discovery_on.load(Ordering::Acquire) {
            return Vec::new();
        }
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        self.coordinator
            .visible_services(tag)
            .into_iter()
            .map(|(t, name, txt)| PeerDiscovery {
                peer_handle: PeerHandle(t),
                service_name: name,
                txt_record: txt,
            })
            .collect()
    }

    async fn create_group(&self, config: &GroupConfig) -> Result<GroupInfo, String> {
        if !self.available.load(Ordering::Acquire) {
            return Err("wifi_direct_unavailable".to_string());
        }
        if self.band_restricted.load(Ordering::Acquire) && config.band != OperatingBand::Auto {
            return Err("band_restricted".to_string());
        }
        if self.group_id.lock().unwrap_or_else(|p| p.into_inner()).is_some() {
            return self
                .group_info()
                .await
                .ok_or_else(|| "group_missing".into());
        }
        let my_tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if my_tag == 0 {
            return Err("wifi_direct_not_registered".to_string());
        }
        let gid = self.coordinator.create_group(my_tag);
        *self.group_id.lock().unwrap_or_else(|p| p.into_inner()) = Some(gid);
        Ok(GroupInfo {
            group_id: gid,
            go: PeerHandle(my_tag),
            go_addr: Some(GO_STATIC_ADDR.to_string()),
            clients: Vec::new(),
        })
    }

    async fn join_group(&self, go: PeerHandle, _config: &GroupConfig) -> Result<GroupInfo, String> {
        if !self.available.load(Ordering::Acquire) {
            return Err("wifi_direct_unavailable".to_string());
        }
        if !self.coordinator.has_peer(go.0) {
            return Err("peer not found".to_string());
        }
        let my_tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if my_tag == 0 {
            return Err("wifi_direct_not_registered".to_string());
        }
        // RT-003/RT-012: never manufacture a "phantom" group for a peer that
        // is not a group owner — a client can only join an existing GO.
        let gid = self
            .coordinator
            .group_of_go(go.0)
            .ok_or_else(|| "peer is not a group owner".to_string())?;
        {
            let mut slot = self.group_id.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(current) = *slot {
                // Single-group-per-adapter platform contract: joining a second
                // group must not silently overwrite the first.
                if current != gid {
                    return Err("already in a group".to_string());
                }
                // Idempotent re-join of the same group.
                return Ok(GroupInfo {
                    group_id: current,
                    go,
                    go_addr: Some(GO_STATIC_ADDR.to_string()),
                    clients: self
                        .coordinator
                        .members_of(current)
                        .into_iter()
                        .map(PeerHandle)
                        .collect(),
                });
            }
            *slot = Some(gid);
        }
        self.coordinator.add_member(gid, my_tag);
        Ok(GroupInfo {
            group_id: gid,
            go,
            go_addr: Some(GO_STATIC_ADDR.to_string()),
            clients: self
                .coordinator
                .members_of(gid)
                .into_iter()
                .map(PeerHandle)
                .collect(),
        })
    }

    async fn add_client(&self, client: PeerHandle) -> Result<(), String> {
        if !self.coordinator.has_peer(client.0) {
            return Err("peer not found".to_string());
        }
        let gid = self
            .group_id
            .lock()
            .unwrap()
            .ok_or_else(|| "no_group".to_string())?;
        self.coordinator.add_member(gid, client.0);
        Ok(())
    }

    async fn remove_group(&self) -> Result<(), String> {
        if let Some(gid) = self.group_id.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.coordinator.remove_group(gid);
        }
        Ok(())
    }

    async fn group_info(&self) -> Option<GroupInfo> {
        let gid = (*self.group_id.lock().unwrap_or_else(|p| p.into_inner()))?;
        let go = PeerHandle(self.coordinator.go_of(gid)?);
        Some(GroupInfo {
            group_id: gid,
            go,
            go_addr: Some(GO_STATIC_ADDR.to_string()),
            clients: self
                .coordinator
                .members_of(gid)
                .into_iter()
                .map(PeerHandle)
                .collect(),
        })
    }

    fn go_addr(&self) -> Option<String> {
        self.group_id
            .lock()
            .unwrap()
            .and_then(|gid| self.coordinator.go_of(gid))
            .map(|_| GO_STATIC_ADDR.to_string())
    }

    async fn set_operating_band(&self, band: OperatingBand) -> Result<(), String> {
        *self.band.lock().unwrap_or_else(|p| p.into_inner()) = band;
        Ok(())
    }

    async fn p2p_send(&self, peer: PeerHandle, payload: &[u8]) -> Result<(), String> {
        if !self.available.load(Ordering::Acquire) {
            return Err("wifi_direct_unavailable".to_string());
        }
        let my_tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if !self.coordinator.in_same_group(my_tag, peer.0) {
            return Err("peer not in group".to_string());
        }
        self.coordinator.proxy_send(my_tag, peer.0, payload);
        Ok(())
    }

    async fn incoming(&self) -> Vec<IncomingWifiDirectData> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        self.coordinator
            .drain_outbox(tag)
            .into_iter()
            .map(|(from, payload)| {
                let sender = self
                    .coordinator
                    .txt_of(from)
                    .and_then(|t| WifiDirectTxtRecord::parse(&t).ok())
                    .map(|r| r.candidate_peer_id());
                IncomingWifiDirectData { sender, payload }
            })
            .collect()
    }

    async fn shutdown(&self) -> Result<(), String> {
        let tag = *self.tag.lock().unwrap_or_else(|p| p.into_inner());
        if tag != 0 {
            self.coordinator.unregister_peer(tag);
        }
        if let Some(gid) = self.group_id.lock().unwrap_or_else(|p| p.into_inner()).take() {
            self.coordinator.remove_group(gid);
        }
        self.dns_sd_on.store(false, Ordering::Release);
        self.discovery_on.store(false, Ordering::Release);
        Ok(())
    }

    fn is_available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// Static capabilities for the Wi-Fi Direct transport (DESIGN §4 matrix).
fn wifi_direct_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        max_message_size: MAX_WIFI_DIRECT_MESSAGE_BYTES,
        supports_broadcast: true, // DNS-SD service discovery (AC-2)
        supports_unicast: true,
        supports_multicast: false,
        range_m_min: 50,
        range_m_max: 200,
        range_m_typical: 100,
        typical_throughput_bps: 10_000_000,
        typical_latency_ms: 1000,
        requires_infrastructure: false,
        supports_background_android: false, // foreground/FGS + wakelock only (AC-10)
        supports_background_ios: false,
        requires_special_hardware: false,
        cost_class: crate::transport::TransportCostClass::Free,
        regulatory_band: None,
        conflict_group: crate::transport::RadioConflictGroup::WiFi24GHz,
    }
}

/// An open group link to a peer.
struct WifiDirectLink {
    handle: PeerHandle,
    peer_id: PeerId,
}

/// Wi-Fi Direct transport (see module docs).
pub struct WifiDirectTransport {
    id: crate::transport::TransportId,
    display: String,
    caps: TransportCapabilities,
    adapter: AsyncMutex<Option<Arc<dyn WifiDirectAdapter>>>,
    state: Arc<AtomicState>,
    state_tx: broadcast::Sender<TransportStateEvent>,
    /// RF-30: wrapped in Option so shutdown() can drop it, closing all receivers.
    incoming_tx: StdMutex<Option<broadcast::Sender<IncomingMessage>>>,
    poller: AsyncMutex<Option<AbortHandle>>,
    avail_watcher: AsyncMutex<Option<AbortHandle>>,
    links: Arc<StdMutex<Vec<WifiDirectLink>>>,
    group_config: GroupConfig,
    /// Serializes the whole open-link critical section (single group per
    /// transport; bounded GO connection table, AC-9).
    connect_gate: AsyncMutex<()>,
    /// Set once `shutdown()` has completed; prevents a racing bring-up from
    /// resurrecting a dead transport.
    shutdown_flag: AtomicBool,
    /// FFI-13: set once `ensure_started()` learns this device has no Wi-Fi
    /// Direct hardware at all (adapter.start() returns NotSupported). This is
    /// a genuinely permanent condition — unlike a transient I/O failure — so
    /// it's tracked separately from `state` (whose Unavailable variant is
    /// also the pre-start default and would otherwise be indistinguishable
    /// from "never tried yet").
    permanently_unsupported: AtomicBool,
    /// HW-13: set once `ensure_started()` has registered the empty DNS-SD
    /// placeholder at least once. `ensure_started()` — despite its own doc
    /// comment calling it "one-shot" — has no other guard against running
    /// its body more than once: `discover_peers()` calls it on EVERY pass,
    /// not just the first. The Android bridge's `dnsSdGate` only ever runs
    /// its `addLocalService` registration once and caches that forever
    /// (matching its own doc: registers "at most once per discovery
    /// session"), so it treats every `start_dns_sd()` call after the first
    /// as a request to re-register under new data — meaning a SECOND
    /// unconditional empty-placeholder call (the second, third, ... N-th
    /// discovery pass) silently overwrote the real beacon `start_advertising`
    /// had already registered, permanently reverting this node to
    /// undiscoverable after its very first discovery pass. Confirmed live on
    /// 2 physical devices: the real 22-byte beacon registered correctly for
    /// several seconds, then a same-second follow-up empty registration
    /// silently reverted it — with `engine.start_all_partial` still reporting
    /// wifi-direct-0 successfully started throughout, no error anywhere.
    dns_sd_bootstrapped: AtomicBool,
    /// Discovery re-arm window end (app cadence, 30 s); None = not discovering.
    discovery_until: StdMutex<Option<Instant>>,
    /// Inbound frames that could not be delivered upstream (telemetry).
    dropped_inbound: Arc<AtomicU64>,
}

impl fmt::Debug for WifiDirectTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WifiDirectTransport")
            .field("id", &self.id)
            .field("display", &self.display)
            .field("state", &self.state.load())
            .finish()
    }
}

impl WifiDirectTransport {
    /// Create a transport wrapping the given platform adapter. The transport
    /// begins Unavailable; it becomes Available after the first successful
    /// async interaction (DNS-SD advertisement / discovery), and Connected once
    /// a group link is open.
    pub fn new(adapter: Option<Arc<dyn WifiDirectAdapter>>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _incoming_rx0) = broadcast::channel(INCOMING_CHANNEL_CAPACITY);
        let id = crate::transport::TransportId::from("wifi-direct-0");
        let state = Arc::new(AtomicState::default());
        state.store(TransportState::Unavailable);
        WifiDirectTransport {
            caps: wifi_direct_capabilities(),
            id,
            display: "Wi-Fi Direct".to_string(),
            adapter: AsyncMutex::new(adapter),
            state,
            state_tx,
            incoming_tx: StdMutex::new(Some(incoming_tx)),
            poller: AsyncMutex::new(None),
            avail_watcher: AsyncMutex::new(None),
            links: Arc::new(StdMutex::new(Vec::new())),
            group_config: GroupConfig::default(),
            connect_gate: AsyncMutex::new(()),
            shutdown_flag: AtomicBool::new(false),
            permanently_unsupported: AtomicBool::new(false),
            dns_sd_bootstrapped: AtomicBool::new(false),
            discovery_until: StdMutex::new(None),
            dropped_inbound: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Construct with a group config (GO intent bias 14/7/3, band, persistence).
    pub fn with_group_config(
        adapter: Option<Arc<dyn WifiDirectAdapter>>,
        cfg: GroupConfig,
    ) -> Self {
        let mut t = Self::new(adapter);
        t.group_config = cfg;
        t
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

    /// FFI-7: classify an adapter error string, distinguishing a revoked
    /// runtime permission from a plain I/O failure. Every discovery-path
    /// platform call (`discoverServices`, `addServiceRequest`,
    /// `addLocalService`, `connect`, `createGroup`) throws a synchronous
    /// `SecurityException` when `NEARBY_WIFI_DEVICES` (API 33+) /
    /// `ACCESS_FINE_LOCATION` (API <=32) isn't granted; Kotlin's
    /// `awaitAction` now converts that into `IrisFfiError::PermissionDenied`
    /// ("permission denied") instead of letting it propagate undeclared.
    /// MG-41: mapped to `TransportError::PermissionDenied` — same target as
    /// BLE's `BleError::PermissionDenied` (see `ble.rs`) — so the manager
    /// can surface an actionable prompt instead of treating a permission
    /// gate as a retryable I/O blip it will spin on forever. Unlike FFI-13's
    /// `permanently_unsupported` latch, this does NOT mark the transport
    /// permanently unsupported — a revoked permission can be re-granted at
    /// any time (Settings, a fresh runtime prompt), so the next call is
    /// simply free to try again.
    fn classify_error(context: &str, e: String) -> TransportError {
        if e.contains("permission denied") {
            TransportError::PermissionDenied {
                permission: "wifi_direct",
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
    async fn adapter(&self) -> Result<Arc<dyn WifiDirectAdapter>, TransportError> {
        self.adapter
            .lock()
            .await
            .clone()
            .ok_or(TransportError::HardwareUnavailable)
    }

    /// HW-17: `self.links` is keyed by whatever `PeerId` `connect()` was
    /// called with — and discovery hands `connect()` the TXT record's
    /// candidate id (`WifiDirectTxtRecord::candidate_peer_id()`:
    /// `peer_short(real_pubkey)`, zero-padded to 32 bytes — "candidate
    /// hint," never the verified full identity, mirrors BLE's HW-5 /
    /// DEC-BLE-0006). But `send()` is addressed by the envelope's real
    /// recipient `PeerId`, which is never equal to the zero-padded
    /// candidate form — `links.iter().find(|l| l.peer_id == *peer)` missed
    /// on every real send, confirmed live: `transport: not connected to
    /// peer` immediately after `discovery.connect_ok` fired for the same
    /// physical link. `peer_short()` is a pure function of the real
    /// pubkey, so the candidate form for any real `PeerId` can be
    /// independently re-derived here — same scheme, same fix shape as
    /// `ble.rs`'s `candidate_key_for`.
    fn candidate_key_for(real_peer: &PeerId) -> PeerId {
        let short = crate::identity::peer_id::peer_short(&real_peer.0);
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&short);
        PeerId(id)
    }

    /// Remove a link + drop out of `Connected` when it was the last active link.
    /// The drop lands on `Degraded` (not `Available`) when the data scope is
    /// down, so a transport whose last link dies mid-outage never lies
    /// "Available" to the manager.
    async fn teardown_link(&self, adapter: &Arc<dyn WifiDirectAdapter>, handle: PeerHandle) {
        let removed = {
            let mut links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            let before = links.len();
            links.retain(|l| l.handle != handle);
            before != links.len()
        };
        if removed && self.links.lock().unwrap_or_else(|p| p.into_inner()).is_empty() {
            let next = if adapter.is_available() {
                TransportState::Available
            } else {
                TransportState::Degraded
            };
            self.set_state(next);
        }
    }

    /// One-shot bring-up: register DNS-SD advertisement + spawn the poller + the
    /// availability watcher. Serialized against `shutdown()` via `connect_gate`.
    async fn ensure_started(&self) -> Result<(), TransportError> {
        let _gate = self.connect_gate.lock().await;
        if self.shutdown_flag.load(Ordering::SeqCst) {
            return Err(TransportError::ShuttingDown);
        }
        let adapter = self.adapter().await?;
        if let Err(e) = adapter.start().await {
            // FFI-13: a device with no Wi-Fi Direct hardware (or a platform
            // Channel that failed to initialize) used to report Ok(()) here
            // — Kotlin's initialize() returned null and SessionGate cached
            // it as a "successful" start. Now that it throws "not supported"
            // (IrisFfiError's Display: "not supported on this device"),
            // treat it as the permanent condition it is: TransportError::
            // HardwareUnavailable drives the transport straight to
            // Unavailable (see the caller), instead of Io — which every
            // OTHER caller in this file already treats as retryable and
            // would have kept this transport polling a subsystem that can
            // never come up.
            return Err(if e.contains("not supported") {
                // Permanent: this device cannot ever bring up Wi-Fi Direct.
                // Flip state here (not just return an error) so the manager
                // stops selecting/polling a transport that will never
                // recover, instead of leaving state to whatever it happened
                // to be before this call. Also latch the dedicated flag so
                // discover_peers/start_advertising/connect can fail fast on
                // their NEXT call instead of re-invoking adapter.start().
                self.set_state(TransportState::Unavailable);
                self.permanently_unsupported.store(true, Ordering::SeqCst);
                TransportError::HardwareUnavailable
            } else {
                // FFI-7: a revoked permission is not the permanent condition
                // "not supported" is — deliberately NOT latching
                // permanently_unsupported here, unlike the branch above.
                Self::classify_error("wifi_direct.start", e)
            });
        }
        // Generic bring-up (also reached from discover_peers, which has no
        // NodeAdvertisement to build a real beacon from): register with an
        // empty placeholder just to get the DNS-SD subsystem up. The real
        // beacon is registered by start_advertising's own start_dns_sd call
        // immediately after, once the node's actual identity is known.
        //
        // HW-13: only do this ONCE per transport lifetime, not on every
        // `ensure_started()` call (discover_peers calls this every single
        // discovery pass). The Android bridge's `dnsSdGate` registers via
        // `addLocalService` exactly once and then caches that registration
        // forever; every `start_dns_sd()` call after the first is instead a
        // request to REPLACE it. A second empty-placeholder call — the
        // second, third, ... discovery pass, all still hitting this
        // "generic bring-up" path — silently reverted the real beacon
        // `start_advertising` had already registered back to an empty one,
        // permanently undoing discoverability after the very first
        // discovery pass, with zero errors anywhere (`start_advertising`
        // itself only runs once at engine startup and never re-fires to
        // restore it).
        if !self.dns_sd_bootstrapped.swap(true, Ordering::SeqCst) {
            adapter
                .start_dns_sd(Vec::new())
                .await
                .map_err(|e| Self::classify_error("wifi_direct.dns_sd", e))?;
        }
        self.spawn_poller(adapter.clone()).await?;
        if self.state.load() == TransportState::Unavailable {
            let initial = if adapter.is_available() {
                TransportState::Available
            } else {
                TransportState::Degraded
            };
            self.set_state(initial);
        }
        self.spawn_avail_watcher(adapter).await;
        Ok(())
    }

    /// Spawn (once) the task that drains inbound group frames into the incoming
    /// channel (bounded per-tick budget; overflow buffers locally, no loss).
    async fn spawn_poller(
        &self,
        adapter: Arc<dyn WifiDirectAdapter>,
    ) -> Result<(), TransportError> {
        let mut poller = self.poller.lock().await;
        if poller.is_some() {
            return Ok(());
        }
        let Some(incoming_tx) = self
            .incoming_tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
        else {
            // shutdown already dropped the sender; skip spawning.
            return Ok(());
        };
        let transport_id = self.id.clone();
        let links = self.links.clone();
        let dropped_inbound = self.dropped_inbound.clone();
        let handle = tokio::spawn(async move {
            let mut backlog: std::collections::VecDeque<IncomingWifiDirectData> =
                std::collections::VecDeque::new();
            loop {
                let fast = !links.lock().unwrap_or_else(|p| p.into_inner()).is_empty();
                tokio::time::sleep(Duration::from_millis(if fast { 10 } else { IDLE_POLL_MS }))
                    .await;
                backlog.extend(adapter.incoming().await);
                for _ in 0..MAX_FRAMES_PER_TICK {
                    let Some(frame) = backlog.pop_front() else {
                        break;
                    };
                    let Some(len) = frame_payload_len(&frame.payload) else {
                        // RF-29: log malformed/oversized header drops.
                        tracing::warn!(
                            transport = %transport_id,
                            raw_len = frame.payload.len(),
                            "inbound Wi-Fi Direct frame: truncated/oversized header; dropped"
                        );
                        continue;
                    };
                    let start = 4; // u32 LE length prefix (internet frame header)
                    let end = start + len;
                    if len == 0 || end > frame.payload.len() {
                        // RF-29: log zero-length / frame-overrun drops.
                        tracing::warn!(
                            transport = %transport_id,
                            declared_len = len,
                            raw_len = frame.payload.len(),
                            "inbound Wi-Fi Direct frame: zero-length or overrun; dropped"
                        );
                        continue;
                    }
                    let payload = frame.payload[start..end].to_vec();
                    // Best-effort sender candidate from the TXT record; the IRIS
                    // envelope layer performs real verification (DEC-WD-0007).
                    //
                    // FFI-8: `sender: None` used to be silently indistinguishable
                    // from "verified as peer zero" — PeerId([0u8; 32]) is a
                    // specific, valid-looking id every unattributed frame
                    // collided on, not an "unknown sender" marker. The
                    // finding's fuller fix (making None a distinguishable
                    // state the engine treats as pending envelope-level
                    // identification, rather than either fabricating an id
                    // OR dropping the frame outright) needs a
                    // MessageEngine::process_incoming contract change this
                    // fix does not make — IncomingMessage.peer_id is
                    // required, not Option, and at least one existing
                    // integration test exercises delivery through a
                    // simulated adapter that never sets `sender` at all, so
                    // dropping here silently regressed real delivery paths
                    // rather than only closing the collision hole. What
                    // FFI-1/FFI-3 already fix is the PRIMARY cause of
                    // sender ever being None in practice on real hardware
                    // (the two handle namespaces `verifiedCache` needs to
                    // agree on to resolve a sender at all) — this substitution
                    // remains a last-resort fallback, not the common case.
                    let peer_id = frame.sender.unwrap_or(PeerId([0u8; 32]));
                    if incoming_tx
                        .send(IncomingMessage {
                            peer_id,
                            transport_id: transport_id.to_string(),
                            payload,
                            received_at: Instant::now(),
                        })
                        .is_err()
                    {
                        // RF-31: make the first drop observable via tracing so
                        // the counter has at least one reader path.
                        let n = dropped_inbound.fetch_add(1, Ordering::Relaxed) + 1;
                        if n == 1 {
                            tracing::warn!(
                                transport = %transport_id,
                                "first inbound frame dropped: no subscriber yet"
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

    /// Total inbound frames dropped because no subscriber existed at delivery time.
    pub fn dropped_inbound(&self) -> u64 {
        self.dropped_inbound.load(Ordering::Relaxed)
    }

    /// Spawn (once) the task that mirrors availability churn into transport
    /// state (band restriction / coex → `Degraded`; recovery → re-attach).
    async fn spawn_avail_watcher(&self, adapter: Arc<dyn WifiDirectAdapter>) {
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
                    if current >= TransportState::Available && current != TransportState::Degraded {
                        Self::store_state_with_event(
                            &id,
                            &state,
                            &state_tx,
                            TransportState::Degraded,
                        );
                    }
                } else if current == TransportState::Degraded {
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

    /// Discovery re-arm gate: re-arms the framework find window only when the
    /// app-level cadence has elapsed (AC-5, G-WD-7).
    fn discovery_should_rearm(&self) -> bool {
        let mut slot = self.discovery_until.lock().unwrap_or_else(|p| p.into_inner());
        match *slot {
            None => true,
            Some(until) if Instant::now() >= until => {
                *slot = Some(Instant::now() + DISCOVERY_WINDOW);
                true
            }
            Some(_) => false,
        }
    }

    fn clear_discovery(&self) {
        *self.discovery_until.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
}

/// Classify an adapter `p2p_send` error as terminal link loss. The FFI contract
/// pins the markers for a dead path; anything else is transient congestion and
/// must NOT tear the link down (reconnect-storm avoidance).
fn is_link_loss_error(e: &str) -> bool {
    let e = e.to_ascii_lowercase();
    e.contains("not in group")
        || e.contains("closed")
        || e.contains("disconnect")
        || e.contains("reset")
        || e.contains("link lost")
}

#[async_trait]
impl Transport for WifiDirectTransport {
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
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "wifi_direct.state")
    }

    async fn discover_peers(
        &self,
        config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        // FFI-13: once ensure_started() has permanently marked this device as
        // lacking Wi-Fi Direct hardware, don't re-attempt adapter.start() on
        // every discovery call — fail fast instead of repeatedly hitting a
        // subsystem that can never come up. (Can't key this off
        // TransportState::Unavailable alone — that's also the pre-start
        // default, so checking it here would block the very first call.)
        if self.permanently_unsupported.load(Ordering::SeqCst) {
            return Err(TransportError::HardwareUnavailable);
        }
        self.ensure_started().await?;
        let adapter = self.adapter().await?;
        if self.discovery_should_rearm() {
            adapter
                .start_discovery()
                .await
                .map_err(|e| Self::classify_error("wifi_direct.find", e))?;
        }
        let matches = adapter.matches().await;
        let mut infos: Vec<PeerInfo> = Vec::new();
        // RF-33: take(max_peers) must come AFTER all per-peer filters (TXT parse,
        // config.filter id check) so that filtered-out entries don't consume budget.
        // .filter(service_name) stays first so foreign DNS-SD services never enter.
        for m in matches
            .iter()
            .filter(|m| m.service_name == WIFI_DIRECT_SERVICE_NAME)
        {
            if infos.len() >= config.max_peers {
                break;
            }
            let Ok(txt) = WifiDirectTxtRecord::parse(&m.txt_record) else {
                continue;
            };
            let peer_id = txt.candidate_peer_id();
            if let Some(filter) = config.filter.as_ref() {
                if !filter.contains(&peer_id) {
                    continue;
                }
            }
            // HW-18: `adapter.matches()` can legitimately hand back more than
            // one entry for the same candidate peer_id in a single pass —
            // the DNS-SD service listener and the TXT record listener
            // (kept as a secondary/redundant path since HW-16 made the TXT
            // listener publish directly) can each independently register a
            // match for the same beacon. Without a dedup, `connect()` was
            // called twice per peer per scan_once() (visible live as two
            // `discovery.connect_failed` for the identical peer ~4s apart)
            // — and on hardware where connect() triggers the OS's STA-vs-P2P
            // radio-conflict prompt (confirmed via AOSP's WifiP2pService
            // dialog strings: `wifi_p2p_dialog_title`/`wifi_p2p_turnon_message`,
            // shown when a P2P connect is attempted while STA is bound to an
            // AP — the exact situation here, all three test devices joined
            // to a home AP), each duplicate connect() attempt re-triggers
            // that OS prompt, which is what made it reappear every few
            // seconds instead of once per real retry interval.
            if infos.iter().any(|existing| existing.peer_id == peer_id) {
                continue;
            }
            infos.push(PeerInfo {
                peer_id,
                addresses: Vec::new(),
                transport_addresses: vec![("wifi-direct".to_string(), m.peer_handle.0.to_string())],
                last_seen: Some(Instant::now()),
            });
        }
        Ok(Box::pin(stream::iter(infos)))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        let adapter = self.adapter().await?;
        adapter
            .stop_discovery()
            .await
            .map_err(|e| Self::classify_error("wifi_direct.stop_find", e))?;
        self.clear_discovery();
        Ok(())
    }

    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError> {
        // FFI-13: see discover_peers — fail fast once permanently unsupported.
        if self.permanently_unsupported.load(Ordering::SeqCst) {
            return Err(TransportError::HardwareUnavailable);
        }
        self.ensure_started().await?;
        let adapter = self.adapter().await?;
        if !adapter.is_available() {
            // MG-41: the adapter exists (we're holding it) but currently
            // reports itself unable to operate — closer to "radio off" than
            // to "no hardware at all".
            return Err(TransportError::RadioDisabled);
        }
        // FFI-5: build the real 22-byte TXT-record beacon from the node's
        // actual identity — same construction BLE/Wi-Fi Aware's
        // start_advertising already do — instead of calling start_dns_sd()
        // with nothing at all for Kotlin to advertise.
        let mut caps = WifiDirectCapBits::empty();
        caps.set(WifiDirectCapBits::CLIENT_CAPABLE);
        if self.group_config.go_intent >= GO_INTENT_BALANCED {
            caps.set(WifiDirectCapBits::GROUP_OWNER_CAPABLE);
        }
        let peer_short = crate::identity::peer_id::peer_short(&info.peer_id.0);
        let txt_record = WifiDirectTxtRecord::build(
            caps,
            peer_short,
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                / 60) as u16,
        );
        adapter
            .start_dns_sd(txt_record)
            .await
            .map_err(|e| Self::classify_error("wifi_direct.dns_sd", e))?;
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        let adapter = self.adapter().await?;
        adapter
            .stop_dns_sd()
            .await
            .map_err(|e| Self::classify_error("wifi_direct.dns_sd", e))?;
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        if self.state.load() == TransportState::Unavailable {
            // MG-41: `TransportState::Unavailable` itself doesn't yet
            // distinguish hardware-absent from permission/radio-off (that
            // split is out of this fix's scope — see error.rs's module
            // doc); `HardwareUnavailable` is the closest available meaning
            // for "not usable" without finer-grained state to draw on.
            return Err(TransportError::HardwareUnavailable);
        }
        let adapter = self.adapter().await?;
        let handle = peer
            .transport_addresses
            .iter()
            .find(|(k, _)| k == "wifi-direct")
            .and_then(|(_, v)| v.parse::<u64>().ok())
            .ok_or(TransportError::PeerNotFound)?;
        // Serialize the open-link critical section: no double-open for one peer,
        // no exceeding MAX_GO_CLIENTS, no link registered during shutdown.
        let _gate = self.connect_gate.lock().await;
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::ShuttingDown);
        }
        if !adapter.is_available() {
            // MG-41: the adapter exists (we're holding it) but currently
            // reports itself unable to operate — closer to "radio off" than
            // to "no hardware at all".
            return Err(TransportError::RadioDisabled);
        }
        {
            let links = self.links.lock().unwrap_or_else(|p| p.into_inner());
            if links.iter().any(|l| l.peer_id == peer.peer_id) {
                // RT-004: a reused link re-promotes the transport to Connected
                // so a pull-only adapter whose state drifted (no availability
                // stream) can never be stuck below Connected over a live link.
                self.set_state(TransportState::Connected);
                return Ok(TransportLink {
                    peer_id: peer.peer_id,
                    transport_id: self.id.to_string(),
                    established_at: Instant::now(),
                });
            }
            if links.len() >= MAX_GO_CLIENTS {
                return Err(TransportError::Busy);
            }
        }
        // HW-20: the in-memory reuse check above only catches a link THIS
        // process already negotiated. It misses the case confirmed live —
        // researched against WifiP2pManager's own documented `connect()`
        // contract ("if the current device is part of an existing P2P
        // group... an invitation is sent" rather than fresh negotiation) —
        // where the OS already formed and holds a live group with this
        // exact peer (e.g. this node lost a prior GO/GC race and was made
        // the client by the platform's own negotiation), but this
        // process's `self.links` was never populated because ITS OWN
        // `create_group()`/`join_group()` call never got to return `Ok`.
        // Every subsequent scan pass then re-attempted group formation from
        // scratch against a peer the OS considers already connected —
        // observed live as a permanent `connect_failed` loop on the client
        // side of a group that `dumpsys wifip2p` showed as genuinely
        // `groupFormed: true`/`CONNECTED`. Consulting the adapter's live
        // `group_info()` before negotiating and short-circuiting when this
        // peer is already the GO or a listed client closes that gap without
        // touching the negotiation path at all for the genuinely-new-peer
        // case.
        if let Some(info) = adapter.group_info().await {
            let already_grouped =
                info.go.0 == handle || info.clients.iter().any(|c| c.0 == handle);
            if already_grouped {
                self.links.lock().unwrap_or_else(|p| p.into_inner()).push(WifiDirectLink {
                    handle: PeerHandle(handle),
                    peer_id: peer.peer_id,
                });
                self.set_state(TransportState::Connected);
                return Ok(TransportLink {
                    peer_id: peer.peer_id,
                    transport_id: self.id.to_string(),
                    established_at: Instant::now(),
                });
            }
        }
        // Group formation: GO-biased config forms/keeps a persistent GO and
        // invites the peer (p2p_invite); otherwise join the peer's group (GC).
        if self.group_config.go_intent >= GO_INTENT_BALANCED {
            // Try the requested band; band-constrained GO creation fails → AUTO
            // fallback (AC-3, DEC-WD-0005).
            let mut cfg = self.group_config.clone();
            let go_result = match tokio::time::timeout(
                CONNECT_TIMEOUT,
                adapter.create_group(&cfg),
            )
            .await
            .map_err(|_| "group creation timed out".to_string())
            .and_then(|r| r)
            {
                Ok(g) => Ok(g),
                Err(e) if e.contains("band") => {
                    cfg.band = OperatingBand::Auto;
                    tokio::time::timeout(CONNECT_TIMEOUT, adapter.create_group(&cfg))
                        .await
                        .map_err(|_| "group creation timed out".to_string())
                        .and_then(|r| r)
                }
                Err(e) => Err(e),
            };
            // FFI-14: this used to discard go_result's GroupInfo entirely
            // (`let _ = ...`) — Kotlin's degradedGroupInfo() fallback (now
            // removed) meant a synthetic groupId=0/empty-clients result
            // looked identical to a real group here. Now that Kotlin throws
            // instead of fabricating, group_id == 0 can't legitimately
            // happen — checked anyway as defense-in-depth, since this is the
            // only evidence standing between a real group and the Connected
            // state this function is about to claim.
            let group = go_result.map_err(|_| TransportError::ConnectionFailed)?;
            if group.group_id == 0 {
                return Err(TransportError::ConnectionFailed);
            }
            let _ = tokio::time::timeout(CONNECT_TIMEOUT, adapter.add_client(PeerHandle(handle)))
                .await
                .map_err(|_| TransportError::ConnectionFailed)?
                .map_err(|_| TransportError::ConnectionFailed)?;
        } else {
            // FFI-14: same discard as the GO branch above, same fix.
            let group = tokio::time::timeout(
                CONNECT_TIMEOUT,
                adapter.join_group(PeerHandle(handle), &self.group_config),
            )
            .await
            .map_err(|_| TransportError::ConnectionFailed)?
            .map_err(|_| TransportError::ConnectionFailed)?;
            if group.group_id == 0 {
                return Err(TransportError::ConnectionFailed);
            }
        }
        // Defense-in-depth: re-check availability/state after group formation.
        if self.state.load() == TransportState::Unavailable || !adapter.is_available() {
            return Err(TransportError::ShuttingDown);
        }
        self.links.lock().unwrap_or_else(|p| p.into_inner()).push(WifiDirectLink {
            handle: PeerHandle(handle),
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
        if message.payload.len() > MAX_WIFI_DIRECT_MESSAGE_BYTES {
            // MG-40: resolvable by fragmenting upstream and retrying on
            // this same transport — not a framing/decode failure.
            return Err(TransportError::MessageTooLarge {
                limit: MAX_WIFI_DIRECT_MESSAGE_BYTES,
                actual: message.payload.len(),
            });
        }
        if message.payload.is_empty() {
            // RT-007: a zero-length frame is dropped inbound (INTERNET-001
            // framing `len == 0` guard); accepting then silently losing it is
            // worse than a clean protocol error (receive-path symmetry).
            return Err(TransportError::Protocol("empty payload".into()));
        }
        let adapter = self.adapter().await?;
        if !adapter.is_available() {
            // RT-005: data scope is down — the group links are unusable. Drop
            // them so the transport can never sit "Connected" over a dead radio
            // (the last-link teardown lands on Degraded, never a lazy
            // "Available", keeping pull-only adapters honest).
            let handles: Vec<PeerHandle> = self
                .links
                .lock()
                .unwrap()
                .iter()
                .map(|l| l.handle)
                .collect();
            for h in handles {
                self.teardown_link(&adapter, h).await;
            }
            return Err(TransportError::RadioDisabled);
        }
        if self.state.load() == TransportState::Unavailable {
            return Err(TransportError::HardwareUnavailable);
        }
        // HW-17: try the exact real key first (the common case once/if a
        // verified real-identity mapping exists), falling back to the
        // discovery-derived candidate key, which is what's actually in
        // `links` today — see `candidate_key_for`'s doc comment.
        let handle = {
            let links = self.links.lock().unwrap();
            links
                .iter()
                .find(|l| l.peer_id == *peer)
                .or_else(|| {
                    let candidate = Self::candidate_key_for(peer);
                    links.iter().find(|l| l.peer_id == candidate)
                })
                .map(|l| l.handle)
                .ok_or(TransportError::NotConnected)?
        };
        let frame = encode_frame(message)?;
        let send_result = tokio::time::timeout(SEND_TIMEOUT, adapter.p2p_send(handle, &frame))
            .await
            .unwrap_or(Err("p2p_send timed out".to_string()));
        match send_result {
            Ok(()) => {}
            Err(e) => {
                if is_link_loss_error(&e) {
                    self.teardown_link(&adapter, handle).await;
                    return Err(TransportError::NotConnected);
                }
                return Err(TransportError::Busy);
            }
        }
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: frame.len(),
            sent_at: Instant::now(),
        })
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        // RF-30: subscribe only if the sender is still alive (not taken by shutdown).
        if let Some(tx) = self.incoming_tx.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            crate::transport::broadcast_stream(tx.subscribe(), "wifi_direct.messages")
        } else {
            Box::pin(futures_util::stream::empty())
        }
    }

    fn cost_snapshot(&self) -> TransportCost {
        let connected = self.state.load() == TransportState::Connected;
        TransportCost {
            estimated_battery_ma: if connected {
                WIFI_DIRECT_COST.connected_idle_ma
            } else {
                WIFI_DIRECT_COST.scan_ma
            },
            monetary_cost_per_kb: 0.0,
            bandwidth_available_bps: 10_000_000,
            congestion_level: 0.0,
        }
    }

    fn set_send_priority_hint(&self, _priority: MessagePriority) {
        // Scaffold: no duty-cycle / power tuning yet (WIFI_DIRECT.md §Battery).
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        let _gate = self.connect_gate.lock().await;
        if let Some(handle) = self.poller.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.avail_watcher.lock().await.take() {
            handle.abort();
        }
        if let Ok(adapter) = self.adapter().await {
            adapter.remove_group().await.ok();
            adapter.shutdown().await.ok();
        }
        self.links.lock().unwrap_or_else(|p| p.into_inner()).clear();
        self.clear_discovery();
        // RF-30: drop the sender so all `incoming_messages()` streams see `Closed`
        // and terminate rather than hanging forever.
        self.incoming_tx.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.shutdown_flag.store(true, Ordering::SeqCst);
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

    fn sample_message(payload: &[u8]) -> SerializedMessage {
        SerializedMessage {
            message_id: crate::protocol::MessageId::from([0x21; 16]),
            priority: MessagePriority::P2,
            payload: payload.to_vec(),
        }
    }

    async fn advertise(t: &WifiDirectTransport) {
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([1u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: vec![],
        })
        .await
        .expect("advertise ok");
    }

    /// WD-AC-4 — GO/GC TCP E2E round-trip: A (GO) connects to B and delivers a
    /// payload to B's incoming stream over the in-memory group (TCP-over-GO
    /// model). The frame must be attributed to A's TXT-record candidate PeerId,
    /// never the zero fallback (DEC-WD-0007).
    #[tokio::test]
    async fn group_roundtrip_delivers_payload() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::with_group_config(
            Some(a.clone()),
            GroupConfig {
                go_intent: 14, // fixed infra → GO
                ..GroupConfig::default()
            },
        );
        let tb = WifiDirectTransport::with_group_config(
            Some(b.clone()),
            GroupConfig {
                go_intent: 0, // client
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
        advertise(&tb).await;
        let discovered: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(discovered.len(), 1, "A sees B's DNS-SD advertisement");
        let peer_b = discovered[0].peer_id;
        let link = ta.connect(&discovered[0]).await.unwrap();
        assert_eq!(link.peer_id, peer_b);
        assert_eq!(ta.state(), TransportState::Connected);

        // B's view of A — the candidate PeerId inbound frames must be
        // attributed to (candidate-only identity; real verification later).
        let peer_a_from_b: Vec<PeerInfo> = tb
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peer_a_from_b.len(), 1, "B sees A's DNS-SD advertisement");
        let peer_a = peer_a_from_b[0].peer_id;

        // B joins A's group as a client.
        let bl = tb.connect(&peer_a_from_b[0]).await.unwrap();
        assert_eq!(bl.peer_id, peer_a);

        let mut rx = tb.incoming_messages();
        let msg = sample_message(b"p2p-hello");
        ta.send(&peer_b, &msg).await.unwrap();

        let got = tokio::time::timeout(Duration::from_secs(3), rx.next())
            .await
            .expect("frame delivered in time")
            .expect("incoming stream alive");
        assert_eq!(got.payload, b"p2p-hello".to_vec());
        assert_eq!(got.transport_id, "wifi-direct-0");
        assert_ne!(
            got.peer_id,
            PeerId([0u8; 32]),
            "sender attributed, not zero"
        );
        assert_eq!(got.peer_id, peer_a, "frame attributed to the real sender");
    }

    /// WD-AC-2 — DNS-SD discovery: a subscribed transport matches every
    /// advertising peer carrying the IRIS p2p service name and maps each to a
    /// candidate PeerId from the TXT-record short id.
    #[tokio::test]
    async fn discovery_finds_advertising_peer() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::new(Some(a));
        let tb = WifiDirectTransport::new(Some(b));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peers.len(), 1);
        assert!(!peers[0].peer_id.0.is_empty());
        assert_eq!(peers[0].transport_addresses[0].0, "wifi-direct");
    }

    /// WD-AC-3 — band-restriction degradation: GO creation with a 5 GHz band
    /// fails while band-restricted; the transport falls back to AUTO (AC-3,
    /// DEC-WD-0005). A wholly unavailable transport reports no peers and
    /// never leaves Unavailable.
    #[tokio::test]
    async fn band_restricted_go_creation_falls_back() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        a.set_band_restricted(true);
        let ta = WifiDirectTransport::with_group_config(
            Some(a.clone()),
            GroupConfig {
                go_intent: 14,
                band: OperatingBand::Ghz5,
                ..GroupConfig::default()
            },
        );
        let tb = WifiDirectTransport::with_group_config(
            Some(b.clone()),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
        advertise(&tb).await;
        let discovered: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert!(!discovered.is_empty());
        // 5 GHz restricted → transport falls back to AUTO → group forms.
        assert!(ta.connect(&discovered[0]).await.is_ok());
        assert_eq!(ta.state(), TransportState::Connected);

        // Unavailable adapter: no peers ever surface; connect is NotSupported.
        a.set_available(false);
        assert!(ta.connect(&discovered[0]).await.is_err());
    }

    /// WD-AC-5 — discovery re-arm + stop: a find-stop clears the window and the
    /// transport returns cleanly; re-discovery re-arms (30-s app cadence,
    /// G-WD-7).
    #[tokio::test]
    async fn discovery_rearm_and_stop() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::new(Some(a.clone()));
        let tb = WifiDirectTransport::new(Some(b.clone()));

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peers.len(), 1);
        // Stop the find window (WIFI_P2P_DISCOVERY_CHANGED_ACTION stop event).
        ta.stop_discovery().await.unwrap();
        assert!(ta.discovery_until.lock().unwrap_or_else(|p| p.into_inner()).is_none());
        // Re-arm works after stop.
        let peers2: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peers2.len(), 1);
        assert_eq!(ta.state(), TransportState::Available);
    }

    /// WD-AC-8 — lifecycle: shutdown aborts the poller, removes the group, and
    /// returns the transport to Unavailable so the manager never selects it.
    #[tokio::test]
    async fn shutdown_returns_to_unavailable() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::with_group_config(
            Some(a),
            GroupConfig {
                go_intent: 14,
                ..GroupConfig::default()
            },
        );
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert!(!peers.is_empty(), "discovery works before shutdown");
        assert!(ta.connect(&peers[0]).await.is_ok());
        assert_eq!(ta.state(), TransportState::Connected);
        ta.shutdown().await.unwrap();
        assert_eq!(ta.state(), TransportState::Unavailable);
        assert!(ta
            .send(&peers[0].peer_id, &sample_message(b"x"))
            .await
            .is_err());
    }

    /// WD-AC-9 — bounded connection table + per-peer single link: repeated
    /// connects to the same peer reuse one link; the table never exceeds
    /// MAX_GO_CLIENTS.
    #[tokio::test]
    async fn single_link_per_peer_and_bounded_table() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = Arc::new(WifiDirectTransport::with_group_config(
            Some(a),
            GroupConfig {
                go_intent: 14,
                ..GroupConfig::default()
            },
        ));
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();

        // Hammer connect for the same peer concurrently; exactly one link.
        let mut handles = Vec::new();
        for _ in 0..8 {
            let t = ta.clone();
            let p = peer.clone();
            handles.push(tokio::spawn(async move { t.connect(&p).await }));
        }
        let mut results = Vec::new();
        for h in handles {
            results.push(h.await.unwrap());
        }
        assert!(results.iter().all(|r| r.is_ok()));
        assert_eq!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).len(), 1, "single link per peer");
    }

    /// WD-AC-1 — registration: an Unavailable transport is never selected by the
    /// manager; it becomes selectable after discovery starts (AC-1).
    #[tokio::test]
    async fn registration_gating() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let adapter: Arc<dyn WifiDirectAdapter> = Arc::new(SimulatedWifiDirectAdapter::new(coord));
        let t = WifiDirectTransport::new(Some(adapter));
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

    /// RT-001 — the registered advertisement is always attributable: the tag
    /// and its TXT record land atomically, and the advertised short id is never
    /// zero (an all-zero record is rejected at parse, see
    /// `wifi_direct_serv::tests::zero_peer_short_rejected`).
    #[tokio::test]
    async fn advertised_txt_is_attributable() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::new(Some(a.clone()));
        advertise(&ta).await;

        let tag = *a.tag.lock().unwrap_or_else(|p| p.into_inner());
        assert_ne!(tag, 0, "registered");
        let txt = coord.txt_of(tag).expect("advertisement present");
        let parsed = WifiDirectTxtRecord::parse(&txt).expect("advertised TXT parses");
        assert_ne!(parsed.peer_short, [0u8; 16], "short id never all-zero");
    }

    /// RT-002 — a full outbox evicts the oldest frame for the *same*
    /// destination only; frames staged for other peers are never evicted.
    #[test]
    fn outbox_eviction_keeps_other_destinations() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let cap = SimP2pCoordinator::MAX_OUTBOX_FRAMES;
        for _ in 0..(cap / 2) {
            coord.proxy_send(9, 2, b"a");
            coord.proxy_send(9, 4, b"b");
        }
        assert_eq!(coord.outbox.lock().unwrap_or_else(|p| p.into_inner()).len(), cap);
        // One more frame for dest 2 → evicts the oldest dest-2 frame only.
        coord.proxy_send(9, 2, b"c");
        let outbox = coord.outbox.lock().unwrap_or_else(|p| p.into_inner());
        assert_eq!(outbox.len(), cap, "still bounded");
        let to2 = outbox.iter().filter(|(to, _, _)| *to == 2).count();
        let to4 = outbox.iter().filter(|(to, _, _)| *to == 4).count();
        assert_eq!(to2, cap / 2, "dest-2 stays bounded at capacity");
        assert_eq!(to4, cap / 2, "dest-4 frames never evicted");
        assert!(
            outbox.iter().any(|(_, _, p)| p == b"c"),
            "new frame survived"
        );
    }

    /// RT-003/RT-012 — a client can only join a real group owner; joining a
    /// non-GO peer is rejected and never manufactures a phantom group.
    #[tokio::test]
    async fn join_non_go_peer_is_rejected() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::with_group_config(
            Some(a),
            GroupConfig {
                go_intent: 0, // client-only: never forms a group
                ..GroupConfig::default()
            },
        );
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
        advertise(&tb).await;
        let peers: Vec<PeerInfo> = tb
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(peers.len(), 1, "A is discoverable");
        assert!(matches!(
            tb.connect(&peers[0]).await,
            Err(TransportError::ConnectionFailed)
        ));
        assert_eq!(tb.state(), TransportState::Available, "no phantom group");
        assert_eq!(
            coord.groups.lock().unwrap_or_else(|p| p.into_inner()).len(),
            0,
            "no group manufactured"
        );
    }

    /// RT-004 — re-connect reuses the existing link and re-promotes Connected
    /// after availability churn (band restriction → recovery).
    #[tokio::test]
    async fn connect_reuse_after_churn_recovers() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = Arc::new(WifiDirectTransport::with_group_config(
            Some(a.clone()),
            GroupConfig {
                go_intent: 14,
                ..GroupConfig::default()
            },
        ));
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
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

        // Churn: radio down (band restriction / coex) → Degraded.
        a.set_available(false);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(ta.state(), TransportState::Degraded);

        // Recovery: a re-connect reuses the single link and the transport
        // ends Connected (watcher + reuse path both converge).
        a.set_available(true);
        ta.connect(&peer).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(ta.state(), TransportState::Connected);
        assert_eq!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).len(), 1, "single link reused");
    }

    /// RT-005 — sending on an unavailable adapter refuses and tears the group
    /// links down (last-link teardown lands on Degraded, never "Available").
    #[tokio::test]
    async fn send_on_unavailable_tears_down_links() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = Arc::new(WifiDirectTransport::with_group_config(
            Some(a.clone()),
            GroupConfig {
                go_intent: 14,
                ..GroupConfig::default()
            },
        ));
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        let peer = peers[0].clone();
        ta.connect(&peer).await.unwrap();
        assert_eq!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).len(), 1);

        a.set_available(false);
        assert!(matches!(
            ta.send(&peer.peer_id, &sample_message(b"x")).await,
            Err(TransportError::RadioDisabled)
        ));
        assert!(ta.links.lock().unwrap_or_else(|p| p.into_inner()).is_empty(), "links torn down");
        assert_eq!(ta.state(), TransportState::Degraded);
    }

    /// RT-007 — a zero-length payload is rejected cleanly, never accepted then
    /// silently dropped by the receive path.
    #[tokio::test]
    async fn empty_payload_send_rejected() {
        let coord = Arc::new(SimP2pCoordinator::new());
        let a = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let b = Arc::new(SimulatedWifiDirectAdapter::new(coord.clone()));
        let ta = WifiDirectTransport::with_group_config(
            Some(a),
            GroupConfig {
                go_intent: 14,
                ..GroupConfig::default()
            },
        );
        let tb = WifiDirectTransport::with_group_config(
            Some(b),
            GroupConfig {
                go_intent: 0,
                ..GroupConfig::default()
            },
        );

        advertise(&ta).await;
        advertise(&tb).await;
        let peers: Vec<PeerInfo> = ta
            .discover_peers(DiscoveryConfig::default())
            .await
            .unwrap()
            .collect()
            .await;
        ta.connect(&peers[0]).await.unwrap();
        assert!(matches!(
            ta.send(&peers[0].peer_id, &sample_message(b"")).await,
            Err(TransportError::Protocol(_))
        ));
    }
}
