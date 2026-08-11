# Bandwidth — Transport Capacity and Protocol Overhead

**Component:** All IRIS transports
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Transport Bandwidth Characteristics

| Transport | Raw bandwidth | IRIS effective throughput | Notes |
|---|---|---|---|
| BLE 5.x (2M PHY) | 2 Mbps | ~800 kbps | ATT MTU 512B, protocol overhead |
| BLE 5.x (1M PHY) | 1 Mbps | ~400 kbps | Default PHY |
| Wi-Fi Direct (5 GHz) | 300 Mbps | ~100 Mbps | Shared medium, 3 dBi gain |
| Wi-Fi Direct (2.4 GHz) | 144 Mbps | ~50 Mbps | More common on mobile |
| LoRa SF7 @ 125 kHz | 5.47 kbps | ~4 kbps | After duty cycle |
| LoRa SF9 @ 125 kHz | 1.76 kbps | ~1.4 kbps (after 1% duty) | IRIS default |
| LoRa SF12 @ 125 kHz | 0.29 kbps | ~0.23 kbps | Long range, low rate |
| Cellular (LTE) | 50–150 Mbps | ~30 Mbps (effective) | Shared cell capacity |
| Iridium SBD | 2.4 kbps | ~0.68 kbps | Per-message overhead |
| Starlink | 80 Mbps (typical) | ~60 Mbps | Terminal dependent |

---

## 2. Per-Priority Bandwidth Consumption

### 2.1 Expected Message Sizes and Rates

Using the traffic model parameters from `docs/simulation/TRAFFIC_MODELS.md` for sustained disaster phase:

| Priority | Msg size | Rate (per node/hour) | Bandwidth per node |
|---|---|---|---|
| P0 (SOS) | 256 B | 1 | 0.57 bps |
| P1 (Medical) | 256 B | 5 | 2.84 bps |
| P2 (Location) | 128 B | 30 (every 2 min) | 8.53 bps |
| P3 (Emergency text) | 512 B | 10 | 11.38 bps |
| P4 (Normal) | 1024 B | 5 | 11.38 bps |
| **P0–P4 total** | — | — | **~34 bps** |

**34 bps per node** is trivial on BLE or Wi-Fi. On LoRa (1,400 bps effective), 34 bps is only 2.4% of capacity — the network is bandwidth-constrained only when running epidemic routing at high node counts.

### 2.2 Epidemic Routing Amplification

With epidemic routing and L copies:

```
Bandwidth_total = Bandwidth_per_node × L × hop_count_avg
```

For L=20 copies, 3-hop average:
```
34 bps × 20 × 3 = 2,040 bps per node
```

At 50 nodes, total LoRa bandwidth needed: 100 kbps >> 1.4 kbps available. This confirms that epidemic routing cannot run P4+ traffic on LoRa at any meaningful scale.

**Policy:** Epidemic routing is limited to P0–P1 on LoRa. P2+ uses Spray-and-Wait with L ≤ 10 on LoRa.

---

## 3. Protocol Overhead Analysis

### 3.1 Message Header Overhead

Each IRIS message has a fixed header plus per-hop relay list:

```
Field                   Size (bytes)
─────────────────────────────────────
version                 1
priority                1
flags                   1
message_id              16   (UUID)
sender_id               32   (Ed25519 public key)
recipient_id            32   (Ed25519 public key, or broadcast)
timestamp               8    (Unix nanoseconds)
ttl_seconds             4
hop_count               1
max_hops                1
signature               64   (Ed25519)
─────────────────────────────────────
Fixed header total:     161 bytes

Per-hop relay entry:    34 bytes (relay_id 32 + timestamp 2)
Relay list (k hops):    34k bytes
```

**Header as % of payload:**

| Message type | Payload | Header (0 relays) | Overhead % |
|---|---|---|---|
| P0 SOS | 64 B | 161 B | 72% |
| P2 Location | 128 B | 161 B | 56% |
| P3 Emergency | 512 B | 161 B | 24% |
| P4 Normal (1 KB) | 1024 B | 161 B | 14% |
| P5 Photo (512 KB) | 524288 B | 161 B | 0.03% |

**Key observation:** P0 SOS messages have 72% header overhead. For Iridium SBD (340-byte limit), this leaves only 179 bytes for payload. IRIS uses a compact P0 encoding for Iridium (see `docs/simulation/SATELLITE_SIMULATION.md`).

### 3.2 Bloom Filter Sync Overhead

During a contact event, peers exchange Bloom filters to determine which messages the other does not have. Bloom filter exchange is the primary handshake step.

| Parameter | Value |
|---|---|
| Bloom filter size | 8 KB (65,536 bits) |
| Filters per sync | 10 (one per time window) |
| Total per sync | 80 KB |
| Sync frequency | Every contact (BLE/Wi-Fi) |

**Bloom filter overhead at BLE (400 kbps):**
```
80 KB / 400 kbps ≈ 1.6 seconds per sync
```

For a 45-second BLE contact window, 1.6 seconds of Bloom sync overhead is 3.6% — acceptable.

**Bloom filter overhead at LoRa SF9 (1.4 kbps):**
```
80 KB / 1.4 kbps ≈ 457 seconds per sync
```

Full Bloom filter exchange is **not feasible over LoRa**. IRIS uses a compressed summary:
- Over LoRa: exchange only a 256-bit MinHash sketch (32 bytes) → 0.18 seconds
- Precision loss: ~5% false negative rate (some messages not forwarded that should be)
- Acceptable tradeoff for constrained transport

---

## 4. Congestion Handling

### 4.1 Multiple Peers Arriving Simultaneously

When 5 peers arrive simultaneously (e.g., at a checkpoint), the naive approach serializes all Bloom syncs and transfers, creating a queue:

```
Time to service 5 peers simultaneously (BLE):
  Bloom sync: 5 × 1.6s = 8 seconds (overlapped with connection)
  Message transfer: depends on queue overlap
  Total: ~15-30 seconds to complete all syncs
```

IRIS handles multiple simultaneous peers with:

1. **Parallel BLE connections:** Android supports up to 7 simultaneous BLE connections (GATT server). IRIS services up to 4 simultaneous peers.
2. **Priority ordering:** Within a contact window, P0–P1 messages are transferred first regardless of connection order
3. **Time-bounded transfer:** Each peer gets a maximum of 20 seconds for initial Bloom sync + priority message transfer, then connection is released if queue persists

### 4.2 LoRa Channel Congestion

Multiple nodes transmitting LoRa simultaneously cause collisions. At 50 nodes with duty cycle enforcement:

```
Max TX rate per node: 250 packets/hour (1% duty, 144ms air time)
50 nodes total: 12,500 packets/hour = 3.47 packets/second
Air time utilization: 3.47 × 0.144 = 0.50 = 50% channel busy
```

At 50% channel busy, ALOHA collision probability:
```
P(collision) ≈ 1 - e^{-G} where G = offered load
For slotted ALOHA, P(success) = G × e^{-G}
At G = 0.5: P(success) ≈ 0.30 → effective throughput reduced to 30%
```

At 50% channel load, LoRa throughput degrades significantly. **Recommendation:** Limit LoRa relay nodes to ≤ 20 per km² to keep channel load below 20%.

---

## 5. Bandwidth Budget by Mode

| Mode | BLE target | Wi-Fi target | LoRa target |
|---|---|---|---|
| Full | 50 kbps average | Up to 10 Mbps | < 1.0 kbps (P0–P3 only) |
| Reduced | 20 kbps | Disabled | < 0.5 kbps (P0–P2 only) |
| Emergency | 5 kbps | Disabled | < 0.3 kbps (P0–P1 only) |
| SOS-only | 1 kbps | Disabled | < 0.1 kbps (P0 only) |

---

## 6. Bandwidth Monitoring

IRIS tracks per-transport bandwidth in the operational dashboard:

```
Bandwidth (last 1 hour)
  BLE:    TX 1.2 MB  RX 3.4 MB  Overhead: 12%
  LoRa:   TX 48 KB   RX 112 KB  Channel busy: 8%
  Wi-Fi:  TX 45 MB   RX 120 MB  Overhead: 3%
```

---

## 7. References

- BLE 5.x throughput: Bluetooth SIG Core Specification 5.4, Vol 6
- Wi-Fi Direct: Wi-Fi Alliance P2P specification
- LoRa data rate: Semtech AN1200.22
- ALOHA throughput: Abramson (1970), "The ALOHA System"
- Iridium SBD: Iridium SBD Developer's Guide
- Performance budgets: `docs/performance/PERFORMANCE_BUDGETS.md`
