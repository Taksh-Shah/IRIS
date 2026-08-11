# Layer Model

## Overview
Seven-layer model for IRIS. Unlike OSI, designed for disruption-tolerant heterogeneous networks. Each layer has defined responsibilities, failure behaviors, and recovery strategies. The model prioritizes emergency communication at every layer.

## Layer 0 — Physical Transport
Raw physical communication. BLE radio, Wi-Fi radio, LoRa radio, Ethernet wire, satellite link. Managed by OS/hardware. IRIS interacts via transport adapters.

Physical transports available:
- **Bluetooth Low Energy (BLE 5.x)**: 2–50m range, 2Mbps theoretical (125–250kbps practical), low power, ubiquitous on phones
- **Wi-Fi Direct (P2P)**: 50–200m range, 54–600Mbps theoretical, higher power, requires group owner negotiation
- **Wi-Fi Aware (NAN)**: 50–100m range, discovery without hotspot, Android 8.0+ only, not available on iOS
- **LoRa (SX1276/SX1262)**: 2–15km range, 250bps–50kbps, very low power, ISM band (865–867MHz in India), hardware required
- **Ethernet**: up to 1Gbps, wired, power from PoE possible, always infrastructure-attached
- **Satellite (Iridium/Starlink/GSAT)**: global coverage, 2.4kbps–100Mbps depending on terminal, high latency (500–700ms GEO, 20–40ms LEO), high cost
- **Cellular (4G/5G)**: treated as Internet transport; not direct peer-to-peer

IRIS does not control Layer 0 hardware directly. It uses platform APIs (Android BluetoothManager, CoreBluetooth on iOS, Linux BlueZ/nl80211, etc.).

## Layer 1 — Transport Adapter
Platform-specific adapters that normalize each physical transport into a common TransportAdapter trait. Responsibilities: open/close transport, scan for peers, advertise presence, send/receive raw bytes. Each adapter reports capability metadata to higher layers.

### TransportAdapter Trait (Rust pseudocode)
```rust
pub trait TransportAdapter: Send + Sync {
    fn transport_type(&self) -> TransportType;
    fn open(&mut self) -> Result<(), TransportError>;
    fn close(&mut self) -> Result<(), TransportError>;
    fn scan_for_peers(&self) -> Result<Vec<PeerInfo>, TransportError>;
    fn advertise(&self, adv_data: &AdvertisementData) -> Result<(), TransportError>;
    fn send(&self, peer: &PeerId, data: &[u8]) -> Result<SendResult, TransportError>;
    fn recv(&self) -> Result<ReceivedMessage, TransportError>;
    fn mtu(&self) -> usize;
    fn bandwidth_estimate_bps(&self) -> u32;
    fn latency_estimate_ms(&self) -> u32;
    fn battery_cost_class(&self) -> BatteryCostClass;  // LOW / MEDIUM / HIGH
    fn requires_infrastructure(&self) -> bool;
    fn background_capable(&self) -> bool;
    fn is_available(&self) -> bool;
}
```

### Adapter Inventory

| Transport | Platform | MTU | Battery Cost | Background | Infrastructure |
|-----------|----------|-----|-------------|------------|----------------|
| BLE GATT | Android/iOS/Linux | 512 bytes | LOW | Yes (limited) | No |
| Wi-Fi Direct | Android/Linux | ~1400 bytes | MEDIUM | Android only | No |
| Wi-Fi Aware | Android 8+ | ~1400 bytes | MEDIUM | Partial | No |
| LoRa | Linux/embedded | 255 bytes | LOW | Yes | No |
| Ethernet | Linux/macOS | 1500 bytes | NONE | Yes | Yes |
| Internet TCP | All | 64KB segments | MEDIUM | Yes | Yes |
| Satellite | Linux/embedded | 1500 bytes | HIGH | Yes | Yes |

### iOS Limitations (Non-Negotiable)
- BLE: can advertise and scan in foreground; background scan limited to 180s before throttled
- Wi-Fi Aware: **not available** on iOS — adapter returns UNAVAILABLE
- Wi-Fi Direct: **not available** on iOS via public API
- Workaround for iOS: BLE for discovery, Multipeer Connectivity Framework (MCF) for data exchange

## Layer 2 — Discovery & Neighbor Management
Peer discovery across all available transports. Maintains a neighbor table of currently reachable peers.

### Neighbor Table Schema
```
neighbor_table: {
    node_id:               bytes(32),   // Cryptographic node identity
    transport:             TransportType,
    peer_address:          string,      // BLE MAC, IP:port, etc.
    last_seen:             Timestamp,
    signal_strength_dbm:   i16,
    estimated_bandwidth:   u32,         // bps
    capability_bundle:     CapabilityBundle,
    relay_enabled:         bool,
    gateway_types:         Vec<GatewayType>,
}
```

### Discovery Protocols Per Transport
- **BLE**: GATT service advertisement (UUID: `IRIS-DISCOVERY-v1`), scan interval 500ms on foreground, 5s on background
- **Wi-Fi Aware**: NAN publish/subscribe on service `iris.mesh.v1`
- **Wi-Fi Direct**: mDNS advertisement `_iris._tcp.local.`
- **LoRa**: periodic HELLO broadcast, 60s interval (spectrum conservation)
- **Internet**: DNS-SD or configured relay server for Internet-mediated discovery

### Neighbor Expiry
- BLE neighbor: expires after 30s without advertisement
- Wi-Fi neighbor: expires after 10s without keep-alive
- LoRa neighbor: expires after 5 minutes without HELLO
- Expired neighbors removed from routing consideration; delivery probability reduced

### Capability Exchange
On first contact (or after protocol version change):
1. Node A sends CapabilityBundle (signed, ~200 bytes)
2. Node B verifies signature, stores capability
3. Node B replies with its own CapabilityBundle
4. Both nodes update neighbor table with capabilities

## Layer 3 — Transport Manager
Selects optimal transport(s) for each outbound message. Manages multiple concurrent transports.

### Transport Selection Algorithm
```
For each message to send:
  1. Get recipient from routing table → preferred_transport
  2. Check if preferred_transport is available and has sufficient bandwidth
  3. If no preferred: score all available transports
     score(t) = bandwidth_weight * bandwidth(t)
              - latency_weight * latency(t)
              - battery_weight * battery_cost(t)
              + reliability_weight * reliability(t)
  4. Select highest-scoring available transport
  5. For P0-P1: select ALL available transports (multi-path)
```

### Battery Policy
- FULL mode: any transport permitted
- BALANCED mode: prefer LOW battery cost transports
- LOW_BATTERY mode: BLE only, no Wi-Fi Direct
- CRITICAL mode: no discovery; relay only P0-P1 received messages
- EMERGENCY_OVERRIDE: all transports active regardless of battery (P0 only)

### Transport Failure Detection
- Send failure: mark transport DEGRADED, retry after 5s
- Three consecutive failures: mark transport UNAVAILABLE
- Transport restoration: periodic probe (30s interval)

## Layer 4 — Routing Engine
Routing decisions. Maintains routing table, contact history, delivery probability estimates.

### Routing Table Schema
```
routing_table: {
    destination_id:     bytes(32),
    next_hop_id:        bytes(32),
    transport:          TransportType,
    delivery_prob:      f32,           // PRoPHET estimate 0.0-1.0
    last_contact_time:  Timestamp,
    hop_count:          u8,
}
```

### Routing Algorithms
1. **Direct**: recipient is a current neighbor → send directly
2. **Spray-and-Wait**: send N copies, let recipients wait (for low-connectivity scenarios)
3. **PRoPHET**: predictive routing based on contact history (for mobile DTN)
4. **Epidemic**: flood to all neighbors (last resort, P0 emergency)

### Routing Algorithm Selection
- P0: Epidemic (maximum propagation)
- P1-P2: PRoPHET if history available, else Spray-and-Wait (n=3)
- P3-P4: PRoPHET or direct
- P5-P7: Direct only (no relay for large messages)

### Store-Carry-Forward Integration
When no next hop available: message stored locally. On each new neighbor contact, routing engine re-evaluates all stored messages and forwards those with suitable delivery probability.

### Deduplication at Layer 4
Before relay, check `seen_message_cache` (Bloom filter + recent 10,000 IDs in LRU).
If message_id seen: DROP silently.

## Layer 5 — Message Engine
Message lifecycle management. Priority queues, storage, TTL enforcement, fragmentation.

### Priority Queue Structure
```
outbound_queue: BinaryHeap<QueuedMessage>  // Max-heap by priority
  sorted by: (priority, age, delivery_probability)
```

### Store-and-Forward Buffer
SQLite-backed persistent store. All P0-P4 messages persisted. P5-P7 optional (storage quota dependent).

### TTL Enforcement
Every message has `expires_at` timestamp. TTL enforced at:
- Queue insertion: check TTL before enqueue
- Queue dequeue: check TTL before send
- Relay receive: check TTL before store
- Scheduled cleanup: every 60s, purge expired messages

### Fragmentation
Required when message > MTU of selected transport:
- BLE: fragment at 512 bytes (GATT MTU)
- LoRa: fragment at 255 bytes
- Fragments numbered 0..n, each carries: message_id, fragment_seq, total_fragments
- Each fragment independently acknowledged at transport layer
- Full message reassembled at Layer 5 before delivery to application

## Layer 6 — Security Layer
Encryption, signing, and identity verification.

### Security Operations
- **Sign**: Ed25519 signature applied to every outbound message envelope
- **Verify**: Ed25519 signature verified on every received message before relay or delivery
- **Encrypt**: X25519 ECDH + HKDF → ChaCha20-Poly1305 for private messages
- **Decrypt**: reverse of above, only at final recipient (not relays)

### What Relays See
- message_id (visible, needed for deduplication)
- sender_id (visible, needed for routing)
- recipient_id (visible, needed for routing)
- priority (visible, needed for queueing)
- payload (ENCRYPTED — relays cannot read content)
- signature (visible, verified before relay)

### Key Management
Keys stored in platform secure storage:
- Android: Android Keystore (TEE-backed)
- iOS: Secure Enclave (hardware-backed)
- Linux: encrypted file store (no TEE equivalent)

### Security Failures
- Invalid signature: DROP + log (never relay unverified messages)
- Decryption failure: log + discard (do not surface garbled content to user)
- Unknown sender: relay is still permitted (sender unknown does not mean malicious)

## Layer 7 — Application API
Clean, platform-native API for apps.

### Core Operations
```rust
// Send a message
fn send_message(recipient: NodeId, content: MessageContent, priority: Priority) -> MessageId;

// Receive messages (callback)
fn on_message_received(callback: Fn(Message));

// Group operations
fn subscribe_group(group_id: GroupId);
fn send_group_message(group_id: GroupId, content: MessageContent) -> MessageId;

// Emergency
fn send_sos(location: Option<GpsCoord>, text: Option<String>) -> MessageId;

// Network status
fn get_network_status() -> NetworkStatus;
fn get_neighbor_count() -> u32;
fn get_delivery_status(message_id: MessageId) -> DeliveryStatus;
```

### Platform Adapters
- **Android (Kotlin)**: JNI bridge to Rust core. Foreground service for BLE. JobScheduler for background sync.
- **iOS (Swift)**: Swift-Rust FFI. Background fetch + BLE central/peripheral modes.
- **Desktop (TypeScript/Electron)**: IPC to Rust daemon. Full transport capabilities.
- **Edge (Linux daemon)**: Direct Rust binary. Systemd service. Full transport capabilities.

## Layer Interaction Rules
1. Higher layers may optimize lower layers (ML routing improves Layer 4 decisions)
2. Lower layers must never depend on higher layers for correctness
3. Layer 6 security is non-optional for private messages
4. Emergency messages (P0-P1) bypass normal queueing at Layer 5
5. Each layer has defined failure behavior when layer above/below fails
6. No layer may silently drop P0 messages — if drop is forced, log reason

## Failure Behavior Per Layer

| Layer | What Fails | Behavior | Recovery |
|-------|-----------|---------|---------|
| L1 Transport Adapter | OS revokes BLE access | Mark transport UNAVAILABLE, continue on others | OS grants access → restart adapter |
| L2 Discovery | All scans fail | Use last-known neighbor table | Discovery restored on OS permission restore |
| L3 Transport Manager | All transports unavailable | Queue messages, store locally | Any transport restores → flush queue |
| L4 Routing Engine | No route to destination | Store message, re-evaluate on new contact | New neighbor contact → re-route |
| L5 Message Engine | Storage full | Evict P7→P5, protect P0 | User clears storage OR P7-P5 TTL expires |
| L6 Security | Key not found | Cannot encrypt/decrypt, block send | User re-authenticates, key restored |
| L7 Application API | App crash | Daemon continues operating independently | App restart re-connects to daemon state |
