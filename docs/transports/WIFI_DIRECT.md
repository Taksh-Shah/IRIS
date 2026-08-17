# Wi-Fi Direct Transport

## Overview

Wi-Fi Direct (also called Wi-Fi P2P, standardized by Wi-Fi Alliance) allows devices to form a direct Wi-Fi connection without an access point. In IRIS, Wi-Fi Direct serves as a **high-bandwidth data transfer channel** between nearby devices — typically triggered after BLE discovery identifies a peer that needs to exchange a large payload (images, audio clips, large message bundles). Wi-Fi Direct is not used as the primary discovery mechanism due to its slow group formation and high battery impact.

---

## Wi-Fi Direct Capabilities

| Parameter              | Specification               | Practical Observed        |
|------------------------|-----------------------------|---------------------------|
| Range                  | Up to 200 m (line of sight) | 30–100 m typical          |
| Bandwidth              | Up to 250 Mbps (802.11n)    | 10–80 Mbps practical      |
| Connection setup time  | 2–15 seconds                | 3–8 seconds typical       |
| Latency (after setup)  | 1–5 ms                      | 2–10 ms                   |
| Frequency bands        | 2.4 GHz and 5 GHz           | Device-dependent          |
| Concurrent connections | 1 GO + up to 8 clients      | Usually 1 GO + 3–5 clients|

> **Correction (RES-0021, iter 104):** "1 GO + 8 clients" is a **vendor/HAL ceiling**, not a Wi-Fi Alliance guarantee (the P2P spec mandates 1:1; one-to-many is optional). IRIS uses an **N-client admission model** (bounded GO-side connection table, admitted client queue) and never hard-codes 8.
| Battery impact         | High (Wi-Fi radio active)   | 50–200 mA additional      |

Wi-Fi Direct uses standard 802.11 protocols but operates in infrastructure mode with a software-defined access point role (Group Owner). Encryption is WPA2-Personal (WPA2-PSK/AES-CCMP), negotiated during group formation.

> **Correction (RES-0021, iter 104):** Group security floor = **WPA2-Personal**. **WPA3-SAE is now supported on Wi-Fi Direct R2-capable devices** (`WifiP2pGroup` security types `SECURITY_TYPE_WPA3_SAE`/`WPA3_COMPATIBILITY`, Android API 36) — IRIS capability-gates SAE to a later phase (`isWiFiDirectR2Supported`/`isPccModeSupported`). **WPS-PIN is prohibited** (WPS deprecated client-mode API 28; CVE-2021-0326 P2P RCE via WPS-PIN); the group passphrase is pushed over the **authenticated BLE control plane**; PBC is a legacy dev-doc fallback only.

---

## Group Owner (GO) and Client Roles

Wi-Fi Direct networks have a mandatory asymmetry: one device is the **Group Owner (GO)** and all others are **clients**.

```
                    ┌─────────────────────────────┐
                    │         Group Owner         │
                    │  (acts as soft-AP)          │
                    │  IP: 192.168.49.1           │
                    └──────┬──────────────┬───────┘
                           │              │
               ┌───────────▼──┐      ┌────▼────────────┐
               │   Client A   │      │    Client B     │
               │192.168.49.x  │      │ 192.168.49.y   │
               └──────────────┘      └─────────────────┘
```

Key properties of the GO role:
- The GO provides DHCP for clients.
- Clients can communicate with the GO directly.
- Clients **cannot communicate directly with each other** without the GO relaying traffic.
- The GO has higher battery impact (maintains beacon transmission).
- GO election during group formation uses an intent value (0–15); higher intent = more likely to become GO. IRIS sets intent to 7 (neutral) by default, higher (12) when the device is plugged in.

**Implications for IRIS:** When two IRIS devices form a Wi-Fi Direct group, one becomes GO. The GO's IP (192.168.49.1) is always predictable. The client's IP is assigned by GO's DHCP (192.168.49.2–254 range). IRIS establishes a bidirectional TCP or QUIC connection after GO election completes.

---

## Android Wi-Fi Direct API

### Initialization and Channel Setup

```kotlin
val wifiP2pManager: WifiP2pManager = 
    context.getSystemService(Context.WIFI_P2P_SERVICE) as WifiP2pManager
val channel: WifiP2pManager.Channel = 
    wifiP2pManager.initialize(context, mainLooper, channelListener)

// Register for Wi-Fi P2P intents
val intentFilter = IntentFilter().apply {
    addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
    addAction(WifiP2pManager.WIFI_P2P_PEERS_CHANGED_ACTION)
    addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
    addAction(WifiP2pManager.WIFI_P2P_THIS_DEVICE_CHANGED_ACTION)
}
context.registerReceiver(wifiP2pReceiver, intentFilter)
```

### Peer Discovery

```kotlin
wifiP2pManager.discoverPeers(channel, object : WifiP2pManager.ActionListener {
    override fun onSuccess() {
        // Discovery started; wait for WIFI_P2P_PEERS_CHANGED_ACTION broadcast
    }
    override fun onFailure(reason: Int) {
        handleDiscoveryFailure(reason)
        // Reasons: ERROR (0), P2P_UNSUPPORTED (1), BUSY (2)
    }
})

// In BroadcastReceiver when WIFI_P2P_PEERS_CHANGED_ACTION received:
wifiP2pManager.requestPeers(channel) { peerList ->
    val peers = peerList.deviceList
    // Filter IRIS peers by device name or service discovery
}
```

**Service Discovery (Preferred):** Rather than filtering by device name, IRIS uses Wi-Fi Direct Service Discovery (built on DNS-SD/Bonjour over Wi-Fi Direct):

```kotlin
val serviceInfo = WifiP2pDnsSdServiceInfo.newInstance(
    "IRIS",
    "_iris._tcp",
    mapOf(
        "pk" to publicKeyFingerprint,
        "ver" to IRIS_PROTOCOL_VERSION.toString(),
        "caps" to capabilities.toHex()
    )
)
wifiP2pManager.addLocalService(channel, serviceInfo, actionListener)

// Discover IRIS services
wifiP2pManager.setDnsSdResponseListeners(channel,
    { instanceName, registrationType, sourceDevice ->
        // Instance name = "IRIS", this is an IRIS device
    },
    { fullDomainName, record, device ->
        val peerCaps = IrisCapabilities.fromMap(record)
        peerDiscoveredViaWifiDirect(device, peerCaps)
    }
)
wifiP2pManager.discoverServices(channel, actionListener)
```

Service discovery is slower (~5–10 seconds) but more reliable for identifying IRIS peers vs arbitrary Wi-Fi Direct devices.

### Group Formation and Connection

```kotlin
// Connect to a discovered peer
val config = WifiP2pConfig().apply {
    deviceAddress = peerDevice.deviceAddress
    wps.setup = WpsInfo.PBC  // Push Button Configuration (no PIN exchange)
    // GO intent: 0 = prefer to be client, 15 = prefer to be GO
    groupOwnerIntent = if (isPluggedIn()) 12 else 7
}

wifiP2pManager.connect(channel, config, object : WifiP2pManager.ActionListener {
    override fun onSuccess() {
        // Connection initiated; wait for WIFI_P2P_CONNECTION_CHANGED_ACTION
    }
    override fun onFailure(reason: Int) {
        handleConnectionFailure(reason)
    }
})

// In BroadcastReceiver when WIFI_P2P_CONNECTION_CHANGED_ACTION:
val networkInfo = intent.getParcelableExtra<NetworkInfo>(WifiP2pManager.EXTRA_NETWORK_INFO)
if (networkInfo?.isConnected == true) {
    wifiP2pManager.requestConnectionInfo(channel) { info ->
        val groupOwnerAddress = info.groupOwnerAddress
        val isGroupOwner = info.isGroupOwner
        startDataTransfer(groupOwnerAddress, isGroupOwner)
    }
}
```

### Data Transfer

After group formation, IRIS uses a plain TCP socket (or QUIC) for data transfer:

```kotlin
// On Group Owner side: listen for incoming connection
fun startAsServer() {
    serverSocket = ServerSocket(IRIS_WIFI_DIRECT_PORT)
    val clientSocket = serverSocket.accept()
    handleDataChannel(clientSocket.inputStream, clientSocket.outputStream)
}

// On client side: connect to GO
fun startAsClient(groupOwnerAddress: InetAddress) {
    val socket = Socket()
    socket.bind(null)
    socket.connect(InetSocketAddress(groupOwnerAddress, IRIS_WIFI_DIRECT_PORT), 5000)
    handleDataChannel(socket.inputStream, socket.outputStream)
}
```

The data channel uses the IRIS framing protocol (4-byte length prefix + message bytes) to multiplex multiple messages over the single TCP connection.

### Background Restrictions

**Critical limitation:** Wi-Fi Direct discovery and connection management **does not work reliably in background** on Android. The Wi-Fi P2P framework requires active foreground context for reliable operation. Specifically:

- `discoverPeers()` called from background may succeed but the discovery results arrive via `BroadcastReceiver` which may be rate-limited.
- Group formation events (`WIFI_P2P_CONNECTION_CHANGED_ACTION`) may be delayed or dropped in background.
- **Solution:** IRIS triggers Wi-Fi Direct only when the app is in foreground or when running under a foreground service with the device screen on (emergency mode). BLE handles background discovery; Wi-Fi Direct is only activated for active transfers.

**Wakelock requirement:** Wi-Fi P2P operations require the CPU to stay awake. IRIS acquires a `PowerManager.WakeLock` before initiating discovery and holds it until the transfer completes and the group is removed.

```kotlin
val wakeLock = powerManager.newWakeLock(
    PowerManager.PARTIAL_WAKE_LOCK,
    "IRIS:WifiDirectTransfer"
)
wakeLock.acquire(TRANSFER_TIMEOUT_MS)
try {
    performWifiDirectTransfer()
} finally {
    wakeLock.release()
}
```

---

## iOS: Wi-Fi Direct is Not Available

**iOS does not expose Wi-Fi Direct to third-party applications.** Apple uses Wi-Fi Direct internally for AirDrop, AirPlay, and Handoff, but these APIs are private.

**Multipeer Connectivity Framework (MCSession)** is Apple's closest public API:
- Works on iOS, macOS, and tvOS.
- Uses a combination of BLE, Wi-Fi Direct, and infrastructure Wi-Fi under the hood (decided by OS).
- **Apple-to-Apple only:** Multipeer Connectivity uses Bonjour with Apple-specific protocols. An iOS device using MCSession cannot directly communicate with an Android device using Wi-Fi P2P.
- Supports up to 8 peers in a session.
- Provides reliable and unreliable message channels.

**IRIS approach for iOS:** When an iOS device needs high-bandwidth transfer to another iOS device, Multipeer Connectivity is used. When communicating with Android, the iOS device falls back to BLE (lower bandwidth) or relies on infrastructure Wi-Fi if available. This is a known capability gap.

```swift
// iOS Multipeer Connectivity (Apple-to-Apple only)
import MultipeerConnectivity

let myPeerId = MCPeerID(displayName: irisNodeId)
let serviceType = "iris-mesh"  // 1-15 chars, lowercase, digits, hyphens

let session = MCSession(peer: myPeerId, securityIdentity: nil, encryptionPreference: .required)
let advertiser = MCNearbyServiceAdvertiser(peer: myPeerId, discoveryInfo: nil, serviceType: serviceType)
let browser = MCNearbyServiceBrowser(peer: myPeerId, serviceType: serviceType)
```

---

## Coexistence with BLE

The canonical IRIS workflow uses BLE and Wi-Fi Direct in tandem:

```
Phase 1 (BLE): Discovery
─────────────────────────
Node A continuously advertises on BLE.
Node B scans, discovers A, reads node advertisement.
Both nodes know each other's IRIS identity, capabilities, stored message count.

Phase 2 (Wi-Fi Direct): Transfer decision
──────────────────────────────────────────
If pending transfer > 10 KB:
  A sends "Wi-Fi Direct transfer offer" message to B via BLE GATT.
  B accepts, both nodes initiate Wi-Fi Direct group formation.

Phase 3 (Wi-Fi Direct): Transfer
──────────────────────────────────
Group forms in 3–8 seconds.
Messages transferred at 10–80 Mbps.
A sends all queued messages to B; B sends queued messages to A.
Transfer completes.

Phase 4 (BLE): Teardown signal
────────────────────────────────
B sends "transfer complete" message to A via BLE.
Both nodes call wifiP2pManager.removeGroup().
BLE scanning resumes.
```

This workflow means BLE is always the "control plane" and Wi-Fi Direct is the "data plane" for large transfers. IRIS does not attempt to use Wi-Fi Direct as a standalone transport without prior BLE coordination.

---

## Known Issues and Limitations

### Slow Group Formation (2–15 seconds)

Wi-Fi Direct group formation involves:
1. GO negotiation (exchange of intent values via Action Frames): ~200 ms
2. GO sets up soft-AP: ~500 ms–2 s
3. Client scans for GO's AP: ~1–3 s
4. WPA2 handshake: ~500 ms
5. DHCP: ~500 ms–2 s

Total: typically 3–8 seconds, worst case 15 seconds. This is unacceptable for emergency P0/P1 messages — IRIS uses BLE for those. Wi-Fi Direct is reserved for P3+ where latency can absorb the setup overhead.

**Optimization:** IRIS maintains Wi-Fi Direct groups with frequently-contacted peers (e.g., the node acts as persistent GO). This eliminates repeated group formation overhead.

### GO Election Issues

If both devices have the same intent value, GO election is pseudo-random. In practice this causes occasional failures where both devices think the other is the GO. Mitigation: IRIS sets intent based on device role (fixed infrastructure node: 14, battery-powered: 7, low battery: 3), creating a predictable hierarchy.

### Battery Impact on Continuous Scan

`discoverPeers()` when running continuously drains ~80–150 mA additional to baseline. IRIS does not run continuous Wi-Fi Direct discovery. Instead:
1. BLE detects a peer with queued data.
2. IRIS activates Wi-Fi Direct discovery for a 30-second app-controlled window.
3. If group forms, transfer proceeds.
4. If no group forms in 30 seconds, deactivate Wi-Fi Direct, retry via BLE next contact.

> **Correction (RES-0021, iter 104):** The framework P2P find window is **120 s** (`DISCOVER_TIMEOUT_S`); the documented 30 s is the **app-level re-arm cadence** (IRIS stops/restarts find within/after that window). `WIFI_P2P_DISCOVERY_CHANGED_ACTION` (find stop event) returns IRIS to BLE control-plane waiting. Discovery remains best-effort on OEM builds — BLE is the durable trigger.

### OS Patch Floor (RES-0021, iter 104)

Wi-Fi stack CVEs are **OS-patch-gated** (never app-patchable). Deployment floor recorded (AC-14): **Android security patch level ≥ 2021-02** (CVE-2021-0326 P2P unauthenticated RCE), **Linux wpa_supplicant ≥ 2.12** (w1.fi 2026-1 mgmt-frame / 2026-3 SAE), kernel with 2024–26 Wi-Fi driver fixes (CVE-2024-26895/27053/47712/47724/56539/46755; CVE-2025-40321; CVE-2026-31780/46069). Single-radio STA+P2P coexistence is a runtime-availability constraint (`COEX_RESTRICTION_WIFI_DIRECT`/`WifiAvailableChannel` API 34) — IRIS degrades to BLE, never assumes concurrency.

### Platform Support Matrix

| Feature                      | Android 7+  | Android 10+ | iOS       | Linux (wpa_supplicant) |
|------------------------------|-------------|-------------|-----------|------------------------|
| Basic Wi-Fi P2P              | Yes         | Yes         | No        | Yes                    |
| Service Discovery (DNS-SD)   | Yes         | Yes         | No        | Limited                |
| Background operation         | Unreliable  | Unreliable  | No        | Yes                    |
| Persistent GO mode           | Partial     | Yes         | No        | Yes                    |
| 5 GHz support                | Device-dep. | Most devices| N/A       | Yes                    |
| Multipeer Connectivity (MCF) | No          | No          | Yes       | No                     |

### Interference with Infrastructure Wi-Fi

On most Android devices, Wi-Fi Direct and infrastructure Wi-Fi **cannot operate simultaneously on the same band.** Specifically:
- If the device is connected to a 2.4 GHz AP, Wi-Fi Direct may be forced to 2.4 GHz, causing interference.
- On 5 GHz-capable devices with dual-band support, Wi-Fi Direct can use 5 GHz while infrastructure stays on 2.4 GHz.
- Check: Android API 29+ exposes `WifiP2pConfig.setGroupOperatingBand()` to hint at band preference (AUTO / 2.4 / 5 / 6 GHz). IRIS prefers 5 GHz with AUTO fallback; band-constrained group creation fails → fall back to AUTO / 2.4 GHz / degrade to BLE.

**Recommendation:** IRIS checks the current AP connection band before initiating Wi-Fi Direct and selects the opposite band where possible.

---

## Transport Decision Summary

Use Wi-Fi Direct when:
- Payload exceeds 10 KB (large images, audio, document bundles)
- Both devices are Android (iOS cannot participate)
- App is in foreground (or foreground service active)
- Battery level > 15%
- Device is not already in a Wi-Fi Direct group with another peer (single-GO limitation)

Do NOT use Wi-Fi Direct when:
- Target is an iOS device
- App is in background and no foreground service
- Message priority is P0/P1 (setup latency too high — use BLE immediately)
- Battery < 15%
- Devices are > 100 m apart (use LoRa gateway instead)
