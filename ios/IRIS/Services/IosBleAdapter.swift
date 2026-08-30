// IOS-001 AC-4..AC-9 — IosBleAdapter: the Swift/CoreBluetooth implementation of
// the UniFFI foreign trait `FfiBleAdapter` (crates/iris-ios ffi/ble_adapter.rs).
//
// Maps the 11-op contract onto the `BleCentralSeam`/`BlePeripheralSeam` (real
// CBCentralManager/CBPeripheralManager wrappers; mocked in IOS-CoreBluetooth-Mock
// unit tests, AC-16). Carry-forwards:
//   - DEC-BLE-002-0015  `gatt_read` connect-to-identify (AC-5)
//   - BLE-RT-C003       hard read timeout + cancel (AC-6)
//   - BLE-RT-C004       probe-admission cache / 8-probe-per-scan budget (AC-7)
//   - DEC-BLE-002-0004  set_mtu -> maximumWriteValueLength negotiate-late,
//                       cap 512, 20-B degraded guard (AC-8)
//   - DEC-BLE-002-0002  AC-9: service-data NEVER transmitted; the beacon is
//                       served from IRIS_IDENTIFY_CHARACTERISTIC (GATT read).
//
// D-2/§8: `final class … : FfiBleAdapter, @unchecked Sendable`. Mutable state
// is confined behind `lock` (the FFI seam is synchronous `throws`; CoreBluetooth
// callbacks arrive on the seam queue). Evaluated from the tokio worker when the
// Rust core polls — never from the main queue.
import Foundation
import CoreBluetooth

public final class IosBleAdapter: FfiBleAdapter, @unchecked Sendable {

    // MARK: - Dependencies / config

    private let central: BleCentralSeam
    private let peripheral: BlePeripheralSeam
    /// BLE-RT-C003: hard call-timeout for `gattRead` (default 10 s, configurable).
    private let gattReadTimeout: TimeInterval
    /// AC-7: maximum probe connects per start_scan admission window.
    private let maxProbesPerScan: Int
    /// AC-7: rejection window for a probed peer that fails connect-to-identify.
    private let probeRejectionWindow: TimeInterval

    public init(
        central: BleCentralSeam,
        peripheral: BlePeripheralSeam,
        gattReadTimeout: TimeInterval = 10.0,
        maxProbesPerScan: Int = 8,
        probeRejectionWindow: TimeInterval = 60.0
    ) {
        self.central = central
        self.peripheral = peripheral
        self.gattReadTimeout = gattReadTimeout
        self.maxProbesPerScan = maxProbesPerScan
        self.probeRejectionWindow = probeRejectionWindow
        super.init()
        central.delegate = self
        peripheral.delegate = self
    }

    // MARK: - State (lock-confined)

    private let lock = NSLock()
    private var state: IOSBleCentralState = .unknown

    private var nextScanHandle: UInt64 = 1
    private var nextAdvHandle: UInt64 = 1
    private var nextGattHandle: UInt64 = 1

    /// token (12-hex) -> peer record. Peer identity = CBPeripheral.identifier.
    private var peers: [String: PeerRecord] = [:]
    /// gatt handle -> peer token (inverse map for handle-keyed ops).
    private var tokenByHandle: [UInt64: String] = [:]
    /// peer token -> gatt handle (outbound-event attribution for inbound writes).
    private var handleByToken: [String: UInt64] = [:]
    /// The last accepted scan filter (re-applied from willRestoreState, AC-10).
    private var activeScanFilter: FfiScanFilter?

    private var scanResultBuffer: [FfiScanResult] = []
    private var gattWriteBuffer: [FfiGattWriteEvent] = []

    /// AC-7 admission state.
    private var admittedThisScan: [String: Date] = [:]
    private var rejectedUntil: [String: Date] = [:]

    /// BLE-RT-C003 outstanding connect-to-identify reads (keyed by token).
    private var pendingReads: [String: PendingIdentifyRead] = [:]
    /// Bug #7+#8: outstanding gattWrite completions (keyed by token, one per peer).
    private var pendingWrites: [String: PendingWrite] = [:]

    // MARK: - Per-peer record

    private struct PeerRecord {
        let identifier: UUID
        var connected: Bool = false
        var servicesDiscovered: Bool = false
        var characteristicsDiscovered: Bool = false
        var services: [CBUUID] = []
        var characteristics: [CBUUID] = []
    }

    /// Stage machine for one `gatt_read` (connect-to-identify).
    private enum ReadStage {
        case pendingConnect
        case discoveringServices
        case discoveringCharacteristics
        case reading
    }

    private final class PendingIdentifyRead {
        let semaphore = DispatchSemaphore(value: 0)
        var stage: ReadStage = .pendingConnect
        var result: Result<Data, IrisFfiError>?
        let deadline: DispatchTime
        init(timeout: TimeInterval) {
            deadline = DispatchTime.now() + timeout
        }
    }

    private final class PendingWrite {
        let semaphore = DispatchSemaphore(value: 0)
        var result: Result<Void, IrisFfiError>?
        let deadline: DispatchTime
        init(timeout: TimeInterval) {
            deadline = DispatchTime.now() + timeout
        }
    }

    // MARK: - FfiBleAdapter (11-op — AC-4 op-coverage)

    public func startScan(filter: FfiScanFilter) throws -> UInt64 {
        // AC-9: IRIS service UUID is MANDATORY (background scanning requires a
        // service-UUID filter; UUID-less config rejected at the boundary).
        guard !filter.serviceUuid.isEmpty else {
            throw IrisFfiError.invalidArgument("IRIS service UUID mandatory (AC-9)")
        }
        guard IrisBleConstants.cbuuid(hex32: filter.serviceUuid) != nil else {
            throw IrisFfiError.invalidArgument("malformed service UUID (AC-9)")
        }
        let handle = lock.withLock { () -> UInt64 in
            let h = nextScanHandle
            nextScanHandle += 1
            activeScanFilter = filter
            admittedThisScan = [:]
            return h
        }
        central.startScanning(
            serviceUuids: [IrisBleConstants.serviceUUID],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
        return handle
    }

    public func stopScan(handle: UInt64) {
        central.stopScanning()
    }

    public func startAdvertising(data: FfiAdvertisementData) throws -> UInt64 {
        // AC-9 / DEC-BLE-002-0002: the beacon payload programs the GATT identify
        // characteristic; the advertisement itself carries service UUID + local
        // name ONLY (never service data — CoreBluetooth has no such API, and we
        // never fake it).
        let handle = lock.withLock { () -> UInt64 in
            let h = nextAdvHandle
            nextAdvHandle += 1
            return h
        }
        peripheral.addService(
            serviceUuid: IrisBleConstants.serviceUUID,
            characteristics: [
                IOSBleCharacteristicSpec(
                    uuid: IrisBleConstants.identifyUUID,
                    value: data.payload,
                    supportsRead: true,
                    supportsWrite: false
                ),
                IOSBleCharacteristicSpec(
                    uuid: IrisBleConstants.controlUUID,
                    value: Data(),
                    supportsRead: false,
                    supportsWrite: true
                ),
            ]
        )
        peripheral.startAdvertising(
            serviceUuid: IrisBleConstants.serviceUUID,
            localName: IrisBleConstants.localName
        )
        return handle
    }

    public func stopAdvertising(handle: UInt64) {
        peripheral.stopAdvertising()
    }

    public func connectGatt(address: String) throws -> UInt64 {
        let cleaned = address.filter { $0 != ":" && $0 != "-" }.uppercased()
        guard let peer = lock.withLock({ peers[cleaned] }) else {
            throw IrisFfiError.deviceNotFound
        }
        // AC-7 probe-admission: never re-probe a peer rejected within the window.
        if lock.withLock({ rejectedUntil[cleaned] }) ?? .distantPast > Date() {
            throw IrisFfiError.deviceNotFound
        }
        // AC-7 budget: at most `maxProbesPerScan` new probe connects per scan
        // admission window (c001_ios_leg_probe_budget_caps_probe_connects).
        let admitted: Bool = lock.withLock {
            if admittedThisScan[cleaned] != nil {
                return true
            }
            guard admittedThisScan.count < maxProbesPerScan else {
                return false
            }
            admittedThisScan[cleaned] = Date()
            return true
        }
        guard admitted else {
            throw IrisFfiError.gattFailure("probe admission budget exhausted (AC-7)")
        }
        let handle = lock.withLock { () -> UInt64 in
            let h = nextGattHandle
            nextGattHandle += 1
            tokenByHandle[h] = cleaned
            handleByToken[cleaned] = h
            return h
        }
        central.connectPeripheral(identifier: peer.identifier, options: nil)
        return handle
    }

    public func disconnectGatt(handle: UInt64) {
        let token = lock.withLock { tokenByHandle[handle] }
        guard let token, let peer = lock.withLock({ peers[token] }) else { return }
        central.cancelConnection(identifier: peer.identifier)
    }

    public func gattWrite(handle: UInt64, charUuid: String, data: Data) throws {
        guard let token = lock.withLock({ tokenByHandle[handle] }),
              let peer = lock.withLock({ peers[token] }),
              let uuid = IrisBleConstants.cbuuid(hex32: charUuid)
        else {
            throw IrisFfiError.deviceNotFound
        }
        guard peer.connected else {
            throw IrisFfiError.gattFailure("peer not connected")
        }
        let pending = PendingWrite(timeout: gattReadTimeout)
        let installed: Bool = lock.withLock {
            guard pendingWrites[token] == nil else { return false }
            pendingWrites[token] = pending
            return true
        }
        guard installed else {
            throw IrisFfiError.gattFailure("gatt_write already in flight for this peer")
        }
        central.writeValue(
            identifier: peer.identifier,
            characteristicUuid: uuid,
            data: data,
            withResponse: true
        )
        let outcome = pending.semaphore.wait(timeout: pending.deadline)
        lock.withLock { pendingWrites[token] = nil }
        switch outcome {
        case .timedOut:
            throw IrisFfiError.timeout
        case .success:
            guard let result = pending.result else { throw IrisFfiError.timeout }
            return try result.get()
        }
    }

    public func gattRead(handle: UInt64, charUuid: String) throws -> Data {
        // DEC-BLE-002-0015 connect-to-identify: only IRIS_IDENTIFY_CHARACTERISTIC
        // is readable on this surface.
        guard let token = lock.withLock({ tokenByHandle[handle] }),
              let peer = lock.withLock({ peers[token] })
        else {
            throw IrisFfiError.deviceNotFound
        }
        guard IrisBleConstants.cbuuid(hex32: charUuid) == IrisBleConstants.identifyUUID else {
            throw IrisFfiError.invalidArgument("non-identify characteristic (DEC-BLE-002-0015)")
        }

        let pending = PendingIdentifyRead(timeout: gattReadTimeout)
        // One outstanding read per token: a concurrent gatt_read for the same
        // peer would otherwise clobber the parked PendingIdentifyRead and
        // strand the first caller until its own hard timeout (SECURITY_REVIEW
        // IOS-RT-104).
        let installed: Bool = lock.withLock {
            guard pendingReads[token] == nil else { return false }
            pendingReads[token] = pending
            return true
        }
        guard installed else {
            throw IrisFfiError.invalidArgument("gatt_read already in flight for this peer")
        }

        // Drive connect -> discover -> read -> didUpdateValueFor, advancing the
        // stage machine from the delegate callbacks.
        if !peer.connected {
            central.connectPeripheral(identifier: peer.identifier, options: nil)
            advance(token: token, to: .pendingConnect)
        } else {
            central.discoverServices(identifier: peer.identifier, uuids: [IrisBleConstants.serviceUUID])
            advance(token: token, to: .discoveringServices)
        }

        // BLE-RT-C003: hard wait; on timeout -> typed error + connection cancel
        // (no hang, no leaked task — DEC-IOS-0007).
        let outcome = pending.semaphore.wait(timeout: pending.deadline)
        lock.withLock { pendingReads[token] = nil }

        switch outcome {
        case .timedOut:
            // BLE-RT-C003 + AC-7: a connect-to-identify timeout IS an identify
            // failure — gate the admission window so the next scan cycle does
            // not re-probe a peer that already stalled once (IOS-RT-103).
            lock.withLock {
                rejectedUntil[token] = Date().addingTimeInterval(probeRejectionWindow)
            }
            central.cancelConnection(identifier: peer.identifier)
            throw IrisFfiError.timeout
        case .success:
            guard let result = pending.result else { throw IrisFfiError.timeout }
            return try result.get()
        }
    }

    public func setMtu(handle: UInt64, mtu: UInt16) throws -> UInt16 {
        guard let token = lock.withLock({ tokenByHandle[handle] }),
              let peer = lock.withLock({ peers[token] })
        else {
            throw IrisFfiError.deviceNotFound
        }
        // DEC-BLE-002-0004 / AC-8: no-op request; return the negotiated ATT
        // payload (`maximumWriteValueLength(for: .withResponse)`, cap 512).
        let negotiated = central.maximumWriteValueLength(identifier: peer.identifier)
        // 20-B degraded guard (RES-0024 DI-5 / iOS 16.x regression): a broken
        // negotiation must never report more than the practical floor.
        let guarded = negotiated >= 23 ? negotiated : 20
        return UInt16(min(guarded, 512))
    }

    public func incomingGattWrites() -> [FfiGattWriteEvent] {
        lock.withLock {
            let batch = gattWriteBuffer
            gattWriteBuffer.removeAll(keepingCapacity: true)
            return batch
        }
    }

    public func scanResults() -> [FfiScanResult] {
        lock.withLock {
            let batch = scanResultBuffer
            scanResultBuffer.removeAll(keepingCapacity: true)
            return batch
        }
    }
}

// MARK: - BleCentralSeamDelegate (events from the real/mock central manager)

extension IosBleAdapter: BleCentralSeamDelegate {
    public func centralSeamDidUpdateState(_ seam: BleCentralSeam, state: IOSBleCentralState) {
        lock.withLock { self.state = state }
        if state == .poweredOff {
            lock.withLock { scanResultBuffer.removeAll(keepingCapacity: true) }
        }
    }

    public func centralSeam(_ seam: BleCentralSeam, didDiscover identifier: UUID, rssi: Int) {
        let token = IrisBleConstants.token(for: identifier)
        lock.withLock {
            if peers[token] == nil {
                peers[token] = PeerRecord(identifier: identifier)
            }
            // RES-0024 DI-1: advertisementData carries NO IRIS service data on
            // iOS — payload stays empty; RSSI + identity only.
            scanResultBuffer.append(
                FfiScanResult(address: token, payload: Data(), rssi: Int32(rssi))
            )
        }
    }

    public func centralSeam(_ seam: BleCentralSeam, didConnect identifier: UUID) {
        let token = IrisBleConstants.token(for: identifier)
        lock.withLock {
            peers[token]?.connected = true
        }
        // A connect-to-identify read may be parked waiting for this.
        if let pending = lock.withLock({ pendingReads[token] }),
           pending.stage == .pendingConnect {
            if let p = lock.withLock({ peers[token] }) {
                central.discoverServices(identifier: p.identifier, uuids: [IrisBleConstants.serviceUUID])
            }
            advance(token: token, to: .discoveringServices)
        }
    }

    public func centralSeam(_ seam: BleCentralSeam, didFailToConnect identifier: UUID, error: Error?) {
        failRead(token: IrisBleConstants.token(for: identifier), with: .gattFailure(error?.localizedDescription ?? "connect failed"))
    }

    public func centralSeam(_ seam: BleCentralSeam, didDisconnect identifier: UUID, error: Error?) {
        let token = IrisBleConstants.token(for: identifier)
        lock.withLock {
            peers[token]?.connected = false
            peers[token]?.servicesDiscovered = false
            peers[token]?.characteristicsDiscovered = false
        }
        failRead(token: token, with: .gattFailure(error?.localizedDescription ?? "disconnected"))
    }

    public func centralSeam(_ seam: BleCentralSeam, didCompleteServiceDiscovery identifier: UUID, services: [CBUUID], error: Error?) {
        let token = IrisBleConstants.token(for: identifier)
        if let error {
            failRead(token: token, with: .gattFailure(error.localizedDescription))
            return
        }
        lock.withLock {
            peers[token]?.services = services
            peers[token]?.servicesDiscovered = true
        }
        guard let pending = lock.withLock({ pendingReads[token] }),
              pending.stage == .discoveringServices,
              let p = lock.withLock({ peers[token] })
        else { return }
        central.discoverCharacteristics(
            identifier: p.identifier,
            characteristicUuids: [IrisBleConstants.identifyUUID, IrisBleConstants.controlUUID],
            serviceUuid: IrisBleConstants.serviceUUID
        )
        advance(token: token, to: .discoveringCharacteristics)
    }

    public func centralSeam(_ seam: BleCentralSeam, didCompleteCharacteristicDiscovery identifier: UUID, serviceUuid: CBUUID, characteristics: [CBUUID], error: Error?) {
        let token = IrisBleConstants.token(for: identifier)
        if let error {
            failRead(token: token, with: .gattFailure(error.localizedDescription))
            return
        }
        lock.withLock {
            peers[token]?.characteristics = characteristics
            peers[token]?.characteristicsDiscovered = true
        }
        guard let pending = lock.withLock({ pendingReads[token] }),
              pending.stage == .discoveringCharacteristics,
              let p = lock.withLock({ peers[token] })
        else { return }
        central.readValue(identifier: p.identifier, characteristicUuid: IrisBleConstants.identifyUUID)
        advance(token: token, to: .reading)
    }

    public func centralSeam(_ seam: BleCentralSeam, didRead identifier: UUID, characteristicUuid: CBUUID, data: Data?, error: Error?) {
        let token = IrisBleConstants.token(for: identifier)
        if let error {
            failRead(token: token, with: .gattFailure(error.localizedDescription))
            return
        }
        guard let pending = lock.withLock({ pendingReads[token] }),
              pending.stage == .reading,
              let data
        else { return }
        pending.result = .success(data)
        pending.semaphore.signal()
    }

    public func centralSeam(_ seam: BleCentralSeam, didWriteValue identifier: UUID, characteristicUuid: CBUUID, error: Error?) {
        let token = IrisBleConstants.token(for: identifier)
        guard let pending = lock.withLock({ pendingWrites[token] }) else { return }
        if let error {
            pending.result = .failure(.gattFailure(error.localizedDescription))
        } else {
            pending.result = .success(())
        }
        pending.semaphore.signal()
    }

    public func centralSeam(_ seam: BleCentralSeam, didRestore identifiers: [UUID]) {
        // AC-10 willRestoreState re-arm: re-discover restored peers and restart
        // the last accepted scan filter (no auto-resume of the adapter).
        let filter = lock.withLock { activeScanFilter }
        guard let filter else { return }
        for id in identifiers {
            let token = IrisBleConstants.token(for: id)
            lock.withLock {
                if peers[token] == nil { peers[token] = PeerRecord(identifier: id) }
            }
        }
        try? startScan(filter: filter)
    }
}

// MARK: - BlePeripheralSeamDelegate (GATT server inbound frames)

extension IosBleAdapter: BlePeripheralSeamDelegate {
    public func peripheralSeamDidUpdateState(_ seam: BlePeripheralSeam, state: IOSBleCentralState) {
        // The seam keeps a pending advertisement; poweredOn pull-through is its job.
    }

    public func peripheralSeam(_ seam: BlePeripheralSeam, didStartAdvertising error: Error?) {
        // Best-effort: the CoreBluetooth startAdvertising error is informational;
        // the GATT service is already installed (AC-9 honesty: the beacon lives on
        // the characteristic, not the wire).
    }

    public func peripheralSeam(_ seam: BlePeripheralSeam, didReceiveWrite centralIdentifier: UUID, characteristicUuid: CBUUID, data: Data) {
        let token = IrisBleConstants.token(for: centralIdentifier)
        lock.withLock {
            gattWriteBuffer.append(
                FfiGattWriteEvent(
                    handle: handleByToken[token] ?? 0,
                    charUuid: IrisBleConstants.toHex32(characteristicUuid),
                    data: data
                )
            )
        }
    }

    public func peripheralSeam(_ seam: BlePeripheralSeam, didRestoreServiceUuids uuids: [CBUUID]) {
        // Peripheral restore: the seam re-adds services; nothing to drive here.
    }
}

// MARK: - Internals

extension IosBleAdapter {
    private func advance(token: String, to stage: ReadStage) {
        lock.withLock { pendingReads[token]?.stage = stage }
    }

    private func failRead(token: String, with error: IrisFfiError) {
        guard let pending = lock.withLock({ pendingReads[token] }) else { return }
        // A first probe failure also gates the admission window (AC-7): don't
        // re-probe a peer that recently failed connect-to-identify.
        lock.withLock {
            rejectedUntil[token] = Date().addingTimeInterval(probeRejectionWindow)
        }
        pending.result = .failure(error)
        pending.semaphore.signal()
    }
}

extension NSLock {
    func withLock<T>(_ body: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try body()
    }
}