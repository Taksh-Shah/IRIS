# Control Flow

## Overview
IRIS has two planes of operation: data plane (user messages) and control plane (network management). This document covers the control plane: how nodes discover each other, exchange routing information, synchronize state, and manage emergency conditions.

## Control Plane vs Data Plane

| Plane | Purpose | Message Types | Priority |
|-------|---------|--------------|---------|
| Data | User messages | Text, image, voice, SOS | P0-P7 |
| Control | Network management | Discovery, routing gossip, sync, ACK, heartbeat | P_CONTROL |

Control message priority:
- `P_CONTROL_ROUTING`: between P0 and P1 (routing updates critical for emergency routing)
- `P_CONTROL_SYNC`: between P3 and P4 (sync is important but not life-critical)
- `P_CONTROL_HEARTBEAT`: between P5 and P6 (keep-alive, lowest control priority)

Control messages are size-limited (< 1KB) and encrypted+signed like data messages.

## Discovery Protocol

Discovery operates continuously in background, bounded by power mode.

### BLE Discovery (Primary)
```
Node starts up → BLE adapter initializes
         │
         ▼
Advertise IRIS presence:
  ├─ GATT service UUID: "5f4b5400-3c4b-4952-4953-000000000001"  // IRIS-DISCOVERY-v1
  ├─ Advertisement data: adv_id (rotating, 6 bytes) + protocol_version (1 byte)
  │   adv_id = HMAC-SHA256(privkey, floor(now/900))[0..6]  // Rotates every 15 minutes
  ├─ Advertisement interval: 100ms (foreground) / 1000ms (background iOS) / 5s (power saver)
  └─ GATT characteristic: CapabilityBundle (200 bytes, read-only)
         │
         ▼
Scan for peers:
  ├─ Scan for IRIS GATT service UUID
  ├─ Scan interval: 500ms on/500ms off (foreground), 2s on/8s off (background)
  ├─ On advertisement received: resolve adv_id to NodeId (from known contacts cache)
  └─ If unknown adv_id: initiate GATT connection to read CapabilityBundle
         │
         ▼
On GATT connection established to new peer:
  ├─ Read peer CapabilityBundle characteristic
  ├─ Verify CapabilityBundle signature
  ├─ Extract node_id from CapabilityBundle
  ├─ Add to neighbor table
  ├─ Trigger: message sync check (do I have messages for this node?)
  ├─ Trigger: routing table gossip exchange
  └─ If peer is gateway: update gateway availability
```

### Wi-Fi Aware Discovery (Android 8+)
```
Node starts → Wi-Fi Aware manager enabled
         │
         ▼
Publish IRIS service:
  ├─ Service name: "iris.mesh.v1"
  ├─ Service-specific info: adv_id (6 bytes) + protocol_version (1 byte)
  └─ Discovery window: every ~512ms (NAN standard timing)
         │
         ▼
Subscribe to IRIS service:
  ├─ Match filter: "iris.mesh.v1"
  ├─ On peer discovered: establish Wi-Fi Aware data path
  └─ Proceed with CapabilityBundle exchange via data channel
```

### Wi-Fi Direct Discovery (Android/Linux)
```
Node starts → Wi-Fi P2P manager enabled
         │
         ▼
Advertise via mDNS:
  ├─ Service type: "_iris._tcp.local."
  ├─ TXT record: adv_id, protocol_version, node_type
  └─ Port: 47221 (IRIS default, configurable)
         │
         ▼
Discover via mDNS:
  ├─ Browse "_iris._tcp.local."
  ├─ On peer found: TCP connection to peer:47221
  └─ CapabilityBundle exchange over TCP
```

### LoRa Discovery
```
Edge node with LoRa → periodic HELLO broadcast
  ├─ Interval: 60 seconds (conservative for spectrum sharing)
  ├─ HELLO payload: adv_id, node_type, gateway_capabilities, channel_info
  ├─ LoRa channel: 865.1 MHz (IN regulation), SF7, BW125, CR 4/5
  └─ Nodes hearing HELLO: add to LoRa neighbor table, reply with own HELLO
         │
         ▼
LoRa neighbor discovery complete → CapabilityBundle exchange
  ├─ Exchange via LoRa data channel
  ├─ Rate limited: LoRa duty cycle 1% (max 36s/hour transmit)
  └─ Capability exchange amortized over multiple HELLO cycles
```

## Routing Control Protocol

Nodes exchange routing information on every significant contact. This gossip propagates knowledge of the network beyond direct neighbors.

### Routing Gossip Payload
```rust
struct RoutingGossip {
    sender_id:           NodeId,
    timestamp:           u64,
    delivery_probs:      Vec<(NodeId, f32)>,    // PRoPHET estimates I have
    gateway_sightings:   Vec<GatewaySighting>,   // Gateways I know about
    network_state:       NetworkState,           // NORMAL/DEGRADED/CRISIS/EMERGENCY
    signature:           [u8; 64],               // Ed25519 signature
}

struct GatewaySighting {
    gateway_node_id: NodeId,
    gateway_type:    GatewayType,
    quality:         f32,          // 0.0-1.0
    last_seen:       u64,          // Timestamp
    seen_by:         NodeId,       // Which node saw this gateway
}
```

### Gossip Exchange Sequence
```
A contacts B:
  1. A sends RoutingGossip to B (my delivery probability table)
  2. B updates its routing table with A's gossip
     - For each destination D in A's gossip:
       P(B→D) = max(P(B→D), P(A→D) * transitivity_factor)
       transitivity_factor = 0.25 (configurable)
  3. B sends RoutingGossip to A
  4. A updates its routing table with B's gossip
```

### Gossip Fanout Control
To prevent gossip flooding:
- Gossip limited to 3 hops (each gossip message tracks hop count)
- Gossip deduplicated by (sender_id, timestamp) pair
- Gossip size limited: max 100 delivery probability entries per message

## Synchronization Protocol

On contact between nodes, they synchronize stored messages to maximize delivery.

### Full Sync Sequence
```
A contacts B (new or returning contact):
         │
         ▼
Step 1: Bloom Filter Exchange
  ├─ A sends: sync_request { bloom_filter_of_stored_messages, my_node_id }
  └─ B sends: sync_request { bloom_filter_of_stored_messages, my_node_id }
         │
         ▼
Step 2: Message Transfer (simultaneous both directions)
  ├─ A sends B messages:
  │    ├─ Messages in A's store where B's Bloom filter says NOT present
  │    ├─ Filter: only send messages B has capacity for (based on B's capability bundle)
  │    ├─ Filter: only send messages B is likely to route toward destination
  │    └─ Respect sync_budget: max bytes = min(avail_bandwidth × 30s, 10MB)
  └─ B sends A messages:
       ├─ Messages in B's store where A's Bloom filter says NOT present
       └─ Same filters and budget apply
         │
         ▼
Step 3: Routing Table Update
  ├─ Exchange RoutingGossip (see above)
  └─ Update delivery probabilities for newly discovered paths
         │
         ▼
Step 4: Completion
  ├─ Log sync metrics: messages exchanged, bytes transferred, duration
  └─ Update contact record in temporal_contacts table
```

### Incremental Sync (Re-contact)
When nodes have contacted recently (< 30 minutes):
- Skip Bloom filter exchange (use cached knowledge)
- Only exchange messages created since last contact
- Faster: reduces overhead for frequently-contacting neighbors

### Sync Budget Enforcement
Sync is bounded to prevent monopolizing transport:
- Max sync duration: 30 seconds per contact
- Max sync bytes: 10MB per contact (configurable)
- Priority: P0-P2 messages always synced first, then P3-P7 by remaining budget
- Budget split: 50% local→remote, 50% remote→local (prevents one-sided flooding)

## Emergency Broadcast State Machine

Used by authorized nodes (government, NDRF, operator) to broadcast emergency alerts.

### Authority Certificate Chain
```
Root CA (IRIS Emergency Authority)
    ├─ State Authority (e.g., Gujarat SDMA)
    │    └─ Operator Node (SDMA-certified device)
    └─ National Authority (NDMA)
         └─ Operator Node (NDMA-certified device)
```

Any node with a valid authority certificate can originate emergency broadcasts. Certificate chain validated by every relay before forwarding.

### Broadcast Flow
```
AUTHORITY_NODE originates emergency broadcast:
  ├─ Message type: EMERGENCY_BROADCAST
  ├─ Recipient: EMERGENCY_BROADCAST address (0x00...01)
  ├─ Signed with: authority_private_key
  ├─ Includes: authority_certificate_chain (embedded in payload)
  └─ Priority: P_EMERGENCY_BROADCAST (above P0 in queue)
         │
         ▼
Broadcast propagates via Flood:
  ├─ Every receiving node: verify authority certificate chain
  │    ├─ Valid chain: relay immediately to ALL neighbors via ALL transports
  │    └─ Invalid chain: DROP + log UNAUTHORIZED_BROADCAST_ATTEMPT
  ├─ No hop limit for emergency broadcasts (flood without bound)
  └─ Deduplication prevents infinite loops (seen_message_cache)
         │
         ▼
Nodes in affected area: display alert
  ├─ Persistent notification (high-priority system notification)
  ├─ Alert includes: message text, severity, area (if included), issuer
  └─ User cannot dismiss without acknowledgment
         │
         ▼
Authority sends CANCEL broadcast (when emergency resolves):
  ├─ References original broadcast's message_id
  ├─ Signed by same authority
  └─ Receiving nodes: suppress further relay of original broadcast_id
```

### Unauthorized Broadcast Protection
- All nodes verify authority certificate chain before relaying
- Invalid certificate: DROP + log
- Certificate revocation: propagated via signed revocation notice (same flood mechanism)
- Rogue broadcasts: logged, reported to operator when Internet available

## Network Crisis State Machine

Tracks and propagates network-wide crisis conditions.

```
      NORMAL
  (all systems operational)
         │
         │ Triggers:
         │  - SOS message rate > threshold (5 P0 messages/hour)
         │  - Majority of gateways fail
         │  - Operator command
         ▼
    DEGRADED
  (reduce bandwidth-heavy operations)
  - Suspend P6-P7 message relay
  - Increase routing aggressiveness
  - Alert application layer
         │
         │ Triggers:
         │  - >80% of routing paths fail
         │  - Operator command
         ▼
      CRISIS
  (emergency routing only)
  - Suspend P5-P7 message relay
  - Focus resources on P0-P2
  - All nodes enter EMERGENCY_OVERRIDE power mode
  - Increase sync frequency
         │
         │ Triggers:
         │  - Operator explicit activation
         │  - Catastrophic event detection (extreme SOS surge)
         ▼
    EMERGENCY
  (maximum relay, all resources)
  - Ignore battery policy (emergency override)
  - Activate satellite if available
  - Epidemic routing for all P0-P2 messages
  - Report to external authorities when any Internet available
```

### State Propagation
Crisis state is gossiped via RoutingGossip. When a node hears from neighbors that network_state > its own state:
- Adopt the higher crisis state
- Propagate to its own neighbors
- Crisis state propagates across the network within minutes

### State Transition Back to NORMAL
- Manual: operator command sent via authorized node
- Automatic: SOS rate drops below threshold for 1 hour AND gateways return
- Cooldown: 30 minutes minimum in each state before downgrade

## Heartbeat and Keep-Alive Protocol

For long-lived connections (Wi-Fi, Ethernet, Internet), heartbeats maintain connection state.

### Heartbeat Format
```
Heartbeat {
    sender_id:       NodeId,
    timestamp:       u64,
    network_state:   NetworkState,
    relay_queue_len: u32,          // How many messages waiting to forward
    signature:       [u8; 64],
}
```

Interval: 30 seconds for active connections, 5 minutes for long-range LoRa.
On heartbeat timeout (3 missed): mark connection STALE, trigger rediscovery.

## Control Message Rate Limits

To prevent control traffic from overwhelming data capacity:

| Message Type | Max Rate | Per Neighbor |
|-------------|---------|-------------|
| CapabilityBundle | 1/5min | Per neighbor |
| RoutingGossip | 1/60s | Per neighbor |
| SyncRequest | 1/30s | Per neighbor |
| Heartbeat | 1/30s | Per connection |
| EmergencyBroadcast | Unlimited (authority only) | N/A |

Rate limits suspended for P_CONTROL_ROUTING messages during CRISIS or EMERGENCY state.
