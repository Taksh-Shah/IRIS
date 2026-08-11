# Fragmentation

## Purpose
Defines how IRIS splits large messages into fragments for transmission over bandwidth-constrained transports (primarily LoRa), and ensures correct reassembly.

## Scope
Fragmentation at the transport adapter level. Covers fragment format, ordering, loss handling.

---

## When Fragmentation Occurs

Fragmentation is triggered when `message_size > transport.max_payload_bytes`:

| Transport   | Max Payload | Fragment Threshold |
|-------------|-------------|-------------------|
| LoRa SF12   | 51 bytes    | Any message >51 bytes |
| LoRa SF7    | 222 bytes   | Any message >222 bytes |
| BLE 5.x     | 244 bytes   | Any message >244 bytes |
| Wi-Fi Direct| 65,535 bytes| Any message >64 KB |
| Satellite (Iridium SBD) | 270 bytes | Any message >270 bytes |

For LoRa and Satellite, fragmentation of P3+ messages is **prohibited by default** because:
- Each fragment consumes duty cycle
- Lost fragments cannot be recovered easily
- Better to refuse than to partially deliver

Only P0-P2 messages are fragmented on LoRa/Satellite.

---

## Fragment Format (CBOR)

```
Fragment = {
    0: msg_id,          ; original message ID (24 bytes)
    1: frag_index,      ; 0-based fragment index (uint)
    2: frag_count,      ; total number of fragments (uint)
    3: frag_data,       ; fragment payload (bytes)
    4: frag_checksum,   ; BLAKE3 hash of frag_data (8 bytes, truncated)
}
```

The original message `msg_id` is preserved so deduplication applies to the full message, not individual fragments.

---

## Fragmentation Algorithm

```rust
fn fragment(msg: &[u8], max_frag_size: usize, msg_id: MsgId) -> Vec<Fragment> {
    let chunks: Vec<&[u8]> = msg.chunks(max_frag_size).collect();
    let frag_count = chunks.len();
    chunks.iter().enumerate().map(|(i, chunk)| {
        Fragment {
            msg_id,
            frag_index: i as u16,
            frag_count: frag_count as u16,
            frag_data: chunk.to_vec(),
            frag_checksum: blake3::hash(chunk).as_bytes()[..8].try_into().unwrap(),
        }
    }).collect()
}
```

Fragments are transmitted in order (0..frag_count-1) but may arrive out of order.

---

## Fragment Ordering

Fragments are transmitted in order for efficiency, but the reassembler handles any order. Each fragment carries its index, so order is derived from the CBOR, not transmission order.

On LoRa (regulated duty cycle):
- All fragments of a P0 message are transmitted before any P1-P2
- Inter-fragment gap respects duty cycle limits

---

## Reassembly

See `REASSEMBLY.md` for the full reassembly protocol. Summary:
- Fragments are buffered until `frag_count` fragments with the same `msg_id` are received
- Checksums are verified per fragment before including in reassembly
- If any fragment is missing after timeout, the message is treated as lost

---

## LoRa Fragment Budget (P0 Example)

P0 SOS message: 226 bytes (fixed header) + 29 bytes payload = 255 bytes  
LoRa SF12 max payload: 51 bytes  

```
Fragments needed = ceil(255 / 51) = 5 fragments
Air time per fragment (SF12, 125 kHz BW): ~2.5s
Total air time: 5 × 2.5s = 12.5s
Duty cycle (1%): 1250s listening budget per 12.5s used
Recovery time: 12.5s / 0.01 = 1250s ≈ 20 minutes
```

For P0 SOS, this 20-minute duty cycle recovery is acceptable. For P3 text (1 KB), it would require ~20 fragments and 4+ hours of recovery — prohibited.

---

## Fragment Loss Handling

If fragments are lost:
1. Receiver buffers complete fragments, tracks missing indices
2. After `reassembly_timeout`, sends `FragmentRequest` to sender (if path exists)
3. Sender retransmits missing fragments only
4. If no path, marks message as `FragmentLoss` and discards buffer

```rust
struct FragmentBuffer {
    msg_id: MsgId,
    frag_count: u16,
    received: HashMap<u16, Vec<u8>>,
    created_at: Instant,
    timeout: Duration,              // 60s for LoRa, 10s for BLE
}
```

---

## Known Limitations

- LoRa fragmentation of P3+ messages is prohibited due to duty cycle cost. Effectively, P3+ cannot be sent over LoRa.
- Fragment loss is common on LoRa links with intermittent coverage. A 5-fragment P0 message with 10% packet loss has a 41% chance of losing at least one fragment (`1 - 0.9^5 = 0.41`). The retransmit mechanism mitigates this but adds latency.
- No interleaving or FEC — fragmentation is pure splitting. FEC is a future enhancement.

---

## References

- `docs/protocol/REASSEMBLY.md`
- `docs/transports/LORA.md` — duty cycle constraints
- `docs/transports/SATELLITE.md` — Iridium SBD 270-byte limit
- `docs/protocol/PRIORITY_MODEL.md` — LoRa eligibility by priority
