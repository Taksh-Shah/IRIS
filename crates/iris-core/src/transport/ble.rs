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

/// Platform-neutral BLE adapter trait (mirrors the spec; real adapters via FFI).
pub trait BleAdapter: Send + Sync + 'static {
    fn start_scan(&self, filter: ScanFilter) -> Result<ScanHandle, BleError>;
    fn stop_scan(&self, handle: ScanHandle);
    fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError>;
    fn stop_advertising(&self, handle: AdvHandle);
    fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError>;
    fn disconnect_gatt(&self, handle: GattHandle);
    fn gatt_write(&self, handle: GattHandle, char_uuid: Uuid, data: Vec<u8>) -> Result<(), BleError>;
    fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError>;
    fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>>;
}

/// In-memory BLE adapter for tests and simulation. No hardware involved.
#[derive(Default)]
pub struct SimulatedBleAdapter {
    events: std::sync::Mutex<Vec<GattWriteEvent>>,
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
        Ok(GattHandle(1))
    }
    fn disconnect_gatt(&self, _handle: GattHandle) {}
    fn gatt_write(&self, _handle: GattHandle, _char_uuid: Uuid, _data: Vec<u8>) -> Result<(), BleError> {
        Ok(())
    }
    fn set_mtu(&self, _handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
        Ok(mtu)
    }
    fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
        self.events.lock().unwrap()
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
    /// GATT handle returned by connect_gatt; reused by send() so traffic always
    /// reaches the connected peer (RED-0009-01, was hardcoded GattHandle(1)).
    gatt_handle: std::sync::Mutex<Option<GattHandle>>,
    /// Abort handle for the inbound-poll task so shutdown() actually stops it
    /// (RED-0009-02, was an unowned infinite loop leaking a task).
    poller: std::sync::Mutex<Option<tokio::task::AbortHandle>>,
}

impl BleTransport {
    pub fn new(adapter: Option<std::sync::Arc<dyn BleAdapter>>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        let state = AtomicState::default();
        state.store(TransportState::Available);
        let caps = TransportCapabilities {
            max_message_size: 512, // BLE ATT MTU ~ 512 max negotiated
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
            gatt_handle: std::sync::Mutex::new(None),
            poller: std::sync::Mutex::new(None),
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
        _config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        let adapter = self.adapter()?;
        let handle = adapter.start_scan(ScanFilter::default()).map_err(to_transport_err)?;
        *self.scan_handle.lock().unwrap() = Some(handle);
        // Real BLE scan results arrive via adapter callbacks (Android scan
        // results); the scaffold returns an empty stream. SCAFFOLD — BLK-0005.
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        if let Some(h) = *self.scan_handle.lock().unwrap() {
            self.adapter()?.stop_scan(h);
        }
        Ok(())
    }

    async fn start_advertising(&self, _info: NodeAdvertisement) -> Result<(), TransportError> {
        let adapter = self.adapter()?;
        let handle = adapter
            .start_advertising(AdvertisementData::default())
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
        self.set_state(TransportState::Connecting);
        // A prior connect may still be polling; abort it so two loops never
        // race for the adapter's write queue (RED-0001-06).
        if let Some(p) = self.poller.lock().unwrap().take() {
            p.abort();
        }
        // Peer's BLE address comes from transport_addresses (["ble", "AA:BB:.."]).
        let mac = peer
            .transport_addresses
            .iter()
            .find(|(t, _)| t == "ble")
            .map(|(_, addr)| parse_mac(addr))
            .ok_or(TransportError::PeerNotFound)?;
        let handle = adapter.connect_gatt(mac).map_err(to_transport_err)?;
        *self.gatt_handle.lock().unwrap() = Some(handle);
        self.set_state(TransportState::Connected);
        let established_at = Instant::now();
        // Poll inbound GATT writes into the incoming stream (SCAFFOLD: polling
        // adapter; real impl uses a callback/notify channel).
        let incoming = self.incoming_tx.clone();
        let adapter = adapter.clone();
        let tid = self.id.clone();
        let poller = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(50)).await;
                let mut writes = adapter.incoming_gatt_writes();
                if writes.is_empty() {
                    continue;
                }
                let drained: Vec<GattWriteEvent> = writes.drain(..).collect();
                drop(writes);
                for w in drained {
                    let _ = incoming.send(IncomingMessage {
                        peer_id: PeerId([0u8; 32]), // from connection state in real impl
                        transport_id: tid.0.clone(),
                        payload: w.data,
                        received_at: Instant::now(),
                    });
                }
            }
        });
        *self.poller.lock().unwrap() = Some(poller.abort_handle());
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
        if self.state.load() != TransportState::Connected {
            return Err(TransportError::NotConnected);
        }
        let adapter = self.adapter()?;
        let gatt = self.gatt_handle.lock().unwrap().ok_or(TransportError::NotConnected)?;
        let char_uuid = Uuid::from_u128(0x1); // IRIS control characteristic
        adapter
            .gatt_write(gatt, char_uuid, message.payload.clone())
            .map_err(to_transport_err)?;
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
        if let Some(p) = self.poller.lock().unwrap().take() {
            p.abort();
        }
        self.set_state(TransportState::Unavailable);
        if let Some(h) = self.gatt_handle.lock().unwrap().take() {
            self.adapter()?.disconnect_gatt(h);
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

        adapter.inject_write(GattHandle(1), Uuid::from_u128(1), b"inbound".to_vec());
        let mut incoming = t.incoming_messages();
        let got = tokio::time::timeout(Duration::from_millis(500), incoming.next()).await;
        let msg = got.expect("must receive injected write").unwrap();
        assert_eq!(msg.payload, b"inbound");
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
        assert_eq!(t.send(&PeerId([9u8; 32]), &msg).await.unwrap_err(), TransportError::NotConnected);
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
        fn gatt_write(&self, handle: GattHandle, char_uuid: Uuid, data: Vec<u8>) -> Result<(), BleError> {
            self.inner.gatt_write(handle, char_uuid, data)
        }
        fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
            self.inner.set_mtu(handle, mtu)
        }
        fn incoming_gatt_writes(&self) -> std::sync::MutexGuard<'_, Vec<GattWriteEvent>> {
            self.inner.incoming_gatt_writes()
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
            assert_eq!(scans[0], ScanHandle(1), "stop_scan must round-trip the start_scan handle");
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
        assert_eq!(advs.len(), 1, "stop_advertising must stop the advertisement");
        assert_eq!(advs[0], AdvHandle(1), "stop_advertising must round-trip the start handle");
    }

    #[test]
    fn capability_matrix_marks_ios_background_limited() {
        let t = BleTransport::new(None);
        let caps = t.capabilities();
        assert!(!caps.supports_background_ios, "iOS background BLE is limited (DISC-0006)");
        assert!(caps.supports_background_android);
        assert!(!caps.requires_special_hardware);
        assert_eq!(caps.cost_class, TransportCostClass::Free);
    }

    #[test]
    fn mac_parser_works() {
        let b = parse_mac("AA:BB:CC:DD:EE:FF");
        assert_eq!(b.0, [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]);
    }
}
