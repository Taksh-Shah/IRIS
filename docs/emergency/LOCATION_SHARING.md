# Location Sharing Design

## Privacy Principle

Location is the most sensitive data a person can share. Real-time location reveals where
you live, where you work, who you meet, where you worship, and where your children are.
In a domestic violence context, a survivor's location can be a matter of life and death.

IRIS design rule: **location is never shared without explicit, in-the-moment user action.**
No automatic location attachment. No background location upload. No "check-in" that 
reveals location without the user knowing and choosing.

## Location Sharing Modes

### Mode 1: One-Time Share to Specific Contact

The user explicitly sends their current location to one or more specific contacts.

```
User action: "Share Location" button in conversation
Selection: current location (auto-fetched from GPS) OR map pin (user selects point)
Target: one contact, multiple contacts, or a group
Duration: one-time (single message with snapshot location)
```

Message type: P2 (Location), encrypted to recipient(s) only.
Recipient sees location on a map within the conversation.
Location is NOT shared with relay nodes (encrypted payload).

### Mode 2: Continuous Sharing During Active SOS

When SOS is active, the user can enable continuous location sharing to their emergency
contact list. Location updates sent every 5 minutes.

```
UI: "Share your location with emergency contacts while SOS is active? [Yes] [No]"
Default: Yes (user can change default in settings)
Duration: while SOS is active (automatic stop when SOS is cancelled or resolved)
Update interval: every 5 minutes
Target: emergency contact list (defined in settings)
```

Each update is a separate P2 message. Contacts see updated location in real time.
Location stops being shared automatically when SOS is cancelled or 30 minutes after
last SOS retransmit.

### Mode 3: Area Code Share (Coarser, Public Channel)

For public community channels, users can share approximate location (district/city level)
without revealing precise GPS coordinates.

```
Area code format: ISO 3166-2 district code (e.g., "IN-GJ-19" for Kutch)
Resolution: district level (thousands of km²)
Encoding: 10-character string
Use case: "I'm in Kutch, anyone nearby?" without revealing neighborhood
```

Area code sharing does not use GPS — user selects from a dropdown of known area codes.

## Location Data Format

### P2 Location Message (Binary, Compact)

Designed for LoRa compatibility (fits in single LoRa packet alongside other fields):

```rust
#[repr(C, packed)]
pub struct CompactLocation {
    // Latitude: -90.0000 to +90.0000 degrees
    // Stored as i32 = degrees × 10000
    // Accuracy: ±0.0001 degrees ≈ ±11 meters at equator
    pub lat_fixed: i32,      // 4 bytes, little-endian
    
    // Longitude: -180.0000 to +180.0000 degrees  
    // Stored as i32 = degrees × 10000
    pub lon_fixed: i32,      // 4 bytes, little-endian
    
    // Altitude: -1000m to +32767m (0 if unavailable)
    pub altitude_m: i16,     // 2 bytes, signed
    
    // Horizontal accuracy in meters (0 = unknown)
    pub accuracy_m: u16,     // 2 bytes, unsigned, max 65535m
    
    // GPS fix time (seconds since Unix epoch)
    pub fix_timestamp: u32,  // 4 bytes (good until 2106)
    
    // Flags byte
    pub flags: LocationFlags, // 1 byte
}
// Total: 17 bytes

pub struct LocationFlags {
    pub source: LocationSource,  // 2 bits: GPS(0), Network(1), Passive(2), Manual(3)
    pub has_altitude: bool,      // 1 bit
    pub is_stale: bool,          // 1 bit (set if >15 minutes old at time of sending)
    // 4 bits reserved
}
```

Binary encoding total: 17 bytes. Combined with SOS envelope: ~50 bytes for location data.
Fits comfortably in a LoRa SOS packet (250 byte limit).

### GPS Coordinate Encoding Examples

```
Location: 23.2321° N, 69.6707° E (Bhuj, Gujarat)
  lat_fixed = 232321  (23.2321 × 10000)
  lon_fixed = 696707  (69.6707 × 10000)
  
Accuracy: ±4 decimal places = ±0.0001° 
  At 23°N: 1° latitude ≈ 111km → 0.0001° ≈ 11.1m
  Acceptable for emergency response purposes.
  
For higher precision (future): 5 decimal places (±1.1m) using i32 × 100000
  would require value range check (lat: ±9000000, lon: ±18000000, within i32 range ✓)
```

## Location Sources and Accuracy

| Source | Typical Accuracy | IRIS Use | Battery Cost |
|--------|-----------------|----------|-------------|
| GPS (GNSS) | ±3-10m | Primary for SOS | High (~100mW) |
| Network (cell/Wi-Fi) | ±50-300m | Fallback if GPS unavailable | Low (~5mW) |
| Passive | ±500-2000m | Last resort | Minimal |
| Manual (user input) | User-defined | When GPS unavailable | None |

GPS acquisition time:
- Cold start (no recent fix): 30-60 seconds
- Warm start (recent fix <1h): 5-15 seconds
- Hot start (continuous GPS): <1 second

**IRIS SOS GPS behavior:**
1. If GPS is warm or hot: include location immediately
2. If GPS is cold: show "Acquiring GPS..." in SOS UI, send SOS without location, then send
   P2 location update message when GPS fix acquired (within 60 seconds)
3. If GPS not available after 60 seconds: note "Location not available" clearly in SOS message.
   Do NOT send inaccurate location. A missing location is less dangerous than a wrong one.

## Location Expiry

Stale location is dangerous. A location 2 hours old is useless for emergency response
(the person has moved). IRIS enforces location expiry:

```
P2 Location message TTL by type:
  SOS continuous update: 30 minutes (after 30 minutes, next update is expected)
  One-time share: 2 hours
  Area code share: 12 hours (area code is less sensitive to staleness)
```

Recipients see location age: "Location from 12 minutes ago." After TTL expiry,
the UI shows "Location expired — contact for update."

## Privacy in Routing

Location data appears ONLY in the encrypted payload of P2 messages.
It is NEVER included in routing headers, even as a hint.

```
MessageEnvelope.header:
  sender_id:    yes (needed for routing)
  recipient_id: yes (needed for delivery)
  location:     NO — not in header, never, by design

MessageEnvelope.encrypted_payload:
  CompactLocation: yes — encrypted with recipient's public key
```

Relay nodes process the routing header. They never see location data. Only the intended
recipient can decrypt the payload and read the location.

## User Consent Architecture

Location sharing consent is managed at three levels:

**Level 1: App permission (OS)**: IRIS requests location permission from the OS.
- Android: `ACCESS_FINE_LOCATION` and `ACCESS_BACKGROUND_LOCATION` (for SOS)
- iOS: "When In Use" and "Always" (for SOS active state)

**Level 2: Default preference (settings)**: user sets default for "include location in SOS"
and "share location with emergency contacts during SOS."

**Level 3: Per-share consent (in-moment)**: each location share (one-time, or starting
continuous SOS sharing) asks the user in the moment, even if defaults are set.

Consent level 3 overrides levels 1 and 2. We never share location silently.

## Location and Safety

Special consideration for domestic violence and stalking scenarios:

- **Contact-only sharing**: location can only be shared with explicit IRIS contacts —
  not with strangers who message you
- **No location in group messages**: one-time location shares go to specific recipients,
  not group channels (where membership may be broader than trusted contacts)
- **Easy revoke**: location sharing can be stopped at any point from the SOS cancel screen
- **Disable always**: a setting allows permanently disabling all location features if the
  user's safety requires that GPS never be used by IRIS
