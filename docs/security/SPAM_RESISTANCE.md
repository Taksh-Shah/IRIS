# Spam Resistance in Mesh Network

## Overview

IRIS operates without a central authority. There is no email provider to block a spam domain,
no phone carrier to block a SIM, no server to denylist an IP address. Spam resistance must be
achieved through decentralized mechanisms that operate correctly even when most or all
infrastructure is offline. This document describes the spam threat model and IRIS defenses.

## Spam Threat Model

### What Makes IRIS Spam Different

Traditional spam (email, SMS) is fought by centralized filters. IRIS has no central filter.
An attacker with a valid IRIS identity can:

1. Generate messages at will (no per-message cost without PoW)
2. Relay messages through any willing node
3. Create new identities freely (no centralized registration)
4. Operate offline, beyond the reach of any blocking system

### Spam Impact

- **Storage exhaustion**: spam fills relay stores, blocking legitimate messages
- **Battery drain**: receiving and processing spam wastes battery on victim devices
- **User experience**: spam in public channels degrades community communication
- **Emergency degradation**: P4+ spam can interfere with P4 legitimate messages
  (note: P0-P2 are protected regardless — see DOS_RESISTANCE.md)

## Defense Mechanisms

### 1. Rate Limiting Per Identity

The primary spam defense. Each relay node enforces per-sender rate limits (see DOS_RESISTANCE.md
for full rate table). For P4 text messages: 10 messages/minute per sender.

A spammer sending 10,000 messages/minute from one identity will have 99.9% dropped at the
first relay that applies rate limiting. Remaining 10 messages/minute are delivered normally.

**Multi-identity attack**: a spammer with 100 identities gets 100×10 = 1000 messages/minute.
This is addressed by Sybil resistance (SYBIL_RESISTANCE.md) and social graph filtering below.

### 2. Storage Quotas Per Identity

Each sender identity may use at most 10MB of relay storage across all nodes. A spammer cannot
fill relay stores because their messages are evicted once the per-sender quota is reached.

### 3. Social Graph Filtering

IRIS applies trust-based filtering when storage is constrained:

```
Trust tiers (highest to lowest):
  Tier 1: Direct contacts (in user's contact list)
  Tier 2: Contacts-of-contacts (known to your contacts)
  Tier 3: Verified community members (member of shared group)
  Tier 4: Unknown senders

Priority for relay storage (when >80% full):
  Tier 1 messages: always stored
  Tier 2 messages: stored up to 80% of remaining space
  Tier 3 messages: stored up to 50% of remaining space
  Tier 4 messages: stored up to 20% of remaining space, P0-P2 only
```

An unknown spammer's messages are deprioritized when storage is constrained.
Your direct contacts' messages are always delivered.

### 4. Local Block List

Users can block specific sender NodeIds. Blocked messages are silently dropped at the receiving
node. The block list is:
- Stored locally on device (not shared without user consent)
- Optionally shareable with trusted contacts (social block propagation)
- Applied before any other processing (fast drop)

Block list sharing: a user can share their block list with up to 50 trusted contacts.
Recipients can choose to apply shared block lists. This enables community-level spam
suppression without central authority.

### 5. Proof-of-Work for Unknown Senders (Experimental)

For P4+ messages from Tier 4 (unknown) senders, relay nodes can optionally require a
proof-of-work (PoW) token embedded in the message.

```
PoW parameters:
  Algorithm: SHA256-based partial preimage
  Target difficulty: ~100ms CPU on a mid-range 2022 smartphone
  Scope: per-message (not per-session — must be recomputed for each message)
  Exemptions: P0-P2 always exempt, Tier 1-3 senders exempt
```

Impact on legitimate users: sending one message to an unknown contact costs 100ms extra.
Impact on spammer: sending 10,000 messages costs 1,000,000ms = 277 hours. Economically
infeasible.

Disabled by default — not yet validated for production. Enable in node config:
```toml
[spam_resistance]
proof_of_work_enabled = false
proof_of_work_difficulty = 16  # bits of leading zeros required
```

### 6. Community Channel Moderation

Public community channels have designated admins. Admin capabilities (offline):
- Send signed REMOVE_MESSAGE command (message_id, admin_cert, reason)
- Every relay receiving the command removes the specified message from public channel store
- MUTE_SENDER command: relay nodes stop forwarding from that sender to this channel
- Commands propagate as signed P3 messages (emergency text priority)

Moderation is eventually consistent — an admin command reaches all nodes within minutes
in a connected mesh. During complete network partition, moderation is delayed.

### 7. Community Reporting System

Any user can flag a message as spam:
```
SPAM_REPORT {
  message_id: "abc123",
  reporter_id: sender_id,
  channel_id: channel_id,
  timestamp: now,
  signature: reporter_signature
}
```

Spam reports propagate as low-priority (P7) messages. Relay nodes aggregate reports.
Threshold: if >10% of channel members report a message within 1 hour → auto-remove
from channel store (requires signature verification of each report).

Single reporter cannot spam-flag legitimate messages into oblivion — threshold required.

### 8. Anti-Harvesting Controls

Spam requires knowing target addresses. IRIS prevents address harvesting:

- **Rate-limited discovery**: node discovery responses are rate-limited to 10 responses/minute
  to any single requester
- **Contact list opacity**: user's full contact list is never transmitted; only the hashes
  needed for routing are included in messages
- **No address directory**: IRIS has no global address directory that can be scraped
- **Ephemeral advertisement**: BLE advertisements rotate NodeId every 15 minutes, preventing
  passive collection of stable addresses

### 9. Emergency Exception (Cannot Block Emergency Messages)

P0 SOS and P1 Medical messages bypass all spam filters, block lists, and rate limits.
This is a fundamental design constraint: the system must never prevent a genuine
emergency message from reaching potential helpers.

Consequence: an attacker can always send ONE SOS to reach a user even if blocked.
Defense: SOS rate limiting (max 3 per hour per identity) in EMERGENCY_ABUSE.md.

## Anti-Spam Configuration Reference

```toml
[spam_resistance]
# Rate limits (msgs/min, 0 = unlimited)
p4_rate_per_sender = 10
p5_rate_per_sender = 6
p6_rate_per_sender = 2
p7_rate_per_sender = 1

# Storage quotas
max_storage_per_sender_mb = 10
storage_pressure_threshold = 0.80  # fraction, triggers social graph filtering

# Social graph filtering
enable_social_graph_filtering = true
contacts_of_contacts_max_fraction = 0.80
unknown_sender_max_fraction = 0.20

# Proof of work (experimental)
proof_of_work_enabled = false
proof_of_work_difficulty_bits = 16

# Block list
max_local_blocklist_size = 10000
max_shared_blocklist_contacts = 50
enable_shared_blocklist = false  # user opt-in required

# Community moderation
community_spam_report_threshold_fraction = 0.10
community_spam_report_window_minutes = 60
```

## Limitations

- **Offline moderation lag**: moderation commands take time to propagate; spam reaches
  recipients before moderation can act.
- **Sybil limits**: per-identity rate limits are defeated by enough fake identities.
  See SYBIL_RESISTANCE.md.
- **PoW hardware inequality**: PoW penalizes old/slow devices more than attackers with
  compute resources.
- **No content inspection**: all filtering is identity/metadata-based. IRIS does not
  inspect message content (by design — E2EE).

## Interaction with Other Systems

- DoS resistance: see DOS_RESISTANCE.md (overlapping mechanisms, different threat models)
- Sybil resistance: see SYBIL_RESISTANCE.md (spam via multiple identities)
- Emergency abuse: see EMERGENCY_ABUSE.md (spam of P0-P2 channels)
- Privacy: all spam signals are metadata-only; no content inspection ever
