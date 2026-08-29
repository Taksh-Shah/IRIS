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

/// IRIS GATT service UUID — **STABLE, never per-session** (DEC-BLE-002-0001):
/// a background/foreground scanner needs the UUID in its service-UUID filter
/// (iOS requires it for background scanning), and overflow-area hash matching
/// requires a stable scanned-for UUID.
pub const IRIS_SERVICE_UUID: Uuid = Uuid([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/// IRIS GATT identification characteristic (connect-to-identify,
/// DEC-BLE-002-0002): carries the 22-byte discovery beacon on iOS, where
/// `startAdvertising(_:)` can transmit only local name + service UUIDs —
/// never service data. Read after connect; Android peers may serve it
/// identically but can skip the read via ad-carried service data. One parser,
/// two carriers (DEC-BLE-002-0008).
pub const IRIS_IDENTIFY_CHARACTERISTIC: Uuid =
    Uuid([2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/// IRIS GATT write characteristic — the one actual `AndroidBleTransportAdapter`
/// (and its iOS counterpart) exposes on the peripheral's GATT server for
/// inbound message frames. Must byte-match `AndroidBleTransportAdapter
/// .IRIS_CHARACTERISTIC_UUID` (3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0e) exactly:
/// `send()` below hands this value across the FFI boundary as the literal
/// UUID the platform adapter searches `gatt.services` for. `send()` used to
/// pass [`IRIS_SERVICE_UUID`] here instead — a different, never-matching
/// value — so `resolveCharacteristic` on the Android side always returned
/// null and every real BLE write failed with "characteristic not yet
/// discovered", tearing the peer back down via `close_peer` on the very
/// first send. No message has ever actually reached a GATT characteristic
/// on real hardware.
pub const IRIS_WRITE_CHARACTERISTIC: Uuid = Uuid([
    0x3e, 0x5c, 0x6b, 0x1a, 0x2a, 0x10, 0x4f, 0x6e, 0x9c, 0x31, 0x5f, 0x3e, 0x5a, 0x0b, 0x0c, 0x0e,
]);

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
///
/// `connectable` used to not exist here at all (BLE-9): the Android bridge
/// hardcoded `non_connectable: true` on every call because the core type
/// gave it nothing else to forward. A non-connectable advertisement
/// (`ADV_NONCONN_IND`) is a Bluetooth Core Spec constraint, not a platform
/// quirk — no central can ever open a GATT connection to one, so
/// `BleTransport::connect()` -> `connect_gatt()` was architecturally
/// unreachable regardless of anything else being correct. `send()` requires
/// a live GATT connection for every message, so this one field was
/// sufficient by itself to make delivery impossible on real hardware in
/// both directions (confirmed live on two physical Android devices).
#[derive(Debug, Clone, Default)]
pub struct AdvertisementData {
    pub local_name: Option<String>,
    pub service_uuid: Option<Uuid>,
    pub service_data: Vec<u8>,
    /// True unless a future beacon-only broadcast mode is added — every
    /// live call site advertises connectably today, matching that
    /// `connect()`/`send()` both require it.
    pub connectable: bool,
}

/// Opaque handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvHandle(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// HW-9: a remote central that connected TO this node's GATT server
/// (peripheral role) — the counterpart to `connect_gatt()`'s own outbound
/// connections. Until this existed, nothing on the peripheral side ever
/// told the core a new link had been accepted, so no per-connection
/// reassembly poller was ever spawned for it: `onCharacteristicWriteRequest`
/// correctly queued the raw write, but no Rust task was ever draining that
/// queue for a connection the local node did not itself initiate. Confirmed
/// live: sender-side `msg.sent` succeeded and the write physically arrived
/// at the peer's GATT server (visible in its own `onCharacteristicWriteRequest`
/// log), but the message was never surfaced anywhere upstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedConnection {
    pub handle: GattHandle,
    pub address: BleAddress,
}

/// Platform-neutral BLE adapter trait (mirrors the spec; real adapters via FFI).
///
/// `IosBleAdapter` (Swift/CoreBluetooth, IOS-001 scope) semantics
/// (RES-0024 / BLE_002_DESIGN.md §5, DEC-BLE-002-0001..0008):
/// - `start_advertising`: transmits ONLY local name + service UUIDs — the
///   `service_data` field is NEVER transmitted on iOS (CoreBluetooth has no
///   API for it); the discovery beacon is served instead from
///   `IRIS_IDENTIFY_CHARACTERISTIC` (connect-to-identify).
/// - `set_mtu`: no request API — no-op request returning
///   `peripheral.maximumWriteValueLength(for: .withResponse)` (negotiated ATT
///   payload, cap 512; may be 23 default or a degraded 20 B on iOS 16.0/16.0.1).
///   The Rust core translates payload → MTU via `ble_att::payload_to_mtu`.
/// - `scan_results`: `centralManager(_:didDiscover:advertisementData:rssi:)`
///   drain — advertisementData carries NO IRIS service data on iOS, so payloads
///   are empty; RSSI + identity only. The transport's connect-to-identify
///   branch probes empty-payload results (see `BleTransport::discover_peers`).
/// - `gatt_read`: `peripheral.readValue(for: characteristic)` — used for the
///   identification-characteristic read during connect-to-identify.
/// - Restoration: `willRestoreState` MUST re-call `start_scan`/`start_advertising`
///   (restoration does NOT auto-resume); restore ids stable across launches;
///   never initialize BLE state outside a valid launch path (DEC-BLE-002-0005).
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
    /// Read a GATT characteristic value (CoreBluetooth `readValue(for:)`).
    /// Used by connect-to-identify to fetch the discovery beacon from
    /// `IRIS_IDENTIFY_CHARACTERISTIC` (DEC-BLE-002-0002).
    fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError>;
    fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError>;
    /// Drain inbound GATT writes addressed to `handle` only (BLE-4).
    ///
    /// This used to be `incoming_gatt_writes(&self) -> MutexGuard<'_,
    /// Vec<GattWriteEvent>>` — a persistent, shared buffer callers were
    /// trusted to partition themselves, writing foreign-peer frames *back*
    /// for another poller to collect later. `SimulatedBleAdapter`'s buffer
    /// really is durable across calls, so every test built on that
    /// contract passed — but both real platform bridges implement the same
    /// signature by `clear()`-ing their buffer and refilling it from a
    /// one-shot FFI drain every call, silently destroying any frames
    /// written back for a sibling poller. With ≥2 concurrent GATT peers on
    /// real hardware, roughly half of each peer's inbound frames were lost
    /// and every multi-chunk message to the loser stalled and TTL-expired
    /// 30s later — invisible in CI, which only ever exercises the
    /// simulator. Handle-scoped draining removes the shared-buffer
    /// write-back contract entirely: each poller only ever asks for (and
    /// the adapter only ever returns) its own connection's frames, so there
    /// is nothing left for a real bridge to destroy.
    fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent>;
    /// Scan results delivered since last drain (real impl: callbacks).
    fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>>;
    /// HW-9: connections accepted by this node's GATT SERVER (a remote
    /// central dialled us) since the last call — the peripheral-role
    /// counterpart to `connect_gatt()`'s central-role connections. Default
    /// empty so every existing `BleAdapter` implementor (the simulator's
    /// many test-local delegating wrappers included) keeps compiling
    /// unchanged; only adapters that can actually observe an accepted
    /// connection (the real Android bridge, `SimulatedBleAdapter` for
    /// tests) need to override it.
    fn accepted_connections(&self) -> Vec<AcceptedConnection> {
        Vec::new()
    }
}

/// In-memory BLE adapter for tests and simulation. No hardware involved.
///
/// Only available in `#[cfg(test)]` builds or when the `test-support` feature
/// is enabled. Not for production use — it silently no-ops hardware calls
/// and is invisible at the type level in release builds (SYS-6).
#[cfg(any(test, feature = "test-support"))]
#[derive(Default)]
pub struct SimulatedBleAdapter {
    events: std::sync::Mutex<Vec<GattWriteEvent>>,
    scan_results: std::sync::Mutex<Vec<ScanResult>>,
    writes: std::sync::Mutex<Vec<(GattWriteEvent, GattHandle)>>,
    connect_calls: std::sync::Mutex<usize>,
    next_handle: std::sync::atomic::AtomicU64,
    handles: std::sync::Mutex<Vec<GattHandle>>,
    /// iOS-leg simulation (BLE-002): `start_advertising` transmits NO service
    /// data (CoreBluetooth supports only local name + service UUIDs —
    /// RES-0024/DI-1); the beacon is served via `IRIS_IDENTIFY_CHARACTERISTIC`
    /// reads (connect-to-identify, DEC-BLE-002-0002).
    ios_mode: std::sync::atomic::AtomicBool,
    /// Value served by `IRIS_IDENTIFY_CHARACTERISTIC` reads (the node's own
    /// 22-byte beacon; inject via `inject_identify_read`).
    identify_data: std::sync::Mutex<Vec<u8>>,
    /// Advertisement data as received from the transport.
    last_ad_raw: std::sync::Mutex<Option<AdvertisementData>>,
    /// Advertisement data as actually transmitted on the wire (iOS: service
    /// data dropped at the FFI boundary, exactly like CoreBluetooth).
    last_ad_wire: std::sync::Mutex<Option<AdvertisementData>>,
    /// HW-1: distinct handle per `start_scan` call (was hardcoded to
    /// `ScanHandle(1)` always, which couldn't distinguish call N from call
    /// N+1 — exactly the ambiguity that let the real bug through unit tests).
    next_scan_handle: std::sync::atomic::AtomicU64,
    /// HW-1: every handle `stop_scan` was called with, in order — lets a
    /// test assert a re-arm stops the PREVIOUS scan before starting a new one.
    scan_stop_calls: std::sync::Mutex<Vec<ScanHandle>>,
}

#[cfg(any(test, feature = "test-support"))]
impl SimulatedBleAdapter {
    pub fn new() -> Self {
        Self::default()
    }
    /// Enter iOS-leg mode: advertising transmits no service data; the beacon is
    /// served from `IRIS_IDENTIFY_CHARACTERISTIC` (BLE-002, DEC-BLE-002-0002).
    pub fn simulate_ios(&self) -> &Self {
        self.ios_mode
            .store(true, std::sync::atomic::Ordering::Relaxed);
        self
    }
    /// Inject the node's identification-characteristic value (its 22-byte
    /// discovery beacon) that `gatt_read` serves for `IRIS_IDENTIFY_CHARACTERISTIC`.
    pub fn inject_identify_read(&self, data: Vec<u8>) {
        *self.identify_data.lock().unwrap_or_else(|p| p.into_inner()) = data;
    }
    /// Advertisement data as passed to `start_advertising` by the transport.
    pub fn last_ad_received(&self) -> Option<AdvertisementData> {
        self.last_ad_raw.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    /// Advertisement data as actually transmitted on the wire.
    pub fn last_ad_wire(&self) -> Option<AdvertisementData> {
        self.last_ad_wire.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    /// Inject an inbound GATT write (simulates a peer writing to our server).
    pub fn inject_write(&self, handle: GattHandle, char_uuid: Uuid, data: Vec<u8>) {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).push(GattWriteEvent {
            handle,
            char_uuid,
            data,
        });
    }
    /// Inject a discovered advertisement (simulates a peer advertising near us).
    pub fn inject_scan_result(&self, address: BleAddress, payload: Vec<u8>, rssi: i32) {
        self.scan_results.lock().unwrap_or_else(|p| p.into_inner()).push(ScanResult {
            address,
            payload,
            rssi,
        });
    }
    /// Last GATT write received (for asserting outbound segmentation).
    pub fn last_writes(&self) -> std::sync::MutexGuard<'_, Vec<(GattWriteEvent, GattHandle)>> {
        self.writes.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Number of `connect_gatt` calls (AC-9: must be 1 across reconnects).
    pub fn connect_count(&self) -> usize {
        *self.connect_calls.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Handles assigned to live connections (for multi-peer tests).
    pub fn assigned_handles(&self) -> std::sync::MutexGuard<'_, Vec<GattHandle>> {
        self.handles.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// HW-1: every handle `stop_scan` was called with, in order.
    pub fn scan_stop_calls(&self) -> Vec<ScanHandle> {
        self.scan_stop_calls.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

#[cfg(any(test, feature = "test-support"))]
impl BleAdapter for SimulatedBleAdapter {
    fn start_scan(&self, _filter: ScanFilter) -> Result<ScanHandle, BleError> {
        // HW-1: was hardcoded ScanHandle(1) for every call — indistinguishable
        // from a re-arm reusing the same handle, which is exactly what let
        // the double-start-without-stop bug through every existing test.
        let id = self
            .next_scan_handle
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        Ok(ScanHandle(id))
    }
    fn stop_scan(&self, handle: ScanHandle) {
        self.scan_stop_calls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(handle);
    }
    fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError> {
        *self.last_ad_raw.lock().unwrap_or_else(|p| p.into_inner()) = Some(data.clone());
        // A real adapter that advertises an IRIS discovery beacon serves the
        // SAME beacon via the GATT identification characteristic on the
        // connect-to-identify path (DEC-BLE-002-0002). Mirror that in the sim:
        // the beacon the peer reads is the beacon this node advertises
        // (BLE-RT-C005) — no separate injection needed for the honest path.
        if !data.service_data.is_empty() {
            *self.identify_data.lock().unwrap_or_else(|p| p.into_inner()) = data.service_data.clone();
        }
        let wire = if self.ios_mode.load(std::sync::atomic::Ordering::Relaxed) {
            // iOS `startAdvertising(_:)` transmits ONLY local name + service
            // UUIDs — never service data (RES-0024 DI-1). The beacon rides the
            // GATT identification characteristic instead (connect-to-identify,
            // DEC-BLE-002-0002); a real IosBleAdapter drops it at the FFI
            // boundary the same way.
            AdvertisementData {
                service_data: Vec::new(),
                ..data
            }
        } else {
            data
        };
        *self.last_ad_wire.lock().unwrap_or_else(|p| p.into_inner()) = Some(wire);
        Ok(AdvHandle(1))
    }
    fn stop_advertising(&self, _handle: AdvHandle) {}
    fn connect_gatt(&self, _address: BleAddress) -> Result<GattHandle, BleError> {
        *self.connect_calls.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        // Distinct handle per connection so multi-peer tests can attribute
        // inbound writes to the right link (BLE-RT-016).
        let h = GattHandle(
            self.next_handle
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1,
        );
        self.handles.lock().unwrap_or_else(|p| p.into_inner()).push(h);
        Ok(h)
    }
    fn disconnect_gatt(&self, _handle: GattHandle) {}
    fn gatt_write(
        &self,
        handle: GattHandle,
        char_uuid: Uuid,
        data: Vec<u8>,
    ) -> Result<(), BleError> {
        self.writes.lock().unwrap_or_else(|p| p.into_inner()).push((
            GattWriteEvent {
                handle,
                char_uuid,
                data: data.clone(),
            },
            handle,
        ));
        Ok(())
    }
    fn gatt_read(&self, _handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError> {
        // Only the IRIS identification characteristic exists on the simulated
        // server; reads of anything else fail like an absent characteristic.
        if char_uuid == IRIS_IDENTIFY_CHARACTERISTIC {
            Ok(self.identify_data.lock().unwrap_or_else(|p| p.into_inner()).clone())
        } else {
            Err(BleError::DeviceNotFound)
        }
    }
    fn set_mtu(&self, _handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
        Ok(mtu)
    }
    fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
        let mut events = self.events.lock().unwrap_or_else(|p| p.into_inner());
        let (mine, foreign): (Vec<_>, Vec<_>) =
            events.drain(..).partition(|w| w.handle == handle);
        *events = foreign;
        mine
    }
    fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
        self.scan_results.lock().unwrap_or_else(|p| p.into_inner())
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
    /// HW-9: `Arc`-wrapped (was a bare `Mutex`) so the accept-poller task
    /// spawned in `start_advertising` — which must outlive any single
    /// `&self` call and therefore cannot borrow `self` — can hold a cheap
    /// clone of the SAME map `send()`/`shutdown()` read, instead of a
    /// snapshot that would immediately go stale.
    connections:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PeerId, (GattHandle, u16)>>>,
    /// Per-peer async lock (BLE-2): concurrent `connect()` calls to the SAME
    /// peer serialize on this (BLE-RT-002 — exactly one GATT link/poller per
    /// peer), but connects to DIFFERENT peers run fully in parallel. The
    /// previous design held `connections` (a `std::sync::Mutex`) across the
    /// entire blocking `connect_gatt()`/`set_mtu()` FFI round trip — on
    /// Android that is a real ACL handshake, 100ms to 30s — so dialling one
    /// slow/unreachable peer stalled `send()` to every OTHER already-
    /// connected peer, including P0 emergency traffic, and could exhaust the
    /// tokio worker pool outright.
    connect_locks:
        std::sync::Mutex<std::collections::HashMap<PeerId, std::sync::Arc<tokio::sync::Mutex<()>>>>,
    /// Abort handles for the per-peer inbound-poll tasks so shutdown() actually
    /// stops them (RED-0009-02, was an unowned infinite loop leaking a task).
    /// HW-9: `Arc`-wrapped for the same reason as `connections` above — the
    /// accept-poller task registers pollers it spawns into this same map so
    /// `shutdown()` aborts them too, not just the ones `connect()` spawned.
    pollers: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<PeerId, tokio::task::AbortHandle>>>,
    /// HW-9: best-known candidate `PeerId` for every `BleAddress` seen in a
    /// discovery beacon (`discover_peers`, DEC-BLE-0006 candidate hint,
    /// same semantics `send()`/HW-5 already treat candidate ids with) —
    /// consulted by the accept-poller to attribute an inbound connection it
    /// did not itself dial, since Android's peripheral role never gives the
    /// GATT server a beacon/identity for the central that just connected to
    /// it, only its MAC address.
    known_addresses: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<BleAddress, PeerId>>>,
    /// HW-9: the single long-lived task that drains `accepted_connections()`
    /// and spawns a reassembly poller for each new one. One per transport
    /// instance, started (idempotently) from `start_advertising` — the
    /// natural point at which this node becomes dialable at all.
    accept_poller: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    /// Fragment-header codec only. BLE-5: the doc here used to claim the
    /// segmenter's own MTU resizes live via `on_mtu_changed` — it never did;
    /// `on_mtu_changed`/`mtu()` have zero callers outside `ble_att.rs`'s own
    /// tests. The real per-peer negotiate-late MTU lives in `connections`
    /// (see its doc above) and `send()` calls `segment_for_mtu(payload, mtu)`
    /// with that per-connection value directly — this segmenter's internal
    /// MTU state is never read on the send path. Verified correct behaviour,
    /// misleading comment; not a functional defect.
    segmenter: AttSegmenter,
    /// Scan restart backoff state (AC-5; >=5 start/stop per 30s ceiling,
    /// RES-0019 R6.2 + G4 Android-17 hardening).
    scan_times: std::sync::Mutex<std::collections::VecDeque<std::time::Instant>>,
    scan_backoff_s: std::sync::Mutex<u64>,
    refused_until: std::sync::RwLock<Option<std::time::Instant>>,
    /// iOS-leg flag (BLE-002): `set_mtu` reports the negotiated ATT *payload*
    /// (iOS `maximumWriteValueLength`), not the MTU — translated via
    /// `ble_att::payload_to_mtu` (DEC-BLE-002-0004).
    ios_leg: bool,
    /// BLE-16: count frames dropped because the inbound broadcast channel has
    /// no subscriber yet (early-connect window). Mirrors wifiaware.rs:dropped_inbound.
    dropped_inbound: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

/// Scan-restart ceiling: max scan starts in the window.
const SCAN_CEILING: usize = 5;
/// Backoff window length (RES-0019: >=5 start/stop per 30 s → silent failure).
const SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
/// Backoff cap (Android 17 hardens throttling; exponential 1s..=30s).
const SCAN_BACKOFF_MAX_S: u64 = 30;

// SYS-2: deadline constants for radio adapter calls (connect / MTU).
// A hung connectGatt call blocks the node indefinitely without these.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MTU_TIMEOUT: Duration = Duration::from_secs(10);

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
            conflict_group: crate::transport::RadioConflictGroup::Bluetooth24GHz,
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
            connections: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            connect_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
            pollers: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
            known_addresses: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            accept_poller: std::sync::Mutex::new(None),
            segmenter: AttSegmenter::new(),
            scan_times: std::sync::Mutex::new(std::collections::VecDeque::new()),
            scan_backoff_s: std::sync::Mutex::new(0),
            refused_until: std::sync::RwLock::new(None),
            ios_leg: false,
            dropped_inbound: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Construct the iOS-leg transport (BLE-002): same platform-neutral core as
    /// the Android leg, with an `IosBleAdapter` (Swift/CoreBluetooth, IOS-001
    /// scope) injected via the same `BleAdapter` FFI seam. Distinct id/display;
    /// iOS constraints stay baked in (`supports_background_ios: false` —
    /// RES-0024: foreground-always symmetric discovery).
    pub fn new_ios(adapter: Option<std::sync::Arc<dyn BleAdapter>>) -> Self {
        let mut t = Self::new(adapter);
        t.id = TransportId::from("ble-ios");
        t.display = "BLE (iOS)".to_string();
        t.ios_leg = true;
        t
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    fn adapter(&self) -> Result<std::sync::Arc<dyn BleAdapter>, TransportError> {
        self.adapter
            .lock()
            .unwrap()
            .clone()
            .ok_or(TransportError::HardwareUnavailable)
    }

    /// HW-5: `self.connections` is keyed by the discovery-time candidate
    /// `PeerId` (`DiscoveryBeacon::candidate_peer_id()`:
    /// `[SHA-256(real_pubkey)[..16], 0u8; 16]` — see that method's own doc
    /// comment, "candidate hint," DEC-BLE-0006), not the real identity a
    /// caller addresses `send()` by. `peer_short` is a pure function of the
    /// real pubkey, so this independently re-derives what `connections`'
    /// key for `real_peer` would be, without needing any live mapping
    /// table. Kept as a single source of truth so `send()`'s fallback
    /// lookup and any future caller stay in sync with
    /// `DiscoveryBeacon::candidate_peer_id()`'s exact byte layout.
    fn candidate_key_for(real_peer: &PeerId) -> PeerId {
        let short = crate::identity::peer_id::peer_short(&real_peer.0);
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&short);
        PeerId(id)
    }

    /// HW-9: deterministic fallback identity for an accepted connection
    /// whose `BleAddress` has no known candidate id yet (this node has not
    /// — or not yet — scanned a beacon from it, even though it just dialed
    /// us). Distinct hash domain (`b"ble-unknown-addr:"` prefix) from
    /// `candidate_key_for`/`peer_short`'s own hashing so this can never
    /// collide with a genuine candidate id and be mistaken for one — it is
    /// explicitly a last-resort placeholder, not a trust claim, consistent
    /// with candidate ids themselves never being a trust boundary
    /// (DEC-BLE-0006). Upstream (message/envelope layer) is expected to
    /// re-attribute the real sender from the decrypted envelope itself,
    /// same as any other candidate-keyed connection.
    fn synthesize_unknown_peer_id(address: &BleAddress) -> PeerId {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"ble-unknown-addr:");
        hasher.update(address.0);
        let digest = hasher.finalize();
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&digest[..16]);
        PeerId(id)
    }

    /// HW-9: the reassembly poller body shared by `connect()` (central-role,
    /// a connection we dialed) and `ensure_accept_poller()` (peripheral-role,
    /// a connection a remote central dialed to US). Previously this loop
    /// existed only inline inside `connect()` — a connection accepted the
    /// other way around had no equivalent, so `onCharacteristicWriteRequest`
    /// could queue a write forever with nothing ever draining it. Free
    /// function (no `&self`) so it can be spawned as a genuinely `'static`
    /// task from either call site without borrowing the transport.
    fn spawn_inbound_poller(
        incoming: broadcast::Sender<IncomingMessage>,
        adapter: std::sync::Arc<dyn BleAdapter>,
        handle: GattHandle,
        peer_id: PeerId,
        tid: TransportId,
        dropped_inbound: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> tokio::task::AbortHandle {
        let task = tokio::spawn(async move {
            // GAP-12: this poller ran a fixed 50ms sleep forever, with no
            // backoff, per connected peer — 8 peers meant 160 wakeups/sec
            // indefinitely, including while completely idle, which also
            // prevents the OS reaching a deep sleep state. Wi-Fi Aware
            // solved this for its (single, transport-wide) poller with a
            // fast/idle split keyed on recent activity; mirror that here,
            // keyed on whether THIS peer has produced a frame recently.
            const FAST_POLL_MS: u64 = 50;
            const IDLE_POLL_MS: u64 = 500;
            const ACTIVE_WINDOW: Duration = Duration::from_secs(2);
            let mut recon = Reassembler::new();
            let mut last_evict = std::time::Instant::now();
            let mut last_activity = std::time::Instant::now();
            loop {
                let fast = last_activity.elapsed() < ACTIVE_WINDOW;
                tokio::time::sleep(Duration::from_millis(if fast {
                    FAST_POLL_MS
                } else {
                    IDLE_POLL_MS
                }))
                .await;
                // Coarse TTL sweep so partial-message slots are reclaimed
                // (BLE-RT-001: eviction is NOT dead code at runtime).
                let now = std::time::Instant::now();
                if now.duration_since(last_evict) >= Duration::from_secs(1) {
                    recon.evict_stale(REASSEMBLY_TTL, now);
                    last_evict = now;
                }
                // BLE-4: handle-scoped drain — the adapter itself now owns
                // partitioning frames by connection (see `drain_gatt_writes`
                // doc), so there is no shared buffer here for a real bridge
                // to clear out from under a sibling poller (BLE-RT-016).
                let mine = adapter.drain_gatt_writes(handle);
                if !mine.is_empty() {
                    last_activity = std::time::Instant::now();
                }
                for w in mine {
                    // Reject frames addressed to another characteristic.
                    if w.char_uuid != IRIS_WRITE_CHARACTERISTIC {
                        continue;
                    }
                    match recon.push(&w.data, std::time::Instant::now()) {
                        Ok(Some(payload)) => {
                            // BLE-16: count frames dropped due to no subscriber.
                            if incoming.send(IncomingMessage {
                                peer_id,
                                transport_id: tid.0.clone(),
                                payload,
                                received_at: std::time::Instant::now(),
                            }).is_err() {
                                dropped_inbound.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        Ok(None) => {} // awaiting more chunks
                        Err(_) => {}   // malformed frame dropped (adversarial-safe)
                    }
                }
            }
        });
        task.abort_handle()
    }

    /// HW-9: start (idempotently) the single long-lived task that drains
    /// `adapter.accepted_connections()` — connections a remote central
    /// dialed to US — and spawns `spawn_inbound_poller` for each new one,
    /// exactly as `connect()` already does for connections we dial
    /// ourselves. Called from `start_advertising`: becoming dialable at all
    /// is the natural point to also start accepting. A no-op on every call
    /// after the first (an already-running accept-poller keeps running
    /// across advertising restarts/re-announcements).
    fn ensure_accept_poller(&self, adapter: std::sync::Arc<dyn BleAdapter>) {
        let mut guard = self.accept_poller.lock().unwrap_or_else(|p| p.into_inner());
        if guard.is_some() {
            return;
        }
        let known_addresses = self.known_addresses.clone();
        let connections = self.connections.clone();
        let pollers = self.pollers.clone();
        let incoming_tx = self.incoming_tx.clone();
        let tid = self.id.clone();
        let dropped_inbound_accept = self.dropped_inbound.clone();
        let task = tokio::spawn(async move {
            // Handles already turned into a spawned poller — an accepted
            // connection is reported on every drain until it disconnects
            // (mirrors `drain_gatt_writes`'s own "since last call" framing
            // for the underlying queue), so this guards against spawning a
            // duplicate poller for the same still-live connection.
            let mut spawned: std::collections::HashSet<GattHandle> = std::collections::HashSet::new();
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                for accepted in adapter.accepted_connections() {
                    if !spawned.insert(accepted.handle) {
                        continue;
                    }
                    let peer_id = known_addresses
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .get(&accepted.address)
                        .copied()
                        .unwrap_or_else(|| BleTransport::synthesize_unknown_peer_id(&accepted.address));
                    // Registered under whatever MTU `set_mtu` would default
                    // to — the peripheral role never proactively negotiates
                    // (only the central side calls `requestMtu`), so this is
                    // a size assumption, not a measured value, exactly like
                    // `MTU_DEFAULT`'s existing use as a pre-negotiation
                    // fallback elsewhere in this file.
                    connections
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .entry(peer_id)
                        .or_insert((accepted.handle, crate::transport::ble_att::MTU_DEFAULT));
                    let abort = BleTransport::spawn_inbound_poller(
                        incoming_tx.clone(),
                        adapter.clone(),
                        accepted.handle,
                        peer_id,
                        tid.clone(),
                        dropped_inbound_accept.clone(),
                    );
                    if let Some(prev) = pollers.write().unwrap_or_else(|p| p.into_inner()).insert(peer_id, abort) {
                        prev.abort();
                    }
                }
            }
        });
        *guard = Some(task.abort_handle());
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
        if let Some((handle, _)) = self.connections.lock().unwrap_or_else(|p| p.into_inner()).remove(peer) {
            adapter.disconnect_gatt(handle);
        }
        // Not load-bearing for correctness (a stale empty-Mutex entry just
        // gets reused on the next connect), only for not growing forever
        // across a long-lived node's peer churn.
        self.connect_locks.lock().unwrap_or_else(|p| p.into_inner()).remove(peer);
        if let Some(p) = self.pollers.write().unwrap_or_else(|p| p.into_inner()).remove(peer) {
            p.abort();
        }
        // Fall back to `Available` once a dead link is gone so the manager's
        // `state >= Available` selection filter doesn't keep routing into it.
        if self.connections.lock().unwrap_or_else(|p| p.into_inner()).is_empty() {
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
        let mut times = self.scan_times.lock().unwrap_or_else(|p| p.into_inner());
        while let Some(&t) = times.front() {
            if now.duration_since(t) > SCAN_WINDOW {
                times.pop_front();
            } else {
                break;
            }
        }
        // Honor an active backoff refusal window first.
        {
            let refused = self.refused_until.read().unwrap_or_else(|p| p.into_inner());
            if let Some(until) = *refused {
                if now < until {
                    return false;
                }
            }
        }
        if times.len() >= SCAN_CEILING {
            // Refuse and grow the backoff for future attempts.
            let mut backoff = self.scan_backoff_s.lock().unwrap_or_else(|p| p.into_inner());
            *backoff = if *backoff == 0 {
                1
            } else {
                (*backoff * 2).min(SCAN_BACKOFF_MAX_S)
            };
            let wait = *backoff;
            drop(backoff);
            let until = now + std::time::Duration::from_secs(wait);
            *self.refused_until.write().unwrap_or_else(|p| p.into_inner()) = Some(until);
            return false;
        }
        // BLE-17: reset the backoff when the window is quiet (times is empty after
        // expiry — no burst in the last SCAN_WINDOW seconds).
        if times.is_empty() {
            *self.scan_backoff_s.lock().unwrap_or_else(|p| p.into_inner()) = 0;
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
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "ble.state")
    }

    async fn discover_peers(
        &self,
        config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        let adapter = self.adapter()?;
        if !self.scan_allowed() {
            // MG-40: throttling is transient (this same transport will
            // accept a scan again once the cooldown passes) — it is
            // neither a framing failure nor a permanent policy denial, so
            // it belongs with `Busy`, not `Protocol`.
            return Err(TransportError::Busy);
        }
        let filter = if self.ios_leg {
            // iOS background contract requires a service-UUID-filtered scan
            // (RES-0024 DI-1 / BLE_002_DESIGN.md); connect-to-identify probes
            // must never run against unfiltered ambient advertising.
            ScanFilter {
                service_uuids: vec![IRIS_SERVICE_UUID],
                ..ScanFilter::default()
            }
        } else {
            ScanFilter::default()
        };
        // HW-1 (found on real hardware, 2026-08-27): every discovery pass
        // called start_scan() again without first stopping the PREVIOUS
        // scan — unlike start_advertising just below, which already learned
        // this lesson (BLE-RT-011: "must stop any prior advertisement
        // first"). Android's BluetoothLeScanner.startScan() called twice
        // with the same callback object doesn't throw synchronously; it
        // returns normally (so Kotlin minted a new handle and reported
        // success to Rust) and THEN asynchronously fires
        // ScanCallback.onScanFailed(SCAN_FAILED_ALREADY_STARTED) — a
        // "returns Ok but actually failed" defect, invisible without a
        // real device. Confirmed live: two phones running this exact
        // session's build showed one successful startScan on launch, then
        // `IrisBle: startScan failed errorCode=1` repeating on every
        // ~30s re-arm forever after, on the device whose discovery loop
        // fired a second pass before this fix.
        {
            let mut scan_handle = self.scan_handle.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(prior) = *scan_handle {
                adapter.stop_scan(prior);
            }
            let handle = adapter.start_scan(filter).map_err(to_transport_err)?;
            *scan_handle = Some(handle);
        }
        // Drain the first max_peers scan results. BLE-13: drain(..).take(n)
        // removes and discards the whole Vec's tail — use a bounded drain so
        // results past the cap are left queued for the next pass (BLE-RT-C007).
        let results: Vec<ScanResult> = {
            let mut q = adapter.scan_results();
            let n = q.len().min(config.max_peers);
            q.drain(..n).collect()
        };
        let max_peers = config.max_peers;
        // iOS-leg probe budget: connect-to-identify probes are the most
        // expensive BLE op; cap consecutive probes per discovery pass at the
        // iOS practical connection ceiling (~8, RES-0024 RQ-3) so ambient /
        // hostile empty-ad traffic cannot churn the GATT pool (BLE-RT-C001/003).
        let probe_budget: usize = if self.ios_leg { 8 } else { 0 };
        // An advertisement may carry NO IRIS beacon payload. That is the iOS
        // reality: `startAdvertising(_:)` transmits only local name + service
        // UUIDs — never service data (RES-0024 DI-1) — so the beacon is served
        // from `IRIS_IDENTIFY_CHARACTERISTIC` instead (connect-to-identify,
        // DEC-BLE-002-0002): probe-connect, read the characteristic, parse,
        // and close the probe. The result is candidate-level only, never a
        // trust boundary (DEC-BLE-002-0006); malformed/absent identify reads
        // drop the peer without panic (AC-6).
        let mut probe_count: usize = 0;
        let peers: Vec<PeerInfo> = results
            .into_iter()
            .take(max_peers)
            .filter_map(|r| {
                // Connect-to-identify (iOS leg ONLY, BLE-RT-C001): on Android
                // the ad-carried beacon is the discovery payload — an empty
                // payload means "not an IRIS peer", never a probe target.
                // Gating on ios_leg prevents blind GATT connects against
                // ambient non-IRIS advertising on the production Android leg.
                let probe_guard = &mut probe_count;
                let beacon = if r.payload.is_empty() && self.ios_leg {
                    // Probe budget (BLE-RT-C001/003): at most probe_budget
                    // connect/read/disconnect cycles per discovery pass.
                    if *probe_guard == probe_budget {
                        return None;
                    }
                    *probe_guard += 1;
                    let handle = adapter.connect_gatt(r.address).ok()?;
                    let payload = adapter.gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC).ok();
                    adapter.disconnect_gatt(handle);
                    DiscoveryBeacon::parse(&payload?).ok()
                } else if r.payload.is_empty() {
                    None
                } else {
                    DiscoveryBeacon::parse(&r.payload).ok()
                }?;
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
                // HW-9: record this address's candidate id so the
                // accept-poller (a connection WE did not dial, so its
                // GATT-server callback only ever sees a MAC, never a beacon)
                // can attribute it to a peer we've independently seen while
                // scanning, exactly as `send()`'s HW-5 candidate resolution
                // already treats this same value.
                self.known_addresses
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .insert(r.address, beacon.candidate_peer_id());
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
        if let Some(h) = *self.scan_handle.lock().unwrap_or_else(|p| p.into_inner()) {
            self.adapter()?.stop_scan(h);
        }
        Ok(())
    }

    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError> {
        let adapter = self.adapter()?;
        // Re-announcement (e.g. after identity/key rotation) must stop any prior
        // advertisement first so Android's max ~4 advertising-set budget is not
        // leaked and the radio doesn't keep a stale set alive (BLE-RT-011).
        if let Some(prior) = *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) {
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
                service_uuid: Some(IRIS_SERVICE_UUID),
                service_data: beacon,
                // BLE-9: every live call site needs a connectable
                // advertisement — connect()/send() both require a GATT
                // connection, which a non-connectable advert can never
                // accept.
                connectable: true,
            })
            .map_err(to_transport_err)?;
        *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        // HW-9: becoming dialable is the natural point to also start
        // accepting — idempotent, so re-announcement (key rotation etc.)
        // never spawns a second accept-poller alongside the first.
        self.ensure_accept_poller(adapter);
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        if let Some(h) = *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) {
            self.adapter()?.stop_advertising(h);
        }
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        let adapter = self.adapter()?;
        // Peer already connected: reuse the link (AC-9 — one GATT connection per
        // peer; GATT-client churn bounded, RES-0019 R6.4/G2).
        if self.connections.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&peer.peer_id) {
            return Ok(TransportLink {
                peer_id: peer.peer_id,
                transport_id: self.id.0.clone(),
                established_at: Instant::now(),
            });
        }
        // BLE-2: per-peer async lock. Concurrent connects to the SAME peer
        // serialize here (BLE-RT-002 — exactly one GATT link/poller ever
        // gets created), but connects to DIFFERENT peers no longer share one
        // lock held across the blocking FFI round trip below — see the
        // `connect_locks` field doc for what that used to cost.
        let peer_lock = {
            let mut locks = self.connect_locks.lock().unwrap_or_else(|p| p.into_inner());
            locks
                .entry(peer.peer_id)
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _peer_guard = peer_lock.lock().await;
        // Re-check now that we hold the per-peer lock: the connect that held
        // it before us may have just finished (this is what makes rt002's
        // "both calls succeed, only one real GATT connection" contract work
        // without the second caller redoing any work).
        if self.connections.lock().unwrap_or_else(|p| p.into_inner()).contains_key(&peer.peer_id) {
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
        // BLE-3: only the FIRST connection in flight should move the
        // transport-wide state to `Connecting`. Doing this unconditionally
        // meant that dialling peer #4 while peers #1-3 were already
        // `Connected` and working temporarily downgraded the WHOLE transport
        // to `Connecting` — a state the manager's scoring (manager.rs
        // `_ => -1000.0`) treats as "eliminate this transport", so every
        // other live peer briefly lost all routing eligibility on every new
        // connect attempt.
        let had_other_connections = !self.connections.lock().unwrap_or_else(|p| p.into_inner()).is_empty();
        if !had_other_connections {
            self.set_state(TransportState::Connecting);
        }
        // BLE-2: `connect_gatt` now blocks until the platform actually
        // confirms the connection (or fails/times out) — see
        // AndroidBleTransportAdapter.connectGatt(), which used to return the
        // instant `device.connectGatt()` was merely CALLED, not once Android
        // reported `STATE_CONNECTED`. Rust believed the peer was live and
        // immediately tried to write to it — before the real ACL handshake
        // had even finished — so the first write always failed
        // "characteristic not yet discovered" and tore the brand-new
        // connection back down via `close_peer` (BLE-1) before it was ever
        // usable. Every subsequent discovery cycle repeated the identical
        // race forever: no message could ever be delivered.
        //
        // Now that the FFI call itself can take real wall-clock seconds
        // (a genuine ACL handshake, not a fire-and-forget request), running
        // it inline on this tokio worker would stall every other task
        // scheduled on it — `spawn_blocking` hands it to the blocking pool
        // instead.
        let blocking_adapter = adapter.clone();
        let connect_result = match tokio::time::timeout(
            CONNECT_TIMEOUT,
            tokio::task::spawn_blocking(move || blocking_adapter.connect_gatt(mac)),
        )
        .await
        {
            Ok(join_result) => join_result
                .unwrap_or_else(|e| {
                    Err(BleError::GattFailure(format!("connect_gatt task panicked: {e}")))
                })
                .map_err(to_transport_err),
            Err(_elapsed) => Err(TransportError::ConnectionFailed),
        };
        let handle = match connect_result {
            Ok(h) => h,
            // Restore a usable state on connection failure (BLE-RT-004,
            // RED-0001-13 pattern from internet.rs): never stay stuck
            // `Connecting` and never bind an entry. Restore to `Connected`
            // rather than `Available` if other peers are still live — a
            // failed dial to a new peer must not make the transport look
            // less capable than it actually is.
            Err(e) => {
                self.set_state(if had_other_connections {
                    TransportState::Connected
                } else {
                    TransportState::Available
                });
                return Err(e);
            }
        };
        // Negotiate MTU on the FIRST connection to each peer (RES-0019 R1:
        // Android 14+ negotiates 517 on first requestMtu and disregards later
        // ones). Record the negotiated MTU PER CONNECTION (BLE-RT-003) — later
        // writes to this peer are sized to its own MTU, never a global value.
        // Also run on the blocking pool: it is the same kind of synchronous
        // platform round trip as connect_gatt above.
        let mtu_adapter = adapter.clone();
        let negotiated = tokio::time::timeout(
            MTU_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                mtu_adapter.set_mtu(handle, crate::transport::ble_att::MTU_NEGOTIATED)
            }),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        .and_then(Result::ok)
        .unwrap_or(crate::transport::ble_att::MTU_DEFAULT);
        // iOS reports the negotiated ATT *payload* (`maximumWriteValueLength`,
        // no request API) rather than the MTU — translate so per-connection
        // frame sizing honors the FULL report (DEC-BLE-002-0004). Degraded
        // 20-B payloads (iOS 16.0/16.0.1) are accepted as-is, never an error.
        let stored_mtu = if self.ios_leg {
            crate::transport::ble_att::payload_to_mtu(negotiated)
        } else {
            negotiated
        };
        self.connections
            .lock()
            .unwrap()
            .insert(peer.peer_id, (handle, stored_mtu));
        self.set_state(TransportState::Connected);
        let established_at = Instant::now();
        // Poll inbound GATT writes for THIS peer into a per-peer reassembler,
        // then the incoming stream. Frames are defensively parsed and reassembled
        // (AC-4/AC-6); malformed frames are dropped, never panic (RES-0019 R5).
        // The poller owns `my_handle` and only consumes frames addressed to its
        // own connection (BLE-RT-016: the adapter queue is shared across peers).
        // HW-9: extracted to `spawn_inbound_poller` so the accept-poller
        // (peripheral-role connections we did not dial) can spawn the exact
        // same reassembly loop instead of a second, drifting copy of it.
        let abort_handle = Self::spawn_inbound_poller(
            self.incoming_tx.clone(),
            adapter.clone(),
            handle,
            peer.peer_id,
            self.id.clone(),
            self.dropped_inbound.clone(),
        );
        // Abort any previous poller for this peer (a reconnect after a partial
        // teardown must not leave the old task draining the shared queue).
        if let Some(prev) = self.pollers.write().unwrap_or_else(|p| p.into_inner()).remove(&peer.peer_id) {
            prev.abort();
        }
        self.pollers
            .write()
            .unwrap()
            .insert(peer.peer_id, abort_handle);
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
        // HW-5: `self.connections` is keyed by whatever `PeerId` `connect()`
        // was called with — and discovery calls `connect()` with the
        // BEACON'S candidate id (`DiscoveryBeacon::candidate_peer_id()`:
        // SHA-256(real pubkey)[..16], zero-padded to 32 bytes — "candidate
        // hint," never the verified full identity, DEC-BLE-0006). But
        // `deliver_outbound` addresses `send()` by the envelope's real
        // recipient `PeerId` — the actual 64-hex identity a user/relay
        // specifies (`/to <peer-id>`), which is NEVER equal to the
        // zero-padded candidate form. `connections.get(peer)` therefore
        // missed on every real send — confirmed live: `transport: not
        // connected to peer`, despite `discovery.connect_ok` having fired
        // repeatedly for the very same physical link seconds earlier.
        // `peer_short()` is a pure function of the real pubkey (no live
        // state needed), so the candidate form for ANY real PeerId can be
        // independently re-derived here — try the exact key first (the
        // common case once/if a verified real-identity mapping exists), and
        // fall back to the derived candidate key, which is what's actually
        // in the map today.
        let resolved_key = {
            let connections = self.connections.lock().unwrap();
            if connections.contains_key(peer) {
                Some(*peer)
            } else {
                let candidate = Self::candidate_key_for(peer);
                connections.contains_key(&candidate).then_some(candidate)
            }
        };
        let resolved_key = resolved_key.ok_or(TransportError::NotConnected)?;
        let (handle, mtu) = self
            .connections
            .lock()
            .unwrap()
            .get(&resolved_key)
            .copied()
            .ok_or(TransportError::NotConnected)?;
        // Segment the payload into ATT frames sized to THIS peer's negotiated
        // MTU (AC-2 + BLE-RT-003). A late high-MTU peer must never inflate
        // frames destined to a low-MTU peer.
        let (_msg_id, frames) = self.segmenter.segment_for_mtu(&message.payload, mtu).map_err(|e| {
            // MG-40: a payload/chunk-count overflow is resolvable by
            // fragmenting upstream and retrying — a genuine framing bug
            // (TooShort/BadChunkIndex/Truncated/TooManyPartials) is not.
            match e {
                crate::transport::ble_att::FrameError::MessageTooLarge
                | crate::transport::ble_att::FrameError::ChunkCountTooLarge => {
                    TransportError::MessageTooLarge {
                        limit: MAX_MESSAGE_BYTES,
                        actual: message.payload.len(),
                    }
                }
                other => TransportError::Protocol(format!("ble: segmentation failed: {other}")),
            }
        })?;
        let char_uuid = IRIS_WRITE_CHARACTERISTIC;
        for frame in &frames {
            if let Err(e) = adapter.gatt_write(handle, char_uuid, frame.clone()) {
                // Write failure = link is dead: tear the peer down (BLE-RT-005)
                // rather than leaving a zombie `Connected` entry the manager
                // keeps selecting. Must close by `resolved_key` (whichever
                // form is actually in `connections`), not the caller's
                // `peer` — HW-5: those two can legitimately differ.
                self.close_peer(&resolved_key, adapter.clone(), Some(&e));
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
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "ble.messages")
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
        for (_, p) in self.pollers.write().unwrap_or_else(|p| p.into_inner()).drain() {
            p.abort();
        }
        // HW-9: the accept-poller is a separate, transport-lifetime task —
        // not one of the per-peer entries above — so it needs its own abort
        // or shutdown() would leave it running forever.
        if let Some(p) = self.accept_poller.lock().unwrap_or_else(|p| p.into_inner()).take() {
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
        // MG-41: these three used to collapse into one `NotSupported` —
        // "no adapter", "no permission", and "adapter present but off" have
        // three different user remediations (hide the feature / prompt an
        // in-app permission dialog / point at system Bluetooth settings).
        BleError::NotSupported => TransportError::HardwareUnavailable,
        BleError::PermissionDenied => TransportError::PermissionDenied {
            permission: "bluetooth",
        },
        BleError::AdapterOff => TransportError::RadioDisabled,
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
    async fn no_adapter_returns_hardware_unavailable() {
        let t = BleTransport::new(None);
        let peer = peer_with_mac(PeerId([1u8; 32]), "AA:BB:CC:DD:EE:FF");
        let err = t.connect(&peer).await.unwrap_err();
        assert_eq!(err, TransportError::HardwareUnavailable);
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
        adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, frame);
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
            adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, f);
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
            adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, f.clone());
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
        adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, vec![0u8; 5]); // < header
        adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, vec![0xFF; 64]); // count overflow
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
    async fn hw5_send_by_real_peer_id_reaches_a_connection_keyed_by_candidate_id() {
        // HW-5: found on real hardware — `connections` is keyed by whatever
        // PeerId `connect()` was called with, and discovery calls connect()
        // with the beacon's CANDIDATE id (see discover_beacon_maps_to_peer
        // _exactly above: candidate_peer_id() zero-pads a 16-byte SHA-256
        // short hash, never the real 32-byte identity). But
        // deliver_outbound addresses send() by the envelope's REAL
        // recipient PeerId — what a user actually specifies (`/to
        // <peer-id>`) — which is never equal to that zero-padded candidate
        // form. Confirmed live: `transport: not connected to peer` on
        // every real send, despite `discovery.connect_ok` firing
        // repeatedly for the same physical link seconds earlier.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let real_peer_id = PeerId([0x0F; 32]);
        let short = crate::identity::peer_id::peer_short(&real_peer_id.0);
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        let beacon = DiscoveryBeacon::build(caps, short, 0);
        adapter.inject_scan_result(BleAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]), beacon, -60);

        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let discovered = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap();
        // Sanity: discovery really does hand back the candidate form, not
        // the real id — this is the precondition the bug depended on.
        assert_ne!(discovered.peer_id, real_peer_id);

        t.connect(&discovered).await.unwrap();

        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([9u8; 16]),
            priority: MessagePriority::P2,
            payload: b"hello".to_vec(),
        };
        // The actual bug: addressing by the REAL peer id (what
        // deliver_outbound does) used to fail with NotConnected even though
        // a live connection exists for this exact physical peer, just
        // stored under its candidate key.
        assert!(
            t.send(&real_peer_id, &msg).await.is_ok(),
            "send() must resolve the real PeerId to the connection stored under its derived candidate key"
        );
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
            matches!(err, Err(TransportError::Busy)),
            "burst beyond {} starts within 30s must be refused (AC-5)",
            SCAN_CEILING
        );
    }

    #[tokio::test]
    async fn hw1_rearming_discovery_stops_the_previous_scan_first() {
        // HW-1: found on real hardware — a second discover_peers() call
        // used to call adapter.start_scan() again without ever stopping
        // the FIRST scan. On a real BluetoothLeScanner this produces
        // SCAN_FAILED_ALREADY_STARTED forever after (confirmed live on
        // two physical devices this session); the simulated adapter
        // couldn't have caught it before because start_scan() always
        // returned the SAME hardcoded ScanHandle(1) — indistinguishable
        // from a genuine re-arm.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));

        let _first = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        assert!(
            adapter.scan_stop_calls().is_empty(),
            "the very first scan has nothing prior to stop"
        );

        let _second = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let stops = adapter.scan_stop_calls();
        assert_eq!(
            stops,
            vec![ScanHandle(1)],
            "re-arming discovery must stop_scan the FIRST call's handle \
             before starting a new one — exactly once, with the prior handle"
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
            self.started_scans.lock().unwrap_or_else(|p| p.into_inner()).push(filter.clone());
            self.inner.start_scan(filter)
        }
        fn stop_scan(&self, handle: ScanHandle) {
            self.stopped_scans.lock().unwrap_or_else(|p| p.into_inner()).push(handle);
            self.inner.stop_scan(handle);
        }
        fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError> {
            self.started_adv.lock().unwrap_or_else(|p| p.into_inner()).push(data.clone());
            self.inner.start_advertising(data)
        }
        fn stop_advertising(&self, handle: AdvHandle) {
            self.stopped_adv.lock().unwrap_or_else(|p| p.into_inner()).push(handle);
            self.inner.stop_advertising(handle);
        }
        fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError> {
            self.inner.connect_gatt(address)
        }
        fn disconnect_gatt(&self, handle: GattHandle) {
            self.disconnects.lock().unwrap_or_else(|p| p.into_inner()).push(handle);
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
        fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(handle, char_uuid)
        }
        fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(handle, mtu)
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
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
            adapter.disconnects.lock().unwrap_or_else(|p| p.into_inner()).len(),
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
        assert_eq!(adapter.started_scans.lock().unwrap_or_else(|p| p.into_inner()).len(), 1);
        t.stop_discovery().await.unwrap();
        {
            let scans = adapter.stopped_scans.lock().unwrap_or_else(|p| p.into_inner());
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
        assert_eq!(adapter.started_adv.lock().unwrap_or_else(|p| p.into_inner()).len(), 1);
        t.stop_advertising().await.unwrap();
        let advs = adapter.stopped_adv.lock().unwrap_or_else(|p| p.into_inner());
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

    // --- BLE-002 iOS-leg tests (iter ~135, AC-1/2/3/4/6/10/12) ---

    /// Test adapter mimicking `IosBleAdapter::set_mtu` = `maximumWriteValueLength`
    /// — iOS has NO MTU request API; it reports the negotiated ATT *payload*
    /// (cap 512, may be 23 default or a degraded 20 B; DEC-BLE-002-0004).
    #[derive(Default)]
    struct IosPayloadAdapter {
        inner: SimulatedBleAdapter,
        reported: u16,
    }

    impl BleAdapter for IosPayloadAdapter {
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
        fn gatt_read(&self, h: GattHandle, c: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(h, c)
        }
        fn set_mtu(&self, _h: GattHandle, _m: u16) -> Result<u16, BleError> {
            Ok(self.reported)
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    #[test]
    fn ios_leg_constructs_with_distinct_id_and_background_cap_held() {
        // AC-1/AC-10 (RES-0024 RQ-2/DI-3): the iOS leg is a distinct
        // BleTransport id/display; `supports_background_ios` stays false —
        // foreground-always symmetric discovery, no PendingIntent analog.
        let t = BleTransport::new_ios(Some(std::sync::Arc::new(SimulatedBleAdapter::new())));
        assert_eq!(t.transport_id().as_str(), "ble-ios");
        assert_eq!(t.display_name(), "BLE (iOS)");
        assert!(
            !t.capabilities().supports_background_ios,
            "iOS background BLE support stays false on the iOS leg (RES-0024)"
        );
    }

    #[tokio::test]
    async fn ios_connect_to_identify_yields_candidate_from_identify_characteristic() {
        // AC-3 (DEC-BLE-002-0002): iOS ads carry NO discovery beacon (no
        // service data — RES-0024 DI-1); discovery = connect →
        // read IRIS_IDENTIFY_CHARACTERISTIC → parse → candidate peer.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.simulate_ios();
        let t = BleTransport::new_ios(Some(adapter.clone()));
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        let beacon = DiscoveryBeacon::build(caps, [7u8; 16], 1234);
        adapter.inject_identify_read(beacon.clone());
        // An iOS advertisement with an EMPTY payload (UUID + local name only).
        adapter.inject_scan_result(BleAddress([0x11; 6]), Vec::new(), -60);
        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let peer = tokio::time::timeout(Duration::from_secs(1), stream.next())
            .await
            .expect("candidate must arrive")
            .expect("some peer");
        let expected = DiscoveryBeacon::parse(&beacon).unwrap().candidate_peer_id();
        assert_eq!(peer.peer_id, expected);
        assert_eq!(peer.transport_addresses[0].0, "ble");
        assert_eq!(
            peer.transport_addresses[0].1, "11:11:11:11:11:11",
            "the probe-connect MAC must carry into the candidate for later connect()"
        );
    }

    #[tokio::test]
    async fn ios_empty_ad_without_identify_yields_no_candidate() {
        // AC-6: an iOS ad with no servable identification data must be dropped
        // (no candidate) without panic — defensive parse on both carriers.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.simulate_ios();
        let t = BleTransport::new_ios(Some(adapter.clone()));
        adapter.inject_scan_result(BleAddress([0x22; 6]), Vec::new(), -60);
        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let got = tokio::time::timeout(Duration::from_secs(1), stream.next()).await;
        assert!(
            got.unwrap().is_none(),
            "empty identify read => no candidate (no panic, AC-6)"
        );
    }

    #[tokio::test]
    async fn android_and_ios_carriers_share_one_beacon_parser() {
        // AC-3/AC-12 (DEC-BLE-002-0008): the SAME 22-byte beacon produces the
        // same candidate whether carried in ad service data (Android) or served
        // from the identification characteristic (iOS) — one parser, two
        // carriers.
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::ADVERTISE_BROADCAST);
        let beacon = DiscoveryBeacon::build(caps, [9u8; 16], 99);
        // Android leg: ad-carried payload.
        let android = std::sync::Arc::new(SimulatedBleAdapter::new());
        android.inject_scan_result(BleAddress([0x01; 6]), beacon.clone(), -50);
        let ta = BleTransport::new(Some(android.clone()));
        let mut sa = ta.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let pa = tokio::time::timeout(Duration::from_secs(1), sa.next())
            .await
            .unwrap()
            .unwrap();
        // iOS leg: characteristic-carried payload, empty ad.
        let ios = std::sync::Arc::new(SimulatedBleAdapter::new());
        ios.simulate_ios();
        ios.inject_identify_read(beacon.clone());
        ios.inject_scan_result(BleAddress([0x02; 6]), Vec::new(), -55);
        let ti = BleTransport::new_ios(Some(ios.clone()));
        let mut si = ti.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let pi = tokio::time::timeout(Duration::from_secs(1), si.next())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pa.peer_id, pi.peer_id, "one parser, two carriers");
    }

    #[tokio::test]
    async fn ios_advertising_drops_service_data_on_the_wire() {
        // AC-3 (RES-0024 DI-1): `startAdvertising(_:)` transmits ONLY local
        // name + service UUIDs — NEVER service data. The transport passes the
        // beacon as service_data; the iOS adapter drops it at the FFI boundary
        // and serves the beacon via IRIS_IDENTIFY_CHARACTERISTIC instead.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.simulate_ios();
        let t = BleTransport::new_ios(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([7u8; 32]),
            public_ip_addr: None,
            hostname: Some("iris-node".to_string()),
            tags: Vec::new(),
        };
        t.start_advertising(adv).await.unwrap();
        let received = adapter
            .last_ad_received()
            .expect("transport passed ad data");
        assert!(
            !received.service_data.is_empty(),
            "transport still passes the beacon as service_data (shared carrier)"
        );
        assert_eq!(received.service_uuid, Some(IRIS_SERVICE_UUID));
        let wire = adapter.last_ad_wire().expect("wire form recorded");
        assert!(
            wire.service_data.is_empty(),
            "iOS wire form carries NO service data (RES-0024 DI-1)"
        );
        assert_eq!(wire.service_uuid, Some(IRIS_SERVICE_UUID));
        assert_eq!(wire.local_name.as_deref(), Some("iris-node"));
    }

    #[tokio::test]
    async fn ios_mtu_payload_sizes_frames_to_full_reported_budget() {
        // AC-2 (DEC-BLE-002-0004): the iOS leg honors maximumWriteValueLength
        // (ATT payload, cap 512) — connect() translates payload → MTU so frames
        // reach the FULL reported 512-B budget (512, not 512−5).
        let adapter = std::sync::Arc::new(IosPayloadAdapter {
            reported: 512,
            ..Default::default()
        });
        let t = BleTransport::new_ios(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([2u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();
        let payload: Vec<u8> = (0..1_500u32).map(|i| (i % 251) as u8).collect();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([3u8; 16]),
            priority: MessagePriority::P2,
            payload: payload.clone(),
        };
        t.send(&peer.peer_id, &msg).await.unwrap();
        let frames: Vec<Vec<u8>> = adapter
            .inner
            .last_writes()
            .iter()
            .map(|(w, _)| w.data.clone())
            .collect();
        assert!(frames.len() >= 2, "1.5 KB must chunk");
        assert!(
            frames.iter().all(|f| f.len() <= 512),
            "frames must honor the FULL 512-B reported payload budget"
        );
        assert_eq!(
            frames[0].len(),
            512,
            "first full frame must reach the 512-B budget (header 6 + data 506), not 507"
        );
    }

    #[tokio::test]
    async fn ios_degraded_20b_payload_still_delivers_end_to_end() {
        // AC-2 (RES-0024 RQ-3/DI-5): iOS 16.0/16.0.1 reported a 20-B negotiated
        // payload (fixed 16.1) — the iOS leg treats it as degraded-but-
        // functional: frames chunk small and delivery still completes, never
        // an error and never a retry loop.
        let adapter = std::sync::Arc::new(IosPayloadAdapter {
            reported: 20,
            ..Default::default()
        });
        let t = BleTransport::new_ios(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([4u8; 32]), "AA:BB:CC:DD:EE:FF");
        t.connect(&peer).await.unwrap();
        let payload = vec![7u8; 100];
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([5u8; 16]),
            priority: MessagePriority::P2,
            payload: payload.clone(),
        };
        t.send(&peer.peer_id, &msg).await.unwrap();
        let frames: Vec<Vec<u8>> = adapter
            .inner
            .last_writes()
            .iter()
            .map(|(w, _)| w.data.clone())
            .collect();
        assert!(
            frames.len() >= 4,
            "100 B at a 20-B payload (14-B data/frame) must chunk many frames"
        );
        assert!(
            frames.iter().all(|f| f.len() <= 20),
            "degraded frames must respect the 20-B payload budget"
        );
        // The degraded link still delivers end-to-end through the poller.
        for f in frames {
            adapter
                .inner
                .inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, f);
        }
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        let back = got.expect("degraded link must still deliver").unwrap();
        assert_eq!(back.payload, payload);
        assert_eq!(back.peer_id, PeerId([4u8; 32]));
    }

    #[tokio::test]
    async fn ios_leg_registers_and_is_selectable_via_transport_manager() {
        // AC-1: the iOS leg registers + selects via TransportManager like any
        // transport (platform legs are distinct transports in the manager).
        use crate::message::MessagePriority as Mp;
        use crate::transport::manager::{TransportManager, TransportSelectionRequest};

        let mgr = TransportManager::new();
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.simulate_ios();
        let t = std::sync::Arc::new(BleTransport::new_ios(Some(adapter.clone())));
        mgr.register(t.clone()).await.unwrap();
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
        assert_eq!(
            selected.len(),
            1,
            "iOS BLE transport must be eligible (AC-1)"
        );
        assert_eq!(selected[0].transport_id.as_str(), "ble-ios");
        assert!(selected[0].score > 0.0);
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
                IRIS_WRITE_CHARACTERISTIC,
                encode_frame(id, 0, 2, 600, &[0u8; 200]).unwrap(),
            );
        }
        // Wait past the 1s sweep cadence; the partials must be evicted.
        tokio::time::sleep(Duration::from_millis(1500)).await;
        // A fresh complete message now flows end-to-end.
        let payload: Vec<u8> = (0..300u32).map(|i| (i % 251) as u8).collect();
        let frame = encode_frame(50_000, 0, 1, payload.len(), &payload).unwrap();
        adapter.inject_write(GattHandle(1), IRIS_WRITE_CHARACTERISTIC, frame);
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
        assert_eq!(t.pollers.read().unwrap_or_else(|p| p.into_inner()).len(), 1, "exactly one poller");
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
            adapter.inject_write(ha, IRIS_WRITE_CHARACTERISTIC, f.clone());
        }
        // Single-chunk frame for peer B.
        let payload_b = b"b-only".to_vec();
        let fb = encode_frame(1, 0, 1, payload_b.len(), &payload_b).unwrap();
        adapter.inject_write(hb, IRIS_WRITE_CHARACTERISTIC, fb);

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
        assert_ne!(err, TransportError::HardwareUnavailable);
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
            t.connections.lock().unwrap_or_else(|p| p.into_inner()).len(),
            0,
            "dead peer must be removed from connections (BLE-RT-005)"
        );
        assert_eq!(t.pollers.read().unwrap_or_else(|p| p.into_inner()).len(), 0, "poller aborted");
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
        let stopped = adapter.stopped_adv.lock().unwrap_or_else(|p| p.into_inner());
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
        fn gatt_read(&self, h: GattHandle, c: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(h, c)
        }
        fn set_mtu(&self, handle: GattHandle, _mtu: u16) -> Result<u16, BleError> {
            // Legacy device keeps MTU 23; modern peer negotiates 517.
            Ok(if handle == GattHandle(1) {
                MTU_DEFAULT
            } else {
                MTU_NEGOTIATED
            })
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    /// `connect_gatt` blocks for `delay` when dialling `slow_mac`, simulating
    /// a real ACL handshake that takes real wall-clock time (BLE-2). Every
    /// other op forwards to the inner simulated adapter unchanged.
    struct SlowConnectAdapter {
        inner: SimulatedBleAdapter,
        slow_mac: BleAddress,
        delay: Duration,
    }

    impl BleAdapter for SlowConnectAdapter {
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
            if a == self.slow_mac {
                std::thread::sleep(self.delay);
            }
            self.inner.connect_gatt(a)
        }
        fn disconnect_gatt(&self, h: GattHandle) {
            self.inner.disconnect_gatt(h)
        }
        fn gatt_write(&self, h: GattHandle, c: Uuid, d: Vec<u8>) -> Result<(), BleError> {
            self.inner.gatt_write(h, c, d)
        }
        fn gatt_read(&self, h: GattHandle, c: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(h, c)
        }
        fn set_mtu(&self, h: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(h, mtu)
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    /// BLE-2: a slow/unreachable peer's `connect()` must not stall a
    /// concurrent `connect()` to a DIFFERENT peer. Before the fix, both
    /// calls serialized on one `connections` lock held across the entire
    /// blocking `connect_gatt` FFI round trip, so the fast peer's connect
    /// could not complete until the slow one did — real behaviour observed
    /// on physical hardware as every message stalling behind one bad dial.
    #[tokio::test]
    async fn ble2_slow_connect_to_one_peer_does_not_stall_another() {
        let slow_mac = BleAddress([0xAA; 6]);
        let adapter = std::sync::Arc::new(SlowConnectAdapter {
            inner: SimulatedBleAdapter::new(),
            slow_mac,
            delay: Duration::from_millis(500),
        });
        let t = BleTransport::new(Some(adapter));
        let slow_peer = peer_with_mac(PeerId([0x40; 32]), "AA:AA:AA:AA:AA:AA");
        let fast_peer = peer_with_mac(PeerId([0x41; 32]), "BB:BB:BB:BB:BB:BB");

        let slow_fut = t.connect(&slow_peer);
        // Give the slow connect a head start so it is genuinely holding
        // whatever it holds before the fast one starts.
        tokio::time::sleep(Duration::from_millis(20)).await;
        let start = std::time::Instant::now();
        t.connect(&fast_peer)
            .await
            .expect("fast peer connect must succeed");
        let fast_elapsed = start.elapsed();
        assert!(
            fast_elapsed < Duration::from_millis(300),
            "connect to a different, healthy peer must not wait on a slow peer's \
             connect_gatt (BLE-2): took {fast_elapsed:?}"
        );
        slow_fut.await.expect("slow peer connect must also succeed");
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
        fn gatt_read(&self, h: GattHandle, c: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(h, c)
        }
        fn set_mtu(&self, h: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(h, mtu)
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
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
        fn gatt_read(&self, h: GattHandle, c: Uuid) -> Result<Vec<u8>, BleError> {
            self.inner.gatt_read(h, c)
        }
        fn set_mtu(&self, h: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(h, mtu)
        }
        fn drain_gatt_writes(&self, handle: GattHandle) -> Vec<GattWriteEvent> {
            self.inner.drain_gatt_writes(handle)
        }
        fn scan_results(&self) -> std::sync::MutexGuard<'_, Vec<ScanResult>> {
            self.inner.scan_results()
        }
    }

    /// BLE-RT-C001 regression: the Android leg NEVER probe-connects empty-payload
    /// (non-IRIS) advertising — connect-to-identify is iOS-leg only.
    #[tokio::test]
    async fn c001_android_leg_never_probe_connects_empty_ads() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        // Non-IRIS ambient advertising: empty payloads (no IRIS beacon).
        for i in 0..6u8 {
            adapter.inject_scan_result(BleAddress([i, 0xAA, 0, 0, 0, 0]), Vec::new(), -45);
        }
        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }
        assert_eq!(
            count, 0,
            "Android leg must not yield candidates from empty ads"
        );
        assert_eq!(
            adapter.connect_count(),
            0,
            "Android leg must never probe-connect for discovery (BLE-RT-C001)"
        );
    }

    /// BLE-RT-C001/003 regression: the iOS-leg probe budget caps connect/read/
    /// disconnect cycles per discovery pass at the ~8-peer practical ceiling.
    #[tokio::test]
    async fn c001_ios_leg_probe_budget_caps_probe_connects() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.simulate_ios();
        let t = BleTransport::new_ios(Some(adapter.clone()));
        // Seed identify values for peers 0..=9; inject 20 empty ads so the
        // probe path would otherwise connect far beyond the ceiling.
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        for i in 0..10u8 {
            let beacon = DiscoveryBeacon::build(caps, [i; 16], 1);
            adapter.inject_identify_read(beacon.clone());
            adapter.inject_scan_result(BleAddress([i, 0xBB, 0, 0, 0, 0]), Vec::new(), -50);
        }
        for i in 10..20u8 {
            adapter.inject_scan_result(BleAddress([i, 0xBB, 0, 0, 0, 0]), Vec::new(), -50);
        }
        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let mut count = 0;
        while stream.next().await.is_some() {
            count += 1;
        }
        assert!(
            count <= 8,
            "iOS-leg probe budget must bound candidates (BLE-RT-C001/003), got {count}"
        );
        assert!(
            adapter.connect_count() <= 8,
            "iOS-leg probe budget must bound GATT connects, got {}",
            adapter.connect_count()
        );
    }

    /// BLE-RT-C005 regression: the sim's GATT server serves the beacon this
    /// node ADVERTISED (advertise → read handoff), so a connect-to-identify
    /// probe against a simulated peer resolves the peer's real advertised
    /// beacon without a test-only injection setter.
    #[tokio::test]
    async fn c005_sim_serves_advertised_beacon_on_identify_read() {
        let peer = std::sync::Arc::new(SimulatedBleAdapter::new());
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        let beacon = DiscoveryBeacon::build(caps, [0xC5; 16], 77);
        peer.start_advertising(AdvertisementData {
            local_name: Some("iris-peer".to_string()),
            service_uuid: Some(IRIS_SERVICE_UUID),
            service_data: beacon.clone(),
            connectable: true,
        })
        .unwrap();
        // iOS leg probes the peer's identify characteristic directly.
        let handle = peer.connect_gatt(BleAddress([0xC5; 6])).unwrap();
        let served = peer
            .gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC)
            .expect("identify read must be served");
        peer.disconnect_gatt(handle);
        assert_eq!(
            served, beacon,
            "sim GATT server must serve exactly the advertised beacon (BLE-RT-C005)"
        );
        // iOS-mode advertising (empty service data) does NOT clobber the
        // served identify value — the beacon still rides the characteristic.
        let peer_ios = std::sync::Arc::new(SimulatedBleAdapter::new());
        peer_ios.simulate_ios();
        peer_ios
            .start_advertising(AdvertisementData {
                local_name: Some("iris-peer".to_string()),
                service_uuid: Some(IRIS_SERVICE_UUID),
                service_data: beacon.clone(),
                connectable: true,
            })
            .unwrap();
        let h2 = peer_ios.connect_gatt(BleAddress([0xC6; 6])).unwrap();
        let served2 = peer_ios
            .gatt_read(h2, IRIS_IDENTIFY_CHARACTERISTIC)
            .expect("ios-mode identify read must be served");
        assert_eq!(served2, beacon, "ios-mode serve path must hold");
    }
}
