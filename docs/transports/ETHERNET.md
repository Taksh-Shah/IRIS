# Ethernet Transport

## Overview

Ethernet is the most reliable transport available to IRIS. Wired connections eliminate
interference, multipath fading, and spectrum congestion. For fixed nodes — disaster
operations centers, hospital command posts, edge relay nodes on Raspberry Pi — Ethernet
is the preferred primary transport. For mobile phones, Ethernet is available via USB
adapter and serves niche but important use cases.

## Use Cases

### Disaster Operations Center (DOC)

A district-level DOC during a major disaster has:
- Ethernet switch connecting multiple coordinator workstations
- Raspberry Pi edge nodes connected to mesh radio hardware (LoRa, Wi-Fi AP)
- VSAT or Starlink terminal uplinked to Internet
- All connected via gigabit Ethernet

IRIS nodes in this environment have unlimited bandwidth, sub-millisecond LAN latency,
and permanent connectivity. They become high-capacity relay and storage nodes.

### Hospital Emergency Network

During a mass casualty event:
- Hospital's existing LAN becomes IRIS backbone
- IRIS edge nodes on each ward connect to hospital LAN
- Medical staff carry phones that connect to ward IRIS nodes via Wi-Fi/BLE
- Patient tracking, triage status, resource requests flow over hospital Ethernet LAN

### Field Relay Node (Raspberry Pi)

```
                 [Ethernet to ISP router]
                         ↓
           [Raspberry Pi 4 — IRIS Edge Node]
           /              |              \
    [LoRa radio]  [Wi-Fi AP (hostapd)]  [BLE]
```

Raspberry Pi runs full IRIS routing engine, LoRa gateway, Wi-Fi access point, and
Ethernet gateway simultaneously. Ethernet provides the Internet uplink. Multiple
Pi nodes in a disaster area each have Ethernet to local ISP fiber (if surviving).

## APIs by Platform

### Linux

Linux exposes Ethernet via standard BSD socket API. IRIS on Linux uses:

```rust
// Linux Ethernet transport — full raw socket capability
use std::net::{TcpListener, TcpStream};

pub struct LinuxEthernetTransport {
    bind_interface: String,  // e.g., "eth0", "enp3s0"
    listener: TcpListener,
    mdns_daemon: MdnsDaemon,
}

impl LinuxEthernetTransport {
    pub fn new(interface: &str) -> Result<Self> {
        // Bind to specific interface (not all interfaces)
        let socket = Socket::new(Domain::IPV4, Type::STREAM, None)?;
        socket.bind_device(Some(interface.as_bytes()))?;
        // SO_BINDTODEVICE — ensures traffic goes over this interface
        Ok(Self {
            bind_interface: interface.to_string(),
            listener: socket.into(),
            mdns_daemon: MdnsDaemon::new(interface)?,
        })
    }
}
```

NetworkManager integration for interface detection:

```rust
// Monitor for Ethernet interface appearance/disappearance
// via netlink RTNLGRP_LINK messages
pub fn watch_interfaces(tx: mpsc::Sender<InterfaceEvent>) -> Result<()> {
    let nl = NetlinkSocket::new()?;
    nl.bind_to_group(RTNLGRP_LINK)?;
    loop {
        let msg = nl.recv()?;
        match msg {
            NetlinkMessage::NewLink(link) if link.is_ethernet() => {
                tx.send(InterfaceEvent::Up(link.name))?;
            }
            NetlinkMessage::DelLink(link) => {
                tx.send(InterfaceEvent::Down(link.name))?;
            }
            _ => {}
        }
    }
}
```

### macOS

```swift
// macOS: Network.framework for high-level, BSD sockets for low-level
import Network

class MacEthernetTransport {
    private let monitor = NWPathMonitor(requiringInterfaceType: .wiredEthernet)

    func startMonitoring() {
        monitor.pathUpdateHandler = { path in
            for interface in path.availableInterfaces
                where interface.type == .wiredEthernet {
                EthernetRegistry.shared.registerInterface(interface)
            }
        }
        monitor.start(queue: .global(qos: .utility))
    }

    func createListener(on port: UInt16) throws -> NWListener {
        let params = NWParameters.tcp
        params.requiredInterfaceType = .wiredEthernet
        let listener = try NWListener(using: params, on: NWEndpoint.Port(rawValue: port)!)
        return listener
    }
}
```

macOS also has Bonjour (mDNS) via `NSNetService` / `Network.framework NWBrowser`:

```swift
let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_iris._tcp", domain: nil), using: .tcp)
browser.browseResultsChangedHandler = { results, changes in
    for result in results {
        IrisNodeDiscovery.shared.foundNode(result.endpoint)
    }
}
```

### Windows

```csharp
// Windows: WinRT NetworkInformation API
using Windows.Networking.Connectivity;

public class WindowsEthernetTransport {
    public IEnumerable<ConnectionProfile> GetEthernetProfiles() {
        return NetworkInformation.GetConnectionProfiles()
            .Where(p => p.NetworkAdapter?.IanaInterfaceType == 6); // 6 = Ethernet
    }

    public async Task DiscoverNodesAsync() {
        // DNS-SD (mDNS) via Windows 10 mDNS APIs (Dnsapi.dll)
        // or manual UDP broadcast on all Ethernet interfaces
        var interfaces = GetEthernetProfiles();
        foreach (var iface in interfaces) {
            await BroadcastDiscovery(iface);
        }
    }
}
```

Winsock 2 for raw socket operations:

```rust
// Rust on Windows: std::net works cross-platform
// For interface binding: winapi crate + setsockopt IP_UNICAST_IF
```

## Local Network Discovery

### mDNS (Zero-configuration DNS)

Implementations:
- Linux: Avahi daemon (`avahi-browse _iris._tcp`)
- macOS: Bonjour (built-in)
- Windows: Windows mDNS (built-in since Windows 10 1703)
- Android: Network Service Discovery (NsdManager)
- iOS: Bonjour via NetService

IRIS service type: `_iris._tcp.local.`

TXT record format:
```
nodeid=<hex-encoded-32-bytes>
version=1
priority=<P0..P7 min handled>
capabilities=RELAY,GATEWAY,LORA
```

### UDP Broadcast Discovery

Fallback when mDNS is unavailable or blocked:

```rust
pub async fn broadcast_discovery(bind_addr: IpAddr, subnet: Ipv4Net) -> Vec<DiscoveredNode> {
    let sock = UdpSocket::bind((bind_addr, IRIS_DISCOVERY_PORT)).await?;
    sock.set_broadcast(true)?;

    // Broadcast to subnet broadcast address
    let broadcast_addr = SocketAddr::new(
        IpAddr::V4(subnet.broadcast()),
        IRIS_DISCOVERY_PORT
    );

    let probe = DiscoveryProbe::new();
    sock.send_to(&probe.encode(), broadcast_addr).await?;

    let mut nodes = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let mut buf = [0u8; 1024];
        if let Ok((len, addr)) = sock.recv_from(&mut buf).await {
            if let Ok(response) = DiscoveryResponse::decode(&buf[..len]) {
                nodes.push(DiscoveredNode { addr, info: response });
            }
        }
    }
    nodes
}
```

## Android Ethernet via USB Adapter

Android devices with USB-C / USB-OTG support external USB-to-Ethernet adapters:

- No special app permission required beyond `INTERNET`
- Android detects adapter as Ethernet interface via USB host mode
- IRIS sees it as a standard network interface (TRANSPORT_ETHERNET in NetworkCapabilities)

```kotlin
// Detection — same as USB networking detection
val request = NetworkRequest.Builder()
    .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
    .build()
connectivityManager.requestNetwork(request, ethernetCallback)
```

Tested adapters: UGREEN USB-C Gigabit (~₹1,200), Anker USB-C Ethernet (~₹1,800).
Both work on Pixel and Samsung Galaxy devices.

## iOS Ethernet

iOS 14+ supports Ethernet via USB-C adapters (iPad Pro, iPhone 15 Pro):

- Works for infrastructure network (Internet access)
- Apps see it as a standard network connection
- No raw Ethernet access for apps
- IRIS treats it identically to Wi-Fi: IP-based transport

**Limitation**: Lightning port iPhones (pre-iPhone 15) require Lightning-to-USB3 adapter
(Apple only, expensive). USB-C iPhones work with standard USB-C Ethernet adapters.

## Power over Ethernet (PoE)

PoE dramatically simplifies edge node deployment:

| Standard | Power | Use Case |
|----------|-------|----------|
| 802.3af (PoE) | 15.4W | Raspberry Pi Zero 2W, small nodes |
| 802.3at (PoE+) | 30W | Raspberry Pi 4, with peripherals |
| 802.3bt (PoE++) | 60–90W | High-power nodes with radio hardware |

Deployment scenario:
```
Generator → PoE Switch → (30m Cat6 cable to) → Raspberry Pi 4 Edge Node
```

One cable carries both power and data. Edge node needs no separate power supply.
Enables rapid deployment: run a single CAT6 cable, plug in, done.

```yaml
# IRIS edge node PoE deployment spec
hardware:
  sbc: "Raspberry Pi 4 Model B 4GB"
  poe_hat: "Raspberry Pi PoE+ HAT (802.3at, 25.5W)"
  lora: "RAK2287 mPCIe LoRa concentrator (SX1302)"
  wifi: "Alfa AWUS036ACS USB Wi-Fi (hostapd)"
power_budget_watts:
  pi4_idle: 3.4
  pi4_load: 6.4
  lora_rx: 1.0
  lora_tx: 2.5
  wifi_ap: 3.0
  total_max: 13.0  # well within PoE+ 25.5W budget
```

## Performance Characteristics

| Interface | Throughput | Latency | Jitter | Notes |
|-----------|-----------|---------|--------|-------|
| GbE LAN | 940 Mbps | <0.1 ms | <0.01 ms | Best possible |
| 100 Mbps LAN | 94 Mbps | <0.5 ms | <0.05 ms | Hospital/office networks |
| USB 3.0 Ethernet | 200 Mbps | <1 ms | <0.1 ms | Android/laptop via adapter |
| USB 2.0 Ethernet | 30 Mbps | <2 ms | <0.5 ms | Older adapters |

For IRIS message traffic (small packets, 200 bytes–10 KB), GbE LAN can handle
thousands of simultaneous relay sessions with no congestion.

## Ethernet as Gateway Node

When an Ethernet-connected IRIS node has Internet access, it automatically
becomes a gateway for the mesh:

```rust
pub struct GatewayAdvertisement {
    pub node_id: NodeId,
    pub gateway_type: GatewayType, // Internet, LoRa, Satellite
    pub bandwidth_bps: u64,
    pub latency_ms: u32,
    pub cost_factor: f32,  // 0.0 = free, 1.0 = expensive
    pub priorities_supported: Vec<Priority>,
}
// Ethernet Internet gateway: bandwidth 10_000_000 bps, latency 30ms, cost 0.1
// Supports all priorities P0–P7
```

This advertisement propagates via BLE/Wi-Fi mesh so remote nodes know to route
Internet-bound traffic toward the Ethernet-connected gateway.
