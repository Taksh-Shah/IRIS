# Temporal Graph

## Overview
Unlike static networks, IRIS operates on a temporal graph where link availability changes over time. The same two nodes may be in contact for 5 minutes every hour, or once a day for 30 minutes. Messages must be scheduled considering when links will be available, not just whether they currently exist.

Formally: the IRIS network is a time-varying graph G(t) = (V, E(t)) where E(t) is not static but a function of time. Each edge e has an associated temporal contact sequence: a set of time intervals during which the link is active.

## Contact Opportunity Model

A contact opportunity is a time window [t_start, t_end] during which node A can communicate with node B via transport T with reliability r.

### Contact Opportunity Sources

**1. Physical Proximity (Opportunistic)**
Mobile nodes meeting by chance. No schedule. Detected at time of contact.
- Example: two commuters passing each other with phones in pocket
- Detected via: BLE advertisement scan, Wi-Fi Aware discovery
- Duration: seconds to minutes
- Predictability: zero (but past contact patterns inform PRoPHET estimates)

**2. Periodic Vehicle Routes (Scheduled)**
Relay vehicle traveling a known route on a regular schedule.
- Example: ambulance travels from remote village to district hospital every morning
- Duration: minutes at each waypoint
- Predictability: high (if schedule is maintained)
- Configured as: contact schedule (see below)

**3. Gateway Availability Windows (Semi-Scheduled)**
Gateways that power on at specific times.
- Example: satellite terminal powered 08:00-09:00 daily to conserve energy
- LoRa gateway with intermittent power
- Predictability: high when schedule known, medium when inferred from history

**4. User Behavior Patterns (Learned)**
Users have predictable daily patterns that create contact opportunities.
- Example: coworkers in same office → daily contact 09:00-17:00
- Commuters → morning and evening contact windows
- These patterns are learned from contact history and used by PRoPHET

**5. Scheduled Checkpoints (Explicit)**
NDRF/SDRF teams establish communication checkpoints during disaster operations.
- Example: relay team at checkpoint A contacts headquarters every 2 hours
- Configured via: operator tool or in-app schedule

## Contact Schedule Format

For predictable contacts, operators can configure contact schedules:
```yaml
# contact_schedules.yaml
schedules:
  - id: "sched_kutch_hospital_route"
    description: "Ambulance route Bhuj Hospital to Nakhatrana"
    contacts:
      - from: "node_vehicle_ambulance_01"
        to_region: "IN-GJ-19-BHUJ"   # Region-based contact (any node in region)
        transport: BLE
        windows:
          - days: [Mon, Tue, Wed, Thu, Fri, Sat]
            start_time: "07:30"
            end_time: "08:15"
          - days: [Mon, Tue, Wed, Thu, Fri, Sat]
            start_time: "17:00"
            end_time: "17:45"
        reliability_estimate: 0.85
        
  - id: "sched_satellite_window"
    description: "Satellite terminal at Bhuj collector office"
    contacts:
      - from: "node_edge_bhuj_01"
        to: SATELLITE_GATEWAY
        transport: SATELLITE
        windows:
          - days: [Mon, Tue, Wed, Thu, Fri, Sat, Sun]
            start_time: "08:00"
            end_time: "08:30"
          - days: [Mon, Tue, Wed, Thu, Fri, Sat, Sun]
            start_time: "20:00"
            end_time: "20:30"
        reliability_estimate: 0.92
        notes: "GSAT-16 visibility window at this location"
```

### Schedule Distribution
Contact schedules are shared via:
- Pre-loaded on nodes during setup (for known operational schedules)
- Distributed via mesh gossip (for dynamically learned schedules)
- Updated via Internet when available (for operator schedule changes)

## TTL Calculation in Temporal Networks

Message TTL must be designed around the expected contact opportunity timeline.

### TTL Defaults by Priority
```
P0  SOS:           TTL = 72 hours  // Must survive until ANY contact opportunity
P1  Medical:       TTL = 24 hours
P2  Location:      TTL = 12 hours  // Location data stales quickly
P3  Emergency text: TTL = 12 hours
P4  Normal text:   TTL = 8 hours
P5  Image:         TTL = 4 hours
P6  Voice:         TTL = 2 hours
P7  Video/Bulk:    TTL = 1 hour
```

### TTL Encoding
Dual TTL: message expires at the FIRST of:
- `hop_limit`: maximum relay hops (prevents infinite forwarding in loops)
- `expires_at`: absolute UTC timestamp (prevents stale messages persisting)

```
hop_limit:
  P0: 100 hops (effectively unlimited)
  P1-P3: 20 hops
  P4-P7: 10 hops
```

### Adaptive TTL (Future)
When contact schedule is known, TTL can be adapted:
- P4 message in area with hourly scheduled contacts → TTL can be shorter
- P4 message in remote area with weekly vehicle visit → TTL should be extended

Not implemented in v1. Tracked in roadmap as ADAPTIVE_TTL feature.

## Scheduling vs Opportunistic Delivery

### Opportunistic Delivery (Default)
No schedule assumed. Message sent whenever any contact opportunity appears.
- Simple, resilient
- Works even when schedule assumptions are wrong
- Potentially slower delivery if better opportunities exist later

### Scheduled Delivery (Optional)
When contact schedule is known and configured:
- Message held until scheduled contact window opens
- Transport prepared and warmed up before window start
- Maximizes use of contact window bandwidth

### Combined Strategy
For P0-P2 emergency messages: always opportunistic (never wait for schedule).
For P4-P7 bulk messages: scheduled delivery preferred when schedule known (conserves battery by not attempting delivery outside windows).

Implementation in TransportManager:
```rust
fn should_attempt_delivery_now(msg: &Message, schedule: &ContactSchedule) -> bool {
    if msg.priority <= Priority::P2 {
        return true; // Emergency: never wait
    }
    if schedule.next_window(msg.recipient).is_within(Duration::from_secs(300)) {
        return true; // Next window in 5 minutes: prepare now
    }
    if schedule.is_in_window_now(msg.recipient) {
        return true; // Window is open: send now
    }
    false // Wait for next window
}
```

## Clock Synchronization Problem

In a disconnected mesh, nodes may not have synchronized clocks. GPS provides time on phones with GPS fix. Without GPS or Internet NTP, clocks can drift.

### Measured Clock Drift
- Modern phone clock without sync: ±1 second per day typical
- Older hardware: up to ±30 seconds per day
- After 7 days without sync: ±7 to ±210 seconds drift possible
- Acceptable skew for IRIS: ±30 minutes

### Clock Sync Approach in IRIS
1. **On contact**: exchange timestamps, log skew
2. **Skew detection**: if |local_time - peer_time| > 30 minutes, flag as CLOCK_SKEW_DETECTED
3. **Conservative TTL enforcement**: use minimum (sender_expiry, receiver_expiry) interpretation
4. **No clock adjustment**: IRIS does not adjust system clock (security concern: malicious nodes could cause clock attacks)
5. **GPS priority**: GPS time is trusted when available; always prefer GPS-synchronized nodes for TTL reference

### Clock Skew Impact on TTL
Conservative approach: use the interpretation that expires the message sooner.
- Sender timestamp: T_s = 10:00:00, TTL = 8h → expires 18:00:00 sender time
- Receiver timestamp: T_r = 10:25:00 (receiver clock 25min fast)
- Receiver interprets: message expires at 18:00:00 receiver time = 17:35:00 sender time
- Message may expire 25 minutes early from sender's perspective
- This is the correct trade-off: we lose 25 minutes of TTL, not 25 minutes of stale delivery

### NTP Unavailability
Do NOT rely on NTP for TTL enforcement. NTP requires Internet. In disaster scenarios where IRIS is most needed, NTP may be unavailable. TTL calculations must be robust to NTP absence.

## Temporal Graph Analytics

For operator use and network planning, IRIS records temporal contact data:

### Contact Duration Distribution
```
For each pair (A, B): record contact durations
Metrics: mean, median, 90th percentile, min, max
Use: identify high-reliability links vs fleeting contacts
```

### Inter-Contact Time Distribution
```
For each pair (A, B): record time between contacts
Metrics: mean, 90th percentile
Use: estimate delivery delay for messages targeting this pair
```

### Contact Frequency Heatmap
```
For each node: contact frequency by hour of day, day of week
Visualization: heatmap in operator tool
Use: identify optimal scheduled contact windows
```

### Delivery Delay Estimation
Given: message priority, source node, destination node, current network state
Estimate: expected delivery delay using contact history
Algorithm: Monte Carlo simulation using contact interval distributions (offline, not in critical path)

## Temporal Graph Storage

Temporal contact data stored in SQLite:
```sql
CREATE TABLE temporal_contacts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    node_a       BLOB NOT NULL,
    node_b       BLOB NOT NULL,
    transport    INTEGER,
    start_time   INTEGER NOT NULL,    -- Unix timestamp
    end_time     INTEGER,             -- NULL if contact ongoing
    duration_s   INTEGER,            -- Computed on close
    bytes_xfrd   INTEGER DEFAULT 0
);
CREATE INDEX idx_temporal_node_a ON temporal_contacts(node_a, start_time);
CREATE INDEX idx_temporal_node_b ON temporal_contacts(node_b, start_time);

-- Retention: keep 90 days of contact history
-- Cleanup: DELETE FROM temporal_contacts WHERE start_time < (now - 90 days)
```

## Integration with Routing Engine

The temporal graph feeds into routing decisions:
1. PRoPHET uses contact history to estimate delivery probability
2. Scheduled contacts used by TransportManager for delivery timing
3. Contact duration estimates used to size the amount of data to transfer in a window
4. Inter-contact time estimates used to set per-link TTL recommendations

Routing engine queries temporal graph on each routing decision:
```rust
fn estimate_delivery_probability(&self, destination: NodeId) -> f32 {
    // PRoPHET: based on contact history
    self.prophet_state.delivery_probability(destination)
}

fn estimate_next_contact_time(&self, destination: NodeId) -> Option<Timestamp> {
    // From contact schedule (if configured) + contact history
    self.contact_schedule.next_window(destination)
        .or_else(|| self.contact_history.estimate_next_contact(destination))
}
```
