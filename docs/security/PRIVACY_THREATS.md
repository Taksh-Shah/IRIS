# Privacy Threat Analysis

## Overview

IRIS is designed for privacy. End-to-end encryption protects message content. However,
a decentralized mesh network has inherent privacy limitations that differ from centralized
systems: metadata flows through many nodes, radio-layer signals are observable by anyone
with a receiver, and the cooperative nature of routing creates surveillance opportunities.
This document is an honest analysis of the privacy threats in IRIS and what we do (and
cannot do) about them.

## Threat Model

**Adversary types considered:**
- **Passive relay node**: a relay in the mesh that logs all metadata it observes
- **Active relay node**: a relay that modifies or injects messages to probe identities
- **Passive radio observer**: an adversary with radio equipment observing BLE/Wi-Fi signals
- **Aggregating adversary**: multiple colluding nodes with shared logs
- **State-level adversary**: access to ISP-level data when Internet is used, legal compulsion

**Adversary capabilities:**
- Can observe radio signals (BLE advertisements, Wi-Fi probe requests, LoRa)
- Can run relay nodes in the mesh
- Can correlate multiple observations over time
- Cannot break Ed25519, X25519, or ChaCha20-Poly1305 with current computing
- Cannot compel device to reveal private key (assumed)

## Threat 1: Traffic Analysis

### Description

Even without decrypting messages, a passive relay node observes:
- WHO communicates with WHOM (sender_id, recipient_id are in routing headers)
- WHEN (timestamp)
- HOW MUCH (payload_size)
- HOW URGENT (priority)

Over time, these observations reveal communication patterns, social relationships, and
behavioral patterns — even without any message content.

### Mitigation (Partial)

- **Encryption of payload**: content protected by E2EE. Not addressed here.
- **No content leak**: priority and size reveal content type (P0 = SOS, P5 = image) but not
  content itself
- **Future cover traffic**: inject dummy traffic to obscure real traffic patterns (planned,
  not implemented). Cover traffic has battery cost.
- **Future message padding**: pad all messages to fixed sizes within priority class (planned).
  Hides exact payload size.
- **Tor integration (optional)**: route messages through Tor when Internet is available,
  hiding IP address. High latency. Not suitable for emergency use.

### Residual Risk

Metadata analysis is a fundamental limitation of any routing protocol. We cannot
both route messages AND keep routing metadata private. This is a known, accepted tradeoff.
IRIS provides payload confidentiality, not traffic anonymity.

## Threat 2: BLE Tracking

### Description

BLE advertisements are visible to anyone with a Bluetooth scanner within 50-100m range.
If a device advertises a stable, fixed identifier, a passive observer with multiple scanners
can track device movements over time: "NodeId 0x1234ABCD was at bus stop at 9:05 AM, mall
at 2:15 PM, home at 7:30 PM."

### Mitigation (Implemented)

**Rotating advertisement IDs**: IRIS rotates the BLE advertisement identifier every 15 minutes.
The rotation uses a pseudorandom sequence derived from a device-specific secret:

```
advertisement_id(t) = HMAC-SHA256(device_secret, floor(t / 900))[0:8]
  where 900 = 15 minutes in seconds
```

Known contacts can derive the current advertisement_id from the shared secret (established
at contact add time). Unknown observers see different IDs every 15 minutes and cannot link
them to the same device.

**Limitation**: a scanner that continuously monitors a specific location for 15+ minutes
can still associate two consecutive IDs to the same device by observing the transition.
This requires active, sustained monitoring — significantly harder than passive logging.

**Wi-Fi MAC randomization**: IRIS uses OS-provided Wi-Fi MAC address randomization. On
Android 10+ and iOS 14+, this is available. On older OS, MAC is fixed — IRIS warns user.

## Threat 3: Metadata Leakage to Relay Nodes

### Description

Every relay node that carries a message sees the full routing header:
- `sender_id`: who sent it (pseudonymous — public key hash, not name)
- `recipient_id`: who it is for
- `timestamp`: when it was sent
- `priority`: urgency level
- `payload_size`: size of content
- `payload_type`: type of content (text, image, voice, SOS, etc.)

A relay node that observes many messages can build a detailed picture of a user's
communication patterns without accessing any message content.

### Mitigation (Partial)

- **Relay all messages**: IRIS nodes relay messages for all users, not just their contacts.
  A relay node cannot tell the difference between a message it is "meant" to see and one
  it is just carrying. This reduces the inference value of any single observation.
- **Pseudonymous IDs**: `sender_id` is a public key hash, not a name. Linking to a real
  identity requires knowing the public key → person mapping.
- **Group recipient ID**: for group messages, recipient_id is the group's hash, not individual
  members. Relay nodes learn "someone sent to group G" not "Alice sent to Bob."
- **Broadcast recipient**: emergency messages use a broadcast recipient_id that reveals nothing
  about the sender's social graph.

### Future Work

- **Onion routing**: encrypt routing headers for each relay separately (like Tor) so each
  relay sees only its own hop, not the full path. Significant overhead.
- **Blind routing**: route using encrypted identifiers that only the destination can decrypt.

## Threat 4: BLE Presence Information

### Description

When discovery mode is active, a device advertises its IRIS presence continuously. This
reveals that the user has IRIS installed and is currently active at this location.

### Mitigation

- **Discovery mode toggle**: users can disable BLE discovery mode when they do not need it.
  In passive mode, the device scans for others but does not advertise.
- **Default**: discovery mode is ON by default (needed for functionality).
- **User education**: UI explains that BLE discovery reveals presence in area.

## Threat 5: Contact Graph Inference

### Description

By observing which relay nodes carry messages between which sender/recipient pairs over time,
an aggregating adversary can infer the social graph of IRIS users — who is connected to whom.

### Mitigation

- **Relay for all**: as above, relay nodes carry messages for all users, not just contacts.
  This adds noise to contact graph inference.
- **No explicit contact list sharing**: the user's full contact list is stored locally and
  never broadcast. Only the identifiers needed for routing appear in messages.
- **Anti-harvesting rate limits**: rate-limit responses to contact discovery to prevent
  systematic scraping.

## Threat 6: Location Privacy

### Description

Although IRIS does not include GPS coordinates in routing metadata (unlike some protocols),
location can still be inferred:
- Which relay nodes have seen a user's messages indicates their geographic area
- BLE/Wi-Fi contacts reveal proximity to specific devices
- Location explicitly shared in messages (P2 priority) reveals exact GPS if captured

### Mitigation

- **Location in payload only**: GPS coordinates appear only in the encrypted payload (P2
  messages), never in routing headers. Relay nodes cannot read payload location.
- **Explicit consent for location sharing**: P2 messages require explicit user action.
- **Location accuracy options**: user can share approximate location (±1km) instead of
  precise (±5m).
- **Location expiry**: P2 location messages have short TTL (2 hours).

## Fundamental Limitation: Privacy vs. Anonymity

IRIS provides **privacy** (content confidentiality, metadata minimization) but NOT
**anonymity** (inability to link messages to a person).

IRIS is intentionally non-anonymous:
- Every message is signed with a persistent identity (Ed25519 key)
- Sender identity is visible to all relays
- Persistent identity enables accountability for emergency system abuse

If you need anonymity, IRIS is not the right tool. Strong anonymity requires Tor-like
infrastructure (onion routing, cover traffic, mix networks) with significant latency overhead
that is incompatible with emergency communication requirements (P0 < 100ms hop target).

We document this limitation clearly and honestly. Users who need anonymity should use Tor
or similar tools for their non-emergency, non-safety communication needs.
