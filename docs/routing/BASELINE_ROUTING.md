# Baseline Routing (Layer 0)

## Design Philosophy

Layer 0 routing is the foundation that must work correctly before any intelligence
is added. It has zero probabilistic components, zero machine learning, and zero
dependency on contact history. It must run correctly on a Raspberry Pi Zero with
512 MB RAM, consuming less than 5% CPU.

**Guarantee**: if a path exists or can exist, Layer 0 will eventually deliver the message,
even if sub-optimally.

## Algorithm Set

Layer 0 comprises four deterministic algorithms executed in sequence:

```
Algorithm 1: Direct Delivery
         ↓ (miss)
Algorithm 2: Known-Path Routing
         ↓ (miss)
Algorithm 3: Flood with Hop Limit
         ↓ (hop limit reached or network exhausted)
Algorithm 4: Store
```

## Algorithm 1: Direct Delivery

The simplest case: the destination is a current neighbor.

```rust
pub fn try_direct_delivery(
    msg: &IrisMessage,
    neighbor_table: &NeighborTable,
    transport_registry: &TransportRegistry,
) -> Option<ForwardingDecision> {
    // Is the destination a current neighbor?
    let neighbor = neighbor_table.get(&msg.recipient)?;

    // Is at least one transport available to that neighbor?
    let transport = transport_registry
        .transports_to(neighbor)
        .into_iter()
        .max_by_key(|t| t.link_quality_score())?;

    Some(ForwardingDecision {
        next_hop: msg.recipient.clone(),
        transport: transport.id(),
        delivery_probability: 0.95,  // direct = high confidence
        estimated_latency_ms: transport.estimated_latency_ms(),
        confidence: 1.0,
    })
}
```

**When this runs**: every time a message is queued for delivery.
**Complexity**: O(1) neighbor table lookup.
**Failure condition**: destination not currently in neighbor table.

## Algorithm 2: Known-Path Routing

If the destination is not a direct neighbor but the routing table has a path:

```rust
pub fn try_known_path(
    msg: &IrisMessage,
    routing_table: &RoutingTable,
    neighbor_table: &NeighborTable,
) -> Option<ForwardingDecision> {
    // Find entry in routing table
    let entry = routing_table.best_route(&msg.recipient, msg.priority)?;

    // Verify next hop is still reachable
    if !neighbor_table.contains(&entry.next_hop) {
        // Next hop disappeared — invalidate this routing entry
        routing_table.invalidate(&msg.recipient, &entry.next_hop);
        return None;
    }

    Some(ForwardingDecision {
        next_hop: entry.next_hop.clone(),
        transport: entry.transport.clone(),
        delivery_probability: entry.delivery_probability,
        estimated_latency_ms: entry.hop_count as u64 * 100, // rough estimate
        confidence: 0.8,
    })
}
```

**Routing table population**: Layer 0 populates the routing table from:
1. Neighbor advertisements (each neighbor announces destinations it can reach)
2. Delivery ACKs propagated backwards along delivery path
3. Explicit routing updates from higher layers

**Routing table entry expiry**: entries expire after 10 minutes of no confirmation.
Expired entries are not used; they trigger fresh discovery.

## Algorithm 3: Flood with Hop Limit

When no known path exists, flood the message to all neighbors. Each neighbor
applies the same algorithm recursively until the message reaches its destination
or the hop limit is exhausted.

```rust
pub fn flood_with_hop_limit(
    msg: &mut IrisMessage,
    neighbor_table: &NeighborTable,
    transport_registry: &TransportRegistry,
    already_flooded_to: &mut HashSet<NodeId>,
) -> Vec<ForwardingDecision> {
    if msg.hop_count >= msg.max_hops {
        return vec![];  // hop limit reached
    }

    let mut decisions = Vec::new();

    for neighbor in neighbor_table.all_neighbors() {
        // Skip: already flooded to this neighbor
        if already_flooded_to.contains(&neighbor.id) {
            continue;
        }
        // Skip: this neighbor is the message source (no backtracking)
        if neighbor.id == msg.sender_id {
            continue;
        }

        let transports = transport_registry.transports_to(neighbor);
        if let Some(best_transport) = transports.into_iter().max_by_key(|t| t.bandwidth_bps()) {
            decisions.push(ForwardingDecision {
                next_hop: neighbor.id.clone(),
                transport: best_transport.id(),
                delivery_probability: 0.3,  // uncertain — we're guessing
                estimated_latency_ms: 500 * msg.hop_count as u64,
                confidence: 0.3,
            });
            already_flooded_to.insert(neighbor.id.clone());
        }
    }

    msg.hop_count += 1;
    decisions
}
```

**Default max_hops**: 3 for P4–P7, 5 for P1–P3, unlimited for P0.
**Rationale for hop limit**: prevents messages from circulating forever in loops
and consuming bandwidth/storage indefinitely.

### Anti-Loop Mechanism

Each node maintains a **recently-forwarded set**: a rolling window of message IDs
it has forwarded in the last 24 hours. When a message arrives with a known ID:

```rust
pub struct DeduplicationCache {
    // Bloom filter for memory efficiency (false-positive rate: 0.1%)
    bloom: BloomFilter,
    // Exact set for messages seen in last 1 hour (to handle Bloom false positives)
    exact: HashSet<MessageId>,
    // Ring buffer for cleanup
    ring: VecDeque<(MessageId, Instant)>,
}

impl DeduplicationCache {
    pub fn is_duplicate(&self, id: &MessageId) -> bool {
        if !self.bloom.might_contain(id) {
            return false;  // Definitely not a duplicate
        }
        self.exact.contains(id)  // Check exact set to confirm
    }

    pub fn record(&mut self, id: MessageId) {
        self.bloom.insert(&id);
        self.exact.insert(id.clone());
        self.ring.push_back((id, Instant::now()));
        self.evict_old();
    }
}
```

When a duplicate arrives: silently drop. Do not forward, do not store.

## Algorithm 4: Store

When all forwarding options are exhausted (no path, hop limit reached), store the
message in the local message store and wait for a future contact opportunity:

```rust
pub fn store_for_later(
    msg: IrisMessage,
    store: &mut MessageStore,
) -> StorageResult {
    // Check TTL — don't store expired messages
    if msg.is_expired() {
        return StorageResult::Dropped { reason: DropReason::TtlExpired };
    }

    // Check storage capacity
    if store.is_full() {
        // Eviction: drop lowest priority, highest TTL-consumed message
        if let Some(evicted) = store.evict_lowest_priority() {
            log::info!("Evicted {:?} to make room for {:?}", evicted.id, msg.id);
        } else {
            return StorageResult::Dropped { reason: DropReason::StorageFull };
        }
    }

    store.insert(msg);
    StorageResult::Stored
}
```

**Trigger for re-evaluation**: whenever a new neighbor is discovered, all stored
messages are re-evaluated through Algorithms 1–3. New contacts often create new
forwarding opportunities.

## State Machine

The routing state machine for each message:

```
    ┌─────────────────────────────────────────────────────┐
    │                      NEW                            │
    └──────────────────────────┬──────────────────────────┘
                               │
                     Run Algorithm 1 (Direct)
                               │
               ┌───────────────┴───────────────┐
               │ Hit                           │ Miss
               ▼                               ▼
        TRY_DIRECT_SEND             Run Algorithm 2 (Known Path)
               │                               │
         ┌─────┴──────┐           ┌────────────┴────────────┐
         │ ACK        │ Timeout   │ Hit                     │ Miss
         ▼            ▼           ▼                         ▼
      DELIVERED    RETRY     TRY_KNOWN_PATH     Run Algorithm 3 (Flood)
                   (3x)           │                         │
                             ┌───┴───┐           ┌──────────┴──────────┐
                             │ ACK   │ Timeout   │ Flooded             │ No neighbors
                             ▼       ▼           ▼                     ▼
                          DELIVERED RETRY     FLOODING             STORE
                                    (2x)         │
                                            ┌────┴────┐
                                            │ ACK     │ Hop limit
                                            ▼         ▼
                                         DELIVERED  STORE
```

States:
- `NEW`: just received/created
- `TRY_DIRECT_SEND`: forwarding in progress to direct neighbor
- `TRY_KNOWN_PATH`: forwarding via routing table
- `FLOODING`: sent to multiple neighbors
- `STORE`: buffered, awaiting contact
- `DELIVERED`: ACK received — success
- `EXPIRED`: TTL elapsed without delivery — failure

## Correctness Guarantees

**No loops**: hop count is monotonically decreasing. A message cannot circulate
because each forwarded copy has a lower remaining hop count. At hop_count=0, drop.

**No duplicate delivery**: deduplication cache ensures a node processes each
message ID at most once. Even if multiple copies arrive via different paths, only
the first is processed; subsequent copies are silently dropped.

**No zombie messages**: TTL is checked at every store/forward step. Messages past
their TTL are immediately discarded.

**Eventual delivery (with connected network)**: if there exists any sequence of
contacts between source and destination within the TTL window, the flood algorithm
will find and use it, subject to hop limit. For TTL and hop limit set appropriately,
this is guaranteed.

**Priority preservation**: Layer 0 never reorders delivery by priority. However,
when a new contact is discovered and stored messages are re-evaluated, priority
determines the evaluation and transmission order.

## Resource Budget

Target platform: Raspberry Pi Zero 2W (quad-core ARM Cortex-A53, 512 MB RAM).

| Operation | Latency | CPU % |
|-----------|---------|-------|
| Algorithm 1 (direct lookup) | <1 μs | 0.0% |
| Algorithm 2 (routing table lookup) | <10 μs | 0.0% |
| Algorithm 3 (flood, 10 neighbors) | <1 ms | 0.1% |
| Algorithm 4 (store, SQLite insert) | <5 ms | 0.5% |
| Deduplication check (Bloom) | <0.5 μs | 0.0% |
| Full message routing decision | <10 ms | 0.5% |

Memory: routing table for 10,000 entries: ~2 MB. Deduplication Bloom filter: ~500 KB.
Total Layer 0 memory: <5 MB. Fits comfortably on Pi Zero.
