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
    DiscoveryConfig, IncomingMessage, LinkQuality, MessagePriority, NodeAdvertisement, PeerId,
    PeerInfo, SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::ble_advert::{BEACON_LEN, CapabilityBits, DiscoveryBeacon};
use crate::transport::ble_att::{
    AttSegmenter, BLE_MAX_MESSAGE_CONSERVATIVE, Reassembler, MAX_MESSAGE_BYTES, REASSEMBLY_TTL,
};
use crate::transport::{
    AtomicState, EwmaGoodput, Transport, TransportCapabilities, TransportCost, TransportCostClass,
    TransportId, TransportState, TransportStateEvent,
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

/// Minimum RSSI (dBm) for a scan candidate to be admitted (BLE-26).
///
/// HV-12: was -85 dBm, described as "~10 m open office" — and it did exactly
/// what that implies: "it only works when the phones are right next to each
/// other". A phone in a pocket or one interior wall away is routinely -88…-95
/// dBm and was filtered out. This is the *single* source: the Android
/// `FfiScanFilter.rssi_floor` is threaded from here (`ScanFilter::rssi_threshold`
/// → the bridge), and the Android adapter's own default now matches it (it no
/// longer starts at -127 "accept everything" and only picks up a real floor
/// inside a *successful* `startScan`). -95 keeps genuinely unusable links
/// (deep multi-wall / far out of range, typically < -98) out while admitting
/// the through-a-wall / in-a-bag links a resilience mesh must carry. Tunable
/// per platform later if a specific radio needs it.
pub const RSSI_FLOOR_DBM: i32 = -95;

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
    /// BLE-1: drain handles the OS has disconnected since the last call.
    ///
    /// Real platform bridges push each handle into an internal queue as the
    /// OS fires its link-down callback (CBCentralManager `didDisconnect` on
    /// iOS, `onConnectionStateChange(DISCONNECTED)` on Android). The accept
    /// poller drains this once per tick and tears down the corresponding peer
    /// entry so the manager stops routing to a dead link. Default returns
    /// empty so all existing implementors keep compiling unchanged.
    fn drain_disconnected_handles(&self) -> Vec<GattHandle> {
        Vec::new()
    }
    /// HV-11: `ScanCallback.onScanFailed` error codes fired since the last
    /// call. Android's async scan-failure callback is otherwise invisible to
    /// the core — a `SCAN_FAILED_SCANNING_TOO_FREQUENTLY` (6),
    /// `_APPLICATION_REGISTRATION_FAILED` (2) or `_INTERNAL_ERROR` (3) leaves
    /// the transport believing a scan is live when the OS refused it. Default
    /// empty so non-Android implementors keep compiling.
    fn drain_scan_failures(&self) -> Vec<i32> {
        Vec::new()
    }
    /// HV-10/HV-31: adapter-lifecycle events the platform surfaced since the
    /// last call, otherwise invisible to the core:
    ///   1 = advertising failed and the platform-side bounded retry gave up
    ///   2 = Bluetooth adapter turned OFF (all scan/advertise/GATT handles dead)
    ///   3 = Bluetooth adapter turned ON — the platform adapter has replayed
    ///       advertise + scan; the transport can leave `Unavailable`
    /// Default empty so non-Android implementors keep compiling.
    /// HV-21/DEC-BLE-0008: set the bytes an inbound `gatt_read` of
    /// `IRIS_IDENTIFY_CHARACTERISTIC` should serve — the local node's own
    /// beacon, rebuilt with the real Wi-Fi Direct MAC folded in when known
    /// (unlike the advertisement, which stays legacy-sized, this GATT path
    /// has no ~31-byte ceiling). Called from `start_advertising` and again
    /// from `set_local_wifi_direct_mac`. Default no-op: only Android's real
    /// GATT server (and iOS's, once implemented) needs to act on it —
    /// `SimulatedBleAdapter`'s many test-local delegating wrappers keep
    /// compiling unchanged.
    fn set_identify_payload(&self, _data: Vec<u8>) {}

    fn drain_adapter_events(&self) -> Vec<i32> {
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
    /// BLE-24: per-address identify data for multi-peer tests; looked up by
    /// handle→address before falling back to the global `identify_data` slot.
    identify_by_address: std::sync::Mutex<std::collections::HashMap<BleAddress, Vec<u8>>>,
    /// BLE-24: maps each allocated GattHandle back to the BleAddress it was
    /// opened for, so `gatt_read` can serve the right per-peer beacon.
    handle_to_address: std::sync::Mutex<std::collections::HashMap<GattHandle, BleAddress>>,
    /// BLE-1: queue of handles the simulated OS has disconnected — drained by
    /// `drain_disconnected_handles`. Tests call `simulate_disconnect` to inject
    /// a link-down event exactly as a real OS bridge would.
    disconnected: std::sync::Mutex<Vec<GattHandle>>,
    /// HV-92: number of upcoming `gatt_write` calls that fail with the Android
    /// "characteristic not yet discovered" `GattFailure` — models a connection
    /// whose GATT DB was still incomplete when `connect_gatt` returned `Ok`.
    unready_writes: std::sync::atomic::AtomicU32,
    /// HV-7: number of upcoming `gatt_write` calls that fail with a *transient*
    /// `GattFailure` (write-completion timeout) then recover — models a
    /// momentary RF dropout / stack contention mid-message.
    transient_write_failures: std::sync::atomic::AtomicU32,
    /// HV-11: queued `onScanFailed` error codes, drained by `drain_scan_failures`.
    scan_failures: std::sync::Mutex<Vec<i32>>,
    /// HV-10/HV-31: queued adapter-lifecycle codes, drained by
    /// `drain_adapter_events` (1 = advertising gave up, 2 = BT off, 3 = BT on).
    adapter_events: std::sync::Mutex<Vec<i32>>,
    /// HV-95: fail the next `n` `start_advertising` calls (wedged GATT server).
    advertise_fail_first: std::sync::Mutex<u32>,
    /// HV-30: when set, `start_scan` / `start_advertising` fail with
    /// `PermissionDenied` (models a runtime grant revoked mid-session).
    permission_revoked: std::sync::atomic::AtomicBool,
    /// HV-93/HV-98: accepted connections (a remote central dialled our GATT
    /// server) re-reported on every `accepted_connections()` drain until
    /// cleared — exactly how the Kotlin adapter re-announces on a write-gap.
    accepted: std::sync::Mutex<Vec<AcceptedConnection>>,
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
    /// BLE-1: inject a simulated OS disconnect event for `handle`. The next
    /// `drain_disconnected_handles` call will return it, causing the accept
    /// poller to tear down the peer — exactly as a real platform bridge would
    /// when `didDisconnect`/`onConnectionStateChange(DISCONNECTED)` fires.
    /// HV-93/HV-98: a remote central dialled our GATT server. It is re-reported
    /// on every `accepted_connections()` drain (mirroring the Kotlin adapter's
    /// write-gap re-announce) until `clear_accepted` — so the accept-poller can
    /// re-spawn the inbound reassembly poller for a peer that re-linked after a
    /// `close_peer`.
    #[cfg(any(test, feature = "test-support"))]
    pub fn inject_accepted_connection(&self, handle: GattHandle, address: BleAddress) {
        let mut a = self.accepted.lock().unwrap_or_else(|p| p.into_inner());
        if !a.iter().any(|c| c.handle == handle) {
            a.push(AcceptedConnection { handle, address });
        }
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn clear_accepted(&self, handle: GattHandle) {
        self.accepted
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .retain(|c| c.handle != handle);
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_disconnect(&self, handle: GattHandle) {
        self.disconnected
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(handle);
    }
    /// BLE-24: inject a per-address identify beacon. When a GATT connection is
    /// opened to `addr`, `gatt_read` will serve `data` instead of the global
    /// slot, so multi-peer tests can assert which specific peers were resolved.
    pub fn inject_identify_read_for_address(&self, addr: BleAddress, data: Vec<u8>) {
        self.identify_by_address
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(addr, data);
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
    /// HV-92: make the next `n` `gatt_write` calls fail with the Android
    /// "characteristic not yet discovered" `GattFailure`, as a real link does
    /// when `onServicesDiscovered` returned an incomplete GATT database.
    pub fn fail_next_writes_unready(&self, n: u32) {
        self.unready_writes
            .store(n, std::sync::atomic::Ordering::Relaxed);
    }
    /// HV-7: make the next `n` `gatt_write` calls fail with a *transient*
    /// `GattFailure` (write never completed) then recover — the per-frame retry
    /// in `send()` should ride over it without tearing the link down.
    pub fn fail_next_writes_transient(&self, n: u32) {
        self.transient_write_failures
            .store(n, std::sync::atomic::Ordering::Relaxed);
    }
    /// HV-11: inject an `onScanFailed(code)` the transport will observe on its
    /// next `discover_peers` via `drain_scan_failures`.
    pub fn inject_scan_failure(&self, code: i32) {
        self.scan_failures
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(code);
    }
    /// HV-10/HV-31: inject an adapter-lifecycle event the transport will observe
    /// on its next `discover_peers` via `drain_adapter_events` (1 = advertising
    /// retries exhausted, 2 = BT off, 3 = BT on / recovered).
    pub fn inject_adapter_event(&self, code: i32) {
        self.adapter_events
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(code);
    }
    /// HV-95: make the next `n` `start_advertising` calls fail (wedged GATT
    /// server) — the core watchdog should re-drive it on a `poll_health` tick.
    pub fn fail_next_advertise(&self, n: u32) {
        *self.advertise_fail_first.lock().unwrap_or_else(|p| p.into_inner()) = n;
    }
    /// HV-30: revoke (`true`) / restore (`false`) the runtime BLE permission —
    /// `start_scan` / `start_advertising` fail `PermissionDenied` while revoked.
    pub fn set_permission_revoked(&self, revoked: bool) {
        self.permission_revoked
            .store(revoked, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl BleAdapter for SimulatedBleAdapter {
    fn start_scan(&self, _filter: ScanFilter) -> Result<ScanHandle, BleError> {
        if self.permission_revoked.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(BleError::PermissionDenied);
        }
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
        if self.permission_revoked.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(BleError::PermissionDenied);
        }
        // HV-95: model a wedged stack that refuses to open the GATT server for
        // the first `n` calls (Kotlin's `startAdvertising` throws `Transport`
        // when `ensureGattServer` fails).
        {
            let mut n = self.advertise_fail_first.lock().unwrap_or_else(|p| p.into_inner());
            if *n > 0 {
                *n -= 1;
                return Err(BleError::GattFailure("BLE GATT server unavailable".into()));
            }
        }
        *self.last_ad_raw.lock().unwrap_or_else(|p| p.into_inner()) = Some(data.clone());
        // HV-21/DEC-BLE-0008 correction: this used to mirror the advertised
        // `service_data` straight into `identify_data` on the theory that
        // "the beacon the peer reads is the beacon this node advertises"
        // (BLE-RT-C005) — true when there was only one beacon. Now
        // `ble.rs::start_advertising` calls `set_identify_payload` with a
        // DELIBERATELY different (potentially MAC-carrying) payload
        // immediately before this call; mirroring here would silently
        // clobber that back to the legacy-sized advertised form on every
        // (re-)advertise, which is exactly backwards. `set_identify_payload`
        // is the only path that sets `identify_data` now — tests that want
        // the old "identify mirrors the advert" behaviour (no MAC involved)
        // should call `inject_identify_read` explicitly.
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
    fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError> {
        *self.connect_calls.lock().unwrap_or_else(|p| p.into_inner()) += 1;
        // Distinct handle per connection so multi-peer tests can attribute
        // inbound writes to the right link (BLE-RT-016).
        let h = GattHandle(
            self.next_handle
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                + 1,
        );
        self.handles.lock().unwrap_or_else(|p| p.into_inner()).push(h);
        // BLE-24: record handle→address so gatt_read can serve per-peer beacons.
        self.handle_to_address
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(h, address);
        Ok(h)
    }
    fn disconnect_gatt(&self, _handle: GattHandle) {}
    /// BLE-1: override so the accept poller actually sees simulated link-down
    /// events. Drains the queue injected by `simulate_disconnect`.
    fn drain_disconnected_handles(&self) -> Vec<GattHandle> {
        std::mem::take(
            &mut *self.disconnected.lock().unwrap_or_else(|p| p.into_inner()),
        )
    }
    fn drain_scan_failures(&self) -> Vec<i32> {
        std::mem::take(&mut *self.scan_failures.lock().unwrap_or_else(|p| p.into_inner()))
    }
    fn drain_adapter_events(&self) -> Vec<i32> {
        std::mem::take(&mut *self.adapter_events.lock().unwrap_or_else(|p| p.into_inner()))
    }
    fn accepted_connections(&self) -> Vec<AcceptedConnection> {
        self.accepted.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn gatt_write(
        &self,
        handle: GattHandle,
        char_uuid: Uuid,
        data: Vec<u8>,
    ) -> Result<(), BleError> {
        // HV-92: a connection whose GATT DB was still incomplete when
        // `connect_gatt` returned rejects writes until discovery catches up.
        if self
            .unready_writes
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| if n > 0 { Some(n - 1) } else { None },
            )
            .is_ok()
        {
            return Err(BleError::GattFailure(format!(
                "characteristic {char_uuid:?} not yet discovered"
            )));
        }
        // HV-7: transient write failure that recovers on retry.
        if self
            .transient_write_failures
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |n| if n > 0 { Some(n - 1) } else { None },
            )
            .is_ok()
        {
            return Err(BleError::GattFailure(
                "gatt write initiated but onCharacteristicWrite never fired within 5000ms".into(),
            ));
        }
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
    fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError> {
        // Only the IRIS identification characteristic exists on the simulated
        // server; reads of anything else fail like an absent characteristic.
        if char_uuid != IRIS_IDENTIFY_CHARACTERISTIC {
            return Err(BleError::DeviceNotFound);
        }
        // BLE-24: prefer per-address beacon (set via inject_identify_read_for_address)
        // so multi-peer tests can assert which specific peers were resolved.
        let addr = self
            .handle_to_address
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&handle)
            .copied();
        if let Some(a) = addr {
            if let Some(data) = self
                .identify_by_address
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .get(&a)
                .cloned()
            {
                return Ok(data);
            }
        }
        Ok(self.identify_data.lock().unwrap_or_else(|p| p.into_inner()).clone())
    }
    fn set_identify_payload(&self, data: Vec<u8>) {
        *self.identify_data.lock().unwrap_or_else(|p| p.into_inner()) = data;
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
    /// HV-91: `discover_peers` call count. The scan, once started, keeps
    /// running — so most passes just harvest results (fast, cheap on the
    /// throttle). Every `SCAN_REARM_EVERY` passes it is torn down and
    /// restarted as a dead-scan safety net (a scan the OS silently killed —
    /// e.g. Bluetooth toggled — is otherwise never noticed; HV-11).
    scan_pass: std::sync::atomic::AtomicU64,
    adv_handle: std::sync::Mutex<Option<AdvHandle>>,
    /// HV-31: the last `NodeAdvertisement` handed to `start_advertising`, kept so
    /// the transport can re-advertise itself after a Bluetooth off→on toggle
    /// (the Kotlin adapter tears the OS-level advertiser down; nothing in the
    /// core loop otherwise re-drives advertising — it is a one-shot at
    /// `start_all`).
    last_advertisement: std::sync::Mutex<Option<NodeAdvertisement>>,
    /// HV-21: this device's own Wi-Fi Direct MAC, once
    /// `set_local_wifi_direct_mac` has been told it — folded into the
    /// discovery beacon (DEC-BLE-0008) so a peer can match a plain
    /// `discoverPeers()` result back to an already-BLE-known IRIS peer.
    wifi_direct_mac: std::sync::Mutex<Option<[u8; 6]>>,
    /// GATT handles + negotiated ATT MTU per connected peer — one connection
    /// per peer is reused for all sends (AC-9; RES-0019 R6.4/G2 bounds
    /// GATT-client churn). MTU is per-link (BLE-RT-003), never global.
    /// HW-9: `Arc`-wrapped (was a bare `Mutex`) so the accept-poller task
    /// spawned in `start_advertising` — which must outlive any single
    /// `&self` call and therefore cannot borrow `self` — can hold a cheap
    /// clone of the SAME map `send()`/`shutdown()` read, instead of a
    /// snapshot that would immediately go stale.
    /// BLE-25: tuple is (handle, mtu, per-connection-next-msg-id). The third
    /// field replaces the shared AttSegmenter counter so concurrent sends to
    /// different peers cannot collide on the same id space.
    connections:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PeerId, (GattHandle, u16, u16)>>>,
    /// HV-27: last time a `gatt_write` to this peer succeeded. Drives
    /// `link_quality()` (→ `NeighborTable` → `/diag`) so a link that has gone
    /// quiet reads Fair/Poor instead of a hardcoded `Good`. Removed by
    /// `close_peer`.
    link_activity: std::sync::Mutex<std::collections::HashMap<PeerId, tokio::time::Instant>>,
    /// HV-107: GATT handles for connections a REMOTE central dialed to OUR GATT
    /// server (inbound / peripheral role). Kept strictly separate from
    /// `connections` (outbound-only — client links THIS node dialed). An inbound
    /// peripheral handle can never carry a client `gatt_write`, so it must not
    /// live in `connections`: doing so made `connect()` see the peer as "already
    /// connected" and skip the real outbound dial, and `send()` resolve to a
    /// handle it could not write — so after the peer restarted (RETRY /
    /// permission re-grant, where the OEM GATT server never fires a disconnect)
    /// this node kept receiving from the peer but could never send or ACK back.
    /// Consulted only by the accept-poller's reassembly path; torn down by
    /// `close_peer` / `shutdown` / `drop_all_links` alongside `connections`.
    inbound_handles:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<PeerId, GattHandle>>>,
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
    /// HW-9: best-known candidate `PeerId` for every `BleAddress` seen in a
    /// discovery beacon (`discover_peers`, DEC-BLE-0006 candidate hint,
    /// same semantics `send()`/HW-5 already treat candidate ids with) —
    /// consulted by the accept-poller to attribute an inbound connection it
    /// did not itself dial, since Android's peripheral role never gives the
    /// GATT server a beacon/identity for the central that just connected to
    /// it, only its MAC address.
    /// HV-13: value carries the last-seen `Instant` so `discover_peers` can
    /// sweep stale MAC→candidate hints (a MAC the OS has since rotated, a peer
    /// long gone) and cap the map — it used to grow unbounded across a
    /// long-lived node's neighbour churn and could hand the accept-poller a
    /// candidate id for a MAC that now belongs to a different device.
    known_addresses:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<BleAddress, (PeerId, std::time::Instant)>>>,
    /// HW-9: the single long-lived task that drains `accepted_connections()`
    /// and spawns a reassembly poller for each new one. One per transport
    /// instance, started (idempotently) from `start_advertising` — the
    /// natural point at which this node becomes dialable at all.
    accept_poller: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
    /// HV-98: GATT handles the accept-poller has already spawned an inbound
    /// reassembly poller for. Was a task-local `HashSet` inside the
    /// accept-poller closure — unreachable from `close_peer`, so a peer that
    /// re-linked after a drop (its inbound poller aborted by our own
    /// `close_peer` / `drop_all_links`) was never re-spawned because its
    /// handle still looked "spawned". Shared so `close_peer` can prune it.
    accept_spawned: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<GattHandle>>>,
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
    /// MG-17: EWMA goodput tracker.
    ewma: EwmaGoodput,
}

/// Scan-restart ceiling: max scan starts in the window.
const SCAN_CEILING: usize = 5;
/// Backoff window length (RES-0019: >=5 start/stop per 30 s → silent failure).
const SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
/// Backoff cap (Android 17 hardens throttling; exponential 1s..=30s).
const SCAN_BACKOFF_MAX_S: u64 = 30;

/// HV-13: `known_addresses` (MAC → candidate id hint for the accept-poller) is
/// swept on every `discover_peers` pass. An entry older than this — a MAC the
/// OS has since rotated (BLE RPAs rotate every ~15 min), a peer long gone — is
/// stale and could misattribute a re-used MAC. Matches the discovery
/// neighbour TTL family.
const KNOWN_ADDR_TTL: std::time::Duration = std::time::Duration::from_secs(300);
/// Hard cap regardless of age, so a burst of ambient MACs cannot bloat the map
/// between sweeps; oldest entries are dropped first.
const KNOWN_ADDR_CAP: usize = 128;

// SYS-2: deadline constants for radio adapter calls (connect / MTU).
// A hung connectGatt call blocks the node indefinitely without these.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MTU_TIMEOUT: Duration = Duration::from_secs(10);
/// BLE-14: per-probe deadline (connect + read + disconnect). CoreBluetooth's
/// own connect timeout is ~5 s; cap the whole triple at 8 s so a stalled
/// probe cannot hold the executor indefinitely.
const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

impl BleTransport {
    pub fn new(adapter: Option<std::sync::Arc<dyn BleAdapter>>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = AtomicState::default();
        state.store(TransportState::Available);
        let caps = TransportCapabilities {
            // GAP-2: report the worst-case floor so the manager's eligibility filter
            // and fragmenter never overestimate what BLE can carry. Uses MTU_DEFAULT
            // (23 B) as the denominator; actual cap rises with the negotiated MTU.
            max_message_size: BLE_MAX_MESSAGE_CONSERVATIVE,
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
            scan_pass: std::sync::atomic::AtomicU64::new(0),
            adv_handle: std::sync::Mutex::new(None),
            wifi_direct_mac: std::sync::Mutex::new(None),
            last_advertisement: std::sync::Mutex::new(None),
            connections: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            link_activity: std::sync::Mutex::new(std::collections::HashMap::new()),
            inbound_handles: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            connect_locks: std::sync::Mutex::new(std::collections::HashMap::new()),
            known_addresses: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            accept_poller: std::sync::Mutex::new(None),
            accept_spawned: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            segmenter: AttSegmenter::new(),
            scan_times: std::sync::Mutex::new(std::collections::VecDeque::new()),
            scan_backoff_s: std::sync::Mutex::new(0),
            refused_until: std::sync::RwLock::new(None),
            ios_leg: false,
            dropped_inbound: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
            ewma: EwmaGoodput::new(),
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

    /// HV-30: a scan/advertise op that fails with a revoked runtime permission
    /// or a disabled radio is not transient — the transport is dead until the
    /// user acts. Mark it `Unavailable` so `select_transports` stops picking it
    /// (every send would fail + requeue) and the UI's UNAVAILABLE / RetryNotice
    /// path fires, instead of the UI showing RUNNING against a dark radio.
    /// Returns the error unchanged.
    fn demote_on_fatal(&self, err: TransportError) -> TransportError {
        if matches!(
            err,
            TransportError::PermissionDenied { .. }
                | TransportError::RadioDisabled
                | TransportError::HardwareUnavailable
        ) && self.state() != TransportState::Unavailable
        {
            tracing::warn!(
                event = "ble.transport_unavailable",
                reason = %err,
                "BLE op failed fatally (permission revoked / radio off) — transport Unavailable"
            );
            self.set_state(TransportState::Unavailable);
        }
        err
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
        // BLE-8: upper 16 bytes must be 0xFF sentinel to match candidate_peer_id()
        // in ble_advert.rs — connections are keyed by that exact form.
        let mut id = [0xFFu8; 32];
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

    /// HV-104: drain every live GATT handle's inbound frames — outbound links
    /// we dialed (`connections`) and inbound links a remote central dialed to
    /// our GATT server (`inbound_handles`) — into a per-handle `Reassembler`,
    /// then onto the incoming stream. Called once per poll tick by the single
    /// transport-wide poller (see `ensure_accept_poller`).
    ///
    /// Was one `tokio::spawn` **per connected peer** at a fixed 50 ms cadence
    /// (GAP-12): 8 peers = 8 tasks × 20 wakeups/s even fully idle, which also
    /// kept the SoC out of its deeper sleep states. Now one task, one fast/idle
    /// cadence keyed on whether *any* peer produced a frame recently — the same
    /// shape the Wi-Fi Aware transport already uses for its single poller.
    #[allow(clippy::too_many_arguments)]
    fn drain_inbound_once(
        incoming: &broadcast::Sender<IncomingMessage>,
        adapter: &std::sync::Arc<dyn BleAdapter>,
        connections: &std::sync::Arc<
            std::sync::Mutex<std::collections::HashMap<PeerId, (GattHandle, u16, u16)>>,
        >,
        inbound_handles: &std::sync::Arc<
            std::sync::Mutex<std::collections::HashMap<PeerId, GattHandle>>,
        >,
        recons: &mut std::collections::HashMap<GattHandle, Reassembler>,
        tid: &TransportId,
        dropped_inbound: &std::sync::Arc<std::sync::atomic::AtomicU64>,
        now: std::time::Instant,
    ) -> bool {
        // Snapshot every (handle, peer_id) currently live, de-duplicated by
        // handle (a handle should only ever be in one of the two maps, but a
        // resolve-vs-accept race could briefly put it in both — HV-107).
        let mut targets: std::collections::HashMap<GattHandle, PeerId> =
            std::collections::HashMap::new();
        for (pid, (h, ..)) in connections.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            targets.entry(*h).or_insert(*pid);
        }
        for (pid, h) in inbound_handles.lock().unwrap_or_else(|p| p.into_inner()).iter() {
            targets.entry(*h).or_insert(*pid);
        }
        // Drop reassemblers for handles that are no longer live.
        recons.retain(|h, _| targets.contains_key(h));

        let mut saw_frame = false;
        for (handle, peer_id) in targets {
            let mine = adapter.drain_gatt_writes(handle);
            if mine.is_empty() {
                continue;
            }
            saw_frame = true;
            tracing::debug!(
                event = "ble.inbound_drain",
                handle = handle.0,
                frames = mine.len(),
                "transport poller drained frames"
            );
            let recon = recons.entry(handle).or_insert_with(Reassembler::new);
            for w in mine {
                if w.char_uuid != IRIS_WRITE_CHARACTERISTIC {
                    tracing::debug!(event = "ble.inbound_wrong_char", handle = handle.0, "frame for another characteristic dropped");
                    continue;
                }
                match recon.push(&w.data, now) {
                    Ok(Some(payload)) => {
                        let n = incoming.receiver_count();
                        if incoming
                            .send(IncomingMessage {
                                peer_id,
                                transport_id: tid.0.clone(),
                                payload,
                                received_at: now,
                            })
                            .is_err()
                        {
                            dropped_inbound.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            tracing::warn!(event = "ble.inbound_no_subscriber", handle = handle.0, receivers = n, "reassembled message dropped — no subscriber");
                        } else {
                            tracing::debug!(event = "ble.inbound_delivered", handle = handle.0, receivers = n, "reassembled message pushed to engine");
                        }
                    }
                    Ok(None) => {
                        tracing::debug!(event = "ble.inbound_partial", handle = handle.0, "frame accepted, awaiting more");
                    }
                    Err(_) => {
                        tracing::debug!(event = "ble.inbound_malformed", handle = handle.0, "frame rejected by reassembler");
                    }
                }
            }
        }
        saw_frame
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
        let inbound_handles = self.inbound_handles.clone();
        let incoming_tx = self.incoming_tx.clone();
        let tid = self.id.clone();
        let dropped_inbound = self.dropped_inbound.clone();
        // HV-98: shared with `close_peer` so a peer that re-links after a drop
        // is picked up again. An accepted connection is re-reported on every
        // drain (Kotlin re-announces on a post-reconnect write-gap).
        let spawned = self.accept_spawned.clone();
        let task = tokio::spawn(async move {
            tracing::debug!(event = "ble.accept_poller_started", "transport poller task running");
            // HV-104: fast/idle cadence keyed on whether ANY handle produced a
            // frame recently — one task for the whole transport, not one per
            // peer (GAP-12). Accepted-connection pickup + disconnect drain run
            // on the slower 500 ms beat regardless (they are cheap and not
            // latency-critical).
            const FAST_POLL_MS: u64 = 50;
            const IDLE_POLL_MS: u64 = 500;
            const ACTIVE_WINDOW: Duration = Duration::from_secs(2);
            let mut recons: std::collections::HashMap<GattHandle, Reassembler> =
                std::collections::HashMap::new();
            let mut last_activity = std::time::Instant::now();
            let mut last_evict = std::time::Instant::now();
            let mut last_housekeep = std::time::Instant::now() - Duration::from_secs(1);
            loop {
                let fast = last_activity.elapsed() < ACTIVE_WINDOW;
                tokio::time::sleep(Duration::from_millis(if fast { FAST_POLL_MS } else { IDLE_POLL_MS })).await;
                let now = std::time::Instant::now();

                // --- housekeeping (every ~500 ms): pick up newly accepted
                //     inbound connections + drain OS link-down events ---
                if now.duration_since(last_housekeep) >= Duration::from_millis(500) {
                    last_housekeep = now;
                    for accepted in adapter.accepted_connections() {
                        if !spawned.lock().unwrap_or_else(|p| p.into_inner()).insert(accepted.handle) {
                            continue;
                        }
                        let peer_id = known_addresses
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .get(&accepted.address)
                            .map(|(pid, _)| *pid)
                            .unwrap_or_else(|| BleTransport::synthesize_unknown_peer_id(&accepted.address));
                        tracing::debug!(
                            event = "ble.accept_new_connection",
                            handle = accepted.handle.0,
                            "transport poller picked up a remote central's inbound link"
                        );
                        // HV-107: inbound handles get their OWN map, never
                        // `connections` (outbound client links only) — else
                        // `connect()` skips the real dial and `send()` can't reply.
                        inbound_handles
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .insert(peer_id, accepted.handle);
                    }
                    for dead in adapter.drain_disconnected_handles() {
                        spawned.lock().unwrap_or_else(|p| p.into_inner()).remove(&dead);
                        recons.remove(&dead);
                        // The dead handle may be outbound (`connections`) or
                        // inbound (`inbound_handles`, HV-107) — clear both.
                        connections
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .retain(|_, (h, _, _)| *h != dead);
                        inbound_handles
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .retain(|_, h| *h != dead);
                    }
                }

                // --- TTL sweep of every reassembler (BLE-RT-001) ---
                if now.duration_since(last_evict) >= Duration::from_secs(1) {
                    for r in recons.values_mut() {
                        r.evict_stale(REASSEMBLY_TTL, now);
                    }
                    last_evict = now;
                }

                // --- the hot path: drain every live handle once ---
                if BleTransport::drain_inbound_once(
                    &incoming_tx,
                    &adapter,
                    &connections,
                    &inbound_handles,
                    &mut recons,
                    &tid,
                    &dropped_inbound,
                    now,
                ) {
                    last_activity = std::time::Instant::now();
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
        if let Some((handle, ..)) = self.connections.lock().unwrap_or_else(|p| p.into_inner()).remove(peer) {
            adapter.disconnect_gatt(handle);
            // HV-98: forget this handle from the accept-poller's "already
            // spawned" set so that if the peer re-links (and Kotlin re-announces
            // the accepted connection on the post-reconnect write-gap), a fresh
            // inbound reassembly poller IS spawned — the OEM GATT server never
            // fires the disconnect callback that would otherwise clear it.
            self.accept_spawned.lock().unwrap_or_else(|p| p.into_inner()).remove(&handle);
        }
        // HV-107: an inbound (peripheral-role) handle for the same peer, if any,
        // is torn down on the same terms — otherwise a `drop_all_links` / RETRY
        // would leave the stale inbound poller running and its handle stuck in
        // `accept_spawned`, so the re-linking peer never gets a fresh poller.
        if let Some(handle) = self.inbound_handles.lock().unwrap_or_else(|p| p.into_inner()).remove(peer) {
            adapter.disconnect_gatt(handle);
            self.accept_spawned.lock().unwrap_or_else(|p| p.into_inner()).remove(&handle);
        }
        // Not load-bearing for correctness (a stale empty-Mutex entry just
        // gets reused on the next connect), only for not growing forever
        // across a long-lived node's peer churn.
        self.connect_locks.lock().unwrap_or_else(|p| p.into_inner()).remove(peer);
        // HV-105: do NOT evict this peer's MAC→candidate hint here. The inbound
        // accept-poller (`ensure_accept_poller`) resolves an incoming MAC to its
        // real candidate PeerId *through* `known_addresses`; if a transient drop
        // (forced link loss, peer RETRY / stopMesh+startMesh) evicts the hint
        // and the peer then re-links to our GATT server before our own next
        // scan re-sees its beacon, the poller falls back to a synthesized peer
        // id and the reconnected inbound frames never reassemble to a delivery.
        // HV-13's actual bug (unbounded growth + a MAC the OS later rotates) is
        // fully covered by the TTL (300 s) + cap (128) sweep in `discover_peers`
        // — a rotated MAC's stale hint expires on its own within 5 min. Session
        // 13's forced-drop recovery was ~3.5 s *because* this hint survived the
        // drop; the eager eviction added in 49413e5 regressed every
        // teardown-then-reconnect path to a permanent stall (Session 20).
        // HV-27: forget this link's activity timestamp.
        self.link_activity
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(peer);
        // HV-104: no per-peer poller to abort — the one transport-wide poller
        // simply stops seeing this handle once it is out of `connections` /
        // `inbound_handles` and drops its reassembler on the next tick.
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

    /// HV-10/HV-31/HV-99: drain and act on the Kotlin adapter's lifecycle
    /// events (Bluetooth off→on toggle, advertising-retry exhaustion). Called
    /// both from `discover_peers` (every scan pass) and from `poll_health`
    /// (a cheap 5 s tick) so a toggle is felt in ~5 s, not up to the 30 s
    /// linked `scan_interval`.
    ///   2 = BT off  → drop scan/adv handles + all GATT links, transport Unavailable
    ///   3 = BT on   → transport Available, re-advertise from the cached beacon,
    ///                 force a scan re-arm
    ///   1 = advertising retries exhausted → re-drive advertising from the core
    async fn handle_adapter_events(&self, adapter: &std::sync::Arc<dyn BleAdapter>) {
        let mut readvertise = false;
        for code in adapter.drain_adapter_events() {
            match code {
                2 => {
                    *self.scan_handle.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    let dead: Vec<PeerId> = {
                        // HV-107: outbound + inbound both die on a BT-off.
                        let mut ks: Vec<PeerId> = self
                            .connections
                            .lock()
                            .unwrap_or_else(|p| p.into_inner())
                            .keys()
                            .copied()
                            .collect();
                        ks.extend(
                            self.inbound_handles
                                .lock()
                                .unwrap_or_else(|p| p.into_inner())
                                .keys()
                                .copied(),
                        );
                        ks.sort_unstable();
                        ks.dedup();
                        ks
                    };
                    for peer in &dead {
                        self.close_peer(peer, adapter.clone(), None);
                    }
                    self.set_state(TransportState::Unavailable);
                    tracing::warn!(
                        event = "ble.adapter_off",
                        links_dropped = dead.len(),
                        "Bluetooth turned off — transport Unavailable, handles + links dropped"
                    );
                }
                3 => {
                    *self.scan_handle.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) = None;
                    if self.state() == TransportState::Unavailable {
                        self.set_state(TransportState::Available);
                    }
                    readvertise = true;
                    tracing::info!(
                        event = "ble.adapter_recovered",
                        "Bluetooth turned back on — re-advertising, forcing scan re-arm"
                    );
                }
                1 => {
                    readvertise = true;
                    tracing::warn!(
                        event = "ble.advertising_failed",
                        "platform advertising retry exhausted — re-driving advertising from the core"
                    );
                }
                _ => {}
            }
        }
        // HV-95: watchdog — if we are supposed to be advertising (we have a
        // cached beacon and are not Unavailable) but have no live advertise
        // handle, a prior `start_advertising` failed (e.g. the GATT server would
        // not open on a wedged stack). Re-drive it; `start_advertising` retries
        // `ensureGattServer` and only sets `adv_handle` on success.
        if !readvertise
            && self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()).is_none()
            && self.state() != TransportState::Unavailable
            && self.last_advertisement.lock().unwrap_or_else(|p| p.into_inner()).is_some()
        {
            readvertise = true;
            tracing::warn!(
                event = "ble.advertise_watchdog",
                "advertising was expected but no handle is live — re-driving"
            );
        }
        if readvertise {
            let info = self
                .last_advertisement
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone();
            if let Some(info) = info {
                if let Err(e) = self.start_advertising(info).await {
                    tracing::warn!(
                        event = "ble.readvertise_failed",
                        error = %e,
                        "could not re-advertise after adapter event"
                    );
                }
            }
        }
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
        // HV-91: the scan-restart throttle only matters when we are actually
        // going to restart the scan. A harvest-only pass costs the OS nothing,
        // so it must not burn the `scan_allowed` budget (this was the
        // "discover_peers counts, startScan counts" conflation — HV-14).
        // HV-11: consume any async `onScanFailed` the platform fired since the
        // last pass. A hard refusal (2 APPLICATION_REGISTRATION_FAILED,
        // 3 INTERNAL_ERROR, 6 SCANNING_TOO_FREQUENTLY) means the scan we think
        // is live is not — drop the handle so this pass *re-arms* instead of
        // harvesting a dead scan, and on 6 arm the same `refused_until` backoff
        // `scan_allowed()` uses so we do not immediately trip the OS ceiling
        // again.
        for code in adapter.drain_scan_failures() {
            if matches!(code, 2 | 3 | 6) {
                *self.scan_handle.lock().unwrap_or_else(|p| p.into_inner()) = None;
                tracing::warn!(
                    event = "ble.scan_refused",
                    code,
                    "platform onScanFailed — forcing a scan re-arm"
                );
                if code == 6 {
                    let until = std::time::Instant::now() + SCAN_WINDOW;
                    *self.refused_until.write().unwrap_or_else(|p| p.into_inner()) = Some(until);
                }
            }
        }
        // HV-10/HV-31/HV-99: react to any Bluetooth off→on toggle the Kotlin
        // adapter surfaced. Runs here AND on the cheap `poll_health` path so the
        // reaction is not gated on the 30 s linked scan cadence.
        self.handle_adapter_events(&adapter).await;

        // HV-13: sweep stale / overflowing MAC→candidate hints once per pass.
        {
            let mut known = self.known_addresses.lock().unwrap_or_else(|p| p.into_inner());
            known.retain(|_, (_, seen)| seen.elapsed() < KNOWN_ADDR_TTL);
            if known.len() > KNOWN_ADDR_CAP {
                let mut by_age: Vec<(BleAddress, std::time::Instant)> =
                    known.iter().map(|(a, (_, t))| (*a, *t)).collect();
                by_age.sort_by_key(|(_, t)| *t);
                for (addr, _) in by_age.into_iter().take(known.len() - KNOWN_ADDR_CAP) {
                    known.remove(&addr);
                }
            }
        }

        const SCAN_REARM_EVERY: u64 = 5;
        let pass = self.scan_pass.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let want_rearm = {
            let scan_handle = self.scan_handle.lock().unwrap_or_else(|p| p.into_inner());
            scan_handle.is_none() || pass % SCAN_REARM_EVERY == 0
        };
        if want_rearm && !self.scan_allowed() {
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
                rssi_threshold: Some(RSSI_FLOOR_DBM),
            }
        } else {
            ScanFilter { rssi_threshold: Some(RSSI_FLOOR_DBM), ..ScanFilter::default() }
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
        if want_rearm {
            // HW-1: never call start_scan twice without stopping the prior scan
            // (real BluetoothLeScanner → SCAN_FAILED_ALREADY_STARTED forever).
            let mut scan_handle = self.scan_handle.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(prior) = *scan_handle {
                adapter.stop_scan(prior);
            }
            let handle = adapter.start_scan(filter).map_err(|e| self.demote_on_fatal(to_transport_err(e)))?;
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
        // BLE-14: probe the iOS-leg GATT characteristic inside spawn_blocking
        // with a per-probe timeout so stalled CoreBluetooth connects cannot
        // hold the tokio executor thread indefinitely. Previously this ran
        // synchronously inside a filter_map closure with no timeout.
        let mut probe_count: usize = 0;
        let mut peers: Vec<PeerInfo> = Vec::new();
        for r in results.into_iter().take(max_peers) {
            // Connect-to-identify (iOS leg ONLY, BLE-RT-C001): on Android
            // the ad-carried beacon is the discovery payload — an empty
            // payload means "not an IRIS peer", never a probe target.
            // Gating on ios_leg prevents blind GATT connects against
            // ambient non-IRIS advertising on the production Android leg.
            let beacon_opt: Option<DiscoveryBeacon> = if r.payload.is_empty() && self.ios_leg {
                // Probe budget (BLE-RT-C001/003): at most probe_budget
                // connect/read/disconnect cycles per discovery pass.
                if probe_count >= probe_budget {
                    continue;
                }
                probe_count += 1;
                let adapter2 = adapter.clone();
                let addr = r.address;
                // Run the blocking GATT triple on a dedicated thread with a
                // deadline so a stalled probe never monopolises the executor.
                match tokio::time::timeout(
                    PROBE_TIMEOUT,
                    tokio::task::spawn_blocking(move || -> Option<DiscoveryBeacon> {
                        let handle = adapter2.connect_gatt(addr).ok()?;
                        let payload = adapter2.gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC).ok();
                        adapter2.disconnect_gatt(handle);
                        DiscoveryBeacon::parse(&payload?).ok()
                    }),
                )
                .await
                {
                    Ok(Ok(b)) => b,
                    _ => continue,
                }
            } else if r.payload.is_empty() {
                continue;
            } else {
                DiscoveryBeacon::parse(&r.payload).ok()
            };
            let beacon = match beacon_opt {
                Some(b) => b,
                None => continue,
            };
            let mut addresses = Vec::with_capacity(2);
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
            // HV-21/DEC-BLE-0008: a peer's own Wi-Fi Direct MAC, carried in
            // its beacon (v2) — surfaced here via the existing
            // `transport_addresses` convention ("transport id" -> address)
            // rather than a new `PeerInfo` field, so the Wi-Fi Direct
            // transport (or whatever aggregates cross-transport discovery
            // results, e.g. a future `NeighborTable` consumer) can match a
            // plain `discoverPeers()`-found device's MAC back to this
            // already-BLE-known `PeerId` without depending on Wi-Fi
            // Direct's own DNS-SD service discovery.
            if let Some(mac) = beacon.wifi_direct_mac {
                addresses.push((
                    "wifi-direct-0".to_string(),
                    format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                    ),
                ));
            }
            let candidate_id = beacon.candidate_peer_id();
            // BLE-22: skip peers that do not advertise GATT unicast support —
            // they cannot participate in the IRIS data path.
            if !beacon.capabilities.contains(CapabilityBits::GATT_UNICAST) {
                continue;
            }
            // BLE-22: advisory staleness — skip beacons older than ~60 minutes.
            // freshness==0 is the "no freshness info" sentinel (sim/builder didn't set it).
            // Treated as advisory: a device with a wrong clock is not blocked.
            if beacon.freshness_minutes != 0 {
                let age_min = crate::transport::freshness_minutes_now()
                    .wrapping_sub(beacon.freshness_minutes);
                if age_min > 60 {
                    continue;
                }
            }
            // BLE-19: honour DiscoveryConfig::filter — only emit peers the
            // caller asked for (None = all, mirroring wifiaware.rs:891-895).
            if let Some(filter) = config.filter.as_ref() {
                if !filter.contains(&candidate_id) {
                    continue;
                }
            }
            // HW-9: record this address's candidate id so the
            // accept-poller (a connection WE did not dial, so its
            // GATT-server callback only ever sees a MAC, never a beacon)
            // can attribute it to a peer we've independently seen while
            // scanning, exactly as `send()`'s HW-5 candidate resolution
            // already treats this same value.
            self.known_addresses
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(r.address, (candidate_id, std::time::Instant::now()));
            peers.push(PeerInfo {
                peer_id: candidate_id,
                addresses: Vec::new(),
                transport_addresses: addresses,
                last_seen: Some(std::time::Instant::now()),
            });
        }
        Ok(Box::pin(futures_util::stream::iter(peers)))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        // BLE-15: clear the slot so start_discovery doesn't stop a handle the
        // OS has already released on the next re-arm.
        let h = self.scan_handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(h) = h {
            self.adapter()?.stop_scan(h);
        }
        Ok(())
    }

    async fn start_advertising(&self, info: NodeAdvertisement) -> Result<(), TransportError> {
        let adapter = self.adapter()?;
        // HV-106: start accepting inbound connections BEFORE the fallible
        // `adapter.start_advertising()` below. A peer that is already connected
        // to our GATT server (common after our own `stopMesh`/`startMesh` — the
        // OEM server rarely fires a disconnect, so the remote central keeps its
        // link and keeps writing) must keep being serviced even if re-advertising
        // transiently fails. Gating the accept-poller on advertising success
        // meant that after a RETRY the inbound half of the link went permanently
        // dead: writes reached `onCharacteristicWriteRequest` but nothing drained
        // them. Idempotent — the guard in `ensure_accept_poller` no-ops a second
        // call.
        self.ensure_accept_poller(adapter.clone());
        // HV-31: keep it so `discover_peers` can re-advertise after a BT toggle.
        *self.last_advertisement.lock().unwrap_or_else(|p| p.into_inner()) = Some(info.clone());
        // Re-announcement (e.g. after identity/key rotation) must stop any prior
        // advertisement first so Android's max ~4 advertising-set budget is not
        // leaked and the radio doesn't keep a stale set alive (BLE-RT-011).
        // BLE-15: take() clears the slot immediately after the stop, so a
        // subsequent failure of start_advertising does not leave a stale handle.
        let prior = self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(prior) = prior {
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
        // HV-21 correction (Session 24, same session): the v2 beacon (28 B)
        // was confirmed on real hardware to push legacy BLE advertising over
        // its payload ceiling — `startAdvertising failed errorCode=1`
        // (ADVERTISE_FAILED_DATA_TOO_LARGE) on BOTH bench phones, every
        // single advertise attempt, the moment this shipped. The Kotlin
        // adapter uses legacy `BluetoothLeAdvertiser`/`AdvertiseSettings`
        // (`AndroidBleTransportAdapter.startAdvertising`), never the
        // extended path — `EXTENDED_ADV` in `caps` is advisory-only,
        // nothing here actually switches advertising mode. Legacy adverts
        // are ceilinged at 31 bytes total (flags + service-UUID + this
        // service-data AD structure); the original 22-byte beacon already
        // used most of that budget deliberately (the module doc's own "fits
        // the legacy 251 B" comment — mis-stated ceiling, corrected here to
        // the real legacy limit). The broadcast beacon therefore MUST NOT
        // grow — `wifi_direct_mac` stays `None` on this path. `parse` still
        // accepts a v2 payload (harmless, unused for now); a future session
        // needs to carry the MAC over the GATT identify-characteristic
        // fallback path instead (already used when the advert payload can't
        // carry the beacon at all, see `discover_peers`'s `gatt_read` call),
        // which has no such size ceiling — not the broadcast advertisement.
        let wifi_direct_mac: Option<[u8; 6]> = None;
        let freshness_now = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 60) as u16;
        let beacon = DiscoveryBeacon::build(caps, peer_short, freshness_now, wifi_direct_mac);
        // HV-21/DEC-BLE-0008: the GATT identify-characteristic read has no
        // ~31-byte ceiling (L2CAP fragmentation, bounded only by the
        // negotiated ATT MTU) — serve the v2 form there, with the real MAC
        // folded in when known, so a peer that reads it (already how iOS
        // gets the beacon at all, DEC-BLE-002-0002) learns our Wi-Fi Direct
        // address without the advertisement ever growing.
        adapter.set_identify_payload(DiscoveryBeacon::build(
            caps,
            peer_short,
            freshness_now,
            *self.wifi_direct_mac.lock().unwrap_or_else(|p| p.into_inner()),
        ));
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
            .map_err(|e| self.demote_on_fatal(to_transport_err(e)))?;
        *self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()) = Some(handle);
        // HV-106: the accept-poller was already ensured above, before the
        // fallible advertise call — nothing to do here.
        // GAP-3: if a previous shutdown() left the transport Unavailable, recover
        // to Available so a stop→start cycle re-enters manager selection.
        // The state never goes above Unavailable on the bring-up path, so this
        // is the earliest safe place to transition it back.
        if self.state() == TransportState::Unavailable {
            self.set_state(TransportState::Available);
        }
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        // BLE-15: take() clears the slot before the FFI call so a subsequent
        // start_advertising does not attempt to stop an already-released handle.
        let h = self.adv_handle.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(h) = h {
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
        // BLE-11: parse_mac now returns Option — malformed addresses produce
        // PeerNotFound rather than silently connecting to 00:00:00:00:00:00.
        let mac = peer
            .transport_addresses
            .iter()
            .find(|(t, _)| t == "ble")
            .and_then(|(_, addr)| parse_mac(addr))
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
        let mtu_result = tokio::time::timeout(
            MTU_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                mtu_adapter.set_mtu(handle, crate::transport::ble_att::MTU_NEGOTIATED)
            }),
        )
        .await
        .ok()
        .and_then(|r| r.ok())
        // flatten: Ok(Ok(v)) -> Ok(v), Ok(Err(e)) -> Err(e), timeout -> None -> Ok(MTU_DEFAULT)
        .map_or(Ok(crate::transport::ble_att::MTU_DEFAULT), |r| r);
        // BLE-6 / BLE-7: distinguish error kinds before applying payload_to_mtu.
        // Fatal errors (radio off, permission revoked) surface so the caller can
        // tear down; transient GATT failures are warned and fall back to the
        // conservative MTU_DEFAULT.
        let stored_mtu = match mtu_result {
            Ok(val) if self.ios_leg => {
                // iOS reports ATT payload, not MTU — translate (DEC-BLE-002-0004).
                // Degraded 20-B payloads (iOS 16.0/16.0.1) are accepted as-is.
                crate::transport::ble_att::payload_to_mtu(val)
            }
            Ok(val) => val,
            Err(BleError::PermissionDenied) | Err(BleError::AdapterOff) => {
                self.set_state(TransportState::Unavailable);
                return Err(TransportError::ShuttingDown);
            }
            Err(e) => {
                tracing::warn!(peer = ?peer.peer_id, err = %e, "set_mtu failed; using MTU_DEFAULT");
                crate::transport::ble_att::MTU_DEFAULT
            }
        };
        self.connections
            .lock()
            .unwrap()
            .insert(peer.peer_id, (handle, stored_mtu, 0u16));
        self.set_state(TransportState::Connected);
        let established_at = Instant::now();
        // HV-104: inbound frames for this new link are drained by the single
        // transport-wide poller (`ensure_accept_poller`), which iterates every
        // live `connections` + `inbound_handles` entry each tick — no per-peer
        // task. Ensure it is running (idempotent): on a cold bring-up
        // `start_advertising` already started it, but a `connect()` before the
        // first advertise (some tests) still needs it.
        self.ensure_accept_poller(adapter.clone());
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
        // BLE-25: fetch handle+mtu and atomically increment the per-connection
        // msg_id counter in one lock hold so concurrent sends to different
        // peers use independent counters (was: one shared AtomicU16 on segmenter).
        let (handle, mtu, msg_id) = {
            let mut conns = self.connections.lock().unwrap();
            let entry = conns.get_mut(&resolved_key).ok_or(TransportError::NotConnected)?;
            let h = entry.0;
            let m = entry.1;
            let id = entry.2;
            entry.2 = entry.2.wrapping_add(1);
            (h, m, id)
        };
        // Segment the payload into ATT frames sized to THIS peer's negotiated
        // MTU (AC-2 + BLE-RT-003). A late high-MTU peer must never inflate
        // frames destined to a low-MTU peer.
        let (_msg_id, frames) = self.segmenter.segment_for_mtu_with_id(&message.payload, mtu, msg_id).map_err(|e| {
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
        // HV-7: a single failed ATT write used to `close_peer` immediately, so
        // one dropped fragment (frame 7 of N) tore down the whole GATT link and
        // discarded the message — every later send then paid the ~30 s
        // re-discovery. Most write failures on a live ACL are transient
        // (crickshaw.dev "Surviving GATT_ERROR 133"; Martijn van Welie pt.2):
        // stack contention, a momentary RF dropout. Retry each frame a few
        // times with jittered backoff before giving up; only `close_peer` when
        // a frame truly exhausts its retries or the error is non-retryable
        // (a framing fault). A write timeout mid-message (the link dropped
        // under load — HV-96) is `NotConnected` so the engine HOLDS the
        // message across the reconnect rather than dropping it.
        const FRAME_WRITE_ATTEMPTS: u32 = 3;
        const FRAME_BACKOFF_MS: [u64; 2] = [250, 500];
        for (fi, frame) in frames.iter().enumerate() {
            let mut attempt = 0u32;
            loop {
                let e = match adapter.gatt_write(handle, char_uuid, frame.clone()) {
                    Ok(()) => break,
                    Err(e) => e,
                };
                // A resolved connection whose handle the adapter does not know
                // is a phantom — the inbound-only accept-poller entry, or a
                // handle the disconnect callback already tore down. Retrying it
                // is pointless; drop it and report `NotConnected` so the engine
                // HOLDS the message and re-resolves to a real connection.
                if matches!(&e, BleError::GattFailure(m) if m.contains("unknown gatt connection")) {
                    self.close_peer(&resolved_key, adapter.clone(), None);
                    return Err(TransportError::NotConnected);
                }
                let te = to_transport_err(e);
                attempt += 1;
                if !te.is_retryable() || attempt >= FRAME_WRITE_ATTEMPTS {
                    self.close_peer(&resolved_key, adapter.clone(), None);
                    return Err(te);
                }
                // jitter: 0..64 ms keyed off (handle, frame idx, attempt)
                let base = FRAME_BACKOFF_MS
                    [(attempt as usize - 1).min(FRAME_BACKOFF_MS.len() - 1)];
                let jitter = (handle.0 ^ (fi as u64) ^ (attempt as u64)) % 64;
                tokio::time::sleep(Duration::from_millis(base + jitter)).await;
            }
        }
        // BLE-10: count wire bytes actually sent (payload + per-frame ATT headers).
        let bytes_sent: usize = frames.iter().map(Vec::len).sum();
        self.ewma.record_send(bytes_sent); // MG-17
        // HV-27: every frame of this message landed — the link is demonstrably alive.
        self.link_activity
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(resolved_key, tokio::time::Instant::now());
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent,
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
            bandwidth_available_bps: self.ewma.bandwidth_bps(self.caps.typical_throughput_bps),
            congestion_level: congestion,
        }
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        // HV-104: one transport-wide poller (was one task per peer). Abort it;
        // a fresh one starts on the next `start_advertising` / `connect`.
        if let Some(p) = self.accept_poller.lock().unwrap_or_else(|p| p.into_inner()).take() {
            p.abort();
        }
        // HV-98: a fresh accept-poller (its `spawned` set) starts on the next
        // `start_advertising`; clear it here so it does not carry stale handles
        // across a stop/start cycle.
        self.accept_spawned.lock().unwrap_or_else(|p| p.into_inner()).clear();
        self.set_state(TransportState::Unavailable);
        // Disconnect every live GATT link (RED-0009-01) — outbound and, HV-107,
        // inbound.
        let adapter = self.adapter()?;
        let mut handles: Vec<GattHandle> = self
            .connections
            .lock()
            .unwrap()
            .drain()
            .map(|(_, (h, ..))| h)
            .collect();
        handles.extend(
            self.inbound_handles
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .drain()
                .map(|(_, h)| h),
        );
        for h in handles {
            adapter.disconnect_gatt(h);
        }
        Ok(())
    }

    /// HV-97: drop every live GATT link but keep scanning/advertising up so the
    /// discovery loop re-forms them — the deterministic reconnect trigger the
    /// bench harness needs to verify HV-14/HV-15. Unlike `shutdown`, the
    /// transport poller stays running, and the state goes to `Available` (not
    /// `Unavailable`), so `connect()` is re-selected.
    async fn drop_all_links(&self) {
        let Ok(adapter) = self.adapter() else { return };
        let dropped: Vec<PeerId> = {
            // HV-107: both outbound (`connections`) and inbound (`inbound_handles`)
            // peers must be torn down so the reconnect re-forms a clean link
            // in both directions.
            let mut ks: Vec<PeerId> = self
                .connections
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .keys()
                .copied()
                .collect();
            ks.extend(
                self.inbound_handles
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .keys()
                    .copied(),
            );
            ks.sort_unstable();
            ks.dedup();
            ks
        };
        for peer in &dropped {
            self.close_peer(peer, adapter.clone(), None);
        }
        tracing::warn!(
            event = "ble.drop_all_links",
            count = dropped.len(),
            "test hook: dropped all GATT links, discovery will re-form them"
        );
    }

    async fn poll_health(&self) {
        // HV-99: only the cheap adapter-event drain — never a scan.
        if let Ok(adapter) = self.adapter() {
            self.handle_adapter_events(&adapter).await;
        }
    }

    /// HV-21: fold the local Wi-Fi Direct MAC into the discovery beacon. If
    /// advertising is already live, re-publish immediately (mirrors the
    /// identity-rotation re-announce path, BLE-15) so a peer that already
    /// scanned us before the MAC was known picks it up on the next scan
    /// pass rather than waiting for an unrelated re-advertise trigger.
    async fn set_local_wifi_direct_mac(&self, mac: [u8; 6]) {
        *self.wifi_direct_mac.lock().unwrap_or_else(|p| p.into_inner()) = Some(mac);
        let info = self
            .last_advertisement
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        if let Some(info) = info {
            let _ = self.start_advertising(info).await;
        }
    }

    fn link_quality(&self, peer: &PeerId) -> Option<LinkQuality> {
        // Resolve to whatever key `connections` / `link_activity` is stored
        // under (real id or the discovery-time candidate) — same as `send()`.
        let key = {
            let conns = self.connections.lock().unwrap_or_else(|p| p.into_inner());
            if conns.contains_key(peer) {
                *peer
            } else {
                let cand = Self::candidate_key_for(peer);
                if conns.contains_key(&cand) { cand } else { return None }
            }
        };
        let last = *self
            .link_activity
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&key)?;
        // Fresh traffic → the link is carrying data now. Quiet for a while →
        // suspect (we have not confirmed it live, and the OEM disconnect
        // callback is unreliable). Thresholds are generous so a normally-idle
        // mesh link does not flap to Poor between user messages.
        Some(match last.elapsed().as_secs() {
            0..=15 => LinkQuality::Good,
            16..=45 => LinkQuality::Fair,
            _ => LinkQuality::Poor,
        })
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
        // HV-92: a `GattFailure` is only a genuine protocol fault when the link
        // is up and carried a frame that failed. The "link is still coming up"
        // failures — the write characteristic not yet in the discovered GATT DB
        // (Android's incomplete first-discovery), a disconnect racing the
        // connect handshake, or `writeCharacteristic` never accepting the
        // initiation — are transient: the discovery/connect loop is actively
        // (re)establishing a usable link. Map them to `NotConnected`, which
        // HV-90 made retryable *and* transient, so a message sent into the
        // ~30 s connect window is held (TTL-bounded) instead of `DeliveryFailed`
        // at attempt 0.
        BleError::GattFailure(m) if gatt_failure_is_link_not_ready(&m) => {
            TransportError::NotConnected
        }
        BleError::GattFailure(m) => TransportError::Protocol(m),
    }
}

/// HV-92 / HV-96: `true` when a `GattFailure` message describes a link that is
/// not (yet) usable — still establishing, or dropped mid-write — as opposed to a
/// framing/decode fault on a live link. These are transient: the discovery /
/// connect loop is (re)establishing a link, so the message should be **held**
/// (`NotConnected` — retryable + transient) rather than `DeliveryFailed`.
fn gatt_failure_is_link_not_ready(msg: &str) -> bool {
    msg.contains("not yet discovered")
        || msg.contains("disconnected before connect completed")
        || msg.contains("not initiated")
        || msg.contains("service discovery failed")
        // HV-96: the link dropped while a write was in flight — the write
        // future timed out waiting for `onCharacteristicWrite`. Sustained
        // bidirectional load triggers this on real hardware. (NOT "unknown
        // gatt connection" — that means the handle itself is wrong/gone and
        // retrying it is pointless; it must fail over, not hold.)
        || msg.contains("onCharacteristicWrite never fired")
        // HV-33: classify the numeric Android GATT status the Kotlin adapter
        // embeds as `status=<n>`. A `GattFailure` carrying one of the transient
        // codes is a link that dropped or a controller that hiccuped, not a
        // framing fault — hold + reconnect, do not `DeliveryFailed`.
        || gatt_status_is_transient(msg)
}

/// HV-33: Android GATT status codes that mean "the link / controller had a
/// transient problem — retry", as opposed to a genuine protocol/framing fault.
/// Sources: `android.bluetooth.BluetoothGatt` + AOSP `gatt_api.h`;
/// arstagaev/BLE-Status-Codes; Martijn van Welie "Making Android BLE work pt.2";
/// crickshaw.dev "Surviving GATT_ERROR 133"; dev.to/ble_advertiser GATT-133.
///   `133` (0x85 GATT_ERROR)          — catch-all, usually transient (HCI cmd
///                                      failed / link dropped mid-op / degraded
///                                      controller); the canonical "just retry".
///   `8`   (0x08 GATT_CONN_TIMEOUT)   — link supervision timeout: peer restarted
///                                      / out of range / lost — reconnect.
///   `62`  (0x3E CONN_FAIL_ESTABLISH) — could not establish — retry.
///   `22`  (0x16 CONN_TERMINATE_LOCAL_HOST) — Android tore it down; if we still
///                                      want the peer, reconnect.
///   `19`  (0x13 CONN_TERMINATE_PEER_USER)  — peer disconnected gracefully;
///                                      normal, reconnect only if still
///                                      discovered (the per-peer connect backoff
///                                      keeps this from spinning).
/// `257` (0x101 GATT_FAILURE) and anything else stay a `Protocol` fault.
fn gatt_status_is_transient(msg: &str) -> bool {
    let Some(rest) = msg.split("status=").nth(1) else { return false };
    let code: i64 = rest
        .trim_start()
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    matches!(code, 133 | 8 | 62 | 22 | 19)
}

/// Parse `"AA:BB:CC:DD:EE:FF"` into a `BleAddress`.
///
/// Returns `None` on any parse error — fewer than 6 octets, non-hex digits,
/// or values out of range — so callers can propagate `PeerNotFound` rather
/// than silently connecting to `00:00:00:00:00:00` (BLE-11).
fn parse_mac(s: &str) -> Option<BleAddress> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut b = [0u8; 6];
    for (i, part) in parts.iter().enumerate() {
        b[i] = u8::from_str_radix(part, 16).ok()?;
    }
    Some(BleAddress(b))
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
        // BLE-10: bytes_sent is wire accounting (sum of encoded ATT frame
        // lengths, including the frame header) since an earlier fix, not
        // raw payload length — "ble payload" is 11 payload bytes but 18
        // wire bytes once the 7-byte frame header (GAP-5: +1 version byte) is
        // included.
        assert_eq!(receipt.bytes_sent, 18);
    }

    #[test]
    fn hv92_characteristic_not_yet_discovered_maps_to_not_connected() {
        // HV-92: the "link still coming up" GattFailures must be retryable so
        // message_engine holds the message across the ~30 s reconnect window.
        for m in [
            "characteristic 3e5c6b1a...0c0e not yet discovered",
            "disconnected before connect completed, status=133",
            "gatt write not initiated after 3 attempts",
            "service discovery failed status=129",
        ] {
            let e = to_transport_err(BleError::GattFailure(m.into()));
            assert_eq!(e, TransportError::NotConnected, "{m:?}");
            assert!(e.is_retryable(), "{m:?} must be retryable");
        }
        // A genuine framing fault on a live link stays a non-retryable Protocol.
        assert!(matches!(
            to_transport_err(BleError::GattFailure("bad frame: truncated".into())),
            TransportError::Protocol(_)
        ));
    }

    #[test]
    fn hv33_transient_gatt_status_codes_are_held_not_failed() {
        // HV-33: 133 (GATT_ERROR), 8 (CONN_TIMEOUT), 62 (FAIL_ESTABLISH),
        // 22 (LOCAL_HOST terminate), 19 (PEER_USER terminate) on a *write* are a
        // dropped/hiccuping link — hold + reconnect. Before this they hit the
        // `GattFailure(m) => Protocol(m)` arm and `send()` failed the message +
        // tore the peer down on the first blip.
        for code in [133, 8, 62, 22, 19] {
            let m = format!("gatt write failed status={code}");
            let e = to_transport_err(BleError::GattFailure(m.clone()));
            assert_eq!(e, TransportError::NotConnected, "{m:?}");
            assert!(e.is_retryable(), "{m:?} must be retryable");
        }
        // 257 (GATT_FAILURE) and unknown codes stay a Protocol fault.
        for code in [257, 129, 1] {
            assert!(
                matches!(
                    to_transport_err(BleError::GattFailure(format!("gatt write failed status={code}"))),
                    TransportError::Protocol(_)
                ),
                "status={code} must stay Protocol"
            );
        }
        // The parser only fires on a real `status=<n>` token.
        assert!(!gatt_status_is_transient("bad frame: truncated"));
        assert!(!gatt_status_is_transient("status=abc"));
    }

    #[tokio::test(start_paused = true)]
    async fn hv7_transient_frame_write_failure_retries_without_closing_the_link() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([31u8; 32]), "AA:BB:CC:DD:EE:F7");
        let link = t.connect(&peer).await.unwrap();
        let handle_before = adapter.connect_count();

        // The next two ATT writes fail transiently (write never completed),
        // then recover — the per-frame retry must ride over it.
        adapter.fail_next_writes_transient(2);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([32u8; 16]),
            priority: MessagePriority::P2,
            payload: b"a 300-ish char message that spans a couple of ATT frames \
                       so the retry logic is exercised on more than the first \
                       frame; the link must survive a transient miss".to_vec(),
        };
        let receipt = t.send(&peer.peer_id, &msg).await.expect(
            "HV-7: a transient mid-message write failure must NOT fail the send",
        );
        assert!(receipt.bytes_sent > 0);
        // The link was NOT torn down (no reconnect, still Connected).
        assert_eq!(t.state(), TransportState::Connected);
        assert_eq!(adapter.connect_count(), handle_before, "no reconnect");
        assert_eq!(link.peer_id, peer.peer_id);
    }

    #[tokio::test]
    async fn hv98_inbound_poller_re_attaches_after_our_own_drop() {
        use crate::transport::ble_att::encode_frame;
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = std::sync::Arc::new(BleTransport::new(Some(adapter.clone())));
        // Become dialable → the accept-poller starts.
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([50u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        })
        .await
        .unwrap();

        let handle = GattHandle(77);
        let addr = BleAddress([9, 9, 9, 9, 9, 9]);
        let mut incoming = t.incoming_messages();

        // A remote central dials our server and writes a frame.
        adapter.inject_accepted_connection(handle, addr);
        adapter.inject_write(
            handle,
            IRIS_WRITE_CHARACTERISTIC,
            encode_frame(1, 0, 1, b"first".len(), b"first").unwrap(),
        );
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        assert_eq!(got.expect("first inbound").unwrap().payload, b"first");

        // We tear our own links down (drop_all_links / a write failure). The OEM
        // GATT server never tells us the peer dropped.
        t.drop_all_links().await;

        // The peer re-links: the adapter re-announces (write-gap heuristic) and
        // writes again on the SAME handle. The inbound poller MUST re-attach.
        adapter.inject_accepted_connection(handle, addr);
        adapter.inject_write(
            handle,
            IRIS_WRITE_CHARACTERISTIC,
            encode_frame(2, 0, 1, b"after reconnect".len(), b"after reconnect").unwrap(),
        );
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        assert_eq!(
            got.expect("HV-98: inbound must resume after a drop+reconnect").unwrap().payload,
            b"after reconnect"
        );
    }

    #[tokio::test]
    async fn hv94_client_disconnect_tears_down_peer_before_next_send() {
        // HV-94: the peer's *app process* died — its GATT server is gone and the
        // ACL dropped, so the OS fires `onConnectionStateChange(DISCONNECTED)`
        // on our client. The Kotlin adapter now surfaces that handle via
        // `drain_disconnected_handles`; the accept-poller must consume it and
        // tear the peer down, so the NEXT `send()` fails cleanly (`NotConnected`
        // → the engine HOLDS + reconnects) instead of writing into a dead
        // handle.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = std::sync::Arc::new(BleTransport::new(Some(adapter.clone())));
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([60u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        })
        .await
        .unwrap();

        let peer = peer_with_mac(PeerId([1u8; 32]), "AA:BB:CC:DD:EE:F1");
        t.connect(&peer).await.unwrap();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([2u8; 16]),
            priority: MessagePriority::P2,
            payload: b"hi".to_vec(),
        };
        t.send(&peer.peer_id, &msg).await.expect("send works while linked");

        // The OS reports the client link down (peer's process died).
        adapter.simulate_disconnect(GattHandle(1));
        // The accept-poller drains it within a poll interval.
        tokio::time::sleep(Duration::from_millis(700)).await;

        assert_eq!(
            t.send(&peer.peer_id, &msg).await.unwrap_err(),
            TransportError::NotConnected,
            "a dropped client link must be torn down, not written into"
        );
    }

    #[tokio::test]
    async fn hv97_drop_all_links_clears_connections_but_stays_reconnectable() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([41u8; 32]), "AA:BB:CC:DD:EE:F9");
        t.connect(&peer).await.unwrap();
        assert_eq!(t.state(), TransportState::Connected);

        t.drop_all_links().await;
        // Link is gone but the transport is NOT shut down — it can be selected
        // and re-connected without a start_all().
        assert_ne!(t.state(), TransportState::Unavailable);
        assert!(t.send(&peer.peer_id, &SerializedMessage {
            message_id: crate::protocol::MessageId::from([42u8; 16]),
            priority: MessagePriority::P2,
            payload: b"x".to_vec(),
        }).await.is_err(), "no link right after drop_all_links");

        // Discovery re-forms it.
        t.connect(&peer).await.unwrap();
        assert_eq!(t.state(), TransportState::Connected);
    }

    #[tokio::test(start_paused = true)]
    async fn hv7_write_failure_that_exhausts_retries_still_holds_the_message() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([33u8; 32]), "AA:BB:CC:DD:EE:F8");
        t.connect(&peer).await.unwrap();

        // Every write fails transiently — retries exhaust. The error must still
        // be `NotConnected` (held), not `Protocol` (dropped).
        adapter.fail_next_writes_transient(50);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([34u8; 16]),
            priority: MessagePriority::P2,
            payload: b"held not dropped".to_vec(),
        };
        let err = t.send(&peer.peer_id, &msg).await.unwrap_err();
        assert_eq!(
            err,
            TransportError::NotConnected,
            "HV-96: a link that dropped mid-write is transient — hold the message"
        );
        assert!(err.is_retryable());
    }

    #[tokio::test]
    async fn hv92_send_into_unready_link_is_held_then_delivers_after_reconnect() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([21u8; 32]), "AA:BB:CC:DD:EE:F1");
        t.connect(&peer).await.unwrap();

        // The first connection's GATT DB was still incomplete: the write fails
        // "characteristic not yet discovered" past the HV-7 per-frame retry
        // budget (3 attempts on this 1-frame message), exactly as on hardware.
        adapter.fail_next_writes_unready(3);
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([22u8; 16]),
            priority: MessagePriority::P2,
            payload: b"held then sent".to_vec(),
        };
        let err = t.send(&peer.peer_id, &msg).await.unwrap_err();
        assert_eq!(
            err,
            TransportError::NotConnected,
            "HV-92: an unready link is transient, not a Protocol fault"
        );

        // The discovery/connect loop re-establishes the link; this time service
        // discovery is complete and the held message lands.
        t.connect(&peer).await.unwrap();
        let receipt = t.send(&peer.peer_id, &msg).await.unwrap();
        assert!(receipt.bytes_sent > 0);
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
        // BLE-10: same wire-accounting basis as
        // simulated_adapter_connects_and_sends above — the 420-byte
        // difference from the raw 30,000-byte payload is the summed
        // per-segment frame header overhead (GAP-5: 7-byte header, was 6)
        // across every ATT frame the 30 KB payload was split into.
        assert_eq!(receipt.bytes_sent, 30_420);
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
        let beacon = DiscoveryBeacon::build(caps, short, 0, None);
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
    async fn hv95_advertise_watchdog_redrives_a_failed_start() {
        // HV-95: `start_advertising` failed (the OEM GATT server would not open
        // on a wedged stack, so Kotlin threw). Nothing used to retry — the node
        // sat with no server, invisible/unusable, until a manual BT toggle. The
        // `poll_health` watchdog must re-drive it.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.fail_next_advertise(2);
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([70u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        assert!(t.start_advertising(adv.clone()).await.is_err(), "1st attempt fails (wedged)");
        assert!(t.adv_handle.lock().unwrap().is_none());

        // Each health tick re-drives; the 2nd fails, the 3rd succeeds.
        t.poll_health().await;
        assert!(t.adv_handle.lock().unwrap().is_none(), "2nd attempt still fails");
        t.poll_health().await;
        assert!(
            t.adv_handle.lock().unwrap().is_some(),
            "the watchdog must recover advertising once the stack does"
        );
    }

    #[tokio::test]
    async fn hv106_accept_poller_runs_even_when_start_advertising_fails() {
        // HV-106: a peer already connected to our GATT server (it kept its link
        // across our stopMesh/startMesh — the OEM server rarely fires a
        // disconnect) must keep being serviced even if our own re-advertise
        // transiently fails. The accept-poller must NOT be gated on the fallible
        // `adapter.start_advertising()` call.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        adapter.fail_next_advertise(1);
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([71u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        assert!(t.start_advertising(adv).await.is_err(), "advertise leg fails");
        assert!(t.adv_handle.lock().unwrap().is_none(), "no advertise handle");
        assert!(
            t.accept_poller.lock().unwrap().is_some(),
            "the inbound accept-poller must be running regardless of the advertise failure",
        );
    }

    #[tokio::test]
    async fn hv107_inbound_accept_does_not_populate_connections_or_block_the_outbound_dial() {
        use crate::transport::ble_att::encode_frame;
        // HV-107: an inbound (peripheral-role) connection a remote central
        // dialed to our GATT server must NOT land in `connections`. If it does,
        // `connect()` treats the peer as "already connected" and never dials
        // out, and `send()` resolves to a handle it cannot write — so after the
        // peer restarts (RETRY / permission re-grant) this node keeps receiving
        // but can never reply/ACK. The handle belongs in `inbound_handles`.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = std::sync::Arc::new(BleTransport::new(Some(adapter.clone())));
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([72u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        })
        .await
        .unwrap();

        let handle = GattHandle(88);
        let addr = BleAddress([7, 7, 7, 7, 7, 7]);
        let mut incoming = t.incoming_messages();
        adapter.inject_accepted_connection(handle, addr);
        adapter.inject_write(
            handle,
            IRIS_WRITE_CHARACTERISTIC,
            encode_frame(1, 0, 1, b"rx".len(), b"rx").unwrap(),
        );
        // Inbound delivery works…
        let got = tokio::time::timeout(Duration::from_secs(2), incoming.next()).await;
        assert_eq!(got.expect("inbound must deliver").unwrap().payload, b"rx");

        // …but `connections` stays empty — the peer is only in `inbound_handles`.
        assert!(
            t.connections.lock().unwrap().is_empty(),
            "an inbound accept must NOT create a `connections` entry (HV-107)",
        );
        assert_eq!(
            t.inbound_handles.lock().unwrap().len(),
            1,
            "the inbound handle is tracked in its own map",
        );

        // So the peer's own PeerInfo still needs a real outbound dial:
        // `send()` (addressed by any id resolving to that inbound peer) must
        // report NotConnected, not resolve to the un-writable peripheral handle.
        let inbound_pid = *t.inbound_handles.lock().unwrap().keys().next().unwrap();
        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([73u8; 16]),
            priority: MessagePriority::P2,
            payload: b"ack".to_vec(),
        };
        assert_eq!(
            t.send(&inbound_pid, &msg).await.unwrap_err(),
            TransportError::NotConnected,
            "send to an inbound-only peer must be NotConnected so discovery dials a real client link",
        );

        // And a real `connect()` to that peer actually dials (does not no-op on
        // the phantom entry) — after it, `send()` succeeds.
        let peer = peer_with_mac(inbound_pid, "07:07:07:07:07:07");
        t.connect(&peer).await.unwrap();
        assert!(
            t.connections.lock().unwrap().contains_key(&inbound_pid),
            "connect() must establish a real outbound client link",
        );
        assert!(t.send(&inbound_pid, &msg).await.is_ok(), "reply path is live after the dial");
    }

    #[tokio::test]
    async fn hv13_known_addresses_is_bounded_by_ttl_and_cap() {
        // HV-13: the MAC→candidate hint map used to grow unbounded. The cap
        // (128) + TTL (300 s) sweep in `discover_peers` bounds it.
        // HV-105: `close_peer` must NOT evict the hint — the inbound
        // accept-poller resolves a re-linking peer's MAC through it, and a
        // transient drop must not permanently break reassembly.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);

        // A discovered peer → its MAC is recorded.
        let peer_id = PeerId([0x0F; 32]);
        let short = crate::identity::peer_id::peer_short(&peer_id.0);
        let mac = BleAddress([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
        adapter.inject_scan_result(mac, DiscoveryBeacon::build(caps, short, 0, None), -60);
        // …plus a burst of ambient IRIS beacons well over the cap.
        for i in 0..(KNOWN_ADDR_CAP as u16 + 40) {
            let bmac = BleAddress([(i >> 8) as u8, i as u8, 0x33, 0x33, 0x33, 0x33]);
            adapter.inject_scan_result(bmac, DiscoveryBeacon::build(caps, [i as u8; 16], 0, None), -60);
        }
        let _ = t.discover_peers(DiscoveryConfig::default()).await.unwrap();

        {
            let known = t.known_addresses.lock().unwrap();
            assert!(
                known.len() <= KNOWN_ADDR_CAP,
                "known_addresses must be capped at {KNOWN_ADDR_CAP}, was {}",
                known.len()
            );
        }

        // Re-discover just our real peer so it survives the cap, then close it.
        adapter.inject_scan_result(mac, DiscoveryBeacon::build(caps, short, 0, None), -60);
        let _ = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let candidate = DiscoveryBeacon {
            capabilities: caps,
            peer_short: short,
            freshness_minutes: 0,
            wifi_direct_mac: None,
        }
        .candidate_peer_id();
        assert!(t.known_addresses.lock().unwrap().values().any(|(p, _)| *p == candidate));
        // HV-105: the hint SURVIVES close_peer so a re-linking peer still
        // resolves to its real candidate id (not a synthesized one).
        t.close_peer(&candidate, adapter.clone(), None);
        assert!(
            t.known_addresses.lock().unwrap().values().any(|(p, _)| *p == candidate),
            "close_peer must NOT drop the MAC hint (HV-105) — the accept-poller needs it across a transient drop"
        );
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
        let beacon = DiscoveryBeacon::build(caps, short, 0, None);
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

    /// HV-21/DEC-BLE-0008: a discovered peer's beacon carrying a Wi-Fi
    /// Direct MAC surfaces it in `transport_addresses` under the
    /// `"wifi-direct-0"` key — the mechanism the (future) Wi-Fi Direct
    /// discovery rewrite matches a plain `discoverPeers()` result against.
    /// A peer whose beacon carries no MAC yet must not fabricate one.
    #[tokio::test]
    async fn discovered_peer_wifi_direct_mac_surfaces_in_transport_addresses() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);

        let with_mac = DiscoveryBeacon::build(
            caps,
            [0x11; 16],
            0,
            Some([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
        );
        adapter.inject_scan_result(BleAddress([1, 0, 0, 0, 0, 0]), with_mac, -50);
        let without_mac = DiscoveryBeacon::build(caps, [0x22; 16], 0, None);
        adapter.inject_scan_result(BleAddress([2, 0, 0, 0, 0, 0]), without_mac, -50);

        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let mut seen = Vec::new();
        while let Ok(Some(p)) =
            tokio::time::timeout(Duration::from_millis(200), stream.next()).await
        {
            seen.push(p);
        }
        assert_eq!(seen.len(), 2);

        let has_mac = seen
            .iter()
            .find(|p| p.transport_addresses.iter().any(|(k, _)| k == "wifi-direct-0"))
            .expect("the peer whose beacon carried a MAC must surface it");
        assert!(
            has_mac
                .transport_addresses
                .contains(&("wifi-direct-0".to_string(), "AA:BB:CC:DD:EE:FF".to_string())),
            "wifi-direct-0 address must be the exact MAC from the beacon: {:?}",
            has_mac.transport_addresses
        );

        let no_mac = seen
            .iter()
            .find(|p| !p.transport_addresses.iter().any(|(k, _)| k == "wifi-direct-0"))
            .expect("the peer whose beacon carried no MAC must not have one fabricated");
        assert_eq!(no_mac.transport_addresses.len(), 1, "only the ble address, no wifi-direct-0 entry");
    }

    #[tokio::test]
    async fn scan_burst_is_throttled_within_window() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        // HV-91: only the periodic *re-arms* count against the throttle, not
        // every harvest pass. Force SCAN_CEILING re-arms, then expect the next.
        // to be refused. (pass 0, 5, 10, … are re-arms.)
        const REARM_EVERY: u64 = 5;
        let calls_for_ceiling_rearms = REARM_EVERY as usize * (SCAN_CEILING - 1) + 1;
        for _ in 0..calls_for_ceiling_rearms {
            assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
        }
        // Advance to the next re-arm pass and expect the throttle to bite.
        let mut err = Ok(());
        for _ in 0..REARM_EVERY {
            err = t.discover_peers(DiscoveryConfig::default()).await.map(|_| ());
            if err.is_err() {
                break;
            }
        }
        assert!(
            matches!(err, Err(TransportError::Busy)),
            "the {}-th scan re-arm within 30 s must be refused (AC-5)",
            SCAN_CEILING + 1
        );
    }

    #[tokio::test]
    async fn hv11_platform_scan_refusal_forces_a_rearm_and_arms_backoff() {
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        // First pass re-arms and succeeds (pass 0).
        assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
        let stops_after_first = adapter.scan_stop_calls().len();

        // Platform fires onScanFailed(SCANNING_TOO_FREQUENTLY) asynchronously.
        adapter.inject_scan_failure(6);
        // Next pass (not normally a re-arm pass) must be refused — the drained
        // failure both cleared `scan_handle` (forcing a re-arm) and armed the
        // `refused_until` window `scan_allowed()` honours.
        let err = t.discover_peers(DiscoveryConfig::default()).await.map(|_| ());
        assert!(
            matches!(err, Err(TransportError::Busy)),
            "a platform SCANNING_TOO_FREQUENTLY must back the transport off, got {err:?}"
        );
        // It did NOT start a new scan while backed off (no extra stop_scan).
        assert_eq!(adapter.scan_stop_calls().len(), stops_after_first);

        // A non-hard failure code (e.g. 1 ALREADY_STARTED handled elsewhere) is
        // ignored by this path — sanity that only 2/3/6 arm the backoff.
        adapter.inject_scan_failure(5);
        // still refused because the code-6 window has not elapsed
        assert!(t.discover_peers(DiscoveryConfig::default()).await.is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn hv27_link_quality_tracks_recent_activity() {
        // HV-27: `LinkQuality` was hardcoded `Good` in discovery. It now reflects
        // how long since a `gatt_write` to the peer last succeeded.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let peer = peer_with_mac(PeerId([27u8; 32]), "AA:BB:CC:DD:EE:27");
        t.connect(&peer).await.expect("connect");
        assert_eq!(t.link_quality(&peer.peer_id), None, "no traffic yet → no signal");

        let msg = SerializedMessage {
            message_id: crate::protocol::MessageId::from([27u8; 16]),
            priority: MessagePriority::P3,
            payload: b"health".to_vec(),
        };
        t.send(&peer.peer_id, &msg).await.expect("send");
        assert_eq!(t.link_quality(&peer.peer_id), Some(LinkQuality::Good), "just sent");

        tokio::time::advance(Duration::from_secs(20)).await;
        assert_eq!(t.link_quality(&peer.peer_id), Some(LinkQuality::Fair), "quiet 20 s");

        tokio::time::advance(Duration::from_secs(40)).await;
        assert_eq!(t.link_quality(&peer.peer_id), Some(LinkQuality::Poor), "quiet 60 s");

        // A fresh send resets it; close_peer forgets it.
        t.send(&peer.peer_id, &msg).await.expect("send again");
        assert_eq!(t.link_quality(&peer.peer_id), Some(LinkQuality::Good));
        t.close_peer(&peer.peer_id, adapter.clone(), None);
        assert_eq!(t.link_quality(&peer.peer_id), None);
    }

    #[tokio::test]
    async fn hv30_revoked_permission_marks_the_transport_unavailable_and_recovers() {
        // HV-30: a runtime grant revoked mid-session must take the transport out
        // of manager selection (so sends stop failing + requeuing and the UI's
        // UNAVAILABLE path fires), and restoring it + a re-advertise recovers.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([30u8; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        t.start_advertising(adv.clone()).await.expect("advertise ok");
        assert_ne!(t.state(), TransportState::Unavailable);

        adapter.set_permission_revoked(true);
        // A scan pass under revoked permission demotes the transport.
        let e = t.discover_peers(DiscoveryConfig::default()).await.map(|_| ());
        assert!(matches!(e, Err(TransportError::PermissionDenied { .. })), "got {e:?}");
        assert_eq!(t.state(), TransportState::Unavailable);

        // Grant restored + re-advertise → back in play.
        adapter.set_permission_revoked(false);
        t.start_advertising(adv).await.expect("advertise recovers");
        assert_eq!(t.state(), TransportState::Available);
    }

    #[test]
    fn hv12_rssi_floor_is_one_mesh_appropriate_value() {
        // HV-12: three layers used to disagree (-85 / -95 / -127). The core
        // constant is now the single source; -85 filtered out through-a-wall
        // peers a resilience mesh must carry, -127 filtered nothing.
        assert_eq!(RSSI_FLOOR_DBM, -95);
        assert!(
            (-100..=-90).contains(&RSSI_FLOOR_DBM),
            "the mesh RSSI floor must admit wall-attenuated links (~-90..-95) \
             without admitting genuinely dead ones (< -98)"
        );
        // Both call sites in discover_peers carry exactly this value.
        let ios = ScanFilter { service_uuids: vec![IRIS_SERVICE_UUID], rssi_threshold: Some(RSSI_FLOOR_DBM) };
        let android = ScanFilter { rssi_threshold: Some(RSSI_FLOOR_DBM), ..ScanFilter::default() };
        assert_eq!(ios.rssi_threshold, android.rssi_threshold);
        assert_eq!(ios.rssi_threshold, Some(-95));
    }

    #[tokio::test]
    async fn hv31_bluetooth_toggle_marks_transport_unavailable_then_recovers() {
        // HV-31: a Bluetooth off→on toggle invalidates every OS handle. The
        // Kotlin adapter drops the dead handles + replays advertise/scan and
        // surfaces the transition; the core must follow so manager selection
        // and metrics reflect reality.
        let adapter = std::sync::Arc::new(SimulatedBleAdapter::new());
        let t = BleTransport::new(Some(adapter.clone()));
        assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
        assert_ne!(t.state(), TransportState::Unavailable);

        // Platform fires ACTION_STATE_CHANGED -> STATE_OFF.
        adapter.inject_adapter_event(2);
        let _ = t.discover_peers(DiscoveryConfig::default()).await;
        assert_eq!(
            t.state(),
            TransportState::Unavailable,
            "Bluetooth OFF must take the BLE transport out of manager selection"
        );

        // Platform fires STATE_ON — the adapter has already replayed advertise
        // + scan; the core just needs to come back.
        adapter.inject_adapter_event(3);
        let _ = t.discover_peers(DiscoveryConfig::default()).await;
        assert_eq!(
            t.state(),
            TransportState::Available,
            "Bluetooth ON must return the BLE transport to Available"
        );

        // An advertising-give-up event (1) is surfaced but does not change state
        // (scan may still be working).
        adapter.inject_adapter_event(1);
        assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
        assert_eq!(t.state(), TransportState::Available);
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

        // HV-91: the running scan keeps delivering — passes 2..=5 just harvest,
        // no stop/start churn.
        for _ in 0..4 {
            let _ = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        }
        assert!(
            adapter.scan_stop_calls().is_empty(),
            "between re-arm points, discover_peers must not stop/restart the scan"
        );

        // Pass 5 (the 6th call) re-arms — and when it does, it stops the prior
        // handle first (the HW-1 contract: never start_scan twice unstopped).
        let _ = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        assert_eq!(
            adapter.scan_stop_calls(),
            vec![ScanHandle(1)],
            "the periodic re-arm must stop the prior scan handle exactly once first"
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
        fn set_identify_payload(&self, data: Vec<u8>) {
            self.inner.set_identify_payload(data);
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
        let beacon = DiscoveryBeacon::build(caps, [7u8; 16], 0, None);
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
        caps.set(CapabilityBits::GATT_UNICAST);
        caps.set(CapabilityBits::ADVERTISE_BROADCAST);
        let beacon = DiscoveryBeacon::build(caps, [9u8; 16], 0, None);
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
        let b = parse_mac("AA:BB:CC:DD:EE:FF").expect("valid MAC must parse");
        assert_eq!(b.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
        // BLE-11: malformed inputs must return None, not 00:00:00:00:00:00.
        assert!(parse_mac("garbage").is_none());
        assert!(parse_mac("AA:BB").is_none());
        assert!(parse_mac("").is_none());
        assert!(parse_mac("ZZ:BB:CC:DD:EE:FF").is_none());
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
        assert_eq!(
            t.connections.lock().unwrap_or_else(|p| p.into_inner()).len(),
            1,
            "exactly one connection (HV-104: one transport-wide poller drains it)"
        );
        assert!(t.accept_poller.lock().unwrap().is_some(), "transport poller running");
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
        assert!(
            t.inbound_handles.lock().unwrap_or_else(|p| p.into_inner()).is_empty(),
            "no stale inbound handle for the dead peer"
        );
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

    /// HV-21/DEC-BLE-0008 (corrected, same session — see `start_advertising`'s
    /// own comment): a v2, MAC-carrying beacon was found on real hardware to
    /// exceed legacy BLE advertising's payload ceiling
    /// (`ADVERTISE_FAILED_DATA_TOO_LARGE` on every attempt, both bench
    /// phones). The broadcast beacon is therefore pinned back to `None` for
    /// the MAC regardless of what `set_local_wifi_direct_mac` was told —
    /// carrying the MAC over the air instead uses the GATT
    /// identify-characteristic path (no size ceiling there — see
    /// `identify_payload_carries_the_wifi_direct_mac_once_known` below).
    /// This test asserts the broadcast side specifically: learning the MAC
    /// re-triggers advertising (the plumbing works) but the OTA payload
    /// does not grow.
    #[tokio::test]
    async fn hv21_wifi_direct_mac_does_not_grow_the_legacy_advertisement() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([0x62; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        t.start_advertising(adv.clone()).await.unwrap();
        let first = DiscoveryBeacon::parse(
            &adapter.started_adv.lock().unwrap_or_else(|p| p.into_inner()).last().unwrap().service_data,
        )
        .unwrap();
        assert_eq!(first.wifi_direct_mac, None, "no MAC known yet");

        t.set_local_wifi_direct_mac([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]).await;

        let started = adapter.started_adv.lock().unwrap_or_else(|p| p.into_inner());
        let second_data = &started.last().unwrap().service_data;
        assert_eq!(
            second_data.len(),
            BEACON_LEN,
            "the broadcast beacon must stay at the legacy-safe 22-byte length, never grow to carry the MAC"
        );
        let second = DiscoveryBeacon::parse(second_data).unwrap();
        assert_eq!(
            second.wifi_direct_mac, None,
            "the MAC must not appear in the over-the-air advertisement, whatever set_local_wifi_direct_mac was told"
        );
    }

    /// HV-21/DEC-BLE-0008: the GATT identify-characteristic payload (served
    /// via `gatt_read(IRIS_IDENTIFY_CHARACTERISTIC)`, no advertisement size
    /// ceiling) DOES carry the real Wi-Fi Direct MAC once known — the
    /// complement to the advertisement-side test above, which asserts the
    /// MAC must NOT appear there.
    #[tokio::test]
    async fn identify_payload_carries_the_wifi_direct_mac_once_known() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        let adv = NodeAdvertisement {
            peer_id: PeerId([0x63; 32]),
            public_ip_addr: None,
            hostname: None,
            tags: Vec::new(),
        };
        t.start_advertising(adv).await.unwrap();
        let before = DiscoveryBeacon::parse(
            &adapter.inner.gatt_read(GattHandle(1), IRIS_IDENTIFY_CHARACTERISTIC).unwrap(),
        )
        .unwrap();
        assert_eq!(before.wifi_direct_mac, None, "no MAC known yet");

        t.set_local_wifi_direct_mac([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]).await;

        let raw = adapter.inner.gatt_read(GattHandle(1), IRIS_IDENTIFY_CHARACTERISTIC).unwrap();
        let after = DiscoveryBeacon::parse(&raw).unwrap();
        assert_eq!(
            after.wifi_direct_mac,
            Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
            "the GATT identify payload must carry the MAC once known, unlike the advertisement"
        );
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
                    None,
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
        // BLE-24: use per-address injection so each probe returns that peer's
        // own beacon — previously the global slot was overwritten each iteration,
        // meaning every resolved peer appeared identical (peer 9's beacon).
        let mut caps = CapabilityBits::empty();
        caps.set(CapabilityBits::GATT_UNICAST);
        for i in 0..10u8 {
            let addr = BleAddress([i, 0xBB, 0, 0, 0, 0]);
            let beacon = DiscoveryBeacon::build(caps, [i; 16], 0, None);
            adapter.inject_identify_read_for_address(addr, beacon);
            adapter.inject_scan_result(addr, Vec::new(), -50);
        }
        for i in 10..20u8 {
            adapter.inject_scan_result(BleAddress([i, 0xBB, 0, 0, 0, 0]), Vec::new(), -50);
        }
        let mut stream = t.discover_peers(DiscoveryConfig::default()).await.unwrap();
        let mut resolved: std::collections::HashSet<[u8; 16]> = std::collections::HashSet::new();
        while let Some(peer) = stream.next().await {
            // The lower 16 bytes of a candidate PeerId are the peer_short from
            // the beacon (upper 16 bytes are the 0xFF sentinel added by BLE-8).
            let short: [u8; 16] = peer.peer_id.0[..16].try_into().unwrap();
            resolved.insert(short);
        }
        let count = resolved.len();
        assert!(
            count <= 8,
            "iOS-leg probe budget must bound candidates (BLE-RT-C001/003), got {count}"
        );
        assert!(
            adapter.connect_count() <= 8,
            "iOS-leg probe budget must bound GATT connects, got {}",
            adapter.connect_count()
        );
        // Each resolved peer must have a distinct beacon short-id drawn from
        // the 10 injected peers (injected as [i; 16] for i in 0..10).
        // With per-address injection, each connect resolves a different beacon —
        // confirming attribution is not broken by single-slot overwriting.
        for short in &resolved {
            let uniform_byte = short[0];
            assert!(
                uniform_byte < 10 && short.iter().all(|&b| b == uniform_byte),
                "resolved peer_short {short:?} not from injected set"
            );
        }
        assert_eq!(
            resolved.len(),
            count,
            "no two resolved peers must share a short-id"
        );
    }

    /// BLE-RT-C005 regression, updated for HV-21/DEC-BLE-0008: the
    /// simulator no longer auto-mirrors the advertised `service_data` into
    /// the identify characteristic (it used to, on the now-outdated
    /// assumption that there was only ever one beacon) — `ble.rs`'s real
    /// `start_advertising` is what wires them explicitly via
    /// `set_identify_payload`, deliberately building the identify payload
    /// from the SAME `peer_short`/`caps`/`freshness` as the advertisement
    /// (just with the Wi-Fi Direct MAC folded in when known — HV-21). This
    /// test now exercises the real transport method, which is the actual
    /// BLE-RT-C005 guarantee: an honest connect-to-identify probe resolves
    /// what this node is really advertising, not a disconnected test-only
    /// injection — no `Some(mac)` case here (no known-peers-mac-aware
    /// case), so the ad and the identify payload are still byte-identical.
    #[tokio::test]
    async fn c005_transport_serves_the_advertised_beacon_on_identify_read() {
        let adapter = std::sync::Arc::new(RecordingBleAdapter::default());
        let t = BleTransport::new(Some(adapter.clone()));
        t.start_advertising(NodeAdvertisement {
            peer_id: PeerId([0xC5; 32]),
            public_ip_addr: None,
            hostname: Some("iris-peer".to_string()),
            tags: Vec::new(),
        })
        .await
        .unwrap();
        let advertised = adapter.started_adv.lock().unwrap_or_else(|p| p.into_inner()).last().unwrap().service_data.clone();
        let handle = adapter.inner.connect_gatt(BleAddress([0xC5; 6])).unwrap();
        let served = adapter
            .inner
            .gatt_read(handle, IRIS_IDENTIFY_CHARACTERISTIC)
            .expect("identify read must be served");
        adapter.inner.disconnect_gatt(handle);
        assert_eq!(
            served, advertised,
            "an honest connect-to-identify probe must resolve exactly what this node is advertising (BLE-RT-C005), with no known Wi-Fi Direct MAC to differ by"
        );
    }
}
