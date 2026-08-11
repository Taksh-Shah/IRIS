# TTL (Time-to-Live)

## Purpose
Defines how IRIS manages message expiry: how TTLs are set, transmitted, decremented, and enforced across relay nodes.

## Scope
All message types, all relay behaviors. Includes clock skew handling.

---

## TTL Field in Message Envelope

Each message carries:
- `expires_at_unix`: u64 — absolute Unix timestamp (seconds) when message expires
- Derived from `created_at + ttl_seconds` at origination

Absolute expiry is used instead of relative TTL because:
1. Relay nodes do not need synchronized hop counters
2. Handles variable relay delays correctly
3. Compatible with store-carry-forward (message may be stored 3 days before relay)

---

## Default TTL by Priority

| Priority | Default TTL | Rationale                                            |
|----------|-------------|------------------------------------------------------|
| P0 SOS   | 72 hours    | SOS must survive multi-day disruptions               |
| P1 Med   | 48 hours    | Medical requests remain actionable for 2 days        |
| P2 Loc   | 24 hours    | Location data stale after 24h                        |
| P3 Emtx  | 12 hours    | Emergency text relevant for 12h                      |
| P4 Norm  | 6 hours     | Normal message relevance window                      |
| P5 Img   | 3 hours     | Images: shorter cache window                         |
| P6 Voice | 1 hour      | Voice messages: time-sensitive                       |
| P7 Video | 30 minutes  | Video: high storage cost, short relevance            |

Users may set shorter TTL (never longer) for P4-P7. P0-P3 TTLs are not user-configurable.

---

## TTL Enforcement at Relay

Before forwarding a message:

```rust
fn should_forward(msg: &Message, now: UnixTimestamp, clock_skew_budget: Duration) -> bool {
    let expires = msg.expires_at_unix;
    let cushion = clock_skew_budget.as_secs();  // 300s default
    // Drop if expired even accounting for clock skew
    if now > expires + cushion {
        return false;
    }
    // Drop if would expire before estimated delivery
    let estimated_delivery = now + estimated_relay_delay(msg);
    if estimated_delivery > expires {
        return false;  // not worth forwarding — will expire in transit
    }
    true
}
```

Expired messages are dropped and their storage slot freed.

---

## Clock Skew Handling

IRIS devices may have clocks that differ by minutes or hours (no GPS, no NTP in disaster).

Approach:
1. **Clock skew budget**: 300 seconds (5 minutes). Messages are considered non-expired if `now - expires_at < 300s`.
2. **Observed skew tracking**: when two devices exchange messages, estimate clock delta from message timestamps. Track as `observed_skew: HashMap<NodeId, i64>`.
3. **Skew-adjusted expiry check**: `expires_at + observed_skew[sender]` for per-peer TTL calculation.
4. **Never extend TTL**: relay nodes may apply the skew budget to be lenient about expiry but cannot extend `expires_at` in the message itself.

For devices with very large clock skew (>1 hour):
- Log the discrepancy as an observability event
- Apply 5-minute budget only (do not apply observed skew > 3600s — may indicate a tampered timestamp)
- Flag in `MessageStatus::ClockSkewSuspect`

---

## Storage and TTL

Messages in store-carry-forward storage are subject to TTL:

```sql
-- Periodic cleanup (run every 5 minutes)
DELETE FROM messages
WHERE expires_at_unix < strftime('%s', 'now') - 300;
-- 300s clock skew budget applied to storage cleanup too
```

P0 messages are exempt from automatic storage cleanup — they remain in storage until explicitly delivered and acknowledged, or manually cleared.

---

## TTL and Deduplication

Expired message IDs are retained in the deduplication Bloom filter for `2 × max_ttl` (144 hours) after expiry to prevent re-injection of old messages with reset TTL. See `DEDUPLICATION.md`.

---

## Known Limitations

- Clock skew > 5 minutes causes TTL drift. In large disasters, device clocks can be off by hours. The 300-second budget is a compromise — too large and old messages persist, too small and relay nodes reject valid messages.
- There is no mechanism to extend TTL from the receiving side. A message with a 6-hour TTL that hasn't been delivered after 6 hours is gone.
- P0 TTL of 72 hours means a fully-partitioned node will accumulate P0 messages. Storage management for long-duration P0 floods is an open problem.

---

## References

- `docs/protocol/MESSAGE_ENVELOPE.md` — `expires_at_unix` field definition
- `docs/protocol/DEDUPLICATION.md` — ID retention after expiry
- `docs/protocol/DELIVERY_POLICIES.md` — Retry schedules interact with TTL
- RFC 9171 (BPv7) §4.2.4 — Bundle lifetime
