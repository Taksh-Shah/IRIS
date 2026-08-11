# Network Graph Model

## Overview
The IRIS network is modeled as a time-varying directed weighted graph G(t) = (V, E(t)) where:
- V = set of known nodes (discovered through direct contact or gossip)
- E(t) = set of active or predicted links at time t
- Each edge has weight vector = (latency_ms, bandwidth_bps, reliability, battery_cost)

The graph is not globally known — each node maintains a local view of the graph bounded by its discovery horizon (contacts up to N hops away). In large or sparse networks, the local view may be very partial.

## Graph Data Structures

### Core Graph
```rust
pub struct NetworkGraph {
    nodes: HashMap<NodeId, NodeInfo>,
    links: HashMap<(NodeId, NodeId), LinkInfo>,
    contact_history: HashMap<NodeId, Vec<ContactRecord>>,
    delivery_probabilities: HashMap<(NodeId, NodeId), DeliveryProbability>,
    partition_detector: PartitionDetector,
    last_updated: Timestamp,
    local_node_id: NodeId,
}

pub struct NodeInfo {
    node_id: NodeId,
    node_type: NodeType,
    capability: CapabilityBundle,
    first_seen: Timestamp,
    last_seen: Timestamp,
    hop_distance: u8,         // 0 = self, 1 = direct neighbor, 2 = neighbor-of-neighbor, etc.
    source_transport: TransportType,  // How we learned about this node
}

pub struct LinkInfo {
    from_node: NodeId,
    to_node: NodeId,
    transport: TransportType,
    latency_ms: u32,
    bandwidth_bps: u32,
    reliability: f32,         // 0.0-1.0 (1.0 = perfectly reliable)
    battery_cost: CostClass,  // LOW / MEDIUM / HIGH
    signal_strength_dbm: i16,
    last_seen: Timestamp,
    expires_at: Timestamp,
    is_active: bool,
}

pub struct ContactRecord {
    peer_node_id: NodeId,
    contact_start: Timestamp,
    contact_end: Option<Timestamp>,
    transport: TransportType,
    messages_exchanged: u32,
    bytes_exchanged: u64,
}

pub struct DeliveryProbability {
    destination: NodeId,
    probability: f32,           // PRoPHET estimate 0.0-1.0
    last_updated: Timestamp,
    sample_count: u32,
    via_node: Option<NodeId>,   // Best next hop for this probability
}
```

### Directed vs Undirected Edges
Edges are directed. A→B does not imply B→A has the same characteristics:
- BLE: typically bidirectional with similar characteristics
- LoRa: may be asymmetric (terrain, power differences)
- Satellite: typically asymmetric (downlink >> uplink)

When forward channel exists without confirmed reverse: mark edge as `unidirectional = true`. Messages can be sent A→B but ACKs may not return via same path.

## Graph Updates

### Link Addition
A link is added when:
- Node A discovers Node B via any transport (direct observation)
- Node A receives gossip from Node C that C has a link to B (indirect, 2-hop topology)

Gossiped links are marked with lower confidence and flagged with `is_gossip = true`. Gossip links expire faster than directly observed links.

### Link Removal
Links are removed (marked inactive) when:
- Neighbor not seen for `link_timeout`:
  - BLE: 30 seconds
  - Wi-Fi: 15 seconds
  - Wi-Fi Aware: 30 seconds
  - LoRa: 5 minutes
  - Internet: 60 seconds (TCP keepalive failure)
- Transport adapter reports explicit disconnection

Links are not immediately deleted — they remain in graph with `is_active = false` for contact history and PRoPHET calculations.

### Link Aging (Reliability Decay)
Reliability degrades over time without contact using exponential decay:
```
reliability(t) = reliability_at_last_contact * exp(-decay_rate * (t - last_contact_time))
decay_rate = ln(2) / half_life_seconds
half_life for BLE:      3600s  (1 hour)
half_life for LoRa:     7200s  (2 hours)
half_life for Internet: 1800s  (30 minutes)
```
When reliability falls below 0.05, remove link from active routing consideration.

## Topology Discovery

### 1-Hop Topology (Direct)
Directly discovered via transport adapter scan. Highly reliable. Updated continuously.

### 2-Hop Topology (Gossip)
Each node advertises its known 1-hop neighbors during capability exchange. This gives a 2-hop view.
- 2-hop links have `hop_distance = 2` and `is_gossip = true`
- Reliability confidence is 0.6 × the reported direct link reliability (discount for gossip uncertainty)

### N-Hop Topology (Routing Table Gossip)
During synchronization, nodes share their routing table summaries (not full tables — summaries with delivery probabilities). This extends topology awareness beyond 2 hops.
- Used for PRoPHET routing only
- Not stored in link table (too large)
- Stored in delivery_probabilities table instead

### Topology Incompleteness
In large sparse networks, local topology is typically very incomplete. IRIS is designed to operate with incomplete topology:
- Routing degrades gracefully to contact-based probability
- Store-carry-forward covers gaps
- No algorithm assumes full topology knowledge

## Partition Detection

A network partition is detected when:
- No path exists (via BFS) from local node to any known gateway
- All known cross-region links are inactive
- Previously reachable cluster of nodes becomes unreachable

### Partition Detection Algorithm
```
PartitionDetector runs every 60 seconds:
1. BFS from local_node_id over active links
2. Identify reachable_set (nodes reachable)
3. Identify unreachable_set (previously known nodes now unreachable)
4. If unreachable_set is non-empty and was previously in reachable_set:
   → emit PartitionEvent { isolated_nodes: unreachable_set, detected_at: now }
5. If no gateway in reachable_set:
   → emit GatewayLostEvent
```

### Partition Response
When partition detected:
- Increase epidemic routing aggressiveness (higher spray count)
- Store more messages (extend eviction threshold)
- Alert application layer (user sees "Network partitioned" indicator)
- Begin logging vehicle contact opportunities (for DTN mule routing)

## Network Healing

When a previously isolated node appears:
1. Discovery detects new/returning neighbor
2. Check: was this neighbor previously unreachable?
3. If yes: trigger `NetworkHealingSync`
   - Exchange Bloom filters of stored messages (efficient differential sync)
   - Transfer messages each node lacks
   - Update routing tables with new contact information
   - Recalculate delivery probabilities
4. Emit `PartitionHealed` event to application layer
5. Resume normal routing (reduce epidemic aggressiveness)

### Bloom Filter Exchange Protocol
```
Sync step 1: A sends Bloom filter of its stored message IDs
Sync step 2: B sends Bloom filter of its stored message IDs
Sync step 3: A sends messages likely not in B's Bloom filter
Sync step 4: B sends messages likely not in A's Bloom filter
Note: Bloom filters have ~1% false positive rate → ~1% of messages re-transferred unnecessarily
      False positive rate is acceptable; it's better than missing messages
```

## Graph Memory Constraints

Mobile nodes have bounded graph storage:

| Constraint | Default | Configurable |
|-----------|---------|-------------|
| Max nodes tracked | 1,000 | 500-5,000 |
| Max links per node | 20 | 10-50 |
| Max contact records per node | 100 | 50-500 |
| Max delivery probability entries | 10,000 | 1,000-50,000 |
| Max graph age | 7 days | 1-30 days |

### Eviction Policy
When memory constraints are reached:
1. Evict nodes not seen for more than 7 days (oldest first)
2. Evict low-delivery-probability entries for unreachable destinations
3. Evict oldest contact records beyond per-node limit
4. Compact SQLite database (VACUUM) if fragmentation > 30%

### Graph Persistence
Graph is serialized to SQLite on every update for crash recovery:
```sql
CREATE TABLE graph_nodes (
    node_id BLOB PRIMARY KEY,
    node_type INTEGER,
    capability_bundle BLOB,
    first_seen INTEGER,
    last_seen INTEGER,
    hop_distance INTEGER
);
CREATE TABLE graph_links (
    from_node BLOB NOT NULL,
    to_node BLOB NOT NULL,
    transport INTEGER,
    reliability REAL,
    bandwidth_bps INTEGER,
    latency_ms INTEGER,
    last_seen INTEGER,
    expires_at INTEGER,
    is_active INTEGER,
    is_gossip INTEGER,
    PRIMARY KEY (from_node, to_node, transport)
);
CREATE TABLE contact_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_node BLOB NOT NULL,
    peer_node BLOB NOT NULL,
    contact_start INTEGER,
    contact_end INTEGER,
    transport INTEGER,
    messages_exchanged INTEGER
);
```

## Graph Algorithms Used

### Dijkstra (Shortest Path)
Used for: direct connected networks (gateway-connected scenarios).
Input: latency as edge weight.
Not used in partitioned/DTN scenarios.

### BFS (Connectivity Check)
Used for: partition detection, hop-distance calculation.
Runs periodically.

### PRoPHET (Probabilistic Routing)
Used for: routing in intermittently connected mobile networks.
Delivery probability updated on each contact:
```
P(a,b) = P_old(a,b) + (1 - P_old(a,b)) * P_encounter_constant
P_encounter_constant = 0.75 (configurable)

Aging: P(a,b) *= aging_factor^(elapsed_time / aging_interval)
aging_factor = 0.98, aging_interval = 1 minute

Transitivity: P(a,c) = max(P(a,c), P(a,b) * P(b,c) * beta)
beta = 0.25 (transitivity coefficient)
```

### Spray-and-Wait
Used for: fallback when PRoPHET history is sparse.
Initial spray count: 3 copies.
Wait: each copy waits at a node until either direct delivery or another copy's ACK received.

## Graph Visualization (Operator Tool)

For network health monitoring, the graph can be exported:
```
GET http://localhost:8080/api/graph/export?format=graphml&anonymize=true
```

Export includes:
- Node IDs (BLAKE3-hashed again for anonymization)
- Edge weights (latency, bandwidth, reliability)
- Node types (USER/RELAY/GATEWAY)
- Contact frequencies (how often each link is active)

Does NOT include:
- Original node IDs (cryptographic addresses)
- Message content or metadata
- User identities

Used for: network planning, disaster response coordination, infrastructure health monitoring.
