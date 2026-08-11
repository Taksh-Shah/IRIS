# Priority Model

## Purpose
Defines the eight-level message priority system that governs queuing, transmission, storage eviction, and resource allocation across all IRIS transports.

## Scope
All message types, all transports, all node types. Priority is assigned at origination and cannot be changed in transit.

## Non-Goals
- Dynamic priority re-assignment based on network conditions (priority is immutable once set)
- Per-user priority customization (priority is determined by message type)

---

## Priority Levels

| Level | Name        | Code | Use Cases                                     | Max TTL | Max Size  | LoRa Eligible |
|-------|-------------|------|-----------------------------------------------|---------|-----------|---------------|
| P0    | SOS         | 0    | Life-threatening emergency, SOS beacon        | 72h     | 256 bytes | Yes           |
| P1    | Medical     | 1    | Medical situation, triage request             | 48h     | 512 bytes | Yes           |
| P2    | Location    | 2    | GPS location share, evacuation route          | 24h     | 128 bytes | Yes           |
| P3    | Emergency Text | 3 | Emergency text message, status update        | 12h     | 1 KB      | No (fragmented) |
| P4    | Normal      | 4    | Standard text message                         | 6h      | 4 KB      | No            |
| P5    | Image       | 5    | Photo, document scan                          | 3h      | 512 KB    | No            |
| P6    | Voice       | 6    | Voice message recording                       | 1h      | 2 MB      | No            |
| P7    | Video       | 7    | Video clip                                    | 30min   | 10 MB     | No            |

---

## Priority Determination at Origination

Priority is set by message type, not by user choice:

```
fn assign_priority(msg: &Message) -> Priority {
    match msg.content_type {
        ContentType::SosBeacon           => Priority::P0,
        ContentType::MedicalRequest      => Priority::P1,
        ContentType::LocationUpdate      => Priority::P2,
        ContentType::EmergencyText       => Priority::P3,
        ContentType::Text                => Priority::P4,
        ContentType::Image               => Priority::P5,
        ContentType::Voice               => Priority::P6,
        ContentType::Video               => Priority::P7,
    }
}
```

Users may request P3 (Emergency Text) when sending any text message during an active emergency declaration. The app validates the declaration context before allowing the upgrade.

---

## Queue Discipline

Each transport maintains a priority queue. Transmission order:

1. P0 always first. Multiple P0 messages: FIFO within P0.
2. P1 next. Multiple P1: FIFO within P1.
3. Continue through P7.
4. Starvation prevention: after 60 seconds of continuous P0 transmission, insert one P1-or-lower message. Reset timer. This prevents P4-P7 from being completely starved during sustained emergency.

```
struct PriorityQueue {
    queues: [VecDeque<QueuedMessage>; 8],  // P0..P7
    starvation_timer: Instant,
    starvation_threshold: Duration,        // 60s
}
```

---

## Storage Eviction Policy

When storage is full, evict by priority (lowest first) and within same priority by TTL proximity (soonest-expired first):

```
fn eviction_candidate(store: &MessageStore) -> Option<MessageId> {
    for priority in (Priority::P7..=Priority::P0).rev() {
        let expired = store.soonest_expiring_at(priority);
        if let Some(msg) = expired {
            return Some(msg.id);
        }
    }
    None  // store empty or all P0 — caller must handle
}
```

**Never evict P0 messages.** If store is full of P0 messages, reject new P1-P7 rather than evicting P0.

---

## Transport Capability vs Priority

| Transport   | P0 | P1 | P2 | P3 | P4 | P5 | P6 | P7 |
|-------------|----|----|----|----|----|----|----|----|
| BLE 5.x     | ✓  | ✓  | ✓  | ✓  | ✓  | ✗  | ✗  | ✗  |
| Wi-Fi Direct| ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  |
| LoRa        | ✓  | ✓  | ✓  | ✗  | ✗  | ✗  | ✗  | ✗  |
| Satellite   | ✓  | ✓  | ✓  | ✗  | ✗  | ✗  | ✗  | ✗  |
| Cellular    | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  |
| Internet    | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  | ✓  |

P3+ messages on LoRa/Satellite require fragmentation; this is prohibited by default.

---

## Emergency Priority Override

When a node receives a P0 message addressed to or from a node in the same mesh partition:
- All non-P0 transmissions pause for up to 5 seconds
- P0 transmitted on all available transports simultaneously (multi-homing broadcast)
- Acknowledgement tracked with 30-second timeout before re-broadcast

This override cannot be triggered by relay nodes — only by the originating device or a verified gateway acting on explicit relay authority.

---

## Battery Mode Interaction

| Battery Level | Priority Cutoff     | Behavior                              |
|---------------|---------------------|---------------------------------------|
| >50%          | None                | All priorities transmitted normally   |
| 20-50%        | P5-P7 paused        | Images/voice/video deferred           |
| 10-20%        | P4-P7 paused        | Only emergency messages transmitted   |
| 5-10%         | P2-P7 paused        | Only P0-P1 transmitted                |
| <5%           | P1-P7 paused        | Emergency beacon (P0) only            |

The device always transmits its own SOS (P0) regardless of battery level.

---

## Known Limitations

- P3 (Emergency Text) on LoRa requires fragmentation which increases air time and duty cycle consumption. In practice, P3 on LoRa is infeasible without extended dwell time.
- The 60-second starvation prevention is a heuristic — in extreme sustained P0 floods, P4-P7 may be effectively blocked for extended periods.
- Priority is assigned at origination: a message sent as P4 remains P4 even if the situation changes. Users must re-send with higher priority if needed.

---

## References

- `docs/protocol/MESSAGE_MODEL.md` — Per-priority message schemas
- `docs/protocol/DELIVERY_POLICIES.md` — Retry schedules by priority
- `engineering/PRIORITY_POLICY.yaml` — Node execution priority (separate from message priority)
- `docs/architecture/FAILURE_ARCHITECTURE.md` — Priority during failure modes
