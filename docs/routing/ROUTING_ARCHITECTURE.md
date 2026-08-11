# Routing Architecture

## Overview

IRIS routing solves a fundamental problem: how to deliver messages in a network with:
- No guaranteed end-to-end path
- Intermittent and heterogeneous connectivity
- Nodes that move, appear, and disappear unpredictably
- Wide range of transport characteristics (BLE at 200 kbps vs satellite at 1 byte/s)
- Strong priority requirements (P0 SOS must arrive; P7 video can be dropped)

The routing engine is the intelligence center of IRIS. It consumes network state
and produces forwarding decisions. It is transport-agnostic: the same routing engine
works whether the underlying link is BLE, LoRa, Wi-Fi, or Ethernet.

## Core Problem: DTN Routing

Delay-Tolerant Networking (DTN, RFC 4838) addresses "challenged networks" where:
- End-to-end paths may not exist at any given instant
- Path existence is intermittent and unpredictable
- Latency may be minutes, hours, or days
- Bidirectional connectivity is not guaranteed

DTN routing is fundamentally different from Internet routing (OSPF, BGP):
- Internet routing: assumes persistent paths, reacts to link failures
- DTN routing: assumes no persistent paths, exploits contact opportunities

IRIS uses a layered routing approach: deterministic methods first, probabilistic
and epidemic methods only when deterministic methods are insufficient.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│ Message Origin / Application Layer                              │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────────────────────┐
│ L4: Epidemic Routing (P0 emergency override)                    │
│     Broadcast to ALL neighbors, all transports simultaneously   │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓ (if no L3 path)
┌─────────────────────────────────────────────────────────────────┐
│ L3: Statistical / ML-Enhanced Routing                           │
│     Delivery probability estimates from contact history         │
│     PRoPHET algorithm + ML correction                           │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓ (if no L2 path)
┌─────────────────────────────────────────────────────────────────┐
│ L2: Opportunistic Routing                                       │
│     Spray-and-Wait, contact-based forwarding                    │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓ (if no L1 path)
┌─────────────────────────────────────────────────────────────────┐
│ L1: Known-Path Routing                                          │
│     Routing table (destination → next_hop), shortest path       │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓ (if no L0 path)
┌─────────────────────────────────────────────────────────────────┐
│ L0: Direct Delivery                                             │
│     Destination in neighbor table? Deliver directly.            │
└─────────────────────────┬───────────────────────────────────────┘
                          ↓ (if no direct path)
┌─────────────────────────────────────────────────────────────────┐
│ Store-Carry-Forward                                             │
│     Buffer message, wait for contact opportunity                │
└─────────────────────────────────────────────────────────────────┘
```

## Routing Engine Core

```rust
pub struct RoutingEngine {
    // Current network state
    pub neighbor_table: NeighborTable,
    pub routing_table: RoutingTable,
    pub contact_history: ContactHistory,
    pub link_quality_estimator: LinkQualityEstimator,

    // Algorithm implementations
    pub algorithms: Vec<Box<dyn RoutingAlgorithm>>,

    // Message storage
    pub message_store: MessageStore,

    // Metrics
    pub metrics: RoutingMetrics,
}

pub trait RoutingAlgorithm: Send + Sync {
    fn algorithm_id(&self) -> AlgorithmId;
    fn priority_range(&self) -> (Priority, Priority);
    fn can_route(&self, msg: &IrisMessage, state: &NetworkState) -> bool;
    fn compute_forwarding(&self, msg: &IrisMessage, state: &NetworkState)
        -> Vec<ForwardingDecision>;
}

pub struct ForwardingDecision {
    pub next_hop: NodeId,
    pub transport: TransportId,
    pub delivery_probability: f32,   // 0.0–1.0
    pub estimated_latency_ms: u64,
    pub confidence: f32,             // how certain is this estimate
}
```

## Routing Table

The routing table maps destinations to next-hop forwarding decisions:

```rust
pub struct RoutingTable {
    entries: HashMap<NodeId, RoutingEntry>,
    updated_at: Instant,
}

pub struct RoutingEntry {
    pub destination: NodeId,
    pub next_hop: NodeId,
    pub transport: TransportId,
    pub delivery_probability: f32,
    pub hop_count: u8,
    pub last_updated: SystemTime,
    pub source_algorithm: AlgorithmId,
    pub expiry: SystemTime,
}

impl RoutingTable {
    pub fn best_route(&self, dest: &NodeId, min_priority: Priority) -> Option<&RoutingEntry> {
        self.entries.get(dest).filter(|e| {
            !e.is_expired() && e.transport.supports_priority(min_priority)
        })
    }

    pub fn update(&mut self, new_entry: RoutingEntry) {
        // Only update if new entry has better delivery probability
        // or the existing entry is expired
        match self.entries.get(&new_entry.destination) {
            Some(existing) if existing.delivery_probability >= new_entry.delivery_probability
                && !existing.is_expired() => {}
            _ => { self.entries.insert(new_entry.destination.clone(), new_entry); }
        }
    }
}
```

## Priority → Routing Method Mapping

| Priority | Name | Routing Behavior |
|----------|------|-----------------|
| P0 | SOS / Life Threatening | Epidemic + all transports simultaneously, no TTL check |
| P1 | Evacuation Order | Spray-and-Wait (L=5) + epidemic if no ACK in 5 min |
| P2 | Medical Emergency | Spray-and-Wait (L=3) + PRoPHET |
| P3 | Shelter / Resource | PRoPHET, fall back to spray |
| P4 | Infrastructure Update | Known path or PRoPHET, no spray |
| P5 | Community Coordination | Known path only, store on miss |
| P6 | Personal Message | Direct or known path only |
| P7 | Media / Bulk | Direct delivery only, no store |

## Emergency Override: P0 Epidemic Routing

P0 messages trigger a special mode that bypasses all routing logic:

```rust
pub fn handle_p0_emergency(msg: &IrisMessage, engine: &mut RoutingEngine) {
    // Activate epidemic routing
    engine.set_mode(RoutingMode::Epidemic);

    // Forward to ALL current neighbors on ALL available transports
    for neighbor in engine.neighbor_table.all_neighbors() {
        for transport in engine.transport_registry.available_transports() {
            if transport.can_reach(&neighbor) {
                engine.enqueue_forward(msg, &neighbor, &transport, HIGH_PRIORITY);
            }
        }
    }

    // Schedule re-broadcast every 30 seconds until ACK or TTL expiry
    engine.schedule_rebroadcast(msg, Duration::from_secs(30));

    // Alert all peers to also enter epidemic mode for this message ID
    engine.broadcast_epidemic_alert(msg.id());
}
```

## Routing State Persistence

The routing engine state must survive app crashes and restarts:

```rust
// Persist to SQLite on every significant state change
impl RoutingEngine {
    pub async fn persist_state(&self) -> Result<()> {
        let mut tx = self.db.begin().await?;

        // Persist routing table
        sqlx::query("DELETE FROM routing_table").execute(&mut tx).await?;
        for (dest, entry) in &self.routing_table.entries {
            sqlx::query(
                "INSERT INTO routing_table VALUES (?, ?, ?, ?, ?, ?)"
            )
            .bind(dest.as_bytes())
            .bind(entry.next_hop.as_bytes())
            .bind(entry.delivery_probability)
            .bind(entry.hop_count)
            .bind(entry.last_updated.unix_timestamp())
            .bind(entry.expiry.unix_timestamp())
            .execute(&mut tx).await?;
        }

        // Persist contact history (PRoPHET state)
        self.contact_history.persist(&mut tx).await?;

        tx.commit().await?;
        Ok(())
    }
}
```

## Routing Control Overhead Budget

Routing control messages (table updates, contact advertisements, delivery ACKs)
compete with application messages for bandwidth. Budget:

| Transport | Available Bandwidth | Routing Budget (10%) | Application Budget |
|-----------|--------------------|-----------------------|-------------------|
| BLE | 200 kbps | 20 kbps | 180 kbps |
| Wi-Fi Direct | 20 Mbps | 2 Mbps | 18 Mbps |
| LoRa (SF9) | ~1.4 B/s | 0.14 B/s | 1.26 B/s |
| Satellite SBD | 1 B/s | 0.1 B/s | 0.9 B/s |

On LoRa and satellite, routing overhead is suppressed aggressively. Only delivery
ACKs (4 bytes each) and critical routing updates (new gateway available) are sent.
Full routing table exchanges happen only over higher-bandwidth transports.

## Metrics

```rust
pub struct RoutingMetrics {
    pub messages_forwarded: Counter,
    pub messages_stored: Counter,
    pub messages_dropped: Counter,
    pub messages_delivered: Counter,
    pub average_delivery_latency_ms: Histogram,
    pub routing_table_size: Gauge,
    pub contact_history_size: Gauge,
    pub storage_utilization: Gauge,
    pub per_priority_delivery_rate: HashMap<Priority, f32>,
}
```

Metrics exposed via Prometheus endpoint on edge nodes for operational monitoring.
Mobile app displays simplified routing status: "X messages delivered, Y stored, Z dropped."
