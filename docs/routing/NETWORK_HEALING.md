# Network Healing

## Overview

Network partitions — where the mesh splits into isolated clusters — are a primary
failure mode during disasters. Communications infrastructure damage creates hard
partitions. Node mobility and battery depletion create soft partitions. IRIS must
detect partitions, heal them when connectivity is restored, and manage the
transition period correctly.

## Partition Detection

### Definition

A node declares itself in a partition when it cannot reach any node in a set of
"expected contacts" — nodes it previously communicated with that it believes
should still be active.

```rust
pub struct PartitionDetector {
    expected_contacts: HashMap<NodeId, ContactExpectation>,
    partition_declared_at: Option<SystemTime>,
    partition_threshold: Duration,  // default: 5 minutes
}

pub struct ContactExpectation {
    pub node_id: NodeId,
    pub last_seen: SystemTime,
    pub expected_reachable: bool,  // set by routing table, not just direct neighbor
    pub last_known_cluster: Option<ClusterId>,
}

impl PartitionDetector {
    pub fn check(&mut self, routing_table: &RoutingTable) -> Option<PartitionEvent> {
        let now = SystemTime::now();
        let unreachable: Vec<_> = self.expected_contacts.values()
            .filter(|c| {
                c.expected_reachable &&
                now.duration_since(c.last_seen).unwrap_or_default()
                    > self.partition_threshold
            })
            .collect();

        if unreachable.len() >= self.expected_contacts.len() / 2 {
            // More than half of expected contacts unreachable → partition
            if self.partition_declared_at.is_none() {
                self.partition_declared_at = Some(now);
                return Some(PartitionEvent::Declared {
                    unreachable_nodes: unreachable.iter().map(|c| c.node_id.clone()).collect(),
                    declared_at: now,
                });
            }
        } else {
            // Recovered
            if let Some(declared_at) = self.partition_declared_at.take() {
                return Some(PartitionEvent::Healed {
                    partition_duration: now.duration_since(declared_at).unwrap_or_default(),
                    healed_at: now,
                });
            }
        }
        None
    }
}
```

### Soft vs Hard Partitions

**Soft partition**: caused by node depletion or temporary distance. Heals when
nodes move closer or return from out-of-range. Common during normal operation.

**Hard partition**: caused by physical infrastructure damage (bridge collapse,
road closure) that permanently separates areas. Heals only via explicit relay
(vehicle, LoRa bridge, satellite gateway).

IRIS does not distinguish these at detection time — both appear identical. The
distinction matters for response: soft partitions resolve automatically; hard
partitions require deliberate relay deployment.

## Healing Triggers

A healing event begins when any of the following occur:

1. **New neighbor discovered** that has messages from the other partition
   (detected by Bloom filter exchange — see below)

2. **Known previously-unreachable node becomes reachable**

3. **Vehicle relay returns** from other cluster with stored messages

4. **LoRa link established** bridging previously-unconnected clusters

5. **Internet gateway restored** allowing relay-mediated delivery

## Healing Protocol

When two nodes establish first contact and either is in partition state:

### Step 1: Establish Contact

Standard BLE/Wi-Fi neighbor discovery handshake. Exchange:
- Node ID
- Protocol version
- Capabilities
- Partition state flag

### Step 2: Bloom Filter Exchange

```rust
pub struct SyncBloomFilter {
    /// Bloom filter of message IDs this node holds (stored + delivered)
    pub filter: BloomFilter,
    /// Total message count (for calibration)
    pub message_count: u32,
    /// Oldest message timestamp in store
    pub oldest_message_ts: u64,
    /// Cluster ID this node believes it belongs to
    pub cluster_id: ClusterId,
}

pub async fn exchange_bloom_filters(
    local: &MessageStore,
    remote: &mut TransportSession,
) -> Result<SyncBloomFilter> {
    let local_filter = SyncBloomFilter {
        filter: local.build_bloom_filter().await?,
        message_count: local.message_count().await?,
        oldest_message_ts: local.oldest_timestamp().await?,
        cluster_id: local.cluster_id(),
    };

    // Send our filter
    remote.send(&local_filter.encode()?).await?;

    // Receive their filter
    let remote_bytes = remote.recv().await?;
    let remote_filter = SyncBloomFilter::decode(&remote_bytes)?;

    Ok(remote_filter)
}
```

The Bloom filter allows each side to identify which messages the other is missing,
without transmitting the full message list (which could be thousands of entries).

False positive rate: 0.1% at 10,000 messages with 128KB filter.
This means ~10 unnecessary message transmissions per 10,000 — acceptable overhead.

### Step 3: Transfer Missing Messages

```rust
pub async fn transfer_missing_messages(
    local_store: &MessageStore,
    remote_filter: &SyncBloomFilter,
    session: &mut TransportSession,
    priority_min: Priority,
) -> Result<TransferStats> {
    // Find messages we have that remote is missing
    let missing_from_remote = local_store
        .messages_not_in_filter(&remote_filter.filter)
        .await?
        .into_iter()
        .filter(|m| m.priority <= priority_min as u8)  // priority_min is max int value for P7
        .collect::<Vec<_>>();

    let mut stats = TransferStats::default();

    // Transfer in priority order: P0 first
    for msg in missing_from_remote.iter().sorted_by_key(|m| m.priority) {
        match session.send_message(msg).await {
            Ok(_) => { stats.sent += 1; stats.bytes_sent += msg.serialized_size(); }
            Err(e) => { stats.failed += 1; log::warn!("Failed to send during heal: {}", e); }
        }
    }

    Ok(stats)
}
```

### Step 4: Update Routing Tables

After message transfer:

```rust
pub async fn merge_routing_tables(
    local: &mut RoutingTable,
    remote_table: &RoutingTableSnapshot,
) -> Result<()> {
    for (dest, remote_entry) in &remote_table.entries {
        match local.get(dest) {
            None => {
                // New destination not previously known
                local.insert(dest.clone(), remote_entry.clone());
            }
            Some(local_entry) => {
                // Keep the better route (higher delivery probability)
                if remote_entry.delivery_probability > local_entry.delivery_probability {
                    local.update(dest.clone(), remote_entry.clone());
                }
            }
        }
    }
    Ok(())
}
```

### Step 5: Propagate Topology Update

The healed connection is announced to all current neighbors so they can update
their routing tables:

```rust
pub fn broadcast_topology_update(
    new_route: RoutingEntry,
    neighbors: &NeighborTable,
    transport_registry: &TransportRegistry,
) {
    let announcement = TopologyUpdate {
        updated_route: new_route,
        hop_limit: 3,  // propagate 3 hops away
        origin: self_node_id(),
        timestamp: SystemTime::now(),
    };

    for neighbor in neighbors.all_neighbors() {
        if let Some(transport) = transport_registry.best_transport_to(neighbor) {
            transport.send_control_message(&announcement.encode());
        }
    }
}
```

## Convergence Time

Time for the network to fully converge after a heal event:

| Network Size | Expected Convergence | Notes |
|-------------|---------------------|-------|
| 5 nodes | 5–10 seconds | Direct exchange, single hop |
| 20 nodes | 15–30 seconds | 2–3 hops, fast propagation |
| 100 nodes | 1–3 minutes | Routing table propagation |
| 1,000 nodes | 5–15 minutes | Large partition, dense overlap |
| 10,000 nodes | 30–60 minutes | Major disaster scenario |

Convergence is limited by:
- Number of messages to transfer (bandwidth)
- Number of routing table entries to merge (overhead)
- Number of hops for topology update to propagate

## Emergency Message Re-Propagation

When a heal is detected, P0–P1 messages created during the partition are
immediately re-propagated into the newly-connected cluster:

```rust
pub async fn on_partition_heal(
    &mut self,
    newly_reachable: &[NodeId],
    partition_duration: Duration,
) -> Result<()> {
    let partition_start = SystemTime::now() - partition_duration;

    // Find all P0–P1 messages created during partition
    let emergency_msgs = self.store
        .messages_since(partition_start)
        .await?
        .into_iter()
        .filter(|m| m.priority <= 1)  // P0 and P1
        .collect::<Vec<_>>();

    if !emergency_msgs.is_empty() {
        log::info!(
            "Partition healed after {:?}. Re-propagating {} emergency messages.",
            partition_duration,
            emergency_msgs.len()
        );

        // Activate epidemic mode for these messages temporarily
        for msg in emergency_msgs {
            self.routing_engine.activate_epidemic(msg, Duration::from_secs(300));
        }
    }
    Ok(())
}
```

## Anti-Loop Safeguards During Healing

Healing can create routing loops if not managed carefully:

1. **Message deduplication**: message IDs in deduplication cache prevent same message
   being forwarded twice, even if it arrives via the new link from the other partition.

2. **Routing table versioning**: each entry has a monotonic version number.
   Older versions cannot overwrite newer ones, preventing routing oscillation.

3. **Split horizon**: do not advertise a route to the node it was learned from.
   Prevents two-node routing loops.

## Partial Heal: Single-Relay Bridge

The hardest case: two clusters connected only via a single vehicle relay that
periodically traverses both areas:

```
  [Cluster A: 20 nodes]  ←→ (vehicle, 3 times/day) ←→ [Cluster B: 15 nodes]
```

- Vehicle carries messages from A to B and B to A
- Convergence: each vehicle visit = one healing cycle
- IRIS SCF handles this automatically via vehicle relay behavior
- Routing tables in both clusters update to route via vehicle (via PRoPHET contact history)
- Effective delivery latency: 0–8 hours (depending on vehicle schedule)
- P0 messages: satellite gateway in either cluster should handle, not relying on vehicle
