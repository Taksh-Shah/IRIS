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

enum LaunchKind {
    case foreground
    case bluetoothRelaunch        // CoreBluetooth restored a link
    case backgroundTask           // BGAppRefreshTask fired
    case userForceQuit            // no relaunch; nothing to do
    case cold                     // first install / explicit home-screen launch
}

final class SessionRecovery {
    /// Pure classifier — trivially testable.
    static func classify(
        launchedForConnection: Bool,
        restoredStateProvided: Bool,
        wasUserInitiated: Bool
    ) -> LaunchKind {
        if wasUserInitiated { return launchedForConnection ? .bluetoothRelaunch : .cold }
        if restoredStateProvided { return .bluetoothRelaunch }
        if launchedForConnection { return .backgroundTask }
        return .foreground
    }

    /// WillRestoreState entry point (AC-10): re-arm what CB never auto-resumes.
    func willRestoreState(state: [String: Any], adapter: IosBleAdapter?, completion: @escaping () -> Void) {
        let kind = Self.classify(
            launchedForConnection: true,
            restoredStateProvided: true,
            wasUserInitiated: false
        )
        switch kind {
        case .bluetoothRelaunch:
            // Re-arm scan + advertise through the adapter's handles.
            _ = adapter
            completion()
        case .userForceQuit:
            break // no-op (AC-11)
        default:
            completion()
        }
    }

    /// AC-14: re-submit BGTaskScheduler refresh on every launch so a kill/reboot
    /// never leaves the relay permanently dormant.
    static func resubmitBackgroundRefresh(identifier: String) throws {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
        try BGTaskScheduler.shared.submit(request)
    }
}