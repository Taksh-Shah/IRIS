//! `bridge` — core-adapter traits implemented over the UniFFI projections.
//!
//! AC-4 (ANDROID_DESIGN §4): the FFI adapter traits (`crate::ffi`) are the
//! Kotlin contract; the `iris-core` transport traits are what the transports
//! (`BleTransport`, `WifiAwareTransport`, `WifiDirectTransport`) call. These
//! bridges close that gap with owned/UniFFI-safe conversions (issue #2263):
//!
//! - `BleBridge`        → `iris_core::transport::ble::BleAdapter` (sync)
//! - `WifiAwareBridge`  → `iris_core::transport::wifiaware::WifiAwareAdapter` (async)
//! - `WifiDirectBridge` → `iris_core::transport::wifi_direct::WifiDirectAdapter` (async)
//!
//! The two `MutexGuard`-returning BLE drains (proj-BLE-1) become internal
//! buffers: each drain first transfers the FFI `Vec` into the bridge's buffer,
//! then hands the guard to the transport's poller.

use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::Stream;

use iris_core::transport::ble::{
    AdvHandle, AdvertisementData, BleAdapter, BleAddress, BleError, GattHandle, GattWriteEvent,
    ScanFilter, ScanHandle, ScanResult, Uuid,
};
use iris_core::transport::wifi_direct::{
    GroupConfig, GroupInfo, IncomingWifiDirectData, OperatingBand,
    PeerDiscovery as WdPeerDiscovery, PeerHandle as WdPeerHandle, WifiDirectAdapter,
};
use iris_core::transport::wifiaware::{
    IncomingNdpData, NdpHandle, PeerDiscovery as WaPeerDiscovery, PeerHandle as WaPeerHandle,
    PublishConfig, WifiAwareAdapter,
};

use crate::ffi::ble_adapter::{FfiAdvertisementData, FfiScanFilter};
use crate::ffi::error::IrisFfiError;
use crate::ffi::wifi_aware_adapter::{
    FfiIncomingNdpData, FfiPeerDiscovery, FfiPublishConfig, FfiWifiAwareAdapter,
};
use crate::ffi::wifi_direct_adapter::{
    FfiDirectPeerDiscovery, FfiGroupConfig, FfiGroupInfo, FfiIncomingWifiDirectData,
    FfiOperatingBand, FfiWifiDirectAdapter,
};

// ---------------------------------------------------------------------------
// Conversion helpers (owned, UniFFI-safe)
// ---------------------------------------------------------------------------

/// 16-byte UUID ↔ 32-hex string (dash/colon tolerant on parse).
fn uuid_to_hex(u: Uuid) -> String {
    let mut out = String::with_capacity(32);
    for b in u.0 {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn hex_to_uuid(s: &str) -> Option<Uuid> {
    let clean: String = s.chars().filter(|c| *c != '-' && *c != ':').collect();
    if clean.len() != 32 {
        return None;
    }
    let mut b = [0u8; 16];
    for (i, pair) in clean.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(pair).ok()?;
        b[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(Uuid(b))
}

/// 6-byte MAC ↔ 12-hex string (dash/colon tolerant on parse).
fn ble_addr_to_hex(a: BleAddress) -> String {
    let mut out = String::with_capacity(12);
    for b in a.0 {
        out.push_str(&format!("{b:02X}"));
    }
    out
}

fn hex_to_ble_addr(s: &str) -> Option<BleAddress> {
    let clean: String = s.chars().filter(|c| *c != ':' && *c != '-').collect();
    if clean.len() != 12 {
        return None;
    }
    let mut b = [0u8; 6];
    for (i, pair) in clean.as_bytes().chunks(2).enumerate() {
        let hex = std::str::from_utf8(pair).ok()?;
        b[i] = u8::from_str_radix(hex, 16).ok()?;
    }
    Some(BleAddress(b))
}

fn ffi_err_to_ble(e: IrisFfiError) -> BleError {
    match e {
        IrisFfiError::NotSupported => BleError::NotSupported,
        IrisFfiError::PermissionDenied => BleError::PermissionDenied,
        IrisFfiError::AdapterOff => BleError::AdapterOff,
        IrisFfiError::DeviceNotFound => BleError::DeviceNotFound,
        IrisFfiError::GattFailure(m) => BleError::GattFailure(m),
        other => BleError::GattFailure(other.to_string()),
    }
}

fn band_to_ffi(b: OperatingBand) -> FfiOperatingBand {
    match b {
        OperatingBand::Auto => FfiOperatingBand::Auto,
        OperatingBand::Ghz24 => FfiOperatingBand::Ghz24,
        OperatingBand::Ghz5 => FfiOperatingBand::Ghz5,
        OperatingBand::Ghz6 => FfiOperatingBand::Ghz6,
    }
}

// ---------------------------------------------------------------------------
// BleBridge
// ---------------------------------------------------------------------------

/// Bridges the sync `BleAdapter` trait onto the sync FFI projection.
pub struct BleBridge {
    ffi: Arc<dyn crate::ffi::ble_adapter::FfiBleAdapter>,
    writes: Mutex<Vec<GattWriteEvent>>,
    scans: Mutex<Vec<ScanResult>>,
}

impl BleBridge {
    pub fn new(ffi: Arc<dyn crate::ffi::ble_adapter::FfiBleAdapter>) -> Self {
        Self {
            ffi,
            writes: Mutex::new(Vec::new()),
            scans: Mutex::new(Vec::new()),
        }
    }
}

impl BleAdapter for BleBridge {
    fn start_scan(&self, filter: ScanFilter) -> Result<ScanHandle, BleError> {
        self.ffi
            .start_scan(FfiScanFilter {
                service_uuid: filter
                    .service_uuids
                    .first()
                    .map(|u| uuid_to_hex(*u))
                    .unwrap_or_default(),
                address: String::new(),
                rssi_floor: filter.rssi_threshold.unwrap_or(-95),
            })
            .map(ScanHandle)
            .map_err(ffi_err_to_ble)
    }

    fn stop_scan(&self, handle: ScanHandle) {
        self.ffi.stop_scan(handle.0);
    }

    fn start_advertising(&self, data: AdvertisementData) -> Result<AdvHandle, BleError> {
        // The IRIS beacon rides in `service_data` (see BleTransport
        // `start_advertising`); the projection advertises those raw bytes as a
        // non-connectable broadcast (DEC-BLE-0006).
        self.ffi
            .start_advertising(FfiAdvertisementData {
                payload: data.service_data,
                non_connectable: true,
            })
            .map(AdvHandle)
            .map_err(ffi_err_to_ble)
    }

    fn stop_advertising(&self, handle: AdvHandle) {
        self.ffi.stop_advertising(handle.0);
    }

    fn connect_gatt(&self, address: BleAddress) -> Result<GattHandle, BleError> {
        self.ffi
            .connect_gatt(ble_addr_to_hex(address))
            .map(GattHandle)
            .map_err(ffi_err_to_ble)
    }

    fn disconnect_gatt(&self, handle: GattHandle) {
        self.ffi.disconnect_gatt(handle.0);
    }

    fn gatt_write(
        &self,
        handle: GattHandle,
        char_uuid: Uuid,
        data: Vec<u8>,
    ) -> Result<(), BleError> {
        self.ffi
            .gatt_write(handle.0, uuid_to_hex(char_uuid), data)
            .map_err(ffi_err_to_ble)
    }

    fn gatt_read(&self, _handle: GattHandle, _char_uuid: Uuid) -> Result<Vec<u8>, BleError> {
        // Android discovery skips the connect-to-identify read: IRIS beacons
        // ride in ad-carried service data on Android (DEC-BLE-002-0002).
        // Android↔iOS identify-characteristic reads (discovering a UUID-only
        // iOS node from Android) land with the iOS adapter work under IOS-001
        // + an ANDROID-001 FFI follow-up (BLE-002 known_limitation).
        Err(BleError::DeviceNotFound)
    }

    fn set_mtu(&self, handle: GattHandle, mtu: u16) -> Result<u16, BleError> {
        self.ffi.set_mtu(handle.0, mtu).map_err(ffi_err_to_ble)
    }

    fn incoming_gatt_writes(&self) -> MutexGuard<'_, Vec<GattWriteEvent>> {
        let mut buf = self.writes.lock().unwrap();
        buf.clear();
        buf.extend(
            self.ffi
                .incoming_gatt_writes()
                .into_iter()
                .map(|e| GattWriteEvent {
                    handle: GattHandle(e.handle),
                    char_uuid: hex_to_uuid(&e.char_uuid).unwrap_or(Uuid([0u8; 16])),
                    data: e.data,
                }),
        );
        buf
    }

    fn scan_results(&self) -> MutexGuard<'_, Vec<ScanResult>> {
        let mut buf = self.scans.lock().unwrap();
        buf.clear();
        buf.extend(self.ffi.scan_results().into_iter().map(|r| ScanResult {
            address: hex_to_ble_addr(&r.address).unwrap_or(BleAddress([0u8; 6])),
            payload: r.payload,
            rssi: r.rssi,
        }));
        buf
    }
}

// ---------------------------------------------------------------------------
// WifiAwareBridge
// ---------------------------------------------------------------------------

/// Bridges the async `WifiAwareAdapter` trait onto the async FFI projection.
/// FFI-16 / BLE-28: neither Wi-Fi Aware nor Wi-Fi Direct's Android bridge
/// overrode `availability_stream()`, so both inherited the trait default of
/// `None` — the transport's `spawn_avail_watcher` sees no stream and never
/// spawns anything, so the `Degraded`/recovery state machine never runs on a
/// real device (state only changes when a send or connect happens to call
/// `is_available()`). The FFI projection has no push callback for this, but
/// `is_available()` is a cheap, synchronous, already-working call — poll it
/// on a ticker and synthesize the stream the transport already knows how to
/// consume. The consumer no-ops on a repeated value, so no de-duplication is
/// needed here.
const AVAILABILITY_POLL_MS: u64 = 500;

fn poll_availability_stream(
    is_available: impl Fn() -> bool + Send + 'static,
) -> Pin<Box<dyn Stream<Item = bool> + Send + 'static>> {
    let interval = tokio::time::interval(Duration::from_millis(AVAILABILITY_POLL_MS));
    Box::pin(futures_util::stream::unfold(
        (is_available, interval),
        |(is_available, mut interval)| async move {
            interval.tick().await;
            let available = is_available();
            Some((available, (is_available, interval)))
        },
    ))
}

pub struct WifiAwareBridge {
    ffi: Arc<dyn FfiWifiAwareAdapter>,
}

impl WifiAwareBridge {
    pub fn new(ffi: Arc<dyn FfiWifiAwareAdapter>) -> Self {
        Self { ffi }
    }
}

#[async_trait]
impl WifiAwareAdapter for WifiAwareBridge {
    async fn start(&self) -> Result<(), String> {
        self.ffi.start().await.map_err(|e| e.to_string())
    }

    async fn subscribe(&self) -> Result<(), String> {
        self.ffi.subscribe().await.map_err(|e| e.to_string())
    }

    async fn unsubscribe(&self) -> Result<(), String> {
        self.ffi.unsubscribe().await.map_err(|e| e.to_string())
    }

    async fn publish(&self, config: &PublishConfig) -> Result<(), String> {
        self.ffi
            .publish(FfiPublishConfig {
                service_name: config.service_name.clone(),
                instance_id: config.instance_id,
                cached: config.cached,
                ttl_s: config.ttl_s,
            })
            .await
            .map_err(|e| e.to_string())
    }

    async fn unpublish(&self) -> Result<(), String> {
        self.ffi.unpublish().await.map_err(|e| e.to_string())
    }

    async fn matches(&self) -> Result<Vec<WaPeerDiscovery>, String> {
        Ok(self
            .ffi
            .matches()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|m: FfiPeerDiscovery| WaPeerDiscovery {
                peer_handle: WaPeerHandle(m.peer_handle),
                service_specific_info: m.service_specific_info,
            })
            .collect())
    }

    async fn open_ndp(&self, peer_handle: WaPeerHandle) -> Result<NdpHandle, String> {
        self.ffi
            .open_ndp(peer_handle.0)
            .await
            .map(NdpHandle)
            .map_err(|e| e.to_string())
    }

    async fn close_ndp(&self, ndp: NdpHandle) -> Result<(), String> {
        self.ffi.close_ndp(ndp.0).await.map_err(|e| e.to_string())
    }

    async fn ndp_send(&self, ndp: NdpHandle, payload: &[u8]) -> Result<(), String> {
        self.ffi
            .ndp_send(ndp.0, payload.to_vec())
            .await
            .map_err(|e| e.to_string())
    }

    async fn incoming_ndp(&self) -> Result<Vec<IncomingNdpData>, String> {
        Ok(self
            .ffi
            .incoming_ndp()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|d: FfiIncomingNdpData| IncomingNdpData {
                ndp: NdpHandle(d.ndp_handle),
                sender: d
                    .sender
                    .and_then(|s| <[u8; 32]>::try_from(s).ok())
                    .map(iris_core::message::PeerId::from_bytes),
                payload: d.payload,
            })
            .collect())
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.ffi.shutdown().await.map_err(|e| e.to_string())
    }

    fn is_available(&self) -> bool {
        self.ffi.is_available()
    }

    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        let ffi = self.ffi.clone();
        Some(poll_availability_stream(move || ffi.is_available()))
    }
}

// ---------------------------------------------------------------------------
// WifiDirectBridge
// ---------------------------------------------------------------------------

/// Bridges the async `WifiDirectAdapter` trait onto the async FFI projection.
pub struct WifiDirectBridge {
    ffi: Arc<dyn FfiWifiDirectAdapter>,
}

impl WifiDirectBridge {
    pub fn new(ffi: Arc<dyn FfiWifiDirectAdapter>) -> Self {
        Self { ffi }
    }
}

#[async_trait]
impl WifiDirectAdapter for WifiDirectBridge {
    async fn start(&self) -> Result<(), String> {
        self.ffi.start().await.map_err(|e| e.to_string())
    }

    async fn start_dns_sd(&self) -> Result<(), String> {
        self.ffi.start_dns_sd().await.map_err(|e| e.to_string())
    }

    async fn stop_dns_sd(&self) -> Result<(), String> {
        self.ffi.stop_dns_sd().await.map_err(|e| e.to_string())
    }

    async fn start_discovery(&self) -> Result<(), String> {
        self.ffi.start_discovery().await.map_err(|e| e.to_string())
    }

    async fn stop_discovery(&self) -> Result<(), String> {
        self.ffi.stop_discovery().await.map_err(|e| e.to_string())
    }

    async fn matches(&self) -> Vec<WdPeerDiscovery> {
        self.ffi
            .matches()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m: FfiDirectPeerDiscovery| WdPeerDiscovery {
                peer_handle: WdPeerHandle(m.peer_handle),
                service_name: m.service_name,
                txt_record: m.txt_record,
            })
            .collect()
    }

    async fn create_group(&self, config: &GroupConfig) -> Result<GroupInfo, String> {
        self.ffi
            .create_group(FfiGroupConfig {
                go_intent: config.go_intent,
                persistent: config.persistent,
                band: band_to_ffi(config.band),
            })
            .await
            .map(group_info_from_ffi)
            .map_err(|e| e.to_string())
    }

    async fn join_group(
        &self,
        go: WdPeerHandle,
        config: &GroupConfig,
    ) -> Result<GroupInfo, String> {
        self.ffi
            .join_group(
                go.0,
                FfiGroupConfig {
                    go_intent: config.go_intent,
                    persistent: config.persistent,
                    band: band_to_ffi(config.band),
                },
            )
            .await
            .map(group_info_from_ffi)
            .map_err(|e| e.to_string())
    }

    async fn add_client(&self, client: WdPeerHandle) -> Result<(), String> {
        self.ffi
            .add_client(client.0)
            .await
            .map_err(|e| e.to_string())
    }

    async fn remove_group(&self) -> Result<(), String> {
        self.ffi.remove_group().await.map_err(|e| e.to_string())
    }

    async fn group_info(&self) -> Option<GroupInfo> {
        self.ffi.group_info().await.map(group_info_from_ffi)
    }

    fn go_addr(&self) -> Option<String> {
        self.ffi.go_addr()
    }

    async fn set_operating_band(&self, band: OperatingBand) -> Result<(), String> {
        self.ffi
            .set_operating_band(band_to_ffi(band))
            .await
            .map_err(|e| e.to_string())
    }

    async fn p2p_send(&self, peer: WdPeerHandle, payload: &[u8]) -> Result<(), String> {
        self.ffi
            .p2p_send(peer.0, payload.to_vec())
            .await
            .map_err(|e| e.to_string())
    }

    async fn incoming(&self) -> Vec<IncomingWifiDirectData> {
        self.ffi
            .incoming()
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|d: FfiIncomingWifiDirectData| IncomingWifiDirectData {
                sender: d
                    .sender
                    .and_then(|s| <[u8; 32]>::try_from(s).ok())
                    .map(iris_core::message::PeerId::from_bytes),
                payload: d.payload,
            })
            .collect()
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.ffi.shutdown().await.map_err(|e| e.to_string())
    }

    fn is_available(&self) -> bool {
        self.ffi.is_available()
    }

    fn availability_stream(&self) -> Option<Pin<Box<dyn Stream<Item = bool> + Send + 'static>>> {
        let ffi = self.ffi.clone();
        Some(poll_availability_stream(move || ffi.is_available()))
    }
}

fn group_info_from_ffi(g: FfiGroupInfo) -> GroupInfo {
    GroupInfo {
        group_id: g.group_id,
        go: WdPeerHandle(g.go),
        go_addr: g.go_addr,
        clients: g.clients.into_iter().map(WdPeerHandle).collect(),
    }
}
