# USB Transport

## Overview

USB transport is IRIS's physical sneakernet — the transport of absolute last resort
when all wireless options have failed. Physical cable connection between two devices
enables high-speed, reliable message sync. This transport has no radio interference,
no spectrum licensing concerns, and works in Faraday-cage environments (underground,
inside metal structures) where wireless fails completely.

## Use Cases

1. **Zero-wireless scenario**: underground rescue operation, all radio communication blocked
2. **RF-jammed environment**: military/security scenario with active jamming
3. **Bulk data sync before deployment**: sync cached messages before entering a dead zone
4. **Trusted device pairing**: highest-trust physical key exchange alongside message sync
5. **Edge node setup**: transfer configuration and initial routing tables from field laptop

## Android USB Networking

### USB Host vs Accessory Mode

Android devices can operate in two USB roles:

**Host mode** (Android is the controller):
- Android phone acts as the USB host
- Connected device is the "accessory" (another phone, laptop, Raspberry Pi)
- Requires USB OTG (On-The-Go) support — available on most Android phones since 2012
- Phone powers the connected device at 5V/500mA (USB 2.0) or 5V/900mA (USB 3.0)

**Accessory mode** (Android is the peripheral):
- Android phone acts as the USB device
- The host (typically a laptop or desktop) controls the connection
- Enables Android Open Accessory (AOA) protocol

IRIS uses RNDIS/NCM for both modes — networking protocol over USB.

### RNDIS/NCM Networking

USB networking (Remote NDIS / Network Control Model) creates a virtual Ethernet
interface between two devices:

```
Phone A (USB host, RNDIS)          Phone B (USB device, RNDIS)
    usb0: 192.168.42.1  ←——USB——→  usb0: 192.168.42.2
         IRIS socket bind usb0          IRIS socket bind usb0
```

Android enables USB networking via:
```
Settings → Connected Devices → USB → USB tethering (ON)
```

This creates `usb0` (or `rndis0`) network interface. IRIS detects this interface:

```kotlin
// AndroidUsbNetworkDetector.kt
class AndroidUsbNetworkDetector(private val context: Context) {

    fun detectUsbNetwork(): UsbNetworkInfo? {
        val connectivityManager =
            context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

        val networks = connectivityManager.allNetworks
        for (network in networks) {
            val caps = connectivityManager.getNetworkCapabilities(network) ?: continue
            val linkProps = connectivityManager.getLinkProperties(network) ?: continue

            // USB network: has ETHERNET transport but interface name starts with usb/rndis
            if (caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET)) {
                val iface = linkProps.interfaceName ?: continue
                if (iface.startsWith("usb") || iface.startsWith("rndis") ||
                    iface.startsWith("ncm")) {
                    return UsbNetworkInfo(
                        interfaceName = iface,
                        localAddress = linkProps.linkAddresses.firstOrNull()?.address,
                        network = network
                    )
                }
            }
        }
        return null
    }
}
```

### USB Discovery Protocol

Two IRIS nodes connected via USB negotiate via mDNS on the USB network:

```
1. Node A enables USB tethering → usb0 gets 192.168.42.1
2. Node B connects → usb0 gets 192.168.42.2 (DHCP from Android)
3. Node A broadcasts mDNS: _iris._tcp.local PTR irisnode-A._iris._tcp.local
4. Node B responds with its own mDNS announcement
5. Both nodes discover each other and open IRIS TCP connection on port 7777
6. Perform full sync: exchange Bloom filters, transfer missing messages
```

### MTP File Transfer as Message Carrier

As a fallback when USB networking is unavailable (no USB tethering permission, locked
device), IRIS can use MTP (Media Transfer Protocol) file transfer:

1. Node A writes pending messages to `/sdcard/IRIS/outbox/*.iris` files
2. User plugs cable, selects "File Transfer" mode
3. Node B (or laptop) reads these files via MTP
4. IRIS on Node B imports the `.iris` files from `Downloads/` or IRIS-shared folder

This is manual and slower than USB networking, but requires no special permissions:

```rust
// .iris file format: CBOR-encoded IrisMessageBundle
pub struct IrisMessageBundle {
    pub version: u8,
    pub source_node: NodeId,
    pub messages: Vec<SignedMessage>,
    pub routing_hints: Vec<RoutingHint>,
    pub created_at: UnixTimestamp,
    pub bundle_signature: Signature,  // signs the bundle, not individual messages
}
```

## iOS USB Limitations

iOS has strict restrictions on USB:

**What iOS allows via USB**:
- iTunes/Finder sync (proprietary Apple protocol)
- CarPlay (automotive display)
- Lightning/USB-C audio accessories
- External storage access (Files app, iOS 13+, USB-C iPads only)

**What iOS does NOT allow**:
- USB networking (no RNDIS/NCM API for apps)
- General USB accessory communication without MFi certification
- USB OTG networking to another phone

**IRIS iOS approach**:
- No USB networking transport on iOS
- USB is surfaced as "import IRIS bundle files" via Files app
- User manually selects `.iris` bundle files to import
- This covers the MTP-equivalent use case only

## Desktop USB Transport

Desktop nodes (Linux, macOS, Windows) have full USB networking support:

### Linux

```bash
# When phone connected in tethering mode
# Interface appears as usb0 or enp0s20f0u1
ip link show
# usb0: <BROADCAST,MULTICAST,UP,LOWER_UP>

# IRIS auto-discovers via NetworkManager event
nmcli device status | grep usb
```

```rust
// LinuxUsbTransport: listens for udev events
// When USB network interface appears → run mDNS discovery → connect
pub struct LinuxUsbTransport {
    udev_monitor: UdevMonitor,
    active_connections: HashMap<InterfaceName, TcpStream>,
}
```

### macOS

```swift
// USB network appears as "iPhone USB" or "Android RNDIS" interface
// Detected via NWPathMonitor with .wiredInterface filter
let monitor = NWPathMonitor(requiringInterfaceType: .wiredEthernet)
monitor.pathUpdateHandler = { path in
    for iface in path.availableInterfaces {
        if iface.type == .wiredEthernet {
            IrisUsbTransport.shared.handleUsbInterface(iface)
        }
    }
}
```

### Windows

```
// USB network interface: "Remote NDIS based Internet Sharing Device"
// Appears as a network adapter in Device Manager
// IRIS detects via WinRT: Windows.Networking.Connectivity.NetworkInformation
```

## Security Model

### USB Trust Model

USB attacks (BadUSB, USB Rubber Ducky) are a real threat. Mitigations:

**Connection trust**:
- IRIS requires cryptographic node authentication before message exchange
- USB physical connection ≠ trusted connection
- Node must complete IRIS handshake (mutual Ed25519 authentication) before any data flows
- An attacker with a malicious USB device cannot extract messages or inject messages without
  the correct private key

**Data isolation**:
- Only IRIS protocol frames are processed from USB network connection
- No shell access, no file system access, no ADB-level access
- IRIS protocol parser is hardened: strict length limits, no dynamic allocation in parser

**Attack mitigations**:

| Attack | Mitigation |
|--------|-----------|
| BadUSB HID injection | IRIS only binds to IRIS port (7777); HID events go to OS, not IRIS |
| USB Rubber Ducky | Same as above; IRIS not affected by keyboard HID |
| Malicious USB network device | Auth handshake required; unauthenticated frames dropped |
| Data exfiltration via USB | E2EE: attacker sees only ciphertext; no key = no data |
| Replay attack | Nonce in every message; duplicates rejected |

**User guidance** (displayed in IRIS when USB connection is established):
```
USB Connection Established
Only connect to devices you physically control.
IRIS requires mutual authentication before any data is shared.
```

## Field Use Scenarios

### Scenario A: Underground Rescue

Environment: collapsed building, RF blocked by concrete and rebar.

```
Surface coordinator (laptop, has satellite + LoRa)
    ↑ USB cable (10m USB extension)
Rescue team leader (Android phone)
    → Collects messages from other team members via BLE
    → Syncs to laptop via USB
    → Laptop forwards via satellite for P0 messages
```

### Scenario B: Zero-Wireless Checkpoint

Environment: government facility, no wireless permitted.

```
Field worker exits facility with messages cached on phone
    → Walks to sync station (Raspberry Pi + USB hub)
    → Plugs in phone (USB tethering enabled)
    → Auto-sync: 30 seconds for full message exchange
    → Unplugs and re-enters facility
```

### Scenario C: Large File Transfer Before Deployment

Environment: disaster response team deploying to affected area.

```
Coordinator's laptop (has latest maps, contact list, routing table, cached messages)
    ↓ USB sync to each team member's phone (1-2 minutes each)
Team deploys to area with no connectivity
    → All devices have synchronized state from pre-deployment sync
    → BLE + LoRa mesh forms locally
```

## Performance

| Connection Type | Throughput | Latency | Reliability |
|-----------------|-----------|---------|-------------|
| USB 2.0 (RNDIS) | ~30 Mbps effective | <1ms | 99.9% |
| USB 3.0 (RNDIS) | ~200 Mbps effective | <0.5ms | 99.9% |
| MTP file transfer | 5–20 Mbps | Manual | 99% |
| Wireless (BLE) | 0.2 Mbps | 10–50ms | 95% |

USB is 100–1000x faster than BLE for bulk sync. Use it whenever physically possible.

## Message Priority During USB Sync

USB sync order:
1. P0 messages (all, both directions)
2. P1 messages (all, both directions)
3. P2 messages (all, both directions)
4. P3–P7 messages (newest first, until storage limit or disconnect)

Sync is bidirectional and symmetric. Both nodes simultaneously push their unseen messages
to the other. Total time for 500 P0–P2 messages: approximately 2 seconds over USB 2.0.
