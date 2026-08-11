# IRIS Swift iOS Layer

## Architecture

IRIS iOS uses MVVM with Swift Concurrency (async/await) and Combine for reactive data flows.

```
SwiftUI View → @StateObject / @EnvironmentObject ViewModel
              → async UseCase / Service calls
              → Repository (protocol-based)
              → IrisCoreDataSource (Rust FFI via UniFFI)
              └─ CoreData (metadata)
```

## Key Patterns

- **ObservableObject + @Published**: ViewModels publish state to SwiftUI views
- **async/await**: All async operations use Swift Concurrency
- **Combine**: BLE events use PassthroughSubject → Combine pipeline → @Published ViewModel state
- **Actor**: Shared mutable state (neighbor table, connection state) isolated in actors
- **Result<T, IrisError>**: Typed error propagation throughout

## ViewModel Example

```swift
@MainActor
final class MessagingViewModel: ObservableObject {
    @Published private(set) var messages: [MessageSummary] = []
    @Published private(set) var networkStatus: NetworkStatus = .unknown
    @Published private(set) var sendError: String?
    @Published private(set) var isLoading = true

    private let irisCore: IrisCore
    private let getMessages: GetMessagesUseCase
    private var eventTask: Task<Void, Never>?

    init(irisCore: IrisCore) {
        self.irisCore = irisCore
        self.getMessages = GetMessagesUseCase(irisCore: irisCore)
        startObservingEvents()
    }

    func loadMessages() async {
        isLoading = true
        do {
            messages = try await getMessages.execute(limit: 100)
        } catch {
            sendError = error.localizedDescription
        }
        isLoading = false
    }

    func sendMessage(to recipientId: String, text: String) async {
        do {
            let payload = Data(text.utf8)
            _ = try await irisCore.sendMessage(
                recipientId: recipientId,
                payload: payload,
                priority: 4
            )
        } catch let error as IrisCoreError {
            sendError = error.localizedDescription
        } catch {
            sendError = "Unexpected error: \(error)"
        }
    }

    func sendSos() async {
        do {
            _ = try await irisCore.sendSos()
        } catch {
            sendError = "SOS failed: \(error)"
        }
    }

    private func startObservingEvents() {
        eventTask = Task { [weak self] in
            guard let self else { return }
            for await event in await irisCore.eventStream() {
                await MainActor.run {
                    self.handleEvent(event)
                }
            }
        }
    }

    private func handleEvent(_ event: IrisEvent) {
        switch event {
        case .messageReceived(let summary):
            messages.insert(summary, at: 0)
        case .deliveryAck(let messageId):
            if let index = messages.firstIndex(where: { $0.id == messageId }) {
                messages[index] = messages[index].withStatus(.delivered)
            }
        case .networkStatusChanged(let status):
            networkStatus = status
        default:
            break
        }
    }

    deinit {
        eventTask?.cancel()
    }
}
```

## SwiftUI View

```swift
struct MessagingView: View {
    @StateObject private var viewModel: MessagingViewModel
    @State private var showingSosAlert = false

    init(irisCore: IrisCore) {
        _viewModel = StateObject(wrappedValue: MessagingViewModel(irisCore: irisCore))
    }

    var body: some View {
        NavigationStack {
            Group {
                if viewModel.isLoading {
                    ProgressView("Loading messages...")
                } else {
                    messageList
                }
            }
            .navigationTitle("IRIS")
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    NetworkStatusIndicator(status: viewModel.networkStatus)
                }
                ToolbarItem(placement: .bottomBar) {
                    SosButton {
                        showingSosAlert = true
                    }
                }
            }
            .alert("Send SOS?", isPresented: $showingSosAlert) {
                Button("Send SOS", role: .destructive) {
                    Task { await viewModel.sendSos() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This will alert your emergency contacts via the mesh network.")
            }
        }
        .task {
            await viewModel.loadMessages()
        }
    }

    private var messageList: some View {
        List(viewModel.messages, id: \.id) { message in
            MessageRow(message: message)
        }
        .listStyle(.plain)
        .refreshable {
            await viewModel.loadMessages()
        }
    }
}
```

## CoreBluetooth Manager

```swift
@globalActor
actor BleActor {
    static let shared = BleActor()
}

final class IrisBleManager: NSObject {
    private var centralManager: CBCentralManager!
    private var peripheralManager: CBPeripheralManager!
    private var discoveredPeripherals: [UUID: CBPeripheral] = [:]
    private var activeConnections: [UUID: CBPeripheral] = [:]

    // Combine publishers for BLE events
    let neighborDiscovered = PassthroughSubject<NeighborDiscoveredEvent, Never>()
    let dataReceived = PassthroughSubject<DataReceivedEvent, Never>()

    private let irisCore: IrisCore
    private let queue = DispatchQueue(label: "app.iris.ble", qos: .userInitiated)

    init(irisCore: IrisCore) {
        self.irisCore = irisCore
        super.init()
        centralManager = CBCentralManager(
            delegate: self,
            queue: queue,
            options: [CBCentralManagerOptionRestoreIdentifierKey: "IrisCentral"]
        )
        peripheralManager = CBPeripheralManager(
            delegate: self,
            queue: queue,
            options: [CBPeripheralManagerOptionRestoreIdentifierKey: "IrisPeripheral"]
        )
    }

    func startScanning() {
        guard centralManager.state == .poweredOn else { return }
        centralManager.scanForPeripherals(
            withServices: [CBUUID(string: IrisConstants.serviceUUID)],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
    }

    func startAdvertising() {
        guard peripheralManager.state == .poweredOn else { return }
        setupGattServer()
        peripheralManager.startAdvertising([
            CBAdvertisementDataServiceUUIDsKey: [CBUUID(string: IrisConstants.serviceUUID)]
        ])
    }

    private func setupGattServer() {
        let service = CBMutableService(type: CBUUID(string: IrisConstants.serviceUUID), primary: true)
        let writeChar = CBMutableCharacteristic(
            type: CBUUID(string: IrisConstants.writeCharUUID),
            properties: [.write, .writeWithoutResponse],
            value: nil,
            permissions: .writeable
        )
        let notifyChar = CBMutableCharacteristic(
            type: CBUUID(string: IrisConstants.notifyCharUUID),
            properties: [.notify, .read],
            value: nil,
            permissions: .readable
        )
        service.characteristics = [writeChar, notifyChar]
        peripheralManager.add(service)
    }
}

extension IrisBleManager: CBCentralManagerDelegate {
    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        if central.state == .poweredOn {
            startScanning()
        }
    }

    func centralManager(_ central: CBCentralManager,
                        didDiscover peripheral: CBPeripheral,
                        advertisementData: [String: Any],
                        rssi RSSI: NSNumber) {
        guard !activeConnections.keys.contains(peripheral.identifier) else { return }
        discoveredPeripherals[peripheral.identifier] = peripheral
        peripheral.delegate = self
        central.connect(peripheral, options: nil)

        neighborDiscovered.send(NeighborDiscoveredEvent(
            peripheralId: peripheral.identifier,
            rssi: RSSI.intValue
        ))
    }

    func centralManager(_ central: CBCentralManager,
                        didConnect peripheral: CBPeripheral) {
        activeConnections[peripheral.identifier] = peripheral
        peripheral.discoverServices([CBUUID(string: IrisConstants.serviceUUID)])
    }
}

extension IrisBleManager: CBPeripheralManagerDelegate {
    func peripheralManager(_ peripheral: CBPeripheralManager,
                          didReceiveWrite requests: [CBATTRequest]) {
        for request in requests {
            guard let data = request.value else { continue }
            peripheral.respond(to: request, withResult: .success)

            // Forward received bytes to Rust core
            Task {
                try? await irisCore.handleIncomingData(
                    transport: "ble",
                    peerId: request.central.identifier.uuidString,
                    data: data
                )
            }
        }
    }

    func peripheralManagerDidRestoreState(_ peripheral: CBPeripheralManager) {
        // State restoration: re-add services if needed
        if peripheralManager.services?.isEmpty ?? true {
            setupGattServer()
        }
    }
}
```

## Background Task Handling

```swift
// AppDelegate.swift
func application(_ application: UIApplication,
                 didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
    registerBackgroundTasks()
    return true
}

func registerBackgroundTasks() {
    BGTaskScheduler.shared.register(
        forTaskWithIdentifier: "app.iris.maintenance",
        using: nil
    ) { task in
        self.handleMaintenanceTask(task: task as! BGProcessingTask)
    }
}

func handleMaintenanceTask(task: BGProcessingTask) {
    let opTask = Task {
        await IrisAppState.shared.core?.runMaintenanceCycle()
        task.setTaskCompleted(success: true)
        scheduleNextMaintenanceTask()
    }
    task.expirationHandler = {
        opTask.cancel()
        task.setTaskCompleted(success: false)
    }
}

func scheduleNextMaintenanceTask() {
    let request = BGProcessingTaskRequest(identifier: "app.iris.maintenance")
    request.requiresNetworkConnectivity = false
    request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
    try? BGTaskScheduler.shared.submit(request)
}
```

## Swift-Rust FFI Usage

UniFFI-generated Swift is used directly:

```swift
// IrisCore.swift (auto-generated by uniffi-bindgen)
// Called from app code like regular Swift:

let config = IrisCoreConfig(
    storagePath: storageURL.path,
    routingAlgorithm: "prophet",
    maxStorageBytes: 500_000_000,
    emergencyEnabled: true
)
let core = try IrisCore(config: config)
try await core.start()

// Send message
let messageId = try await core.sendMessage(
    recipientId: recipientNodeId,
    payload: messageData,
    priority: 4
)

// Get neighbors
let neighbors: [NeighborInfo] = core.getNeighbors()
```

## Core Data Integration

```swift
// IRISDataModel.xcdatamodeld
// Entity: MessageMetadata
// Attributes: messageId (String, required, unique),
//             senderName (String, optional),
//             previewText (String, required),
//             priority (Int16, required),
//             status (String, required),
//             createdAt (Date, required),
//             isIncoming (Boolean, required)

@NSManaged class MessageMetadata: NSManagedObject {
    @NSManaged public var messageId: String
    @NSManaged public var senderName: String?
    @NSManaged public var previewText: String
    @NSManaged public var priority: Int16
    @NSManaged public var status: String
    @NSManaged public var createdAt: Date
    @NSManaged public var isIncoming: Bool
}

class MessageMetadataStore {
    private let context: NSManagedObjectContext

    func save(message: MessageSummary) {
        context.perform {
            let entity = MessageMetadata(context: self.context)
            entity.messageId = message.id
            entity.previewText = message.preview
            entity.priority = Int16(message.priority)
            entity.status = message.status.rawValue
            entity.createdAt = Date(timeIntervalSince1970: TimeInterval(message.createdAtMs) / 1000)
            entity.isIncoming = message.isIncoming
            try? self.context.save()
        }
    }
}
```

## Emergency Notification (Critical Alert)

```swift
func requestCriticalAlertPermission() async -> Bool {
    let center = UNUserNotificationCenter.current()
    let options: UNAuthorizationOptions = [.alert, .sound, .badge, .criticalAlert]
    do {
        return try await center.requestAuthorization(options: options)
    } catch {
        return false
    }
}

func showSosReceivedNotification(senderName: String) {
    let content = UNMutableNotificationContent()
    content.title = "SOS Received"
    content.body = "\(senderName) has sent an emergency SOS signal"
    content.sound = UNNotificationSound.criticalSoundNamed(
        UNNotificationSoundName("sos_alert"),
        withAudioVolume: 1.0
    )
    content.interruptionLevel = .critical
    content.categoryIdentifier = "SOS_RECEIVED"

    let request = UNNotificationRequest(
        identifier: UUID().uuidString,
        content: content,
        trigger: nil
    )
    UNUserNotificationCenter.current().add(request)
}
```

Note: `criticalAlert` entitlement requires explicit Apple approval. Standard alert sound used as fallback during development.
