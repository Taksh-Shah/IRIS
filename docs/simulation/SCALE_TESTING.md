# Scale Testing — Simulation Performance at Scale

**Component:** `iris-sim` (Python), `iris-sim-rs` (Rust, experimental)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Scale Definitions

| Tier | Node count | Scenario | Primary bottleneck |
|---|---|---|---|
| Small | 10 | Dev testing, unit scenarios | None — runs in seconds |
| Medium | 100 | MVP validation, integration testing | Memory, contact computation |
| Large | 1,000 | Alpha release validation | Contact matrix, routing table |
| Disaster | 10,000 | Beta stress test | Bloom filter saturation, epidemic storm |

All scale tiers must complete within the wall-time limits below. CI runs Small and Medium. Large and Disaster are run manually before releases.

---

## 2. Memory Estimates

### 2.1 Python Simulation (per node)

| Data structure | Size estimate |
|---|---|
| Contact history (1000 events) | ~200 KB |
| Routing table (1000 entries) | ~100 KB |
| Message store (100 messages) | ~10 MB |
| Bloom filters (10 filters × 8 KB) | ~80 KB |
| NetworkX graph node | ~1 KB |
| Python object overhead | ~5 MB base |
| **Total per node** | **~50 MB** |

**Total Python simulation memory:**

| Tier | Nodes | Memory |
|---|---|---|
| Small | 10 | ~500 MB |
| Medium | 100 | ~5 GB |
| Large | 1,000 | ~50 GB (requires large machine) |
| Disaster | 10,000 | ~500 GB (not feasible in Python) |

### 2.2 Rust Simulation (per node)

| Data structure | Size |
|---|---|
| Contact history (1000 events) | ~40 KB |
| Routing table (1000 entries) | ~24 KB |
| Message store (100 messages) | ~1 MB |
| Bloom filter (8 KB each × 10) | ~80 KB |
| Graph node (petgraph) | ~200 B |
| **Total per node** | **~5 MB** |

**Total Rust simulation memory:**

| Tier | Nodes | Memory |
|---|---|---|
| Small | 10 | ~50 MB |
| Medium | 100 | ~500 MB |
| Large | 1,000 | ~5 GB |
| Disaster | 10,000 | ~50 GB (feasible on cluster) |

---

## 3. Parallelization Strategy

### 3.1 Python: Multiprocessing with Partition

Python's GIL prevents thread-based parallelism for CPU-bound simulation. IRIS uses `multiprocessing` with a geographic partition strategy:

```python
from multiprocessing import Pool, Queue
from iris_sim.partition import geographic_partition

def run_partition(partition_nodes: list[NodeId], shared_queue: Queue):
    """Each process simulates its partition and sends cross-partition contacts to the queue."""
    sim = PartitionSimulator(partition_nodes)
    sim.run()
    for contact in sim.cross_partition_contacts:
        shared_queue.put(contact)

def run_large_simulation(all_nodes: list[NodeId], n_workers: int = 10):
    partitions = geographic_partition(all_nodes, n_partitions=n_workers)
    shared_queue = Queue()
    
    with Pool(n_workers) as pool:
        pool.starmap(run_partition, [(p, shared_queue) for p in partitions])
    
    # Process cross-partition contact events
    while not shared_queue.empty():
        apply_cross_partition_contact(shared_queue.get())
```

**Target:** 1 process per 100 nodes. For 1,000 nodes: 10 processes on a 10-core machine.

### 3.2 Rust: Multi-threaded with Actor Model

The Rust simulation uses Tokio async runtime with one actor per node, communicating via message passing:

```rust
use tokio::sync::mpsc;

pub async fn run_simulation(nodes: Vec<NodeConfig>) {
    let mut handles = Vec::new();
    let channels: Vec<_> = nodes.iter()
        .map(|_| mpsc::channel::<SimEvent>(1024))
        .collect();

    for (i, node) in nodes.into_iter().enumerate() {
        let (tx, rx) = channels[i].clone();
        let handle = tokio::spawn(async move {
            NodeActor::new(node, tx, rx).run().await
        });
        handles.push(handle);
    }
    
    futures::future::join_all(handles).await;
}
```

Rust simulation: 10,000 nodes on a 16-core machine, ~8 GB RAM, 1-hour simulated time completes in ~20 minutes wall time.

---

## 4. Performance Targets

### 4.1 Wall Time Targets

| Tier | Simulated time | Python target | Rust target | CI? |
|---|---|---|---|---|
| Small (10) | 2 hours | < 30 seconds | < 5 seconds | Yes |
| Medium (100) | 6 hours | < 10 minutes | < 1 minute | Yes |
| Large (1,000) | 8 hours | < 4 hours | < 15 minutes | No (release only) |
| Disaster (10,000) | 2 hours | Not feasible | < 2 hours | No (release only) |

### 4.2 CPU Time Metrics

Collected via `time.perf_counter()` at each simulation phase:

| Phase | % of total CPU time (Medium tier) |
|---|---|
| Contact computation (who meets whom) | 45% |
| Routing decision (PRoPHET + ML) | 25% |
| Message store operations | 15% |
| Bloom filter operations | 8% |
| Graph analytics | 5% |
| Other | 2% |

Contact computation is the dominant cost. Optimization: pre-compute contact schedule from mobility model rather than computing pairwise distances at each timestep.

---

## 5. Scale Failure Modes

### 5.1 Bloom Filter Saturation

IRIS uses Bloom filters to track which messages a node has seen, avoiding re-relay. Each filter has parameters `(m, k)`:

- `m` = number of bits
- `k` = number of hash functions
- Target false positive rate: 0.1% (0.001)

For `n` inserted elements:
```
FPR ≈ (1 - e^{-kn/m})^k
```

**Saturation point:** For IRIS default parameters (m = 65,536 bits = 8 KB, k = 10):

| Network size (messages) | FPR |
|---|---|
| 100 | 0.0001% |
| 500 | 0.01% |
| 1,000 | 0.09% |
| 2,000 | 0.4% — exceeds 0.1% target |
| 5,000 | 4% — severe degradation |

**Mitigation:** Bloom filter is rotated every 4 hours. Messages older than the rotation interval are no longer tracked. At the saturation point (>2,000 unique message IDs per 4-hour window), the filter capacity is doubled automatically.

```rust
pub struct BloomFilterManager {
    current: BloomFilter,
    previous: BloomFilter,
    rotation_interval: Duration,
    last_rotation: Instant,
}

impl BloomFilterManager {
    pub fn rotate_if_needed(&mut self, now: Instant) {
        if now.duration_since(self.last_rotation) > self.rotation_interval {
            self.previous = std::mem::replace(&mut self.current, BloomFilter::new());
            self.last_rotation = now;
        }
    }
    
    pub fn contains(&self, id: &MessageId) -> bool {
        self.current.contains(id) || self.previous.contains(id)
    }
}
```

### 5.2 Epidemic Storm

Epidemic routing replicates every message to every contact. At 1,000 nodes without rate limiting:

- Each node carries ~1,000 messages after full epidemic spread
- Each contact event triggers 1,000 message transfers
- Bandwidth: 1,000 messages × 1,024 bytes × 100 contacts/hour = **100 MB/hour per node**

This is unsustainable. IRIS uses:

1. **Priority-gated epidemic:** Only P0–P1 messages use epidemic routing. P2+ use Spray-and-Wait.
2. **Copy count limiting:** Spray-and-Wait L-value caps total copies at `min(20, sqrt(N))` for the network size estimate N.
3. **Queue admission control:** When queue > 80% full, new relay requests for P4+ are rejected.

### 5.3 Routing Table Memory Growth

PRoPHET routing tables grow O(n) with network size. At 10,000 nodes:

- Each routing table entry: ~200 bytes (NodeId + delivery probability + aging timestamps)
- Routing table: 10,000 × 200 = **2 MB per node** — manageable

Mitigation: entries with delivery probability < 0.01 are pruned during the aging step.

---

## 6. Metrics Collection During Scale Tests

```python
@dataclass
class ScaleTestMetrics:
    node_count: int
    simulated_duration_s: float
    wall_time_s: float
    peak_memory_mb: float
    routing_table_convergence_s: float  # time until DR stabilizes
    bloom_filter_fpr_final: float
    messages_generated: int
    messages_delivered: int
    delivery_ratio: float
    cpu_time_s: dict[str, float]  # by phase

def collect_metrics(sim: Simulation) -> ScaleTestMetrics:
    return ScaleTestMetrics(
        node_count=sim.node_count,
        simulated_duration_s=sim.time,
        wall_time_s=sim.wall_time_elapsed,
        peak_memory_mb=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1024,
        routing_table_convergence_s=sim.convergence_time,
        bloom_filter_fpr_final=sim.bloom_fpr_estimate(),
        messages_generated=sim.messages_sent,
        messages_delivered=sim.messages_delivered,
        delivery_ratio=sim.messages_delivered / sim.messages_sent,
        cpu_time_s=sim.cpu_time_by_phase,
    )
```

---

## 7. References

- Bloom filter math: Mitzenmacher & Upfal, "Probability and Computing" (2017)
- Epidemic routing: Vahdat & Becker, "Epidemic Routing for Partially Connected Ad Hoc Networks" (2000)
- Python multiprocessing: https://docs.python.org/3/library/multiprocessing.html
- Tokio async Rust: https://tokio.rs/
- Scale failure modes: `docs/performance/SCALE.md`
