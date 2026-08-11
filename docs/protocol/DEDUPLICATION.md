# Deduplication

## Purpose
Defines how IRIS prevents the same message from being delivered multiple times to the recipient, and how relay nodes avoid forwarding the same message repeatedly.

## Scope
All nodes (relay and user). Covers Bloom filter design, message ID uniqueness, and deduplication window.

---

## Why Deduplication is Hard in DTN

In a delay-tolerant mesh network:
- The same message may arrive via multiple independent paths simultaneously
- A relay node may encounter the same message bundle hours or days apart
- Epidemic routing intentionally floods copies to all neighbors

Without deduplication:
- Recipients see multiple notifications for the same SOS
- Relay nodes amplify message floods exponentially
- Storage fills with duplicate copies

---

## Message ID Uniqueness

Each message has a globally unique `msg_id`:

```
msg_id = BLAKE3(
    originator_node_id ||
    created_at_unix_millis (8 bytes, big-endian) ||
    random_nonce (8 bytes)
)
```

Truncated to 24 bytes (192 bits). Collision probability: negligible at scale.

The random nonce prevents two messages with same originator and timestamp from colliding (e.g., rapid SOS re-sends).

---

## Relay-Level Deduplication (Bloom Filter)

Each node maintains a Bloom filter of recently-seen `msg_id` values:

```rust
struct DeduplicationFilter {
    filter: BloomFilter,
    window_start: Instant,
    window_duration: Duration,   // 144 hours (2× max P0 TTL)
    capacity: usize,             // 100_000 entries
    false_positive_rate: f64,    // 0.001 (0.1%)
}
```

Filter parameters for 100k messages at 0.1% FPR:
- Bit array size: ~1.44 MB
- Hash functions: 10

Before forwarding:

```rust
fn should_forward_relay(msg_id: &MsgId, filter: &mut DeduplicationFilter) -> bool {
    if filter.contains(msg_id) {
        return false;  // Probably already forwarded — skip
    }
    filter.insert(msg_id);
    true
}
```

The 0.1% false positive rate means 1 in 1000 new messages is incorrectly dropped. Acceptable for relay behavior; not acceptable for recipient delivery.

---

## Recipient-Level Deduplication (Exact)

At delivery to the recipient, exact deduplication is used:

```sql
CREATE TABLE delivered_messages (
    msg_id BLOB PRIMARY KEY,    -- 24 bytes
    delivered_at INTEGER,
    expires_at INTEGER          -- retained until 2× max TTL after delivery
);
```

Before delivering to app:

```rust
fn should_deliver_to_app(msg_id: &MsgId, db: &Database) -> bool {
    if db.was_delivered(msg_id) {
        return false;  // exact duplicate — already delivered
    }
    db.mark_delivered(msg_id);
    true
}
```

No false positives at recipient level — every message is checked exactly.

---

## Deduplication Window

| Level    | Window     | Why                                           |
|----------|------------|-----------------------------------------------|
| Relay Bloom | 144h    | 2× P0 TTL — covers maximum message lifetime   |
| Recipient DB | 144h   | Same — prevents redelivery from late paths    |
| After window | Evict | `msg_id` removed from filter, could re-deliver|

After the deduplication window, a message with an old `msg_id` arriving at a relay node would be forwarded again. However:
- Its TTL would have expired (max 72h < 144h window)
- It would be dropped at the TTL check before the dedup check

So: deduplication window > max TTL is the invariant that ensures correctness.

---

## Bloom Filter Synchronization

During peer exchange, nodes share their Bloom filters to help each other skip already-seen messages:

```
// Exchange in discovery handshake
BloomExchange {
    filter_bytes: Vec<u8>,     // serialized Bloom filter
    generation: u64,           // monotonic counter
    created_at: u64,           // unix timestamp
}
```

A node receiving a peer's Bloom filter can skip forwarding messages the peer has already seen, reducing flooding.

Bloom filters are merged with bitwise OR (standard approach). This is conservative — OR can only increase "seen" membership, never falsely clear it.

---

## Storage Deduplication

The message store deduplicates at insert time:

```rust
fn store_message(&mut self, msg: Message) -> StoreResult {
    if self.contains(&msg.id) {
        return StoreResult::Duplicate;
    }
    self.insert(msg);
    StoreResult::Stored
}
```

This prevents the store from accumulating duplicate copies even if the relay Bloom filter had a false negative.

---

## Known Limitations

- The Bloom filter approach has a 0.1% false positive rate at relay level — legitimate new messages may rarely be dropped if their ID collides with a previously-seen message. With 24-byte IDs, genuine collision is negligible; false positives are the real risk.
- Bloom filters are reset when the node restarts (not persisted by default). After restart, the node may forward previously-forwarded messages. This is intentional — the alternative (persisting a 1.44 MB filter) has higher I/O cost.
- A sophisticated attacker can craft messages with `msg_id` values that partially fill the Bloom filter (DoS via filter pollution). Mitigation: rate-limit inserts to the filter from any single source node.

---

## References

- `docs/protocol/MESSAGE_ENVELOPE.md` — `msg_id` field definition
- `docs/protocol/TTL.md` — TTL interaction with dedup window
- `docs/routing/NETWORK_HEALING.md` — Bloom filter exchange during reconnection
- [Bloom Filters by Example](https://llimllib.github.io/bloomfilter-tutorial/)
