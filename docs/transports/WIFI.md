# Wi-Fi Infrastructure Transport

## Overview

When devices share an access point — whether a home router, a mobile hotspot, a community Wi-Fi network, or an emergency-deployed access point — the Wi-Fi infrastructure transport enables high-bandwidth, low-latency local communication. This transport is the highest-throughput option for co-located devices and can serve as a gateway to Internet-based relays when WAN connectivity is present.

---

## Use Cases

1. **Shared AP (router/hotspot):** Multiple IRIS nodes connected to the same Wi-Fi network. Common in early disaster response before full infrastructure failure.
2. **Mobile hotspot:** One device creates a hotspot; others connect. Effectively makes that device a local hub.
3. **Emergency-deployed AP:** Aid organizations deploy portable APs (e.g., GL.iNet travel routers). All nearby IRIS nodes discover and communicate via this shared network.
4. **Internet gateway:** When the Wi-Fi AP has WAN connectivity, IRIS can reach Internet-based relay servers for out-of-area communication.

---

## Local Network Discovery: mDNS / NSD

When multiple IRIS nodes are on the same subnet, they discover each other via multicast DNS (mDNS, also known as Bonjour or Zeroconf). No central DNS server is required.

### Android: Network Service Discovery (NSD)

```kotlin
val nsdManager = context.getSystemService(NsdManager::class.java)

// Register this device as an IRIS service
val serviceInfo = NsdServiceInfo().apply {
    serviceName = "IRIS-${nodeId.take(8)}"
    serviceType = "_iris._tcp."
    port = IRIS_LOCAL_PORT
    // Service attributes (API 34+)
    setAttribute("pk", publicKeyFingerprint)
    setAttribute("ver", IRIS_PROTOCOL_VERSION.toString())
    setAttribute("caps", capabilities.toHex())
}

nsdManager.registerService(serviceInfo, NsdManager.PROTOCOL_DNS_SD,
    object : NsdManager.RegistrationListener {
        override fun onServiceRegistered(info: NsdServiceInfo) {
            localServiceName = info.serviceName  // May be renamed to avoid conflicts
        }
        override fun onRegistrationFailed(info: NsdServiceInfo, errorCode: Int) {
            handleRegistrationFailure(errorCode)
        }
        override fun onServiceUnregistered(info: NsdServiceInfo) {}
        override fun onUnregistrationFailed(info: NsdServiceInfo, errorCode: Int) {}
    }
)

// Discover other IRIS nodes
nsdManager.discoverServices("_iris._tcp.", NsdManager.PROTOCOL_DNS_SD,
    object : NsdManager.DiscoveryListener {
        override fun onServiceFound(service: NsdServiceInfo) {
            nsdManager.resolveService(service, resolveListener)
        }
        override fun onServiceLost(service: NsdServiceInfo) {
            peerLost(service.serviceName)
        }
        override fun onDiscoveryStarted(serviceType: String) {}
        override fun onDiscoveryStopped(serviceType: String) {}
        override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {}
        override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
    }
)
```

### iOS: NetService (Bonjour)

```swift
import Foundation

class IrisMdnsAdvertiser: NSObject, NetServiceDelegate {
    var netService: NetService!

    func startAdvertising() {
        netService = NetService(domain: "local.", type: "_iris._tcp.", name: "IRIS-\(nodeId.prefix(8))", port: Int32(IRIS_LOCAL_PORT))
        netService.delegate = self
        let txtData = NetService.data(fromTXTRecord: [
            "pk": publicKeyFingerprint.data(using: .utf8)!,
            "ver": String(IRIS_PROTOCOL_VERSION).data(using: .utf8)!,
        ])
        netService.setTXTRecord(txtData)
        netService.publish()
    }
}

class IrisMdnsBrowser: NSObject, NetServiceBrowserDelegate, NetServiceDelegate {
    var browser: NetServiceBrowser!

    func startBrowsing() {
        browser = NetServiceBrowser()
        browser.delegate = self
        browser.searchForServices(ofType: "_iris._tcp.", inDomain: "local.")
    }

    func netServiceBrowser(_ browser: NetServiceBrowser, didFind service: NetService, moreComing: Bool) {
        service.delegate = self
        service.resolve(withTimeout: 5.0)
    }

    func netServiceDidResolveAddress(_ sender: NetService) {
        // Extract IP + port + TXT record
        let txtRecord = NetService.dictionary(fromTXTRecord: sender.txtRecordData()!)
        let peerPk = String(data: txtRecord["pk"]!, encoding: .utf8)!
        peerDiscovered(host: sender.hostName!, port: sender.port, publicKey: peerPk)
    }
}
```

### iOS Local Network Permission (iOS 14+)

Starting iOS 14, accessing the local network (mDNS, UDP broadcasts, direct TCP to LAN IPs) requires explicit user permission:

```xml
<!-- Info.plist -->
<key>NSLocalNetworkUsageDescription</key>
<string>IRIS discovers other IRIS nodes on your local Wi-Fi network for emergency communication.</string>

<!-- Required to declare Bonjour service types used -->
<key>NSBonjourServices</key>
<array>
    <string>_iris._tcp</string>
</array>
```

The permission dialog appears the first time the app attempts local network access. If denied, mDNS discovery fails silently on iOS 14+. IRIS handles this by checking for discovery results within 10 seconds and prompting the user to grant local network access if no peers are found.

---

## Protocol Choice: UDP vs TCP vs QUIC

### TCP

Standard reliable stream protocol. Works well for message transfer on stable local networks.

Pros:
- Universal support, no libraries needed
- Reliable, ordered delivery
- Works through NAT (for relay connections)

Cons:
- Head-of-line blocking: one lost packet stalls all subsequent data
- Connection state per peer (for mesh: N×M connections = complex)
- Higher latency due to three-way handshake

### UDP

Unreliable datagram protocol. Used with IRIS's own reliability layer.

Pros:
- No connection overhead
- Multicast support (broadcast to all LAN peers simultaneously)
- Lower latency for small messages

Cons:
- Requires application-level reliability (IRIS must implement ACKs, retransmit)
- Fragmentation at application layer for large messages
- Firewalls and some APs block UDP broadcasts/multicasts

IRIS uses UDP for:
- mDNS (standard protocol)
- Discovery heartbeats (small, loss-tolerant)
- P0/P1 emergency alerts sent as LAN broadcast (multiple recipients simultaneously)

### QUIC

QUIC (RFC 9000) is IRIS's preferred protocol for sustained peer-to-peer data transfer over local networks and Internet connections.

Pros:
- Multiplexed streams without head-of-line blocking
- Built-in TLS 1.3 encryption (required for IRIS anyway)
- 0-RTT reconnection (resumes sessions without full handshake on reconnect)
- Connection migration (when device switches from Wi-Fi to cellular, connection survives)
- Works over UDP (friendly to lossy networks)

Cons:
- Requires QUIC library (IRIS uses `quinn` in Rust)
- Higher CPU than TCP for identical throughput
- Some firewalls and carrier networks block QUIC (HTTP/3 fallback: port 443)

**IRIS default:** QUIC for all sustained data exchange. TCP as fallback when QUIC is blocked or unavailable.

```rust
// Quinn QUIC client setup (Rust)
let mut endpoint = quinn::Endpoint::client("0.0.0.0:0".parse()?)?;
endpoint.set_default_client_config(build_tls_config()?);

let connection = endpoint.connect(peer_addr, &peer_hostname)?.await?;
let (mut send, mut recv) = connection.open_bi().await?;

send.write_all(&message_bytes).await?;
send.finish().await?;
```

---

## Hotspot Detection and Configuration

When a device creates a Wi-Fi hotspot, it can be detected by others and used as a gateway:

### Android Hotspot API

```kotlin
// Check if this device is providing a hotspot
val wifiManager = context.getSystemService(WifiManager::class.java)
// Note: direct hotspot state API requires CHANGE_NETWORK_STATE or root

// Simpler: detect hotspot AP by checking local IP range
fun isHotspotActive(): Boolean {
    val connectivityManager = context.getSystemService(ConnectivityManager::class.java)
    val networks = connectivityManager.allNetworks
    return networks.any { network ->
        connectivityManager.getNetworkCapabilities(network)
            ?.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) == true &&
        connectivityManager.getLinkProperties(network)
            ?.linkAddresses?.any { it.address.hostAddress?.startsWith("192.168.43.") == true } == true
        // 192.168.43.x is Android hotspot DHCP range
    }
}

// Request to start hotspot (requires TETHER_PRIVILEGED or user approval flow)
// IRIS uses the SoftAP suggestion API (Android 10+) or WifiManager.LocalOnlyHotspotCallback
wifiManager.startLocalOnlyHotspot(object : WifiManager.LocalOnlyHotspotCallback() {
    override fun onStarted(reservation: WifiManager.LocalOnlyHotspotReservation) {
        val ssid = reservation.wifiConfiguration?.SSID
        val password = reservation.wifiConfiguration?.preSharedKey
        // Advertise this via BLE so nearby devices can connect
        advertisHotspotViaBlE(ssid, password)
    }
    override fun onStopped() {}
    override fun onFailed(reason: Int) {}
}, handler)
```

### Internet Availability Detection

IRIS distinguishes between "connected to Wi-Fi" and "Wi-Fi with Internet access":

```kotlin
fun hasInternetAccess(context: Context): Boolean {
    val cm = context.getSystemService(ConnectivityManager::class.java)
    val activeNetwork = cm.activeNetwork ?: return false
    val caps = cm.getNetworkCapabilities(activeNetwork) ?: return false
    return caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) &&
           caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
}
```

`NET_CAPABILITY_VALIDATED` means Android has confirmed Internet connectivity by probing a captive portal server. This is more reliable than just checking if Wi-Fi is connected.

When Internet is confirmed:
1. IRIS initiates connection to the relay server (if configured).
2. The Internet transport layer activates.
3. Messages can be routed to nodes outside the local mesh.

---

## Subnet Communication Patterns

### Broadcast (UDP)

```kotlin
// Send emergency alert to all local network devices
val socket = DatagramSocket()
socket.broadcast = true
val broadcastAddr = InetAddress.getByName("255.255.255.255")
val data = emergencyAlert.serialize()
val packet = DatagramPacket(data, data.size, broadcastAddr, IRIS_DISCOVERY_PORT)
socket.send(packet)
```

Note: Subnet broadcast may be blocked by AP firmware (common in hotel Wi-Fi, enterprise networks). IRIS falls back to mDNS-discovered unicast when broadcast fails.

### Direct Unicast TCP/QUIC

Once a peer is discovered via mDNS, IRIS establishes direct TCP or QUIC connections for reliable message exchange. This is the primary data channel for Wi-Fi infrastructure transport.

---

## Battery Impact: Wi-Fi vs Cellular

| Scenario                         | Wi-Fi Current Draw | Cellular (LTE) Current Draw |
|----------------------------------|--------------------|------------------------------|
| Connected, idle                  | 15–30 mA           | 10–25 mA                     |
| Light traffic (10 kbps)          | 30–50 mA           | 40–80 mA                     |
| Sustained transfer (1 Mbps)      | 80–150 mA          | 150–300 mA                   |
| Scanning for AP                  | 50–100 mA          | N/A                          |

Wi-Fi is generally more energy-efficient than cellular for sustained data transfer. The Wi-Fi radio uses OFDM modulation optimized for throughput, while cellular must negotiate channels and handle handoff overhead.

**IRIS preference:** When both Wi-Fi and cellular are available, prefer Wi-Fi for local mesh traffic and cellular only for Internet-bound relay traffic.

---

## Limitations and Known Issues

1. **Enterprise Wi-Fi AP isolation:** Many enterprise and hotel APs enable "AP client isolation," which prevents devices on the same AP from communicating with each other. IRIS cannot discover or communicate with peers on such networks. Detection: attempt UDP broadcast + mDNS; if no responses in 15 seconds despite known local network, assume isolation is active. Fall back to BLE + Internet relay.

2. **iOS local network permission:** Users frequently deny this permission. IRIS must handle this gracefully and guide users to re-enable.

3. **mDNS multicast blocked:** Some APs block IGMP/multicast. IRIS uses a fallback: attempt direct TCP connection to known IRIS peer IP ranges (192.168.x.x subnet sweep — aggressive, not preferred) or rely on BLE advertisement of IP address.

4. **Dynamic IP changes:** When a device reconnects to a Wi-Fi AP, its IP may change. IRIS tracks peers by public key fingerprint, not IP. mDNS re-resolution handles IP changes transparently.

5. **IPv6 link-local:** Prefer IPv6 link-local addresses (fe80::) when available — these are stable across DHCP renewals for a given network interface. IRIS tries IPv6 first, falls back to IPv4.
