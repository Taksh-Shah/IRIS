# Replay Attack Protection

## Overview

A replay attack occurs when an adversary captures a previously transmitted, cryptographically
valid message and re-transmits it later. The message is genuine (valid signature, valid content)
but its retransmission is malicious. In IRIS, replay attacks could cause:
- Duplicate SOS delivery (confusing responders)
- Duplicate delivery of any message
- Confusion about message order or timing
- Exhaustion of storage by replaying old expired messages

This document describes the three-layer defense against replay attacks.

## Why Replay Is Possible in IRIS

Unlike a session-based protocol (TLS, Signal), IRIS is a store-carry-forward network.
Messages are stored at intermediate relay nodes and forwarded later — sometimes hours later.
This is intentional and necessary for disruption-tolerant operation. But it means a relay
that holds a legitimate message IS doing something that looks like a replay: forwarding a
message received in the past.

The challenge: distinguish legitimate store-carry-forward from malicious replay.

## Defense Layer 1: Message ID Deduplication

### Mechanism

Every IRIS node maintains a **seen-message cache** indexed by `message_id`. When a message
arrives, the node first checks: "Have I seen this message_id before?"

If YES → silent drop (do not forward, do not process, do not notify sender).
If NO → add to cache, process message.

### Implementation

Two-tier cache for efficiency and correctness:

**Tier 1: Bloom Filter (approximate, fast)**
```
BloomFilter {
  size: 1MB (8,388,608 bits),
  hash_functions: 7,
  estimated_capacity: 1,000,000 message IDs,
  false_positive_rate: ~1% at capacity
}
```

Query time: O(k) hash operations, ~1 microsecond.
False positive: 1% of unseen messages treated as duplicates (acceptable loss).
False negative: 0% (Bloom filter NEVER says "not seen" for a seen message).

**Tier 2: Exact LRU Cache (precise, for recent messages)**
```
LruCache<MessageId, SeenRecord> {
  capacity: 50,000 entries,
  entry: SeenRecord { first_seen: Instant, priority: u8 },
  eviction: least-recently-inserted (not LRU by access — we don't re-access seen IDs)
}
```

Query: first check Bloom filter. If MAYBE SEEN, check exact LRU.
True duplicate → in both or just Bloom (false positive).
Not seen → in neither.

### Cache Lifetime

Messages are deduplicated for as long as they are within their TTL window. After TTL expiry,
the same message_id could theoretically be reused in a new message — but message_ids are
UUID v4 (128-bit random). Collision probability: negligible (birthday paradox with 2^64 IDs
would require 2^32 ≈ 4 billion messages with 50% collision chance).

**Bloom filter reset**: Bloom filter is reset every 24 hours. Exact LRU serves as ground
truth during transition. After reset, the Bloom filter rebuild from LRU content.

### Cache Persistence

**Critical**: the seen-message cache MUST survive application restart. Without persistence,
a node restart allows replay of any message it previously received.

Implementation: Bloom filter bit array and LRU entries serialized to SQLite on a background
thread after each update (batched, max 100ms write delay). Loaded on startup before first
message is processed.

Graceful degradation: if persistence fails (storage full, corruption), the node starts with
an empty cache and logs a warning. A fresh node is vulnerable to replay of historical messages
until its cache is rebuilt from the current message stream.

## Defense Layer 2: Timestamp Validation

### Mechanism

Every IRIS message has a signed `timestamp` field (Unix seconds). Relays validate:

```
now = current_unix_timestamp()

// Reject future-dated messages (attacker setting far-future timestamp)
if message.timestamp > now + FUTURE_TOLERANCE:
  DROP  // FUTURE_TOLERANCE = 1800 seconds (30 minutes clock skew allowance)

// Reject messages older than their maximum allowed lifetime
max_age = MAX_TTL[message.priority]
if now > message.timestamp + max_age:
  DROP  // Message has expired

// Also check the declared TTL
if now > message.timestamp + message.ttl_seconds:
  DROP  // Message expired per its own TTL
```

Maximum TTL by priority:
| Priority | Max TTL | Rationale |
|----------|---------|-----------|
| P0 SOS   | 7 days  | Emergency reaches even isolated nodes |
| P1 Medical| 2 days | Medical data may need delayed delivery |
| P2 Location| 2 hours| Stale location is dangerous |
| P3 Emergency text| 24 hours | |
| P4 Text  | 7 days  | Normal messages |
| P5 Image | 3 days  | Large messages expire sooner |
| P6 Voice | 1 day   | |
| P7 Video | 12 hours| Large + expires quickly |

### Store-Carry-Forward vs Replay

A legitimately stored message (stored for 6 hours, then delivered) is NOT a replay:
- Its `timestamp` is 6 hours ago
- Its TTL is 7 days (for P4)
- 6 hours < 7 days → still valid → forward

A replayed message from 8 days ago:
- Its `timestamp` is 8 days ago
- Max TTL for P4 = 7 days
- 8 days > 7 days → expired → DROP

The TTL window exactly distinguishes legitimate delayed delivery from out-of-window replay.

### Clock Synchronization

IRIS cannot assume synchronized clocks (disaster scenario, no NTP). The 30-minute future
tolerance is intentionally generous. Nodes with significantly drifted clocks (>1 hour)
may have some messages incorrectly rejected. This is acceptable — severe clock drift
also causes other problems for the user.

When Internet is available, IRIS syncs time via SNTP. GPS also provides highly accurate
time if available (used in emergency scenarios for critical timestamp reliability).

## Defense Layer 3: Encryption Nonce Uniqueness

### Mechanism

All P0-P6 messages encrypt their payload using ChaCha20-Poly1305 with a 12-byte random nonce.

```
nonce: [u8; 12] = random_bytes()  // Generated fresh for each message
ciphertext = ChaCha20Poly1305.encrypt(nonce, key, plaintext)
message.encrypted_payload = nonce || ciphertext
```

A replayed encrypted message:
1. Same ciphertext (attacker has no plaintext access)
2. Same nonce (attacker cannot regenerate a valid different nonce)
3. Different session key (X25519 key exchange produces fresh key per session)

**Consequence**: if a message uses a session-derived key AND the session has expired,
decryption of the replayed message fails (wrong key). Even if message ID deduplication
fails (Bloom filter false negative + LRU evicted), the recipient cannot decrypt the replay.

**Limitation**: within the same session, a replay of a previously delivered message
with the same session key WOULD decrypt. This is exactly the case that Layer 1 (message ID)
and Layer 2 (timestamp + TTL) prevent. The three layers complement each other.

## Interaction Between Layers

```
Message arrives at relay:

→ Layer 1: Check seen_message_cache
   Already seen? → DROP (fast, ~1μs)
   Not seen → continue

→ Layer 2: Check timestamp and TTL
   Future? → DROP
   Expired? → DROP
   Valid time range → continue

→ Layer 1 update: Add message_id to seen cache

→ Process message (verify signature, decrypt if needed, forward)

→ Layer 3: At recipient — decrypt with session key
   Decryption failure (wrong key) → not delivered to application
```

## Testing Replay Protection

Test vectors in `crates/iris-protocol/tests/replay_protection.rs`:

1. **Basic replay**: send message, cache sees ID, replay same message → dropped
2. **Restart replay**: send message, restart app (cache persisted), replay → dropped
3. **TTL expiry replay**: send message with TTL=10s, wait 15s, replay → dropped (expired)
4. **Future timestamp**: craft message with timestamp = now + 2 hours → dropped
5. **Bloom false positive**: generate 1M unique IDs to fill Bloom, then test new ID → some
   false positives expected, exact LRU provides safety net
6. **Store-carry-forward legitimacy**: send message with 7-day TTL, simulate 3-day relay
   storage, forward → accepted (within TTL)
