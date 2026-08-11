# Reassembly

## Purpose
Defines how IRIS nodes reassemble fragmented messages from individual transport fragments back into complete messages.

## Scope
Reassembly at the transport adapter layer, before the message is passed to the routing/delivery layer.

---

## Reassembly Buffer

Each node maintains a reassembly buffer for in-progress messages:

```rust
struct ReassemblyBuffer {
    entries: HashMap<MsgId, ReassemblyEntry>,
    max_entries: usize,         // 64 — prevent DoS via buffer exhaustion
    cleanup_interval: Duration, // 30s
}

struct ReassemblyEntry {
    msg_id: MsgId,
    frag_count: u16,
    received_frags: HashMap<u16, Vec<u8>>,  // index → data
    created_at: Instant,
    last_received_at: Instant,
    transport: TransportId,
    source_node: NodeId,
    timeout: Duration,
}
```

---

## Reassembly Timeouts by Transport

| Transport  | Reassembly Timeout | Rationale                          |
|------------|--------------------|------------------------------------|
| BLE 5.x    | 10 seconds         | Short range, fast — nearby peer    |
| Wi-Fi      | 5 seconds          | High bandwidth — fragments fast    |
| LoRa       | 300 seconds        | Slow, intermittent — 5 min wait    |
| Satellite  | 600 seconds        | Expensive, slow — 10 min wait      |
| Cellular   | 30 seconds         | Usually fast but variable          |

---

## Reassembly Process

```rust
fn receive_fragment(&mut self, frag: Fragment) -> Option<Vec<u8>> {
    let entry = self.entries.entry(frag.msg_id).or_insert(ReassemblyEntry {
        msg_id: frag.msg_id,
        frag_count: frag.frag_count,
        received_frags: HashMap::new(),
        created_at: Instant::now(),
        last_received_at: Instant::now(),
        transport: frag.transport,
        source_node: frag.source_node,
        timeout: self.timeout_for_transport(frag.transport),
    });

    // Verify fragment checksum
    let computed = blake3::hash(&frag.frag_data).as_bytes()[..8].to_vec();
    if computed != frag.frag_checksum {
        metrics::increment("reassembly.checksum_error");
        return None;  // corrupt fragment — discard, wait for retransmit
    }

    entry.received_frags.insert(frag.frag_index, frag.frag_data);
    entry.last_received_at = Instant::now();

    // Check if complete
    if entry.received_frags.len() == entry.frag_count as usize {
        let complete = self.assemble(entry);
        self.entries.remove(&frag.msg_id);
        return Some(complete);
    }

    None  // still waiting for more fragments
}

fn assemble(entry: &ReassemblyEntry) -> Vec<u8> {
    let mut result = Vec::new();
    for i in 0..entry.frag_count {
        result.extend_from_slice(&entry.received_frags[&i]);
    }
    result
}
```

---

## Missing Fragment Recovery

After timeout:

```rust
fn cleanup_stale_entries(&mut self) {
    let now = Instant::now();
    let timed_out: Vec<MsgId> = self.entries.iter()
        .filter(|(_, e)| now - e.last_received_at > e.timeout)
        .map(|(id, _)| *id)
        .collect();

    for msg_id in timed_out {
        let entry = self.entries.remove(&msg_id).unwrap();
        let missing: Vec<u16> = (0..entry.frag_count)
            .filter(|i| !entry.received_frags.contains_key(i))
            .collect();

        if !missing.is_empty() {
            // Request retransmit if source is still reachable
            if self.is_reachable(&entry.source_node) {
                self.send_fragment_request(entry.source_node, msg_id, missing);
            } else {
                // Discard — cannot recover
                metrics::increment("reassembly.timeout_discard");
                log::warn!("Discarding incomplete message {:?} — {} fragments missing",
                    msg_id, missing.len());
            }
        }
    }
}
```

---

## DoS Protection

The reassembly buffer has a hard cap of 64 concurrent entries per node. When full:
- New fragments from the same source as the oldest entry: add to that entry
- Fragments from a new `msg_id`: reject with `ReassemblyBufferFull` error
- The oldest entry (by `created_at`) is evicted if absolutely necessary

Rate limiting: max 100 fragments per source node per 60 seconds. Exceeding this triggers a temporary source-node drop.

---

## Buffer Overflow on Low-Memory Devices

For Pi Zero / ESP32 deployments with <512 MB RAM:
- Buffer cap reduced to 8 entries
- Fragment data stored to SQLite rather than RAM
- Timeout extended by 30% to allow for I/O latency

---

## Reassembled Message Validation

After reassembly, the complete message is verified:
1. CBOR decode — must parse without error
2. Signature verification — Ed25519 signature over the complete payload
3. TTL check — assembled message must not be expired
4. Size check — assembled size must match declared size in envelope

If any check fails, the message is discarded (not passed to routing layer).

---

## Known Limitations

- A 5-fragment LoRa message with one corrupted fragment cannot be recovered until the source retransmits the corrupt fragment. If the source is out of range, the message is lost.
- The 64-entry buffer cap means very busy relay nodes may reject fragments during disaster peaks. The limit can be tuned but trades RAM for reliability.
- No FEC (Forward Error Correction) — fragments are either received intact or not at all. FEC would improve LoRa reliability significantly but adds computational overhead.

---

## References

- `docs/protocol/FRAGMENTATION.md` — How fragments are created
- `docs/transports/LORA.md` — LoRa-specific constraints
- `docs/protocol/MESSAGE_ENVELOPE.md` — Envelope format for assembled message
