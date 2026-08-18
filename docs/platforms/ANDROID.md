# IRIS Android Platform Documentation

## Platform Targets

**Pinned at scaffold (ANDROID-001, iter 116; RES-0022):**
- **Minimum SDK**: 26 (Android 8.0 Oreo ≈ 95%+ device coverage)
- **Target SDK**: 34 (Android 14) — **do not bump to 37+ without the
  `ACCESS_LOCAL_NETWORK` runtime-permission work (see §ACCESS_LOCAL_NETWORK
  SDK-37 cliff below)**
- **Build Tools**: 34.0.0
- **AGP**: 8.7.3
- **Kotlin**: 2.2.10
- **Gradle wrapper**: 8.9
- **Compose BOM**: 2024.12.01 (strong-skipping compiler default ON)
- **KSP**: 2.2.10-2.0.2; **Hilt**: 2.55
- **Rust NDK**: cargo-ndk 4.1.2 + NDK r26 floor / SDK r28.x current — pin exact
  NDK via `ANDROID_NDK_HOME` at scaffold (G-AND-5); ABI set {arm64-v8a
  (mandatory), armeabi-v7a, x86_64 (emulator)}; i686 dropped
- **UniFFI**: 0.31.2 (generator) / uniffi_bindgen 0.31.2 — see §UniFFI Integration

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
Rust FFI:          UniFFI-generated iriscode.kt + libiriscode.so (package facade `iriscode`)
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

**Implemented package layout (ANDROID-001, iter 116)** — `android/app/src/main/kotlin/iriscore/`:
```
├── di/          # Hilt modules (AdapterModule, IrisCoreModule, DispatcherModule)
├── adapter/     # AndroidBleTransportAdapter, AndroidWifiAwareTransportAdapter,
│                #   AndroidWifiDirectTransportAdapter + AdapterLifecycle.kt primitives
├── data/        # MeshRepository (FfiInboxListener → StateFlow), RelayOutbox
├── service/     # IrisBleService (FGS connectedDevice), BleScanSession, BleScanReceiver
├── worker/      # WorkScheduler, IrisBackgroundSyncWorker (@HiltWorker), MeshSyncPolicy
├── identity/    # KeystoreEd25519, X25519StaticAd
├── util/        # BatteryOptimizationGuidance (OEM matrix), PeerIdCodec
└── ui/          # MainActivity, MeshViewModel, HomeScreen, MeshUiState, IrisTheme
```

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

<!-- Android 13+ Wi-Fi Aware/Direct nearby-devices group (neverForLocation) -->
<uses-permission android:name="android.permission.NEARBY_WIFI_DEVICES"
    android:usesPermissionFlags="neverForLocation" />

<!-- Android 10/11: location required for BLE scan -->
<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" />

<!-- Foreground service permissions -->
<uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
<uses-permission android:name="android.permission.FOREGROUND_SERVICE_CONNECTED_DEVICE" />

<!-- Battery optimization exclusion -->
<uses-permission android:name="android.permission.REQUEST_IGNORE_BATTERY_OPTIMIZATIONS" />
```

**targetSdk 34 (v1):** with targetSdk ≤ 36, `INTERNET` implicitly grants the
local-network permission (temporary bridge) — see §ACCESS_LOCAL_NETWORK SDK-37
cliff.

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

**Implemented scan session (AC-7, iter 116):** `BleScanSession` runs a
**`PendingIntent`-based** scan (`PendingIntent.getBroadcast` + `BleScanReceiver`
manifest receiver, `SCAN_MODE_OPPORTUNISTIC`, `IRIS_SERVICE_UUID` filter) and
surfaces results via a `BleScanEvents` flow. `ScanRestartPolicy` enforces a
**5 s floor after every restart, backing off to 30 s during empty discovery
windows** so background scans stay inside Android's 5-starts/30-s throttle
envelope while discovery remains latency-bounded.

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
| Honor | Battery > background app management | Guide user to keep IRIS unrestricted |
| Huawei | App launch > background activity | Guide user to allow auto/manual launch |
| Google/Pixel | Doze + App Standby buckets | `ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS`; FGS grant |

Implemented as `BatteryOptimizationGuidance` (AC-7, iter 116): OEM matrix above
drives install-time/run-time setup guidance and the
`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` request (with `MeshSyncPolicy`
deferring background sync while battery is critical).

IRIS must detect these OEMs and display OEM-specific setup guidance after install. Use AutoStart library or detect OEM and show manual steps.

## UniFFI Integration

IRIS binds the Rust `iris-core` engine to Kotlin with **UniFFI 0.31.2**
(`uniffi_bindgen 0.31.2`). Foreign-trait callback interfaces are preferred over
the soft-deprecated UDL "callback interfaces": the three transport adapter
traits (`BleAdapter` 10-op sync; `WifiAwareAdapter` 12-op async;
`WifiDirectAdapter` 20-op async) are exported from `crates/iris-android` via
`#[uniffi::export(foreign)]` + `#[async_trait]` — UniFFI translates async trait
methods to Kotlin `suspend` functions carried back through the foreign-future /
oneshot mechanism. Async adapter calls are driven not by
`#[uniffi::export(async_runtime="tokio")]` (ineffective on exported-trait impls,
UniFFI issue #2576) but by an **explicit tokio runtime handle** owned by
`IrisEngine`, polled via `runtime.block_on` (proven by the G-AND-3 spike).

**Generated Kotlin is committed** to the repo (not regenerated at build time) so
Kotlin compiles stay independent of the Rust/NDK toolchain on CI:
```
kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt   # generated IrisEngine + 3 foreign-trait interfaces + IrisFfiException
kotlin/src/main/kotlin/iriscode/api.kt                        # NEW (AC-11): package iriscode facade re-exporting uniffi.iriscode
```
The app `sourceSets` mount `kotlin/src/main/kotlin` as a source root; adapter and
shell code imports the single stable namespace `import iriscode.*`.

The compiled native library is included in the APK:
```
android/app/src/main/jniLibs/
├── arm64-v8a/
│   └── libiriscode.so
├── armeabi-v7a/
│   └── libiriscode.so
└── x86_64/
    └── libiriscode.so
```

Build with cargo-ndk 4.1.2 (NDK pinned via `ANDROID_NDK_HOME`, r26 floor / SDK
r28.x current):
```bash
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o android/app/src/main/jniLibs build --release --manifest-path crates/iris-android/Cargo.toml
```
Gradle integration: `org.mozilla.rust-android-gradle.rust-android` 0.9.6 (Gradle
8.x / AGP 8.7.3 pinned at scaffold) or Mullvad fork `net.mullvad.rust-android`
0.10.1 if Gradle 9+ is adopted.
**Env-gated (recorded limitation, not blocker):** the `cargo-ndk` build (AC-1)
and `gradlew assembleDebug` (AC-6) run on a toolchain-equipped host/CI — the
dev host has no Android SDK/NDK/gradle/kotlinc.

## Security

### Android Keystore — Ed25519 identity (AC-8, iter 116)

**Supersedes the dated note below** (which claimed Ed25519 was not in Keystore
and proposed ECDSA P-256 conversion — REJECTED per RES-0022 Q4 / IDENT-001
RED-0005): Android Keystore natively supports **Ed25519/X25519 hardware-backed
since Android 13** (`KeyGenParameterSpec` Curve25519 via KeyMint v2 HAL, TEE).
StrongBox's algorithm subset **excludes Ed25519** (RSA/AES/ECDSA P-256/ECDH
P-256/HMAC-SHA256/3DES only) and StrongBox-Ed25519 attestation has a known CTS
failure — so the IRIS identity deliberately uses **TEE Ed25519**.

`KeystoreEd25519` (implemented, `android/app/.../iriscore/identity/`):
- **AutoBackend**: `AndroidKeyStore` TEE Ed25519 (`KeyProperties.KEY_ALGORITHM_ED25519`,
  API 33 floor, `setIsStrongBoxBacked(false)`) — runtime fallback to a JCA
  `SoftwareBackend` on older/insecure devices; API < 26 entirely software.
- 32-byte raw public key = the 64-hex-char IRIS PeerId; identity sign/verify
  keyed to `IDENT-001` RED-0005 and surfaceable through UniFFI.
- `X25519StaticAd`: the static advertisement is **Ed25519-signed-X25519**
  (RED-0005 binding: identity key signs the X25519 static key used in
  pre-handshake advertisements) — no P-256 anywhere in the identity path.
- Session keys stay in-engine (Rust Ed25519/X25519 in `iris-core`; CRYPTO-001);
  only the long-lived identity/static keys are Keystore-anchored.

### Message Storage

Room database with SQLCipher:

```kotlin
Room.databaseBuilder(context, IrisDatabase::class.java, "iris.db")
    .openHelperFactory(SupportFactory(passphrase))
    .build()
```

Passphrase derived from a key stored in Android Keystore.

### ACCESS_LOCAL_NETWORK — SDK-37 cliff (RES-0022 Q5)

Android 17 adds `ACCESS_LOCAL_NETWORK`, a **runtime** permission (NEARBY_DEVICES
group, no re-prompt if a sibling is granted) mandatory for apps **targeting API
37+**. It gates **outgoing TCP, incoming TCP, UDP unicast/multicast/broadcast —
i.e. the entire IRIS data plane** (TCP-over-GO, NDP IPv6 sockets, NAN UDP) plus
mDNS/DNS-SD.

- **v1 stays targetSdk 34**: at targetSdk ≤ 36, `INTERNET` implicitly grants
  local-network access (temporary bridge) → **no LNP needed for v1**.
- **Before any bump to 37+**: declare + runtime-request `ACCESS_LOCAL_NETWORK`;
  `android.net.nsd` system-mediated `DiscoveryRequest` picker + `NsdManager` IPs
  are exempt from the permission; raw in-process sockets are NOT.
- **Android 17 BLE carry-forward**: `BluetoothSocket.read()` (RFCOMM) returns
  **-1 on close/drop** for targetSdk 37 — Kotlin read loops must check -1
  explicitly.

## Testing

- Unit tests: JUnit 5 + MockK for ViewModel and UseCase layers
- Integration: Hilt testing with in-memory Room, fake Rust FFI (test double)
- BLE: physical device testing only (cannot mock BluetoothAdapter reliably)
- CI: unit + integration run on CI; BLE tests require device farm
