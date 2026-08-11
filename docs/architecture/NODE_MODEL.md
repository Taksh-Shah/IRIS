# Node Model

## What Is a Node
Any device running IRIS software capable of: originating, receiving, or relaying messages. Every node has a cryptographic identity (Ed25519 key pair). Nodes are identified by their public key hash (32 bytes, BLAKE3 of the Ed25519 public key).

A node may simultaneously be:
- An originator (creates messages)
- A recipient (receives messages addressed to it)
- A relay (forwards messages for others, with user consent)
- A gateway (bridges mesh to external network)

## Node Types

### User Node (Phone/Tablet)
- Has UI, user account associated with cryptographic identity
- Battery constrained: subject to OS background restrictions
- Primary role: originate and receive messages
- Secondary role: relay messages when relay_enabled = true (user consent required)
- Typical transports: BLE, Wi-Fi Direct/Aware, Internet
- Storage: bounded by app quota (default 500MB)
- Power mode: changes dynamically based on battery level
- Background behavior: BLE advertising continues (limited), active relay requires foreground service

### Relay Node (Headless)
- No user interface, no user account
- Mains-powered or solar-powered — no battery constraint
- Always-on: designed for 24/7 operation
- Primary role: relay, store-and-forward
- May also function as gateway
- Typical transports: BLE, Wi-Fi, LoRa, Ethernet
- Storage: large (10GB+)
- Managed by: operator, not end user

### Gateway Node
- Has connectivity to external network (Internet, LoRa backbone, satellite)
- Bridges mesh to external network
- Advertises gateway capability in CapabilityBundle
- May be any other node type (user phone, relay, dedicated hardware)
- Critical network role during disasters: provides path out of isolated mesh

### Vehicle Node
- Mobile store-carry-forward (DTN mule)
- Traverses physically disconnected regions (disaster zones, remote areas)
- High storage capacity (10GB+) to carry messages between partitions
- Multi-transport: BLE + Wi-Fi to collect from nodes, LoRa for range
- Operates without Internet: pure mesh
- Examples: NDRF vehicle, supply convoy, ambulance

### Edge Node (Deployed Infrastructure)
- Raspberry Pi 4 / CM4 or similar single-board computer
- Fixed location (rooftop, community building, tower)
- Solar-powered for disaster resilience
- Transports: LoRa (long range) + BLE + Wi-Fi (local coverage) + Ethernet (Internet gateway)
- Managed remotely via SSH when Internet available; BLE config interface when not
- Reference configuration documented in EDGE_ARCHITECTURE.md

## Node State Machine

```
          ┌─────────────────────────────────────┐
          │           INITIALIZING              │
          │  Loading keys, starting adapters    │
          └──────────────┬──────────────────────┘
                         │ startup complete
                         ▼
          ┌─────────────────────────────────────┐
          │              ONLINE                 │◄──────────────────────┐
          │  All transports active              │                       │
          │  Full routing, relay, discovery     │                       │ recovery
          └──────┬──────────────────────────────┘                       │
                 │ transport loss / degradation                          │
                 ▼                                                       │
          ┌─────────────────────────────────────┐                       │
          │            DEGRADED                 │                       │
          │  Some transports failed             │                       │
          │  Routing with available transports  │                       │
          │  Alert sent to application layer    │                       │
          └──────┬──────────────────────────────┘                       │
                 │ all transports fail                                   │
                 ▼                                                       │
          ┌─────────────────────────────────────┐                       │
          │             OFFLINE                 │─────────────────────►─┘
          │  No connectivity                    │  any transport restores
          │  Store all messages                 │
          │  Wait for any contact               │
          └──────┬──────────────────────────────┘
                 │ P0 received OR explicit trigger
                 ▼
          ┌─────────────────────────────────────┐
          │              CRISIS                 │
          │  Emergency mode                     │
          │  P0-P2 messages only                │
          │  Maximum relay aggressiveness       │
          └──────┬──────────────────────────────┘
                 │ battery critical
                 ▼
          ┌─────────────────────────────────────┐
          │         BATTERY_CRITICAL            │
          │  Emergency relay only               │
          │  No BLE scan (saves power)          │
          │  Only relay P0 received messages    │
          └─────────────────────────────────────┘
```

### State Transition Triggers

| From | To | Trigger |
|------|----|---------|
| INITIALIZING | ONLINE | All configured transports started |
| ONLINE | DEGRADED | One or more transports fail |
| DEGRADED | ONLINE | Failed transports recover |
| DEGRADED | OFFLINE | All transports fail |
| OFFLINE | DEGRADED | One transport recovers |
| OFFLINE | CRISIS | P0 SOS received via any path OR operator command |
| CRISIS | ONLINE | Explicit deactivation by authorized node |
| ANY | BATTERY_CRITICAL | Battery < 5% |
| BATTERY_CRITICAL | ONLINE | Battery > 15% (hysteresis) |

## Node Capability Advertisement
On discovery, nodes exchange a signed CapabilityBundle. This tells neighbors what the node can do and prevents routing to incapable nodes.

### CapabilityBundle Schema (CBOR)
```
CapabilityBundle {
    node_id:             bytes(32),    // BLAKE3(Ed25519_pubkey)
    node_type:           uint,         // 0=USER, 1=RELAY, 2=GATEWAY, 3=VEHICLE, 4=EDGE
    transports_available: [uint],      // List of available transport types
    gateway_types:       [uint],       // 0=NONE, 1=INTERNET, 2=LORA, 3=SATELLITE, 4=CELLULAR
    storage_available_mb: uint,        // Available storage for relay
    battery_level:       int,          // 0-100 percent, -1 if mains powered
    relay_enabled:       bool,         // User has consented to relay for others
    max_relay_hops:      uint,         // Max hops this node will relay
    software_version:    string,       // e.g. "0.1.0"
    protocol_version:    uint,         // e.g. 1
    signature:           bytes(64),    // Ed25519 signature of above fields
}
```

Bundle size target: < 200 bytes to fit in BLE advertisement GATT response.

### Capability Caching
Capability bundles are cached for 5 minutes. Re-exchanged on:
- New contact after node absence
- Protocol version change (detected by advertisement)
- Explicit capability refresh request

## Node Storage Model

Each node maintains five persistent storage components:

### 1. Message Store
SQLite database:
```sql
CREATE TABLE messages (
    message_id        TEXT PRIMARY KEY,   -- UUIDv7
    sender_id         BLOB NOT NULL,      -- 32-byte node_id
    recipient_id      BLOB NOT NULL,      -- 32-byte address
    priority          INTEGER NOT NULL,
    status            TEXT NOT NULL,      -- PENDING/IN_TRANSIT/DELIVERED/EXPIRED/FAILED
    payload           BLOB,              -- Encrypted payload
    signature         BLOB NOT NULL,     -- Ed25519 signature
    created_at        INTEGER NOT NULL,  -- Unix timestamp
    expires_at        INTEGER NOT NULL,  -- Unix timestamp
    ack_received      INTEGER DEFAULT 0,
    hop_count         INTEGER DEFAULT 0,
    relay_count       INTEGER DEFAULT 0
);
CREATE INDEX idx_priority_expires ON messages(priority, expires_at);
CREATE INDEX idx_status ON messages(status);
```

### 2. Seen Message Cache
Purpose: deduplication — never relay or store a message already processed.
Implementation: two-layer cache
- Layer 1: Bloom filter (space-efficient, probabilistic, no false negatives)
  - 100,000 entries, 1% false positive rate → ~120KB memory
  - Bloom filter reset every 24h to prevent false positives from accumulation
- Layer 2: LRU set of last 10,000 exact message_ids (for false positive resolution)

### 3. Routing State
SQLite:
- Contact history: (node_id, contact_time, transport, duration, messages_exchanged)
- Delivery probability estimates: (destination_id, probability, last_updated)
- These tables persist across app restarts; routing adapts to historical patterns

### 4. Neighbor Table
In-memory (not persisted — rebuilt from discovery on restart):
- Current neighbors with transport info, capability bundles, signal strength
- Expiry handled by background timer

### 5. Identity Store
- Own Ed25519 key pair (in platform secure storage: Keystore/Secure Enclave)
- Contact directory: (node_id, display_name, public_key, verified_at)
- Group memberships: (group_id, group_name, group_key, member_list)
- Trust anchors: public keys of emergency authorities

## Power Modes

| Mode | BLE Scan Interval | Wi-Fi Direct | Wi-Fi Aware | Routing Activity | Relay | Threshold |
|------|------------------|--------------|-------------|-----------------|-------|-----------|
| FULL | 500ms | Active | Active | Full PRoPHET | Yes, all priorities | Battery > 50% |
| BALANCED | 2s | On demand | On demand | Full PRoPHET | Yes, P0-P4 | Battery 20-50% |
| LOW_BATTERY | 10s | Off | Off | Essential only | P0-P2 only | Battery 10-20% |
| CRITICAL | 30s | Off | Off | P0 only | P0 only | Battery 5-10% |
| EMERGENCY_OVERRIDE | 500ms | Active | Active | Maximum | All priorities | P0 SOS active |

Emergency override ignores battery policy for P0 messages. This is a deliberate trade-off: a dead phone cannot help anyone, but a P0 SOS needs maximum propagation right now.

### Power Mode Enforcement
Power mode is managed by the PowerManager component:
- Subscribes to OS battery change events
- Changes mode when threshold crossed (with hysteresis: ±2% to prevent rapid oscillation)
- Notifies TransportManager and RoutingEngine of mode change
- Logs mode transitions for diagnostics

## Node Identity and Key Rotation

### Initial Setup
1. Generate Ed25519 key pair on first launch
2. Store private key in platform secure storage (TEE/Secure Enclave)
3. Derive node_id = BLAKE3(public_key)[0..32]
4. Generate human-readable display address for sharing

### Key Rotation Policy
- Recommended: yearly rotation for long-term identity continuity
- Required: on device compromise, on key export, on device transfer
- Key rotation: generate new key pair, publish signed rotation notice linking old → new key
- Rotation notice propagated via mesh, stored by contacts
- Old key remains valid for message decryption for TTL of oldest stored message

### Multi-Device Identity (Future)
- One user identity (master key) can authorize multiple device keys
- Device keys are sub-keys signed by master
- Receiving node validates sub-key chain
- Not implemented in v1; tracked in roadmap

## Node Bootstrapping (First Run in New Area)

When a node enters an area with no known contacts:
1. Begin BLE advertising immediately (always-on)
2. Begin BLE scanning (discover peers with no prior contact)
3. On first peer discovery: exchange CapabilityBundles
4. Receive routing table gossip from peer
5. Node now has partial view of local network
6. Continue discovery to expand neighbor table

Bootstrapping is fully automatic. No manual configuration required.

## Node Health Monitoring

Nodes track internal health metrics:
- transport_errors per transport per hour
- relay_queue_depth (messages waiting to forward)
- storage_utilization percent
- message_drop_rate (messages dropped vs forwarded)
- discovery_rate (new neighbors per hour)

Health metrics exposed to operator tools via:
- Local JSON API (on edge nodes): `GET http://localhost:8080/health`
- BLE characteristic (on mobile): read-only health summary
- Telemetry push (when Internet available): anonymous metrics to operator dashboard

Threshold alerts:
- storage_utilization > 90%: alert, begin eviction
- relay_queue_depth > 10000: alert, check transport health
- transport_errors > 100/hour for one transport: mark transport DEGRADED
