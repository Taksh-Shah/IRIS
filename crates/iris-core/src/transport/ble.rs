//! BLE-001 — BLE Transport scaffold.
//!
//! Implements the platform-adapter architecture from
//! `docs/transports/TRANSPORT_ABSTRACTION.md` §Platform Adapter Architecture:
//!
//! ```text
//! BleTransport (Rust core)  --calls-->  BleAdapter trait
//!                                          ├─ AndroidBleAdapter (Kotlin/JNI)
//!                                          └─ IosBleAdapter     (Swift/FFI)
//! ```
//!
//! The Rust core depends only on the `BleAdapter` trait object. Real Android/iOS
//! adapters are injected at runtime via `uniffi`/FFI (BLK-0005). A
//! [`SimulatedBleAdapter`] is provided for in-memory tests — no hardware needed.
//!
//! Status: SCAFFOLD. Real device connect/send/receive is gated on hardware
//! (BLK-0005) and the Android platform crate.

use std::fmt;
use std::pin::Pin;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;
use tokio::sync::broadcast;

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::ble_advert::{CapabilityBits, DiscoveryBeacon};
use crate::transport::ble_att::{AttSegmenter, Reassembler, MAX_MESSAGE_BYTES, REASSEMBLY_TTL};
use crate::transport::{
    AtomicState, Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportState, TransportStateEvent,
};
use crate::TransportError;

// ---------------------------------------------------------------------------
// BLE domain types (from TRANSPORT_ABSTRACTION.md §BleAdapter)
// ---------------------------------------------------------------------------

/// 128-bit BLE UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Uuid(pub [u8; 16]);

impl Uuid {
    pub fn from_u128(v: u128) -> Self {
        Uuid(v.to_le_bytes())
    }
}

/// Bluetooth device address (48-bit MAC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BleAddress(pub [u8; 6]);

/// Scan filter.
#[derive(Debug, Clone, Default)]
pub struct ScanFilter {
    pub service_uuids: Vec<Uuid>,
    pub rssi_threshold: Option<i32>,
}

/// Advertisement payload builder input.
#[derive(Debug, Clone, Default)]
pub struct AdvertisementData {
    pub local_name: Option<String>,
    pub service_uuid: Option<Uuid>,
    pub service_data: Vec<u8>,
}

/// Opaque handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GattHandle(pub u64);

/// An inbound GATT write from a peer.
#[derive(Debug, Clone)]
pub struct GattWriteEvent {
    pub handle: GattHandle,
    pub char_uuid: Uuid,
    pub data: Vec<u8>,
}

/// BLE adapter error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BleError {
    NotSupported,
    PermissionDenied,
    AdapterOff,
    DeviceNotFound,
    GattFailure(String),
}

impl fmt::Display for BleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BleError::NotSupported => write!(f, "ble: not supported"),
            BleError::PermissionDenied => write!(f, "ble: permission denied"),
            BleError::AdapterOff => write!(f, "ble: adapter off"),
            BleError::DeviceNotFound => write!(f, "ble: device not found"),
            BleError::GattFailure(m) => write!(f, "ble: gatt failure: {m}"),
        }
    }
}

impl std::error::Error for BleError {}

/// A BLE advertisement received by a scanner.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub address: BleAddress,
    /// Raw advertisement payload (the IRIS discovery beacon).
    pub payload: Vec<u8>,
    pub rssi: i32,
}

/// Platform-neutral BLE adapter trait (mirrors the spec; real adapters via FFI).
pub trait BleAdapter: Send + Sync + 'static {
    fn start_scan(&self, filter: ScanFilter) -> Result<ScanHandle, BleError>;
    fn stop_scan(&self, handle: ScanHandle);
    fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError>;
    fn stop_advertising(&self, handle: AdvHandle);
    fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError>;
    fn disconnect_gatt(&self, handle: GattHandle);
    fn gatt_write(
        &self,
        handle: GattHandle,
        char_uuid: Uuid,
        data: Vec<u8>,
    ) -> Result<(), BleError>;
    fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError>;
    fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>>;
    /// Scan results delivered since last drain (real impl: callbacks).
    fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>>;
}

/// In-memory BLE adapter for tests and simulation. No hardware involved.
#[derive(Default)]
pub struct SimulatedBleAdapter {
    events: std::sync::Mutex<Vec<GattWriteEvent>>,
    scan_results: std::sync::Mutex<Vec<ScanResult>>,
    writes: std::sync::Mutex<Vec<(GattWriteEvent, GattHandle)>>,
    connect_calls: std::sync::Mutex<usize>,
    next_handle: std::sync::atomic::AtomicU64,
    handles: std::sync::Mutex<Vec<GattHandle>>,
}

impl SimulatedBleAdapter {
    pub fn new() -> Self {
        Self::default()
    }
    /// Inject an inbound GATT write (simulates a peer writing to our server).
    pub fn inject_write(&self, handle: GattHandle, char_uuid: Uuid, data: Vec<u8>) {
        self.events.lock().unwrap().push(GattWriteEvent {
            handle,
            char_uuid,
            data,
        });
    }
    /// Inject a discovered advertisement (simulates a peer advertising near us).
    pub fn inject_scan_result(&self, address: BleAddress, payload: Vec<u8>, rssi: i32) {
        self.scan_results.lock().unwrap().push(ScanResult {
            address,
            payload,
            rssi,
        });
    }
    /// Last GATT write received (for asserting outbound segmentation).
    pub fn last_writes(&self) -> std::sync::MutexGuard<'_, Vec<(GattWriteEvent, GattHandle)>> {
        self.writes.lock().unwrap()
    }
    /// Number of `connect_gatt` calls (AC-9: must be 1 across reconnects).
    pub fn connect_count(&self) -> usize {
        *self.connect_calls.lock().unwrap()
    }
    /// Handles assigned to live connections (for multi-peer tests).
    pub fn assigned_handles(&self) -> std::sync::MutexGuard<'_, Vec<GattHandle>> {
        self.handles.lock().unwrap()
    }
}

impl BleAdapter for SimulatedBleAdapter {
    fn start_scan(&self, _filter: ScanFilter) -> Result<ScanHandle, BleError> {
        Ok(ScanHandle(1))
    }
    fn stop_scan(&self, _handle: ScanHandle) {}
    fn start_advertising(&self, _data: AdvertisementData) -> Result<AdvHandle, BleError> {
        Ok(AdvHandle(1))
    }
    fn stop_advertising(&self, _handle: AdvHandle) {}
    fn connect_gatt(&self, _address: BleAddress) -> Result<GattHandle, BleError> {
        *self.connect_calls.lock().unwrap() += 1;
        // Distinct handle per connection so multi-peer tests can attribute
        // inbound writes to the right link (BLE-RT-016).
        let h = GattHandle(
            self.next_handle
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1,
        );
        self.handles.lock().unwrap().push(h);
        Ok(h)
    }
    fn disconnect_gatt(&self, _handle: GattHandle) {}
    fn gatt_write(
        &self,
        handle: GattHandle,
        char_uuid: Uuid,
        data: Vec<u8>,
    ) -> Result<(), BleError> {
        self.writes.lock().unwrap().push((
            GattWriteEvent {
                handle,
                char_uuid,
                data: data.clone(),
            },
            handle,
        ));
        Ok(())
    }
    fn set_mtu(&self, _handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
        Ok(mtu)
    }
    fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
        self.events.lock().unwrap()
    }
    fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
        self.scan_results.lock().unwrap()
    }
}

/// A BLE transport bound to a platform adapter.
pub struct BleTransport {
    id: TransportId,
    display: String,
    caps: TransportCapabilities,
    adapter: std::sync::Mutex<Option<std::sync::Arc<dyn BleAdapter>>>,
    state: AtomicState,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    scan_handle: std::sync::Mutex<Option<ScanHandle>>,
    adv_handle: std::sync::Mutex<Option<AdvHandle>>,
    /// GATT handles + negotiated ATT MTU per connected peer — one connection
    /// per peer is reused for all sends (AC-9; RES-0019 R6.4/G2 bounds
    /// GATT-client churn). MTU is per-link (BLE-RT-003), never global.
    connections: std::sync::Mutex<std::collections::HashMap<PeerId, (GattHandle, u16)>>,
    /// Abort handles for the per-peer inbound-poll tasks so shutdown() actually
    /// stops them (RED-0009-02, was an unowned infinite loop leaking a task).
    pollers: std::sync::RwLock<std::collections::HashMap<PeerId, tokio::task::AbortHandle>>,
    /// Live ATT MTU (negotiate-late; segmenter resizes on `on_mtu_changed`).
    segmenter: AttSegmenter,
    /// Scan restart backoff state (AC-5; >=5 start/stop per 30s ceiling,
    /// RES-0019 R6.2 + G4 Android-17 hardening).
    scan_times: std::sync::Mutex<std::collections::VecDeque<std::time::Instant>>,
    scan_backoff_s: std::sync::Mutex<u64>,
    refused_until: std::sync::RwLock<Option<std::time::Instant>>,
}

/// Scan-restart ceiling: max scan starts in the window.
const SCAN_CEILING: usize = 5;
/// Backoff window length (RES-0019: >=5 start/stop per 30 s → silent failure).
const SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
/// Backoff cap (Android 17 hardens throttling; exponential 1s..=30s).
const SCAN_BACKOFF_MAX_S: u64 = 30;

impl BleTransport {
    pub fn new(adapter: Option<std::sync::Arc<dyn BleAdapter>>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = AtomicState::default();
        state.store(TransportState::Available);
        let caps = TransportCapabilities {
            max_message_size: MAX_MESSAGE_BYTES, // segmented; wire cap u16::MAX (AC-2)
            supports_broadcast: true,
            supports_unicast: true,
            supports_multicast: false,
            range_m_min: 1,
            range_m_max: 100,
            range_m_typical: 10,
            typical_throughput_bps: 1_000_000,
            typical_latency_ms: 30,
            requires_infrastructure: false,
            supports_background_android: true,
            supports_background_ios: false, // iOS background BLE is severely limited
            requires_special_hardware: false,
            cost_class: TransportCostClass::Free,
            regulatory_band: None,
        };
        BleTransport {
            id: TransportId::from("ble-android"),
            display: "BLE (Android)".to_string(),
            caps,
            adapter: std::sync::Mutex::new(adapter),
            state,
            state_tx,
            incoming_tx,
            scan_handle: std::sync::Mutex::new(None),
            adv_handle: std::sync::Mutex::new(None),
            connections: std::sync::Mutex::new(std::collections::HashMap::new()),
            pollers: std::sync::RwLock::new(std::collections::HashMap::new()),
            segmenter: AttSegmenter::new(),
            scan_times: std::sync::Mutex::new(std::collections::VecDeque::new()),
            scan_backoff_s: std::sync::Mutex::new(0),
            refused_until: std::sync::RwLock::new(None),
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        let _ = self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    fn adapter(&self) -> Result<std::sync::Arc<dyn BleAdapter>, TransportError> {
        self.adapter
            .lock()
            .unwrap()
            .clone()
            .ok_or(TransportError::NotSupported)
    }

    /// Tear down a peer's GATT link (BLE-RT-005): remove the connection entry,
    /// abort its poller, and disconnect the GATT handle so a dead link cannot
    /// be re-selected by the manager. `send` calls this on write failure;
    /// `shutdown` keeps its own full-drain path.
    fn close_peer(
        &self,
        peer: &PeerId,
        adapter: std::sync::Arc<dyn BleAdapter>,
        _cause: Option<&BleError>,
    ) {
        if let Some((handle, _)) = self.connections.lock().unwrap().remove(peer) {
            adapter.disconnect_gatt(handle);
        }
        if let Some(p) = self.pollers.write().unwrap().remove(peer) {
            p.abort();
        }
        // Fall back to `Available` once a dead link is gone so the manager's
        // `state >= Available` selection filter doesn't keep routing into it.
        if self.connections.lock().unwrap().is_empty() {
            self.set_state(TransportState::Available);
        }
    }

    /// Throttle scan restarts (AC-5): a burst of >= `SCAN_CEILING` starts within
    /// `SCAN_WINDOW` is refused, and the refusal arms an exponential backoff
    /// (1s..=SCAN_BACKOFF_MAX_S) so a stuck restart loop does not hammer the
    /// stack (Android ≥14 silently fails; Android 17 hardens the throttle —
    /// RES-0019 R6.2/G4).
    fn scan_allowed(&self) -> bool {
        let now = std::time::Instant::now();
        let mut times = self.scan_times.lock().unwrap();
        while let Some(&t) = times.front() {
            if now.duration_since(t) > SCAN_WINDOW {
                times.pop_front();
            } else {
                break;
            }
        }
        // Honor an active backoff refusal window first.
        {
            let refused = self.refused_until.read().unwrap();
            if let Some(until) = *refused {
                if now < until {
                    return false;
                }
            }
        }
        if times.len() >= SCAN_CEILING {
            // Refuse and grow the backoff for future attempts.
            let mut backoff = self.scan_backoff_s.lock().unwrap();
            *backoff = if *backoff == 0 {
                1
            } else {
                (*backoff * 2).min(SCAN_BACKOFF_MAX_S)
            };
            let wait = *backoff;
            drop(backoff);
            let until = now + std::time::Duration::from_secs(wait);
            *self.refused_until.write().unwrap() = Some(until);
            return false;
        }
        times.push_back(now);
        true
    }
}

#[async_trait]
impl Transport for BleTransport {
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
        crate::transport::broadcast_stream(self.state_tx.subscribe())
    }

    async fn discover_peers(
        &self,
        config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        let adapter = self.adapter()?;
        if !self.scan_allowed() {
            return Err(TransportError::Protocol(
                "ble: scan throttled (start/stop burst within 30s)".to_string(),
            ));
        }
        let handle = adapter
            .start_scan(ScanFilter::default())
            .map_err(to_transport_err)?;
        *self.scan_handle.lock().unwrap() = Some(handle);
        // Drain scan results produced by the adapter (real impl: Android
        // BluetoothLeScanner callbacks; simulated: inject_scan_result). The
        // OS can batch-deliver hundreds of reports; cap at the caller's
        // max_peers so an advert flood can't blow past the requested budget
        // (BLE-RT-013).
        let results: Vec<ScanResult> = adapter.scan_results().drain(..).collect();
        let max_peers = config.max_peers;
        let peers: Vec<PeerInfo> = results
            .into_iter()
            .take(max_peers)
            .filter_map(|r| {
                let beacon = DiscoveryBeacon::parse(&r.payload).ok()?;
                let mut addresses = Vec::with_capacity(1);
                addresses.push((
                    "ble".to_string(),
                    format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        r.address.0[0],
                        r.address.0[1],
                        r.address.0[2],
                        r.address.0[3],
                        r.address.0[4],
                        r.address.0[5]
                    ),
                ));
                Some(PeerInfo {
                    peer_id: beacon.candidate_peer_id(),
                    addresses: Vec::new(),
                    transport_addresses: addresses,
                    last_seen: Some(std::time::Instant::now()),
                })
            })
            .collect();
        Ok(Box::pin(futures_util::stream::iter(peers)))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        if let Some(h) = *self.scan_handle.lock().unwrap() {
            self.adapter()?.stop_scan(h);
        }
        Ok(())
    }

    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError> {
        let adapter = self.adapter()?;
        // Re-announcement (e.g. after identity/key rotation) must stop any prior
        // advertisement first so Android's max ~4 advertising-set budget is not
        // leaked and the radio doesn't keep a stale set alive (BLE-RT-011).
        if let Some(prior) = *self.adv_handle.lock().unwrap() {
            adapter.stop_advertising(prior);
        }
        // Build the IRIS discovery beacon (22-byte, candidate-level, never a
        // trust boundary — DEC-BLE-0006).
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        caps.set(CapabilityBits::ADVERTISE_BROADCAST);
        if self.caps.supports_background_android {
            caps.set(CapabilityBits::EXTENDED_ADV);
        }
        let peer_short = crate::identity::peer_id::peer_short(&info.peer_id.0);
        let beacon = DiscoveryBeacon::build(
            caps,
            peer_short,
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                / 60) as u16,
        );
        let handle = adapter
            .start_advertising(AdvertisementData {
                local_name: info.hostname,
                service_uuid: Some(Uuid::from_u128(0x1)), // IRIS discovery UUID
                service_data: beacon,
            })
            .map_err(to_transport_err)?;
        *self.adv_handle.lock().unwrap() = Some(handle);
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        if let Some(h) = *self.adv_handle.lock().unwrap() {
            self.adapter()?.stop_advertising(h);
        }
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        let adapter = self.adapter()?;
        // Serialize per-peer connect setup under the connections lock so two
        // concurrent connects to the same peer cannot create a duplicate GATT
        // link or orphan a poller task (BLE-RT-002).
        let mut conns = self.connections.lock().unwrap();
        // Peer already connected: reuse the link (AC-9 — one GATT connection per
        // peer; GATT-client churn bounded, RES-0019 R6.4/G2).
        if let Some(&(handle, _)) = conns.get(&peer.peer_id) {
            let _ = handle;
            return Ok(TransportLink {
                peer_id: peer.peer_id,
                transport_id: self.id.0.clone(),
                established_at: Instant::now(),
            });
        }
        // Peer's BLE address comes from transport_addresses (["ble", "AA:BB:.."]).
        let mac = peer
            .transport_addresses
            .iter()
            .find(|(t, _)| t == "ble")
            .map(|(_, addr)| parse_mac(addr))
            .ok_or(TransportError::PeerNotFound)?;
        self.set_state(TransportState::Connecting);
        let connect_result = adapter.connect_gatt(mac).map_err(to_transport_err);
        let handle = match connect_result {
            Ok(h) => h,
            // Restore a usable state on connection failure (BLE-RT-004,
            // RED-0001-13 pattern from internet.rs): never stay stuck
            // `Connecting` and never bind an entry.
            Err(e) => {
                self.set_state(TransportState::Available);
                return Err(e);
            }
        };
        // Negotiate MTU on the FIRST connection to each peer (RES-0019 R1:
        // Android 14+ negotiates 517 on first requestMtu and disregards later
        // ones). Record the negotiated MTU PER CONNECTION (BLE-RT-003) — later
        // writes to this peer are sized to its own MTU, never a global value.
        let negotiated = adapter
            .set_mtu(handle, crate::transport::ble_att::MTU_NEGOTIATED)
            .unwrap_or(crate::transport::ble_att::MTU_DEFAULT);
        conns.insert(peer.peer_id, (handle, negotiated));
        self.set_state(TransportState::Connected);
        let established_at = Instant::now();
        // Poll inbound GATT writes for THIS peer into a per-peer reassembler,
        // then the incoming stream. Frames are defensively parsed and reassembled
        // (AC-4/AC-6); malformed frames are dropped, never panic (RES-0019 R5).
        // The poller owns `my_handle` and only consumes frames addressed to its
        // own connection (BLE-RT-016: the adapter queue is shared across peers).
        let incoming = self.incoming_tx.clone();
        let adapter = adapter.clone();
        let tid = self.id.clone();
        let peer_id = peer.peer_id;
        let poller = tokio::spawn(async move {
            let mut recon = Reassembler::new();
            let mut last_evict = std::time::Instant::now();
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                // Coarse TTL sweep so partial-message slots are reclaimed
                // (BLE-RT-001: eviction is NOT dead code at runtime).
                let now = std::time::Instant::now();
                if now.duration_since(last_evict) >= Duration::from_secs(1) {
                    recon.evict_stale(REASSEMBLY_TTL, now);
                    last_evict = now;
                }
                let mut writes = adapter.incoming_gatt_writes();
                // Partition the shared queue IN PLACE: take frames addressed to
                // this poller's own connection and leave every other peer's
                // frames for their poller (BLE-RT-016). Draining+filtering
                // would silently drop foreign frames.
                let mut mine: Vec<GattWriteEvent> = Vec::new();
                let mut foreign: Vec<GattWriteEvent> = Vec::with_capacity(writes.len());
                for w in writes.drain(..) {
                    if w.handle == handle {
                        mine.push(w);
                    } else {
                        foreign.push(w);
                    }
                }
                *writes = foreign;
                drop(writes);
                for w in mine {
                    // Reject frames addressed to another characteristic.
                    if w.char_uuid != Uuid::from_u128(0x1) {
                        continue;
                    }
                    match recon.push(&w.data, std::time::Instant::now()) {
                        Ok(Some(payload)) => {
                            let _ = incoming.send(IncomingMessage {
                                peer_id,
                                transport_id: tid.0.clone(),
                                payload,
                                received_at: std::time::Instant::now(),
                            });
                        }
                        Ok(None) => {} // awaiting more chunks
                        Err(_) => {}   // malformed frame dropped (adversarial-safe)
                    }
                }
            }
        });
        // Abort any previous poller for this peer (a reconnect after a partial
        // teardown must not leave the old task draining the shared queue).
        if let Some(prev) = self.pollers.write().unwrap().remove(&peer.peer_id) {
            prev.abort();
        }
        self.pollers
            .write()
            .unwrap()
            .insert(peer.peer_id, poller.abort_handle());
        Ok(TransportLink {
            peer_id: peer.peer_id,
            transport_id: self.id.0.clone(),
            established_at,
        })
    }

    async fn send(
        &self,
        peer: &PeerId,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        let adapter = self.adapter()?;
        let (handle, mtu) = self
            .connections
            .lock()
            .unwrap()
            .get(peer)
            .copied()
            .ok_or(TransportError::NotConnected)?;
        // Segment the payload into ATT frames sized to THIS peer's negotiated
        // MTU (AC-2 + BLE-RT-003). A late high-MTU peer must never inflate
        // frames destined to a low-MTU peer.
        let (_msg_id, frames) = self
            .segmenter
            .segment_for_mtu(&message.payload, mtu)
            .map_err(|e| TransportError::Protocol(format!("ble: segmentation failed: {e}")))?;
        let char_uuid = Uuid::from_u128(0x1); // IRIS control characteristic
        for frame in &frames {
            if let Err(e) = adapter.gatt_write(handle, char_uuid, frame.clone()) {
                // Write failure = link is dead: tear the peer down (BLE-RT-005)
                // rather than leaving a zombie `Connected` entry the manager
                // keeps selecting.
                self.close_peer(peer, adapter.clone(), Some(&e));
                return Err(to_transport_err(e));
            }
        }
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: message.payload.len(),
            sent_at: Instant::now(),
        })
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe())
    }

    fn cost_snapshot(&self) -> TransportCost {
        let congestion = match self.state.load() {
            TransportState::Connected => 0.2,
            _ => 0.0,
        };
        TransportCost {
            estimated_battery_ma: 3.0,
            monetary_cost_per_kb: 0.0,
            bandwidth_available_bps: 1_000_000,
            congestion_level: congestion,
        }
    }

    fn set_send_priority_hint(&self, _priority: MessagePriority) {
        // Could lower tx power for non-emergency; deferred to real impl.
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        // Abort all per-peer pollers (RED-0009-02).
        for (_, p) in self.pollers.write().unwrap().drain() {
            p.abort();
        }
        self.set_state(TransportState::Unavailable);
        // Disconnect every live GATT link (RED-0009-01).
        let adapter = self.adapter()?;
        let handles: Vec<GattHandle> = self
            .connections
            .lock()
            .unwrap()
            .drain()
            .map(|(_, (h, _))| h)
            .collect();
        for h in handles {
            adapter.disconnect_gatt(h);
        }
        Ok(())
    }
}

fn to_transport_err(e: BleError) -> TransportError {
    match e {
        BleError::NotSupported => TransportError::NotSupported,
        BleError::PermissionDenied | BleError::AdapterOff => TransportError::NotSupported,
        BleError::DeviceNotFound => TransportError::PeerNotFound,
        BleError::GattFailure(m) => TransportError::Protocol(m),
    }
}

/// Parse `"AA:BB:CC:DD:EE:FF"` into a `BleAddress`.
fn parse_mac(s: &str) -> BleAddress {
    let mut b = [0u8; 6];
    for (i, part) in s.split(':').take(6).enumerate() {
        b[i] = u8::from_str_radix(part, 16).unwrap_or(0);
    }
    BleAddress(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ble_att::{MTU_DEFAULT, MTU_NEGOTIATED};
    use futures_util::StreamExt;

    fn peer_with_mac(peer_id: PeerId, mac: &str) -> PeerInfo {
        PeerInfo {
            peer_id,
            addresses: Vec::new(),
            transport_addresses: vec![("ble".to_string(), mac.to_string())],
            last_seen: None,
        }
    }

    #[tokio::test]
    async fn no_adapter_returns_not_supported() {
        let t = BleTransport::new(None);
        let peer = peer_with_mac(PeerId([1u8; 32]), "AA:BB:CC:DD:EE:FF");
        let err = t.connect(&peer).await.unwrap_err();
        assert_eq!(err, TransportError::NotSupported);
    }

    #[tokio::test]
    async fn simulated_adapter_connects_and_sends() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([1u8; 32]), "AA:BB:CC:DD:EE:FF");
        let link = t.connect(&peer).await.unwrap();
        assert_eq!(link.peer_id, PeerId([1u8; 32]));
        assert_eq!(t.state(), TransportState::Connected);

        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([2u8; 16]),
            priority: MessagePriority::P2,
            payload: b"ble payload".to_vec(),
        };
        let receipt = t.send(&peer.peer_id, &msg).await.unwrap();
        assert_eq!(receipt.bytes_sent, 11);
    }

    #[tokio::test]
    async fn gatt_write_flows_into_incoming() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([3u8; 32]), "00:11:22:33:44:55");
        t.connect(&peer).await.unwrap();

        // Inject a valid single-chunk frame (header + payload).
        use crate::transport::ble_att::encode_frame;
        let frame = encode_frame(1, 0, 1, b"inbound".len(), b"inbound").unwrap();
        adapter.inject_write(GattHandle(1), Uuid::from_u128(1), frame);
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_millis(500), incoming.next()).await;
        let msg = got.expect("must receive injected write").unwrap();
        assert_eq!(msg.payload, b"inbound");
        assert_eq!(msg.peer_id, PeerId([3u8; 32]));
    }

    #[tokio::test]
    async fn large_message_segments_across_mtu_and_reassembles() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([5u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();

        // 30 KB payload > MTU 23 default → must split into many frames.
        let payload: Vec<u8> = (0..30_000u32).map(|i| (i % 251) as u8).collect();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([6u8; 16]),
            priority: MessagePriority::P2,
            payload: payload.clone(),
        };
        let receipt = t.send(&peer.peer_id, &msg).await.unwrap();
        assert_eq!(receipt.bytes_sent, 30_000);
        let frames: Vec<Vec<u8>> = adapter
            .last_writes()
            .iter()
            .map(|(w, _)| w.data.clone())
            .collect();
        assert!(
            frames.len() > 10,
            "30 KB must segment into many ATT frames, got {}",
            frames.len()
        );

        // Remote peer replays the same frames back (in-order) → reassembled.
        for f in frames {
            adapter.inject_write(GattHandle(1), Uuid::from_u128(1), f);
        }
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        let back = got.expect("reassembled message must arrive").unwrap();
        assert_eq!(back.payload.len(), 30_000);
        assert_eq!(back.payload, payload);
        assert_eq!(back.peer_id, PeerId([5u8; 32]));
    }

    #[tokio::test]
    async fn out_of_order_reassembly_matches_sender_order() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([8u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();

        let payload: Vec<u8> = (0..1000u32).map(|i| (i % 256) as u8).collect();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([9u8; 16]),
            priority: MessagePriority::P1,
            payload: payload.clone(),
        };
        t.send(&peer.peer_id, &msg).await.unwrap();
        let frames: Vec<Vec<u8>> = adapter
            .last_writes()
            .iter()
            .map(|(w, _)| w.data.clone())
            .collect();
        assert!(frames.len() > 1);

        // Deliver chunks in reverse order — the reassembler must reorder (AC-4).
        for f in frames.iter().rev() {
            adapter.inject_write(GattHandle(1), Uuid::from_u128(1), f.clone());
        }
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        let back = got.expect("out-of-order chunks must reassemble").unwrap();
        assert_eq!(back.payload, payload);
    }

    #[tokio::test]
    async fn malformed_gatt_writes_are_dropped_not_panicked() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([11u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();

        // A barrage of garbage writes must not panic the poller (AC-6).
        adapter.inject_write(GattHandle(1), Uuid::from_u128(0xDEAD), b"junk".to_vec());
        adapter.inject_write(GattHandle(1), Uuid::from_u128(1), vec![0u8; 5]); // < header
        adapter.inject_write(GattHandle(1), Uuid::from_u128(1), vec![0xFF; 64]); // count overflow
        tokio::time::sleep(Duration::from_millis(200)).await;
        // No message arrives; transport still works.
        let small = SerializedMessage {
            message_id: crate::protocol::MessageId::from([12u8; 16]),
            priority: MessagePriority::P2,
            payload: b"ok".to_vec(),
        };
        assert!(t.send(&peer.peer_id, &small).await.is_ok());
    }

    #[tokio::test]
    async fn discover_beacon_maps_to_peer_exactly() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer_id = PeerId([0x0F; 32]);
        let short = crate::identity::peer_id::peer_short(&peer_id.0);
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        let beacon = DiscoveryBeacon::build(caps, short, 0);
        adapter.inject_scan_result(
            BleAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
            beacon,
            -60,
        );

        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            got.transport_addresses,
            vec![("ble".to_string(), "AA:BB:CC:DD:EE:FF".to_string())]
        );
        // candidate_peer_id() zero-pads 16B short -> 32B; the leading 16 bytes
        // must match the advertised short (candidate-level hint, not a trust
        // boundary — DEC-BLE-0006).
        assert_eq!(&got.peer_id.0[..16], &short);
    }

    #[tokio::test]
    async fn scan_burst_is_throttled_within_window() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        for _ in 0..SCAN_CEILING {
            assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
        }
        let err = t.discover_peers(DiscoveryConfig::default()).await;
        assert!(
            matches!(err, Err(TransportError::Protocol(msg)) if msg.contains("throttled")),
            "burst beyond {} starts within 30s must be refused (AC-5)",
            SCAN_CEILING
        );
    }

    #[tokio::test]
    async fn reconnect_reuses_live_connection() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([2u8; 32]), "AA:BB:CC:DD:EE:FF");
        let first = t.connect(&peer).await.unwrap();
        assert_eq!(first.peer_id, peer.peer_id);
        let second = t.connect(&peer).await.unwrap();
        assert_eq!(second.peer_id, peer.peer_id);
        assert_eq!(
            adapter.inner_connect_count(),
            1,
            "a live connection must be reused, not re-created (AC-9)"
        );
    }

    #[tokio::test]
    async fn send_before_connect_errors() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter));
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([4u8; 16]),
            priority: MessagePriority::P1,
            payload: b"x".to_vec(),
        };
        assert_eq!(
            t.send(&PeerId([9u8; 32]), &msg).await.unwrap_err(),
            TransportError::NotConnected
        );
    }

    /// Wraps `SimulatedBleAdapter` and records lifecycle calls so tests can
    /// assert the transport really drove the adapter.
    #[derive(Default)]
    struct RecordingBleAdapter {
        inner: SimulatedBleAdapter,
        started_scans: std::sync::Mutex<Vec<ScanFilter>>,
        stopped_scans: std::sync::Mutex<Vec<ScanHandle>>,
        started_adv: std::sync::Mutex<Vec<AdvertisementData>>,
        stopped_adv: std::sync::Mutex<Vec<AdvHandle>>,
        disconnects: std::sync::Mutex<Vec<GattHandle>>,
    }

    impl RecordingBleAdapter {
        fn inner_connect_count(&self) -> usize {
            self.inner.connect_count()
        }
    }

    impl BleAdapter for RecordingBleAdapter {
        fn start_scan(&self, filter: ScanFilter) -> Result<ScanHandle, BleError> {
            self.started_scans.lock().unwrap().push(filter.clone());
            self.inner.start_scan(filter)
        }
        fn stop_scan(&self, handle: ScanHandle) {
            self.stopped_scans.lock().unwrap().push(handle);
            self.inner.stop_scan(handle);
        }
        fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError> {
            self.started_adv.lock().unwrap().push(data.clone());
            self.inner.start_advertising(data)
        }
        fn stop_advertising(&self, handle: AdvHandle) {
            self.stopped_adv.lock().unwrap().push(handle);
            self.inner.stop_advertising(handle);
        }
        fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError> {
            self.inner.connect_gatt(address)
        }
        fn disconnect_gatt(&self, handle: GattHandle) {
            self.disconnects.lock().unwrap().push(handle);
            self.inner.disconnect_gatt(handle);
        }
        fn gatt_write(
            &self,
            handle: GattHandle,
            char_uuid: Uuid,
            data: Vec<u8>,
        ) -> Result<(), BleError> {
            self.inner.gatt_write(handle, char_uuid, data)
        }
        fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(handle, mtu)
        }
        fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
            self.inner.incoming_gatt_writes()
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    #[tokio::test]
    async fn shutdown_sets_unavailable_and_disconnects() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([1u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();
        assert_eq!(t.state(), TransportState::Connected);

        t.shutdown().await.unwrap();
        assert_eq!(t.state(), TransportState::Unavailable);
        assert_eq!(
            adapter.disconnects.lock().unwrap().len(),
            1,
            "shutdown must disconnect the GATT link"
        );
    }

    #[tokio::test]
    async fn scan_and_advertising_handles_round_trip() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));

        // Scan start → stop: stop_scans must receive the exact handle from start_scan.
        let _stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        assert_eq!(adapter.started_scans.lock().unwrap().len(), 1);
        t.stop_discovery().await.unwrap();
        {
            let scans = adapter.stopped_scans.lock().unwrap();
            assert_eq!(scans.len(), 1, "stop_discovery must stop the scan");
            assert_eq!(
                scans[0],
                ScanHandle(1),
                "stop_scan must round-trip the start_scan handle"
            );
        }

        // Advertising start → stop: same handle round-trip.
        let adv = NodeAdvertisement {
            peer_id: PeerId([7u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        t.start_advertising(adv).await.unwrap();
        assert_eq!(adapter.started_adv.lock().unwrap().len(), 1);
        t.stop_advertising().await.unwrap();
        let advs = adapter.stopped_adv.lock().unwrap();
        assert_eq!(
            advs.len(),
            1,
            "stop_advertising must stop the advertisement"
        );
        assert_eq!(
            advs[0],
            AdvHandle(1),
            "stop_advertising must round-trip the start handle"
        );
    }

    #[test]
    fn capability_matrix_marks_ios_background_limited() {
        let t = BleTransport::new(None);
        let caps = t.capabilities();
        assert!(
            !caps.supports_background_ios,
            "iOS background BLE is limited (DISC-0006)"
        );
        assert!(caps.supports_background_android);
        assert!(!caps.requires_special_hardware);
        assert_eq!(caps.cost_class, TransportCostClass::Free);
    }

    #[tokio::test]
    async fn registers_and_is_selectable_via_transport_manager() {
        // AC-1: BleTransport is a Transport-trait impl registerable with and
        // selectable via TransportManager like INTERNET-001.
        use crate::message::MessagePriority as Mp;
        use crate::transport::manager::{TransportManager, TransportSelectionRequest};
        use crate::transport::TransportState as Ts;

        let mgr = TransportManager::new();
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = std::sync::Arc::new(BleTransport::new(Some(adapter.clone())));
        mgr.register(t.clone()).await.unwrap();
        assert_eq!(t.state(), Ts::Available, "BleTransport begins Available");

        let req = TransportSelectionRequest {
            target_peer: None,
            message_size: 300, // <= u16::MAX segmented cap
            priority: Mp::P2,
            max_latency_ms: None,
            prefer_low_cost: false,
            multipath: false,
            fragmentable: false,
        };
        let selected = mgr.select_transports(&req).await;
        assert_eq!(selected.len(), 1, "BLE transport must be eligible (AC-1)");
        assert_eq!(selected[0].transport_id.as_str(), "ble-android");
        assert!(selected[0].score > 0.0);
    }

    #[test]
    fn mac_parser_works() {
        let b = parse_mac("AA:BB:CC:DD:EE:FF");
        assert_eq!(b.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }

    // --- BLE-RT regressions (AC-15 SECURITY_REVIEW, iter 88) ---

    /// Attacker fills the partial map; a NEW multi-chunk message must still
    /// complete once the TTL sweep runs in the live poller (BLE-RT-001 —
    /// eviction was dead code at runtime; the TTL door was permanent DoS).
    #[tokio::test]
    async fn rt001_partial_slots_reclaimed_by_ttl_sweep_in_poller() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([0x10; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();
        // Stampede: 64+ DISTINCT msg_id partials (count=2, only chunk 0 sent).
        use crate::transport::ble_att::encode_frame;
        for id in 0..66u16 {
            // fake: total declared 600, idx 0, count 2
            adapter.inject_write(
                GattHandle(1),
                Uuid::from_u128(1),
                encode_frame(id, 0, 2, 600, &[0u8; 200]).unwrap(),
            );
        }
        // Wait past the 1s sweep cadence; the partials must be evicted.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        // A fresh complete message now flows end-to-end.
        let payload: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
        let frame = encode_frame(50_000, 0, 1, payload.len(), &payload).unwrap();
        adapter.inject_write(GattHandle(1), Uuid::from_u128(1), frame);
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        let msg = got.expect("post-eviction message must reassemble").unwrap();
        assert_eq!(msg.payload, payload);
    }

    /// Two concurrent connects to the same peer must produce a single GATT
    /// link and a single poller (BLE-RT-002 TOCTOU: duplicate connect + orphan).
    #[tokio::test]
    async fn rt002_concurrent_connect_single_gatt_and_poller() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = std::sync::Arc::new(BleTransport::new(Some(adapter.clone())));
        let peer = peer_with_mac(PeerId([0x20; 32]), "AA:BB:CC:DD:EE:FF");
        let (a, b) = tokio::join!(t.connect(&peer), t.connect(&peer));
        a.unwrap();
        b.unwrap();
        assert_eq!(
            adapter.connect_count(),
            1,
            "concurrent connects to same peer must not double-connect (BLE-RT-002)"
        );
        assert_eq!(
            adapter.assigned_handles().len(),
            1,
            "exactly one GATT handle assigned"
        );
        assert_eq!(t.pollers.read().unwrap().len(), 1, "exactly one poller");
        t.shutdown().await.unwrap();
    }

    /// Writes to a low-MTU peer must stay sized to ITS MTU even after a
    /// high-MTU peer connects (BLE-RT-003 — one global MTU blackholed legacy).
    #[tokio::test]
    async fn rt003_frames_sized_to_each_peer_own_mtu() {
        let adapter = std::sync::Arc::new(TwoMtuAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        // low-MTU peer (sim `set_mtu` returns MTU 23) connects first.
        let low = peer_with_mac(PeerId([0x30; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&low).await.unwrap();
        let high = peer_with_mac(PeerId([0x31; 32]), "00:11:22:33:44:55");
        t.connect(&high).await.unwrap();

        let payload: Vec<u8> = (0..400u32).map(|i| (i % 251) as u8).collect();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([0x32; 16]),
            priority: MessagePriority::P2,
            payload,
        };
        t.send(&low.peer_id, &msg).await.unwrap(); // goes to handle 1
        t.send(&high.peer_id, &msg).await.unwrap(); // goes to handle 2
        let writes = adapter.inner.last_writes();
        // Find which handle each write went to: handle 1 = low (MTU 23 → ≤12B data
        // or 18B att payload), handle 2 = high (MTU 517 → ≤512B).
        let low_frames: Vec<&[u8]> = writes
            .iter()
            .filter(|(_, h)| *h == GattHandle(1))
            .map(|(w, _)| w.data.as_slice())
            .collect();
        let high_frames: Vec<&[u8]> = writes
            .iter()
            .filter(|(_, h)| *h == GattHandle(2))
            .map(|(w, _)| w.data.as_slice())
            .collect();
        assert!(
            !low_frames.is_empty() && low_frames.iter().all(|f| f.len() <= 18),
            "low-MTU peer frames must stay <=18B (MTU-23), got {:?}",
            low_frames.iter().map(|f| f.len()).collect::<Vec<_>>()
        );
        assert!(
            high_frames.iter().all(|f| f.len() <= 512 + 6),
            "high-MTU peer frames may be large, got {:?}",
            high_frames.iter().map(|f| f.len()).collect::<Vec<_>>()
        );
        assert!(
            low_frames.len() > high_frames.len(),
            "400B needs more frames at MTU 23 than at 512"
        );
    }

    /// Each peer's poller only consumes frames addressed to ITS OWN GATT
    /// connection; foreign frames stay in the shared queue (BLE-RT-016
    /// misattribution/split).
    #[tokio::test]
    async fn rt016_pollers_do_not_steal_each_others_frames() {
        use crate::transport::ble_att::encode_frame;
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let pa = peer_with_mac(PeerId([0x41; 32]), "AA:BB:CC:DD:EE:FF");
        let pb = peer_with_mac(PeerId([0x42; 32]), "00:11:22:33:44:55");
        t.connect(&pa).await.unwrap(); // handle 1
        t.connect(&pb).await.unwrap(); // handle 2
        let (ha, hb) = {
            // GattHandle is Copy; copy the two assigned handles out and drop
            // the guard before any await (clippy: no guard across await).
            let guard = adapter.assigned_handles();
            let a = guard[0];
            let b = guard[1];
            drop(guard);
            (a, b)
        };

        // Multi-chunk message for peer A (2 chunks per its high MTU).
        let payload_a: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        let (_id, frames) = s.segment(&payload_a).unwrap();
        assert!(frames.len() > 1);
        for f in &frames {
            adapter.inject_write(ha, Uuid::from_u128(1), f.clone());
        }
        // Single-chunk frame for peer B.
        let payload_b = b"b-only".to_vec();
        let fb = encode_frame(1, 0, 1, payload_b.len(), &payload_b).unwrap();
        adapter.inject_write(hb, Uuid::from_u128(1), fb);

        let mut incoming = t.incoming_messages();
        let first = tokio::time::timeout(Duration::from_secs(2), incoming.next())
            .await
            .expect("timed out")
            .unwrap();
        let second = tokio::time::timeout(Duration::from_secs(2), incoming.next())
            .await
            .expect("timed out")
            .unwrap();
        let mut groups: Vec<(PeerId, Vec<u8>)> = vec![
            (first.peer_id, first.payload),
            (second.peer_id, second.payload),
        ];
        groups.sort_by_key(|(p, _)| p.0);
        assert_eq!(groups[0].0, PeerId([0x41; 32]));
        assert_eq!(groups[0].1, payload_a);
        assert_eq!(groups[1].0, PeerId([0x42; 32]));
        assert_eq!(groups[1].1, payload_b);
    }

    /// Failed connect() must restore `Available` (BLE-RT-004) — never stuck
    /// `Connecting` (manager selection filter keeps routing into it).
    #[tokio::test]
    async fn rt004_connect_failure_restores_available() {
        let adapter = std::sync::Arc::new(FailingConnectAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([0x50; 32]), "AA:BB:CC:DD:EE:FF");
        let err = t.connect(&peer).await.unwrap_err();
        assert_ne!(err, TransportError::NotSupported);
        assert_eq!(
            t.state(),
            TransportState::Available,
            "failed connect must fall back to Available"
        );
    }

    /// A failed GATT write must tear down the peer (connection entry + poller +
    /// state) so dead links are never re-selected (BLE-RT-005).
    #[tokio::test]
    async fn rt005_write_failure_tears_down_dead_peer() {
        let adapter = std::sync::Arc::new(FailingWriteAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([0x51; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();
        assert_eq!(t.state(), TransportState::Connected);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([0x52; 16]),
            priority: MessagePriority::P2,
            payload: b"boom".to_vec(),
        };
        assert!(t.send(&peer.peer_id, &msg).await.is_err());
        assert_eq!(
            t.connections.lock().unwrap().len(),
            0,
            "dead peer must be removed from connections (BLE-RT-005)"
        );
        assert_eq!(t.pollers.read().unwrap().len(), 0, "poller aborted");
        assert_eq!(t.state(), TransportState::Available);
    }

    /// start_advertising must stop the prior advertisement before announcing a
    /// new one (BLE-RT-011 — Android advertising-set budget leak).
    #[tokio::test]
    async fn rt011_readvertise_stops_prior_handle() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([0x61; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        t.start_advertising(adv.clone()).await.unwrap();
        // Force an interval so a NEW AdvHandle is assigned (nullable adapter
        // demo: RecordingBleAdapter returns AdvHandle(1) always, so a second
        // start must still issue a stop of the prior handle).
        t.start_advertising(adv).await.unwrap();
        let stopped = adapter.stopped_adv.lock().unwrap();
        assert_eq!(
            stopped.len(),
            1,
            "prior advertisement must be stopped on re-announce"
        );
        assert_eq!(stopped[0], AdvHandle(1));
    }

    /// discover_peers honors a caller-provided max_peers cap (BLE-RT-013).
    #[tokio::test]
    async fn rt013_discovery_enforces_max_peers() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        for i in 0..5u8 {
            adapter.inject_scan_result(
                BleAddress([i, 0, 0, 0, 0, 0]),
                DiscoveryBeacon::build(
                    {
                        let mut c = CapabilityBits::empty();
                        c.set(CapabilityBits::GATT_UNICAST);
                        c
                    },
                    [i; 16],
                    0,
                ),
                -40,
            );
        }
        let cfg = DiscoveryConfig {
            max_peers: 2,
            ..Default::default()
        };
        let mut stream = t.discover_peers(cfg).await.unwrap();
        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 2, "discovery must honor max_peers (BLE-RT-013)");
    }

    /// encode_frame rejects oversized totals/counts instead of truncating
    /// (BLE-RT-007).
    #[test]
    fn rt007_encode_frame_rejects_oversized_values() {
        use crate::transport::ble_att::encode_frame;
        assert_eq!(
            encode_frame(0, 0, 1, 70_000, &[0u8; 4]).unwrap_err(),
            crate::transport::ble_att::FrameError::MessageTooLarge
        );
        assert_eq!(
            encode_frame(0, 0, 300, 10, &[0u8; 4]).unwrap_err(),
            crate::transport::ble_att::FrameError::ChunkCountTooLarge
        );
        assert_eq!(
            encode_frame(0, 5, 3, 10, &[0u8; 4]).unwrap_err(),
            crate::transport::ble_att::FrameError::BadChunkIndex
        );
    }

    // --- test-only adapters for the RT regressions above ---

    /// `set_mtu` returns MTU 23 for handle 1 (legacy) and 517 for handle 2.
    #[derive(Default)]
    struct TwoMtuAdapter {
        inner: SimulatedBleAdapter,
    }

    impl BleAdapter for TwoMtuAdapter {
        fn start_scan(&self, f: ScanFilter) -> Result<ScanHandle, BleError> {
            self.inner.start_scan(f)
        }
        fn stop_scan(&self, h: ScanHandle) {
            self.inner.stop_scan(h)
        }
        fn start_advertising(&self, d: AdvertisementData) -> Result<AdvHandle, BleError> {
            self.inner.start_advertising(d)
        }
        fn stop_advertising(&self, h: AdvHandle) {
            self.inner.stop_advertising(h)
        }
        fn connect_gatt(&self, a: BleAddress) -> Result<GattHandle, BleError> {
            self.inner.connect_gatt(a)
        }
        fn disconnect_gatt(&self, h: GattHandle) {
            self.inner.disconnect_gatt(h)
        }
        fn gatt_write(&self, h: GattHandle, c: Uuid, d: Vec<u8>) -> Result<(), BleError> {
            self.inner.gatt_write(h, c, d)
        }
        fn set_mtu(&self, handle: GattHandle, _mtu: u16) -> Result<u16, BleError> {
            // Legacy device keeps MTU 23; modern peer negotiates 517.
            Ok(if handle == GattHandle(1) {
                MTU_DEFAULT
            } else {
                MTU_NEGOTIATED
            })
        }
        fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
            self.inner.incoming_gatt_writes()
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    #[derive(Default)]
    struct FailingWriteAdapter {
        inner: SimulatedBleAdapter,
    }

    impl BleAdapter for FailingWriteAdapter {
        fn start_scan(&self, f: ScanFilter) -> Result<ScanHandle, BleError> {
            self.inner.start_scan(f)
        }
        fn stop_scan(&self, h: ScanHandle) {
            self.inner.stop_scan(h)
        }
        fn start_advertising(&self, d: AdvertisementData) -> Result<AdvHandle, BleError> {
            self.inner.start_advertising(d)
        }
        fn stop_advertising(&self, h: AdvHandle) {
            self.inner.stop_advertising(h)
        }
        fn connect_gatt(&self, a: BleAddress) -> Result<GattHandle, BleError> {
            self.inner.connect_gatt(a)
        }
        fn disconnect_gatt(&self, h: GattHandle) {
            self.inner.disconnect_gatt(h)
        }
        fn gatt_write(&self, _h: GattHandle, _c: Uuid, _d: Vec<u8>) -> Result<(), BleError> {
            Err(BleError::GattFailure("link dead".to_string()))
        }
        fn set_mtu(&self, h: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(h, mtu)
        }
        fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
            self.inner.incoming_gatt_writes()
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    #[derive(Default)]
    struct FailingConnectAdapter {
        inner: SimulatedBleAdapter,
    }

    impl BleAdapter for FailingConnectAdapter {
        fn start_scan(&self, f: ScanFilter) -> Result<ScanHandle, BleError> {
            self.inner.start_scan(f)
        }
        fn stop_scan(&self, h: ScanHandle) {
            self.inner.stop_scan(h)
        }
        fn start_advertising(&self, d: AdvertisementData) -> Result<AdvHandle, BleError> {
            self.inner.start_advertising(d)
        }
        fn stop_advertising(&self, h: AdvHandle) {
            self.inner.stop_advertising(h)
        }
        fn connect_gatt(&self, _a: BleAddress) -> Result<GattHandle, BleError> {
            Err(BleError::DeviceNotFound)
        }
        fn disconnect_gatt(&self, h: GattHandle) {
            self.inner.disconnect_gatt(h)
        }
        fn gatt_write(&self, h: GattHandle, c: Uuid, d: Vec<u8>) -> Result<(), BleError> {
            self.inner.gatt_write(h, c, d)
        }
        fn set_mtu(&self, h: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(h, mtu)
        }
        fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
            self.inner.incoming_gatt_writes()
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }
}
