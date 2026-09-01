// IOS-001 — AppDelegate: launch wiring (AC-10/AC-11/AC-14).
//
// didFinishLaunching → construct identity + adapter + engine, run
// SessionRecovery (launch-reason classify, BLE re-arm gate on protected data,
// BGTask re-submit), register BGTaskWiring, and request notification auth.
import UIKit
import CryptoKit
import os.log

@MainActor
public final class AppDelegate: UIResponder, UIApplicationDelegate {

    /// Bug #25: UIKit creates the real AppDelegate via the app lifecycle — creating
    /// a second instance via `let shared = AppDelegate()` leaves its adapter nil,
    /// so callers get a useless object. Use UIApplication.shared.delegate instead.
    public static var shared: AppDelegate { UIApplication.shared.delegate as! AppDelegate }

    public private(set) var engine: IrisEngine?
    public private(set) var adapter: IosBleAdapter?
    public private(set) var sessionRecovery: SessionRecovery?
    @available(iOS 16.1, *)
    public private(set) var liveActivity: LiveActivityController?

    private var bgTaskWiring: BGTaskWiring?

    public func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
    ) -> Bool {
        // ---- Adapter + engine (AC-4..AC-9). The adapter is constructed FIRST
        // so the pre-warm retry in applicationDidBecomeActive can reuse it
        // (the identity guard below may return early before first unlock —
        // IOS-SECURITY_REVIEW IOS-RT-101).
        let adapter = IosBleAdapter(
            central: RealBleCentralSeam(),
            peripheral: RealBlePeripheralSeam()
        )
        self.adapter = adapter

        // ---- Identity (AC-12): identity.v1 provision/load; no biometric flag.
        guard let pair = try? KeychainEd25519.identity() else {
            // Protected data unavailable at pre-warm: retry on didBecomeActive
            // (recorded limitation; never hang-on-launch).
            return true
        }
        // IDENT_DESIGN D1: node PeerId = Ed25519 public-key bytes (32 B).
        let nodeId = pair.verifyingKeyRaw
        if let engine = try? IrisEngine(ble: adapter, nodeId: nodeId) {
            self.engine = engine
        } else {
            return true
        }

        // ---- BGTask registration (AC-14) happens once at startup.
        let wiring = BGTaskWiring(maintenance: BestEffortMaintenance(engine: self.engine))
        wiring.register()
        self.bgTaskWiring = wiring

        // ---- SessionRecovery (AC-10/AC-11). Shares the single BGTaskWiring instance.
        let recovery = SessionRecovery(
            launchOptions: UIKitLaunchOptionsSource(launchOptions),
            protectedData: UIKitProtectedDataGate(),
            taskResubmitter: wiring,
            bleLifecycle: BleRestorationTarget(engine: self.engine, adapter: adapter)
        )
        self.sessionRecovery = recovery
        recovery.run()

        // ---- Notifications (standard + critical-alert fallback).
        Task {
            _ = await IrisNotifications().requestAuthorization()
        }

        // ---- Live Activity (AC-13, foreground-gated, iOS 16.1+).
        if #available(iOS 16.1, *) {
            self.liveActivity = LiveActivityController()
        }

        // Bug #32: log startAll errors rather than swallowing them.
        do {
            try engine.startAll()
        } catch {
            os_log("ios: engine.startAll failed: %@", type: .error, error.localizedDescription)
        }
        return true
    }

    public func applicationDidBecomeActive(_ application: UIApplication) {
        // Pre-warm retry: if first-unlock data arrived after launch, bring the
        // engine up now.
        guard engine == nil else { return }
        guard let pair = try? KeychainEd25519.identity() else { return }
        let nodeId = pair.verifyingKeyRaw
        if let adapter = self.adapter, let e = try? IrisEngine(ble: adapter, nodeId: nodeId) {
            self.engine = e
            // Bug #32: log errors on retry path too.
            do {
                try e.startAll()
            } catch {
                os_log("ios: engine.startAll (retry) failed: %@", type: .error, error.localizedDescription)
            }
        }
    }
}

/// Best-effort maintenance work (AC-14 / G-IOS-4): routing maintenance, message
/// expiry, metric flush. Never critical messaging / emergency delivery.
@MainActor
public final class BestEffortMaintenance: IrisMaintenanceWork {
    private weak var engine: IrisEngine?
    public init(engine: IrisEngine?) {
        self.engine = engine
    }
    public func run(token: @escaping () -> Void) {
        // Bug #58: trigger metric flush on the Rust engine so the background
        // task is not a complete no-op. Routing/expiry are event-driven and do
        // not need an explicit tick; the token must always be called so the
        // BGTask never hangs.
        engine?.performMaintenance()
        token()
    }
}

// MARK: - UIKit-backed seam adapters (LaunchOptionsSource / ProtectedDataGating)

struct UIKitLaunchOptionsSource: LaunchOptionsSource {
    private let options: [UIApplication.LaunchOptionsKey: Any]?
    init(_ options: [UIApplication.LaunchOptionsKey: Any]?) { self.options = options }
    func launchOptions() -> [String: Any] {
        var result: [String: Any] = [:]
        options?.forEach { key, value in result[key.rawValue] = value }
        return result
    }
}

struct UIKitProtectedDataGate: ProtectedDataGating {
    var isProtectedDataAvailable: Bool {
        UIApplication.shared.isProtectedDataAvailable
    }
}

/// AC-10/AC-11 target: re-arm scan + advertise after willRestoreState.
@MainActor
public final class BleRestorationTarget: BleLifecycleRecovering {
    private weak var engine: IrisEngine?
    private weak var adapter: IosBleAdapter?
    // Bug #25: inject adapter directly — reading AppDelegate.shared.adapter during
    // init() could reference a dummy second instance created by the old `static let`.
    public init(engine: IrisEngine?, adapter: IosBleAdapter?) {
        self.engine = engine
        self.adapter = adapter
    }
    public func reArmAfterRestore() {
        // The CoreBluetooth managers re-drive willRestoreState internally
        // (RealBleCentralSeam/RealBlePeripheralSeam forward didRestore); the
        // adapter re-arms the last scan filter from its centralSeam(didRestore).
        // The engine stays the poller owner.
        guard let engine else { return }
        try? engine.startAll()
    }
}