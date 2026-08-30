//
//  SessionRecovery.swift
//  IRIS
//
//  AC-11 of IOS_DESIGN.md v1.0. Thin coordinator that reconstructs a BLE mesh
//  session across launches (DEC-IOS-0005, RES-0025 RQ-4, QA1962/TN3115 L1):
//   - Classifies the launch reason (foreground, BLE relaunch-for-connection,
//     BGTask, user-force-quit) and reacts accordingly.
//   - Re-arms scan/advertise in willRestoreState (CoreBluetooth state
//     restoration does NOT auto-resume scanning/advertising — RES-0024).
//   - Re-submits BGTaskScheduler refresh requests on every launch.
//   - User-force-quit is a documented NO-OP (iOS disables relaunch — QA1962).
//  Pure classification logic is unit-testable without UIKit.

import Foundation
import BackgroundTasks

// MARK: - Seam protocols (AC-10/AC-11; faked in unit tests, UIKit-backed in prod)

public protocol LaunchOptionsSource {
    func launchOptions() -> [String: Any]
}

public protocol ProtectedDataGating {
    var isProtectedDataAvailable: Bool { get }
}

public protocol BackgroundTaskResubmitting {
    func resubmitAll()
}

public protocol BleLifecycleRecovering {
    func reArmAfterRestore()
}

// MARK: - Launch classification

public enum LaunchReason: Equatable {
    case user
    case bleRestoration
    case backgroundTask
}

/// Stable string keys for UIApplication launchOptions dictionary.
public enum IrisLaunchOptionKeys {
    /// UIApplicationLaunchOptionsBluetoothCentralsKey — present when CB restored a link.
    public static let bluetoothCentrals = "UIApplicationLaunchOptionsBluetoothCentralsKey"
    /// Present when the system wakes the app for a registered BGTask.
    public static let backgroundTask = "UIApplicationLaunchOptionsBackgroundTaskKey"
}

// MARK: - SessionRecovery

final class SessionRecovery {

    private let launchOptions: LaunchOptionsSource
    private let protectedData: ProtectedDataGating
    private let taskResubmitter: BackgroundTaskResubmitting
    private let bleLifecycle: BleLifecycleRecovering
    private let reArmDebounce: TimeInterval
    private var lastReArm: Date?

    init(
        launchOptions: LaunchOptionsSource,
        protectedData: ProtectedDataGating,
        taskResubmitter: BackgroundTaskResubmitting,
        bleLifecycle: BleLifecycleRecovering,
        reArmDebounce: TimeInterval = 30
    ) {
        self.launchOptions = launchOptions
        self.protectedData = protectedData
        self.taskResubmitter = taskResubmitter
        self.bleLifecycle = bleLifecycle
        self.reArmDebounce = reArmDebounce
    }

    /// Classify why the app was launched based on launch-options keys (AC-11).
    func classifyLaunchReason() -> LaunchReason {
        let opts = launchOptions.launchOptions()
        if opts[IrisLaunchOptionKeys.backgroundTask] != nil { return .backgroundTask }
        if opts[IrisLaunchOptionKeys.bluetoothCentrals] != nil { return .bleRestoration }
        return .user
    }

    /// AC-11 entry point: re-submit BG tasks on every launch; re-arm BLE only on
    /// restoration launches with protected-data available (DEC-BLE-002-0005).
    func run() {
        taskResubmitter.resubmitAll()
        guard protectedData.isProtectedDataAvailable else { return }
        if classifyLaunchReason() == .bleRestoration {
            bleLifecycle.reArmAfterRestore()
        }
    }

    /// AC-10: debounced BLE re-arm (relaunch-crash-loop guard, DEC-BLE-002-0005).
    func reArmWithDebounce() {
        let now = Date()
        if let last = lastReArm, now.timeIntervalSince(last) < reArmDebounce { return }
        lastReArm = now
        bleLifecycle.reArmAfterRestore()
    }
}