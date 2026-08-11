# BLE Transport

## Overview

Bluetooth Low Energy (BLE) is the primary short-range discovery and communication transport in IRIS. It operates on all modern Android and iOS devices without special hardware, works in background modes (with platform-specific limitations), and has low power draw compared to Wi-Fi. BLE is used for peer discovery (advertising presence), short message exchange (GATT characteristic writes), and as the control channel when Wi-Fi Direct handles bulk data.

---

## BLE 5.x Capabilities

### Physical Layer

| Parameter               | BLE 4.2             | BLE 5.0              | BLE 5.1              | BLE 5.2              |
|-------------------------|---------------------|----------------------|----------------------|----------------------|
| PHY Modes               | 1M PHY              | 1M, 2M, Coded(S2/S8) | 1M, 2M, Coded        | 1M, 2M, Coded        |
| Range (1M PHY)          | ~30–50 m            | ~50–100 m            | ~50–100 m            | ~50–100 m            |
| Range (Coded S8 PHY)    | N/A                 | ~400–1000 m          | ~400–1000 m          | ~400–1000 m          |
| Throughput (2M PHY)     | N/A                 | ~1.4 Mbps            | ~1.4 Mbps            | ~1.4 Mbps            |
| Advertising channels    | 3 primary           | 3 primary + 37 sec.  | 3 primary + 37 sec.  | 3 primary + 37 sec.  |
| ATT MTU max             | 247 bytes           | 512 bytes            | 512 bytes            | 512 bytes            |

### Practical Throughput

The theoretical 2 Mbps data rate of BLE 5.0 2M PHY is almost never achieved in practice. Real-world throughput with GATT protocol overhead:

- **ATT MTU 23 bytes (default, no negotiation):** ~2–4 kbps effective  
- **ATT MTU 185 bytes (negotiated, Android default):** ~25–50 kbps  
- **ATT MTU 512 bytes (maximum, both sides must support):** ~80–120 kbps  
- **BLE 5.0 2M PHY + MTU 512:** up to ~200–250 kbps practical  

For IRIS: negotiate MTU to maximum supported (512 bytes), use 2M PHY where available. Expect 50–150 kbps in field conditions.

### Range Characteristics

- **Open air, 1M PHY:** 30–100 m depending on antenna design and obstructions.
- **Urban, indoors:** 10–30 m through walls. Concrete walls reduce range by 50–70%.
- **BLE 5.0 Coded PHY (S8):** extends range 4× at cost of 8× lower throughput. Useful for rural emergency scenarios where range matters more than speed.
- **Practical IRIS default:** use 1M PHY for data, optionally advertise on Coded PHY for extended discovery in rural emergencies.

### Power Consumption

| Operation                    | Approximate Current Draw |
|------------------------------|--------------------------|
| BLE radio off                | 0 mA                     |
| Passive scan (duty cycle 10%)| 3–8 mA                   |
| Active scan (continuous)     | 8–15 mA                  |
| Advertising (1 Hz interval)  | 1–3 mA                   |
| Advertising (10 Hz interval) | 5–10 mA                  |
| GATT connection, idle        | 2–4 mA                   |
| GATT TX at ~100 kbps         | 8–12 mA                  |

Battery impact is manageable. A 4000 mAh Android battery would sustain continuous BLE scanning for ~20 hours at 8 mA draw. IRIS uses adaptive duty cycling to extend this further.

---

## Android BLE API

### Required Permissions

```xml
<!-- AndroidManifest.xml -->
<!-- Android < 12 (API < 31) -->
<uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30"/>
<uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30"/>
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" android:maxSdkVersion="30"/>

<!-- Android 12+ (API 31+) -->
<uses-permission android:name="android.permission.BLUETOOTH_SCAN"
    android:usesPermissionFlags="neverForLocation"/>
<uses-permission android:name="android.permission.BLUETOOTH_CONNECT"/>
<uses-permission android:name="android.permission.BLUETOOTH_ADVERTISE"/>

<!-- Required for background scanning on Android 8+ -->
<uses-permission android:name="android.permission.FOREGROUND_SERVICE"/>
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE"/>
```

Runtime permission flow (Android 12+):
```kotlin
val blePermissions = arrayOf(
    Manifest.permission.BLUETOOTH_SCAN,
    Manifest.permission.BLUETOOTH_CONNECT,
    Manifest.permission.BLUETOOTH_ADVERTISE,
)
ActivityCompat.requestPermissions(activity, blePermissions, REQUEST_CODE_BLE)
```

### Scanning: BluetoothLeScanner

```kotlin
val bluetoothManager = context.getSystemService(BluetoothManager::class.java)
val adapter = bluetoothManager.adapter
val scanner = adapter.bluetoothLeScanner

// Build scan filter to find IRIS devices only
val scanFilter = ScanFilter.Builder()
    .setServiceUuid(ParcelUuid(IRIS_SERVICE_UUID))
    .build()

// Scan settings — balance between discovery speed and battery
val scanSettings = ScanSettings.Builder()
    .setScanMode(ScanSettings.SCAN_MODE_BALANCED)      // ~5.12s scan, ~5s rest
    // For emergency: SCAN_MODE_LOW_LATENCY (continuous, high battery)
    // For background: SCAN_MODE_LOW_POWER (duty-cycle ~0.1%)
    .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
    .setMatchMode(ScanSettings.MATCH_MODE_AGGRESSIVE)  // fewer misses
    .setNumOfMatches(ScanSettings.MATCH_NUM_MAX_ADVERTISEMENT)
    .setReportDelay(0L)                                // report immediately
    .build()

val scanCallback = object : ScanCallback() {
    override fun onScanResult(callbackType: Int, result: ScanResult) {
        val serviceData = result.scanRecord?.getServiceData(ParcelUuid(IRIS_SERVICE_UUID))
        val advertisement = serviceData?.let { IrisAdvertisement.decode(it) } ?: return
        peerDiscovered(advertisement, result.device, result.rssi)
    }
    override fun onScanFailed(errorCode: Int) {
        handleScanFailure(errorCode)
    }
}

scanner.startScan(listOf(scanFilter), scanSettings, scanCallback)
```

**Android Scan Throttling (Critical):** Starting with Android 8.0 (API 26), the OS throttles apps that call `startScan()` more than 5 times in 30 seconds — subsequent calls fail silently. IRIS must maintain a single scan session and adjust `ScanMode` dynamically rather than stopping and restarting scans.

Starting Android 8.0, scans in background are limited to 30-second windows with 30-second gaps unless the app holds a foreground service with type `FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE`.

### Advertising: BluetoothLeAdvertiser

IRIS uses BLE advertising to broadcast node presence without requiring a prior connection. The advertisement payload carries:
- IRIS service UUID (identifies this as an IRIS node)
- Compressed node advertisement (public key fingerprint, node ID, capability flags, sequence number)

```kotlin
val advertiser = adapter.bluetoothLeAdvertiser

// IRIS advertisement data fits in 31-byte legacy advertisement payload
// or up to 1650 bytes in BLE 5.0 extended advertising
val advertiseData = AdvertiseData.Builder()
    .setIncludeDeviceName(false)       // save space
    .setIncludeTxPowerLevel(true)      // helps RSSI calibration
    .addServiceUuid(ParcelUuid(IRIS_SERVICE_UUID))
    .addServiceData(ParcelUuid(IRIS_SERVICE_UUID), nodeAdvertisementBytes)
    .build()

val advertiseSettings = AdvertiseSettings.Builder()
    .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_BALANCED)
    // ADVERTISE_MODE_LOW_LATENCY: 100ms interval, high visibility, more battery
    // ADVERTISE_MODE_LOW_POWER: 1000ms interval, saves battery
    .setConnectable(true)              // IRIS GATT server accepts connections
    .setTimeout(0)                     // advertise indefinitely
    .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_HIGH)
    .build()

advertiser.startAdvertising(advertiseSettings, advertiseData, advertiseCallback)
```

**Android advertising limit:** A device can run at most 4 simultaneous BLE advertisements (GATT server + 3 additional). IRIS uses 1 advertisement slot.

### GATT Server (Receiving Messages)

IRIS runs a GATT server to receive messages from connecting peers. A single service with a writable characteristic handles inbound message fragments.

```kotlin
val gattServerCallback = object : BluetoothGattServerCallback() {
    override fun onCharacteristicWriteRequest(
        device: BluetoothDevice, requestId: Int,
        characteristic: BluetoothGattCharacteristic,
        preparedWrite: Boolean, responseNeeded: Boolean,
        offset: Int, value: ByteArray
    ) {
        // value is one message fragment
        fragmentAssembler.receive(device.address, offset, value)

        if (responseNeeded) {
            gattServer.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
        }
    }

    override fun onMtuChanged(device: BluetoothDevice, mtu: Int) {
        // Update per-peer MTU tracking
        mtuMap[device.address] = mtu
    }
}

val gattServer = bluetoothManager.openGattServer(context, gattServerCallback)
val irisService = BluetoothGattService(IRIS_SERVICE_UUID, BluetoothGattService.SERVICE_TYPE_PRIMARY)
val messageChar = BluetoothGattCharacteristic(
    IRIS_MESSAGE_CHAR_UUID,
    BluetoothGattCharacteristic.PROPERTY_WRITE or BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE,
    BluetoothGattCharacteristic.PERMISSION_WRITE
)
irisService.addCharacteristic(messageChar)
gattServer.addService(irisService)
```

### GATT Client (Sending Messages)

```kotlin
val gattCallback = object : BluetoothGattCallback() {
    override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
        if (newState == BluetoothProfile.STATE_CONNECTED) {
            gatt.requestMtu(512)  // negotiate maximum MTU immediately
        }
    }

    override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) {
        // Can now send mtu-3 bytes per write (3 bytes GATT overhead)
        val usableMtu = mtu - 3
        sendPendingMessages(gatt, usableMtu)
    }

    override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
        val messageChar = gatt
            .getService(IRIS_SERVICE_UUID)
            ?.getCharacteristic(IRIS_MESSAGE_CHAR_UUID)
        // begin writing fragments
    }
}

val gatt = device.connectGatt(context, false, gattCallback, BluetoothDevice.TRANSPORT_LE)
```

### Background Scanning (Android 8+)

Background BLE operation requires a foreground service. IRIS runs a persistent foreground service during active emergency mode:

```kotlin
class IrisBleService : Service() {
    override fun onCreate() {
        super.onCreate()
        val notification = buildNotification("IRIS Active — Emergency Mode")
        // API 34+ requires specifying type
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(NOTIFICATION_ID, notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
        startBleScanning()
    }
}
```

Without the foreground service, Android 7+ stops background BLE scans after approximately 30 minutes via Doze mode. Even with the foreground service, scan mode is downgraded in Doze.

**Doze Mode Behavior:**
- Doze light: scan continues but at reduced duty cycle (~3% vs 50%).
- Doze full: only `PendingIntent`-based scans survive. IRIS uses `BluetoothLeScanner.startScan(filters, settings, PendingIntent)` as fallback. On result, the PendingIntent wakes the app.

---

## iOS CoreBluetooth

### Required Permissions

```xml
<!-- Info.plist -->
<key>NSBluetoothAlwaysUsageDescription</key>
<string>IRIS uses Bluetooth to communicate with nearby devices during emergencies when Internet is unavailable.</string>

<!-- If supporting iOS 12 and earlier (deprecated in iOS 13) -->
<key>NSBluetoothPeripheralUsageDescription</key>
<string>IRIS uses Bluetooth to communicate with nearby devices.</string>

<!-- Required background modes -->
<key>UIBackgroundModes</key>
<array>
    <string>bluetooth-central</string>    <!-- scanning / connecting -->
    <string>bluetooth-peripheral</string> <!-- advertising / GATT server -->
</array>
```

### CBCentralManager (Scanning)

```swift
import CoreBluetooth

class IrisBleScanner: NSObject, CBCentralManagerDelegate, CBPeripheralDelegate {
    var centralManager: CBCentralManager!
    var discoveredPeripherals: [CBPeripheral] = []

    override init() {
        super.init()
        // Restoration identifier enables state restoration after OS kill
        let options: [String: Any] = [
            CBCentralManagerOptionRestoreIdentifierKey: "com.iris.ble.central",
            CBCentralManagerOptionShowPowerAlertKey: true
        ]
        centralManager = CBCentralManager(delegate: self, queue: .global(qos: .utility), options: options)
    }

    func centralManagerDidUpdateState(_ central: CBCentralManager) {
        guard central.state == .poweredOn else { return }
        startScanning()
    }

    func startScanning() {
        let serviceUUIDs = [CBUUID(string: IRIS_SERVICE_UUID)]
        let options: [String: Any] = [
            CBCentralManagerScanOptionAllowDuplicatesKey: false
        ]
        // Note: allowDuplicates: true needed for RSSI updates but not permitted in background
        centralManager.scanForPeripherals(withServices: serviceUUIDs, options: options)
    }

    func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
                        advertisementData: [String: Any], rssi RSSI: NSNumber) {
        let serviceData = advertisementData[CBAdvertisementDataServiceDataKey] as? [CBUUID: Data]
        let irisData = serviceData?[CBUUID(string: IRIS_SERVICE_UUID)]
        // Decode IrisAdvertisement from irisData
    }
}
```

**iOS Background Scanning Limitation:** When iOS suspends the app, BLE scanning continues but with significant restrictions:
- `allowDuplicates: true` is silently downgraded to `false` in background. This means rapid advertising updates from the same peer are not detected.
- Scan results are batched and delivered on app wake. The app may not receive results for 10–30 seconds.
- iOS uses a lower duty cycle in background — estimated 1–2% vs 50% in foreground.
- Solution: design peer discovery to tolerate intermittent detection. IRIS expects peers to advertise continuously; single missed advertisement is acceptable.

**iOS State Restoration:** If iOS kills the app due to memory pressure, CoreBluetooth can restore the central manager state on next launch via `centralManager(_:willRestoreState:)`. IRIS implements this callback to resume BLE operations transparently.

### CBPeripheralManager (Advertising + GATT Server)

```swift
class IrisBlePeripheral: NSObject, CBPeripheralManagerDelegate {
    var peripheralManager: CBPeripheralManager!
    var messageCharacteristic: CBMutableCharacteristic!

    func startAdvertising() {
        let advertisementData: [String: Any] = [
            CBAdvertisementDataLocalNameKey: "IRIS",
            CBAdvertisementDataServiceUUIDsKey: [CBUUID(string: IRIS_SERVICE_UUID)]
        ]
        peripheralManager.startAdvertising(advertisementData)
        // Note: service data cannot be included in iOS BLE advertisements
        // This is a key iOS limitation vs Android
    }

    func peripheralManager(_ peripheral: CBPeripheralManager,
                           didReceiveWrite requests: [CBATTRequest]) {
        for request in requests {
            guard let data = request.value else { continue }
            fragmentAssembler.receive(request.central.identifier, request.offset, data)
            peripheral.respond(to: request, withResult: .success)
        }
    }
}
```

**Critical iOS Advertising Limitation:** iOS cannot include service data in BLE advertisement payloads. Only the service UUID and local name are advertised. The IRIS node advertisement (public key fingerprint, capabilities) must be retrieved via GATT after connection. This requires a connect-to-identify step for iOS peripherals, which Android can skip by reading advertisement data directly.

**iOS Background Advertising Restriction:** When the app is backgrounded:
- iOS strips advertisement data to just the overflow area (hidden from other iOS devices by default).
- iOS devices cannot discover other backgrounded iOS devices via BLE scan.
- iOS devices can still be discovered by Android devices (iOS continues advertising in a limited form detectable by non-iOS scanners).
- **Solution:** At least one node in the vicinity should be in foreground (Android preferred) to maintain discovery.

---

## MTU Negotiation and Message Fragmentation

BLE GATT has a maximum payload of MTU - 3 bytes per write operation. For MTU=512, this is 509 bytes. IRIS messages can be up to 64 KB (configurable). Fragmentation is required.

### Fragmentation Protocol

IRIS uses a simple, reliable fragmentation scheme over BLE GATT:

```
Fragment Header (4 bytes):
┌────────────────┬────────────────┬──────────────┬──────────────┐
│  Message ID    │  Fragment Idx  │  Total Frags │   Flags      │
│   (2 bytes)    │   (1 byte)     │   (1 byte)   │  (1 byte)    │
└────────────────┴────────────────┴──────────────┴──────────────┘
Followed by: payload bytes (MTU - 3 - 5 header bytes)
```

Reassembly buffer per sender:
```rust
struct FragmentAssembler {
    pending: HashMap<(PeerId, u16), MessageAssembly>,  // keyed by (peer, message_id)
}

struct MessageAssembly {
    total_fragments: u8,
    received: Vec<Option<Vec<u8>>>,
    first_seen: Instant,
}

impl FragmentAssembler {
    fn receive(&mut self, peer: PeerId, header: FragmentHeader, data: &[u8]) -> Option<Vec<u8>> {
        let key = (peer, header.message_id);
        let assembly = self.pending.entry(key).or_insert_with(|| {
            MessageAssembly {
                total_fragments: header.total_frags,
                received: vec![None; header.total_frags as usize],
                first_seen: Instant::now(),
            }
        });
        assembly.received[header.fragment_idx as usize] = Some(data.to_vec());

        if assembly.received.iter().all(|f| f.is_some()) {
            // All fragments received — reassemble
            let complete: Vec<u8> = assembly.received.iter()
                .flat_map(|f| f.as_ref().unwrap().iter().copied())
                .collect();
            self.pending.remove(&key);
            Some(complete)
        } else {
            None
        }
    }
}
```

**No retransmission at BLE layer:** IRIS relies on GATT acknowledgment (write-with-response) for individual fragments. If the BLE connection drops mid-message, the message is retransmitted in full at the IRIS message layer, not at the BLE fragment layer.

---

## Multi-Connection Limits

| Platform     | Max Concurrent GATT Connections | Notes                                          |
|--------------|---------------------------------|------------------------------------------------|
| Android      | ~7 (practical), up to 32        | Varies by chipset; Qualcomm typically 7–10    |
| iOS          | ~8 (practical)                  | Undocumented limit; empirical                  |
| Linux (BlueZ)| ~7 (HCI default)                | Configurable via HCI parameters                |

IRIS enforces a per-device connection limit (default: 6) to leave headroom. When limit reached, new connections replace the lowest-priority (most recently idle) existing connection.

---

## BLE Mesh vs Point-to-Point

IRIS uses **point-to-point BLE connections** rather than the Bluetooth Mesh SIG profile. Reasons:

1. **BLE Mesh requires provisioning:** The SIG mesh stack requires each device to be provisioned with a network key by a provisioner. This is impractical in disaster scenarios where arbitrary devices join the network.
2. **Phone support is fragmented:** BLE Mesh support on Android requires API 32+ with specific Bluetooth stack features. iOS support is essentially non-existent in standard apps.
3. **Publish-subscribe model is inflexible:** BLE Mesh publish-subscribe does not map cleanly to IRIS's DTN routing model.
4. **Application-layer mesh is more flexible:** By managing mesh behavior at the IRIS application layer (store-carry-forward, routing decisions), we gain complete control and can implement DTN routing patterns that BLE Mesh cannot support.

The routing engine creates a logical mesh over point-to-point BLE connections. When Node A wants to reach Node C and only knows Node B:
- A connects to B, writes the message to B's GATT server.
- B's IRIS daemon routes to C when B later comes in range of C.

---

## Security

### Transport vs Application Layer Encryption

BLE supports link-layer encryption via Bluetooth pairing (LE Secure Connections). IRIS does **not** rely on BLE pairing for security. Instead:
- BLE connections are established **without pairing** (unencrypted at link layer).
- All message payloads are **end-to-end encrypted at the application layer** using ChaCha20-Poly1305.
- The BLE transport is treated as an untrusted channel, like TCP.

Rationale: Bluetooth pairing requires user interaction and persistent bonding state. In a disaster scenario with unknown devices, this is impractical. Application-layer E2EE is always present regardless of BLE configuration.

### Rotating BLE Addresses

Android and iOS both implement **BLE MAC address randomization** by default. IRIS does not attempt to track peers by MAC address. Instead, peers are identified by their IRIS public key fingerprint, extracted from the IRIS advertisement or GATT handshake. This provides privacy: a node's BLE address changes every 15 minutes (Android) or per-connection (iOS), but the IRIS identity is stable.

---

## Known Limitations

1. **iOS cannot advertise in background after a connection is established.** Once iOS connects as peripheral to a central device, background advertising stops. Mitigation: iOS nodes should operate primarily as centrals (scanners/clients), with Android nodes acting as peripherals (advertisers/servers).

2. **Android scan throttling:** >5 `startScan()` calls in 30 seconds causes silent failure. IRIS uses a single persistent scan session.

3. **Range limitations:** 10–100 m is insufficient for many disaster scenarios. BLE is the discovery transport; longer-range message delivery uses Wi-Fi Direct, Wi-Fi Aware, or LoRa.

4. **Concurrent connection limits:** Each node can maintain ~7 simultaneous BLE connections. In high-density scenarios (100+ people), the network graph is sparse locally. Routing algorithms must account for this.

5. **BLE advertising payload size:** Legacy BLE advertising allows only 31 bytes. IRIS advertisement must be extremely compact. Extended advertising (BLE 5.0) allows up to 1650 bytes but is not universally supported and iOS does not support extended advertising as peripheral.

6. **No reliable multicast:** BLE has no native multicast. IRIS simulates broadcast by iterating over all connected peers. For 7 connections and a 1 KB message: 7 × 7 fragments × GATT write = ~140 ms minimum.

7. **Background limitations on iOS:** iOS aggressively manages background apps. BLE scanning in background is limited to specific service UUIDs and at reduced duty cycle. IRIS cannot rely on iOS background BLE for time-sensitive discovery.
