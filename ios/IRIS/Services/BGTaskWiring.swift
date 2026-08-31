// IOS-001 AC-14 — BGTaskWiring: BGTaskScheduler maintenance registration.
//
// G-IOS-4 posture: BGAppRefresh + BGProcessing are heuristic — no guaranteed
// cadence, and schedules do NOT survive force-quit (RES-0025 RQ-4). This wiring
// therefore (a) registers both task identifiers, (b) re-submits at EVERY launch
// (via `SessionRecovery` / `AppDelegate`), and (c) treats the maintenance window
// as BEST-EFFORT only — it never schedules critical messaging or emergency
// delivery (AC-14 / G-IOS-4).
//
// iOS-only (BackgroundTasks); the macOS-host test bundle excludes this file and
// tests the pure `MaintenanceGate` logic + resubmit seam instead.
import Foundation
import BackgroundTasks
import os.log

public protocol IrisMaintenanceWork {
    /// Best-effort maintenance: routing tables, message expiry, metric flush.
    /// `token` must be completed on every path (expiration + success).
    func run(token: @escaping () -> Void)
}

public protocol BackgroundTaskScheduling {
    func registerRefresh(id: String, handler: @escaping (BGTask) -> Void)
    func registerProcessing(id: String, handler: @escaping (BGTask) -> Void)
    func submit(_ request: BGTaskRequest) throws
}

/// Real BGTaskScheduler surface (thin wrapper for test seam symmetry).
public final class RealBackgroundTaskScheduler: BackgroundTaskScheduling {
    public init() {}
    public func registerRefresh(id: String, handler: @escaping (BGTask) -> Void) {
        // Bug #24: use background queue so the handler never runs on main
        BGTaskScheduler.shared.register(forTaskWithIdentifier: id, using: DispatchQueue.global(qos: .background), launchHandler: handler)
    }
    public func registerProcessing(id: String, handler: @escaping (BGTask) -> Void) {
        BGTaskScheduler.shared.register(forTaskWithIdentifier: id, using: DispatchQueue.global(qos: .background), launchHandler: handler)
    }
    public func submit(_ request: BGTaskRequest) throws {
        try BGTaskScheduler.shared.submit(request)
    }
}

public final class BGTaskWiring: BackgroundTaskResubmitting {
    public static let refreshIdentifier = "com.iris.bgtask.refresh"
    public static let processingIdentifier = "com.iris.bgtask.processing"

    private let scheduler: BackgroundTaskScheduling
    private let maintenance: IrisMaintenanceWork
    private let now: () -> Date

    public init(
        scheduler: BackgroundTaskScheduling = RealBackgroundTaskScheduler(),
        maintenance: IrisMaintenanceWork,
        now: @escaping () -> Date = Date.init
    ) {
        self.scheduler = scheduler
        self.maintenance = maintenance
        self.now = now
    }

    /// Register both task identifiers (called once at startup).
    public func register() {
        scheduler.registerRefresh(id: Self.refreshIdentifier) { [weak self] task in
            self?.handle(task)
        }
        scheduler.registerProcessing(id: Self.processingIdentifier) { [weak self] task in
            self?.handle(task)
        }
    }

    /// AC-14 / SessionRecovery: re-submit both requests at every launch.
    public func resubmitAll() {
        let refresh = BGAppRefreshTaskRequest(identifier: Self.refreshIdentifier)
        refresh.earliestBeginDate = now().addingTimeInterval(15 * 60)
        do { try scheduler.submit(refresh) } catch {
            // Bug #24: log instead of silently swallowing (TooManyPending, unavailable after force-quit)
            os_log("ios: BG refresh submit failed: %@", type: .error, error.localizedDescription)
        }

        let processing = BGProcessingTaskRequest(identifier: Self.processingIdentifier)
        processing.requiresNetworkConnectivity = false
        processing.earliestBeginDate = now().addingTimeInterval(30 * 60)
        do { try scheduler.submit(processing) } catch {
            os_log("ios: BG processing submit failed: %@", type: .error, error.localizedDescription)
        }
    }

    /// Expiration handler + setTaskCompleted on EVERY path (AC-14).
    private func handle(_ task: BGTask) {
        // Bug #14: guard against double setTaskCompleted (expiration + maintenance
        // token both firing causes a fatal BGTask crash).
        var completed = false
        let lock = NSLock()
        func complete(_ success: Bool) {
            lock.lock(); defer { lock.unlock() }
            guard !completed else { return }
            completed = true
            task.setTaskCompleted(success: success)
        }
        task.expirationHandler = { complete(false) }
        maintenance.run(token: { complete(true) })
    }
}