# IRIS DTN Routing

**Document ID:** IRIS-ARCH-ROUTE-003  
**Version:** 1.0  
**Status:** Active  

---

## 1. Background: Delay-Tolerant Networking

Delay-Tolerant Networking (DTN) addresses networks where end-to-end connectivity cannot be assumed. The IETF Bundle Protocol v7 (BP7), specified in RFC 9171, defines the canonical DTN bundle structure, custody transfer model, and extension blocks. IRIS is BP7-inspired but not strictly compliant: it retains the DTN principles while making opinionated choices for the disaster response use case.

This document describes the DTN architecture of IRIS, how it relates to RFC 9171, and the implementation of store-carry-forward, contact prediction, and inter-bundle scheduling.

---

## 2. BP7 Concepts in IRIS

### 2.1 Alignment with RFC 9171

| RFC 9171 Concept | IRIS Implementation | Deviation |
|-----------------|--------------------|-----------| 
| Bundle | `iris::Bundle` (CBOR-encoded) | IRIS adds priority field and IRIS-specific extension blocks |
| Primary Block | `BundlePrimaryBlock` | Uses BLAKE3 NodeId instead of endpoint IDs |
| Payload Block | `BundlePayload` | ChaCha20-Poly1305 encrypted; same block type 1 |
| Bundle ID | Source NodeId + timestamp + sequence | Same conceptual space |
| Lifetime/TTL | Per-priority default TTL | IRIS sets defaults; originator can shorten, not extend |
| Custody Transfer | `CustodyAcceptance` extension block | Simplified: single custodian, no custody signals |
| Endpoint ID | `NodeId` (32-byte BLAKE3 hash) | Not URI-based; binary for efficiency |
| Status Reports | `DeliveryReceipt` bundle | P4 priority, Ed25519-signed |
| Extension Blocks | Mandatory: routing_metadata, emergency_priority | Additional IRIS-defined blocks |

### 2.2 IRIS-Specific Extension Blocks

```
Block Type 192: IRIS Priority Block
  priority:     uint8   (0-7, P0=0)
  ttl_seconds:  uint32
  retry_count:  uint8

Block Type 193: IRIS Routing Metadata Block
  spray_copies: uint8   (L value for Spray-and-Wait)
  prophet_forwarded_by: [NodeId]  (up to 5, for loop prevention)
  contact_time: uint64  (unix timestamp of first forwarding)

Block Type 194: IRIS Emergency Authority Block
  authority_cert_chain: bytes  (CBOR-encoded Ed25519 cert chain)
  authority_signature: bytes   (over primary + payload blocks)
  authority_type: uint8        (NDRF=0, RedCross=1, SDRF=2, ...)
```

### 2.3 What IRIS Does Not Implement from BP7

- **Fragmentation/Reassembly (Section 5.8 of RFC 9171):** IRIS delegates fragmentation to individual transport adapters. The bundle layer always works with complete bundles.
- **Previous-Hop Insertion Block:** Loop prevention uses the `prophet_forwarded_by` list in the routing metadata block.
- **Age Block:** IRIS uses creation timestamp + TTL; the Age block's elapsed-time tracking is not implemented.
- **Bundle Protocol Agent queuing:** IRIS has its own priority queue implementation that supersedes the generic BPA model.

---

## 3. Store-Carry-Forward State Machine

Each bundle in IRIS storage exists in one of the following states:

```
                    ┌─────────────┐
                    │   RECEIVED  │ ← from transport or locally created
                    └──────┬──────┘
                           │ validate crypto, check TTL
                           ▼
                    ┌─────────────┐
                    │  VALIDATED  │
                    └──────┬──────┘
                           │ routing decision
                   ┌───────┴──────────┐
                   ▼                  ▼
           ┌─────────────┐    ┌─────────────┐
           │   PENDING   │    │  FORWARDING │
           │  (no path)  │    │  (path found)│
           └──────┬──────┘    └──────┬──────┘
                  │                  │ transport ACK
                  │                  ▼
                  │           ┌─────────────┐
                  │           │  DELIVERED  │
                  │           └─────────────┘
                  │ contact opportunity
                  └──────────► FORWARDING
                           
           ┌─────────────┐
           │   EXPIRED   │ ← TTL exceeded in any state
           └─────────────┘
           
           ┌─────────────┐
           │   EVICTED   │ ← storage pressure, P3+ only
           └─────────────┘
```

### 3.1 State Transitions

**RECEIVED → VALIDATED:**
- Verify Ed25519 signature on primary block
- Decrypt payload with ChaCha20-Poly1305
- Check bundle ID not in deduplication Bloom filter
- Check TTL has not expired
- Transition fails → bundle silently dropped (malformed) or logged (expired)

**VALIDATED → PENDING | FORWARDING:**
- Query routing engine for next-hop candidates
- If candidates available: transition to FORWARDING, initiate transport send
- If no candidates: transition to PENDING, store to disk

**PENDING → FORWARDING:**
- On contact opportunity event (peer discovered via BLE scan, Wi-Fi probe, etc.)
- Routing engine re-evaluates pending bundles for each new contact
- High-priority bundles (P0–P2) are re-evaluated immediately; P3–P7 batched every 30 seconds

**FORWARDING → DELIVERED:**
- Transport layer ACK received (transport-specific delivery confirmation)
- For multi-hop delivery: intermediate ACK is "transport received," not "destination received"
- Final delivery ACK comes as a `DeliveryReceipt` bundle from the destination node

**Any state → EXPIRED:**
- Background timer checks bundle TTL every 60 seconds
- On expiry: delete from disk store, emit expiry metric, no retransmission

### 3.2 Rust State Machine Implementation

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BundleState {
    Received { arrival_time: SystemTime },
    Validated { validated_time: SystemTime },
    Pending { stored_time: SystemTime, attempts: u8 },
    Forwarding { path: TransportPath, started: SystemTime },
    Delivered { delivery_time: SystemTime },
    Expired { expiry_time: SystemTime },
    Evicted { eviction_time: SystemTime, reason: EvictionReason },
}

pub struct StoredBundle {
    pub bundle: Bundle,
    pub state: BundleState,
    pub copy_count: u8,  // For Spray-and-Wait: remaining copies to forward
}
```

---

## 4. Contact Opportunity Prediction

IRIS uses historical contact patterns to predict future contact opportunities, enabling proactive scheduling of bundle forwarding.

### 4.1 Contact History

The contact history table is maintained per peer:

```rust
pub struct ContactHistory {
    pub peer_id: NodeId,
    pub contacts: VecDeque<ContactRecord>,  // max 100 records, LRU
}

pub struct ContactRecord {
    pub start_time: SystemTime,
    pub duration: Duration,
    pub transport: TransportType,
    pub bundles_exchanged: u32,
    pub throughput_bps: f64,
}
```

### 4.2 Inter-Contact Time Distribution

For each peer, IRIS fits the inter-contact time (ICT) distribution using a simple exponential model:

```
λ = 1 / mean_inter_contact_time
P(contact within t) = 1 - e^(-λt)
```

For peers with < 5 contact history records, the network-wide average ICT is used as a fallback.

### 4.3 Predicted Contact Windows

Where GPS or mobility data is available (e.g., evacuation route planning in IRIS Command), predicted contact windows can be injected into the contact prediction engine:

```rust
pub struct PredictedContact {
    pub peer_id: NodeId,
    pub expected_start: SystemTime,
    pub expected_duration: Duration,
    pub confidence: f32,  // 0.0-1.0
    pub predicted_throughput_bps: f64,
}
```

Predicted contacts are used to defer bundle transmission: if peer X is not currently reachable but is predicted to arrive in 15 minutes with high confidence, a P3 bundle may be held rather than forwarded to a lower-quality intermediate.

---

## 5. Inter-Bundle Scheduling

When a contact opportunity exists and multiple bundles are pending, the scheduling algorithm determines transmission order.

### 5.1 Earliest Deadline First (EDF) vs Priority-Based

IRIS uses a **hybrid EDF + priority** scheduler. Pure EDF optimizes for delivery before TTL expiry, but ignores the life-or-death significance difference between a P0 SOS and a P3 text. Pure priority ignores TTL urgency.

The combined urgency score:

```
urgency(bundle) = priority_weight(bundle.priority) × ttl_urgency(bundle)

priority_weight:
  P0 → 1000
  P1 → 100
  P2 → 10
  P3 → 5
  P4 → 2
  P5 → 1
  P6 → 0.5
  P7 → 0.2

ttl_urgency(bundle):
  remaining_ttl = bundle.expiry - now
  total_ttl = bundle.creation + bundle.ttl - bundle.creation  // = bundle.ttl
  fraction_remaining = remaining_ttl / total_ttl
  
  if fraction_remaining < 0.1:  // less than 10% TTL remaining
      return 10.0  // boost: deadline imminent
  elif fraction_remaining < 0.25:
      return 2.0
  else:
      return 1.0
```

This ensures P0 always has highest base urgency, but a P2 bundle with < 10% TTL remaining outranks a P1 bundle with > 90% TTL remaining.

### 5.2 Contact Bandwidth Allocation

When a contact window has limited bandwidth (especially LoRa, BLE), the scheduler allocates bandwidth across priorities:

```
P0: 50% of contact window
P1: 25%
P2: 15%
P3-P7: 10% shared
```

If no P0 bundles exist, bandwidth is redistributed downward proportionally. P0 allocation is never given to lower priorities even if P0 queue is empty — that unused bandwidth is used for the next-ranked bundle.

### 5.3 Contact Window Estimation

Before scheduling, the routing engine estimates the available contact window:

```rust
fn estimate_contact_window(peer: &PeerState, transport: TransportType) -> ContactWindow {
    let history = peer.contact_history.recent_contacts(5);
    let avg_duration = history.iter().map(|c| c.duration).sum::<Duration>() / 5;
    let throughput = history.iter().map(|c| c.throughput_bps).sum::<f64>() / 5.0;
    
    ContactWindow {
        estimated_duration: avg_duration,
        estimated_bytes: (avg_duration.as_secs_f64() * throughput) as usize,
    }
}
```

Bundles are scheduled to fit within the estimated window, with the highest urgency bundles scheduled first.

---

## 6. Comparison with RFC 9171

| Aspect | RFC 9171 (BP7) | IRIS |
|--------|---------------|------|
| Endpoint addressing | URI-based (dtn: or ipn: scheme) | Binary 32-byte NodeId |
| Security | BPSec (RFC 9172), separate spec | Integrated: Ed25519+ChaCha20 in every bundle |
| Custody | Bidirectional custody signals | Simplified: accept-only, no custody ACK bundle |
| Fragmentation | Specified in BP7 | Delegated to transport layer |
| Admin records | Status reports, custody signals | Only DeliveryReceipt implemented |
| Extension mechanism | Block type registry | IRIS-private range 192-255 |
| Routing | Not specified in BP7 | PRoPHET, S&W, Epidemic, Direct in IRIS |
| Convergence layer | Not specified | BLE, Wi-Fi Direct, LoRa, etc. adapters |

IRIS chose to deviate from full BP7 compliance to optimize for:
- **Binary efficiency:** URI endpoints are verbose; 32-byte NodeIds are compact for LoRa
- **Integrated security:** BPSec adds complexity; IRIS integrates crypto at every layer
- **Operational simplicity:** Full custody transfer is complex to implement correctly in a mobile app context

---

## 7. Store-Carry-Forward Persistence

### 7.1 On-Disk Format

Bundles are persisted to a SQLite database at `{data_dir}/iris_bundle_store.db`:

```sql
CREATE TABLE bundles (
    bundle_id     BLOB PRIMARY KEY,  -- 32-byte BLAKE3 id
    priority      INTEGER NOT NULL,
    state         TEXT NOT NULL,
    created_at    INTEGER NOT NULL,  -- unix seconds
    expires_at    INTEGER NOT NULL,
    copy_count    INTEGER NOT NULL DEFAULT 1,
    raw_bundle    BLOB NOT NULL,     -- CBOR-encoded full bundle
    stored_at     INTEGER NOT NULL
);

CREATE INDEX idx_priority_expires ON bundles(priority, expires_at);
CREATE INDEX idx_state ON bundles(state);
```

### 7.2 Recovery on Restart

On IRIS startup, all bundles with state PENDING or FORWARDING are loaded into the in-memory routing queue. FORWARDING bundles are reset to PENDING (the in-flight transmission is assumed lost). Expired bundles are purged during startup.
