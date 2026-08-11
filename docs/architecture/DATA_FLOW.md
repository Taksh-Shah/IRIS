# Data Flow

## Overview
This document traces the complete path of a message from origination through delivery, including all security, routing, and transport operations. Understanding this flow is essential for debugging, performance analysis, and correctness review.

## Message Origination Flow

```
User creates message (app UI or API call)
         │
         ▼
Application API Layer (Layer 7)
  ├─ Validate input (recipient exists, content non-empty)
  ├─ Assign message_id (UUIDv7: timestamp-ordered for efficient indexing)
  ├─ Assign timestamp (current UTC, seconds since epoch)
  ├─ Set priority (P0-P7, from user intent or message type)
  ├─ Set TTL (from priority default or user override)
  └─ Set delivery_policy (BEST_EFFORT or CONFIRMED_DELIVERY)
         │
         ▼
Security Layer (Layer 6)
  ├─ Sign message envelope: signature = Ed25519_sign(sender_privkey, envelope_bytes)
  ├─ If private message:
  │    ├─ Lookup recipient public key from contact store
  │    ├─ Generate ephemeral X25519 key pair
  │    ├─ Compute shared secret: X25519(eph_privkey, recipient_pubkey)
  │    ├─ Derive encryption key: HKDF-SHA256(shared_secret, "iris-message-key")
  │    ├─ Generate nonce (12 random bytes)
  │    └─ Encrypt: ChaCha20-Poly1305(key, nonce, plaintext)
  └─ Assemble final envelope with signature + encryption_hdr
         │
         ▼
Message Engine (Layer 5)
  ├─ Add to outbound queue (priority-ordered binary heap)
  ├─ Persist to message_store (SQLite, synchronous write for P0-P2)
  └─ Notify RoutingEngine: new message ready
         │
         ▼
Routing Engine (Layer 4)
  ├─ Lookup destination in routing table
  ├─ If direct neighbor: select direct delivery
  ├─ If known via PRoPHET: select best next hop by delivery probability
  ├─ If unknown: store for opportunistic forwarding
  └─ Select transport(s) based on message priority and availability
         │
         ▼
Transport Manager (Layer 3)
  ├─ Check selected transport(s) availability
  ├─ For P0: activate ALL available transports (multi-path)
  ├─ Apply battery policy (skip Wi-Fi if BALANCED mode)
  ├─ Fragment message if size > transport MTU
  └─ Pass to Transport Adapter
         │
         ▼
Transport Adapter (Layer 1)
  ├─ Platform-specific send (Android BLE GATT write, iOS CoreBluetooth, etc.)
  ├─ Handle transport-layer ACK (link-layer reliability)
  └─ Report: SENT, TRANSPORT_FAILED, or QUEUED_FOR_RETRY
         │
         ▼
       [WIRE]
```

## Message Relay Flow

```
Raw bytes arrive (Transport Adapter, any transport)
         │
         ▼
Transport Adapter (Layer 1)
  ├─ Receive raw bytes
  ├─ Report receive event to Transport Manager
  └─ Pass bytes up
         │
         ▼
Transport Manager (Layer 3)
  ├─ Reassemble fragments (if fragmented: buffer until all fragments received)
  │    ├─ Fragment timeout: 30s (drop incomplete messages after timeout)
  │    └─ Re-request missing fragments if last received > 10s ago
  └─ Pass complete message bytes to Security Layer
         │
         ▼
Security Layer (Layer 6) — Signature Verification
  ├─ Parse CBOR envelope (fail-safe: if parse fails → DROP + log)
  ├─ Lookup sender public key (from contact store or embedded in envelope for unknown senders)
  ├─ Verify Ed25519 signature
  │    ├─ Valid: continue processing
  │    └─ Invalid: DROP immediately, log SECURITY_VIOLATION event
  │               (never relay unverified messages)
         │
         ▼
Message Engine (Layer 5) — Deduplication Check
  ├─ Check Bloom filter: has message_id been seen?
  │    ├─ Yes (Bloom says yes):
  │    │    ├─ Check exact cache (resolve Bloom false positives)
  │    │    ├─ Confirmed seen: DROP silently
  │    │    └─ Bloom false positive: continue processing
  │    └─ No: continue processing
  ├─ Add message_id to Bloom filter + exact cache
         │
         ▼
Message Engine (Layer 5) — TTL and Hop Check
  ├─ Check TTL: is current_time < expires_at?
  │    └─ Expired: DROP, log TTL_EXPIRED
  ├─ Decrement hop_count
  │    └─ hop_count == 0: DROP, log HOP_LIMIT_REACHED
         │
         ▼
Routing Engine (Layer 4) — Recipient Check
  ├─ Is local_node_id == recipient_id?
  │    └─ Yes: deliver to application (see Delivery Flow below)
  ├─ Is this a group message and local node is member?
  │    └─ Yes: deliver to application AND continue relay
  ├─ Is this a broadcast (BROADCAST_ALL or EMERGENCY_BROADCAST)?
  │    └─ Yes: deliver to application AND relay to all neighbors
  └─ Not for me: continue to relay logic
         │
         ▼
Routing Engine (Layer 4) — Next Hop Selection
  ├─ Select next hop(s) from routing table
  ├─ If no next hop known: store in message_store (store-carry-forward)
  └─ Proceed to Transport Manager for forwarding
         │
         ▼
[Same as Origination Flow from Transport Manager onward]
```

## Message Delivery Flow (Final Recipient)

```
Message reaches final recipient (identified in Routing Engine)
         │
         ▼
Security Layer (Layer 6) — Decryption (if encrypted)
  ├─ Extract encryption_hdr (ephemeral_pubkey, nonce)
  ├─ Compute shared secret: X25519(local_privkey, ephemeral_pubkey)
  ├─ Derive key: HKDF-SHA256(shared_secret, "iris-message-key")
  ├─ Decrypt: ChaCha20-Poly1305(key, nonce, ciphertext)
  ├─ Authentication tag verification: if FAIL → DROP, log DECRYPTION_FAILED
  └─ Plaintext message content ready
         │
         ▼
Application API Layer (Layer 7)
  ├─ Parse message content (text, location, image, etc.)
  ├─ Store in received_messages database
  ├─ Notify application via callback (on_message_received)
  ├─ Display to user (push notification, in-app alert)
  └─ If delivery_policy == CONFIRMED_DELIVERY: generate ACK
         │
         ▼
[If ACK required: proceed to ACK Flow]
```

## Emergency Message Override Flow

P0 SOS messages receive special handling at every layer:

```
P0 SOS message received OR originated
         │
         ▼
Message Engine: Priority Override
  ├─ Bypass normal queue: insert at head of all queues
  ├─ Set queue_priority = ABSOLUTE_MAXIMUM
  └─ Notify PowerManager: enter EMERGENCY_OVERRIDE mode
         │
         ▼
PowerManager: Emergency Mode
  ├─ Set battery_override = true (ignore battery level)
  ├─ Activate ALL transport adapters (even if off in current power mode)
  ├─ Extend BLE scan to maximum interval
  └─ Enable Wi-Fi Direct if available
         │
         ▼
Transport Manager: Multi-Path Forwarding
  ├─ Send via ALL active transports simultaneously
  ├─ Do not wait for ACK on one transport before sending on another
  ├─ Track which transports successfully sent
  └─ Continue retrying on failed transports
         │
         ▼
Routing Engine: Epidemic Routing
  ├─ Forward to ALL known neighbors (not just best next hop)
  ├─ Set relay_aggressiveness = MAXIMUM
  └─ Continue forwarding even if delivery probability is low
         │
         ▼
ACK Tracking
  ├─ P0 message held in memory (not only on disk) until ACK received
  ├─ ACK timeout: 30s → retry via alternate path
  ├─ ACK timeout after 10 retries: alert user, continue storing
  └─ On ACK received: notify user, reduce routing aggressiveness
```

## ACK Flow

```
Recipient receives message with delivery_policy == CONFIRMED_DELIVERY
         │
         ▼
Generate ACK envelope:
  ├─ ack_id: UUIDv7 (new)
  ├─ original_message_id: message_id being acknowledged
  ├─ recipient_id: local node_id (I am acknowledging)
  ├─ timestamp: now
  ├─ priority: P_CONTROL (control plane priority)
  └─ signature: Ed25519_sign(privkey, ack_fields)
         │
         ▼
Route ACK back toward sender:
  ├─ Prefer: reverse path (if relay chain is known)
  ├─ Fallback: standard routing toward sender_id
  ├─ Fallback 2: epidemic (if sender location unknown)
  └─ ACK TTL: 4 hours (shorter than data message; ACK is time-sensitive)
         │
         ▼
Sender receives ACK
  ├─ Verify ACK signature
  ├─ Match original_message_id to pending message
  ├─ Remove message from pending_delivery_queue
  ├─ Update delivery_status in message_store
  └─ Notify application: DELIVERED event
```

## Failure Paths and Recovery

| Failure Point | Detection | Behavior | Recovery Mechanism |
|--------------|----------|---------|-------------------|
| No transport available | Transport adapter returns UNAVAILABLE | Store message in message_store | Transport restoration → dequeue and forward |
| Signature invalid | Signature verification fails | Drop immediately, log SECURITY_VIOLATION | None — security boundary |
| Decryption failed | Auth tag mismatch | Drop, log DECRYPTION_FAILED | None — key mismatch or tampering |
| Recipient unreachable | No route in routing table | Store-carry-forward | New contact → re-evaluate routing |
| Message store full | SQLite returns SQLITE_FULL | Evict P7→P5 messages by priority then TTL | User clears storage OR P7-P5 expire |
| Battery critical | OS battery event | Enter CRITICAL mode, queue P0-P1 only | Battery charges → exit CRITICAL mode |
| Network partition | PartitionDetector fires | Increase epidemic aggressiveness | Bridge node appears → sync and forward |
| Fragment reassembly timeout | Timer fires after 30s | Drop incomplete message, log FRAGMENT_TIMEOUT | Sender retransmits (if CONFIRMED_DELIVERY) |
| ACK timeout | ACK not received after 30s | Retry via alternate path | ACK received → deliver confirmation |
| Clock skew | Skew > 30 minutes on contact | Conservative TTL enforcement | GPS sync or Internet NTP restores |
| Hop limit exceeded | hop_count reaches max_hops | Drop, log HOP_LIMIT_REACHED | Message is re-originated if needed |

## Data Flow Invariants

These invariants must hold at every step. Violation is a bug:
1. A message with invalid signature must never be relayed or delivered
2. A message_id seen before must never be stored again (Bloom filter may miss; exact cache must not)
3. An expired message (TTL == 0) must never be forwarded
4. A P0 message must never be dropped due to storage pressure (only TTL expiry)
5. Payload content is never visible to relay nodes (E2EE enforced)
6. hop_count is never decremented below 0 or above max_hops

## Throughput and Latency Estimates

These are measured estimates, not theoretical maximums. See BENCH-0001 through BENCH-0010 for methodology.

| Path | Transport | Throughput | Latency (first byte) | Notes |
|------|-----------|----------|---------------------|-------|
| P0 SOS | BLE | N/A (255B msg) | < 500ms | Direct neighbor |
| P0 SOS | LoRa | N/A (255B msg) | 1-3s | LoRa airtime for 255B |
| P4 Text | BLE | ~50KB/s | < 200ms | GATT MTU 512, multiple fragments |
| P4 Text | Wi-Fi Direct | ~2MB/s | < 100ms | Wi-Fi Direct practical throughput |
| P5 Image | Wi-Fi Direct | ~2MB/s | < 100ms | 1MB image: ~500ms |
| Any | Internet | Depends on ISP | 50-200ms | When gateway available |
