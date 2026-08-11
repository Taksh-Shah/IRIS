# IRIS Priority Delivery

**Document ID:** IRIS-ARCH-EMRG-001  
**Version:** 1.0  
**Status:** Active  

---

## 1. Priority System Overview

IRIS uses an 8-level priority system (P0–P7) to manage the delivery of messages across constrained, intermittent networks. Priority determines queue position, transport allocation, storage protection, and retry behavior.

| Priority | Class | Use Case | TTL Default |
|----------|-------|----------|------------|
| P0 | SOS | Immediate distress signal | 72 hours |
| P1 | Medical | Medical resource requests, patient status | 24 hours |
| P2 | Location | GPS location of person or resource | 30 minutes |
| P3 | Emergency Text | Coordination messages, evacuation instructions | 6 hours |
| P4 | Normal Text | General communication, ACKs | 2 hours |
| P5 | Image | Damage assessment photos, maps | 2 hours |
| P6 | Voice | Audio messages | 1 hour |
| P7 | Video | Video clips | 30 minutes |

---

## 2. P0 SOS: Absolute Priority

### 2.1 P0 Queue Discipline

P0 SOS messages operate under strict FIFO within the P0 class, with no pre-emption within class (all P0 messages are equally urgent). Across priority classes, P0 has absolute pre-emption:

```
Queue discipline:
  1. All P0 bundles transmit before any P1 bundle
  2. Within P0: FIFO (oldest P0 transmits first)
  3. P0 bundles are NEVER evicted from storage regardless of pressure
  4. P0 bundles have UNLIMITED retries (retry until TTL expiry)
  5. P0 is the ONLY priority that transmits at battery < 5%
```

The head-of-line priority enforcement is implemented at the `OutboundQueue`:

```rust
pub struct OutboundQueue {
    p0: VecDeque<Bundle>,
    p1: VecDeque<Bundle>,
    p2: VecDeque<Bundle>,
    p3_p7: BinaryHeap<PrioritizedBundle>,  // urgency-scored heap for P3-P7
}

impl OutboundQueue {
    pub fn next_bundle(&self, battery_level: u8) -> Option<Bundle> {
        // P0 always first, regardless of battery
        if let Some(bundle) = self.p0.front() {
            return Some(bundle.clone());
        }
        
        // Battery-gated priorities
        if battery_level < 5 {
            return None;  // Only P0 below 5%
        }
        if battery_level < 10 {
            return self.p1.front().cloned();  // P0+P1 below 10%
        }
        if battery_level < 20 {
            return self.p1.front()
                .or_else(|| self.p2.front())
                .cloned();  // P0+P1+P2 below 20%
        }
        
        // Normal operation: priority order
        self.p1.front()
            .or_else(|| self.p2.front())
            .or_else(|| self.p3_p7.peek().map(|pb| &pb.bundle))
            .cloned()
    }
}
```

### 2.2 P0 Never Evicted

Storage pressure eviction is priority-order, last-in-first-out. P0 bundles are in an eviction-protected zone:

```rust
fn evict_to_meet_quota(
    store: &mut BundleStore,
    required_bytes: usize,
) -> Result<usize, StoreError> {
    let mut freed = 0;
    
    // Evict P7 first, then P6, P5... down to P3
    for priority in [Priority::P7, Priority::P6, Priority::P5, Priority::P4, Priority::P3] {
        if freed >= required_bytes { break; }
        freed += evict_priority_bundles(store, priority, required_bytes - freed);
    }
    
    // P0, P1, P2 are NEVER evicted under storage pressure
    // If P3-P7 eviction is insufficient, return error
    if freed < required_bytes {
        Err(StoreError::InsufficientSpace {
            available: freed,
            required: required_bytes,
            note: "P0-P2 bundles protected from eviction",
        })
    } else {
        Ok(freed)
    }
}
```

### 2.3 P0 Unlimited Retries

Unlike P3–P7 which have bounded retry counts (see Section 2.1 priority table in `ROUTING_REQUIREMENTS.md`), P0 bundles retry until TTL expiry (72 hours). The retry backoff applies to avoid network thrashing, but does not cap at a maximum retry count:

```rust
fn p0_retry_delay(attempt: u32) -> Duration {
    // Exponential backoff: 5s, 10s, 20s, 40s, 80s, cap at 5 minutes
    let base = Duration::from_secs(5);
    let backoff = base * (2u32.pow(attempt.min(6)));
    backoff.min(Duration::from_secs(300))
}
```

---

## 3. Multi-Transport Simultaneous Broadcast for P0

When a P0 SOS is generated, IRIS does not select a single best transport. It broadcasts simultaneously on ALL available transports.

### 3.1 P0 Broadcast Trigger

```rust
pub async fn send_sos(
    &self,
    sos_content: SosContent,
) -> Result<SosBroadcastResult, SosError> {
    let bundle = self.build_p0_bundle(sos_content)?;
    
    // Simultaneously transmit on ALL available transports
    let transports: Vec<Arc<dyn Transport>> = self.transport_manager
        .all_active_transports()
        .collect();
    
    let broadcast_futures: Vec<_> = transports.iter()
        .map(|t| t.broadcast(bundle.clone()))
        .collect();
    
    // Attempt all transports concurrently; succeed if any one delivers
    let results = join_all(broadcast_futures).await;
    
    // Log results for each transport
    self.metrics.record_p0_broadcast(&results);
    
    // Also queue in store for DTN carry-forward if no immediate delivery
    self.bundle_store.store(bundle.clone(), EvictionPolicy::NeverEvict).await?;
    
    let any_success = results.iter().any(|r| r.is_ok());
    Ok(SosBroadcastResult {
        bundle_id: bundle.id(),
        transports_attempted: results.len(),
        transports_succeeded: results.iter().filter(|r| r.is_ok()).count(),
        immediate_delivery: any_success,
    })
}
```

### 3.2 Transport Availability at Low Battery

Even at battery < 5%, P0 broadcast includes all transports (not just the most efficient):

```rust
fn select_transports_for_p0(
    available: &[Arc<dyn Transport>],
    battery_level: u8,
) -> Vec<Arc<dyn Transport>> {
    // P0 uses ALL transports regardless of battery level
    // The cost of not delivering an SOS outweighs the battery cost
    available.to_vec()
}
```

This is a deliberate policy choice. At 5% battery (~3.5 hours remaining on idle), a P0 SOS transmission consumes approximately 0.1–0.5% battery per transport. Transmitting on 4 transports simultaneously costs ~2% battery but maximizes the probability of at least one delivery path succeeding.

---

## 4. P0 vs P1 Arbitration

When both P0 and P1 messages are simultaneously queued for the same transport:

### 4.1 Single Transport Arbitration

P0 always transmits first. No exception. If a P1 medical alert is mid-transmission when a P0 SOS arrives, the P1 transmission is **not interrupted** (mid-transmission interruption would corrupt the P1 bundle). The P0 bundle is queued as next in the outbound queue.

```
P1 transmission in progress:
  [P1 bundle ─────────────────▶] (transmitting)
  [P0 arrives]
  
  Result: P1 completes, then P0 transmits immediately.
  P0 does NOT pre-empt in-progress transmissions.
```

**Rationale:** Interrupting a transmission leaves the receiver with a corrupt partial bundle. The delay of completing a P1 transmission (typically 1–50ms for text, up to 10 seconds for larger P1 bundles) is acceptable given that P0 SOS already triggers multi-transport broadcast on other paths.

### 4.2 Multi-Transport Arbitration

When multiple transports are available:
- P0 occupies the highest-reliability transport immediately
- P1 transmits concurrently on remaining transport(s)
- P0 multipath occupies all transports (P1 waits its turn)

```
BLE available, Wi-Fi Direct available, P0 and P1 both queued:

With P0 L=5 (spray copies) pending:
  BLE:           P0 copy 1
  Wi-Fi Direct:  P0 copy 2
  
P1 waits until P0 spray copies are dispatched, then transmits.
```

### 4.3 Timeout and Re-evaluation

If a P1 bundle has been waiting in queue behind P0 for more than 60 seconds and its TTL is approaching (< 10% TTL remaining), a warning is surfaced to the application layer. The routing engine does not override the P0 priority, but the application can inform the user that the P1 message may be delayed.

---

## 5. Battery-Level Priority Interaction

The battery-priority matrix defines which priorities are active at each battery level:

| Battery Level | P0 | P1 | P2 | P3 | P4 | P5 | P6 | P7 |
|-------------|----|----|----|----|----|----|----|----|
| > 50% | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| 20–50% | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ |
| 10–20% | ✓ | ✓ | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ |
| 5–10% | ✓ | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |
| < 5% | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ | ✗ |

**Note on P2 at < 5% battery:** Location (P2) is disabled below 5% to preserve battery for SOS. Rescuers receiving a P0 SOS know the last known location from the SOS bundle content; P2 separate location updates are not critical once SOS is sent.

### 5.1 Battery Check Implementation

```rust
impl OutboundQueue {
    pub fn should_transmit(&self, bundle: &Bundle, battery: &BatteryGuard) -> bool {
        let level = battery.level_percent();
        match bundle.priority() {
            Priority::P0 => true,  // Always
            Priority::P1 => level >= 5,
            Priority::P2 => level >= 10,
            Priority::P3 | Priority::P4 => level >= 20,
            Priority::P5 | Priority::P6 | Priority::P7 => level >= 50,
        }
    }
}
```

---

## 6. Delivery Receipt and Confirmation

### 6.1 P0 Delivery Receipt

When a P0 SOS is received by the destination node (or a gateway node that can relay it to emergency services), a `DeliveryReceipt` bundle is generated at P4 priority and sent back to the source:

```
DeliveryReceipt = {
  1: bstr,    ; original_bundle_id: the P0 bundle ID
  2: uint,    ; received_at: unix timestamp
  3: bstr,    ; receiver_node_id: who received it
  4: bstr,    ; receiver_signature: Ed25519 signature
  ? 5: text,  ; relay_note: "Forwarded to NDRF control" etc.
}
```

The originating node receives the delivery receipt and:
1. Updates the UI: "SOS Confirmed Received [time ago] by [receiver_id]"
2. Stops epidemic re-routing for this bundle (delivery confirmed)
3. Retains the bundle in store (in case delivery receipt itself was wrong)

### 6.2 No Receipt After 30 Seconds

If no delivery receipt is received within 30 seconds of P0 SOS transmission, the routing engine escalates to Epidemic routing:

```rust
async fn p0_delivery_monitor(
    bundle_id: BundleId,
    store: Arc<BundleStore>,
    routing: Arc<RoutingEngine>,
) {
    tokio::time::sleep(Duration::from_secs(30)).await;
    
    if !store.has_receipt_for(&bundle_id).await {
        // Escalate to epidemic
        routing.escalate_to_epidemic(&bundle_id).await;
        
        // Notify UI
        ui_channel.send(UiEvent::SosUnconfirmed { bundle_id }).await;
    }
}
```

---

## 7. P0 SOS Content Structure

```
SosContent = {
  1: text,           ; message: max 256 chars, required
  ? 2: Location,     ; location: GPS if available (optional)
  ? 3: SosType,      ; type: TRAPPED=0, MEDICAL=1, FIRE=2, FLOOD=3, OTHER=4
  ? 4: uint,         ; persons_count: estimated people needing help (optional)
  ? 5: text,         ; contact_callback: phone/other contact if available
}

Location = {
  1: float,  ; latitude
  2: float,  ; longitude
  ? 3: float ; accuracy_meters
}
```

The message field is required. Location is strongly encouraged — the UI prompts for location if GPS is available and not yet attached. SOS without location can still be relayed and aggregated by authorities who may have context about the sender's last known position.
