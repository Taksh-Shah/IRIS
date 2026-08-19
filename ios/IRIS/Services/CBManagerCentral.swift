// IOS-001 AC-4..AC-9 / AC-10 — CBManagerCentral: real CBCentralManager +
// CBPeripheral driver implementing `BleCentralSeam`.
//
// Wraps the central manager (scan/connect/read + willRestoreState) and every
// connected CBPeripheral's discovery/read/MTU surfaces. Restoration identifier
// "IrisCentralManager" (DEC-BLE-002-0005) enables CoreBluetooth to deliver
// `willRestoreState` on a BLE relaunch; the delegate then re-arms the scan via
// the adapter (session/SessionRecovery.swift, AC-10/AC-11).
//
// macOS availability: CBCentralManager/CBPeripheral are macOS APIs too, but at
// runtime macOS permission dialogs apply; adapter-logic tests use the mock seam
// (AC-16) and never construct this class.
import Foundation
import CoreBluetooth

public final class RealBleCentralSeam: NSObject, BleCentralSeam, Sendable {
    public var delegate: BleCentralSeamDelegate?
    public private(set) var state: IOSBleCentralState = .unknown

    private let restoreIdentifier: String?

    public init(restoreIdentifier: String = IrisBleConstants.centralRestoreIdentifier) {
        self.restoreIdentifier = restoreIdentifier
        super.init()
    }

    /// The live CBCentralManager is created lazily so the mock-seam test path
    /// never instantiates CoreBluetooth (and the app's first create happens on
    /// a valid launch path only — AC-10).
    public private(set) lazy var manager: CBCentralManager = {
        var options: [String: Any] = [:]
        if let restoreIdentifier {
            options[CBCentralManagerOptionRestoreIdentifierKey] = restoreIdentifier
        }
        let m = CBCentralManager(delegate: self, queue: nil, options: options)
        synchronizeState(m.state)
        return m
    }()

    /// Peripheral handles created during scanning/connect; CoreBluetooth keeps
    /// them alive for the process, but we hold strong refs so restored
    /// `willRestoreState` peripherals can be re-driven.
    private var peripherals: [UUID: CBPeripheral] = [:]
    private let peripheralLock = NSLock()

    // MARK: - BleCentralSeam

    public func startScanning(serviceUuids: [CBUUID], options: [String: Any]) {
        guard !serviceUuids.isEmpty else { return } // AC-9: IRIS UUID always present
        manager.scanForPeripherals(withServices: serviceUuids, options: options)
    }

    public func stopScanning() {
        manager.stopScan()
    }

    public func connectPeripheral(identifier: UUID, options: [String: Any]?) {
        guard let p = peripheral(for: identifier) else { return }
        manager.connect(p, options: options)
    }

    public func cancelConnection(identifier: UUID) {
        guard let p = peripheral(for: identifier) else { return }
        manager.cancelPeripheralConnection(p)
    }

    public func retrievePeripherals(identifiers: [UUID]) {
        for p in manager.retrievePeripherals(withIdentifiers: identifiers) {
            remember(p)
        }
    }

    public func discoverServices(identifier: UUID, uuids: [CBUUID]?) {
        guard let p = peripheral(for: identifier) else { return }
        p.discoverServices(uuids)
    }

    public func discoverCharacteristics(identifier: UUID, characteristicUuids: [CBUUID]?, serviceUuid: CBUUID) {
        guard let p = peripheral(for: identifier),
              let service = p.services?.first(where: { $0.uuid == serviceUuid })
        else { return }
        p.discoverCharacteristics(characteristicUuids, for: service)
    }

    public func readValue(identifier: UUID, characteristicUuid: CBUUID) {
        guard let p = peripheral(for: identifier),
              let service = p.services?.first,
              let characteristic = service.characteristics?.first(where: { $0.uuid == characteristicUuid })
        else { return }
        p.readValue(for: characteristic)
    }

    public func writeValue(identifier: UUID, characteristicUuid: CBUUID, data: Data, withResponse: Bool) {
        guard let p = peripheral(for: identifier),
              let service = p.services?.first,
              let characteristic = service.characteristics?.first(where: { $0.uuid == characteristicUuid })
        else { return }
        p.writeValue(data, for: characteristic, type: withResponse ? .withResponse : .withoutResponse)
    }

    public func maximumWriteValueLength(identifier: UUID) -> Int {
        guard let p = peripheral(for: identifier) else { return 0 }
        // Negotiated ATT payload (cap 512); NEVER trap on a disconnected
        // peripheral (CoreBluetooth throws NSRangeException).
        guard p.state == .connected else { return 0 }
        return p.maximumWriteValueLength(for: .withResponse)
    }

    // MARK: - helpers

    private func peripheral(for identifier: UUID) -> CBPeripheral? {
        // Unregistered identifiers can still appear (restored peripherals);
        // re-retrieve so connectGatt can drive them.
        if let p = peripherals[identifier] { return p }
        if let p = manager.retrievePeripherals(withIdentifiers: [identifier]).first {
            remember(p)
            return p
        }
        return nil
    }

    private func remember(_ p: CBPeripheral) {
        p.delegate = self
        peripheralLock.lock()
        peripherals[p.identifier] = p
        peripheralLock.unlock()
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
            delegate?.centralSeamDidUpdateState(self, state: mapped)
        }
    }
}

// MARK: - CBCentralManagerDelegate

extension RealBleCentralSeam: CBCentralManagerDelegate {
    public func centralManagerDidUpdateState(_ central: CBCentralManager) {
        synchronizeState(central.state)
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        remember(peripheral)
        delegate?.centralSeam(self, didDiscover: peripheral.identifier, rssi: RSSI.intValue)
    }

    public func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        remember(peripheral)
        delegate?.centralSeam(self, didConnect: peripheral.identifier)
    }

    public func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        delegate?.centralSeam(self, didFailToConnect: peripheral.identifier, error: error)
    }

    public func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        delegate?.centralSeam(self, didDisconnect: peripheral.identifier, error: error)
    }

    public func centralManager(
        _ central: CBCentralManager,
        willRestoreState dict: [String: Any]
    ) {
        let restored = (dict[CBCentralManagerRestoredStatePeripheralsKey] as? [CBPeripheral]) ?? []
        let identifiers = restored.map { $0.identifier }
        for p in restored { remember(p) }
        delegate?.centralSeam(self, didRestore: identifiers)
    }
}

// MARK: - CBPeripheralDelegate

extension RealBleCentralSeam: CBPeripheralDelegate {
    public func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        let services = (peripheral.services ?? []).map { $0.uuid }
        delegate?.centralSeam(
            self,
            didCompleteServiceDiscovery: peripheral.identifier,
            services: services,
            error: error
        )
    }

    public func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        let chars = (service.characteristics ?? []).map { $0.uuid }
        delegate?.centralSeam(
            self,
            didCompleteCharacteristicDiscovery: peripheral.identifier,
            serviceUuid: service.uuid,
            characteristics: chars,
            error: error
        )
    }

    public func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        delegate?.centralSeam(
            self,
            didRead: peripheral.identifier,
            characteristicUuid: characteristic.uuid,
            data: characteristic.value,
            error: error
        )
    }
}