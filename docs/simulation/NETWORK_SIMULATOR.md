# Network Simulator Design

## Overview

IRIS uses a custom discrete-event simulator implemented in Rust (`crates/iris-sim`) with a Python orchestration layer (`tools/simulation`). This document describes the internal design of the simulator: the event engine, node and link models, transport simulations, metrics collection, and the routing algorithm plugin API.

## Design Philosophy

The simulator is not a general-purpose network simulator. It is purpose-built for IRIS:

- Runs the **actual production Rust core** code, not a model of it
- Focuses on **disruption-tolerant** network behavior (store-carry-forward, intermittent contacts)
- Supports **heterogeneous transports** (BLE, Wi-Fi Direct, LoRa, Satellite) with distinct characteristics
- Optimized for **mobility-driven contact graphs** rather than static topologies
- Designed for **scale** — 10,000 nodes in a single run

## Discrete Event Simulation

The simulator uses a discrete-event simulation (DES) paradigm. Time advances by jumping from event to event. No computation occurs between events (unlike time-step simulation).

### Event Queue

Events are stored in a binary min-heap ordered by simulation time:

```rust
pub struct EventQueue {
    heap: BinaryHeap<Reverse<TimedEvent>>,
    clock: AtomicF64,
}

pub struct TimedEvent {
    pub time: SimTime,       // f64 seconds since sim start
    pub event: SimEvent,
    pub node_id: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub enum SimEvent {
    // Node events
    NodeStart { node_id: NodeId },
    NodeFail { node_id: NodeId, reason: FailureReason },
    NodeRecover { node_id: NodeId },
    BatteryDepleted { node_id: NodeId },

    // Transport events
    ScanStart { node_id: NodeId, transport: TransportType },
    ScanComplete { node_id: NodeId, transport: TransportType, discovered: Vec<NodeId> },
    ContactBegin { a: NodeId, b: NodeId, transport: TransportType },
    ContactEnd { a: NodeId, b: NodeId, transport: TransportType },

    // Message events
    MessageInject { msg: SimMessage },
    MessageTransmitStart { from: NodeId, to: NodeId, msg_id: MsgId, transport: TransportType },
    MessageTransmitComplete { from: NodeId, to: NodeId, msg_id: MsgId, success: bool },
    MessageDelivered { msg_id: MsgId, dst: NodeId, hop_count: u8, latency_s: f64 },
    MessageExpired { msg_id: MsgId, reason: ExpiryReason },

    // Mobility events
    NodeMove { node_id: NodeId, new_pos: GeoPoint },
    NodeWaypointReached { node_id: NodeId },
}
```

### Simulation Loop

```rust
pub fn run(&mut self) -> SimResult {
    // Initialize nodes
    for node in &mut self.nodes {
        self.event_queue.push(SimEvent::NodeStart { node_id: node.id }, 0.0);
    }

    // Initialize traffic injection
    self.traffic_model.schedule_first(&mut self.event_queue);

    // Main loop
    while let Some(event) = self.event_queue.pop() {
        if event.time > self.config.duration {
            break;
        }

        self.clock = event.time;
        self.handle_event(event);

        // Collect metrics snapshot every 1s sim-time
        if self.clock - self.last_metrics_snapshot >= 1.0 {
            self.metrics.snapshot(self.clock, &self.nodes);
            self.last_metrics_snapshot = self.clock;
        }
    }

    self.metrics.finalize()
}
```

### Performance Characteristics

For a 1,000-node, 1-hour simulation:
- Event count: ~5-50 million events
- Wall time: ~30-120 seconds (hardware dependent)
- Memory: ~2-8 GB (dominated by message buffers and event log)

For 10,000 nodes:
- Event count: ~500M events
- Wall time: ~30-60 minutes
- Memory: ~16-32 GB (requires server with sufficient RAM)
- Use parallel simulation for large scenarios (see `SCALE_TESTING.md`)

## Node Simulation

Each simulated node models a real IRIS device:

```rust
pub struct SimNode {
    pub id: NodeId,
    pub position: GeoPoint,
    pub state: NodeState,

    // Hardware capabilities
    pub capabilities: NodeCapabilities,

    // Battery model
    pub battery: BatteryModel,

    // Message buffers (per-priority)
    pub message_store: SimMessageStore,

    // Active transport instances
    pub transports: Vec<Box<dyn SimTransport>>,

    // The actual production iris-core instance
    pub core: IrisCore,

    // Mobility model for this node
    pub mobility: Box<dyn MobilityModel>,

    // Current contacts (neighbor nodes currently in range)
    pub active_contacts: HashMap<(NodeId, TransportType), ContactState>,
}

pub enum NodeState {
    Active,
    Failed { reason: FailureReason, recover_at: Option<SimTime> },
    LowBattery,   // Reduced functionality
    Critical,     // Emergency-only mode
    Shutdown,     // Graceful shutdown
}

pub struct NodeCapabilities {
    pub has_ble: bool,
    pub has_wifi_direct: bool,
    pub has_wifi_aware: bool,
    pub has_lora: bool,
    pub has_satellite: bool,
    pub is_gateway: bool,     // Has internet/satellite access
    pub storage_limit_mb: u32,
    pub battery_mah: u32,
}
```

### Node Type Distribution

Realistic simulations use heterogeneous node populations:

```yaml
# Example: Post-disaster urban scenario
nodes:
  - type: smartphone_android
    count: 200
    capabilities: [ble, wifi_direct, wifi_aware]
    battery_mah: 4000
    storage_mb: 500

  - type: smartphone_ios
    count: 150
    capabilities: [ble, multipeer]   # iOS: no Wi-Fi Aware/Direct
    battery_mah: 3500
    storage_mb: 500

  - type: lora_gateway
    count: 5
    capabilities: [lora, wifi_direct, satellite]
    battery_mah: 10000              # External battery pack
    is_gateway: true

  - type: raspberry_pi_node
    count: 10
    capabilities: [ble, wifi_direct, lora]
    battery_mah: 20000              # 20Ah battery pack
    is_gateway: false
    static: true                    # Fixed position
```

## Link Simulation

Links between nodes are not persistent — they form and dissolve based on proximity and transport-specific range models.

### Link Formation

At each scan interval, nodes discover neighbors:

```rust
pub fn compute_links(&self, nodes: &[SimNode]) -> Vec<SimLink> {
    let mut links = Vec::new();

    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i+1..] {
            let distance_m = haversine_distance(&a.position, &b.position);

            for transport in a.capabilities.transports() {
                if b.capabilities.supports(transport) {
                    if let Some(link) = self.transport_models[transport]
                        .compute_link(a, b, distance_m) {
                        links.push(link);
                    }
                }
            }
        }
    }
    links
}
```

### Link Quality Model

```rust
pub struct SimLink {
    pub node_a: NodeId,
    pub node_b: NodeId,
    pub transport: TransportType,

    // Derived from transport model
    pub bandwidth_bps: u64,
    pub latency_ms: u32,
    pub loss_rate: f64,       // 0.0 = no loss, 1.0 = total loss
    pub established_at: SimTime,
    pub expected_duration_s: f64,  // Contact duration estimate
}
```

### Transport-Specific Link Parameters

| Transport | Bandwidth | Latency | Range | Loss Model |
|-----------|-----------|---------|-------|------------|
| BLE 4.2 | 125-700 kbps | 30-150ms | 10-50m | Distance-based log model |
| BLE 5.0 | 500 kbps-2 Mbps | 20-100ms | 40-200m | Log-distance with shadowing |
| Wi-Fi Direct | 20-200 Mbps | 5-50ms | 50-200m | Log-distance, much less loss |
| Wi-Fi Aware | 1-54 Mbps | 10-30ms | 50-150m | Log-distance |
| LoRa SF7 | ~5 kbps | 100-500ms | 2-5km | Stochastic, duty-cycle limited |
| LoRa SF12 | ~250 bps | 500-2000ms | 10-20km | Stochastic, duty-cycle limited |
| Satellite | 2.4 kbps | 600-700ms | Global | Window-based availability |

## Transport Simulation

Each transport has a simulation implementation that models its specific characteristics.

### BLE Simulation

```rust
pub struct BLESimTransport {
    version: BLEVersion,  // V42 or V50
    scan_interval_ms: u32,
    advertising_interval_ms: u32,
    tx_power_dbm: i8,
    link_model: LogDistanceLinkModel,
}

impl BLESimTransport {
    fn compute_range_m(&self, tx_power: i8, path_loss_exponent: f64) -> f64 {
        // Log-distance path loss: PL(d) = PL(d0) + 10*n*log10(d/d0) + X_sigma
        // Solve for d given minimum received power (sensitivity = -93 dBm for BLE)
        let pl_budget = (tx_power as f64) - (-93.0);  // dBm budget
        let d0 = 1.0;  // reference distance 1m
        let pl_d0 = 40.2;  // path loss at 1m for 2.4GHz
        let n = path_loss_exponent;  // typically 2.0-3.5 indoors
        let d = d0 * 10.0_f64.powf((pl_budget - pl_d0) / (10.0 * n));
        d
    }

    fn scan_duration_ms(&self) -> f64 {
        // BLE discovery latency: typically 1-5 scan intervals
        // With 100ms scan interval: expect 100-500ms to discover all nearby devices
        self.scan_interval_ms as f64 * (1.0 + rand::random::<f64>() * 4.0)
    }
}
```

### LoRa Simulation

Models the distinctive characteristics of LoRa: very long range, very low bandwidth, regulatory duty cycle.

```rust
pub struct LoRaSimTransport {
    frequency_mhz: f64,      // 865.0-867.0 for India
    spreading_factor: u8,     // 7-12
    bandwidth_khz: f64,       // 125, 250, or 500
    coding_rate: f64,         // 4/5, 4/6, 4/7, 4/8
    tx_power_dbm: u8,         // 2-22 dBm
    duty_cycle_percent: f64,  // 1.0 for most bands
    duty_cycle_tracker: DutyCycleTracker,
}

impl LoRaSimTransport {
    fn time_on_air_ms(&self, payload_bytes: usize) -> f64 {
        // LoRa time-on-air formula (Semtech AN1200.13)
        let bw = self.bandwidth_khz * 1000.0;
        let sf = self.spreading_factor as f64;
        let symbol_duration_ms = (2.0_f64.powf(sf) / bw) * 1000.0;
        let preamble_symbols = 8.0 + 4.25;
        let payload_symbols = 8.0 + f64::max(
            (8.0 * payload_bytes as f64 - 4.0 * sf + 28.0 + 16.0)
                / (4.0 * (sf - 2.0 * self.low_data_rate_optimize as u8 as f64))
                * self.coding_rate.ceil(),
            0.0
        );
        symbol_duration_ms * (preamble_symbols + payload_symbols)
    }

    fn can_transmit(&self, sim_time: SimTime, payload_bytes: usize) -> bool {
        let toa = self.time_on_air_ms(payload_bytes);
        self.duty_cycle_tracker.can_transmit(sim_time, toa, self.duty_cycle_percent)
    }
}
```

## Message Routing Simulation

Message routing runs in the actual `iris-core` routing engine. The simulator's role is to:

1. Model when contacts are available (link formation/dissolution events)
2. Model how long data transfer takes (bandwidth / message size)
3. Provide the routing engine with contact opportunities

```rust
impl SimNode {
    fn handle_contact_begin(&mut self, peer: NodeId, transport: TransportType, sim_time: SimTime) {
        // Notify the real iris-core routing engine of the contact
        self.core.on_contact_begin(peer, transport, sim_time);

        // The routing engine decides what to send
        let messages_to_send = self.core.get_messages_for_peer(peer);

        // Schedule message transmissions
        for msg in messages_to_send {
            let duration_s = (msg.size_bytes * 8) as f64 / link.bandwidth_bps as f64;
            self.event_queue.push(
                SimEvent::MessageTransmitStart { from: self.id, to: peer, msg_id: msg.id, transport },
                sim_time,
            );
            self.event_queue.push(
                SimEvent::MessageTransmitComplete { from: self.id, to: peer, msg_id: msg.id, success: true },
                sim_time + duration_s + link.latency_ms as f64 / 1000.0,
            );
        }
    }
}
```

## Metrics Collection

Metrics are collected at multiple granularities:

### Per-Message Metrics

```rust
pub struct MessageTrace {
    pub msg_id: MsgId,
    pub priority: Priority,
    pub injected_at: SimTime,
    pub src: NodeId,
    pub dst: NodeId,
    pub delivered_at: Option<SimTime>,
    pub expired_at: Option<SimTime>,
    pub hop_count: u8,
    pub total_transmissions: u32,  // copies made across all nodes
    pub route_taken: Vec<(NodeId, TransportType, SimTime)>,
}
```

### Per-Node Metrics

```rust
pub struct NodeMetrics {
    pub node_id: NodeId,
    pub messages_generated: u32,
    pub messages_received: u32,
    pub messages_forwarded: u32,
    pub messages_dropped_buffer_full: u32,
    pub bytes_transmitted: u64,
    pub bytes_received: u64,
    pub contact_count: u32,
    pub total_contact_duration_s: f64,
    pub battery_remaining_pct: f64,
    pub alive_fraction: f64,  // Fraction of sim time node was operational
}
```

### Aggregate Metrics (Time Series)

Every 1 second of simulation time:

```rust
pub struct MetricsSnapshot {
    pub sim_time: f64,
    pub active_nodes: u32,
    pub messages_in_flight: u32,  // In transit or buffered
    pub cumulative_delivered: u32,
    pub cumulative_injected: u32,
    pub delivery_ratio: f64,
    pub mean_latency_s: f64,
    pub active_contacts: u32,
    pub bandwidth_utilization: HashMap<TransportType, f64>,
}
```

## Visualization

The Python layer provides visualization of simulation results:

```python
# tools/viz/sim_visualizer.py

class SimVisualizer:
    def plot_delivery_timeline(self, result: SimResult):
        """Delivery ratio over time"""

    def plot_contact_graph(self, result: SimResult, at_time: float):
        """Network contact graph at a given simulation time"""

    def plot_message_traces(self, result: SimResult, msg_ids: List[str]):
        """Hop-by-hop message traces on a map"""

    def animate_mobility(self, result: SimResult):
        """Animated node positions and contacts over time"""

    def plot_battery_drain(self, result: SimResult):
        """Battery level over time per node"""

    def plot_routing_overhead(self, result: SimResult):
        """Routing overhead ratio over time"""
```

## Routing Algorithm Plugin API

The simulator supports pluggable routing algorithms for comparative evaluation:

```rust
pub trait RoutingAlgorithm: Send + Sync {
    fn name(&self) -> &str;

    /// Called when a contact opportunity begins.
    /// Returns list of messages to transmit to this peer.
    fn on_contact(
        &mut self,
        local_node: &NodeState,
        peer_node: &NodeState,
        local_buffer: &[Message],
    ) -> Vec<MessageId>;

    /// Called when a message is received.
    /// Returns true if message should be stored.
    fn on_receive(
        &mut self,
        msg: &Message,
        local_node: &NodeState,
    ) -> bool;

    /// Periodic update — can use to maintain routing tables.
    fn on_tick(&mut self, sim_time: f64, local_node: &NodeState);

    /// Collect algorithm-specific metrics
    fn metrics(&self) -> HashMap<String, f64>;
}
```

Built-in routing algorithms:

```rust
pub struct EpidemicRouting { dedup_cache: BloomFilter }
pub struct ProphetRouting { delivery_predictability: HashMap<NodeId, f64>, aging: f64 }
pub struct SprayAndWait { copies_remaining: HashMap<MsgId, u8>, l: u8 }
pub struct GeographicRouting { /* requires position awareness */ }
pub struct HybridRouting { /* PRoPHET + geographic fallback */ }
```

Register and compare:

```bash
# Run comparative evaluation
cargo run --bin iris-sim -- compare \
  --algorithms epidemic,prophet,spray_wait_l8,hybrid \
  --scenario urban_disaster \
  --runs 10 \
  --output comparison_results/
```

## Configuration Reference

```toml
[simulation]
duration_s = 3600
random_seed = 42          # Reproducible runs; null = random

[simulation.time_control]
mode = "accelerated"      # "realtime" | "accelerated" | "stepped"
max_speed_factor = 1000   # Max simulation:wall ratio

[simulation.nodes]
default_storage_mb = 500

[simulation.metrics]
snapshot_interval_s = 1.0
record_event_log = false   # True: full event log (large files)
record_message_traces = true

[simulation.output]
format = "sqlite"          # "sqlite" | "json" | "parquet"
path = "results/"
compress = true
```
