// IOS-001 AC-7 — ProbeAdmissionTests (BLE-RT-C004 carry).
//
// Probe-connect admission mirroring `c001_ios_leg_probe_budget_caps_probe_connects`
// (BLE-002): the Swift adapter must cap probe connects at the configured budget
// per scan window and never re-probe a rejected peer within the window.
import Foundation
import XCTest

#if canImport(IRIS)
import IRIS
#endif

final class ProbeAdmissionTests: XCTestCase {

    private var mock: MockBleSeam!
    private var adapter: IosBleAdapter!

    override func setUp() {
        super.setUp()
        mock = MockBleSeam()
        mock.state = .poweredOn
        adapter = IosBleAdapter(
            central: mock,
            peripheral: mock,
            gattReadTimeout: 1.0,
            maxProbesPerScan: 8,
            probeRejectionWindow: 60.0
        )
    }

    private func seedPeers(count: Int) -> [UUID] {
        var ids: [UUID] = []
        for _ in 0..<count {
            let peer = MockPeer(identifier: UUID())
            mock.peers[peer.identifier] = peer
            mock.fireDiscover(peer: peer)
            ids.append(peer.identifier)
        }
        return ids
    }

    func testBudgetResetsOnNewScanWindow() throws {
        let ids = seedPeers(count: 12)

        // First scan window: 8 admitted, 4 denied.
        var admitted = 0
        for id in ids {
            if (try? adapter.connectGatt(address: IrisBleConstants.token(for: id))) != nil {
                admitted += 1
            }
        }
        XCTAssertEqual(admitted, 8)

        // A new start_scan opens a fresh admission window (per-scan budget).
        _ = try adapter.startScan(filter: FfiScanFilter(serviceUuid: IrisBleConstants.serviceUUIDHex, address: "", rssiFloor: -95))
        var admittedAfterReset = 0
        for id in ids {
            if (try? adapter.connectGatt(address: IrisBleConstants.token(for: id))) != nil {
                admittedAfterReset += 1
            }
        }
        XCTAssertEqual(admittedAfterReset, 8)
    }

    func testReAdmittedPeerWithinWindowIsNotRejected() throws {
        let id = seedPeers(count: 1)[0]
        let token = IrisBleConstants.token(for: id)

        _ = try? adapter.connectGatt(address: token)
        // Same peer re-connect within the window is admitted (idempotent).
        _ = try? adapter.connectGatt(address: token)
        XCTAssertEqual(mock.connectRecords.count, 2)
    }
}