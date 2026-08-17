# Wi-Fi Aware (NAN) Transport

## Overview

Wi-Fi Aware, also known as Neighbor Awareness Networking (NAN), is an IEEE 802.11 protocol that enables devices to discover each other and establish data paths without a traditional access point. It was designed specifically for proximity-based services and fills the gap between BLE (low bandwidth, short range) and Wi-Fi Direct (high bandwidth, slow setup). For IRIS, Wi-Fi Aware is the preferred medium-range transport on Android where it is available.

---

## Wi-Fi Aware Technical Foundation

### NAN Cluster and Discovery Windows

Wi-Fi Aware devices organize into NAN Clusters. Within a cluster, devices periodically wake their radios at synchronized **Discovery Windows (DWs)** — by default every 512 TUs (Timing Units), approximately 524 ms. During DWs, devices exchange Service Discovery Frames (SDF) on channel 6 (2.4 GHz) or channels 44 and 149 (5 GHz).

```
    Device A: ──[DW]────────────────────────[DW]────────────────────[DW]──
    Device B: ──[DW]────────────────────────[DW]────────────────────[DW]──
                 ↑ exchange SDFs               ↑ exchange SDFs
               Radio sleeps between DWs
```

This sleep pattern is what makes Wi-Fi Aware power-efficient — the radio is only active for ~2–6 ms per DW vs continuously active in Wi-Fi infrastructure mode.

### Publish/Subscribe Discovery

Wi-Fi Aware uses an active publish/subscribe model distinct from BLE advertising:

- **Publisher:** broadcasts service name and service-specific information in SDFs.
- **Subscriber:** searches for a specific service name.
- **Match:** when a subscriber receives an SDF matching its subscribed service, a match event fires.
- **Follow-up:** after a match, either party can send a small follow-up message (up to ~255 bytes) before establishing a data path.

Discovery types:
- **Solicited publish / active subscribe:** subscriber sends queries; publisher responds.
- **Unsolicited publish / passive subscribe:** publisher sends beacons; subscriber listens.

IRIS uses **unsolicited publish + passive subscribe** (default, lower battery) and **solicited publish + active subscribe** in emergency mode (faster discovery).

### Data Paths

After service discovery, Wi-Fi Aware can establish a **NDP (NAN Data Path)** — a direct Wi-Fi link with up to 300 Mbps throughput. The data path is represented as a standard Linux network interface, so standard socket APIs (TCP, UDP, QUIC) work over it.

```
Sequence: NAN Discovery → NDP Setup → Standard Networking
─────────────────────────────────────────────────────────
Node A (publisher) ←── subscribe match ── Node B (subscriber)
Node A ←── follow-up msg (request NDP) ── Node B
Node A ──── NDP response ─────────────── Node B
Node A ←──── NDP network interface ────── Node B
           (192.168.0.x / link-local)
Node A and B now communicate via TCP/UDP/QUIC on the NDP interface
```

---

## Android Wi-Fi Aware API

### API Level Requirements

- **Wi-Fi Aware basics:** Android 8.0 (API 26)
- **Background discovery without scanning:** Android 10.0 (API 29) — *historically;
  FGS `connectedDevice` (API 34+) required for production background under
  Android 12+/15/16 (RES-0020 R5, corrected 2026-08-16)*
- **NDP over IPv6 link-local:** Android 10.0+
- **Ranging integration:** Android 9.0 (API 28) with WifiRttManager (802.11mc FTM;
  802.11az NTB API 35+)

### Feature Detection

```kotlin
// Check hardware support
val packageManager = context.packageManager
if (!packageManager.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE)) {
    Log.w("IRIS", "Wi-Fi Aware not supported on this device")
    return
}

// Check runtime availability
val wifiAwareManager = context.getSystemService(WifiAwareManager::class.java)
if (wifiAwareManager == null || !wifiAwareManager.isAvailable) {
    // Wi-Fi Aware available in hardware but currently unavailable
    // (e.g., Wi-Fi turned off, or user disabled NAN)
    return
}
```

Hardware support is present on most devices with Qualcomm Snapdragon 845+ or Exynos 9810+. Specifically:
- Snapdragon 845/855/865/888/8 Gen 1/2/3: supported
- Exynos 9810/9820/2100: supported  
- MediaTek Dimensity 1000+: varies by OEM
- Budget chipsets (Helio G series, Snapdragon 6xx): often NOT supported

Check via `adb shell cmd wifi status` for `Wifi Aware state: enabled`.

### Session Initialization

```kotlin
val attachCallback = object : AttachCallback() {
    override fun onAttached(session: WifiAwareSession) {
        this@IrisWifiAwareTransport.session = session
        startPublishing(session)
        startSubscribing(session)
    }
    override fun onAttachFailed() {
        handleAttachFailure()
        scheduleRetry()
    }
}

// Identity change callback — fires when NAN cluster changes
val identityChangedListener = object : IdentityChangedListener() {
    override fun onIdentityChanged(mac: ByteArray) {
        // NAN MAC address changed (privacy rotation) — update peer mapping
        updateNanMacMapping(mac)
    }
}

wifiAwareManager.attach(attachCallback, identityChangedListener, handler)
```

### Publishing (Advertising Presence)

```kotlin
fun startPublishing(session: WifiAwareSession) {
    // Service-specific information: IRIS node advertisement
    val serviceSpecificInfo = IrisAdvertisement(
        publicKeyFingerprint = identity.publicKeyFingerprint,
        nodeCapabilities = capabilities.toByteArray(),
        sequenceNumber = advertisementSeqNo.incrementAndGet()
    ).encode()  // Must fit in ~1024 bytes

    val publishConfig = PublishConfig.Builder()
        .setServiceName("com.iris.mesh.v1")    // service name for matching
        .setServiceSpecificInfo(serviceSpecificInfo)
        .setPublishType(PublishConfig.PUBLISH_TYPE_UNSOLICITED)  // beacon mode
        .setTerminateNotificationEnabled(true)
        .build()

    val publishCallback = object : DiscoverySessionCallback() {
        override fun onPublishStarted(session: PublishDiscoverySession) {
            publishSession = session
        }
        override fun onMessageReceived(peerHandle: PeerHandle, message: ByteArray) {
            // Follow-up message from subscriber (NDP request or small message)
            handleFollowUpMessage(peerHandle, message)
        }
        override fun onSessionTerminated() {
            restartPublishing()
        }
    }

    session.publish(publishConfig, publishCallback, handler)
}
```

### Subscribing (Discovering Peers)

```kotlin
fun startSubscribing(session: WifiAwareSession) {
    val subscribeConfig = SubscribeConfig.Builder()
        .setServiceName("com.iris.mesh.v1")
        .setSubscribeType(SubscribeConfig.SUBSCRIBE_TYPE_PASSIVE)
        .build()

    val subscribeCallback = object : DiscoverySessionCallback() {
        override fun onSubscribeStarted(session: SubscribeDiscoverySession) {
            subscribeSession = session
        }
        override fun onServiceDiscovered(
            peerHandle: PeerHandle,
            serviceSpecificInfo: ByteArray,
            matchFilter: List<ByteArray>
        ) {
            val peerAdvert = IrisAdvertisement.decode(serviceSpecificInfo) ?: return
            // New IRIS peer discovered!
            peerDiscovered(peerHandle, peerAdvert)
            // Optionally send a follow-up message to exchange additional info
            subscribeSession?.sendMessage(peerHandle, 0, irisHandshakeBytes)
        }
        override fun onServiceDiscoveredWithinRange(
            peerHandle: PeerHandle, serviceSpecificInfo: ByteArray,
            matchFilter: List<ByteArray>, distanceMm: Int
        ) {
            // Available on API 28+ with ranging enabled
            val distanceMeters = distanceMm / 1000.0
            peerDiscoveredWithRange(peerHandle, serviceSpecificInfo, distanceMeters)
        }
    }

    session.subscribe(subscribeConfig, subscribeCallback, handler)
}
```

### Establishing a NAN Data Path (NDP)

For data transfers, IRIS establishes an NDP:

```kotlin
// Requester side (subscriber)
fun requestDataPath(peerHandle: PeerHandle) {
    val networkSpecifier = WifiAwareNetworkSpecifier.Builder(subscribeSession, peerHandle)
        .setPskPassphrase(generateSessionPassphrase())
        .build()

    val networkRequest = NetworkRequest.Builder()
        .addTransportType(NetworkCapabilities.TRANSPORT_WIFI_AWARE)
        .setNetworkSpecifier(networkSpecifier)
        .build()

    val networkCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            val socket = network.socketFactory.createSocket()
            // Use this socket for data transfer — standard TCP
            handleNdpConnection(network, socket)
        }
        override fun onLinkPropertiesChanged(network: Network, lp: LinkProperties) {
            // Get the link-local IPv6 address assigned to this NDP interface
            val ndpAddress = lp.linkAddresses.firstOrNull()?.address
        }
        override fun onLost(network: Network) {
            handleNdpLoss()
        }
    }

    connectivityManager.requestNetwork(networkRequest, networkCallback)
}

// Responder side (publisher) — mirror pattern with WifiAwareNetworkSpecifier builder for publisher
fun acceptDataPath(peerHandle: PeerHandle) {
    val networkSpecifier = WifiAwareNetworkSpecifier.Builder(publishSession, peerHandle)
        .setPskPassphrase(sessionPassphrase)  // must match requester's
        .build()
    // Same NetworkRequest/NetworkCallback pattern
}
```

---

## Range and Environment Characteristics

| Environment          | Expected Range | Notes                                      |
|----------------------|----------------|--------------------------------------------|
| Open field (LoS)     | 100–300 m      | 5 GHz NDP: ~150 m; 2.4 GHz: ~250 m       |
| Urban outdoor        | 30–80 m        | Buildings attenuate 2.4 GHz significantly  |
| Indoor open plan     | 20–50 m        | Office or school hall                      |
| Indoor with walls    | 10–30 m        | Concrete reduces to BLE-like range         |
| Ahmedabad urban dense| 20–60 m        | Typical apartment density with mixed walls |

Discovery range is typically greater than NDP data path range due to different power levels.

---

## Background Operation

Wi-Fi Aware has the best background operation story of any P2P Wi-Fi technology on Android:

- **Android 8–9:** Wi-Fi Aware requires foreground or foreground service for reliable operation.
- **Android 10+ (historical):** Wi-Fi Aware could maintain discovery (publish/subscribe) even when the app is not in foreground, without a foreground service, as long as the session is active.
- **Android 12+ / 15 / 16 (corrected 2026-08-16, RES-0020 R5):** production background
  discovery is **FGS-gated**. The old "API 10+ discovery survives without FGS"
  claim is **dated** — under Android 12+ background limits and the Android 15/16
  FGS runtime-quota tightening, a **foreground service of type `connectedDevice`
  (API 34+)** is required to keep publish/subscribe legal and reliable in
  background. **Suspend/Resume** (API 34+, HAL-gated) is the recognized power
  lever for background cadence. The WIFIAWARE-001 design (AC-10) pins FGS
  `connectedDevice` as the production requirement.
- **Doze mode:** NAN cluster synchronization survives Doze Mode light. In Doze full, the radio duty cycle reduces but the session is maintained. Discovery events wake the app via `AlarmManager.setExactAndAllowWhileIdle()` internally.

IRIS leverages this by keeping a Wi-Fi Aware session active at all times on supported devices, using it for passive background discovery. When a match event fires, the app is woken to handle it. On devices where FGS cannot run or Wi-Fi Aware is unavailable, IRIS degrades to BLE + Wi-Fi Direct.

```kotlin
// Register for Wi-Fi Aware availability changes
wifiAwareManager.registerCallback(object : WifiAwareManager.WifiAwareStateChangedCallback() {
    override fun onAvailabilityChanged(isAvailable: Boolean) {
        if (isAvailable) {
            reattachSession()
        } else {
            handleAwareUnavailable()
        }
    }
}, handler)
```

---

## Power Consumption

| Mode                           | Additional Draw |
|--------------------------------|-----------------|
| NAN cluster joined, no traffic | 5–15 mA         |
| Active subscribe (solicited)   | 15–25 mA        |
| NDP established, idle          | 15–30 mA        |
| NDP active TX (~10 Mbps)       | 80–150 mA       |

The per-DW wake cycle of NAN cluster membership is significantly more efficient than continuously active Wi-Fi:

- Infrastructure Wi-Fi idle: ~30–50 mA
- NAN cluster idle: ~10–15 mA
- BLE scan idle: ~8 mA

Wi-Fi Aware is approximately 2–3× more power-efficient than infrastructure Wi-Fi while providing much greater range and bandwidth than BLE.

**DW interval tuning:** Android exposes limited control over DW intervals. IRIS can hint via `WifiAwareManager.characteristics.supportedDataPathSecurityTypes` and similar queries. For emergency mode, the OS defaults are acceptable (524 ms DW interval); reducing further is not exposed to apps.

---

## iOS: Wi-Fi Aware Not Available (v1) — CORRECTED 2026-08-16 (CONFLICT-1, RES-0020 R9)

**iOS has NO public Wi-Fi Aware / NAN API** for the entire Android-era window
(2018–2025). Apple uses 802.11 NAN internally for certain proximity features
(likely AirDrop's Wi-Fi component) but exposed no public API for NAN discovery
or data paths.

**UPDATE (2026, CONFLICT-1): Apple introduced the `WiFiAware` framework with
iOS 26 / iPhone 12 and later** — a public Wi-Fi Aware (NAN) API on iOS
(2.4/5/6 GHz, measured ~15 MB/s aggregate). This changes the iOS capability row
from "No" to "Yes (iOS 26+, iPhone 12+, un-previewed)". It does NOT change the
v1 IRIS plan:

- IRIS v1 keeps **Android-only Wi-Fi Aware** as the medium-range high-bandwidth
  transport on Android.
- **BLE-002 remains the v1 iOS P2P path** (BLE is universal, background-safe,
  and cross-platform).
- **Android ↔ Apple NDP interoperability is immature** (Android NAN vs iOS
  WiFiAware vendor/ecosystem fragmentation; cross-vendor NDP needs real-device
  validation, which is BLK-0005-gated). Recording this as a known limitation and
  a roadmap item for the platform nodes (ANDROID-001/IOS-001), not a v1
  transport feature.
- IRIS identifies peers by public-key fingerprint, never NAN MAC/PeerHandle, so
  cross-OS peer identity remains consistent regardless of transport.

Mitigation (unchanged for v1):
- iOS devices rely on BLE for discovery and point-to-point transfers.
- When an iOS device is near an Android device with Wi-Fi Aware active, the
  Android device can see the iOS device via BLE and the iOS device can connect
  to the Android's NAN data path via infrastructure Wi-Fi (if the NDP interface
  is bridged) — but this is complex and not currently implemented.
- **Primary mitigation:** iOS devices in IRIS mesh use BLE as their P2P
  transport layer. Wi-Fi Aware is an Android-only enhancement in v1.

---

## Hardware Requirements

Wi-Fi Aware requires specific hardware chipset features:

```
Required chipset capabilities:
- NAN (Neighbor Awareness Networking) hardware support
- NAN beacon transmission / reception
- NDP (NAN Data Path) for data transfer

Supported chipsets (as of 2024):
- Qualcomm QCA6390, QCA6490, WCN3998, WCN6855 and newer
- Realtek RTL8822CE (partial — discovery only, no NDP on some OEMs)
- MediaTek MT7921 (mostly desktop/laptop)

Devices confirmed to support full Wi-Fi Aware (discovery + NDP):
- Google Pixel 3 and newer
- Samsung Galaxy S9 and newer
- OnePlus 6T and newer  
- Most flagship-tier Android phones 2018+

Devices that may NOT support:
- Budget Android phones with Snapdragon 4xx/6xx series
- Some MediaTek-based mid-range phones
- Older devices (pre-2018)
```

IRIS must gracefully degrade: on devices without Wi-Fi Aware, fall back to BLE (for discovery and small messages) and Wi-Fi Direct (for large transfers if needed).

Runtime check:
```kotlin
fun wifiAwareAvailable(context: Context): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return false
    val pm = context.packageManager
    return pm.hasSystemFeature(PackageManager.FEATURE_WIFI_AWARE)
}
```

---

## Regulatory Considerations

Wi-Fi Aware operates on standard IEEE 802.11 channels:
- **2.4 GHz (channel 6):** License-exempt ISM band in India. Max EIRP: 100 mW (20 dBm) per WPC regulations.
- **5 GHz:** U-NII bands. India permits 5150–5250 MHz (indoor only) and 5725–5875 MHz.

No special license or regulatory approval is required to operate Wi-Fi Aware in India. It is standard Wi-Fi radio usage. IRIS does not increase device transmit power above manufacturer defaults.

---

## Use Cases in IRIS

Wi-Fi Aware is the preferred transport for:

1. **Medium-range peer discovery:** Better range than BLE, faster discovery than Wi-Fi Direct.
2. **Message bundles 1–10 MB:** Large enough to warrant Wi-Fi bandwidth, small enough to complete in one NDP session.
3. **Background node presence detection on Android:** The NAN cluster membership survives background → foreground transitions.
4. **Ranging for proximity-aware routing:** Wi-Fi Aware RTT (when available) can provide ~10 cm ranging accuracy, useful for routing decisions.

Wi-Fi Aware is NOT used for:
- **iOS devices** — not available.
- **Sub-1-KB emergency messages** — BLE latency is acceptable and has better background support.
- **10+ MB transfers** — NDP overhead is justified; use Wi-Fi Direct for sustained high-throughput bulk transfer.
- **Devices without Wi-Fi Aware hardware** — fall back to BLE + Wi-Fi Direct.
