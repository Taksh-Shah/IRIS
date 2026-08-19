// IOS-001 — AppDelegate: launch wiring (AC-10/AC-11/AC-14).
//
// didFinishLaunching → construct identity + adapter + engine, run
// SessionRecovery (launch-reason classify, BLE re-arm gate on protected data,
// BGTask re-submit), register BGTaskWiring, and request notification auth.
import UIKit
import CryptoKit

@MainActor
public final class AppDelegate: UIResponder, UIApplicationDelegate {

    /// System-context singleton the UI shells read (engine, identity, recovery).
    public static let shared = AppDelegate()

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
        // ---- Identity (AC-12): identity.v1 provision/load; no biometric flag.
        let identity = KeychainEd25519()
        guard let key = try? identity.loadOrCreate() else {
            // Protected data unavailable at pre-warm: retry on didBecomeActive
            // (recorded limitation; never hang-on-launch).
            return true
        }
        // IDENT_DESIGN D1: node PeerId = Ed25519 public-key bytes (32 B).
        let nodeId = key.publicKey.rawRepresentation

        // ---- Adapter + engine (AC-4..AC-9).
        let adapter = IosBleAdapter(
            central: RealBleCentralSeam(),
            peripheral: RealBlePeripheralSeam()
        )
        self.adapter = adapter
        if let engine = try? IrisEngine(ble: adapter, nodeId: nodeId) {
            self.engine = engine
        } else {
            return true
        }

        // ---- SessionRecovery (AC-10/AC-11).
        let recovery = SessionRecovery(
            launchOptions: UIKitLaunchOptionsSource(launchOptions),
            protectedData: UIKitProtectedDataGate(),
            taskResubmitter: BGTaskWiring(maintenance: BestEffortMaintenance(engine: self.engine)),
            bleLifecycle: BleRestorationTarget(engine: self.engine)
        )
        self.sessionRecovery = recovery
        recovery.run()

        // ---- BGTask registration (AC-14) happens once at startup.
        let wiring = BGTaskWiring(maintenance: BestEffortMaintenance(engine: self.engine))
        wiring.register()
        wiring.resubmitAll()
        self.bgTaskWiring = wiring

        // ---- Notifications (standard + critical-alert fallback).
        Task {
            _ = await IrisNotifications().requestAuthorization()
        }

        // ---- Live Activity (AC-13, foreground-gated, iOS 16.1+).
        if #available(iOS 16.1, *) {
            self.liveActivity = LiveActivityController()
        }

        try? engine.startAll()
        return true
    }

    public func applicationDidBecomeActive(_ application: UIApplication) {
        // Pre-warm retry: if first-unlock data arrived after launch, bring the
        // engine up now.
        guard engine == nil else { return }
        guard let key = try? KeychainEd25519().loadOrCreate() else { return }
        let nodeId = key.publicKey.rawRepresentation
        if let adapter = self.adapter, let e = try? IrisEngine(ble: adapter, nodeId: nodeId) {
            self.engine = e
            try? e.startAll()
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
        // The Rust engine performs routing/expiry/metric maintenance internally;
        // this slot is where periodic maintenance kicks would surface. Always
        // complete the token so the BG task never hangs.
        token()
    }
}

/// AC-10/AC-11 target: re-arm scan + advertise after willRestoreState.
@MainActor
public final class BleRestorationTarget: BleLifecycleRecovering {
    private weak var engine: IrisEngine?
    private weak var adapter: IosBleAdapter?
    public init(engine: IrisEngine?) {
        self.engine = engine
        self.adapter = AppDelegate.shared.adapter
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