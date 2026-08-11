# Store-Carry-Forward (SCF)

## Overview

Store-Carry-Forward is the fundamental primitive that makes IRIS a Delay-Tolerant
Network rather than a conventional mesh network. When no forwarding path exists,
IRIS stores messages locally, waits while the node moves (carry), and forwards
messages when connectivity is re-established.

SCF transforms physical mobility from a problem into a solution: vehicles, rescue
workers, and animals carrying IRIS nodes become message mules connecting otherwise
isolated network partitions.

## Core Concept

```
Time 0: Node A wants to send message to Node C.
        No path exists: A—B is connected, B—C is not.

                [A] ——— [B]       [C]

Time 1: A stores message in B's buffer (B is a relay candidate).
        B is a vehicle that drives toward C's region.

Time 2: Vehicle B moves. Message is "carried" in B's local storage.

                [A]       [B] → → → [C]

Time 3: B arrives in range of C. B forwards the stored message to C.
        Delivery latency: transit time (Time 2 → Time 3).
```

## Storage Buffer Design

### Data Model

```rust
pub struct MessageStore {
    /// Ordered storage by (priority DESC, expiry ASC)
    storage: BTreeMap<StoreKey, StoredMessage>,
    total_bytes: usize,
    max_bytes: usize,  // configurable, default 500 MB
    db: SqlitePool,    // persistence layer
}

/// Key for BTreeMap ordering:
/// Priority higher = smaller key number (0 = highest priority P0)
/// TTL expiry: sooner-expiring messages evicted first among same priority
#[derive(Ord, PartialOrd, Eq, PartialEq)]
pub struct StoreKey {
    pub priority_rank: u8,    // 0 = P0 (highest), 7 = P7 (lowest)
    pub expiry_unix: u64,     // sooner expiry = smaller = evicted first
    pub message_id: MessageId,
}

pub struct StoredMessage {
    pub message: IrisMessage,
    pub stored_at: SystemTime,
    pub forward_attempts: u32,
    pub last_forward_attempt: Option<SystemTime>,
    pub received_from: Option<NodeId>,
    pub delivery_status: DeliveryStatus,
}
```

### Storage Capacity

| Device Type | Recommended Max | Notes |
|-------------|----------------|-------|
| Android phone (user) | 200 MB | Small fraction of total storage |
| Android phone (dedicated relay) | 2 GB | Dedicated relay device |
| Raspberry Pi edge node | 10 GB | Large SD card, edge node role |
| Desktop node | 50 GB | Command center node |

Default 500 MB is configurable in IRIS settings. Power users can increase; devices
with limited storage can decrease to 50 MB.

## Message Lifecycle in SCF

```
CREATED / RECEIVED
       ↓
    STORED ←──────────── FORWARD_FAILED (after max retries)
       ↓                        ↑
  CARRY (in transit)     FORWARD_ATTEMPT
       ↓                        ↑
  FORWARD_OPPORTUNITY ──────────┘
       ↓
  FORWARD_ATTEMPT
       ↓
   ACK_RECEIVED → DELIVERED → (remove from store)
       or
   TTL_EXPIRED → DROPPED → (remove from store, log)
```

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum DeliveryStatus {
    Stored,
    Carrying,           // node is mobile, no current contact for this dest
    AwaitingContact,    // stationary, no contact
    ForwardingInProgress { to: NodeId, transport: TransportId },
    AckPending { sent_to: NodeId, sent_at: SystemTime },
    Delivered { delivered_to: NodeId, at: SystemTime },
    Dropped { reason: DropReason, at: SystemTime },
}
```

## Eviction Policy

When the store exceeds capacity, eviction runs:

```rust
impl MessageStore {
    pub fn evict_to_fit(&mut self, needed_bytes: usize) -> Vec<MessageId> {
        let target = self.max_bytes.saturating_sub(needed_bytes);
        let mut evicted = Vec::new();

        // BTreeMap is ordered: priority_rank DESC (7=P7 first), expiry ASC (soonest first)
        // Iterate from end of map = lowest priority, soonest expiry
        while self.total_bytes > target {
            let key = match self.storage.keys().next_back().cloned() {
                Some(k) => k,
                None => break,
            };

            // NEVER evict P0 (priority_rank 0)
            if key.priority_rank == 0 {
                // Cannot make space without evicting P0 — give up
                log::warn!("Store full, cannot evict P0; dropping incoming message");
                break;
            }

            let msg = self.storage.remove(&key).unwrap();
            self.total_bytes -= msg.message.serialized_size();
            evicted.push(msg.message.id.clone());
            log::info!(
                "Evicted message {:?} (P{}, expires in {:?}) to make space",
                msg.message.id,
                key.priority_rank,
                msg.message.time_to_expiry()
            );
        }

        evicted
    }
}
```

### Eviction Priority (ordered, evict first → last):

1. P7 messages (lowest priority), soonest-expiring first
2. P6 messages, soonest-expiring first
3. P5 messages, soonest-expiring first
4. P4 messages, soonest-expiring first
5. P3 messages (only if P4–P7 exhausted)
6. P2 messages (only in extreme storage pressure)
7. P1 messages (almost never evicted)
8. **P0 messages: NEVER evicted**

## Vehicle Relay Scenario

The canonical IRIS use case for SCF:

### Setup

```
Ahmedabad (earthquake epicenter, no connectivity)
                ↓ BLE mesh
        [Field team A] ←→ [Field team B] ←→ [Medical post]
                            stores P0 SOS messages

                        Car: [Relief volunteer]
                            IRIS on phone
```

### Execution

```
09:00 Relief volunteer arrives at Ahmedabad mesh
      IRIS detects new contacts: field teams A and B
      SCF sync: 47 stored messages transferred (BLE, 30 seconds)
      Volunteer's IRIS: "47 messages received from local mesh"

09:30 Volunteer drives toward Kutch (has satellite/cellular)
      IRIS: P0 messages immediately forwarded via cellular (if available)
      Remaining messages stored for Kutch delivery

11:00 Volunteer arrives at Kutch relief camp
      IRIS connects to Kutch camp Wi-Fi
      Stored messages forwarded to Kutch IRIS nodes
      Kutch nodes forward to Internet relay
      Relay delivers to families in Delhi, Mumbai
```

### Time Complexity

Delivery latency = T_store + T_carry + T_forward

- T_store: time from message creation until stored in relay node
- T_carry: physical transit time (volunteer drives 200 km at 60 km/h = ~3.5 hours)
- T_forward: time from vehicle arrival to forwarding completion (seconds)

Total: ~4 hours. Acceptable for P3+ logistics messages.
For P0 SOS: 4 hours is too long — must use satellite/LoRa, not vehicle relay.

## Storage Integrity

Messages stored in SQLite with full ACID guarantees:

```sql
-- messages table schema
CREATE TABLE stored_messages (
    id              BLOB PRIMARY KEY,    -- MessageId (32 bytes)
    priority        INTEGER NOT NULL,    -- 0–7
    serialized      BLOB NOT NULL,       -- CBOR-encoded IrisMessage
    stored_at       INTEGER NOT NULL,    -- Unix timestamp
    expires_at      INTEGER NOT NULL,    -- Unix timestamp
    forward_count   INTEGER DEFAULT 0,
    last_attempt    INTEGER,             -- Unix timestamp, nullable
    status          TEXT NOT NULL,       -- DeliveryStatus as JSON
    received_from   BLOB                 -- NodeId, nullable
);

CREATE INDEX idx_priority_expiry ON stored_messages(priority DESC, expires_at ASC);
CREATE INDEX idx_expires ON stored_messages(expires_at);

-- Cleanup: remove expired messages (run hourly)
DELETE FROM stored_messages WHERE expires_at < unixepoch();
```

Write-ahead logging (WAL mode) ensures crash safety:

```rust
sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await?;
sqlx::query("PRAGMA synchronous=NORMAL").execute(&pool).await?;
// synchronous=NORMAL is crash-safe with WAL; FULL is slower with no additional benefit
```

## Contact Opportunity Handler

When a new neighbor is discovered, the SCF layer evaluates all stored messages:

```rust
pub async fn on_new_contact(
    &mut self,
    contact: NodeId,
    contact_info: &NeighborInfo,
) -> Result<()> {
    // Get all stored messages that could be forwarded to this contact
    let candidates = self.store.messages_forwardable_to(&contact).await?;

    // Sort by priority (P0 first), then by delivery probability to contact
    let mut ranked = candidates;
    ranked.sort_by(|a, b| {
        a.message.priority.cmp(&b.message.priority)
            .then_with(|| b.delivery_probability(&contact).partial_cmp(
                &a.delivery_probability(&contact)
            ).unwrap())
    });

    // Forward in order, respecting transport bandwidth limits
    for candidate in ranked {
        if self.transport_registry.has_capacity_for(&contact) {
            self.enqueue_forward(candidate.message, &contact).await?;
        }
    }
    Ok(())
}
```

## SCF Metrics

```rust
pub struct ScfMetrics {
    // Storage
    pub messages_stored_total: Counter,
    pub messages_evicted_total: CounterVec<Priority>,
    pub messages_expired_total: Counter,
    pub storage_bytes_used: Gauge,
    pub storage_bytes_max: Gauge,

    // Delivery
    pub messages_delivered_direct: Counter,
    pub messages_delivered_after_carry: Counter,
    pub average_store_time_seconds: Histogram,
    pub average_carry_distance_km: Histogram,

    // Forwarding
    pub forward_attempts_total: Counter,
    pub forward_success_rate: Gauge,
}
```

Key health indicators:
- `storage_bytes_used / storage_bytes_max > 0.9` → storage pressure, increase eviction
- `average_store_time_seconds > 3600` for P2 → routing not finding enough contacts
- `forward_success_rate < 0.8` → transport reliability issues
