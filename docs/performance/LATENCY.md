# Latency — End-to-End Delivery Latency Analysis

**Component:** All IRIS transports and routing
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Latency Budget by Priority

| Priority | Direct target | 1-hop relay target | Multi-hop target |
|---|---|---|---|
| P0 (SOS) | < 30 seconds | < 5 minutes | < 30 minutes |
| P1 (Medical) | < 2 minutes | < 15 minutes | < 2 hours |
| P2 (Location) | < 5 minutes | < 30 minutes | Best effort |
| P3 (Emergency text) | < 10 minutes | < 1 hour | Best effort |
| P4 (Normal) | Best effort | Best effort | Best effort |
| P5–P7 (Media) | Best effort | Best effort | Best effort |

**Note:** "Direct" means the sender and recipient are currently in contact. "1-hop relay" means one intermediate node. "Multi-hop" means 2+ intermediate nodes.

---

## 2. Latency Components

### 2.1 Component Breakdown

End-to-end latency is the sum of:

```
L_e2e = L_app + L_queue + L_transport + L_relay_wait + L_relay_queue + L_delivery
```

| Component | Symbol | Description |
|---|---|---|
| App processing | L_app | Message creation, signing, encryption |
| Queue wait | L_queue | Time waiting in priority queue at sender |
| Transport setup | L_transport | Connection establishment + data transfer |
| Relay wait | L_relay_wait | Time at relay node waiting for next contact |
| Relay queue | L_relay_queue | Time in relay node's priority queue |
| Delivery notification | L_delivery | Ack traversal back to sender (optional) |

### 2.2 L_app — Application Processing

| Operation | Typical time |
|---|---|
| Message creation + serialization | < 1 ms |
| Ed25519 sign | < 2 ms |
| AES-GCM encrypt | < 1 ms |
| Queue insertion | < 0.1 ms |
| **Total L_app** | **< 5 ms** |

### 2.3 L_queue — Priority Queue Wait

P0 messages are inserted at the front of the queue and processed immediately. Wait time depends on transport availability:

| Scenario | P0 queue wait | P4 queue wait |
|---|---|---|
| Transport available | < 10 ms | Seconds to minutes |
| Transport busy | < 100 ms (preempts P4 transfer) | Up to queue drain time |
| Transport unavailable | 0 (stored in queue, forwarded when available) | — |

IRIS implements **preemptive priority:** a P0 message can interrupt a P4 transfer mid-stream (the P4 transfer is suspended, P0 is transmitted, then P4 resumes). This requires transport-level fragmentation support.

### 2.4 L_transport — Transport Characteristics

| Transport | Connection setup | Transfer (256B) | Total for P0 SOS |
|---|---|---|---|
| BLE (already connected) | 0 ms | ~8 ms | ~8 ms |
| BLE (new connection) | 20–100 ms | ~8 ms | 28–108 ms |
| Wi-Fi Direct (established) | 0 ms | < 1 ms | < 1 ms |
| Wi-Fi Direct (new) | 1,000–5,000 ms | < 1 ms | 1–5 s |
| LoRa SF9 (no connection) | 0 ms | 144 ms | 144 ms |
| Cellular | 500–3,000 ms | < 10 ms | 0.5–3 s |
| Iridium SBD | 20–60 s | 2 s | 22–62 s |

### 2.5 L_relay_wait — Waiting for Next Contact

The dominant latency component in multi-hop delivery is waiting at relay nodes for the next contact with an appropriate carrier.

For exponentially distributed inter-contact times with mean μ seconds:

```
E[L_relay_wait] = μ / 2
```

**Example inter-contact times:**

| Network density | Contact model | Mean inter-contact | Expected relay wait |
|---|---|---|---|
| Urban dense (100 nodes/km²) | BLE + WiFi | 5 minutes | 2.5 minutes |
| Urban medium (20 nodes/km²) | BLE | 20 minutes | 10 minutes |
| Rural sparse (0.5 nodes/km²) | LoRa | 2 hours | 1 hour |
| Very sparse (0.1 nodes/km²) | LoRa | 8 hours | 4 hours |

---

## 3. P0 Latency Budget Validation

### 3.1 Direct Delivery (0 hops)

```
L_direct = L_app + L_queue + L_transport
         = 5 ms + 10 ms + 108 ms (BLE, worst case)
         = 123 ms  << 30s target ✓
```

The 30-second budget for direct delivery is easily achieved on any transport. Even Iridium SBD (62s) exceeds the target but is the last resort.

### 3.2 One-Hop Relay

```
L_1hop = L_direct_to_relay + L_relay_wait + L_relay_to_destination
       = 123 ms + E[relay_wait] + 123 ms
       = 0.25 s + E[relay_wait]

For 5-minute budget: E[relay_wait] < 295 s → μ < 590 s (9.8 minutes)
```

**P0 1-hop relay requires mean inter-contact time < ~10 minutes.** This is achievable in urban and suburban scenarios. In rural sparse (μ = 2 hours), the 5-minute budget is not achievable for 1-hop relay — multi-hop via LoRa chains is required.

### 3.3 Multi-Hop (k hops)

```
L_khop = Σ_{i=1}^{k} (L_transport + E[relay_wait_i])
       ≈ k × (0.25 + μ/2)

For 30-minute budget (1800s) with k=5 hops:
  E[relay_wait] per hop < (1800 - 5×0.25) / 5 ≈ 360 s (6 minutes per hop)
  Required μ < 720 s (12 minutes mean inter-contact)
```

5-hop P0 delivery in 30 minutes requires mean inter-contact time < 12 minutes. This is achievable in medium-density scenarios but not in very sparse rural deployments.

---

## 4. Queue Wait Time Under Load

When many messages arrive simultaneously (disaster onset), queue wait time increases for lower-priority messages:

```
L_queue_p = Σ_{q < p} (messages_ahead_q × service_time_q) / throughput
```

For P4 messages during a disaster onset burst (100 P0–P3 messages ahead in queue):

```
Time to clear P0–P3 queue ≈ 100 × 1KB / 50 kbps = 16 seconds
P4 queue wait ≈ 16 seconds before first P4 message is transmitted
```

P4 wait times of minutes are expected during disaster onset. This is acceptable — P4 is best-effort.

---

## 5. Latency Measurement Methodology

### 5.1 Simulation Measurement

```python
class MessageTracker:
    def __init__(self):
        self.send_times: dict[MessageId, float] = {}
        self.delivery_times: dict[MessageId, float] = {}

    def on_send(self, msg_id: MessageId, t: float):
        self.send_times[msg_id] = t

    def on_deliver(self, msg_id: MessageId, t: float):
        self.delivery_times[msg_id] = t

    def latency_stats(self) -> LatencyStats:
        latencies = [
            self.delivery_times[mid] - self.send_times[mid]
            for mid in self.delivery_times
        ]
        return LatencyStats(
            mean=np.mean(latencies),
            median=np.median(latencies),
            p95=np.percentile(latencies, 95),
            p99=np.percentile(latencies, 99),
        )
```

### 5.2 Hardware Measurement

For hardware latency measurement, IRIS uses synchronized clocks (GPS-derived or NTP with < 100 ms accuracy). A tagged test message is sent, and the delivery timestamp is logged at the receiver.

All latency measurements include the GPS/NTP synchronization uncertainty (±100 ms) in their error bars. Sub-100ms latency measurements are not meaningful with this method.

---

## 6. Latency Reporting

The coordinator dashboard shows a rolling 1-hour latency summary:

```
Delivery Latency (last 1 hour)
Priority  Messages  Mean      P50      P95      P99      Max
P0        12        47s       22s      3m12s    8m45s    23m02s
P1        45        4m23s     2m18s    18m11s   31m04s   1h12m
P2        234       18m       8m       45m      2h12m    6h23m
P3        556       1h02m     34m      4h12m    —        —
```

Latency exceeding the budget triggers an alert:

```
WARN  component=routing event=latency_budget_exceeded
      priority=P0 latency_s=547 budget_s=300 hop_count=2
```

---

## 7. References

- DTN latency analysis: Fall, K. (2003). "A Delay-Tolerant Network Architecture for Challenged Internets"
- BLE connection interval: Bluetooth SIG Core Spec 5.4, Vol 6, Part B
- IRIS battery constraints (affect latency tradeoffs): `docs/performance/BATTERY.md`
- IRIS bandwidth constraints: `docs/performance/BANDWIDTH.md`
- Performance model: `docs/performance/PERFORMANCE_MODEL.md`
