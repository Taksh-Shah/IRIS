# IRIS Cross-Platform Interoperability

## Protocol Wire Format: CBOR

All IRIS messages use CBOR (RFC 8949) as the wire format. CBOR is a platform-independent binary format. An Android device encoding a message and a Raspberry Pi decoding it use identical logic — the Rust `iris-proto` crate compiles to every target and handles all encoding/decoding.

A CBOR envelope transmitted over BLE between an Android phone and an iOS phone contains no platform-specific bytes. The envelope is valid on any IRIS node regardless of OS.

## Shared Logic via Rust Core

The `iris-core` crate compiles to:
- `aarch64-linux-android` (Android ARM64)
- `aarch64-apple-ios` (iPhone)
- `x86_64-pc-windows-msvc` (Windows)
- `aarch64-apple-darwin` (macOS Apple Silicon)
- `aarch64-unknown-linux-gnu` (Linux ARM64 / Raspberry Pi)

The same binary logic runs everywhere. Routing decisions, crypto operations, deduplication, TTL enforcement — identical behavior on all platforms. This is critical: a message relayed through Android → iOS → Linux must be processed identically at each hop.

## Interoperability Test Matrix

Tests that must pass before any release:

| Scenario | Transport | Status |
|----------|-----------|--------|
| Android ↔ Android (same OEM) | BLE GATT | PLANNED |
| Android ↔ Android (different OEM, e.g. Pixel ↔ Redmi) | BLE GATT | PLANNED |
| Android ↔ iOS | BLE GATT | PLANNED |
| Android ↔ Linux (Pi) | BLE GATT | PLANNED |
| iOS ↔ iOS | BLE GATT | PLANNED |
| iOS ↔ macOS | BLE GATT + MCF | PLANNED |
| Android ↔ Windows | BLE GATT | PLANNED |
| Android ↔ Linux ↔ iOS (3-platform relay) | BLE GATT | PLANNED |
| Android (foreground) ↔ iOS (background) | BLE GATT | PLANNED |
| Multi-hop: Android → Linux → iOS | BLE GATT | PLANNED |

## Known Asymmetries

### iOS Cannot Advertise in Background

**Description**: When an iOS app goes to background, `CBPeripheralManager` stops advertising. The iOS device is invisible to scanning peers.

**Impact**: An iOS device in background cannot be the entry point of a relay. It can receive messages from connected peers (connections already established when foregrounded) but cannot accept new connections.

**Design response**:
- Document clearly in iOS docs and user-facing help
- iOS contributes fully when in foreground
- In mixed-platform meshes, Android/Linux nodes handle relay-heavy work
- MCF supplements CoreBluetooth for iOS↔iOS paths

**Wire format impact**: None. The asymmetry is transport-layer, not protocol-layer.

### Windows BLE Peripheral Limitations

**Description**: Windows `GattServiceProvider` for running a GATT server has known limitations — some BLE adapters do not support being a GATT server simultaneously while scanning. USB BLE adapters vary in driver quality.

**Impact**: Windows desktop may have inconsistent GATT server behavior depending on BLE adapter. Intel AX200/AX210 (common in laptops) has good driver support. Broadcom adapters (less common) vary.

**Mitigation**: Test with common Intel and Realtek adapters. Document minimum adapter requirements. Provide fallback to client-only mode (connect to peers rather than accepting connections).

### Linux BLE Quality Varies by Adapter

**Description**: BlueZ quality depends on the Bluetooth chipset and firmware. Raspberry Pi's BCM43455 is well-tested. Generic USB BLE dongles (CSR-based) may have firmware issues.

**Impact**: BLE relay reliability varies on Linux.

**Mitigation**: Test specifically with BCM43455 (Pi 4), Intel AX200, and one generic USB dongle. Document supported adapters. The `btleplug` library handles most adapter quirks.

### Android BLE Varies by OEM

**Description**: Samsung, Xiaomi, Realme, and Oppo all modify Android's BLE stack or apply battery kill policies that can suspend BLE service.

**Impact**: Relay continuity may be interrupted on some OEMs.

**Mitigation**: Test on at minimum: Pixel (reference), Redmi Note (Xiaomi), Samsung A-series. Guide users through OEM-specific battery settings.

## Feature Flags and Graceful Degradation

IRIS uses runtime capability detection — no platform assumes a transport is available.

### Transport Capability Detection

```rust
pub struct TransportCapabilities {
    pub ble_central: bool,        // Can scan for BLE peripherals
    pub ble_peripheral: bool,     // Can advertise as BLE peripheral
    pub ble_gatt_server: bool,    // Can run GATT server
    pub wifi_aware: bool,         // Android Wi-Fi Aware available
    pub wifi_direct: bool,        // Wi-Fi Direct available
    pub local_network: bool,      // TCP/UDP on LAN available
    pub lora: bool,               // LoRa hardware available
    pub internet: bool,           // Internet reachable
}
```

### Degradation Ladder

When a transport is unavailable, IRIS falls back to the next best option:

```
1. Wi-Fi Aware (highest bandwidth, best range, Android 8+)
   ↓ unavailable
2. Wi-Fi Direct (good bandwidth, moderate setup time)
   ↓ unavailable
3. BLE GATT (universal, low bandwidth, always available on mobile)
   ↓ range exceeded
4. Store-and-carry (no transport, node carries message physically)
   ↓ Internet available
5. Internet relay (best effort, not guaranteed offline)
```

Each transport level is attempted in order for each message. For P0 messages: all available transports are used simultaneously.

### iOS-Only Mesh

In a network with only iOS devices (e.g., school group with iPhones):
- All devices in foreground: full BLE mesh, all peers visible, normal operation
- Some devices background: foregrounded devices relay for backgrounded ones; backgrounded iOS devices are invisible to scan but can receive if previously connected
- MCF (MultipeerConnectivity): supplemental path for iOS↔iOS that works slightly better in background than raw CoreBluetooth

### Android-Only Mesh

In a network with only Android devices:
- Full BLE mesh with GATT server on all nodes
- Wi-Fi Aware used for high-throughput transfers when available
- Wi-Fi Direct for bulk payloads
- Maximum relay capability on all nodes (no background limitations at foreground service level)

### Mixed Platform Group

All platforms speak the same IRIS protocol. The routing layer is transparent to transport:
- Group has: 3 Android + 2 iOS + 1 Linux Pi
- Message sent from Android: routed by Rust routing engine based on delivery probability
- iOS devices' limited background relay capability is reflected in their PRoPHET delivery probability metric — peers with historically lower contact frequency get lower probability scores
- Pi gets highest delivery probability for messages destined outside BLE range (gateway role)
- The routing engine naturally prefers better-connected nodes without any platform-specific logic

## CBOR Compatibility Test

Run this test on all platforms to verify identical serialization behavior:

```rust
#[test]
fn cross_platform_cbor_canonical() {
    // Known test vector — same bytes on all platforms
    let msg = IrisMessage {
        id: MessageId::from_bytes([0x01; 16]),
        priority: Priority::P4Normal,
        ttl: 3600,
        hop_count: 0,
        sender: NodeId::from_bytes([0xAA; 32]),
        recipient: NodeId::from_bytes([0xBB; 32]),
        payload_length: 5,
        payload: b"hello".to_vec(),
    };

    let encoded = msg.to_cbor().unwrap();
    let expected_hex = "..."; // Pre-computed canonical encoding
    assert_eq!(hex::encode(&encoded), expected_hex);

    let decoded = IrisMessage::from_cbor(&encoded).unwrap();
    assert_eq!(decoded, msg);
}
```

The expected hex must match across Android (JVM), iOS (Swift), and Rust tests. Differences indicate a platform-specific serialization bug.

## Version Compatibility

IRIS includes a protocol version field in every envelope. Version negotiation during peer handshake:

1. Node A connects to Node B
2. A sends `Hello { node_id, protocol_version: [1, 0, 0], capabilities: [...] }`
3. B checks if protocol versions are compatible (major version match)
4. B sends `HelloAck { node_id, protocol_version: [1, 0, 0], capabilities: [...] }`
5. Both nodes know each other's capabilities — iOS limitation known to Android peer

Version mismatch handling:
- Major version mismatch: refuse connection (incompatible protocol)
- Minor version mismatch: connect with feature set limited to lower version
- Patch version mismatch: no effect (patch = bug fix only, no protocol change)

## Transport Bandwidth Reference

| Transport | Typical Bandwidth | Range | Latency |
|-----------|------------------|-------|---------|
| BLE 4.2 GATT | 10-20 KB/s | 10-50m | 50-200ms |
| BLE 5.0 GATT | 20-50 KB/s | 50-200m | 30-100ms |
| Wi-Fi Aware | 1-10 MB/s | 30-100m | 5-20ms |
| Wi-Fi Direct | 10-50 MB/s | 50-200m | 10-50ms |
| TCP/LAN | 100+ MB/s | LAN | <5ms |
| LoRa SF7 | ~5.5 Kbps | 2-5km urban | 100-500ms |
| LoRa SF12 | ~250 bps | 5-15km | 1-2s |

Message priority routing uses this bandwidth information to decide transport selection for each message type.
