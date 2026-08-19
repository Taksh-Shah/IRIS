// IOS-001 AC-10/AC-11 — SessionRecoveryTests (macOS-host logic seams).
//
// Launch-reason classification, BGTask re-submission at every launch, force-quit
// no-op, BLE re-arm gated on protected-data availability, and debounce of the
// re-arm (relaunch-crash-loop guard, DEC-BLE-002-0005).
import Foundation
import XCTest

#if canImport(IRIS)
import IRIS
#endif

// MARK: - Fixtures

final class FakeLaunchOptions: LaunchOptionsSource {
    private let options: [String: Any]
    init(_ options: [String: Any]) { self.options = options }
    func launchOptions() -> [String: Any] { options }
}

final class FakeProtectedData: ProtectedDataGating {
    var available: Bool
    init(_ available: Bool) { self.available = available }
    var isProtectedDataAvailable: Bool { available }
}

final class FakeTaskResubmitter: BackgroundTaskResubmitting {
    var resubmitCount = 0
    func resubmitAll() { resubmitCount += 1 }
}

final class FakeBleLifecycle: BleLifecycleRecovering {
    var reArmCount = 0
    func reArmAfterRestore() { reArmCount += 1 }
}

final class SessionRecoveryTests: XCTestCase {

    private var resubmitter: FakeTaskResubmitter!
    private var ble: FakeBleLifecycle!
    private var protected: FakeProtectedData!

    override func setUp() {
        super.setUp()
        resubmitter = FakeTaskResubmitter()
        ble = FakeBleLifecycle()
        protected = FakeProtectedData(true)
    }

    private func makeSaver(options: [String: Any], debounce: TimeInterval = 10) -> SessionRecovery {
        SessionRecovery(
            launchOptions: FakeLaunchOptions(options),
            protectedData: protected,
            taskResubmitter: resubmitter,
            bleLifecycle: ble,
            reArmDebounce: debounce
        )
    }

    // MARK: - AC-11 launch-reason classification

    func testUserLaunchClassifiedUser() {
        let saver = makeSaver(options: [:])
        XCTAssertEqual(saver.classifyLaunchReason(), .user)
    }

    func testBleRestorationLaunchClassified() {
        let options = [IrisLaunchOptionKeys.bluetoothCentrals: [UUID().uuidString]]
        let saver = makeSaver(options: options)
        XCTAssertEqual(saver.classifyLaunchReason(), .bleRestoration)
    }

    func testBGTaskLaunchClassifiedBackgroundTask() {
        let options = [IrisLaunchOptionKeys.backgroundTask: "com.iris.bgtask.refresh"]
        let saver = makeSaver(options: options)
        XCTAssertEqual(saver.classifyLaunchReason(), .backgroundTask)
    }

    // MARK: - AC-11 re-arm + resubmission semantics

    func testRunResubmitsTasksOnEveryLaunch() {
        let saver = makeSaver(options: [:])
        saver.run()
        saver.run()
        XCTAssertEqual(resubmitter.resubmitCount, 2, "BGTask re-submitted at EVERY launch")
    }

    func testBleReArmOnlyOnRestorationLaunchWithProtectedData() {
        let restoration = makeSaver(options: [IrisLaunchOptionKeys.bluetoothCentrals: [UUID().uuidString]])
        restoration.run()
        XCTAssertEqual(ble.reArmCount, 1, "restoration launch re-arms BLE")

        let user = makeSaver(options: [:])
        user.run()
        XCTAssertEqual(ble.reArmCount, 1, "user launch does NOT re-arm BLE")
    }

    func testGatedOnProtectedData() {
        protected.available = false
        let saver = makeSaver(options: [IrisLaunchOptionKeys.bluetoothCentrals: [UUID().uuidString]])
        saver.run()
        XCTAssertEqual(ble.reArmCount, 0, "BLE init gated on isProtectedDataAvailable")
        XCTAssertEqual(resubmitter.resubmitCount, 1, "task resubmission happens regardless")
    }

    /// AC-10/AC-11 force-quit no-op: user launch with locked data and no
    /// restoration options does nothing but resubmit (never hangs on launch).
    func testForceQuitNoOp() {
        protected.available = true
        let saver = makeSaver(options: [:])
        saver.run()
        XCTAssertEqual(ble.reArmCount, 0)
        XCTAssertEqual(resubmitter.resubmitCount, 1)
    }

    // MARK: - AC-10 debounce (relaunch-crash-loop guard)

    func testDebouncedReArm() {
        let saver = makeSaver(options: [IrisLaunchOptionKeys.bluetoothCentrals: [UUID().uuidString]], debounce: 60)
        saver.reArmWithDebounce()
        saver.reArmWithDebounce()
        XCTAssertEqual(ble.reArmCount, 1, "debounce suppresses a second re-arm inside the window")
    }
}