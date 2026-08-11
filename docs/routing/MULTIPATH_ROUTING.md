# IRIS Multipath Routing

**Document ID:** IRIS-ARCH-ROUTE-002  
**Version:** 1.0  
**Status:** Active  

---

## 1. Overview

Multipath routing in IRIS enables simultaneous bundle transmission across multiple transports when two or more transports are concurrently available to the same peer or toward the same destination. The primary goals are:

1. **Redundancy:** P0 and P1 messages arrive even if one transport fails mid-transmission.
2. **Throughput:** For large bundles (P5 images, P6 voice), parallel paths can increase effective throughput.
3. **Diversity:** Independent path failure modes improve reliability in RF-contested environments.

Multipath is **not** used unconditionally. It is gated by priority, battery level, transport independence, and congestion state.

---

## 2. When Multipath Is Used

### 2.1 Eligibility Criteria

A bundle is eligible for multipath transmission if ALL of the following hold:

| Criterion | Condition |
|-----------|-----------|
| Priority | P0, P1, or P2 only |
| Battery level | ≥ 20% (IRIS battery_guard check) |
| Available transports | ≥ 2 transports connected to same peer or toward destination |
| Path independence | Multipath independence score ≥ 0.5 (see Section 3) |
| Not in crowd mode | Crowd mode disables multipath to reduce channel saturation |

P3–P7 bundles are always sent single-path. If battery drops below 20% during a multipath transmission in progress, the secondary paths are not abandoned (mid-transmission interruption would corrupt the bundle), but no new multipath sessions are initiated.

### 2.2 Transport Combinations

Common valid multipath combinations in IRIS:

| Primary | Secondary | Notes |
|---------|-----------|-------|
| BLE 5.x | Wi-Fi Direct | Most common; high independence |
| BLE 5.x | Wi-Fi Aware | Available when both initialized |
| Wi-Fi Direct | LoRa | LoRa duty cycle limits secondary use |
| Wi-Fi Direct | Cellular | Good independence; cellular cost applies |
| BLE 5.x | Satellite | High independence; satellite latency high |

LoRa as secondary transport is used only for P0 when all other paths fail, because LoRa duty cycle is too constrained for routine multipath.

---

## 3. Path Scoring

Each available transport-peer pair receives a multipath score. The routing engine selects the top-N paths by score, where N is determined by priority (P0: up to 3 paths, P1: up to 2 paths, P2: up to 2 paths).

### 3.1 Score Formula

```
path_score(t, p) = reliability(t) × bandwidth(t) × independence(t, selected_paths)
```

Where:

- **reliability(t):** Exponentially weighted moving average of delivery success rate for transport `t` to this peer. Range [0.0, 1.0].
- **bandwidth(t):** Normalized effective bandwidth. BLE=0.3, Wi-Fi Direct=1.0, Wi-Fi Aware=0.8, LoRa=0.05, Satellite=0.2, Cellular=0.7.
- **independence(t, selected_paths):** Measures physical and logical diversity from already-selected paths. Range [0.0, 1.0].

### 3.2 Independence Scoring

Independence is the key differentiator. Two paths are considered NOT independent if they share:

- The same radio hardware (e.g., BLE and Wi-Fi share the same 2.4 GHz chip on many devices — independence = 0.3)
- The same frequency band (2.4 GHz BLE + 2.4 GHz Wi-Fi, independence = 0.4)
- The same routing intermediate node

Independence matrix (approximate):

| | BLE | Wi-Fi Direct | Wi-Fi Aware | LoRa | Satellite | Cellular |
|---|---|---|---|---|---|---|
| BLE | — | 0.4 | 0.4 | 0.9 | 1.0 | 0.9 |
| Wi-Fi Direct | 0.4 | — | 0.5 | 0.9 | 1.0 | 0.9 |
| Wi-Fi Aware | 0.4 | 0.5 | — | 0.9 | 1.0 | 0.9 |
| LoRa | 0.9 | 0.9 | 0.9 | — | 1.0 | 0.9 |
| Satellite | 1.0 | 1.0 | 1.0 | 1.0 | — | 0.8 |
| Cellular | 0.9 | 0.9 | 0.9 | 0.9 | 0.8 | — |

A combination with independence < 0.5 is rejected for multipath (both paths too likely to fail together).

### 3.3 Path Selection Algorithm

```rust
fn select_paths(
    bundle: &Bundle,
    available: Vec<TransportPath>,
    max_paths: usize,
) -> Vec<TransportPath> {
    let mut selected: Vec<TransportPath> = vec![];
    let mut candidates = available.clone();
    
    // Score and sort by path_score descending
    candidates.sort_by(|a, b| {
        let score_a = path_score(a, &selected);
        let score_b = path_score(b, &selected);
        score_b.partial_cmp(&score_a).unwrap()
    });
    
    for candidate in candidates {
        if selected.len() >= max_paths { break; }
        let indep = independence(&candidate, &selected);
        if indep >= 0.5 {
            selected.push(candidate);
        }
    }
    selected
}
```

---

## 4. Message Deduplication at Receiver

When a bundle arrives via multiple paths, the receiver MUST process it exactly once. IRIS uses a Bloom filter for efficient deduplication with bounded memory.

### 4.1 Bloom Filter Parameters

```
Filter size (m):  512,000 bits (64 KB)
Hash functions (k): 7
Target false-positive rate: < 1% at 400,000 bundle IDs
Hash function: BLAKE3 with 7 different seeds (domain separation)
```

The bundle deduplication key is the 32-byte BLAKE3 bundle ID (derived from: source NodeId + creation timestamp + sequence number).

### 4.2 Deduplication State Management

The Bloom filter is partitioned into two windows:
- **Active window:** bundles received in the last 24 hours
- **Archive window:** bundles received 24–72 hours ago

When the active window approaches 300,000 entries (estimated via counter), the archive window is discarded, the active window becomes the archive window, and a new active window is initialized. This sliding window prevents unbounded growth while maintaining deduplication for the bundle TTL window.

### 4.3 Deduplication Flow

```
On bundle arrival at receiver:
  1. Compute bundle_id = BLAKE3(source_node_id || creation_time || sequence)
  2. Check active_bloom.contains(bundle_id)
     → true: drop duplicate, emit dedup_metric, return
  3. Check archive_bloom.contains(bundle_id)
     → true: drop duplicate, return
  4. Process bundle normally
  5. active_bloom.insert(bundle_id)
  6. Persist bloom state to disk (async, every 60s)
```

False positives (valid bundle incorrectly identified as duplicate) are acceptable at < 1% rate. The consequence is a single bundle loss, acceptable for P3–P7. For P0–P2, retransmission by the sender handles this case.

---

## 5. TransportMultiplexer Implementation

The `TransportMultiplexer` in `crates/routing/src/multiplexer.rs` is the Rust component that manages multipath dispatch.

### 5.1 Trait Definition

```rust
/// Manages concurrent transmission across multiple transports.
pub struct TransportMultiplexer {
    transports: Vec<Arc<dyn Transport>>,
    battery_guard: Arc<BatteryGuard>,
    dedup: Arc<Mutex<DedupFilter>>,
    path_scorer: PathScorer,
    metrics: MultipathMetrics,
}

impl TransportMultiplexer {
    /// Send bundle using optimal path set for its priority.
    pub async fn send(
        &self,
        bundle: Bundle,
        destination: NodeId,
    ) -> Result<MultipathResult, RoutingError> {
        let priority = bundle.priority();
        let battery = self.battery_guard.level().await;
        
        // Gate: multipath only for P0-P2 with adequate battery
        let max_paths = match (priority, battery) {
            (Priority::P0, b) if b >= 5 => 3,
            (Priority::P1, b) if b >= 20 => 2,
            (Priority::P2, b) if b >= 20 => 2,
            _ => 1,
        };
        
        let available = self.discover_paths(destination).await?;
        let selected = self.path_scorer.select(bundle.clone(), available, max_paths);
        
        // Transmit on all selected paths concurrently
        let futures: Vec<_> = selected.iter()
            .map(|path| path.transport.send(bundle.clone(), destination))
            .collect();
        
        let results = join_all(futures).await;
        self.metrics.record(&results);
        
        // Success if any path delivered
        if results.iter().any(|r| r.is_ok()) {
            Ok(MultipathResult::delivered(results))
        } else {
            Err(RoutingError::AllPathsFailed(results))
        }
    }
    
    /// Receive from all transports; deduplicate.
    pub async fn receive(&self) -> impl Stream<Item = Bundle> {
        let streams: Vec<_> = self.transports.iter()
            .map(|t| t.incoming())
            .collect();
        
        select_all(streams)
            .filter(|bundle| {
                let id = bundle.id();
                !self.dedup.lock().unwrap().contains(id)
            })
            .inspect(|bundle| {
                self.dedup.lock().unwrap().insert(bundle.id());
            })
    }
}
```

### 5.2 Multipath Result Handling

```rust
pub struct MultipathResult {
    pub paths_attempted: usize,
    pub paths_succeeded: usize,
    pub latencies: Vec<Duration>,
    pub first_delivery_latency: Duration,
}
```

The routing engine records which paths succeeded. This feeds back into the reliability EWMA for path scoring. A path that fails 3 times consecutively is marked degraded and excluded from multipath selection for 60 seconds.

---

## 6. Interaction with PRoPHET Routing

Multipath and PRoPHET are not mutually exclusive. PRoPHET determines *which peer* to forward to; multipath determines *which transport* to use for that forwarding. When PRoPHET selects peer X as the best relay for destination Y, the TransportMultiplexer is invoked to find the best multi-transport path to peer X.

---

## 7. Metrics and Observability

The multiplexer emits the following Prometheus-style metrics:

```
iris_multipath_bundles_total{priority, paths_attempted} counter
iris_multipath_success_ratio{transport_pair} gauge
iris_multipath_first_delivery_latency_ms{priority} histogram
iris_dedup_filter_insertions_total counter
iris_dedup_false_positive_estimate gauge  # estimated from filter occupancy
iris_path_score{transport, peer_id} gauge
```

---

## 8. Limitations and Known Issues

| Limitation | Impact | Planned Mitigation |
|------------|--------|-------------------|
| BLE/Wi-Fi independence only 0.4 on shared chipsets | Effective redundancy reduced on single-chip devices | Detect shared chipset at init, adjust independence score |
| LoRa duty cycle limits secondary path use | Cannot reliably use LoRa as multipath for sustained traffic | Accepted; LoRa reserved for P0 fallback |
| Bloom filter false positives ~1% | Rare valid bundle loss on second arrival | Senders retry P0–P2; acceptable for P3+ |
| No stream-level load balancing | Large bundles sent in full on each path | Fragment-level striping considered for v0.5 |
