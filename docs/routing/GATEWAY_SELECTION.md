# Gateway Selection Algorithm

## Overview

A gateway is an IRIS node that provides access to a transport not directly
available to all nodes. Common gateways:
- **Internet gateway**: node with cellular data or Ethernet to ISP
- **LoRa gateway**: node with external LoRa hardware
- **Satellite gateway**: node with Iridium GO! or Starlink terminal

Gateway selection determines which gateway handles a message when a node cannot
reach the destination directly. Wrong selection wastes bandwidth, costs money
(satellite), or results in failed delivery.

## What Makes a Node a Gateway

Nodes self-advertise gateway capability in their capability bundle:

```rust
pub struct GatewayCapability {
    pub gateway_type: GatewayType,
    pub transport_id: TransportId,

    // Quality metrics
    pub bandwidth_bps: u64,
    pub latency_ms: u32,
    pub reliability_score: f32,   // 0.0–1.0, based on historical ACK rate
    pub cost_factor: f32,         // 0.0 = free, 1.0 = very expensive

    // Availability
    pub current_load: f32,        // 0.0–1.0
    pub queue_depth: u32,         // messages waiting
    pub duty_cycle_remaining: f32, // relevant for LoRa

    // Priority support
    pub min_priority: Priority,   // lowest priority served (e.g., satellite: P0 only)
    pub max_priority: Priority,   // highest priority served (always P0)
}

pub enum GatewayType {
    Internet { has_ipv6: bool },
    LoRa { frequency_hz: u32, sf: SpreadingFactor },
    Satellite { provider: SatelliteProvider },
    USBEthernet,
}
```

## Gateway Quality Score

Each gateway is scored before selection:

```rust
pub fn compute_gateway_quality(
    gw: &GatewayCapability,
    msg: &IrisMessage,
) -> f32 {
    // Base quality components (each 0.0–1.0, weighted sum)

    // Reliability: how often does this gateway successfully deliver?
    let reliability = gw.reliability_score;

    // Bandwidth: normalized against expected max
    let bandwidth_factor = (gw.bandwidth_bps as f32 / MAX_EXPECTED_BPS).min(1.0).sqrt();
    // sqrt() compresses the range — log(100) vs log(1) is less dramatic than 100 vs 1

    // Latency: lower is better. Normalize to 0–1 where 1 = instant, 0 = 60s+
    let latency_factor = 1.0 - (gw.latency_ms as f32 / 60_000.0).min(1.0);

    // Cost: 0 = free (factor = 1.0), 1 = max cost (factor = 0.0)
    let cost_factor = 1.0 - gw.cost_factor;

    // Availability: less load = better
    let availability = (1.0 - gw.current_load) * (1.0 - gw.queue_depth as f32 / 100.0).max(0.0);

    // Duty cycle: for LoRa, how much capacity remains
    let duty_cycle_ok = if gw.gateway_type.has_duty_cycle() {
        gw.duty_cycle_remaining
    } else {
        1.0
    };

    // Weighted sum
    let q = 0.30 * reliability
          + 0.20 * bandwidth_factor
          + 0.15 * latency_factor
          + 0.20 * cost_factor
          + 0.10 * availability
          + 0.05 * duty_cycle_ok;

    // Priority compatibility multiplier
    let priority_ok = if gw.serves_priority(msg.priority) { 1.0 } else { 0.0 };

    q * priority_ok
}
```

## Priority Compatibility

Not all gateways serve all priorities. This is enforced, not just preferred:

```rust
impl GatewayCapability {
    pub fn serves_priority(&self, priority: Priority) -> bool {
        match self.gateway_type {
            GatewayType::Satellite { provider: SatelliteProvider::IridiumSBD } => {
                // SBD: P0–P2 only (340 byte limit, cost)
                priority <= Priority::P2
            }
            GatewayType::Satellite { provider: SatelliteProvider::Starlink } => {
                // Starlink: full IP, P0–P3 (P4+ too expensive for satellite)
                priority <= Priority::P3
            }
            GatewayType::LoRa { .. } => {
                // LoRa: P0–P3 only (bandwidth too limited for P4+)
                priority <= Priority::P3
            }
            GatewayType::Internet { .. } => {
                // Internet: all priorities
                true
            }
            GatewayType::USBEthernet => {
                // USB Ethernet: all priorities
                true
            }
        }
    }
}
```

## Selection Algorithm

```rust
pub fn select_gateway(
    msg: &IrisMessage,
    known_gateways: &[GatewayCandidate],
    fallback_order: &[GatewayType],
) -> GatewaySelection {
    // Filter to eligible gateways
    let eligible: Vec<_> = known_gateways.iter()
        .filter(|gw| gw.capability.serves_priority(msg.priority))
        .filter(|gw| !gw.is_failed())  // exclude recently failed gateways
        .collect();

    if eligible.is_empty() {
        return GatewaySelection::None { reason: "No compatible gateway available" };
    }

    // Score each gateway
    let mut scored: Vec<(f32, &GatewayCandidate)> = eligible.iter()
        .map(|gw| (compute_gateway_quality(&gw.capability, msg), *gw))
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());

    // For P0: use ALL gateways simultaneously (multi-gateway mode)
    if msg.priority == Priority::P0 {
        return GatewaySelection::All {
            gateways: scored.iter().map(|(_, gw)| gw.node_id.clone()).collect(),
        };
    }

    // For P1–P2: use top gateway + one backup if available
    if msg.priority <= Priority::P2 && scored.len() >= 2 {
        return GatewaySelection::WithBackup {
            primary: scored[0].1.node_id.clone(),
            backup: scored[1].1.node_id.clone(),
            primary_score: scored[0].0,
        };
    }

    // For P3+: use best single gateway
    GatewaySelection::Single {
        gateway: scored[0].1.node_id.clone(),
        score: scored[0].0,
    }
}
```

## Gateway Discovery

Gateways are discovered at multiple hop distances:

```rust
pub struct GatewayCandidate {
    pub node_id: NodeId,
    pub capability: GatewayCapability,
    pub hop_count: u8,          // 0 = direct neighbor, 1 = one hop away, etc.
    pub path: Vec<NodeId>,      // intermediate hops to reach gateway
    pub discovered_at: SystemTime,
    pub last_confirmed: SystemTime,
}

pub enum GatewayDiscoverySource {
    DirectNeighbor,             // gateway is in BLE/Wi-Fi range
    NeighborAdvertised,         // neighbor told us about a gateway 1 hop away
    RoutingTableInferred,       // routing table shows path through gateway node
}
```

Gateway advertisement propagation:

```rust
// Each node re-advertises gateways it knows about to its neighbors
// with hop_count incremented
// Only propagate gateways with hop_count <= 2 (to limit overhead)
pub fn rebroadcast_gateway(gw: &GatewayCandidate, my_id: &NodeId) -> Option<GatewayAnnouncement> {
    if gw.hop_count >= 2 {
        return None;  // Don't propagate further
    }
    Some(GatewayAnnouncement {
        gateway_node_id: gw.node_id.clone(),
        capability: gw.capability.clone(),
        hop_count: gw.hop_count + 1,
        advertised_by: my_id.clone(),
    })
}
```

## Gateway Failure Detection

```rust
pub struct GatewayHealthMonitor {
    failure_counters: HashMap<NodeId, u32>,
    last_success: HashMap<NodeId, SystemTime>,
    failed_gateways: HashSet<NodeId>,
}

impl GatewayHealthMonitor {
    const FAILURE_THRESHOLD: u32 = 3;
    const RECOVERY_WINDOW: Duration = Duration::from_secs(300); // 5 minutes

    pub fn record_ack_timeout(&mut self, gateway_id: &NodeId) {
        let count = self.failure_counters.entry(gateway_id.clone()).or_insert(0);
        *count += 1;
        if *count >= Self::FAILURE_THRESHOLD {
            self.failed_gateways.insert(gateway_id.clone());
            log::warn!("Gateway {:?} marked as failed after {} consecutive timeouts",
                gateway_id, count);
        }
    }

    pub fn record_success(&mut self, gateway_id: &NodeId) {
        self.failure_counters.insert(gateway_id.clone(), 0);
        self.last_success.insert(gateway_id.clone(), SystemTime::now());
        self.failed_gateways.remove(gateway_id);
    }

    pub fn is_failed(&self, gateway_id: &NodeId) -> bool {
        self.failed_gateways.contains(gateway_id)
    }
}
```

## Gateway Recovery

Failed gateways are re-evaluated when:
1. A new advertisement from the gateway is received (gateway advertises it's back)
2. After RECOVERY_WINDOW (5 minutes): mark as "probation" — use with lower score
3. After successful delivery: remove from failed set, restore full score

```rust
pub fn handle_gateway_advertisement(&mut self, announcement: GatewayAnnouncement) {
    let gateway_id = &announcement.gateway_node_id;

    if self.health_monitor.is_failed(gateway_id) {
        // Gateway is back online — move to probation
        self.health_monitor.set_probation(gateway_id);
        // Apply 50% quality score penalty during probation
        self.gateway_score_modifiers.insert(gateway_id.clone(), 0.5);
        log::info!("Gateway {:?} back online, in probation", gateway_id);
    }

    // Update capability with fresh advertisement
    self.known_gateways.insert(gateway_id.clone(), GatewayCandidate {
        node_id: gateway_id.clone(),
        capability: announcement.capability,
        hop_count: announcement.hop_count,
        discovered_at: self.known_gateways
            .get(gateway_id)
            .map(|g| g.discovered_at)
            .unwrap_or_else(SystemTime::now),
        last_confirmed: SystemTime::now(),
    });
}
```

## Example: Disaster Scenario Gateway Selection

Scenario: 3 available gateways, message is P1 Evacuation Order.

| Gateway | Type | Reliability | Latency | Cost | Score |
|---------|------|------------|---------|------|-------|
| Node-A | Internet (4G) | 0.92 | 80ms | 0.1 | 0.82 |
| Node-B | LoRa | 0.85 | 5000ms | 0.0 | 0.64 |
| Node-C | Satellite SBD | 0.99 | 45000ms | 0.8 | 0.51 |

Selection for P1 message:
- Primary: Node-A (Internet, best score)
- Backup: Node-B (LoRa, second best)
- Node-C: eligible but not selected (high cost, high latency)

If Node-A fails after 3 timeouts:
- Primary: Node-B (LoRa)
- Backup: Node-C (Satellite, despite cost — P1 warrants it)
