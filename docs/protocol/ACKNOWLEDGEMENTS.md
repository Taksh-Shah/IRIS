# Acknowledgements

## Purpose
Defines how IRIS tracks and signals message delivery confirmation across unreliable, intermittently-connected transports.

## Scope
End-to-end acknowledgement (originator ↔ recipient) and hop-by-hop custody acknowledgement for CONFIRMED delivery policy.

---

## Acknowledgement Types

### 1. End-to-End ACK (E2E-ACK)
Confirms the recipient's device received and decrypted the message.

- Sent by: recipient node
- Received by: originator node (may transit relay nodes)
- Format: Small CBOR message with `msg_id` and `ack_type: E2E`
- Signed by recipient's Ed25519 key
- Priority: same priority as original message (P0 ACK is P0)

### 2. Custody ACK (C-ACK)
Confirms a relay node has accepted responsibility for forwarding the message.

- Sent by: each relay that accepts custody
- Received by: previous node in chain
- Format: CBOR with `msg_id`, `ack_type: CUSTODY`, `relay_id`
- Signed by relay's Ed25519 key
- Priority: P4 (ACKs are low overhead, low priority)
- Only generated under CONFIRMED delivery policy

### 3. Delivery-to-Gateway ACK (GW-ACK)
Confirms a gateway has accepted and begun forwarding over Internet/Satellite.

- Sent by: gateway node
- Priority: P3
- Includes estimated delivery time if known

---

## ACK Message Format (CBOR)

```
{0: msg_id,          ; original message ID (24 bytes)
 1: ack_type,        ; 0=E2E, 1=CUSTODY, 2=GATEWAY
 2: ack_node_id,     ; who is ACKing (32 bytes, BLAKE3 hash)
 3: timestamp,       ; unix timestamp of ACK
 4: signature}       ; Ed25519 signature over fields 0-3
```

Total E2E-ACK size: ~120 bytes — eligible for LoRa transmission.

---

## ACK Propagation

ACKs travel in the reverse direction through whatever path is available (not necessarily the same path as the original message). In a partitioned network:

```
Device A → Relay R1 → Relay R2 → Device B
                                     ↓ E2E-ACK sent
Device A ← Relay R1 ← Relay R2 ← ACK path (may differ)
```

If no reverse path exists, the ACK is stored at Device B and forwarded when a path appears.

Originator tracks ACK status per message:

```rust
enum AckStatus {
    Pending { deadline: Instant },
    CustodyConfirmed { relay_id: NodeId, at: Instant },
    GatewayAccepted { gw_id: NodeId, at: Instant },
    Delivered { at: Instant },
    Failed { reason: AckFailureReason },
}
```

---

## ACK Timeout and Retry

| Priority | ACK Wait | Retry After | Max Retries |
|----------|----------|-------------|-------------|
| P0       | 30s      | 30s         | Unlimited until TTL expiry |
| P1       | 60s      | 60s         | Unlimited until TTL expiry |
| P2       | 120s     | 120s        | 10           |
| P3       | 300s     | 300s        | 5            |
| P4       | 600s     | 600s        | 3            |
| P5-P7    | 1800s    | —           | 1 (best-effort only) |

P0 messages retransmit indefinitely until either:
- E2E ACK received, OR
- TTL expires (72 hours), OR
- User cancels

---

## Duplicate ACK Handling

ACKs may be duplicated if retransmitted. The originator deduplicates by `(msg_id, ack_node_id)`:

```rust
fn handle_ack(&mut self, ack: Ack) {
    let key = (ack.msg_id, ack.ack_node_id);
    if self.seen_acks.contains(&key) {
        return;  // duplicate, ignore
    }
    self.seen_acks.insert(key);
    self.update_ack_status(ack);
}
```

---

## Delivery Policy Interaction

| Policy            | ACK Behavior                                              |
|-------------------|-----------------------------------------------------------|
| BEST_EFFORT       | No ACK expected or tracked                                |
| CONFIRMED         | E2E-ACK required; custody ACK at each hop                 |
| PRIORITY          | E2E-ACK tracked; no custody ACK (performance)             |
| EMERGENCY_BROADCAST | ACK from at least one recipient; broadcast continues   |
| MULTICAST         | ACK from each named recipient                             |

See `DELIVERY_POLICIES.md` for full policy definitions.

---

## UI Delivery Status

Mapped from `AckStatus` for display:

| AckStatus              | UI Label       | Icon  |
|------------------------|----------------|-------|
| Pending                | Sending…       | ○     |
| CustodyConfirmed       | Relaying       | ◑     |
| GatewayAccepted        | Internet Route | ◕     |
| Delivered              | Delivered ✓    | ●     |
| Failed                 | Not Delivered  | ✗     |

---

## Security

ACK messages are signed by the acknowledging node. An unsigned or invalid-signature ACK is rejected.

An attacker cannot forge a delivery confirmation because the E2E-ACK is signed by the recipient's private key, which only the recipient holds. A relay node cannot generate a fake E2E-ACK.

Custody ACKs are signed by the relay. A malicious relay could claim custody and then drop the message — this is the DTN custody transfer problem. Mitigation: track per-relay delivery success rate; demote untrustworthy relays.

---

## Known Limitations

- In long-partition scenarios (days), ACK may never arrive even if message was delivered. The originator sees "Pending" indefinitely until TTL expiry.
- ACK path may differ from message path, so custody ACK chain may be incomplete.
- Emergency broadcast ACK is probabilistic — the originator may not receive ACK from all recipients.

---

## References

- `docs/protocol/DELIVERY_POLICIES.md`
- `docs/protocol/RETRIES.md`
- RFC 9171 §5.10 — Bundle status reports (IRIS uses a simplified version)
