// IOS-001 AC-4/AC-9/AC-10 — CBManagerPeripheral: real CBPeripheralManager
// driver implementing `BlePeripheralSeam`.
//
// Serves the IRIS primary GATT service (IRIS_SERVICE_UUID):
//   - IRIS_IDENTIFY_CHARACTERISTIC (read) — the discovery beacon
//     (connect-to-identify, DEC-BLE-002-0002). NEVER in ad data (AC-9,
//     RES-0024 DI-1): CoreBluetooth transmits a startAdvertising dictionary
//     containing local name + service UUIDs only. The beacon rides the
//     characteristic, not the air.
//   - control characteristic (write) — inbound frames drained via
//     IosBleAdapter.incomingGattWrites() (proj-BLE-1 drain semantics).
// Restoration identifier "IrisPeripheralManager" (DEC-BLE-002-0005);
// `willRestoreState` forwards restored service UUIDs to the delegate so
// SessionRecovery re-arms advertising (no auto-resume — RES-0025 RQ-4).
import Foundation
import CoreBluetooth

public final class RealBlePeripheralSeam: NSObject, BlePeripheralSeam, Sendable {
    public var delegate: BlePeripheralSeamDelegate?
    public private(set) var state: IOSBleCentralState = .unknown

    private let restoreIdentifier: String
    private var pendingService: (uuid: CBUUID, characteristics: [CBMutableCharacteristic])?
    private var pendingAdvertisement: (serviceUuid: CBUUID, localName: String?)?
    private var restoredServiceUuids: [CBUUID] = []

    public init(restoreIdentifier: String = IrisBleConstants.peripheralRestoreIdentifier) {
        self.restoreIdentifier = restoreIdentifier
        super.init()
    }

    public private(set) lazy var _manager: CBPeripheralManager = {
        let m = CBPeripheralManager(
            delegate: self,
            queue: nil,
            options: [CBPeripheralManagerOptionRestoreIdentifierKey: restoreIdentifier]
        )
        synchronizeState(m.state)
        return m
    }()

    // MARK: - BlePeripheralSeam

    public func addService(serviceUuid: CBUUID, characteristics: [IOSBleCharacteristicSpec]) {
        let cbCharacteristics: [CBMutableCharacteristic] = characteristics.map { spec in
            var properties: CBCharacteristicProperties = []
            if spec.supportsRead { properties.insert(.read) }
            if spec.supportsWrite { properties.insert([.write, .writeWithoutResponse]) }
            let permissions: CBAttributePermissions =
                (spec.supportsRead ? [.readable] : []) + (spec.supportsWrite ? [.writeable] : [])
            return CBMutableCharacteristic(
                type: spec.uuid,
                properties: properties,
                value: spec.supportsRead ? spec.value : nil,
                permissions: permissions
            )
        }
        let service = CBMutableService(type: serviceUuid, primary: true)
        service.characteristics = cbCharacteristics
        pendingService = (serviceUuid, cbCharacteristics)
        // The manager may not be powered on yet (app cold-start); queue.
        guard state.isPoweredOn else { return }
        manager.add(service)
    }

    public func startAdvertising(serviceUuid: CBUUID, localName: String?) {
        pendingAdvertisement = (serviceUuid, localName)
        guard state.isPoweredOn else { return }
        doStartAdvertising()
    }

    public func stopAdvertising() {
        pendingAdvertisement = nil
        if state.isPoweredOn {
            manager.stopAdvertising()
        }
    }

    public func restoredServiceUuids() -> [CBUUID] {
        restoredServiceUuids
    }

    private var manager: CBPeripheralManager { _manager }

    // MARK: - internals

    private func doStartAdvertising() {
        guard let ad = pendingAdvertisement else { return }
        // AC-9: the ONLY advertisement keys are ServiceUUIDs + (bounded) local
        // name. No service data, no manufacturer data — the beacon is served
        // from the identify characteristic instead.
        var dict: [String: Any] = [
            CBAdvertisementDataServiceUUIDsKey: [ad.serviceUuid],
        ]
        if let name = ad.localName, name.utf8.count <= IrisBleConstants.localNameMaxBytes {
            dict[CBAdvertisementDataLocalNameKey] = name
        }
        manager.startAdvertising(dict)
    }

    private func synchronizeState(_ cb: CBManagerState) {
        let mapped: IOSBleCentralState
        switch cb {
        case .unknown: mapped = .unknown
        case .resetting: mapped = .resetting
        case .unsupported: mapped = .unsupported
        case .unauthorized: mapped = .unauthorized
        case .poweredOff: mapped = .poweredOff
        case .poweredOn: mapped = .poweredOn
        @unknown default: mapped = .unknown
        }
        if mapped != state {
            state = mapped
            delegate?.peripheralSeamDidUpdateState(self, state: mapped)
        }
    }
}

// MARK: - CBPeripheralManagerDelegate

extension RealBlePeripheralSeam: CBPeripheralManagerDelegate {
    public func peripheralManagerDidUpdateState(_ peripheral: CBPeripheralManager) {
        synchronizeState(peripheral.state)
        guard peripheral.state == .poweredOn else { return }
        // Re-apply queued configuration from the cold-start window.
        if let (uuid, chars) = pendingService {
            let service = CBMutableService(type: uuid, primary: true)
            service.characteristics = chars
            manager.add(service)
        }
        doStartAdvertising()
    }

    public func peripheralManagerDidStartAdvertising(_ peripheral: CBPeripheralManager, error: Error?) {
        delegate?.peripheralSeam(self, didStartAdvertising: error)
    }

    public func peripheralManager(
        _ peripheral: CBPeripheralManager,
        didReceiveWrite requests: [CBATTRequest]
    ) {
        for request in requests {
            delegate?.peripheralSeam(
                self,
                didReceiveWrite: request.central.identifier,
                characteristicUuid: request.characteristic.uuid,
                data: request.value ?? Data()
            )
            peripheral.respond(to: request, withResult: .success)
        }
    }

    public func peripheralManager(_ peripheral: CBPeripheralManager, didReceiveRead request: CBATTRequest) {
        let value = request.characteristic.value ?? Data()
        if request.offset > value.count {
            peripheral.respond(to: request, withResult: .invalidOffset)
            return
        }
        request.value = request.offset == 0
            ? value
            : value.subdata(in: request.offset..<value.count)
        peripheral.respond(to: request, withResult: .success)
    }

    public func peripheralManager(
        _ peripheral: CBPeripheralManager,
        willRestoreState dict: [String: Any]
    ) {
        let services = (dict[CBPeripheralManagerRestoredStateServicesKey] as? [CBMutableService]) ?? []
        restoredServiceUuids = services.map { $0.uuid }
        delegate?.peripheralSeam(self, didRestoreServiceUuids: restoredServiceUuids)
    }
}