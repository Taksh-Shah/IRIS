# Privacy Model

## Overview

Privacy in IRIS is not a feature — it is a design constraint. In disaster and emergency
communication, users must be able to communicate without fear that their movements,
social graph, or message content will be surveilled. This is especially important in
India where disaster victims may include marginalized communities, political activists,
or people in conflict zones.

IRIS minimizes data collection at every layer. What cannot be collected cannot be leaked.

## What Is Private

### Message Content: Fully Private

All IRIS message content is end-to-end encrypted. No node in the relay chain —
including IRIS relay servers — can read the content.

```rust
pub struct IrisMessageEnvelope {
    // Everything below is visible to relays (routing metadata)
    pub message_id: MessageId,         // opaque 32-byte random ID
    pub sender_id: NodeId,             // public key hash — no real identity
    pub recipient_id: NodeId,          // or broadcast group ID
    pub priority: u8,                  // P0–P7
    pub ttl_secs: u32,
    pub hop_count: u8,
    pub created_at: u32,               // Unix timestamp (seconds)
    pub transport_hints: Vec<TransportHint>, // optional, can be omitted

    // Everything below is encrypted — relay cannot read it
    pub encrypted_payload: EncryptedPayload,
    pub sender_signature: Signature,   // signs the envelope including encrypted payload
}
```

**What relay sees**: who is messaging whom (NodeId level, not real identity),
when, and approximately how big the message is.

**What relay does NOT see**: message text, location data within payload, attached files,
contact lists, message type details.

### Contact List: Fully Private

IRIS contact lists are stored locally on the device. They are never transmitted
to any server or relay. Contact information is not included in any network message.

No cloud sync of contacts. No "people you may know" features. No contact matching
with server-side databases.

### User's Real Identity: Fully Private

IRIS NodeIds are public key hashes — they carry no real-world identity information.
A NodeId like `A3B7F2C9...` does not reveal name, phone number, location, or any
other real-world identifier unless the user chooses to disclose this as their
display name.

## What Is Semi-Private

### Node Presence

When a device broadcasts via BLE or Wi-Fi, nearby devices can detect that an IRIS
node is present. This is necessary for mesh operation.

**What is visible**: an IRIS node is within BLE range (~50–100m).

**What is NOT visible** without joining the IRIS network: the NodeId of that node
(rotated periodically, see below), the user behind it, any messages.

Mitigations:
- Rotating BLE advertisement IDs (every 15 minutes by default)
- Wi-Fi MAC address randomization (handled by OS on modern Android/iOS)
- IRIS does not broadcast NodeId in clear in advertisement packets (only encrypted
  service UUID used for IRIS recognition)

### Message Existence

Relay nodes know that a message exists and is being forwarded. They see:
- Message ID (opaque 32-byte value)
- Sender NodeId
- Recipient NodeId (or group ID)
- Priority class
- Approximate size
- Timestamp

Relay does NOT know:
- Real identity behind NodeId
- What kind of message (text, location, medical)
- Content

### Message Timing Patterns

A passive observer monitoring network traffic (even without breaking E2EE) can
observe message flow patterns: when messages are sent, between which NodeIds,
frequency. This is traffic analysis. IRIS does not fully mitigate traffic analysis
(a future version may implement cover traffic or onion routing).

## What Is Public

### Emergency Broadcasts (Intentionally Public)

P0 SOS broadcasts are transmitted to all nodes in range. They are not encrypted
(or encrypted with group key known to all IRIS nodes). This is intentional:
emergency broadcasts need to reach any node, including unknown ones.

```rust
pub fn create_sos_broadcast(
    sender: &IrisIdentity,
    location: Option<GpsCoordinate>,
    message: &str,
) -> IrisMessage {
    IrisMessage {
        priority: Priority::P0,
        // SOS uses group broadcast, not individual recipient
        recipient_id: NodeId::BROADCAST_ALL,
        // Payload is NOT encrypted — any relay can read to assist routing/display
        payload: SosPayload {
            location,
            message: message.to_string(),
            sender_display_name: sender.display_name.clone(),
            contact_info: None,  // user chooses whether to include
        }.encode(),
        encrypted: false,
        ..
    }
}
```

Users are warned before sending P0: "Your SOS will be readable by any IRIS node
that receives it. Continue?"

### Public Community Channels

Community channels (public group messages) are encrypted with a group key distributed
to all members, but by design the group is open. Anyone who joins the group can read
all messages.

## Metadata Minimization

### Envelope Design

The IRIS message envelope carries minimum metadata:

```rust
/// Every field that is NOT encrypted must be justified:
///
/// message_id: needed for deduplication — use opaque random value
/// sender_id: needed for routing — use NodeId (public key hash, not real identity)
/// recipient_id: needed for routing — use NodeId or group ID
/// priority: needed for relay decisions — reveal P0-P7 class only
/// ttl_secs: needed for message lifecycle — reveals urgency indirectly
/// hop_count: needed for loop prevention
/// created_at: needed for TTL calculation and ordering — reveals send time
pub struct MinimalEnvelope { /* see above */ }
```

Optional fields that may be omitted for privacy:
- `transport_hints`: which transports to prefer (leaks transport capability)
- Removed: geohash routing hints (would reveal approximate sender location)
- Removed: sender's display name from envelope (moved to encrypted payload)

### Routing Hint Anonymization

When routing hints are included (optional), they are coarsened:

```rust
pub struct AnonymizedRoutingHint {
    /// Geohash precision: India divided into ~10km² cells
    /// Reveals "sender is somewhere in grid cell X" not exact location
    pub destination_geohash: Option<GeohashPrecision5>, // ~5km resolution
    pub preferred_transports: Vec<TransportClass>,  // not specific transport IDs
}
```

Precise GPS coordinates are NEVER in the envelope. If location sharing is requested
(P2 medical, or user opts in), location goes in the encrypted payload only.

## BLE Privacy

### Rotating Advertisement IDs

BLE advertisements contain a device address. Without randomization, any BLE scanner
(including passive third-party scanners) can track IRIS users by their persistent
BLE address.

Mitigations:

**OS-level MAC randomization**: Android 10+ and iOS 8+ randomize MAC addresses per
network. IRIS relies on this and does not need to implement it separately.

**IRIS advertisement ID rotation**: IRIS changes its IRIS-specific device ID
(included in the advertisement service data) every 15 minutes:

```rust
pub struct AdvertisementId {
    // Not the BLE MAC — a separate rotating value in GATT service data
    value: [u8; 8],
    created_at: Instant,
    rotation_interval: Duration,  // default: 15 minutes
}

impl AdvertisementId {
    pub fn current(identity: &IrisIdentity) -> Self {
        // Derive time-based rotating ID using HMAC
        // Only nodes with our public key can verify rotated IDs belong to us
        let slot = SystemTime::now().unix_timestamp() / 900; // 15-minute slots
        let hmac_input = [identity.node_id.as_bytes(), &slot.to_le_bytes()].concat();
        let rotated = hmac_sha256(&identity.private_key_bytes(), &hmac_input);
        AdvertisementId { value: rotated[..8].try_into().unwrap(), .. }
    }
}
```

Known contacts can verify that a rotated advertisement belongs to a known NodeId
(via HMAC). Unknown observers cannot link rotated IDs to each other or to the NodeId.

## Location Privacy

Location information in IRIS messages:

| Scenario | Location Handling |
|----------|------------------|
| P0 SOS (user sends) | User chooses to include location; appears in plaintext payload |
| P2 Medical (user sends) | Optional; encrypted in payload, not in envelope |
| P3–P7 messages | Location never included unless user explicitly adds |
| Routing hints | Coarsened geohash, optional, user-controlled |
| Contact graph | Never transmitted; local only |

**IRIS never automatically includes GPS location in messages.**

GPS access is never requested in the background. Location permission is requested
only when the user taps "Include my location" in message composer.

## Contact Graph Privacy

The social graph (who contacts whom) is sensitive:
- Reveals organizational structures
- Can identify leadership networks
- May expose family relationships of disaster victims

IRIS contact graph is stored locally only:
- No cloud sync
- No "people you may know"
- No relay can observe the full contact graph (they only see individual message flows)
- Contact metadata (notes, labels) never transmitted

## India DPDPA Implications

India's Digital Personal Data Protection Act (2023):

| DPDPA Requirement | IRIS Implementation |
|------------------|---------------------|
| Data minimization | Envelope contains minimum metadata, no PII by default |
| Purpose limitation | Data used only for routing; no analytics |
| Storage limitation | Messages auto-expire (TTL), contact data local only |
| User consent for processing | User explicitly creates identity; no implicit consent |
| Right to erasure | User can delete identity and all local data |
| Cross-border transfer | E2EE: relay servers process only encrypted data |

IRIS relay servers (if any) process only encrypted data. They function as a data
processor under DPDPA: they transmit opaque ciphertext on behalf of the data
principal (the user). No personal data is stored on relay servers beyond transient
routing state (message ID + routing decision, auto-deleted after TTL expiry).

## Known Limitations

**Traffic analysis**: A relay node or passive observer with access to multiple
relay points can correlate message timing and flow to infer who is communicating
with whom, even without breaking E2EE. IRIS does not mitigate this in the current
design. Future versions may implement:
- Cover traffic (send dummy messages at constant rate)
- Onion routing (multiple relay hops, each seeing only adjacent hops)
- Mix networks (batch and reorder messages before forwarding)

**Timing correlation**: Even with rotating BLE IDs, an observer who can monitor
multiple BLE scanners can correlate rotation events and potentially re-identify nodes.
Mitigation: vary rotation interval ±5 minutes randomly.

**Relay metadata**: relay servers see sender and recipient NodeIds. If NodeIds are
ever correlated with real identities (e.g., by another party the user communicated
with), the relay operator could reconstruct communication patterns.

**Device compromise**: if a device is physically seized, all stored messages and
the identity private key are at risk unless the user has enabled IRIS lockdown mode
(requires PIN/biometric for any IRIS access, additional encryption layer on stored
messages).
