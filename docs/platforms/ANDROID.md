# IRIS Android Platform Documentation

## Platform Targets

- **Minimum SDK**: 26 (Android 8.0 Oreo)
- **Target SDK**: 34 (Android 14)
- **Build Tools**: 34.0.0
- **AGP**: 8.2+
- **Kotlin**: 1.9+
- **Compose BOM**: 2024.01.00

## Architecture

### Stack

```
UI Layer:          Jetpack Compose + Material 3
Navigation:        Navigation Compose
ViewModel:         AndroidX ViewModel + SavedStateHandle
DI:                Hilt (Dagger-based)
Async:             Kotlin Coroutines + Flow
Local DB:          Room (metadata only, payload in Rust/SQLCipher)
Network (optional):Ktor client (for relay server when Internet available)
Rust FFI:          UniFFI-generated IrisCore.kt + libiriscode.so
BLE:               Android BLE APIs (BluetoothManager, BluetoothGatt)
Wi-Fi Aware:       WifiAwareManager
Wi-Fi Direct:      WifiP2pManager
```

### Architecture Pattern: MVVM + Clean Architecture

```
app/
├── ui/
│   ├── screens/          # Compose screens
│   ├── components/       # Reusable Compose components
│   └── theme/            # Material 3 theme
├── viewmodel/            # ViewModels (one per screen/feature)
├── usecase/              # Use cases (business logic orchestration)
├── repository/           # Repository interfaces + implementations
├── datasource/
│   ├── local/            # Room DAO wrappers
│   └── rust/             # Rust FFI wrappers (IrisCoreDataSource)
├── service/
│   ├── IrisBleService.kt        # Foreground service: BLE relay
│   └── IrisWifiAwareService.kt  # Foreground service: Wi-Fi Aware
├── di/                   # Hilt modules
├── notification/         # Notification channels and builders
└── MainActivity.kt
```

Data flows downward (ViewModel → UseCase → Repository → DataSource → Rust/Room).
Events flow upward via Kotlin Flow or StateFlow.

## BLE Implementation

### Android BLE APIs

```kotlin
// Core BLE objects
val bluetoothManager: BluetoothManager = getSystemService(BLUETOOTH_SERVICE)
val bluetoothAdapter: BluetoothAdapter = bluetoothManager.adapter
val bleScanner: BluetoothLeScanner = bluetoothAdapter.bluetoothLeScanner
val bleAdvertiser: BluetoothLeAdvertiser = bluetoothAdapter.bluetoothLeAdvertiser
```

### Required Permissions

**Manifest declarations:**
```xml
<!-- Android 12+ permissions -->
<uses-permission android:name="android.permission.BLUETOOTH_SCAN"
    android:usesPermissionFlags="neverForLocation" />
<uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
<uses-permission android:name="android.permission.BLUETOOTH_ADVERTISE" />

<!-- Android 10/11: location required for BLE scan -->
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />

<!-- Foreground service permissions -->
<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />

<!-- Battery optimization exclusion -->
<uses-permission android:name="android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS" />
```

**Runtime permission request (Android 12+):**
```kotlin
val blePermissions = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
    arrayOf(
        Manifest.permission.BLUETOOTH_SCAN,
        Manifest.permission.BLUETOOTH_CONNECT,
        Manifest.permission.BLUETOOTH_ADVERTISE,
    )
} else {
    arrayOf(Manifest.permission.ACCESS_FINE_LOCATION)
}
ActivityCompat.requestPermissions(this, blePermissions, REQUEST_CODE_BLE)
```

### BLE Scan Throttling (Android 8+)

Android 8.0 introduced aggressive scan throttling: if an app starts/stops BLE scans more than 5 times in 30 seconds, the OS throttles it to ~0.1 scans/second. This destroys discovery latency.

**Mitigation strategy:**
1. Run BLE scan inside a ForegroundService. Foreground services are exempt from scan throttling.
2. Use a single long-running scan session rather than repeated start/stop.
3. Use scan filter to reduce CPU load:

```kotlin
val scanFilter = ScanFilter.Builder()
    .setServiceUuid(ParcelUuid(IRIS_SERVICE_UUID))
    .build()

val scanSettings = ScanSettings.Builder()
    .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)  // foreground
    // SCAN_MODE_LOW_POWER for background/battery saving
    .setCallbackType(ScanSettings.CALLBACK_TYPE_ALL_MATCHES)
    .setMatchMode(ScanSettings.MATCH_MODE_AGGRESSIVE)
    .build()

bleScanner.startScan(listOf(scanFilter), scanSettings, scanCallback)
```

### BLE Advertising

Android supports up to 3 simultaneous advertisers (hardware limit on most chipsets). IRIS uses:
- Advertiser 1: IRIS discovery (service UUID + node ID prefix)
- Advertiser 2: reserved for emergency broadcast advertisement

```kotlin
val advertiseSettings = AdvertiseSettings.Builder()
    .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_LATENCY)
    .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM)
    .setConnectable(true)
    .build()

val advertiseData = AdvertiseData.Builder()
    .addServiceUuid(ParcelUuid(IRIS_SERVICE_UUID))
    .addManufacturerData(IRIS_MANUFACTURER_ID, nodeIdBytes.take(4).toByteArray())
    .setIncludeDeviceName(false)  // device name leaks identity
    .build()

bleAdvertiser.startAdvertising(advertiseSettings, advertiseData, advertiseCallback)
```

### GATT Server (Peripheral Role)

IRIS runs a GATT server to receive messages from connected devices:

```kotlin
val gattServer: BluetoothGattServer = bluetoothManager.openGattServer(context, gattServerCallback)

val irisService = BluetoothGattService(
    IRIS_SERVICE_UUID,
    BluetoothGattService.SERVICE_TYPE_PRIMARY
)

// Write characteristic: remote device writes message payload here
val messageWriteChar = BluetoothGattCharacteristic(
    IRIS_MESSAGE_WRITE_UUID,
    BluetoothGattCharacteristic.PROPERTY_WRITE or BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE,
    BluetoothGattCharacteristic.PERMISSION_WRITE
)

// Notify characteristic: IRIS notifies remote device of incoming messages
val messageNotifyChar = BluetoothGattCharacteristic(
    IRIS_MESSAGE_NOTIFY_UUID,
    BluetoothGattCharacteristic.PROPERTY_NOTIFY or BluetoothGattCharacteristic.PROPERTY_READ,
    BluetoothGattCharacteristic.PERMISSION_READ
)
```

## Wi-Fi Aware

Available Android 8.0+. Check availability before use:

```kotlin
val wifiAwareAvailable = packageManager.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE)
val wifiAwareManager: WifiAwareManager? =
    if (wifiAwareAvailable) getSystemService(WIFI_AWARE_SERVICE) as WifiAwareManager else null
```

Wi-Fi Aware uses Discovery Windows (DW) — synchronized wake periods. IRIS publishes a service and subscribes to peer services in the same DW. Typical DW interval: 512ms (active) to 4096ms (low power).

## Wi-Fi Direct

WifiP2pManager for peer-to-peer Wi-Fi. Used for bulk message transfer when large payload exceeds BLE bandwidth budget.

```kotlin
val wifiP2pManager: WifiP2pManager = getSystemService(WIFI_P2P_SERVICE) as WifiP2pManager
val channel: WifiP2pManager.Channel = wifiP2pManager.initialize(context, mainLooper, null)
```

## Foreground Services

### IrisBleService

Type: `ForegroundServiceType.CONNECTED_DEVICE` (Android 10+)  
Android 14 requirement: `FOREGROUND_SERVICE_CONNECTED_DEVICE` permission in manifest.

The service:
1. Starts BLE scan (avoids throttle)
2. Runs GATT server (receives messages)
3. Manages active GATT connections (sends messages)
4. Relays events to `TransportManager` in Rust core via JNI

Notification: persistent notification with connectivity status. Android requires foreground services to show a notification.

### Background Continuity

When foreground service is stopped (user kills app, OS reclaims):
1. WorkManager schedules a periodic `IrisBackgroundSyncWorker`
2. Worker runs for max 10 minutes per invocation
3. Performs scan, deliver queued messages, exits
4. Next invocation scheduled for 15 minutes later

During full Doze mode, WorkManager jobs are deferred. Request battery optimization exclusion:
```kotlin
val intent = Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
intent.data = Uri.parse("package:${packageName}")
startActivity(intent)
```

## OEM Battery Kill Policies

India's primary Android market includes OEMs with aggressive battery management that kills background apps beyond standard Android Doze:

| OEM | Kill Mechanism | Mitigation |
|-----|---------------|-----------|
| Xiaomi (MIUI) | Autostart Manager | Guide user to enable Autostart |
| Realme | Background App Management | Guide user to disable background restriction |
| Vivo | i-Manager / Background Power | Guide user to enable background activity |
| Oppo | Security Manager | Guide user to disable energy saver for IRIS |
| Samsung | Battery > Background usage limits | Guide user to set to Unrestricted |

IRIS must detect these OEMs and display OEM-specific setup guidance after install. Use AutoStart library or detect OEM and show manual steps.

## JNI and UniFFI Integration

UniFFI generates `IrisCore.kt` from `iris.udl`. The compiled Rust library is included in the APK:

```
android/app/src/main/jniLibs/
├── arm64-v8a/
│   └── libiriscode.so
├── armeabi-v7a/
│   └── libiriscode.so
└── x86_64/
    └── libiriscode.so
```

Build with `cargo-ndk`:
```bash
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o android/app/src/main/jniLibs build --release
```

Generated Kotlin is committed to the repo (not regenerated at build time) to keep Kotlin compile independent of Rust toolchain on CI.

## Security

### Android Keystore

Key generation with hardware backing (Android 9+ StrongBox, Android 6+ TEE):

```kotlin
val keyPairGenerator = KeyPairGenerator.getInstance(
    KeyProperties.KEY_ALGORITHM_EC,
    "AndroidKeyStore"
)
keyPairGenerator.initialize(
    KeyGenParameterSpec.Builder(
        IRIS_KEY_ALIAS,
        KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
    )
    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
    .setDigests(KeyProperties.DIGEST_SHA256)
    .setIsStrongBoxBacked(true)  // StrongBox if available
    .build()
)
val keyPair = keyPairGenerator.generateKeyPair()
```

Note: Ed25519 is not directly supported in Android Keystore (uses ECDSA P-256 for hardware-backed signing). IRIS identity key is hardware-backed ECDSA; session keys for encryption are software Ed25519/X25519 in Rust core.

### Message Storage

Room database with SQLCipher:

```kotlin
Room.databaseBuilder(context, IrisDatabase::class.java, "iris.db")
    .openHelperFactory(SupportFactory(passphrase))
    .build()
```

Passphrase derived from a key stored in Android Keystore.

## Testing

- Unit tests: JUnit 5 + MockK for ViewModel and UseCase layers
- Integration: Hilt testing with in-memory Room, fake Rust FFI (test double)
- BLE: physical device testing only (cannot mock BluetoothAdapter reliably)
- CI: unit + integration run on CI; BLE tests require device farm
