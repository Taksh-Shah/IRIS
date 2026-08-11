# Retries

## Purpose
Defines retry behavior for message transmission across IRIS transports — when to retry, how frequently, and when to stop.

## Scope
All transports, all delivery policies. Includes backoff strategy, transport switching, and retry budget.

---

## Retry Triggers

A retry is triggered when:
1. **No ACK received within timeout** (for CONFIRMED policy)
2. **Transport write failure** — OS-level send error
3. **Transport disconnected** — peer moved out of range
4. **Relay rejection** — relay explicitly rejects message (capacity full, rate limited)

Not a retry trigger:
- Message TTL expired — drop, do not retry
- Recipient explicitly rejected message (future: rejection ACK)
- Storage full and message was P7 — drop per eviction policy

---

## Retry Schedule by Priority

Exponential backoff with jitter, bounded by max interval:

```
retry_interval(attempt, priority) = min(
    base_interval(priority) × 2^attempt + jitter(0..base_interval),
    max_interval(priority)
)
```

| Priority | Base Interval | Max Interval | Max Attempts |
|----------|--------------|--------------|--------------|
| P0       | 30s          | 30s (no backoff) | Until TTL  |
| P1       | 60s          | 60s (no backoff) | Until TTL  |
| P2       | 120s         | 300s         | Until TTL    |
| P3       | 300s         | 600s         | 20           |
| P4       | 600s         | 1800s        | 10           |
| P5-P7    | —            | —            | 1 (no retry) |

P0 and P1 do not use backoff — they retry at constant interval because delivery speed is critical.

---

## Transport Switching on Retry

If a message fails on one transport, the retry system attempts other available transports:

```rust
fn retry_on_failure(&mut self, msg: &Message, failed_transport: TransportId) {
    let candidates = self.transports.available_for_priority(msg.priority)
        .filter(|t| *t != failed_transport)
        .sorted_by_key(|t| t.preference_score());

    for transport in candidates {
        if transport.can_send(msg) {
            self.enqueue(msg, transport);
            return;
        }
    }
    // No alternative transport — re-queue on same transport with backoff
    self.enqueue_with_backoff(msg, failed_transport);
}
```

Priority of transport switching (highest first):
1. Wi-Fi Direct / Wi-Fi Aware (high bandwidth, low latency)
2. BLE (lower bandwidth, widely available)
3. Cellular (if available)
4. Internet relay (if gateway reachable)
5. LoRa (P0-P2 only, very low bandwidth)
6. Satellite (P0-P2 only, expensive)

---

## Store-and-Retry

When no transport is available, the message enters store-and-retry state:

```rust
enum MessageState {
    Queued,              // transport available, pending send
    Sending,             // in-progress
    StoreAndRetry {      // no transport — wait for one
        stored_at: Instant,
        attempt: u32,
    },
    Delivered,
    Expired,
}
```

In `StoreAndRetry`, the message is persisted to SQLite. When a new transport becomes available, all stored messages are re-evaluated and queued in priority order.

---

## Jitter

Jitter prevents retry storms when many nodes regain connectivity simultaneously (e.g., when a gateway comes back online):

```rust
fn jitter(base: Duration) -> Duration {
    let max_jitter = base.min(Duration::from_secs(30));
    Duration::from_millis(rand::random::<u64>() % max_jitter.as_millis() as u64)
}
```

---

## Retry Budget Tracking

Each message tracks its retry state:

```rust
struct RetryState {
    attempt: u32,
    last_attempt_at: Instant,
    next_attempt_at: Instant,
    failed_transports: Vec<TransportId>,
    last_failure_reason: Option<TransmitError>,
}
```

The retry budget is consumed across all transports, not per-transport.

---

## Retry and Delivery Policy

| Policy            | Retry Behavior                                  |
|-------------------|-------------------------------------------------|
| BEST_EFFORT       | 1 attempt per transport, no retry               |
| CONFIRMED         | Full retry with ACK tracking                    |
| PRIORITY          | Retry per schedule, no custody ACK              |
| EMERGENCY_BROADCAST | Continuous broadcast, not traditional retry  |
| MULTICAST         | Per-recipient retry tracking                    |

---

## Cancellation

User can cancel a pending/retrying message via the UI. Cancellation:
1. Removes message from retry queue
2. Sends cancellation signal to relays that accepted custody (best-effort)
3. Marks message as `UserCancelled` in local store

Relay nodes receiving a cancellation signal mark the message `Cancelled` and stop forwarding. They cannot recall a message already forwarded.

---

## Known Limitations

- Relay nodes do not implement the full retry protocol — they forward best-effort. Only the originating device retries with full schedule.
- In a fully partitioned network with no storage, a retry storm could occur when connectivity restores if many devices have accumulated P0-P2 backlog.
- "Retry until TTL" for P0-P1 can fill queues on relay nodes in sustained disaster scenarios.

---

## References

- `docs/protocol/DELIVERY_POLICIES.md`
- `docs/protocol/ACKNOWLEDGEMENTS.md`
- `docs/routing/STORE_CARRY_FORWARD.md`
