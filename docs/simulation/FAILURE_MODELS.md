# Failure Models

## Overview

IRIS is designed for disaster scenarios where infrastructure failure is the norm, not the exception. Simulation must exercise the full range of failure modes that occur in real disasters: sudden node failures, cascading battery exhaustion, gateway failures, and network partitions.

Failure models are pluggable — they implement a trait and inject failure events into the simulation timeline.

```rust
pub trait FailureModel: Send + Sync {
    /// Called at simulation start — schedule initial failures
    fn initialize(&mut self, nodes: &[SimNode], event_queue: &mut EventQueue, rng: &mut SimRng);

    /// Called periodically — can schedule additional failures
    fn on_tick(&mut self, sim_time: f64, nodes: &[SimNode], event_queue: &mut EventQueue, rng: &mut SimRng);

    /// Called when a node fails — can trigger cascade
    fn on_node_fail(&mut self, failed_node: NodeId, sim_time: f64, nodes: &[SimNode], event_queue: &mut EventQueue, rng: &mut SimRng);
}
```

## Node Failure Models

### Random Independent Failure

The simplest model: each node fails independently with probability λΔt per unit time (Poisson process). No correlation between failures.

```rust
pub struct RandomNodeFailure {
    pub failure_rate_per_hour: f64,  // Mean time between failures = 1/rate
    pub recovery: Option<RecoveryModel>,
}

impl FailureModel for RandomNodeFailure {
    fn initialize(&mut self, nodes: &[SimNode], event_queue: &mut EventQueue, rng: &mut SimRng) {
        for node in nodes {
            let ttf = rng.exponential(1.0 / self.failure_rate_per_hour * 3600.0);
            event_queue.push(SimEvent::NodeFail { node_id: node.id, reason: FailureReason::Random }, ttf);
        }
    }
}
```

**Use case**: Background failure rate in normal operation. Models occasional device crashes, user turning off device, moving out of area.

**Typical parameters**:
- Normal operation: 0.1 failures/hour (mean 10 hours between failures)
- Stressed scenario: 2.0 failures/hour (mean 30 minutes)

### Correlated Failure (Area-Based)

Failures are geographically correlated — when infrastructure in an area fails (power outage, flood), all devices in that area fail simultaneously or nearly simultaneously.

```rust
pub struct AreaFailureModel {
    pub failure_zones: Vec<FailureZone>,
}

pub struct FailureZone {
    pub center: GeoPoint,
    pub radius_m: f64,
    pub failure_time: f64,       // When the zone fails
    pub failure_fraction: f64,   // Fraction of nodes in zone that fail
    pub failure_spread_s: f64,   // Std dev of failure timing within zone
    pub recovery: Option<ZoneRecoveryModel>,
}

impl FailureModel for AreaFailureModel {
    fn initialize(&mut self, nodes: &[SimNode], event_queue: &mut EventQueue, rng: &mut SimRng) {
        for zone in &self.failure_zones {
            let zone_nodes: Vec<_> = nodes.iter()
                .filter(|n| haversine(n.position, zone.center) <= zone.radius_m)
                .collect();

            let fail_count = (zone_nodes.len() as f64 * zone.failure_fraction) as usize;
            let selected = zone_nodes.choose_multiple(rng, fail_count);

            for node in selected {
                // Add jitter within the zone (simultaneous but not exactly identical)
                let t = zone.failure_time + rng.normal(0.0, zone.failure_spread_s);
                event_queue.push(
                    SimEvent::NodeFail { node_id: node.id, reason: FailureReason::AreaOutage },
                    t.max(0.0),
                );
            }
        }
    }
}
```

### Cascading Failure

When one node fails, it increases failure probability of neighboring nodes. Models power grid cascades or panic-induced device behavior.

```rust
pub struct CascadingFailureModel {
    pub initial_failure: Box<dyn FailureModel>,
    pub cascade_radius_m: f64,
    pub cascade_probability: f64,     // P(neighbor fails given node fails)
    pub cascade_delay_range_s: (f64, f64),
    pub max_cascade_hops: u8,
}

impl FailureModel for CascadingFailureModel {
    fn on_node_fail(&mut self, failed_node: NodeId, sim_time: f64, nodes: &[SimNode],
                    event_queue: &mut EventQueue, rng: &mut SimRng) {
        if self.cascade_hops_from_origin(failed_node) >= self.max_cascade_hops {
            return;
        }

        // Find neighbors of failed node
        let failed_pos = nodes[failed_node].position;
        let neighbors: Vec<_> = nodes.iter()
            .filter(|n| n.id != failed_node && n.state == NodeState::Active)
            .filter(|n| haversine(n.position, failed_pos) <= self.cascade_radius_m)
            .collect();

        for neighbor in neighbors {
            if rng.random::<f64>() < self.cascade_probability {
                let delay = rng.uniform(self.cascade_delay_range_s.0, self.cascade_delay_range_s.1);
                event_queue.push(
                    SimEvent::NodeFail { node_id: neighbor.id, reason: FailureReason::Cascade },
                    sim_time + delay,
                );
            }
        }
    }
}
```

## Battery Exhaustion Model

Battery exhaustion is not a random failure — it is deterministic given power consumption. The simulation tracks battery level per node.

```rust
pub struct BatteryModel {
    pub capacity_mah: f64,
    pub current_charge_mah: f64,

    // Power consumption by activity (in mA at 3.7V)
    pub consumption_idle_ma: f64,       // 5 mA — mostly sleeping
    pub consumption_ble_scan_ma: f64,   // 15 mA — BLE scan active
    pub consumption_ble_tx_ma: f64,     // 20 mA — BLE transmitting
    pub consumption_wifi_ma: f64,       // 150 mA — Wi-Fi Direct active
    pub consumption_lora_tx_ma: f64,    // 120 mA — LoRa transmitting
    pub consumption_cpu_routing_ma: f64, // 30 mA — routing computation
}

impl BatteryModel {
    pub fn consume(&mut self, activity: Activity, duration_s: f64) -> bool {
        let current_ma = match activity {
            Activity::Idle => self.consumption_idle_ma,
            Activity::BleScan => self.consumption_ble_scan_ma,
            Activity::BleTx => self.consumption_ble_tx_ma,
            Activity::WifiDirect => self.consumption_wifi_ma,
            Activity::LoraTx => self.consumption_lora_tx_ma,
            Activity::CpuRouting => self.consumption_cpu_routing_ma,
        };

        let consumed_mah = current_ma * duration_s / 3600.0;
        self.current_charge_mah -= consumed_mah;

        if self.current_charge_mah <= 0.0 {
            self.current_charge_mah = 0.0;
            return false;  // Battery depleted
        }
        true
    }

    pub fn level_pct(&self) -> f64 {
        self.current_charge_mah / self.capacity_mah * 100.0
    }
}
```

### Battery Exhaustion Cascade

When nodes with high relay load drain faster, this creates predictable cascade patterns:

1. High-centrality relay nodes (many messages passing through) drain first
2. Network loses relay capacity
3. Remaining nodes must relay more, drain faster
4. Network fragments

This cascade is important to simulate and design against. Countermeasures:
- Relay load balancing (spread relay burden across nodes)
- Battery-aware routing (avoid routing through low-battery nodes)
- Emergency mode (reduce non-critical activity when battery <20%)

## Link Failure Models

### Probabilistic Link Failure

Each active link has a probability of failing per unit time:

```rust
pub struct StochasticLinkFailure {
    pub failure_rate_per_minute: f64,   // Poisson process
    pub recovery_rate_per_minute: f64,
}
```

Models RF interference, temporary obstacles, people moving.

### Distance-Based Loss

Link quality degrades with distance according to the radio propagation model:

```rust
pub fn packet_loss_rate(distance_m: f64, transport: TransportType) -> f64 {
    match transport {
        TransportType::BLE => {
            // Log-normal shadowing
            let path_loss_db = 40.2 + 10.0 * 2.5 * (distance_m / 1.0).log10();
            let sensitivity_dbm = -93.0;
            let tx_power_dbm = 0.0;
            let margin_db = tx_power_dbm - sensitivity_dbm - path_loss_db;
            // Q-function approximation for probability of failure
            q_function(-margin_db / SHADOWING_STD_DEV)
        }
        TransportType::LoRa { spreading_factor } => {
            // LoRa uses link budget model
            lora_packet_loss(distance_m, spreading_factor)
        }
        _ => 0.0,
    }
}
```

## Gateway Failure Models

### Sudden Gateway Failure

A gateway (satellite uplink, internet-connected node) fails instantly:

```rust
pub struct SuddenGatewayFailure {
    pub gateway_id: NodeId,
    pub failure_at: f64,
    pub recovery_at: Option<f64>,
}
```

Tests whether the mesh can continue operating without internet connectivity.

### Graceful Gateway Degradation

Gateway advertises declining quality before failing — allows routing to adapt:

```rust
pub struct GracefulGatewayDegradation {
    pub gateway_id: NodeId,
    pub degrade_start: f64,
    pub failure_at: f64,
    pub bandwidth_schedule: Vec<(f64, u64)>,  // (time, bandwidth_bps)
}
```

The routing layer can detect bandwidth decline and proactively route traffic to alternative gateways.

### Multi-Gateway Failover

```yaml
# Test scenario: primary gateway fails, secondary takes over
failure_model:
  type: sequential_gateway_failure
  gateways:
    - id: gateway_0
      fail_at: 300   # seconds
      expected_failover_to: gateway_1
      failover_time_limit_s: 60  # Test requirement: failover within 60s
    - id: gateway_1
      fail_at: 1800
      expected_failover_to: gateway_2
```

## Network Partition Model

Partitions a network into isolated segments. Critical for testing store-carry-forward behavior.

```rust
pub struct NetworkPartitionModel {
    pub partition_at: f64,
    pub partition_line: PartitionLine,   // Geographic line dividing nodes
    pub link_types_severed: Vec<TransportType>,  // Which transports are cut
    pub merge_at: Option<f64>,
}

pub enum PartitionLine {
    Geographic { start: GeoPoint, end: GeoPoint },  // Line on map
    NodeGroup { group_a: Vec<NodeId>, group_b: Vec<NodeId> },  // Explicit groups
}
```

### Partition Scenarios

**River flooding** (geographic): Flood bisects city. Nodes on each bank cannot communicate via BLE/Wi-Fi. LoRa may still bridge if gateways on high ground.

**Building collapse**: Survivors trapped in different sections. BLE range sufficient within section, not between.

**Power failure zone**: Large area loses power. Battery-only devices drain faster, creating progressive partition.

## Message Corruption Model

Models bit errors in transmitted messages:

```rust
pub struct MessageCorruptionModel {
    pub ber: f64,   // Bit error rate (e.g., 1e-6)
}

impl MessageCorruptionModel {
    pub fn corrupt(&self, data: &[u8], rng: &mut SimRng) -> Vec<u8> {
        let mut corrupted = data.to_vec();
        for byte in &mut corrupted {
            for bit in 0..8 {
                if rng.random::<f64>() < self.ber {
                    *byte ^= 1 << bit;
                }
            }
        }
        corrupted
    }
}
```

Corrupted messages should:
1. Fail signature verification (detected and dropped) — this is the expected behavior
2. Never be delivered to application layer
3. Generate a corruption metric in results

## Earthquake Scenario: 70% Simultaneous Failure

The primary catastrophic failure scenario for IRIS. Models a major earthquake (M7.5+) hitting an urban area:

```rust
pub struct EarthquakeFailureModel {
    pub epicenter: GeoPoint,
    pub magnitude: f64,           // Richter scale
    pub occurrence_time_s: f64,
    pub failure_fraction_fn: Box<dyn Fn(f64) -> f64>,  // failure% as fn of distance
}

impl EarthquakeFailureModel {
    pub fn delhi_m75() -> Self {
        Self {
            epicenter: GeoPoint::new(28.6, 77.2),
            magnitude: 7.5,
            occurrence_time_s: 300.0,  // 5 minutes into simulation
            // Failure probability by distance from epicenter
            failure_fraction_fn: Box::new(|distance_km: f64| {
                if distance_km < 10.0 { 0.90 }       // 90% within 10km of epicenter
                else if distance_km < 30.0 { 0.70 }  // 70% within 30km
                else if distance_km < 50.0 { 0.40 }  // 40% within 50km
                else { 0.10 }                         // 10% beyond 50km
            }),
        }
    }
}
```

### Expected Behavior Under Earthquake Scenario

| Time (s) | Expected System State |
|----------|----------------------|
| 0-300 | Normal operation, routing converges |
| 300 | Earthquake: 70% nodes fail simultaneously |
| 300-360 | Routing storm: surviving nodes detect many lost neighbors |
| 360-600 | Routing stabilizes to surviving network topology |
| 600+ | Responders arrive (new nodes), gradually restore connectivity |
| 1800+ | Gateway comes back online (satellite uplink recovered) |

**Test assertions for earthquake scenario**:
- P0 SOS messages injected after earthquake: >80% delivered within 5 minutes
- No routing loops after topology stabilization
- System does not crash or corrupt data during the failure event
- Recovery nodes can join and receive pending messages

## Recovery Models

### Scheduled Recovery

```rust
pub struct ScheduledRecovery {
    pub recover_at: f64,
    pub recovery_jitter_s: f64,  // ±jitter
}
```

Models: power restored to area, device rebooted, user powers phone back on.

### Probabilistic Recovery

```rust
pub struct ProbabilisticRecovery {
    pub recovery_rate_per_hour: f64,  // Poisson process
}
```

Models: devices independently recovering at random times.

### Responder-Assisted Recovery

```rust
pub struct ResponderRecovery {
    /// Responders deploy relay nodes as they sweep the area
    pub responder_arrival_schedule: Vec<(f64, GeoPoint)>,  // (time, position)
    pub relay_deployment_radius_m: f64,
    pub relays_per_responder: u32,
}
```

As responders move through disaster zone, they deploy relay nodes, progressively restoring connectivity.

## Simultaneous Failure Analysis

For the earthquake scenario, track the following additional metrics:

```rust
pub struct SimultaneousFailureMetrics {
    pub failure_event_time: f64,
    pub nodes_before: u32,
    pub nodes_after: u32,
    pub failure_fraction: f64,

    // Message survival
    pub p0_messages_pending_at_failure: u32,
    pub p0_messages_delivered_post_failure: u32,
    pub p0_delivery_ratio_post_failure: f64,
    pub p0_mean_latency_post_failure_s: f64,

    // Network recovery
    pub time_to_routing_stability_s: f64,  // When routing table churn drops <threshold
    pub largest_connected_component_fraction: f64,  // At various times post-failure

    // Data integrity
    pub messages_corrupted: u32,   // Should be 0
    pub messages_duplicated: u32,  // Should be 0 after dedup
}
```

## Configuration Reference

```yaml
failure_models:
  # Random background failure
  - type: random_node_failure
    rate_per_hour: 0.1
    recovery:
      type: probabilistic
      rate_per_hour: 0.5

  # Earthquake at T=300s
  - type: earthquake
    epicenter: [28.614, 77.209]
    magnitude: 7.5
    at_time_s: 300
    failure_fractions:
      within_10km: 0.90
      within_30km: 0.70
      within_50km: 0.40
      beyond: 0.10
    recovery:
      type: scheduled
      start_at: 1800
      all_recovered_by: 7200

  # Link failure for LoRa (duty cycle exceeded simulation)
  - type: stochastic_link_failure
    transport: lora
    failure_rate_per_minute: 0.01
    recovery_rate_per_minute: 0.1
```
