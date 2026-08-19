// IOS-001 AC-10/AC-11 — SessionRecovery: thin launch coordinator (DEC-IOS-0005).
//
// Layered on top of CoreBluetooth `willRestoreState` + BGTaskScheduler (NOT a
// custom daemon — iOS forbids background relaunch control). Responsibilities:
//   1. Launch-reason classification: BLE-restoration launch (bluetoothCentrals/
//      bluetoothPeripherals launch options) vs BGTask launch vs user launch.
//   2. BLE init gated on `isProtectedDataAvailable` (a process can be pre-warmed
//      before first unlock — RES-0025 RQ-4).
//   3. Re-arm scan/advertise after restore with debounce (relaunch-crash-loop
//      guard, DEC-BLE-002-0005). No auto-resume of the adapter.
//   4. Re-submit BGAppRefresh + BGProcessing at EVERY launch (schedules do not
//      survive force-quit — RES-0025 RQ-4 finding 3).
//   5. Force-quit ⇒ the system blocks all background launch; the sequence is a
//      documented no-op (never hang-on-launch).
//
// All dependencies are protocols so the macOS-host unit tests (AC-11) drive the
// logic with injected seams (no UIKit/BackgroundTasks/CoreBluetooth).
import Foundation

public enum LaunchReason: String, Equatable {
    case bleRestoration
    case backgroundTask
    case user
}

public protocol LaunchOptionsSource {
    func launchOptions() -> [String: Any]
}

public protocol ProtectedDataGating {
    var isProtectedDataAvailable: Bool { get }
}

public protocol BackgroundTaskResubmitting {
    /// Re-submit BGAppRefresh + BGProcessing requests (best-effort maintenance).
    func resubmitAll()
}

public protocol BleLifecycleRecovering {
    /// Re-arm scan + advertise from willRestoreState (debounced by the caller).
    func reArmAfterRestore()
}

/// UIApplication.LaunchOptionsKey raw values (Foundation-importable; no UIKit
/// needed so the macOS test bundle can construct fixture launch options).
public enum IrisLaunchOptionKeys {
    public static let bluetoothCentrals = "UIApplicationLaunchOptionsBluetoothCentralsKey"
    public static let bluetoothPeripherals = "UIApplicationLaunchOptionsBluetoothPeripheralsKey"
    public static let backgroundTask = "UIApplicationLaunchOptionsBackgroundTaskKey"
}

public final class SessionRecovery {
    private let launchOptions: LaunchOptionsSource
    private let protectedData: ProtectedDataGating
    private let taskResubmitter: BackgroundTaskResubmitting
    private let bleLifecycle: BleLifecycleRecovering
    /// Debounce window between willRestoreState re-arms (relaunch-crash-loop guard).
    private let reArmDebounce: TimeInterval
    private let lock = NSLock()
    private var lastReArm: Date?

    public init(
        launchOptions: LaunchOptionsSource,
        protectedData: ProtectedDataGating,
        taskResubmitter: BackgroundTaskResubmitting,
        bleLifecycle: BleLifecycleRecovering,
        reArmDebounce: TimeInterval = 10.0
    ) {
        self.launchOptions = launchOptions
        self.protectedData = protectedData
        self.taskResubmitter = taskResubmitter
        self.bleLifecycle = bleLifecycle
        self.reArmDebounce = reArmDebounce
    }

    /// Classify the launch reason from the injected launch-options.
    public func classifyLaunchReason() -> LaunchReason {
        let options = launchOptions.launchOptions()
        if options[IrisLaunchOptionKeys.bluetoothCentrals] != nil
            || options[IrisLaunchOptionKeys.bluetoothPeripherals] != nil {
            return .bleRestoration
        }
        if options[IrisLaunchOptionKeys.backgroundTask] != nil {
            return .backgroundTask
        }
        return .user
    }

    /// AC-11 entry point: re-arm BLE (if warranted) + re-submit BG tasks.
    public func run() {
        let reason = classifyLaunchReason()

        // Force-quit / BGTask launches: the system never delivers a real BLE
        // restore here; re-arm only when the launch is a restoration launch and
        // the data is unlocked.
        if reason == .bleRestoration, protectedData.isProtectedDataAvailable {
            reArmWithDebounce()
        }

        // BGTaskScheduler schedules do NOT survive termination/force-quit —
        // re-submit at EVERY launch (AC-11).
        taskResubmitter.resubmitAll()
    }

    /// Debounced re-arm of the central scan + peripheral advertisement.
    public func reArmWithDebounce() {
        let now = Date()
        lock.lock()
        defer { lock.unlock() }
        if let last = lastReArm, now.timeIntervalSince(last) < reArmDebounce {
            return
        }
        lastReArm = now
        bleLifecycle.reArmAfterRestore()
    }
}

// MARK: - Default sources (app target wiring)

#if canImport(UIKit)
import UIKit

/// Real launch-options source backed by the app delegate's launch dictionary.
public final class UIKitLaunchOptionsSource: LaunchOptionsSource {
    private let options: [String: Any]
    public init(_ options: [UIApplication.LaunchOptionsKey: Any]?) {
        self.options = options?.mapKeys(String.init(describing:)) ?? [:]
    }
    public func launchOptions() -> [String: Any] { options }
}

public final class UIKitProtectedDataGate: ProtectedDataGating {
    public init() {}
    public var isProtectedDataAvailable: Bool {
        UIApplication.shared.isProtectedDataAvailable
    }
}
#endif

private extension Dictionary {
    func mapKeys<NewKey: Hashable>(_ transform: (Key) -> NewKey) -> [NewKey: Value] {
        var out: [NewKey: Value] = [:]
        for (k, v) in self { out[transform(k)] = v }
        return out
    }
}