# Disaster Mode Operation

## Overview

Disaster mode is a special operating mode where IRIS reconfigures all subsystems for maximum
emergency performance, suspending non-critical functions to dedicate resources to emergency
communication. This document describes what triggers disaster mode, what changes, and how
the network converges to disaster mode collectively.

## Activation Triggers

### Automatic Activation

IRIS monitors several signals that indicate a disaster may be occurring:

**Signal 1: SOS density surge**
```
Trigger: >10 SOS messages observed within a 1km² area in a 10-minute rolling window
         OR >5 SOS messages from the same area in 5 minutes (more severe)
Rationale: Individual SOS is personal emergency. Geographic clustering indicates mass casualty.
```

**Signal 2: Network degradation**
```
Trigger: Gateway connectivity drops to <10% of 7-day average for >15 minutes
         (measured as fraction of usual gateway nodes reachable)
Rationale: Network going down in disaster pattern (not random or scheduled)
```

**Signal 3: Rapid SOS rate increase**
```
Trigger: SOS rate increases by >10× in a 30-minute window compared to baseline
Rationale: Sudden surge without prior elevation = acute event
```

For automatic activation: device activates disaster mode locally and sends DISASTER_MODE_ENTER
gossip message to propagate to neighbors. Local activation is immediate — no waiting for
network consensus.

### Manual Activation (Authority Certificate)

Any holder of a valid authority certificate can send:
```rust
pub struct DisasterModeActivation {
    pub mode: DisasterMode,
    pub area_code: String,
    pub reason: String,
    pub activated_at: i64,
    pub expires_at: Option<i64>,  // None = until authority deactivates
    pub authority_id: NodeId,
    pub certificate_chain: Vec<AuthorityCertificate>,
    pub signature: Ed25519Signature,
}
```

Receiving nodes verify certificate, then activate disaster mode for their area.
Authority activation overrides automatic deactivation — manual deactivation is required.

## Mode Changes On Activation

### Routing Changes

| Behavior | Normal | Disaster Mode |
|----------|--------|---------------|
| P0 routing | Epidemic | Epidemic + all transports |
| P1-P2 routing | Multi-path | Epidemic |
| P4 routing | Spray-and-wait | Suspended |
| P5-P7 routing | Conservative | Suspended |
| Message replication | Limited | Unlimited for P0-P3 |
| Route diversity | Optimized | Maximum (all paths) |

### Storage Changes

```
Normal:         All priorities compete for storage
Disaster mode:  P0-P2: dedicated pool (all available storage up to 80%)
                P3:    up to remaining 20%
                P4:    suspended — no new P4 stored
                P5-P7: suspended — no new P5-P7 stored
                Existing P4-P7: evicted if storage needed for P0-P2
```

### Transport Changes

All transports are activated unconditionally in disaster mode:
- BLE Mesh: already active
- Wi-Fi Direct: activated if not already
- LoRa (if hardware present): activated at maximum transmit power
- Cellular SMS gateway: activated if any gateway node has cellular
- Satellite: activated for P0-P2 (Iridium SBD / Starlink)

### Battery Policy Changes

```
Normal:           Cutoff at 15% battery (conserve for phone calls)
Disaster mode:    Override cutoff for P0-P2 relay (down to 3% hardware minimum)
                  P0 SOS sender: override at any battery level
                  P3 relay: cutoff at 8% battery
```

Android Doze and iOS Background App Refresh restrictions are overridden in disaster mode
via OS emergency APIs where available (Emergency SOS APIs, foreground service).

### Scan Interval Changes

More aggressive neighbor discovery in disaster mode:
```
Normal BLE scan:        5-second active scan interval
Disaster mode BLE:      0.5-second active scan interval (10× more aggressive)
Battery cost:           ~30% higher battery drain from scanning
Justification:          Finding more relay nodes is worth the battery cost in disaster
```

### UI Changes

```
Normal:     Normal messaging UI
Disaster mode:  Red banner: "DISASTER MODE ACTIVE — Emergency only"
                Main screen shows SOS button larger
                Emergency broadcast section pinned at top
                Normal messaging deprioritized (moved to bottom tab)
                Battery level shown prominently
```

## Node Discovery in Disaster Mode

Phones scan more aggressively and advertise continuously in disaster mode:
- BLE scan interval: 0.5 seconds (normal: 5 seconds)
- Wi-Fi Direct: scan every 10 seconds (normal: 30 seconds)
- LoRa: beacon every 5 seconds on non-transmission slots

All transports advertise simultaneously in disaster mode (normal: staggered to save battery).

## Emergency Channel Auto-Join

In disaster mode, IRIS automatically joins the local emergency channel for the node's area:
- Channel determined by area code (derived from GPS, cell tower area, or manual setting)
- No user action required
- Official emergency broadcasts from area authorities arrive in this channel
- User can post situation reports to this channel

Channel ID format: `emergency_channel_{area_code}` — deterministic, no discovery needed.

## Vehicle Relay Protocol

In disaster mode, any IRIS node installed in a vehicle automatically offers enhanced relay service:
- Announces itself as a vehicle relay with estimated route (if GPS available)
- Increased storage quota for carry-forward (vehicle has power and space)
- Reports estimated route to mesh (helps routing decisions: "vehicle heading to Bhuj is a
  better carry-forward node than a stationary phone")
- Offers file relay (carry larger payloads that don't fit in real-time mesh)

Vehicle relay announcement:
```rust
pub struct VehicleRelayAnnouncement {
    pub relay_id: NodeId,
    pub vehicle_type: VehicleType,  // Car, Truck, Motorcycle, Helicopter, Boat
    pub current_location: Option<CompactLocation>,
    pub destination: Option<String>,  // Human-readable (e.g., "Bhuj relief camp")
    pub estimated_arrival: Option<i64>,
    pub storage_available_kb: u32,
    pub announcement_timestamp: i64,
}
```

## Mode Propagation (Gossip)

Disaster mode propagates to the entire mesh via gossip:

```
Node A enters disaster mode → sends DISASTER_GOSSIP to all neighbors
Each neighbor:
  1. Receives DISASTER_GOSSIP (with area_code, activated_at, activation_reason)
  2. If area_code matches local area AND activation is recent: enter disaster mode locally
  3. Propagate DISASTER_GOSSIP to their neighbors (with hop_count - 1)
  4. Stop propagating at hop_count = 0
```

Gossip hop limit: 5 hops for automatic activation (geographic containment).
Authority-commanded activation: 255 hops (unrestricted, covers full area).

Convergence time: in a typical mesh (50 nodes, 3 hops average), disaster mode
propagates to all nodes within 30-90 seconds.

## Deactivation

Manual deactivation:
- Authority certificate holder sends DISASTER_MODE_EXIT
- Propagates same as activation
- Nodes transition to CRISIS state, then gradually to NORMAL over 30 minutes

Automatic deactivation:
- SOS rate drops below 1× baseline for 30 consecutive minutes
- AND gateway connectivity restored to >50% of 7-day average
- AND no authority-commanded activation is active

Deactivation is intentionally slow — premature return to normal wastes resources
if the disaster is ongoing. Better to remain in disaster mode slightly too long than
to exit prematurely.

Deactivation sequence:
```
EMERGENCY → CRISIS (P5-P7 still suspended, resources gradually restored)
CRISIS → DEGRADED (P5-P7 re-enabled with reduced quotas, scan intervals relaxed)
DEGRADED → NORMAL (full restoration after 15 minutes of stable DEGRADED)
```

Each step: minimum 15 minutes before next step can occur.
