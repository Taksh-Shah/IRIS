# Emergency Communication Architecture

## Overview

Emergency communication is not a feature in IRIS — it is the foundational design requirement.
Every architectural decision in IRIS is evaluated against the question: "Does this help or
hurt emergency communication?" This document describes the emergency communication architecture
as a cohesive system.

## Emergency as First-Class Requirement

In most communication platforms, emergency features are added after the core messaging
platform is designed. This creates fundamental conflicts: the emergency path competes with
normal traffic for the same resources, emergency UI is bolted onto a normal messaging UI.

IRIS inverts this: emergency communication is designed first. Normal messaging (P4+)
is designed around the emergency requirement, not alongside it.

**Architectural consequences:**
- Priority queue at every layer, not just the application layer
- Reserved storage that P4+ cannot access
- Separate radio resource management for P0-P2
- Battery policy that prioritizes P0 delivery over P4+ efficiency
- UI hierarchy where SOS is the most prominent element, not the least

## Priority Model

```
P0: SOS         — Immediate life threat. "I need help now."
P1: Medical     — Medical emergency. Casualty status, medical data.
P2: Location    — Location sharing for emergency coordination.
P3: Emergency text — Emergency text communication, situational awareness.
P4: Text        — Normal text messaging (degraded to background in emergency mode)
P5: Image       — Images and documents (suspended in emergency mode)
P6: Voice clip  — Voice messages (suspended in emergency mode)
P7: Video       — Video (suspended in emergency mode, Wi-Fi/Internet only)
```

Priority is enforced at:
- Queue ordering (P0 dequeued first, always)
- Storage allocation (P0-P2 reserved pool)
- Transport scheduling (P0-P2 get next available transmission slot)
- Battery policy (P0 forces battery_override)
- Routing aggressiveness (P0 uses epidemic routing, P7 uses conservative forwarding)

## System Components

### 1. SOS Module

Handles the full SOS lifecycle:
- Trigger detection (button, hardware button, widget, voice activation)
- SOS message composition and signing
- Multi-path, multi-transport broadcast
- ACK reception and delivery tracking
- False alarm cancel handling
- UI state management (SOS active / acknowledged / cancelled / expired)

Implementation: `crates/iris-emergency/src/sos/`

### 2. Emergency Broadcast System

Handles verified authority broadcast messages:
- Certificate chain verification (offline, no Internet required)
- Broadcast reception and display
- Mandatory relay of valid broadcasts (no node can suppress a valid broadcast)
- UI alert rendering (mandatory display, cannot be dismissed without acknowledgment)
- Broadcast propagation with replication factor

Implementation: `crates/iris-emergency/src/broadcast/`

### 3. Priority Routing Engine

Routes messages according to priority with emergency-specific behavior:
- Epidemic routing for P0-P1 (flood to all available paths)
- Multi-path routing for P0-P1 (simultaneous transmission on all transports)
- ACK-based routing for P2-P3 (delivery confirmation)
- Spray-and-wait for P4 (limited copies, bounded overhead)
- Conservative forwarding for P5-P7 (single path, defer to direct connection)

Implementation: `crates/iris-routing/src/priority_router.rs`

### 4. Location Sharing

Manages emergency and voluntary location sharing:
- GPS acquisition with accuracy feedback
- Binary location encoding (compact, LoRa-compatible)
- One-time and continuous sharing modes
- Location expiry and staleness tracking
- Consent management (explicit user action required)

Implementation: `crates/iris-emergency/src/location/`

### 5. Disaster Mode Controller

Coordinates the system-wide transition into and out of emergency operating mode:
- Mode state machine (NORMAL → DEGRADED → CRISIS → EMERGENCY)
- Automatic detection (SOS density, network degradation)
- Authority-commanded activation
- Resource reallocation across all subsystems
- Peer gossip for mode synchronization across mesh

Implementation: `crates/iris-emergency/src/disaster_mode.rs`

## Emergency State Machine

```
States:
  NORMAL:    Normal operation. All priorities active.
  DEGRADED:  Some infrastructure down. P7 suspended. P0-P2 get larger storage share.
  CRISIS:    Most infrastructure down, elevated SOS count. P5-P7 suspended.
             P0-P2 routing aggressiveness increased.
  EMERGENCY: Explicit activation by authority OR extreme conditions.
             All transports active. P4-P7 suspended. Battery override. Satellite activated.
```

### Transition Triggers

```
NORMAL → DEGRADED:
  - Gateway connectivity <50% of 7-day average
  - OR: infrastructure failure report received from authority
  
DEGRADED → CRISIS:
  - SOS count >5× baseline in rolling 30-minute window
  - OR: gateway connectivity <10% of 7-day average
  - OR: CRISIS_ALERT from authority certificate holder
  
CRISIS → EMERGENCY:
  - EMERGENCY_ACTIVATE from authority certificate holder
  - OR: SOS count >20× baseline (automatic)
  - OR: gateway connectivity = 0 for >30 minutes AND SOS count elevated

EMERGENCY → CRISIS:
  - EMERGENCY_DEACTIVATE from authority
  - OR: SOS count drops to <2× baseline for 30 minutes
  
CRISIS → DEGRADED, DEGRADED → NORMAL:
  - Inverse of forward conditions, with hysteresis (must meet condition for 15 minutes)
```

### State Effects

| Subsystem | NORMAL | DEGRADED | CRISIS | EMERGENCY |
|-----------|--------|----------|--------|-----------|
| P0-P2 storage | 100MB reserved | 200MB reserved | 500MB reserved | All available |
| P4-P7 storage | Up to capacity | Reduced 50% | Suspended | Suspended |
| BLE scan interval | 5s | 3s | 1s | 0.5s |
| BLE advertise | Continuous | Continuous | Continuous | Continuous |
| Wi-Fi Direct | On-demand | Active | Active | Active |
| LoRa (if available) | Active | Active | Active | Active + max power |
| Satellite | Off | Off | P0-P2 only | All emergency priorities |
| Battery override | No | No | P0-P1 only | All P0-P3 |
| UI disaster banner | No | Yellow | Orange | Red |

## Emergency Mode Deactivation

Deactivation is gradual (avoids sudden resource removal during ongoing events):

```
EMERGENCY → CRISIS: P5-P7 still suspended, resources slowly restored
CRISIS → DEGRADED: P5-P7 re-enabled with reduced quotas
DEGRADED → NORMAL: full quotas restored
```

Each step requires 15 minutes of stable conditions before transitioning.
Deactivation is always slower than activation (fail-safe design).

## Interaction with Security Systems

Emergency architecture interacts with security:
- **Emergency bypass of rate limits**: P0-P1 bypass spam rate limits (can't block emergency)
- **Emergency bypass of block lists**: blocked users can still send SOS (see EMERGENCY_ABUSE.md)
- **Authority certificate system**: broadcast requires certificate (see EMERGENCY_GOVERNANCE.md)
- **SOS accountability**: non-repudiable identity, rate limits, audit log (see EMERGENCY_ABUSE.md)
- **Disaster mode detection**: immune to false triggering (SOS density requires many genuine SOSes
  or a compromised authority certificate — addressed in EMERGENCY_ABUSE.md)
