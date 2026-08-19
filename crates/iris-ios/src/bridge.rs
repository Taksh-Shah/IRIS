//! `bridge` — core-adapter traits implemented over the UniFFI projections.
//!
//! IOS-001 (`crates/iris-ios`): `BleBridge` closes the gap between the FFI
//! projection (`crate::ffi::ble_adapter::FfiBleAdapter`, the Swift
//! `IosBleAdapter`'s contract) and the `iris-core` transport trait
//! (`BleAdapter`) that `BleTransport` calls. Owned/UniFFI-safe conversions
//! (issue #2263); the two `MutexGuard`-returning BLE drains become internal
//! buffers then hand the guard to the transport's poller (proj-BLE-1).
//!
//! 11 ops: the projection forwards `gatt_read` for the connect-to-identify
//! path (DEC-BLE-002-0015 / IOS_DESIGN §4 AC-5) — unlike the Android bridge,
//! which is a documented `DeviceNotFound` placeholder. BLE-RT-C003 (AC-6) is
//! enforced inside the Swift `IosBleAdapter` and crosses back as
//! `IrisFfiError::Timeout` (mapped to `BleError::GattFailure` here — the core
//! `BleError` has no `Timeout` variant).

use std::sync::{Arc, Mutex, MutexGuard};

use iris_core::transport::ble::{
    AdvHandle, AdvertisementData, BleAdapter, BleAddress, BleError, GattHandle, GattWriteEvent,
    ScanFilter, ScanHandle, ScanResult, Uuid,
};

use crate::ffi::ble_adapter::{FfiAdvertisementData, FfiScanFilter};
use crate::ffi::error::IrisFfiError;

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

pub(crate) fn ffi_err_to_ble(e: IrisFfiError) -> BleError {
    match e {
        IrisFfiError::NotSupported => BleError::NotSupported,
        IrisFfiError::PermissionDenied => BleError::PermissionDenied,
        IrisFfiError::AdapterOff => BleError::AdapterOff,
        IrisFfiError::DeviceNotFound => BleError::DeviceNotFound,
        IrisFfiError::GattFailure(m) => BleError::GattFailure(m),
        other => BleError::GattFailure(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// BleBridge
// ---------------------------------------------------------------------------

/// Bridges the sync `BleAdapter` trait (11-op) onto the sync FFI projection.
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
        // `start_advertising`); on iOS the Swift adapter never transmits it on
        // the wire (CoreBluetooth has no service/manufacturer data API) and
        // serves it from IRIS_IDENTIFY_CHARACTERISTIC instead — the payload is
        // still passed so the Swift side can program the GATT server
        // (connect-to-identify, DEC-BLE-002-0002).
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

    fn gatt_read(&self, handle: GattHandle, char_uuid: Uuid) -> Result<Vec<u8>, BleError> {
        // iOS connect-to-identify (DEC-BLE-002-0015): forwarded to the Swift
        // adapter, which performs connect -> discoverServices([IRIS]) ->
        // discoverCharacteristics -> readValue(for: identifyChar) ->
        // didUpdateValueFor -> bytes. BLE-RT-C003 hard timeout is applied on
        // the Swift side and returns IrisFfiError::Timeout -> GattFailure here.
        self.ffi
            .gatt_read(handle.0, uuid_to_hex(char_uuid))
            .map_err(ffi_err_to_ble)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ffi::ble_adapter::tests::SimBle;

    const IRIS_IDENTIFY: Uuid = Uuid([
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd,
        0xef,
    ]);

    #[test]
    fn bridge_forwards_gatt_read_for_identify() {
        let sim = Arc::new(SimBle::default());
        let beacon = b"deadbeef-identify-beacon".to_vec();
        sim.inject_identify_read(beacon.clone());
        let bridge = BleBridge::new(sim.clone());
        let got = bridge
            .gatt_read(GattHandle(11), IRIS_IDENTIFY)
            .expect("identify read crosses the bridge");
        assert_eq!(got, beacon);
    }

    #[test]
    fn bridge_forwards_timeout_as_gatt_failure() {
        // Can't easily force SimBle to time out; assert that a projection
        // Timeout maps like every non-core error (documented BLE-RT-C003 seam).
        let e = ffi_err_to_ble(IrisFfiError::Timeout);
        assert!(matches!(e, BleError::GattFailure(_)));
    }
}
