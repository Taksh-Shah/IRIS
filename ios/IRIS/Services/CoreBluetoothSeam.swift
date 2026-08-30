// IOS-001 AC-4..AC-9, AC-16 — CoreBluetooth seam abstractions.
//
// `IosBleAdapter` never touches `CBCentralManager`/`CBPeripheralManager`
// directly; it drives these seams so the macOS-host XCTest target (AC-16,
// IOS-CoreBluetooth-Mock pattern, RES-0025 RQ-3 finding 6) can exercise the
// full central+peripheral adapter logic with deterministic fakes. The real
// implementations (CBManagerCentral.swift / CBManagerPeripheral.swift) map
// 1:1 onto CoreBluetooth and carry the restoration identifiers mandated by
// DEC-BLE-002-0005 (`"IrisCentralManager"` / `"IrisPeripheralManager"`).
//
// Identities: peers are keyed by `UUID` (CBPeripheral.identifier / CBCentral
// identifier — the same remote-device UUID on both sides of CoreBluetooth).
// GATT UUIDs (service/characteristic) are `CBUUID` — the CoreBluetooth-native
// type. The FFI "address" string is a deterministic 12-hex token derived from
// the identifier UUID (IrisBleConstants.token), which round-trips through the
// Rust core's 6-byte BleAddress hex conversion in crate::bridge.
import Foundation
import CoreBluetooth

/// Central/peripheral manager power state (projection of the CoreBluetooth
/// CBManagerState values shared by CBCentralManager and CBPeripheralManager).
public enum IOSBleCentralState: String, Equatable, Sendable {
    case unknown
    case resetting
    case unsupported
    case unauthorized
    case poweredOff
    case poweredOn

    public var isPoweredOn: Bool { self == .poweredOn }
}

/// One characteristic served by the local GATT server (peripheral side).
public struct IOSBleCharacteristicSpec: Sendable, Equatable {
    public let uuid: CBUUID
    /// Value served on read (e.g. the 22-byte discovery beacon under
    /// IRIS_IDENTIFY_CHARACTERISTIC — DEC-BLE-002-0002).
    public let value: Data
    public let supportsRead: Bool
    public let supportsWrite: Bool

    public init(uuid: CBUUID, value: Data, supportsRead: Bool, supportsWrite: Bool) {
        self.uuid = uuid
        self.value = value
        self.supportsRead = supportsRead
        self.supportsWrite = supportsWrite
    }
}

/// Central-side seam drive protocol (CBCentralManager + CBPeripheral gateway).
public protocol BleCentralSeam: AnyObject, Sendable {
    var delegate: BleCentralSeamDelegate? { get set }
    var state: IOSBleCentralState { get }

    /// startScan equivalent. `serviceUuids` is ALWAYS non-empty on the IRIS path
    /// (AC-9 — the Rust core only ever filters by IRIS_SERVICE_UUID).
    func startScanning(serviceUuids: [CBUUID], options: [String: Any])
    func stopScanning()
    func connectPeripheral(identifier: UUID, options: [String: Any]?)
    func cancelConnection(identifier: UUID)
    func retrievePeripherals(identifiers: [UUID])
    func discoverServices(identifier: UUID, uuids: [CBUUID]?)
    func discoverCharacteristics(identifier: UUID, characteristicUuids: [CBUUID]?, serviceUuid: CBUUID)
    func readValue(identifier: UUID, characteristicUuid: CBUUID)
    func writeValue(identifier: UUID, characteristicUuid: CBUUID, data: Data, withResponse: Bool)
    /// maximumWriteValueLength(for: .withResponse) — negotiated ATT *payload*.
    func maximumWriteValueLength(identifier: UUID) -> Int
}

/// Central-side delegate (projection of CBCentralManagerDelegate + CBPeripheralDelegate).
public protocol BleCentralSeamDelegate: AnyObject {
    func centralSeamDidUpdateState(_ seam: BleCentralSeam, state: IOSBleCentralState)
    func centralSeam(_ seam: BleCentralSeam, didDiscover identifier: UUID, rssi: Int)
    func centralSeam(_ seam: BleCentralSeam, didConnect identifier: UUID)
    func centralSeam(_ seam: BleCentralSeam, didFailToConnect identifier: UUID, error: Error?)
    func centralSeam(_ seam: BleCentralSeam, didDisconnect identifier: UUID, error: Error?)
    func centralSeam(_ seam: BleCentralSeam, didCompleteServiceDiscovery identifier: UUID, services: [CBUUID], error: Error?)
    func centralSeam(_ seam: BleCentralSeam, didCompleteCharacteristicDiscovery identifier: UUID, serviceUuid: CBUUID, characteristics: [CBUUID], error: Error?)
    func centralSeam(_ seam: BleCentralSeam, didRead identifier: UUID, characteristicUuid: CBUUID, data: Data?, error: Error?)
    /// Bug #7+#8: write-response completion from peripheral(_:didWriteValueFor:error:).
    func centralSeam(_ seam: BleCentralSeam, didWriteValue identifier: UUID, characteristicUuid: CBUUID, error: Error?)
    /// willRestoreState projection: previously-connected peer identifiers.
    func centralSeam(_ seam: BleCentralSeam, didRestore identifiers: [UUID])
}

/// Peripheral-side seam drive protocol (CBPeripheralManager GATT server + advertiser).
public protocol BlePeripheralSeam: AnyObject, Sendable {
    var delegate: BlePeripheralSeamDelegate? { get set }
    var state: IOSBleCentralState { get }
    /// addService for the IRIS primary service (identify read + control write).
    func addService(serviceUuid: CBUUID, characteristics: [IOSBleCharacteristicSpec])
    /// startAdvertising — local name + service UUID ONLY. No service data,
    /// manufacturer data, or any other ad payload is ever emitted (AC-9 /
    /// RES-0024 DI-1: CoreBluetooth cannot transmit service data anyway).
    func startAdvertising(serviceUuid: CBUUID, localName: String?)
    func stopAdvertising()
    /// willRestoreState projection: service UUIDs recovered on relaunch.
    func restoredServiceUuids() -> [CBUUID]
}

/// Peripheral-side delegate projection.
public protocol BlePeripheralSeamDelegate: AnyObject {
    func peripheralSeamDidUpdateState(_ seam: BlePeripheralSeam, state: IOSBleCentralState)
    func peripheralSeam(_ seam: BlePeripheralSeam, didStartAdvertising error: Error?)
    func peripheralSeam(_ seam: BlePeripheralSeam, didReceiveWrite centralIdentifier: UUID, characteristicUuid: CBUUID, data: Data)
    func peripheralSeam(_ seam: BlePeripheralSeam, didRestoreServiceUuids uuids: [CBUUID])
}