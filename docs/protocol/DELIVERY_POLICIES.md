# Delivery Policies

## Overview
Delivery policy defines what guarantees the system makes about message delivery and what effort it expends on behalf of the sender. Policy is independent of priority: a high-priority message may be best-effort, and a low-priority message may require confirmed delivery.

Policy is encoded in the envelope (field, see MESSAGE_ENVELOPE.md) and respected by all relay nodes.

## Policy Types

### BEST_EFFORT (Policy Code 0)

The simplest policy. Send when a path is available; do not retry if delivery is not confirmed.

**Guarantees made:**
- Message will be sent at least once when a path to recipient is available
- Message will be stored until TTL expires if no path currently available
- No guarantee of delivery — recipient may never receive the message

**Guarantees not made:**
- No delivery confirmation
- No retry on failure beyond transport-layer retransmission
- No notification to sender of delivery status

**Relay behavior:**
- Queue message by priority (normal queue position)
- Forward when opportunity arises
- Do not track delivery after forwarding
- Do not retry on ACK timeout

**Used by:**
- P4 normal text (default)
- P5 images
- P6 voice
- P7 video/bulk
- P2 location (when broadcast)

**Example scenario:** User sends a photo to a contact. Photo is queued and delivered whenever the contact's node is reachable. User does not get a delivery confirmation. Appropriate for non-critical media.

### CONFIRMED_DELIVERY (Policy Code 1)

Send and wait for end-to-end acknowledgment. Retry until ACK received or TTL expires.

**Guarantees made:**
- Message will be retried until: ACK received, or TTL expires
- Sender is notified of delivery (ACK received) or failure (TTL expired, delivery timeout)
- Retry uses exponential backoff to avoid flooding narrow-bandwidth paths

**Guarantees not made:**
- No guarantee of delivery if recipient is permanently unreachable within TTL
- No guarantee of delivery time (may take full TTL duration)

**Relay behavior:**
- Queue message by priority
- Forward when opportunity arises
- Store message until ACK received or TTL expires
- On ACK timeout (30s): mark for retry, attempt alternate path
- On ACK received: remove from pending, notify originator

**Retry schedule:**
```
Attempt 1:  send immediately
Attempt 2:  wait 30s, retry via alternate transport/path
Attempt 3:  wait 60s, retry
Attempt 4:  wait 120s, retry
Attempt 5+: wait 300s (5min), retry every 5min until TTL
```

Exponential backoff prevents repeated flooding of a narrow bandwidth path. Cap at 300s to ensure message is still retried within TTL.

**Used by:**
- P3 emergency text (recommended)
- P4 normal text (when user requests "delivery receipt")
- P1 medical emergency (default)
- Important group notifications

**Example scenario:** User sends a check-in message to family. Message is retried until family's device receives it and sends back an ACK. User sees "✓ Delivered" in UI.

### PRIORITY_DELIVERY (Policy Code 2)

Maximum effort delivery. Send via all available transports simultaneously. Battery override permitted.

**Guarantees made:**
- Message is sent via ALL available transports simultaneously (multi-path)
- Battery override: device enters EMERGENCY_OVERRIDE power mode for this message
- Routing engine uses epidemic mode (all neighbors receive copy)
- ACK is required; aggressive retry until ACK or TTL

**Guarantees not made:**
- No guarantee of delivery — if truly isolated, cannot deliver
- No guarantee of delivery time — depends on network connectivity

**Relay behavior:**
- Insert message at HEAD of all transport queues (immediate)
- Activate all available transports (even if power mode normally restricts)
- Forward to ALL neighbors immediately (epidemic)
- Do not limit to one relay path
- Track ACK from recipient; on receipt, stop flooding

**Used by:**
- P0 SOS (automatic — policy set to PRIORITY_DELIVERY for all P0)
- P1 medical emergency (automatic)
- Explicitly by user for critical P3/P4 messages (future: "Send urgently" option)

**Battery cost:** PRIORITY_DELIVERY ignores battery policy. A P0 message sent via PRIORITY_DELIVERY will activate all transports even at 3% battery. This is intentional: a dead phone after sending SOS is acceptable; an unsent SOS is not.

**Example scenario:** User triggers SOS. Device immediately broadcasts via BLE to all nearby neighbors AND sends via Wi-Fi to any reachable node AND sends via LoRa if gateway available. Every relay immediately floods all their neighbors. Aggressive retry continues until recipient (emergency coordinator) ACKs.

### EMERGENCY_BROADCAST (Policy Code 3)

Flood to all reachable nodes. Authority certificate required. No single recipient.

**Guarantees made:**
- Every node receiving the message will relay it to all its neighbors
- The broadcast propagates epidemically through the reachable network
- Authority certificate is verified before relay (unauthorized broadcasts dropped)

**Guarantees not made:**
- No per-recipient ACK (broadcast has no single recipient)
- No delivery confirmation to sender
- Nodes that are unreachable at broadcast time may never receive it (unless later connected)

**Relay behavior:**
- Verify authority certificate chain (see MESSAGE_ENVELOPE.md)
- If valid: immediately forward to ALL neighbors via ALL available transports
- Do not queue — process immediately
- Deduplication prevents re-broadcasting the same message_id

**Who can use this policy:**
- Only nodes with a valid authority certificate (government, NDRF/SDRF, operator)
- Regular nodes attempting EMERGENCY_BROADCAST without valid cert: message dropped at first relay

**Example scenario:** State disaster management authority broadcasts: "Cyclone warning: evacuate coastal zones within 6 hours." Every IRIS node in the reachable network receives and displays this alert. Nodes not currently reachable (isolated communities) receive it when they reconnect or when a vehicle relay carries it.

### MULTICAST_DELIVERY (Policy Code 4)

Send to a defined group. Each group member tracked independently.

**Guarantees made:**
- Message routed toward all group members using best available paths
- Each recipient tracked independently (Group A may receive while Group B has not yet)
- Optional: aggregate delivery report sent to sender

**Guarantees not made:**
- No guarantee all members receive message (isolated members may never receive)
- No real-time delivery status for all members simultaneously

**Relay behavior:**
- Recipient_id is group_id (not a single node)
- Message relayed toward each known group member via normal routing
- Each receiving node decrypts with group key, delivers to group member application
- Optional: each recipient sends ACK back to sender

**Used by:**
- Group chats
- Team coordination during disaster response
- Multi-organization coordination

**Example scenario:** NDRF team group message: "Meeting at Bhuj hospital at 14:00." Each team member's device receives the message independently. Members with direct Internet receive immediately. Members in field (mesh-only) receive when mesh path becomes available.

## Policy Encoding in Envelope

Field 20 in the CBOR envelope (see MESSAGE_ENVELOPE.md field map):
```
delivery_policy: uint
  0 = BEST_EFFORT
  1 = CONFIRMED_DELIVERY
  2 = PRIORITY_DELIVERY
  3 = EMERGENCY_BROADCAST
  4 = MULTICAST_DELIVERY
```

Policy is included in the signed envelope (covered by Ed25519 signature). A relay cannot change the policy without invalidating the signature.

## Policy vs Priority Interaction

Priority determines queue ordering. Policy determines delivery semantics. They are orthogonal.

| Priority | Default Policy | Override Allowed |
|---------|--------------|-----------------|
| P0 SOS | PRIORITY_DELIVERY | No (always maximum effort) |
| P1 Medical | PRIORITY_DELIVERY | No |
| P2 Location | BEST_EFFORT | Yes (can use CONFIRMED_DELIVERY) |
| P3 Emergency text | CONFIRMED_DELIVERY | Yes (can use PRIORITY_DELIVERY) |
| P4 Normal text | BEST_EFFORT | Yes (user can request CONFIRMED_DELIVERY) |
| P5-P7 Bulk | BEST_EFFORT | No (bulk always best-effort) |

Interaction example:
- A P4 CONFIRMED_DELIVERY message gets lower queue priority than P2 BEST_EFFORT
- But P4 message still has delivery confirmation semantics regardless of queue position
- P2 message may be delivered before P4 message
- Both eventually delivered; P4 has confirmation; P2 does not

## Policy Enforcement at Relay Nodes

Relay nodes enforce policies:

| Policy | Relay Queue Priority | Relay Transport Activation | Relay ACK Tracking |
|--------|---------------------|--------------------------|-------------------|
| BEST_EFFORT | Normal (by message priority) | Current transports | No |
| CONFIRMED_DELIVERY | Normal | Current transports | No (recipient tracks) |
| PRIORITY_DELIVERY | Immediate (head of queue) | All transports | Signal to epidemic relay |
| EMERGENCY_BROADCAST | Immediate (preempts all) | All transports | No (broadcast) |
| MULTICAST_DELIVERY | Normal | Current transports | No (recipient tracks) |

Relay nodes do not track delivery for BEST_EFFORT or CONFIRMED_DELIVERY — that is the responsibility of the originating node (which keeps a copy until ACK received or TTL expires).

For PRIORITY_DELIVERY: relay does not track ACK either, but it knows to use epidemic routing and all transports. The originator tracks ACK.

## ACK Routing

For CONFIRMED_DELIVERY and PRIORITY_DELIVERY:

**ACK message format:**
```
ACK Envelope {
    payload_type: ACK (code 9),
    payload: {
        original_message_id: bytes(16),  // Message being acknowledged
        recipient_id: bytes(32),         // Acknowledging node's ID
        timestamp: uint,                 // Time of delivery
        received_hops: uint,             // How many hops the original took
    }
}
```

**ACK routing:**
- ACK recipient: `sender_id` of original message (extracted from original envelope)
- ACK priority: P_CONTROL_ACK (between P3 and P4)
- ACK TTL: 4 hours (shorter than data messages)
- ACK routing: standard routing toward original sender_id
- If reverse path known (via routing table): prefer reverse path (lower latency)
- If no path to original sender: store ACK, forward when path available (ACK also store-carry-forward)

**ACK timeout (originator perspective):**
- 30s: first retry on alternate path
- 60s: second retry
- 120s: third retry
- After 5 retries: log delivery_attempt_exhausted, continue periodic retry every 5min until TTL

**ACK deduplication:**
- Original sender may receive multiple ACKs (from duplicate deliveries or ACK re-transmissions)
- First ACK received: mark delivered, notify application
- Subsequent ACKs for same message_id: ignore silently

## Policy and Battery Mode Interaction

| Battery Mode | BEST_EFFORT | CONFIRMED_DELIVERY | PRIORITY_DELIVERY | EMERGENCY_BROADCAST |
|-------------|------------|-------------------|------------------|-------------------|
| FULL | Full operation | Full operation | Full operation | Full operation |
| BALANCED | Full operation | Full operation | Full operation | Full operation |
| LOW_BATTERY | Continue | Continue | Battery override active | Battery override active |
| CRITICAL | P0-P2 only queued | P0-P2 only | Override all | Override all |
| BATTERY_CRITICAL | P0 only | P0 only | Override all | Override all |

PRIORITY_DELIVERY and EMERGENCY_BROADCAST always override battery policy. The rationale: battery constraints are never more important than life-safety communication.

## Delivery Status Exposed to Application

The application layer observes delivery status updates:

```rust
pub enum DeliveryStatus {
    Queued,             // In local queue, not yet sent
    Sent,               // Transmitted to at least one relay
    InTransit,          // Confirmed at least one relay holds a copy
    Delivered,          // Recipient received (ACK received by originator)
    DeliveryFailed,     // TTL expired without delivery confirmation
    Expired,            // TTL elapsed (message deleted)
}
```

For BEST_EFFORT messages: status progresses from Queued → Sent. Never reaches Delivered (no ACK).
For CONFIRMED_DELIVERY: Queued → Sent → InTransit → Delivered (or DeliveryFailed).
For PRIORITY_DELIVERY: same as CONFIRMED_DELIVERY, with more aggressive intermediate states.

Application subscribed to status via:
```kotlin
iris.getDeliveryStatus(messageId) -> LiveData<DeliveryStatus>  // Android
iris.deliveryStatusPublisher(messageId) -> AnyPublisher<DeliveryStatus, Never>  // iOS
```
