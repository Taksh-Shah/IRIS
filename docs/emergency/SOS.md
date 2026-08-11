# SOS System Design

## Overview

SOS is IRIS priority P0 — the highest priority message type. An SOS represents an immediate
threat to life. Every design decision for the SOS system is made with this in mind: speed,
reliability, and redundancy over efficiency, battery life, or storage.

## Trigger Mechanisms

### Primary: In-App Button

The in-app SOS button is designed to be:
- **Visible immediately** on the main screen — no navigation required
- **Large and red** — high visual priority
- **Protected against accidental trigger** — requires 3-second continuous hold

```
UI flow:
  1. User presses and holds SOS button
  2. Countdown ring fills (3 seconds) with audio cue
  3. Confirmation screen: "Send SOS? [location icon] Including GPS location"
     Options: [SEND SOS] [CANCEL]
  4. If no input for 3 seconds: auto-send (in extreme emergency, user may be incapacitated)
  5. SOS sent. UI transitions to SOS active state.
```

Auto-send on no input: controversial design choice. Rationale — if a user managed to hold
the button for 3 seconds and reach the confirmation screen, they almost certainly intend
to send SOS. An incapacitated user cannot tap "Cancel." The auto-send timeout provides
safety for this case.

### Secondary: Hardware Button Integration

On supported devices, a hardware button can be configured as SOS trigger:
- **Samsung**: Galaxy devices with Bixby key (Samsung Emergency SOS API integration)
- **Google**: Pixel devices with Emergency SOS (5 rapid power button presses)
- **Generic Android**: volume up + power button combination (device-specific)
- **iOS**: side button + volume button (iOS Emergency SOS API integration)

Hardware button integration operates even when the IRIS app is not in foreground,
and on some devices even when the screen is locked.

### Tertiary: Home Screen Widget

A widget can be added to the home screen for instant SOS access without opening the app.
Widget shows current IRIS status (connected/offline, battery) and a large SOS button.
Same 3-second hold requirement as in-app button.

### Future: Voice Activation

Investigating "Hey IRIS, SOS" voice trigger for situations where hands are not free.
Technical challenge: reliable wake-word detection without constant microphone processing
(battery concern). Target for IRIS v0.4 with hardware accelerated wake-word detection.

## SOS Message Format

SOS is designed to be as compact as possible for maximum transport compatibility,
especially LoRa (250-byte payload limit).

```rust
pub struct SosMessage {
    // Header (part of MessageEnvelope, not duplicated here)
    // type: MessageType::SOS (implied by priority=0)
    
    // Core SOS data (required)
    pub sender_id: NodeId,          // 32 bytes — sender identity
    pub timestamp: i64,             // 8 bytes — when SOS was triggered
    pub sequence: u8,               // 1 byte  — retransmit counter (0=first, 1=retry, etc.)
    
    // Location (optional, if GPS available and user consented)
    pub location: Option<CompactLocation>,
    
    // User message (optional, up to 64 bytes)
    pub message: Option<ShortText>,  // Default: b"HELP"
    
    // Contact info (optional)
    pub contact_name: Option<ShortText>,   // Up to 32 bytes (user's name)
    pub contact_phone: Option<ShortText>,  // Up to 16 bytes (phone number)
    
    // Device state
    pub battery_percent: u8,        // 1 byte
    pub has_gps: bool,              // 1 bit (packed with battery)
}

pub struct CompactLocation {
    pub lat_fixed: i32,    // 4 bytes — latitude × 10000, range ±90.0000°
    pub lon_fixed: i32,    // 4 bytes — longitude × 10000, range ±180.0000°
    pub accuracy_m: u16,   // 2 bytes — GPS accuracy in meters
    pub altitude_m: i16,   // 2 bytes — altitude (optional, 0 if unavailable)
    pub source: LocationSource, // GPS | NETWORK | PASSIVE
}
// CompactLocation total: 13 bytes — ±4m accuracy in latitude/longitude
```

**Total SOS message size target**: 150-200 bytes including all optional fields.
This fits in a single LoRa packet (250-byte limit), a single BLE advertisement extension,
a single SMS (160 characters / ~110 bytes binary).

## Propagation Behavior

SOS propagation is maximally aggressive — the opposite of P4-P7 conservative forwarding.

### Transport Selection

SOS is sent simultaneously on ALL available transports:
```
if BLE_MESH available: send via BLE_MESH
if WIFI_DIRECT available: send via WIFI_DIRECT  
if LORA available: send via LORA
if CELLULAR_SMS available: send via CELLULAR_SMS
if SATELLITE available: send via SATELLITE
// All simultaneously, not sequential
```

### Routing Strategy

Epidemic routing (flood): SOS is copied to every available neighbor node.
```
SOS epidemic rule:
  For each neighbor node N:
    If N has not seen this message_id: forward SOS to N
```

No hop count limit for P0: `max_hops = 255` (effectively unlimited).
No TTL expiry until ACK received or explicit cancel.

### Battery Override

SOS forces battery override mode: the phone continues transmitting even below the normal
battery cutoff (typically 10-15% battery).

```rust
if message.priority == Priority::P0 {
    battery_manager.set_override(true);
    // Phone will continue relaying until:
    // - SOS is acknowledged (ACK received)
    // - SOS is cancelled (CANCEL_SOS sent by user)
    // - Battery reaches true hardware minimum (2-3%)
}
```

## False Alarm Cancel

A user who accidentally triggers SOS can cancel within 60 seconds:

```rust
pub struct CancelSos {
    pub original_message_id: MessageId,
    pub cancel_timestamp: i64,
    pub reason: CancelReason,  // FalseAlarm | Resolved | Testing
    pub sender_id: NodeId,
    pub signature: Ed25519Signature,
}
```

Cancel propagates with P0 priority (same as SOS) and same epidemic routing.
Any relay that receives the cancel removes the original SOS from its forward queue.

Recipients who already received the SOS see: "SOS CANCELLED — [sender name] — False alarm"

## ACK and Delivery Tracking

When a recipient receives and displays an SOS, they send an ACK:
```rust
pub struct SosAck {
    pub original_message_id: MessageId,
    pub ack_sender: NodeId,
    pub ack_timestamp: i64,
    pub ack_type: AckType,  // Received | Helping | Dispatching | CannotHelp
    pub message: Option<ShortText>,  // Optional response
    pub signature: Ed25519Signature,
}
```

ACK propagates back to the SOS sender via any available path (does not need to retrace
the SOS path). SOS sender receives notification: "Your SOS was received by [contact name]
— 3 minutes ago."

## UI During Active SOS

```
┌─────────────────────────────────────────────┐
│ 🔴 SOS ACTIVE                               │
│ Sent 2 minutes ago                          │
│                                             │
│ Location: 23.2321° N, 69.6707° E (GPS ±8m) │
│                                             │
│ Delivered to:                               │
│ ✓ Priya Sharma — 1 min ago                 │
│ ✓ IRIS Relay Node — 2 min ago              │
│ ⏳ Awaiting more deliveries...              │
│                                             │
│ Battery: 47% (override mode OFF)            │
│                                             │
│         [CANCEL SOS — False Alarm]          │
└─────────────────────────────────────────────┘
```

Cancel button remains visible for 60 seconds. After 60 seconds, cancel is still possible
but a confirmation screen asks "Are you sure? Responders may already be en route."

## Post-SOS State

After SOS is sent:
- Battery override remains active until ACK received or cancel
- Periodic SOS retransmit every 5 minutes (with sequence=1, 2, 3...) until ACK
- Location update every 5 minutes if continuous sharing enabled
- Delivery map shown if relay path information is available (future feature)

## Testing

All SOS tests use `MessageType::SOS_DRILL` in test environments. Drill SOS messages:
- Are visually distinct (yellow border, "DRILL" label)
- Logged separately from real SOSes
- Can be cancelled at any time without window restriction
- Do NOT trigger real emergency contacts' notifications (if they have IRIS)

Never test SOS using real priority in production — real SOS triggers real responses.
