# IRIS System Architecture

**Document ID:** IRIS-ARCH-SYS-001  
**Version:** 1.0

---

## 1. Architectural Overview

IRIS is a layered, multi-transport, disruption-tolerant communication system. The architecture separates physical radio concerns from routing logic, routing from message semantics, and message semantics from application behavior.

```
┌─────────────────────────────────────────────────────────┐
│                    APPLICATIONS                          │
│  SOS App  │  Messenger  │  Location  │  Dashboard SDK   │
├─────────────────────────────────────────────────────────┤
│                 APPLICATION API (Layer 6)                │
│              IRISClient SDK (Kotlin/Swift/TS)            │
├─────────────────────────────────────────────────────────┤
│             IDENTITY & SECURITY (Layer 5)                │
│    Ed25519 Identity │ Noise Protocol │ Key Management    │
├─────────────────────────────────────────────────────────┤
│              MESSAGE ENGINE (Layer 4)                    │
│  Store-Carry-Forward │ Deduplication │ TTL │ Priority    │
├─────────────────────────────────────────────────────────┤
│           ROUTING & FORWARDING (Layer 3)                 │
│   Epidemic │ Spray-and-Wait │ PRoPHET │ Direct Route     │
├─────────────────────────────────────────────────────────┤
│        DISCOVERY & NEIGHBOR MGMT (Layer 2)               │
│    BLE Scanning │ Wi-Fi Peer Discovery │ LoRa Beacon     │
├─────────────────────────────────────────────────────────┤
│           TRANSPORT ABSTRACTION (Layer 1)                │
│                  TransportManager                         │
│  BLE │ Wi-Fi Direct │ Wi-Fi Aware │ LoRa │ Satellite │ IP│
├─────────────────────────────────────────────────────────┤
│              PHYSICAL LAYER (Layer 0)                    │
│  2.4GHz Radio │ Sub-GHz Radio │ Optical │ Satellite Link │
└─────────────────────────────────────────────────────────┘
```

---

## 2. Core Components

### 2.1 TransportManager

The TransportManager is the central abstraction for all radio and network transports. It presents a unified interface to upper layers while managing the heterogeneous transport ecosystem.

```
TransportManager
├── BLETransport
│   ├── BLEAdvertiser (discovery beacon)
│   ├── BLEScanner (peer discovery)
│   └── GATTClient/Server (data transfer)
├── WiFiDirectTransport
│   ├── WifiP2pManager (Android API)
│   └── NetworkInterface (TCP/UDP over P2P)
├── WiFiAwareTransport (Android 8+)
│   ├── WifiAwareSession
│   └── NetworkSpecifier
├── LoRaTransport
│   ├── SerialInterface (USB/SPI to LoRa module)
│   ├── PacketFragmenter
│   └── FrequencyPlanner
├── SatelliteTransport
│   ├── ModemInterface
│   └── BandwidthThrottler
└── InternetTransport
    ├── WebSocketRelay
    └── TorTransport (optional)
```

**Transport Selection Algorithm:**

```
function select_transport(message, available_transports, peer):
    if message.priority == P0:
        return all_transports(available_transports)  // multipath
    
    candidates = rank_by(available_transports, [
        throughput_match(message.payload_size),
        power_cost,
        current_congestion,
        peer_capability
    ])
    
    if battery_level < 20%:
        candidates = filter(candidates, low_power_only)
    
    return candidates[0]
```

### 2.2 Message Engine

The Message Engine handles all message lifecycle operations. It is the persistence and routing decision core of IRIS.

```
MessageEngine
├── MessageStore
│   ├── SQLite (persistent storage, Android/iOS/Desktop)
│   ├── PriorityQueue (in-memory scheduling)
│   └── StorageManager (eviction, TTL GC)
├── DeduplicationEngine
│   ├── BloomFilter (seen message IDs)
│   └── LRUCache (recent message IDs)
├── TTLEnforcer
│   ├── TimeBasedExpiry
│   └── HopCountExpiry
├── PriorityScheduler
│   └── WeightedFairQueue (P0 preemptive)
└── ForwardingEngine
    ├── EpidemicForwarder (P0)
    ├── SprayWaitForwarder (P1-P2)
    ├── PRoPHETForwarder (P3-P4)
    └── DirectForwarder (point-to-point)
```

### 2.3 Security Subsystem

```
SecuritySubsystem
├── IdentityManager
│   ├── Ed25519KeyPair (long-term identity)
│   ├── X25519EphemeralKeys (session keys)
│   └── KeyStore (Android Keystore / iOS Secure Enclave)
├── NoiseProtocol
│   ├── HandshakeEngine (Noise_XX pattern)
│   └── CipherState (ChaCha20-Poly1305)
├── MessageSigner
│   └── Ed25519Sign/Verify
├── MessageEncryptor
│   └── ChaCha20-Poly1305 AEAD
└── KeyDistribution
    ├── PublicKeyAnnouncement (via mesh)
    └── OutOfBandVerification (QR code)
```

### 2.4 Discovery Subsystem

```
DiscoverySubsystem
├── BLEDiscovery
│   ├── ServiceAdvertiser (IRIS UUID: iris-svc-00001)
│   └── PeerScanner
├── WiFiAwareDiscovery
│   ├── PublishSession
│   └── SubscribeSession
├── NeighborTable
│   ├── PeerRecord (node_id, transports, rssi, last_seen)
│   └── ContactLogger (for PRoPHET metrics)
└── CapabilityExchange
    └── HelloProtocol (on contact: exchange capabilities, bloom filters)
```

---

## 3. Message Flow: Origination to Delivery

### 3.1 Normal Flow

```
Application
    │
    │ IRISClient.send(message, recipient, priority)
    ▼
SecuritySubsystem
    │ 1. Sign message with Ed25519 private key
    │ 2. Encrypt payload with recipient's X25519-derived key
    │ 3. Build message envelope (CBOR encoded)
    ▼
MessageEngine
    │ 4. Assign message ID (UUID v4)
    │ 5. Set TTL (default per priority)
    │ 6. Store message in MessageStore
    │ 7. Add to priority queue
    ▼
ForwardingEngine
    │ 8. Query NeighborTable for reachable peers
    │ 9. Select routing algorithm based on priority
    │ 10. Determine target peers
    ▼
TransportManager
    │ 11. Select transport(s) for each peer
    │ 12. Fragment if required (LoRa/BLE MTU)
    │ 13. Transmit
    ▼
[Relay Node receives]
    │
    ▼
TransportManager (relay)
    │ 14. Receive fragments, reassemble
    ▼
MessageEngine (relay)
    │ 15. Deduplication check (bloom filter + LRU)
    │ 16. Verify signature (if public key available)
    │ 17. Check TTL validity
    │ 18. Store message
    │ 19. Add to forwarding queue
    ▼
[repeat from step 8 at relay node]
    ...
    ▼
[Destination Node receives]
    │
    ▼
SecuritySubsystem (destination)
    │ 20. Verify Ed25519 signature
    │ 21. Decrypt payload with own private key
    ▼
MessageEngine (destination)
    │ 22. Deliver to application layer
    │ 23. Send delivery ACK (if confirmed delivery policy)
    ▼
Application (destination)
    │ 24. Display to user
```

### 3.2 Emergency Override Flow (P0 SOS)

P0 messages bypass normal queue scheduling:

```
SOS Triggered
    │
    ▼
Priority Preemption
    │ All P4-P7 transmissions suspended immediately
    │ All P1-P3 transmissions paused (resume after P0 batch)
    ▼
TransportManager
    │ All available transports activated simultaneously
    │ BLE: advertising switched to emergency mode (1ms interval)
    │ Wi-Fi Direct: immediate connection attempt to all visible peers
    │ LoRa: immediate transmission on emergency channel
    ▼
EpidemicForwarder
    │ Message forwarded to EVERY reachable node
    │ No delivery probability threshold (forward unconditionally)
    │ Custody mode: relay nodes issue custody ACK
    ▼
[No-drop policy: P0 messages held in storage until:
    a) Delivery ACK received from destination, OR
    b) TTL expires (default: 72 hours for SOS)]
```

---

## 4. Gateway Architecture

Gateways are nodes that bridge two or more different network segments.

```
Internet Gateway:
Local Mesh ──[BLE/WiFi]──► Gateway ──[TCP/TLS]──► Internet ──► Remote Node

LoRa Gateway:
Local Mesh ──[BLE]──► Gateway ──[LoRa 865MHz]──► Remote LoRa Node

Satellite Gateway:
Local Mesh ──[BLE/WiFi]──► Gateway ──[Ka-band]──► Satellite ──► Anywhere
```

Gateway selection from a mobile node's perspective:
```
1. Discover available gateways via HelloProtocol capability exchange
2. Each gateway advertises: type, throughput, cost, reliability score
3. Gateway selection = argmax(reliability × throughput / cost) weighted by message priority
4. P0: use ALL available gateways simultaneously
5. P4-P7: use cheapest gateway that meets throughput requirement
```

---

## 5. Edge Node Architecture

Edge nodes are fixed-location IRIS relay/gateway nodes.

```
Edge Node Components:
┌───────────────────────────────────────────────────────┐
│  Raspberry Pi 4B                                       │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐ │
│  │ BLE (onboard)│  │ Wi-Fi 5GHz   │  │ LoRa HAT     │ │
│  │ Range: 200m  │  │ Range: 500m  │  │ Range: 30km  │ │
│  └─────────────┘  └──────────────┘  └──────────────┘ │
│  ┌─────────────────────────────────────────────────┐  │
│  │ IRIS Core (Rust)                                  │  │
│  │ MessageStore: 32GB SD card                        │  │
│  │ Uptime: 24/7 (battery backed)                     │  │
│  └─────────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────────┐  │
│  │ Power: Solar + LiFePO4 (30+ day autonomy)        │  │
│  └─────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────┘
```

Edge nodes provide:
- **Persistent relay:** Messages stored indefinitely (until TTL)
- **Long-range extension:** LoRa bridging beyond BLE range
- **Gateway capability:** Internet, satellite if available
- **Time synchronization:** NTP sync when internet available; distribute time to mesh

---

## 6. Control Plane vs. Data Plane

**Control Plane** (who talks to whom, routing state):
- Peer discovery (BLE advertising, Wi-Fi Aware)
- Capability exchange (HelloProtocol)
- Routing table construction (PRoPHET encounter counters)
- ACK propagation
- Gateway advertisement

**Data Plane** (actual message forwarding):
- Message transmission over selected transports
- Fragment handling
- Relay forwarding
- Store-and-forward

This separation allows routing decisions to be updated without disrupting in-flight data, and allows different security models for control vs. data traffic.

---

## 7. Failure Architecture Summary

| Failure | Detection | Response |
|---------|-----------|----------|
| Single transport failure | TransportManager health check | Switch to alternative transport |
| Gateway failure | HelloProtocol timeout | Select next gateway; store messages |
| Node disappearance | Neighbor table expiry (30s) | Route around; store for later |
| Storage full | Storage level monitor | Evict P7→P6→P5 (never P0-P2) |
| Battery critical | Battery API | Suspend P4-P7; keep P0-P3 |
| Network partition | No delivery ACK, no peers | Store messages; wait for contact |
| Clock skew > tolerance | NTP comparison | Use conservative TTL window |

The system degrades gracefully: fewer transports means slower propagation, not silence. Storage full means low-priority messages lost, not high-priority messages lost. Battery critical means background functions suspend, not SOS disabled.
