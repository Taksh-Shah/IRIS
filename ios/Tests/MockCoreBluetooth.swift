// IOS-001 AC-16 — MockCoreBluetooth: IOS-CoreBluetooth-Mock seam for XCTest.
//
// The iOS Simulator has NO CoreBluetooth stack (RES-0025 RQ-3 finding 6), so the
// macOS-host test bundle drives `IosBleAdapter` through this deterministic fake
// implementing `BleCentralSeam` + `BlePeripheralSeam`. The seam records every
// drive-op (assertions on AC-9 ad/scan dicts, AC-8 MTU), simulates the
// connect-to-identify sequence by emitting delegate events, and supports a
// configurable read delay so BLE-RT-C003 timeout is testable (AC-6).
import Foundation
import CoreBluetooth
import XCTest

/// One fake peer the central side can discover/connect/read.
public final class MockPeer {
    public let identifier: UUID
    public var rssi: Int
    public var identifyValue: Data
    public var connected = false

    public init(identifier: UUID, rssi: Int = -60, identifyValue: Data = Data()) {
        self.identifier = identifier
        self.rssi = rssi
        self.identifyValue = identifyValue
    }
}

/// Deterministic CoreBluetooth fake (central manager + peripheral manager).
/// Auto-replies to connect/discover/read unless a read delay is configured.
public final class MockBleSeam: BleCentralSeam, BlePeripheralSeam {

    // MARK: - Delegate attachments

    public var delegate: BleCentralSeamDelegate?
    public var peripheralDelegate: BlePeripheralSeamDelegate?

    // MARK: - State

    public var state: IOSBleCentralState = .poweredOn
    private let lock = NSLock()

    // MARK: - Recorded drive ops (assertion surface)

    public var startScanRecords: [([CBUUID], [String: Any])] = []
    public var stopScanCount = 0
    public var connectRecords: [UUID] = []
    public var cancelConnectionRecords: [UUID] = []
    public var discoverServicesRecords: [(UUID, [CBUUID]?)] = []
    public var discoverCharacteristicsRecords: [(UUID, [CBUUID]?, CBUUID)] = []
    public var readValueRecords: [(UUID, CBUUID)] = []
    public var writeValueRecords: [(UUID, CBUUID, Data, Bool)] = []
    public var maximumWriteValueLengthValue = 185
    public var addServiceRecords: [CBUUID] = []
    public var startAdvertisingRecords: [(CBUUID, String?)] = []
    public var stopAdvertisingCount = 0

    // MARK: - Configurable behavior

    public var peers: [UUID: MockPeer] = [:]
    /// Delays the didUpdateValueFor response (BLE-RT-C003 timeout tests).
    public var readDelay: TimeInterval = 0
    /// If set, service discovery fails with this error.
    public var serviceDiscoveryError: Error?

    // MARK: - Central side (BleCentralSeam)

    public func startScanning(serviceUuids: [CBUUID], options: [String: Any]) {
        lock.lock(); defer { lock.unlock() }
        startScanRecords.append((serviceUuids, options))
    }

    public func stopScanning() {
        lock.lock(); defer { lock.unlock() }
        stopScanCount += 1
    }

    public func connectPeripheral(identifier: UUID, options: [String: Any]?) {
        lock.lock(); defer { lock.unlock() }
        connectRecords.append(identifier)
        peers[identifier]?.connected = true
        // Synchronous delegate delivery: the adapter's gatt_read blocks on a
        // semaphore while driving connect->discover->read, so the mock must
        // reply on the caller thread (no main-queue coupling).
        delegate?.centralSeam(self, didConnect: identifier)
    }

    public func cancelConnection(identifier: UUID) {
        lock.lock(); defer { lock.unlock() }
        cancelConnectionRecords.append(identifier)
        peers[identifier]?.connected = false
        delegate?.centralSeam(self, didDisconnect: identifier, error: nil)
    }

    public func retrievePeripherals(identifiers: [UUID]) {
        // No-op in the mock: peers are pre-seeded.
    }

    public func discoverServices(identifier: UUID, uuids: [CBUUID]?) {
        lock.lock(); defer { lock.unlock() }
        discoverServicesRecords.append((identifier, uuids))
        let services = [IrisBleConstants.serviceUUID]
        delegate?.centralSeam(
            self,
            didCompleteServiceDiscovery: identifier,
            services: services,
            error: serviceDiscoveryError
        )
    }

    public func discoverCharacteristics(identifier: UUID, characteristicUuids: [CBUUID]?, serviceUuid: CBUUID) {
        lock.lock(); defer { lock.unlock() }
        discoverCharacteristicsRecords.append((identifier, characteristicUuids, serviceUuid))
        let chars = [IrisBleConstants.identifyUUID, IrisBleConstants.controlUUID]
        delegate?.centralSeam(
            self,
            didCompleteCharacteristicDiscovery: identifier,
            serviceUuid: serviceUuid,
            characteristics: chars,
            error: nil
        )
    }

    public func readValue(identifier: UUID, characteristicUuid: CBUUID) {
        lock.lock(); defer { lock.unlock() }
        readValueRecords.append((identifier, characteristicUuid))
        let data = peers[identifier]?.identifyValue ?? Data()
        let delay = readDelay
        DispatchQueue.global().asyncAfter(deadline: .now() + delay) { [weak self] in
            guard let self else { return }
            self.delegate?.centralSeam(
                self,
                didRead: identifier,
                characteristicUuid: characteristicUuid,
                data: data,
                error: nil
            )
        }
    }

    public func writeValue(identifier: UUID, characteristicUuid: CBUUID, data: Data, withResponse: Bool) {
        lock.lock()
        writeValueRecords.append((identifier, characteristicUuid, data, withResponse))
        let del = delegate
        lock.unlock()
        if withResponse {
            // Synchronous delivery mirrors connectPeripheral: gattWrite blocks on
            // a semaphore waiting for this ACK, so the mock must reply on the
            // same thread rather than deferring to a queue.
            del?.centralSeam(self, didWriteValue: identifier, characteristicUuid: characteristicUuid, error: nil)
        }
    }

    public func maximumWriteValueLength(identifier: UUID) -> Int {
        lock.lock(); defer { lock.unlock() }
        return maximumWriteValueLengthValue
    }

    // MARK: - Peripheral side (BlePeripheralSeam)

    public func addService(serviceUuid: CBUUID, characteristics: [IOSBleCharacteristicSpec]) {
        lock.lock(); defer { lock.unlock() }
        addServiceRecords.append(serviceUuid)
    }

    public func startAdvertising(serviceUuid: CBUUID, localName: String?) {
        lock.lock(); defer { lock.unlock() }
        startAdvertisingRecords.append((serviceUuid, localName))
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.peripheralDelegate?.peripheralSeam(self, didStartAdvertising: nil)
        }
    }

    public func stopAdvertising() {
        lock.lock(); defer { lock.unlock() }
        stopAdvertisingCount += 1
    }

    public func restoredServiceUuids() -> [CBUUID] {
        []
    }

    // MARK: - Test drivers

    /// Simulate a central scan callback (didDiscover + rssi).
    public func fireDiscover(peer: MockPeer) {
        delegate?.centralSeam(self, didDiscover: peer.identifier, rssi: peer.rssi)
    }

    /// Simulate an inbound GATT write (peripheralManager didReceiveWrite).
    public func fireInboundWrite(centralIdentifier: UUID, data: Data, charUuid: CBUUID = IrisBleConstants.controlUUID) {
        peripheralDelegate?.peripheralSeam(
            self,
            didReceiveWrite: centralIdentifier,
            characteristicUuid: charUuid,
            data: data
        )
    }
}

/// Await helper: pumps the given closure's expectation while the async mock
/// delegate events land on the main queue. Each call grants the main queue a
/// beat so the adapter's synchronous FFI waits resolve.
public func waitForMainQueueBeats(_ count: Int = 5) {
    let exp = XCTestExpectation(description: "main-queue-beats")
    let q = DispatchQueue.main
    for _ in 0..<count {
        q.async { }
    }
    q.async { exp.fulfill() }
    _ = XCTWaiter.wait(for: [exp], timeout: 5)
}