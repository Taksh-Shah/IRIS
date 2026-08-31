//! `BleAdapter` — UniFFI foreign-trait projection (sync, by-value, 11-op).
//!
//! Mirrors `iris_core::transport::ble::BleAdapter` with the **11-op iOS
//! surface** (IOS_DESIGN §4 table: 10-op + `gatt_read` row, DEC-BLE-002-0015)
//! using owned, UniFFI-supported types (issue #2263: no references across
//! FFI). The real trait's `MutexGuard`-returning drains
//! (`incoming_gatt_writes()`, `scan_results()`) become `Vec`-returning drain
//! ops — the recorded spike finding (proj-BLE-1). Projection is implemented by
//! the Swift `IosBleAdapter` (CoreBluetooth) and simulated in Rust unit tests.

use crate::ffi::error::IrisFfiError;

/// Owned scan filter (mirrors `ble::ScanFilter` fields flattened to FFI-safe).
#[derive(Debug, Clone, Default, uniffi::Record)]
pub struct FfiScanFilter {
    /// Service UUID hex ("" = all). The Swift `IosBleAdapter` REQUIRES this to
    /// be the IRIS service UUID for background scanning (AC-9); UUID-less
    /// configs are rejected.
    pub service_uuid: String,
    /// Address to filter on ("" = any).
    pub address: String,
    /// RSSI floor dBm (e.g. -95).
    pub rssi_floor: i32,
}

/// Owned advertisement payload for the discovery beacon (`ble::AdvertisementData`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiAdvertisementData {
    /// Raw bytes the platform must advertise. On iOS the beacon is NEVER on
    /// the wire as ad data (CoreBluetooth has no service/manufacturer data);
    /// `IosBleAdapter` serves it from `IRIS_IDENTIFY_CHARACTERISTIC` instead
    /// (connect-to-identify, DEC-BLE-002-0002).
    pub payload: Vec<u8>,
    /// When true, advertise as non-connectable (broadcast beacon).
    pub non_connectable: bool,
}

/// Owned scan-result drain (`ble::ScanResult`; replaces the MutexGuard drain).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiScanResult {
    pub address: String,
    /// Empty on iOS: advertisementData carries NO IRIS service data (RES-0024
    /// DI-1) — RSSI + identity only; the connect-to-identify branch probes
    /// empty-payload results.
    pub payload: Vec<u8>,
    pub rssi: i32,
}

/// Owned inbound GATT write (`ble::GattWriteEvent`; replaces the MutexGuard drain).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiGattWriteEvent {
    /// Peer attribute handle.
    pub handle: u64,
    /// Characteristic UUID hex.
    pub char_uuid: String,
    pub data: Vec<u8>,
}

/// Foreign-trait projection of `ble::BleAdapter` (11-op iOS contract).
/// Implemented by `IosBleAdapter` (Swift/CoreBluetooth); simulated in Rust
/// unit tests. `start_advertising` drops service data at the FFI boundary on
/// iOS (the wire carries only local name + service UUIDs).
#[uniffi::export(with_foreign)]
pub trait FfiBleAdapter: Send + Sync + 'static {
    fn start_scan(&self, filter: FfiScanFilter) -> Result<u64, IrisFfiError>;
    fn stop_scan(&self, handle: u64);
    fn start_advertising(&self, data: FfiAdvertisementData) -> Result<u64, IrisFfiError>;
    fn stop_advertising(&self, handle: u64);
    fn connect_gatt(&self, address: String) -> Result<u64, IrisFfiError>;
    fn disconnect_gatt(&self, handle: u64);
    fn gatt_write(&self, handle: u64, char_uuid: String, data: Vec<u8>)
        -> Result<(), IrisFfiError>;
    /// Connect-to-identify read (DEC-BLE-002-0015 / IOS_DESIGN §4): fetch the
    /// peer's discovery beacon from `IRIS_IDENTIFY_CHARACTERISTIC` ->
    /// `DiscoveryBeacon::parse` -> candidate peer. BLE-RT-C003 timeout is
    /// enforced by the Swift side and surfaced as `IrisFfiError::Timeout`.
    fn gatt_read(&self, handle: u64, char_uuid: String) -> Result<Vec<u8>, IrisFfiError>;
    /// No-op request returning `maximumWriteValueLength(for: .withResponse)`
    /// (negotiated ATT payload, cap 512; AC-8).
    fn set_mtu(&self, handle: u64, mtu: u16) -> Result<u16, IrisFfiError>;
    /// Drain inbound GATT writes (projection of the MutexGuard drain).
    fn incoming_gatt_writes(&self) -> Vec<FfiGattWriteEvent>;
    /// Drain scan results (projection of the MutexGuard drain).
    fn scan_results(&self) -> Vec<FfiScanResult>;
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Rust-side simulated adapter: proves the projection trait can be driven
    /// by a plain Rust implementation (the Swift `IosBleAdapter` is the FFI
    /// twin). Records FFI calls (atomics) and serves a configurable
    /// identify-characteristic value so the 11-op surface incl. `gatt_read`
    /// can be exercised end-to-end in the crate's spike tests (G-IOS, AC-3).
    pub(crate) struct SimBle {
        pub(crate) identify_value: Mutex<Vec<u8>>,
        pub(crate) scans: std::sync::atomic::AtomicU64,
        pub(crate) adverts: std::sync::atomic::AtomicU64,
        pub(crate) connects: std::sync::atomic::AtomicU64,
        pub(crate) gatt_reads: std::sync::atomic::AtomicU64,
    }

    impl Default for SimBle {
        fn default() -> Self {
            Self {
                identify_value: Mutex::new(Vec::new()),
                scans: std::sync::atomic::AtomicU64::new(0),
                adverts: std::sync::atomic::AtomicU64::new(0),
                connects: std::sync::atomic::AtomicU64::new(0),
                gatt_reads: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }

    impl SimBle {
        pub(crate) fn inject_identify_read(&self, data: Vec<u8>) {
            *self.identify_value.lock().unwrap() = data;
        }
    }

    impl FfiBleAdapter for SimBle {
        fn start_scan(&self, filter: FfiScanFilter) -> Result<u64, IrisFfiError> {
            if filter.service_uuid.is_empty() {
                return Err(IrisFfiError::InvalidArgument(
                    "IRIS service UUID mandatory (AC-9)".into(),
                ));
            }
            self.scans
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(7)
        }
        fn stop_scan(&self, _h: u64) {}
        fn start_advertising(&self, _d: FfiAdvertisementData) -> Result<u64, IrisFfiError> {
            self.adverts
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(9)
        }
        fn stop_advertising(&self, _h: u64) {}
        fn connect_gatt(&self, _a: String) -> Result<u64, IrisFfiError> {
            self.connects
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(11)
        }
        fn disconnect_gatt(&self, _h: u64) {}
        fn gatt_write(&self, _h: u64, _c: String, data: Vec<u8>) -> Result<(), IrisFfiError> {
            assert_eq!(data, b"hi".to_vec());
            Ok(())
        }
        fn gatt_read(&self, _h: u64, char_uuid: String) -> Result<Vec<u8>, IrisFfiError> {
            self.gatt_reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            // The identify-characteristic contract (DEC-BLE-002-0015): the
            // read is only valid for IRIS_IDENTIFY_CHARACTERISTIC.
            // Bug #39: real IRIS identify UUID is 02000000… not 0123456789abcdef…
            assert_eq!(char_uuid, "02000000000000000000000000000000");
            Ok(self.identify_value.lock().unwrap().clone())
        }
        fn set_mtu(&self, _h: u64, mtu: u16) -> Result<u16, IrisFfiError> {
            Ok(mtu.min(512))
        }
        fn incoming_gatt_writes(&self) -> Vec<FfiGattWriteEvent> {
            Vec::new()
        }
        fn scan_results(&self) -> Vec<FfiScanResult> {
            Vec::new()
        }
    }

    #[test]
    fn projection_trait_drives_ops() {
        let a = Arc::new(SimBle::default());
        assert_eq!(
            a.start_scan(FfiScanFilter {
                service_uuid: "1812".into(),
                ..Default::default()
            })
            .unwrap(),
            7
        );
        // AC-9: UUID-less scan filter is rejected at the FFI boundary.
        assert!(matches!(
            a.start_scan(FfiScanFilter::default()),
            Err(IrisFfiError::InvalidArgument(_))
        ));
        assert_eq!(a.set_mtu(1, 300).unwrap(), 300);
        assert_eq!(a.set_mtu(1, 900).unwrap(), 512);
        let _ = a.gatt_write(1, String::new(), b"hi".to_vec());
        assert_eq!(a.connects.load(std::sync::atomic::Ordering::Relaxed), 0);
    }

    #[test]
    fn gatt_read_round_trip_returns_identify_beacon() {
        let a = Arc::new(SimBle::default());
        let beacon = b"\x00\x01iris-beacon-22-bytes".to_vec();
        a.inject_identify_read(beacon.clone());
        let got = a
            .gatt_read(11, "02000000000000000000000000000000".into())
            .expect("identify read succeeds");
        assert_eq!(got, beacon);
        assert_eq!(a.gatt_reads.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
