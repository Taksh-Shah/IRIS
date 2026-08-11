# Search and Rescue Coordination

## Overview

IRIS is designed to support search and rescue (SAR) operations in environments where
standard communication infrastructure has failed. This document describes SAR-specific
features, protocols, and integration points.

## SAR Team Communication Requirements

A typical IRIS-supported SAR operation involves:
- **Incident Command Post (ICP)**: coordination center with likely satellite/Internet access
- **Field Teams (3-8 persons each)**: operating in terrain with no cellular coverage
- **Ground SAR Teams**: on foot in the search area
- **Air Assets**: helicopters or drones extending mesh coverage
- **Medical Teams**: forward medical posts requiring status updates

Communication needs:
- **Command → teams**: tasking, search grid assignments, safety instructions
- **Teams → command**: progress reports, find/no-find, medical status
- **Team ↔ team**: coordination at boundaries, handoff of search grids
- **Medical status**: casualty triage from field to ICP

## SAR Team Setup

### Team Channel

Each SAR deployment creates an encrypted team channel:

```
Channel ID: SAR-{incident_id}-{team_id}  (e.g., SAR-GJ2024-TEAM3)
Channel type: closed_group (only invited members)
Admin: team leader (holds admin certificate for the channel)
Members: team members + ICP coordinator
```

Team leader distributes channel membership via QR code at briefing, or via IRIS
contact share if devices are already linked.

### Authority Verification

Team leader's device holds a SAR authority certificate (issued by DDMA or NDRF).
This allows:
- Sending elevated-priority (P1) team coordination messages
- Accessing emergency broadcast channel for this incident
- Issuing moderation commands in the incident channel

Certificate issued offline at pre-deployment briefing: certificate on USB drive or
QR code, loaded onto team leader device.

## Missing Person Protocol

When a missing person is reported:

### Step 1: Create Missing Person Event

```rust
pub struct MissingPersonEvent {
    pub event_id: [u8; 16],
    pub reporter_id: NodeId,
    pub reported_at: i64,
    
    // Person description
    pub name: Option<String>,
    pub age_range: Option<AgeRange>,
    pub description: String,  // Physical description, clothing
    pub last_known_location: Option<CompactLocation>,
    pub last_seen_at: Option<i64>,
    pub last_seen_description: String,
    
    // Photo (optional, may be too large for LoRa — sends via BLE/Wi-Fi only)
    pub photo_hash: Option<[u8; 32]>,  // Hash if photo is available separately
    
    // Contact for reporting sighting
    pub contact_node_id: NodeId,
    pub contact_info: String,  // ICP coordinator contact
}
```

### Step 2: Broadcast to Local Mesh and Emergency Channel

Missing person event sent as P2 (Location priority — SAR context) to:
- Local mesh (epidemic routing)
- Area emergency channel

Every person in the area with IRIS sees the missing person notification with description
and last known location.

### Step 3: FOUND Report

Any IRIS user who spots the missing person sends a FOUND report:

```rust
pub struct MissingPersonFound {
    pub event_id: [u8; 16],     // Reference to missing person event
    pub reporter_id: NodeId,
    pub found_at: i64,
    pub location: CompactLocation,
    pub confidence: FoundConfidence,  // Certain | Probable | Possible
    pub notes: String,               // Any additional information
}
```

FOUND report sent as P2, propagated to ICP and search teams.
FOUND automatically cancels the missing person broadcast.

## SAR Mesh Deployment

### Team Member Devices

All team members carry IRIS-enabled smartphones:
- Pre-loaded with SAR channel keys and team leader certificate
- GPS always-on mode (SAR mode: continuous GPS acquisition)
- IRIS in foreground (prevents OS background restrictions)

### Edge Node Deployment

For large-area searches, edge nodes are deployed:
- **Backpack edge nodes**: carried by team members, extend mesh at team's current location
- **Fixed hilltop nodes**: deployed at ridge lines for longer range (LoRa gateway nodes)
- **Vehicle nodes**: command post vehicle with satellite uplink and long-range LoRa

Edge node spacing: 500m-2km depending on terrain and transport (BLE vs. LoRa).
With LoRa nodes, SAR mesh can cover 50-100 km² per edge node cluster.

### Mobile Mesh Backbone

As teams move through the search area, they carry the mesh with them:
- Each team member's phone is a relay node
- Edge nodes at team positions create denser mesh at active search locations
- Helicopter with IRIS node extends range significantly when overhead

### Helicopter/Drone as Relay

An air asset with an IRIS node (tablet, edge node) can relay between ground teams
that are not directly connected:
```
Team Alpha (valley A) → Helicopter → Team Bravo (valley B)
Range: 5-15km line of sight for BLE/Wi-Fi Direct, 30-50km for LoRa from helicopter altitude
```

The helicopter's elevated position dramatically extends effective mesh range.
IRIS recognizes aerial relay nodes and adjusts routing to use them when available
(by advertising altitude in vehicle relay announcement).

## Location Tracking

### Team Member Location Sharing

All team members in an active SAR operation share continuous location with team leader
(P2, every 5 minutes):
- Team leader's device shows map of all team member locations
- ICP coordinator sees all team locations via command-post node
- Historical track for each member (last 2 hours, enables "last known position" if contact lost)

### Grid Coverage Tracking (Future Feature)

Each team's GPS track is recorded and shared with ICP. ICP coordinator can visualize:
- Which search grids have been covered
- Where teams are currently operating
- Gaps in coverage that need assignment

This is a future feature (IRIS v0.4) requiring a specialized ICP coordination UI.

## Medical Status Messages

Field medics send triage status for casualties:

```rust
pub struct MedicalStatusUpdate {
    pub status_id: [u8; 16],
    pub reporting_medic: NodeId,
    pub timestamp: i64,
    pub location: CompactLocation,
    
    pub triage_category: TriageCategory,  // Immediate | Delayed | Minimal | Expectant | Dead
    pub patient_count: u8,
    pub immediate_count: u8,  // RED — life-threatening, need immediate treatment
    pub delayed_count: u8,    // YELLOW — serious but not immediately life-threatening
    pub minimal_count: u8,    // GREEN — walking wounded
    pub expectant_count: u8,  // BLACK — unsurvivable with available resources
    
    pub resources_needed: String,  // e.g., "2 stretchers, O2, IV fluids"
    pub evacuation_requested: bool,
    pub notes: String,
}
```

Medical status messages sent at P1 (Medical priority). Propagated to ICP immediately.
ICP coordinator uses these to coordinate medical resource allocation and helicopter evacuation.

## Interoperability (Future)

### APRS Integration

Amateur Packet Reporting System (APRS) is used by ham radio operators worldwide for
position reporting and messaging. IRIS-to-APRS gateway (future):
- IRIS node positions reported to APRS network via Internet (when available)
- APRS-IS messages from ham operators relayed into IRIS mesh
- Enables coordination between professional SAR (IRIS) and volunteer ham radio community

### TETRA Integration

TETRA (Terrestrial Trunked Radio) is used by Indian police (MP-COPS), NDRF, and some
state disaster authorities. IRIS-to-TETRA gateway (future):
- Voice calls bridged between TETRA and IRIS (quality limited by IRIS message size)
- Status messages from TETRA operators appear in IRIS SAR channel
- Enables IRIS teams to coordinate with TETRA-equipped NDRF units

### OsmAnd / Maps.me Integration

Offline maps are essential for SAR. IRIS integrates with offline map apps:
- IRIS node locations can be exported to OsmAnd/Maps.me as waypoints
- Missing person location appears as a pin in the map app
- Search grid overlays (future) for team assignment
