# IRIS iOS Platform Documentation

## Platform Targets

- **Minimum iOS**: 14.0
- **Target iOS**: 17.0
- **Xcode**: 15.0+
- **Swift**: 5.9+
- **SwiftUI**: iOS 14+

## Architecture

### Stack

```
UI Layer:          SwiftUI + NavigationStack (iOS 16+) / NavigationView (iOS 14-15)
State Management:  MVVM with ObservableObject + @Published
Async:             Swift Concurrency (async/await) + Combine for BLE events
Persistence:       Core Data (message metadata)
Rust FFI:          UniFFI-generated IrisCore.swift + XCFramework
BLE:               CoreBluetooth (CBCentralManager + CBPeripheralManager)
MCF:               MultipeerConnectivity (Apple-to-Apple supplement)
Network:           Network.framework (NWBrowser, NWConnection)
Background:        BGTaskScheduler
```

### Directory Structure

```
ios/
├── IRIS.xcodeproj
├── IRIS/
│   ├── App/
│   │   ├── IRISApp.swift          # @main entry point
│   │   └── AppDelegate.swift      # UIApplicationDelegate for background tasks
│   ├── UI/
│   │   ├── Screens/               # SwiftUI views (one per major screen)
│   │   └── Components/            # Reusable SwiftUI components
│   ├── ViewModel/                 # ObservableObject ViewModels
│   ├── Services/
│   │   ├── IrisBleManager.swift   # CoreBluetooth wrapper
│   │   ├── IrisMCFManager.swift   # MultipeerConnectivity wrapper
│   │   └── IrisNetworkManager.swift # Network.framework wrapper
│   ├── Repository/                # Data access layer
│   ├── CoreData/                  # Core Data model + NSManagedObject subclasses
│   ├── RustFFI/                   # UniFFI generated files (committed)
│   │   ├── IrisCore.swift
│   │   └── IrisCoreFFI.h
│   └── Resources/
│       └── Info.plist
└── IrisFramework/                 # XCFramework (compiled Rust)
    ├── ios-arm64/
    └── ios-arm64_x86_64-simulator/
```

## CoreBluetooth Implementation

### CBCentralManager (Scanner/Client)

```swift
class IrisBleManager: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    private var centralManager: CBCentralManager!
    private var discoveredPeripherals: [UUID: CBPeripheral] = [:]

    override init() {
        super.init()
        centralManager = CBCentralManager(
            delegate: self,
            queue: DispatchQueue(label: "app.iris.ble", qos: .userInitiated),
            options: [CBCentralManagerOptionRestoreIdentifierKey: "IrisCentralManager"]
        )
    }

    func startScanning() {
        let serviceUUIDs = [CBUUID(string: IRIS_SERVICE_UUID)]
        centralManager.scanForPeripherals(
            withServices: serviceUUIDs,
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
    }
}
```

### CBPeripheralManager (Advertiser/Server)

```swift
class IrisBlePeripheralManager: NSObject, CBPeripheralManagerDelegate {
    private var peripheralManager: CBPeripheralManager!

    override init() {
        super.init()
        peripheralManager = CBPeripheralManager(
            delegate: self,
            queue: DispatchQueue(label: "app.iris.peripheral", qos: .userInitiated),
            options: [CBPeripheralManagerOptionRestoreIdentifierKey: "IrisPeripheralManager"]
        )
    }

    func startAdvertising() {
        let advertisementData: [String: Any] = [
            CBAdvertisementDataServiceUUIDsKey: [CBUUID(string: IRIS_SERVICE_UUID)],
            CBAdvertisementDataLocalNameKey: "IRIS"
        ]
        peripheralManager.startAdvertising(advertisementData)
    }
}
```

### Background Modes (Info.plist)

```xml
<key>UIBackgroundModes</key>
<array>
    <string>bluetooth-central</string>
    <string>bluetooth-peripheral</string>
    <string>fetch</string>
    <string>processing</string>
</array>
```

Both `bluetooth-central` AND `bluetooth-peripheral` must be declared or iOS will not restore the CoreBluetooth managers after app suspension.

## Critical iOS Background Limitation

This is the single most important iOS-specific constraint in IRIS.

### The Problem

When an iOS app goes to the background:
1. **CBCentralManager (scanning)**: CONTINUES. The app can scan for and discover BLE peripherals in the background. Discovery callbacks fire and wake the app briefly.
2. **CBPeripheralManager (advertising)**: **STOPS**. iOS suspends BLE advertising when the app is backgrounded. The peripheral is no longer visible to other scanning devices.

**Consequence**: An iOS device in the background **cannot be discovered** by other IRIS nodes. It cannot act as a relay initiator. It can receive data if a connection was already established before backgrounding, but it cannot accept new connections.

### CoreBluetooth State Preservation/Restoration

iOS provides state preservation so BLE managers survive app termination:
- App is killed by OS to reclaim memory
- A BLE event occurs (device connects, data received)
- iOS restores the app in the background
- CoreBluetooth calls `centralManager(_:willRestoreState:)` and `peripheralManager(_:willRestoreState:)`

The restoration identifiers (`CBCentralManagerOptionRestoreIdentifierKey`) must be consistent across launches.

### Impact on IRIS iOS Role

In background, an iOS device:
- **CAN**: Scan for neighbors, receive messages via GATT from a connected peripheral, forward messages it already has queued to already-connected peers
- **CANNOT**: Advertise its presence, accept new GATT connections, act as a GATT server to new devices

**Design implication**: iOS is a **limited relay node** when backgrounded. In a mixed network:
- Android devices handle the relay-heavy work
- iOS devices contribute when in foreground
- Gateway nodes (Linux/Windows) are critical for iOS users

### Mitigation: MultipeerConnectivity Framework

MCF uses a combination of BLE, Wi-Fi, and Wi-Fi Direct. For Apple-to-Apple communication, MCF works somewhat better in background than raw CoreBluetooth, as Apple has optimized it for this use case.

```swift
class IrisMCFManager: NSObject, MCFSessionDelegate, MCFNearbyServiceAdvertiserDelegate, MCFNearbyServiceBrowserDelegate {
    private let serviceType = "iris-mesh"
    private var session: MCSession!
    private var advertiser: MCNearbyServiceAdvertiser!
    private var browser: MCNearbyServiceBrowser!
    private let myPeerId = MCPeerID(displayName: UIDevice.current.name)

    override init() {
        super.init()
        session = MCSession(peer: myPeerId, securityIdentity: nil, encryptionPreference: .required)
        session.delegate = self
        advertiser = MCNearbyServiceAdvertiser(peer: myPeerId, discoveryInfo: nil, serviceType: serviceType)
        browser = MCNearbyServiceBrowser(peer: myPeerId, serviceType: serviceType)
    }
}
```

MCF transport is used in parallel with CoreBluetooth for Apple-to-Apple paths, with priority given to whichever transport has an established connection.

## Background Task Scheduling

### BGProcessingTask

For periodic background work (routing maintenance, message expiry, metric flush):

```swift
// Registration (AppDelegate or App struct)
BGTaskScheduler.shared.register(forTaskWithIdentifier: "app.iris.maintenance", using: nil) { task in
    self.handleMaintenanceTask(task: task as! BGProcessingTask)
}

// Scheduling
func scheduleMaintenanceTask() {
    let request = BGProcessingTaskRequest(identifier: "app.iris.maintenance")
    request.requiresNetworkConnectivity = false
    request.requiresExternalPower = false
    request.earliestBeginDate = Date(timeIntervalSinceNow: 15 * 60)
    try? BGTaskScheduler.shared.submit(request)
}

func handleMaintenanceTask(task: BGProcessingTask) {
    task.expirationHandler = { task.setTaskCompleted(success: false) }
    Task {
        await IrisCoreManager.shared.runMaintenanceCycle()
        task.setTaskCompleted(success: true)
        scheduleMaintenanceTask()
    }
}
```

BGProcessingTask provides up to ~30 seconds typically, up to a few minutes if conditions are right. Not guaranteed to run on schedule.

### Push Notification Wake-Up

When IRIS relay server is reachable (Internet available), APNs can be used to wake the iOS app when a high-priority message is waiting:

1. Relay server receives P0 message destined for iOS user
2. Relay server sends APNs push (background notification, `content-available: 1`)
3. iOS wakes app for ~30 seconds in background
4. App downloads message via relay server (Internet path)
5. App also triggers BLE scan burst to check for nearby peers

This is a fallback path only — IRIS functions fully offline.

## Required Permissions (Info.plist)

```xml
<key>NSBluetoothAlwaysUsageDescription</key>
<string>IRIS uses Bluetooth to communicate with nearby devices in your mesh network even when the network is unavailable.</string>

<key>NSLocalNetworkUsageDescription</key>
<string>IRIS connects to devices on your local network to relay messages when other connections are unavailable.</string>

<key>NSLocationWhenInUseUsageDescription</key>
<string>IRIS optionally uses your location to improve emergency routing.</string>
```

## Swift-Rust FFI via UniFFI

The same `iris.udl` file used for Android generates Swift bindings:

```bash
uniffi-bindgen generate src/iris.udl --language swift --out-dir ios/IRIS/RustFFI/
```

The XCFramework is built:
```bash
# Build for device and simulator
cargo build --target aarch64-apple-ios --release
cargo build --target aarch64-apple-ios-sim --release
cargo build --target x86_64-apple-ios --release

# Create XCFramework
xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libiriscode.a \
    -library target/universal-sim/libiriscode.a \
    -output ios/IrisFramework/IrisCore.xcframework
```

Swift calling convention example:
```swift
// UniFFI-generated Swift
let core = try IrisCore(config: IrisCoreConfig(nodeId: myNodeId))
let messageId = try core.sendMessage(
    recipientId: recipientId,
    payload: Data(messageBytes),
    priority: .p4Normal
)
```

## Security

### Secure Enclave Key Storage

```swift
let attributes: [String: Any] = [
    kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
    kSecAttrKeySizeInBits as String: 256,
    kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
    kSecPrivateKeyAttrs as String: [
        kSecAttrIsPermanent as String: true,
        kSecAttrApplicationTag as String: "app.iris.identity".data(using: .utf8)!,
        kSecAccessControl as String: SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            .privateKeyUsage,
            nil
        )!
    ]
]
var error: Unmanaged<CFError>?
guard let privateKey = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
    throw error!.takeRetainedValue()
}
```

Note: Secure Enclave supports ECDSA P-256 only, not Ed25519. Same pattern as Android Keystore — hardware-backed identity key uses ECDSA, session encryption keys managed by Rust core.

### Core Data Encryption

Messages metadata stored in Core Data with file protection:

```swift
// NSPersistentStoreDescription
let description = NSPersistentStoreDescription(url: storeURL)
description.setOption(FileProtectionType.complete as NSObject,
                      forKey: NSPersistentStoreFileProtectionKey)
```

`NSFileProtectionComplete`: data is inaccessible when device is locked. Appropriate for message metadata.

## Testing

- Unit tests: XCTest + async test support (`@MainActor`, `XCTestExpectation`)
- UI tests: XCUITest (limited, test basic flows only)
- BLE tests: physical iPhone required — iOS Simulator does not support CoreBluetooth
- Integration tests: Rust core tested separately; iOS layer tested with fake Rust FFI (protocol-level mock)
- Background behavior: manual testing on physical device (background task scheduling cannot be fully automated)
