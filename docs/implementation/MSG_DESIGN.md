# Message Engine Design (MSG-001)

**Document ID**: IRIS-MSG-DESIGN-001
**Version**: 1.0
**Node**: MSG-001
**Date**: 2026-08-11

---

## Overview

The Message Engine (Layer 5) manages the complete message lifecycle in IRIS. It integrates the protocol envelope format, the storage layer, and the transport abstraction into a coherent system for sending, receiving, relaying, and delivering messages.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    MessageEngine                             │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────────────────────┐ │
│  │  Inbound          │  │  Outbound                        │ │
│  │  Processor        │  │  Queue                           │ │
│  │                   │  │                                  │ │
│  │  1. Parse CBOR    │  │  1. Priority enqueue             │ │
│  │  2. Validate      │  │  2. P0 preempt                   │ │
│  │  3. Verify sig    │  │  3. TTL check                    │ │
│  │  4. Dedup check   │  │  4. Transport select             │ │
│  │  5. Decrypt       │  │  5. Fragment if needed           │ │
│  │  6. Deliver/Relay │  │  6. Send via TransportManager    │ │
│  └──────────────────┘  └──────────────────────────────────┘ │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────────────────────┐ │
│  │  TTL              │  │  ACK                             │ │
│  │  Enforcer         │  │  Tracker                         │ │
│  │                   │  │                                  │ │
│  │  - Expiry check   │  │  - Pending ACKs map              │ │
│  │  - Skew budget    │  │  - Retry with backoff            │ │
│  │  - GC (60s cycle) │  │  - Timeout per priority          │ │
│  └──────────────────┘  └──────────────────────────────────┘ │
│                                                              │
│  ┌──────────────────┐  ┌──────────────────────────────────┐ │
│  │  Fragment         │  │  Dedup                           │ │
│  │  Reassembler      │  │  Engine                          │ │
│  │                   │  │                                  │ │
│  │  - Buffer frags   │  │  - Bloom filter (100K, 1% FP)    │ │
│  │  - Timeout 30s    │  │  - LRU cache (10K exact)         │ │
│  │  - Reassemble     │  │  - Reset bloom 24h               │ │
│  │  - Deliver        │  │                                  │ │
│  └──────────────────┘  └──────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## Interfaces

### Storage Interface (to STORE-001)
```rust
trait MessageStorage {
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError>;
    async fn load(&self, id: &MessageId) -> Result<Option<Envelope>, StorageError>;
    async fn update_status(&self, id: &MessageId, status: MessageStatus) -> Result<(), StorageError>;
    async fn evict_expired(&self, before: Timestamp) -> Result<usize, StorageError>;
    async fn evict_by_priority(&self, target_bytes: u64) -> Result<usize, StorageError>;
    async fn get_queue(&self, limit: usize) -> Result<Vec<Envelope>, StorageError>;
}
```

### Transport Interface (to TRANSPORT-001)
```rust
// Uses TransportManager's send() and incoming_messages()
async fn send_to_transport(envelope: &Envelope, peer: &PeerId) -> Result<(), TransportError>;
fn incoming_messages() -> Pin<Box<dyn Stream<Item = IncomingMessage>>>;
```

### Routing Interface (to ROUTE-001)
```rust
fn next_hops(&self, destination: &NodeId) -> Vec<(NodeId, TransportId)>;
fn record_contact(&self, peer: &PeerId, transport: TransportId);
```

### Security Interface (to CRYPTO-001)
```rust
fn sign_envelope(&self, envelope: &Envelope) -> Result<Signature, CryptoError>;
fn verify_envelope(&self, envelope: &Envelope) -> Result<bool, CryptoError>;
fn encrypt_payload(&self, plaintext: &[u8], recipient: &PublicKey) -> Result<(Vec<u8>, EncryptionHdr), CryptoError>;
fn decrypt_payload(&self, ciphertext: &[u8], hdr: &EncryptionHdr) -> Result<Vec<u8>, CryptoError>;
```

## Message Lifecycle

```
CREATED → PENDING_SEND → IN_TRANSIT → DELIVERED → ACKNOWLEDGED
                         ↓              ↓
                       EXPIRED       DELIVERY_FAILED
```

### State Transitions

| From | To | Trigger |
|------|----|---------|
| CREATED | PENDING_SEND | Message created, stored to DB |
| PENDING_SEND | IN_TRANSIT | Forwarded to first relay |
| IN_TRANSIT | DELIVERED | Received by final recipient |
| IN_TRANSIT | EXPIRED | TTL elapsed |
| DELIVERED | ACKNOWLEDGED | ACK received from recipient |
| DELIVERED | DELIVERY_FAILED | All paths exhausted |

## Priority Queue

```rust
struct QueuedMessage {
    envelope: Envelope,
    created_at: Timestamp,
    attempts: u32,
    last_attempt: Option<Timestamp>,
    next_retry: Option<Timestamp>,
}

impl Ord for QueuedMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // P0 always first
        self.envelope.priority.cmp(&other.envelope.priority)
            .then_with(|| self.created_at.cmp(&other.created_at))  // FIFO within priority
            .then_with(|| other.delivery_prob.cmp(&self.delivery_prob))  // Higher prob first
    }
}
```

## Retry Strategy

| Priority | Initial Timeout | Backoff | Max Attempts |
|----------|----------------|---------|-------------|
| P0 | 30s | 2× | Unlimited (until TTL) |
| P1 | 60s | 2× | 10 |
| P2 | 120s | 2× | 5 |
| P3 | 300s | 2× | 3 |
| P4-P7 | 600s | 2× | 2 |

## Starvation Prevention

After 60 seconds of continuous P0 transmission:
1. Insert one P1-or-lower message
2. Reset timer
3. Resume P0

This prevents indefinite blocking of lower-priority traffic during sustained emergencies.

## Thread Safety

The MessageEngine uses:
- `Arc<RwLock<Queue>>` for the priority queue
- `Arc<dyn MessageStorage>` for storage (connection pool internally)
- `broadcast::Channel` for internal events
- `tokio::sync::Semaphore` for concurrent send limiting

## Configuration

```rust
pub struct MessageEngineConfig {
    pub max_queue_depth: usize,        // default: 50,000
    pub starvation_threshold_secs: u64, // default: 60
    pub fragment_timeout_secs: u64,    // default: 30
    pub gc_interval_secs: u64,         // default: 60
    pub max_retry_attempts: u8,        // default: 5
    pub clock_skew_budget_secs: u64,   // default: 300
}
```

## Acceptance Criteria

1. Messages are stored with ACID guarantees
2. Priority queue respects P0 preemption
3. TTL enforced at insertion, dequeue, and relay
4. Deduplication via Bloom filter + LRU cache
5. Retry with exponential backoff per priority
6. Fragment reassembly with timeout
7. Starvation prevention active
8. Thread-safe concurrent access
9. Graceful degradation under storage pressure
10. Observable via metrics and structured logging
