// IOS-001 AC-4..AC-9 — IosBleAdapterTests (macOS-host, IOS-CoreBluetooth-Mock).
//
// Drives `IosBleAdapter` through `MockBleSeam` (no CoreBluetooth on the
// Simulator — RES-0025 RQ-3 finding 6). The adapter's FFI ops are SYNCHRONOUS
// (they block on an internal semaphore during gatt_read), so tests run the
// blocking call on a background thread and wait on an XCTest expectation.
import Foundation
import CoreBluetooth
import XCTest

#if canImport(IRIS)
import IRIS
#endif

final class IosBleAdapterTests: XCTestCase {

    private var mock: MockBleSeam!
    private var adapter: IosBleAdapter!

    override func setUp() {
        super.setUp()
        mock = MockBleSeam()
        mock.state = .poweredOn
        adapter = IosBleAdapter(
            central: mock,
            peripheral: mock,
            gattReadTimeout: 1.0
        )
    }

    // MARK: - Helpers

    /// Seed one discovered peer, then `connect_gatt` it (auto-connected by the
    /// mock). Returns (handle, identifier).
    private func connectPeer(uuid: UUID, beacon: Data) throws -> (UInt64, UUID) {
        let peer = MockPeer(identifier: uuid, identifyValue: beacon)
        mock.peers[uuid] = peer
        mock.fireDiscover(peer: peer)
        let token = IrisBleConstants.token(for: uuid)
        let handle = try adapter.connectGatt(address: token)
        return (handle, uuid)
    }

    /// Run a blocking adapter op on a background thread.
    private func expect<T>(timeout: TimeInterval = 5, _ body: @escaping () throws -> T) throws -> T {
        var result: Result<T, Error>!
        let exp = expectation(description: "background-\(T.self)")
        DispatchQueue.global().async {
            result = Result { try body() }
            exp.fulfill()
        }
        wait(for: [exp], timeout: timeout)
        return try result.get()
    }

    // MARK: - AC-4: 11-op surface + no-service-data

    func testStartScanRegistersIrisServiceOnly() throws {
        let filter = FfiScanFilter(serviceUuid: IrisBleConstants.serviceUUIDHex, address: "", rssiFloor: -95)
        let handle = try adapter.startScan(filter: filter)
        XCTAssertEqual(handle, 1)
        XCTAssertEqual(mock.startScanRecords.count, 1)
        XCTAssertEqual(mock.startScanRecords[0].0, [IrisBleConstants.serviceUUID])
        // allowDuplicates=false for background-scan hygiene.
        XCTAssertEqual(mock.startScanRecords[0].1[CBCentralManagerScanOptionAllowDuplicatesKey] as? Bool, false)

        adapter.stopScan(handle: handle)
        XCTAssertEqual(mock.stopScanCount, 1)
    }

    func testStartScanRejectsUuidlessFilter() {
        // AC-9: UUID-less scan config rejected at the boundary (no seam call).
        let filter = FfiScanFilter(serviceUuid: "", address: "", rssiFloor: -95)
        XCTAssertThrowsError(try adapter.startScan(filter: filter)) { error in
            guard case IrisFfiError.invalidArgument(_) = error else {
                return XCTFail("expected invalidArgument, got \(error)")
            }
        }
        XCTAssertEqual(mock.startScanRecords.count, 0)
    }

    func testStartAdvertisingProgramsIdentifyServiceOnly() throws {
        let adv = FfiAdvertisementData(payload: Data([0x00, 0x01]), nonConnectable: true)
        let handle = try adapter.startAdvertising(data: adv)
        XCTAssertNotEqual(handle, 0)
        // GATT server: identify service installed.
        XCTAssertEqual(mock.addServiceRecords, [IrisBleConstants.serviceUUID])
        // AC-9: local name is the bounded "IRIS" (<=10 B) — no service data key.
        XCTAssertEqual(mock.startAdvertisingRecords.count, 1)
        XCTAssertEqual(mock.startAdvertisingRecords[0].0, IrisBleConstants.serviceUUID)
        XCTAssertEqual(mock.startAdvertisingRecords[0].1, IrisBleConstants.localName)
        XCTAssertTrue(IrisBleConstants.localName.utf8.count <= IrisBleConstants.localNameMaxBytes)

        adapter.stopAdvertising(handle: handle)
        XCTAssertEqual(mock.stopAdvertisingCount, 1)
    }

    // MARK: - AC-5: connect-to-identify gatt_read

    func testGattReadReturnsIdentifyBeacon() throws {
        let beacon = Data([0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07])
        let uuid = UUID()
        let (handle, peerId) = try connectPeer(uuid: uuid, beacon: beacon)

        let data = try expect(timeout: 5) {
            try self.adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)
        }
        XCTAssertEqual(data, beacon)
        // The read target was the identify characteristic (DEC-BLE-002-0015).
        XCTAssertEqual(mock.readValueRecords.last?.1, IrisBleConstants.identifyUUID)
        XCTAssertEqual(mock.readValueRecords.last?.0, peerId)
    }

    func testGattReadRejectsNonIdentifyCharacteristic() throws {
        let (handle, _) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))
        XCTAssertThrowsError(try expect { try self.adapter.gattRead(handle: handle, charUuid: "99999999999999999999999999999999") })
    }

    // MARK: - AC-6: BLE-RT-C003 read timeout

    func testGattReadTimesOutAndCancels() throws {
        // A fresh adapter with a short timeout; the mock delays the read response.
        mock.readDelay = 3.0
        adapter = IosBleAdapter(central: mock, peripheral: mock, gattReadTimeout: 0.3)

        let (handle, peerId) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))

        let before = mock.cancelConnectionRecords.count
        let outcome = expect(timeout: 5) {
            try self.adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)
        }
        guard case .failure(let error) = outcome else {
            return XCTFail("expected timeout error")
        }
        guard case IrisFfiError.timeout = error else {
            return XCTFail("expected .timeout, got \(error)")
        }
        // No hang + connection cancelled (typed FFI error, DEC-IOS-0007).
        XCTAssertEqual(mock.cancelConnectionRecords.count, before + 1)
        XCTAssertEqual(mock.cancelConnectionRecords.last, peerId)
    }

    func testGattReadTimeoutGatesAdmissionWindow() throws {
        // IOS-RT-103: a connect-to-identify TIMEOUT is an identify failure —
        // the peer must enter the rejection window so the next scan cycle does
        // not re-probe a peer that already stalled once.
        mock.readDelay = 3.0
        adapter = IosBleAdapter(central: mock, peripheral: mock, gattReadTimeout: 0.3)

        let uuid = UUID()
        let (handle, _) = try connectPeer(uuid: uuid, beacon: Data([0x01]))
        let outcome = expect(timeout: 5) {
            try self.adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)
        }
        guard case .failure(IrisFfiError.timeout) = outcome else {
            return XCTFail("expected timeout error")
        }
        // Re-probe within the rejection window is denied (AC-7 budget gate).
        XCTAssertThrowsError(try adapter.connectGatt(address: IrisBleConstants.token(for: uuid))) { error in
            guard case IrisFfiError.deviceNotFound = error else {
                return XCTFail("expected deviceNotFound for timed-out peer, got \(error)")
            }
        }
    }

    func testConcurrentGattReadRejected() throws {
        // IOS-RT-104: one outstanding connect-to-identify read per token; a
        // second gatt_read for the same peer is rejected up front rather than
        // stranding the first caller until its hard timeout.
        mock.readDelay = 3.0
        adapter = IosBleAdapter(central: mock, peripheral: mock, gattReadTimeout: 1.0)

        let (handle, _) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))
        let first = expectation(description: "first-read-still-parked")
        DispatchQueue.global().async {
            _ = try? self.adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)
            first.fulfill()
        }
        // Bug #70: give the first read time to install its PendingIdentifyRead entry
        // before the second call. 200ms was too short on loaded CI; 500ms is more
        // robust while still well within the 1s gattReadTimeout.
        Thread.sleep(forTimeInterval: 0.5)
        XCTAssertThrowsError(try adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)) { error in
            guard case IrisFfiError.invalidArgument(_) = error else {
                return XCTFail("expected invalidArgument for concurrent read, got \(error)")
            }
        }
        wait(for: [first], timeout: 5)
    }

    // MARK: - AC-7: probe-admission budget

    func testProbeBudgetCapsProbeConnects() throws {
        adapter = IosBleAdapter(central: mock, peripheral: mock, maxProbesPerScan: 8)

        // 10 discovered peers -> only 8 new probe connects admitted per scan.
        var ids: [UUID] = []
        for i in 0..<10 {
            let peer = MockPeer(identifier: UUID(), identifyValue: Data([UInt8(i)]))
            mock.peers[peer.identifier] = peer
            mock.fireDiscover(peer: peer)
            ids.append(peer.identifier)
        }
        var admitted = 0
        for id in ids {
            if (try? adapter.connectGatt(address: IrisBleConstants.token(for: id))) != nil {
                admitted += 1
            }
        }
        XCTAssertEqual(admitted, 8)
        XCTAssertEqual(mock.connectRecords.count, 8)
    }

    func testProbeRejectionWindowBlocksReprobe() throws {
        // A peer rejected by a failed read stays out of the admission window.
        let uuid = UUID()
        let peer = MockPeer(identifier: uuid)
        peer.connected = false
        mock.peers[uuid] = peer
        mock.fireDiscover(peer: peer)
        let token = IrisBleConstants.token(for: uuid)
        let handle = try adapter.connectGatt(address: token)

        // Fail the identify read through a service-discovery error.
        mock.serviceDiscoveryError = NSError(domain: "test", code: 1)
        _ = expect(timeout: 5) {
            try self.adapter.gattRead(handle: handle, charUuid: IrisBleConstants.identifyCharacteristicHex)
        }

        // Re-probe within the rejection window is denied.
        XCTAssertThrowsError(try adapter.connectGatt(address: token)) { error in
            guard case IrisFfiError.deviceNotFound = error else {
                return XCTFail("expected deviceNotFound for rejected peer, got \(error)")
            }
        }
    }

    // MARK: - AC-8: MTU negotiate-late

    func testSetMtuReturnsNegotiatedPayload20Guard() throws {
        let (handle, _) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))

        mock.maximumWriteValueLengthValue = 185
        XCTAssertEqual(try adapter.setMtu(handle: handle, mtu: 512), 185)

        mock.maximumWriteValueLengthValue = 20
        XCTAssertEqual(try adapter.setMtu(handle: handle, mtu: 512), 20)

        mock.maximumWriteValueLengthValue = 512
        XCTAssertEqual(try adapter.setMtu(handle: handle, mtu: 900), 512)

        mock.maximumWriteValueLengthValue = 1000
        XCTAssertEqual(try adapter.setMtu(handle: handle, mtu: 900), 512)
    }

    // MARK: - drains (scan_results / incoming_gatt_writes)

    func testScanResultsDrainIsEmptyPayload() {
        let peer = MockPeer(identifier: UUID(), rssi: -70)
        mock.peers[peer.identifier] = peer
        mock.fireDiscover(peer: peer)

        let results = adapter.scanResults()
        XCTAssertEqual(results.count, 1)
        XCTAssertEqual(results[0].rssi, -70)
        // RES-0024 DI-1: no IRIS service data on iOS — payload stays empty.
        XCTAssertTrue(results[0].payload.isEmpty)
        XCTAssertEqual(results[0].address, IrisBleConstants.token(for: peer.identifier))
        // Second drain is idempotent (guard-freed).
        XCTAssertEqual(adapter.scanResults().count, 0)
    }

    func testIncomingGattWritesDrainAttributesHandle() throws {
        let (handle, peerId) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))
        mock.fireInboundWrite(centralIdentifier: peerId, data: Data([0xAA, 0xBB]))

        let events = adapter.incomingGattWrites()
        XCTAssertEqual(events.count, 1)
        XCTAssertEqual(events[0].handle, handle)
        XCTAssertEqual(events[0].charUuid, IrisBleConstants.controlCharacteristicHex)
        XCTAssertEqual(events[0].data, Data([0xAA, 0xBB]))
        XCTAssertEqual(adapter.incomingGattWrites().count, 0)
    }

    // MARK: - gatt_write mapping

    func testGattWriteWithResponse() throws {
        let (handle, peerId) = try connectPeer(uuid: UUID(), beacon: Data([0x01]))
        try adapter.gattWrite(handle: handle, charUuid: IrisBleConstants.controlCharacteristicHex, data: Data([0x01, 0x02]))

        XCTAssertEqual(mock.writeValueRecords.count, 1)
        XCTAssertEqual(mock.writeValueRecords[0].0, peerId)
        XCTAssertEqual(mock.writeValueRecords[0].2, Data([0x01, 0x02]))
        XCTAssertEqual(mock.writeValueRecords[0].3, true) // .withResponse
    }
}