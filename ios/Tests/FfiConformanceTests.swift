// IOS-001 AC-3 — FfiConformanceTests (Swift-5 language-mode conformance).
//
// Proves the generated-UniFFI Swift surface (IrisCore.swift, D-2 pin) compiles
// under Swift 5 language mode and that `IosBleAdapter` satisfies the generated
// `FfiBleAdapter: AnyObject, Sendable` protocol via `@unchecked Sendable`
// (DEC-IOS-0002). The `#if compiler(>=6)` Sendable extensions in the generated
// bindings are compiled inside the Swift-6 COMPILER under the `-swift-version 5`
// language mode — these assertions are compile-time contracts, not runtime
// engine calls (the Rust core is linked only on the macOS CI runner).
import Foundation
import XCTest

#if canImport(IRIS)
import IRIS
#endif

private func assertSendable<T: Sendable>(_ value: T) -> T { value }
private func assertSendableClass<T: AnyObject & Sendable>(_ value: T) -> T { value }
private func assertBleAdapter<T: FfiBleAdapter>(_ value: T) -> T { value }

final class FfiConformanceTests: XCTestCase {

    /// AC-3: the 11-op adapter conforms to the generated Sendable protocol.
    func testIosBleAdapterConformsToGeneratedProtocol() {
        let mock = MockBleSeam()
        let adapter = IosBleAdapter(central: mock, peripheral: mock)
        _ = assertBleAdapter(adapter)
        _ = assertSendableClass(adapter)
    }

    /// AC-3: generated FFI records satisfy `Sendable` under the pinned mode.
    func testGeneratedRecordsAreSendable() {
        let filter = FfiScanFilter(serviceUuid: IrisBleConstants.serviceUUIDHex, address: "", rssiFloor: -95)
        let adv = FfiAdvertisementData(payload: Data([0x01]), nonConnectable: false)
        _ = assertSendable(filter)
        _ = assertSendable(adv)
        _ = assertSendable(FfiScanResult(address: "", payload: Data(), rssi: -60))
        _ = assertSendable(FfiGattWriteEvent(handle: 1, charUuid: "", data: Data()))
    }

    /// AC-3: sequence of constant round-trips acting as the compile guard that
    /// the generated types cross the seam with the exact iOS UUIDs.
    func testUuidTranslationsMatchGeneratedConstants() {
        XCTAssertEqual(
            IrisBleConstants.canonicalUUIDString(IrisBleConstants.serviceUUIDHex),
            IrisBleConstants.serviceUUID.uuidString.lowercased()
        )
        XCTAssertEqual(
            IrisBleConstants.canonicalUUIDString(IrisBleConstants.identifyCharacteristicHex),
            IrisBleConstants.identifyUUID.uuidString.lowercased()
        )
        XCTAssertEqual(
            IrisBleConstants.toHex32(IrisBleConstants.serviceUUID),
            IrisBleConstants.serviceUUIDHex
        )
    }
}