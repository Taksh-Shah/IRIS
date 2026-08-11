# Synchronization

## Purpose
Defines how two IRIS nodes synchronize their message stores when they come into contact, ensuring efficient bi-directional transfer without redundant retransmission.

## Scope
Store-and-forward synchronization between any two nodes. Covers discovery, bloom filter exchange, differential transfer, and completion.

---

## Why Synchronization

In DTN store-carry-forward operation, two nodes that encounter each other may each have messages the other needs. Naïve approaches:
- **Flood all messages**: wastes bandwidth, especially on BLE/LoRa
- **Request specific message IDs**: requires prior knowledge of what the other has
- **Bloom filter exchange**: efficient — each node advertises what it has, other side requests only what it's missing

IRIS uses Bloom filter exchange for discovery, followed by targeted transfer.

---

## Synchronization Protocol

### Phase 1: Bloom Filter Exchange

Immediately after transport handshake, both nodes exchange their message inventory Bloom filters:

```
Node A → Node B: SyncOffer {
    bloom_filter: bytes,    // serialized filter of A's stored msg_ids
    node_id: NodeId,
    generation: u64,        // monotonic, for cache invalidation
    timestamp: u64,
}

Node B → Node A: SyncOffer {
    bloom_filter: bytes,
    node_id: NodeId,
    generation: u64,
    timestamp: u64,
}
```

Bloom filter for 10,000 stored messages at 1% FPR: ~12 KB — acceptable for BLE and LoRa.

### Phase 2: Request Missing Messages

Each node queries its own store against the peer's Bloom filter:

```rust
fn compute_transfer_list(
    my_store: &MessageStore,
    peer_bloom: &BloomFilter,
    priority_cutoff: Priority,  // filter by transport capability
) -> Vec<MsgId> {
    my_store.all_ids()
        .filter(|id| !peer_bloom.contains(id))    // peer doesn't have it
        .filter(|id| my_store.priority(id) <= priority_cutoff)
        .sorted_by_key(|id| my_store.priority(id))  // P0 first
        .collect()
}
```

Each node builds its own transfer list (what it can offer the peer).

### Phase 3: Transfer

Messages are transferred in priority order. For each candidate:
1. Sender streams message bundle
2. Receiver verifies signature and TTL
3. Receiver sends micro-ACK per message (4-byte confirmation)
4. Move to next

Transfer is aborted if:
- Peer disconnects
- Transport signals insufficient bandwidth
- TTL of queued messages is imminent

### Phase 4: Completion

```
Node A → Node B: SyncComplete { transferred: u32, bytes: u64 }
Node B → Node A: SyncComplete { transferred: u32, bytes: u64 }
```

Both nodes update their routing tables with new connectivity information observed from the sync.

---

## Sync Priority Constraints

| Transport   | Max Priority Synced | Max Bytes/Sync |
|-------------|---------------------|----------------|
| BLE 5.x     | P4 (Normal text)    | 500 KB         |
| Wi-Fi Direct| P7 (Video)          | Unlimited      |
| LoRa        | P2 (Location)       | 200 bytes      |
| Satellite   | P1 (Medical)        | 340 bytes      |

On BLE/LoRa, large messages (P5-P7) are skipped during sync even if the peer doesn't have them.

---

## Incremental Sync

For long-lived connections (e.g., two nodes in the same shelter for hours), sync runs incrementally:
- After full initial sync, re-run Bloom filter exchange every 60 seconds
- Only new messages since last sync are transferred
- Each node tracks `last_sync_at[peer_id]` to limit re-scanning

---

## Conflict Resolution

Two nodes may have different versions of a "mutable" object (e.g., contact list, routing table). IRIS uses last-write-wins by timestamp for routing data:

```rust
fn merge_routing_entry(local: &RoutingEntry, remote: &RoutingEntry) -> RoutingEntry {
    if remote.updated_at > local.updated_at {
        remote.clone()
    } else {
        local.clone()
    }
}
```

Messages themselves are immutable once created — no conflict resolution needed.

---

## Performance Characteristics

Full sync between two nodes with 1,000 messages each on BLE:
- Bloom filter exchange: ~12 KB × 2 = 24 KB, ~0.5s
- Transfer of 100 unique messages (1 KB avg): ~100 KB, ~2-4s
- Total: ~3-5s for typical encounter

On LoRa (very constrained):
- Bloom filter exchange: ~12 KB — exceeds LoRa frame size, requires fragmentation
- Alternative: send abbreviated filter (top-200 messages by priority) in ~1.5 KB

---

## Known Limitations

- Bloom filter false positives cause the sender to skip messages the receiver actually needs (1% FPR means 1 in 100 messages not transferred). Acceptable for relay behavior.
- In very high-traffic scenarios (100+ devices syncing simultaneously), BLE connection slots fill up and sync is serialized. Sync latency increases with number of simultaneous contacts.
- LoRa Bloom filter exchange requires fragmentation — adds air time and duty cycle burden.

---

## References

- `docs/protocol/DEDUPLICATION.md` — Bloom filter structure
- `docs/routing/STORE_CARRY_FORWARD.md` — Storage format
- `docs/routing/NETWORK_HEALING.md` — Bloom exchange in routing context
- `docs/transports/BLE.md` — BLE throughput constraints
