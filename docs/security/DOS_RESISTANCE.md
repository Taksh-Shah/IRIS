# DoS Resistance Design

## Overview

IRIS is a decentralized, infrastructure-free mesh network. Unlike centralized systems, there is no
single point where traffic can be filtered or rate-limited globally. DoS resistance must therefore
be implemented at every node, independently, using only local information. This document describes
the attack surface and the layered defenses built into the IRIS protocol.

## Attack Vectors

### 1. Message Flood (Volume Attack)

An attacker with a valid identity generates millions of messages, attempting to saturate relay
storage, CPU, and network bandwidth across the mesh.

**Characteristics:**
- Messages are cryptographically valid (attacker has a real NodeId and private key)
- Messages may target real or fake recipients
- Attack can be distributed across multiple controlled identities
- Especially dangerous for high-priority queues if attacker abuses priority levels

### 2. Storage Exhaustion

Attacker fills relay node storage with junk messages — large payloads, many unique recipients,
messages with maximum TTL. Legitimate messages cannot be stored.

**Characteristics:**
- Attack cost scales with storage capacity (cheap for attacker, expensive for defender)
- Particularly effective for store-carry-forward scenarios where nodes are offline for hours
- Attacker uses P4+ messages (not P0-P2) to avoid detection as emergency abuse

### 3. CPU Exhaustion (Computational DoS)

Attacker crafts messages designed to maximize processing cost:
- Malformed CBOR requiring extensive error recovery parsing
- Messages with invalid signatures (triggering Ed25519 verification on garbage data)
- Messages that pass initial validation but fail deep validation
- Crafted routing headers that trigger expensive path computations

### 4. BLE/Wi-Fi Scan Exhaustion

Attacker floods BLE advertisement packets from spoofed MAC addresses, or floods Wi-Fi
probe/beacon frames, causing victim nodes to spend all scan processing time on fake peers.

**Characteristics:**
- Does not require IRIS identity — radio-layer attack
- Affects device battery via continuous scan processing
- Masks real peers among fake advertisement noise

### 5. Relay Chain Amplification

Attacker sends a single message with max_hops=255 and high replication factor, causing the
message to be copied thousands of times across the network, amplifying a single send into
a flood at distant nodes.

## Defense Layers

### Layer 1: Fast-Path Rejection

Before any cryptographic operation, every inbound message passes through a fast-path validation
gate. Invalid messages are rejected in microseconds without consuming significant CPU.

```
FastPathCheck:
  1. Length check: message_size > MAX_SIZE_FOR_PRIORITY[priority] → DROP
  2. CBOR well-formedness: reject malformed CBOR immediately (no deep parse)
  3. Version check: unknown protocol version → DROP
  4. Sender ID format: invalid NodeId format → DROP
  5. TTL range: TTL > MAX_TTL[priority] → DROP
  6. Hop count: hop_count > max_hops → DROP (already exceeded)
  7. Priority range: priority > 7 → DROP
  8. Timestamp rough check: timestamp > now + 30 minutes → DROP (clock attack)
```

Performance target: <50 microseconds per message.

### Layer 2: Message Size Limits (Pre-Signature)

Message size limits are enforced BEFORE signature verification. This prevents the "large message
DoS" where attacker sends a 10MB message, causing every relay to spend time on Ed25519 verification
before discovering it is oversized.

| Priority | Max Payload | Max Total | Rationale |
|----------|-------------|-----------|-----------|
| P0 SOS   | 500 bytes   | 700 bytes | Fits all transports including LoRa |
| P1 Medical| 2 KB       | 2.5 KB    | Medical data compact |
| P2 Location| 200 bytes | 400 bytes | Binary location fix |
| P3 Emergency text| 4 KB | 4.5 KB | Short emergency message |
| P4 Text  | 8 KB        | 10 KB     | Normal text message |
| P5 Image | 2 MB        | 2.1 MB    | Compressed image |
| P6 Voice | 5 MB        | 5.2 MB    | Voice clip |
| P7 Video | 50 MB       | 51 MB     | Video clip (Wi-Fi/Internet only) |

### Layer 3: Rate Limiting Per Sender (NodeId-Based)

Each relay node tracks message counts per sender NodeId using a sliding window counter.

```toml
[rate_limits]
# Messages per minute per sender, by priority class
p0_rate_limit = 0          # unlimited (cannot DoS emergency)
p1_rate_limit = 0          # unlimited (cannot DoS emergency)
p2_rate_limit = 60         # 1/second (location updates)
p3_rate_limit = 30         # 1 per 2 seconds
p4_rate_limit = 10         # text messages
p5_rate_limit = 6          # images (10-second minimum gap)
p6_rate_limit = 2          # voice clips
p7_rate_limit = 1          # video clips
```

Implementation: token bucket per (sender_id, priority_class) tuple. Bucket capacity = 2×rate.
Replenishes at rate per minute. Stored in in-memory LRU cache (max 10,000 entries).

When bucket empty: DROP message, DO NOT propagate drop notification to attacker.
Silent drop prevents attacker from probing rate limit boundaries precisely.

### Layer 4: Storage Quota Per Sender

Relay storage is partitioned by sender identity. Each sender NodeId may occupy at most
`max_storage_per_sender` bytes in the relay store.

```toml
[storage_quotas]
max_storage_per_sender_bytes = 10_485_760  # 10 MB per sender
max_total_relay_storage_bytes = 1_073_741_824  # 1 GB total
emergency_reserved_bytes = 104_857_600  # 100 MB reserved for P0-P2
```

Quota enforcement: when inserting a new message, check current usage for sender.
If `current_usage + new_message_size > max_storage_per_sender`: evict oldest P4+ messages
from that sender before inserting, if still exceeds quota → DROP.

Emergency messages (P0-P2) use a separate reserved pool not subject to per-sender quota.

### Layer 5: Priority DoS Protection

P0-P2 messages always receive resources regardless of P4-P7 flood. This is implemented at
every layer:

- **Queue priority**: P0 dequeued first, always, even if P4-P7 queue is full
- **Storage priority**: P0-P2 use reserved storage that P4-P7 cannot touch
- **Rate limiting**: P0-P1 have no rate limit
- **CPU priority**: P0 message processing is elevated priority thread

A P4-P7 flood CANNOT prevent P0 delivery. This is the fundamental safety guarantee.

### Layer 6: Cryptographic Rate Limiting (Proof-of-Work, Experimental)

For P4+ messages from previously-unknown senders, relay nodes MAY require a proof-of-work
challenge before accepting messages. This makes mass-spam computationally expensive.

```
PoW challenge: find nonce N such that:
  SHA256(message_id || sender_id || nonce)[0:16] == target_prefix
  
Difficulty: ~100ms CPU on modern smartphone (difficulty adjusts to maintain this)
```

PoW is optional and disabled by default. Known contacts are exempt. Emergency messages (P0-P2)
are always exempt. Experimental feature — not yet in production path.

### Layer 7: Backpressure Propagation

When a node's relay storage exceeds 80% capacity, it sets `storage_pressure=true` in its peer
advertisements. Neighboring nodes respond by:
- Reducing the rate of messages forwarded to the pressured node
- Preferring other relay paths
- Not sending P5-P7 to the pressured node

This creates organic backpressure that prevents storage exhaustion from cascading.

### Layer 8: Network-Level Flood Prevention

#### Hop Count Limits

Every message has a `max_hops` field. Each relay increments `hop_count`. When
`hop_count == max_hops`, the message is not forwarded further.

Default max_hops by priority:
- P0: 255 (unlimited — emergency reaches everywhere)
- P1-P2: 20
- P3: 15
- P4-P7: 10

Flood amplification requires many hops. Limiting hops limits amplification.

#### Deduplication

Every relay maintains a seen-message cache. A message with an already-seen `message_id` is
silently dropped. This prevents flood amplification even if hop limit is bypassed.

Seen-message cache implementation:
- Bloom filter: 1MB, ~1M entries, 1% false positive rate (some legitimate messages dropped,
  acceptable tradeoff for DoS resistance)
- Exact LRU cache behind Bloom filter: last 50,000 message IDs
- Combined: extremely low false positive rate for recent messages

## Defense Summary

| Attack | Primary Defense | Secondary Defense |
|--------|-----------------|-------------------|
| Message flood | Rate limit per sender | Storage quota |
| Storage exhaustion | Storage quota per sender | Priority-reserved storage |
| CPU exhaustion | Fast-path rejection (pre-crypto) | Message size limits |
| BLE scan exhaustion | OS-level scan rate limiting | MAC address filtering |
| Relay amplification | Hop count limits | Deduplication |
| Priority abuse | P0-P2 reserved resources | Rate limits on P4+ |
| Unknown sender spam | PoW (experimental) | Rate limits |

## Implementation Notes

All DoS defenses implemented in `crates/iris-core/src/node/dos_protection.rs`.
Rate limiters use the `governor` crate (Rust) for token bucket implementation.
Storage quotas enforced in `crates/iris-store/src/relay_store.rs`.
Fast-path validation in `crates/iris-protocol/src/validation.rs`.

## Known Limitations

- A Sybil attacker with 1000 identities gets 1000× the rate limit. See SYBIL_RESISTANCE.md.
- Physical radio-layer attacks (jamming) cannot be mitigated in software.
- PoW is expensive for legitimate users on old hardware — use with caution.
