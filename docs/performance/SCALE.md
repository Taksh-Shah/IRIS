# Scale — Performance Characteristics at Different Network Sizes

**Component:** All IRIS components
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Scale Tier Definitions

| Tier | Nodes | Scenario | Geographic area |
|---|---|---|---|
| Small | 10 | Single building, small team | < 1 km² |
| Medium | 100 | Neighborhood, response team | 1–10 km² |
| Large | 1,000 | District-level incident | 10–100 km² |
| Disaster | 10,000 | City-scale disaster | > 100 km² |

Each tier has different dominant failure modes, memory requirements, and routing strategies. Design decisions differ by tier.

---

## 2. Memory Growth

### 2.1 Routing Table

PRoPHET routing table grows O(n) with network size. Each entry:

```rust
struct RoutingEntry {
    node_id: NodeId,          // 32 bytes
    delivery_probability: f32, // 4 bytes
    last_updated: u64,        // 8 bytes
    update_count: u32,        // 4 bytes
}
// Total: 48 bytes per entry
```

| Tier (nodes) | Routing table size |
|---|---|
| 10 | 480 bytes |
| 100 | 4.8 KB |
| 1,000 | 48 KB |
| 10,000 | 480 KB |

Routing table memory is negligible at all scales. Even at 10,000 nodes, 480 KB is within budget.

**Pruning:** Entries with delivery probability < 0.01 are pruned during aging. In practice, most nodes never encounter more than ~500 distinct peers over their lifetime, even in large networks.

### 2.2 Contact History

Contact history enables ML features and PRoPHET aging. Stored as a timestamped log:

```rust
struct ContactEvent {
    peer_id: NodeId,    // 32 bytes
    start_time: u64,    // 8 bytes
    duration_s: u32,    // 4 bytes
    bytes_xfr: u64,     // 8 bytes
}
// 52 bytes per contact event
```

**History retention:** 7 days. At 10 contacts/hour × 24h × 7d = 1,680 events:

| Tier | Events retained | Contact history size |
|---|---|---|
| Small (10) | 1,680 | 87 KB |
| Medium (100) | 16,800 | 874 KB |
| Large (1,000) | 168,000 | 8.7 MB |
| Disaster (10,000) | Impractical to store all | Cap at 10,000 events |

**Pruning at scale:** Contact history is capped at 10,000 events per node. Older events are dropped first. ML model performance degrades gracefully with shorter history.

### 2.3 Message Store

Message store grows with carried message count:

| Messages stored | SQLite size (1 KB avg) |
|---|---|
| 100 | ~200 KB |
| 1,000 | ~2 MB |
| 10,000 | ~20 MB |
| 100,000 | ~200 MB |

**Store cap:** IRIS imposes a hard store limit of 50 MB on mobile (Android budget). At 1 KB average message size, this is ~50,000 messages. Eviction is by priority × TTL_remaining (lowest priority, soonest-expiring messages are evicted first).

---

## 3. Routing Table Convergence Time

After a network partition reunites, routing tables need to propagate updated delivery probabilities across the newly merged network.

**Convergence model:** Routing table updates spread via contact events. With mean inter-contact time μ and n nodes, convergence requires approximately `log(n)` contact rounds:

```
T_convergence ≈ μ × log₂(n)
```

| Tier | μ (urban) | T_convergence (urban) | μ (rural) | T_convergence (rural) |
|---|---|---|---|---|
| Small (10) | 5 min | 17 min | 2 hours | 6.6 hours |
| Medium (100) | 5 min | 33 min | 2 hours | 13.3 hours |
| Large (1,000) | 5 min | 50 min | 2 hours | 20 hours |
| Disaster (10,000) | 5 min | 67 min | 2 hours | 26.7 hours |

**Observation:** Convergence time is logarithmic in n — large networks converge reasonably fast in urban scenarios (< 2 hours) but slowly in rural scenarios (> 20 hours for 1,000 nodes).

---

## 4. Bloom Filter Saturation at Scale

IRIS uses Bloom filters (8 KB, 10 hash functions) to track seen messages. The false positive rate (FPR) as a function of element count:

```python
import math

def bloom_fpr(n_elements: int, m_bits: int = 65536, k_hashes: int = 10) -> float:
    return (1 - math.exp(-k_hashes * n_elements / m_bits)) ** k_hashes
```

| Messages seen (n) | FPR | Status |
|---|---|---|
| 100 | 0.0001% | Excellent |
| 500 | 0.03% | Good |
| 1,000 | 0.1% | Target threshold |
| 2,000 | 0.8% | Degraded |
| 5,000 | 8.5% | Severely degraded |
| 10,000 | 47% | Bloom filter useless |

**Saturation at scale:** In a 10,000-node network during a disaster, a busy relay node could see 10,000+ unique message IDs, saturating the Bloom filter. Mitigations:

1. **Rotation:** Bloom filter rotates every 4 hours. Messages older than 4 hours are no longer tracked (acceptable, as TTL-expired messages are pruned from store)
2. **Larger filter:** At disaster scale (Tier 4), automatically resize to 256 KB (8× larger), pushing saturation to 32,000 messages
3. **Counting filter:** If exact membership is needed, use a counting Bloom filter (4× memory)

---

## 5. Epidemic Routing Storm at Scale

Without rate limiting, epidemic routing at large scale causes a broadcast storm:

**Storm scenario:** 1,000 nodes, 10 new messages per hour per node, epidemic routing, 3-hop average:

```
Total messages in network per hour:
  New messages: 1,000 nodes × 10 msg/hr = 10,000 msg/hr
  Epidemic copies: 10,000 × (n-1) ≈ 10,000 × 1,000 = 10,000,000 relay operations/hr

Per-node relay rate: 10,000 relays/hr = 2.78/second
BLE capacity: ~400 kbps → 400 KB/s / 1 KB per msg = 400 msg/s → feasible per node

LoRa capacity: 1.4 kbps → 1.4 KB/s / 1 KB per msg × 144ms/msg duty = 10 msg/hr per node
```

**LoRa cannot sustain epidemic routing above ~10 messages/node/hour at 1,000 nodes.** This is a hard physical limit.

**Mitigation: Priority-gated routing**

| Transport | Epidemic routing | Spray-and-Wait | Direct-only |
|---|---|---|---|
| BLE (400 kbps) | P0–P1 | P2–P3 | P4+ |
| Wi-Fi (100 Mbps) | P0–P3 | P4 | P5+ |
| LoRa (1.4 kbps) | P0 only | P1 (L≤5) | P2+ direct |

---

## 6. Scale-Specific Recommendations

### Small (10 nodes)

- All routing algorithms viable
- Use epidemic for all priorities (network is small)
- BLE is sufficient as primary transport
- No tuning required

### Medium (100 nodes)

- Use Spray-and-Wait for P2–P4 (L = sqrt(n) ≈ 10)
- Keep epidemic for P0–P1 only
- Monitor Bloom filter: rotate if FPR > 0.5%
- LoRa suitable as a secondary transport

### Large (1,000 nodes)

- Spray-and-Wait L ≤ 15 for P2–P3
- P4+ uses direct delivery only
- LoRa: P0 epidemic only; P1+ Spray-and-Wait L ≤ 5
- Bloom filter: use 32 KB filter (or rotation every 2h)
- Deploy edge servers as routing super-nodes (≥ 1 per 100 nodes)

### Disaster (10,000 nodes)

- Community detection to partition the routing problem
- Intra-community: PRoPHET + ML
- Inter-community: epidemic for P0 only, direct for P1+
- LoRa: P0 only, no relay (direct SOS to nearest gateway)
- Edge servers critical: at least 1 per 500 nodes
- Bloom filter: 256 KB, rotate every 1h

---

## 7. Scale Monitoring

```bash
# Check scale metrics on edge server
iris-cli scale-report

# Output:
# IRIS Scale Report — 2026-08-11 14:32 UTC
# Known nodes: 847
# Routing table entries: 823
# Message store: 12,847 messages (34 MB)
# Bloom filter FPR estimate: 0.08%
# Contact history events: 8,412
# Community count: 7
# Bridge nodes: 3
# Network partitions: 0
# Recommendation: LARGE tier — consider Spray-and-Wait L reduction
```

---

## 8. References

- Bloom filter math: `docs/simulation/SCALE_TESTING.md`
- Epidemic storm analysis: `docs/performance/BANDWIDTH.md`
- Community detection: `docs/intelligence/GRAPH_INTELLIGENCE.md`
- Performance budgets: `docs/performance/PERFORMANCE_BUDGETS.md`
