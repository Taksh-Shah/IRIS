# Congestion Control

## Overview

Congestion occurs when a node receives more messages to forward than its resources
(storage, bandwidth, battery) can handle. Without congestion control, a busy relay
node becomes a bottleneck or fails entirely, degrading the entire mesh.

IRIS congestion control is multi-layered: per-queue priority management, backpressure
signaling, rate limiting per sender, and per-transport regulation. The overriding
constraint: P0–P1 messages are never dropped due to congestion.

## Congestion Signals

A node monitors three congestion indicators:

```rust
pub struct CongestionState {
    pub storage_utilization: f32,   // 0.0–1.0 (bytes used / max)
    pub queue_depth: u32,           // messages waiting to forward
    pub battery_level: f32,         // 0.0–1.0
    pub battery_drain_rate: f32,    // mA currently drawn
}

impl CongestionState {
    pub fn level(&self) -> CongestionLevel {
        // Storage pressure
        if self.storage_utilization > 0.95 { return CongestionLevel::Critical; }
        if self.storage_utilization > 0.80 { return CongestionLevel::High; }
        if self.storage_utilization > 0.60 { return CongestionLevel::Medium; }

        // Battery pressure (only relevant for mobile nodes)
        if self.battery_level < 0.10 { return CongestionLevel::Critical; }
        if self.battery_level < 0.20 && self.battery_drain_rate > 500.0 {
            return CongestionLevel::High;
        }

        // Queue pressure
        if self.queue_depth > 500 { return CongestionLevel::High; }
        if self.queue_depth > 200 { return CongestionLevel::Medium; }

        CongestionLevel::Normal
    }
}

pub enum CongestionLevel { Normal, Medium, High, Critical }
```

## Per-Transport Priority Queues

Each transport maintains a priority queue of outbound messages:

```rust
pub struct TransportQueue {
    // Separate queues per priority class
    queues: [VecDeque<QueuedMessage>; 8],  // index 0 = P0, index 7 = P7
    total_size_bytes: usize,
    max_size_bytes: usize,
    transport_id: TransportId,
}

impl TransportQueue {
    /// Dequeue next message to send (priority order)
    pub fn dequeue(&mut self) -> Option<QueuedMessage> {
        // Always service P0 first
        for priority in 0..=7 {
            if let Some(msg) = self.queues[priority].pop_front() {
                self.total_size_bytes -= msg.size_bytes;
                return Some(msg);
            }
        }
        None
    }

    /// Enqueue with priority. Returns whether message was accepted.
    pub fn enqueue(&mut self, msg: QueuedMessage) -> EnqueueResult {
        let priority = msg.priority as usize;

        // P0–P1: always accept, may exceed queue max briefly
        if priority <= 1 {
            self.queues[priority].push_back(msg);
            self.total_size_bytes += msg.size_bytes;
            return EnqueueResult::Accepted;
        }

        // P2+: check capacity first
        if self.total_size_bytes + msg.size_bytes > self.max_size_bytes {
            return EnqueueResult::Rejected { reason: "Queue full" };
        }

        self.queues[priority].push_back(msg);
        self.total_size_bytes += msg.size_bytes;
        EnqueueResult::Accepted
    }
}
```

## Drop Policy

When the overall message store or queue is full, drop messages to make space:

```rust
pub fn apply_drop_policy(
    store: &mut MessageStore,
    incoming: &IrisMessage,
) -> DropDecision {
    let utilization = store.utilization();

    // Never drop P0–P1 for any reason
    if incoming.priority <= Priority::P1 {
        // Force evict P7, P6, P5 to make room if needed
        store.force_evict_to_fit(incoming.serialized_size(), Priority::P4);
        return DropDecision::Accept;
    }

    match utilization {
        u if u < 0.70 => DropDecision::Accept,
        u if u < 0.80 => {
            // Soft pressure: drop only P7
            if incoming.priority == Priority::P7 {
                DropDecision::Drop { reason: "Storage pressure (soft), dropping P7" }
            } else {
                DropDecision::Accept
            }
        }
        u if u < 0.90 => {
            // Medium pressure: drop P7, P6
            if incoming.priority >= Priority::P6 {
                DropDecision::Drop { reason: "Storage pressure (medium)" }
            } else {
                DropDecision::Accept
            }
        }
        u if u < 0.95 => {
            // High pressure: drop P7–P5
            if incoming.priority >= Priority::P5 {
                DropDecision::Drop { reason: "Storage pressure (high)" }
            } else {
                DropDecision::Accept
            }
        }
        _ => {
            // Critical: drop anything below P2
            if incoming.priority >= Priority::P2 {
                DropDecision::Drop { reason: "Storage pressure (critical)" }
            } else {
                DropDecision::Accept
            }
        }
    }
}
```

## Backpressure Signaling

When a node is under storage pressure, it signals upstream nodes to reduce forwarding:

```rust
pub struct BackpressureAdvertisement {
    pub node_id: NodeId,
    pub storage_pressure: f32,      // 0.0–1.0
    pub battery_pressure: f32,      // 0.0–1.0
    pub max_accepted_priority: Priority,  // do not send lower-priority messages
    pub queue_depth: u32,
    pub estimated_clear_time_secs: u32,
}

// Advertised in capability bundle, updated every 30 seconds
// or immediately when level changes significantly
```

When a node receives backpressure from a neighbor:

```rust
pub fn apply_backpressure(
    neighbor: &NodeId,
    pressure: &BackpressureAdvertisement,
    routing_engine: &mut RoutingEngine,
) {
    // Temporarily lower quality score for this neighbor as a relay
    routing_engine.routing_table.apply_penalty(
        neighbor,
        RoutingPenalty {
            score_multiplier: 1.0 - pressure.storage_pressure,
            expires_at: Instant::now() + Duration::from_secs(
                pressure.estimated_clear_time_secs as u64
            ),
        }
    );

    // Don't forward messages below max_accepted_priority to this neighbor
    routing_engine.per_neighbor_priority_limits.insert(
        neighbor.clone(),
        pressure.max_accepted_priority,
    );

    log::info!("Applying backpressure from {:?}: max priority {:?}, score penalty {}",
        neighbor, pressure.max_accepted_priority, pressure.storage_pressure);
}
```

## Rate Limiting Per Sender

To prevent a single node from flooding the mesh:

```rust
pub struct SenderRateLimiter {
    /// Token bucket per sender
    buckets: HashMap<NodeId, TokenBucket>,
    /// Default rate for P4+ messages: 10/minute
    default_rate: Rate,
}

pub struct TokenBucket {
    pub tokens: f64,           // current tokens available
    pub capacity: f64,         // max tokens (burst size)
    pub refill_rate: f64,      // tokens per second
    pub last_refill: Instant,
}

impl SenderRateLimiter {
    pub fn check(&mut self, sender: &NodeId, priority: Priority) -> RateDecision {
        // P0–P2: no rate limit
        if priority <= Priority::P2 {
            return RateDecision::Allow;
        }

        let bucket = self.buckets.entry(sender.clone())
            .or_insert_with(|| TokenBucket::new(10.0, 1.0 / 6.0)); // 10/min

        bucket.refill();

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            RateDecision::Allow
        } else {
            RateDecision::Throttle {
                retry_after: Duration::from_secs_f64(
                    (1.0 - bucket.tokens) / bucket.refill_rate
                ),
            }
        }
    }
}
```

## Anti-Flooding via Deduplication

Forwarding the same message multiple times wastes bandwidth and storage.
The deduplication cache (described in BASELINE_ROUTING.md) prevents this.

Additional: per-transport message tracking prevents sending the same message
twice on the same transport to the same neighbor:

```rust
pub struct ForwardingRecord {
    pub message_id: MessageId,
    pub forwarded_to: NodeId,
    pub transport: TransportId,
    pub forwarded_at: SystemTime,
    pub ack_received: bool,
}

// Before forwarding: check this record doesn't exist
// After ACK: mark ack_received = true, stop retries
```

## Per-Transport Rate Limits

Each transport imposes hard rate limits based on physical and regulatory constraints:

```rust
pub struct TransportRateLimit {
    pub transport: TransportId,
    pub max_messages_per_second: f32,
    pub max_bytes_per_second: u64,
    pub regulatory: bool,  // if true, violation is illegal
}

pub fn get_transport_limits() -> Vec<TransportRateLimit> {
    vec![
        TransportRateLimit {
            transport: TransportId::LoRa,
            max_messages_per_second: 0.01, // 36 packets/hour = 0.01/second
            max_bytes_per_second: 1,        // ~1 byte/second effective
            regulatory: true,               // WPC 1% duty cycle — enforced
        },
        TransportRateLimit {
            transport: TransportId::BLE,
            max_messages_per_second: 50.0, // ~50 messages/second practical
            max_bytes_per_second: 25_000,  // 25 KB/s effective on BLE 5.0
            regulatory: false,
        },
        TransportRateLimit {
            transport: TransportId::WiFiDirect,
            max_messages_per_second: 1_000.0,
            max_bytes_per_second: 5_000_000, // 5 MB/s practical
            regulatory: false,
        },
        TransportRateLimit {
            transport: TransportId::SatelliteIridiumSBD,
            max_messages_per_second: 0.003, // ~10 messages/hour
            max_bytes_per_second: 1,         // cost-based limit, not regulatory
            regulatory: false,
        },
    ]
}
```

## Battery-Aware Congestion

Mobile nodes (phones, tablets) apply additional limits when battery is low:

```rust
pub fn battery_adjusted_limits(
    base_limits: &TransportRateLimit,
    battery_level: f32,
) -> TransportRateLimit {
    let throttle_factor = match battery_level {
        b if b > 0.50 => 1.0,     // No throttling above 50%
        b if b > 0.30 => 0.75,    // 25% reduction at 30–50%
        b if b > 0.20 => 0.50,    // 50% reduction at 20–30%
        b if b > 0.10 => 0.25,    // 75% reduction at 10–20%
        _ => 0.10,                 // Survival mode: 90% reduction, P0–P2 only
    };

    TransportRateLimit {
        max_messages_per_second: base_limits.max_messages_per_second * throttle_factor,
        max_bytes_per_second: (base_limits.max_bytes_per_second as f32 * throttle_factor) as u64,
        ..base_limits.clone()
    }
}
```

## Congestion Metrics

```rust
pub struct CongestionMetrics {
    pub messages_dropped_by_priority: [Counter; 8],
    pub messages_rate_limited_by_sender: Counter,
    pub backpressure_events: Counter,
    pub average_queue_depth: Gauge,
    pub storage_utilization_p95: Histogram,  // 95th percentile over time
    pub forward_latency_by_transport: HashMap<TransportId, Histogram>,
}
```

Alerts:
- `messages_dropped[P2] > 0` in any 5-minute window → critical alert
- `storage_utilization > 0.85` for >10 minutes → add storage or reduce relay scope
- `backpressure_events > 100/hour` → mesh topology change needed (add relay nodes)
