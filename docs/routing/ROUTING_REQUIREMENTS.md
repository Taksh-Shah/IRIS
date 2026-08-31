# IRIS Routing Requirements

**Document ID:** IRIS-REQ-ROUTE-001  
**Version:** 1.0  
**Status:** Active  

---

## 1. Purpose and Scope

This document specifies functional and non-functional requirements for the IRIS routing subsystem. It covers all routing algorithms (PRoPHET, Spray-and-Wait, Epidemic, Direct Delivery), transport-layer interaction, and testability criteria. Requirements are traceable to implementation modules in `crates/routing/`.

---

## 2. Functional Requirements

### 2.1 Delivery Ratio Targets by Priority

| Priority | Class | Target Delivery Ratio | Max Acceptable Latency | Retry Budget |
|----------|-------|----------------------|------------------------|--------------|
| P0 | SOS | ≥ 99.0% | 60 seconds (best-effort) | Unlimited |
| P1 | Medical | ≥ 95.0% | 5 minutes | 50 retries |
| P2 | Location | ≥ 90.0% | 15 minutes | 20 retries |
| P3 | Emergency Text | ≥ 85.0% | 30 minutes | 10 retries |
| P4 | Normal Text | ≥ 75.0% | 2 hours | 5 retries |
| P5 | Image | ≥ 60.0% | 6 hours | 3 retries |
| P6 | Voice | ≥ 50.0% | 12 hours | 2 retries |
| P7 | Video | ≥ 40.0% | 24 hours | 1 retry |

**REQ-ROUTE-F-001:** The routing engine MUST maintain per-priority delivery tracking and emit metrics enabling delivery ratio calculation within any 1-hour rolling window.

**REQ-ROUTE-F-002:** When P0 and P1 messages are simultaneously queued for the same transport, P0 MUST be transmitted first without exception. There is no pre-emption exception for battery level below 5%.

**REQ-ROUTE-F-003:** A P0 message that has been locally accepted MUST NOT be evicted from the store under any storage pressure condition. All other priorities may be evicted in reverse priority order.

**REQ-ROUTE-F-004:** The system MUST support delivery acknowledgment propagation back to the originating node. ACKs propagate using the same routing fabric as data bundles, at P4 priority.

### 2.2 Routing Algorithm Selection

**REQ-ROUTE-F-010:** The routing engine MUST implement all four algorithms and select dynamically based on context:

| Algorithm | Trigger Condition |
|-----------|------------------|
| Direct Delivery | Source and destination are within range of same transport |
| PRoPHET | Network has been stable for ≥ 10 minutes; delivery predictability data available |
| Spray-and-Wait | No predictability data; L=5 for P0–P2, L=3 for P3–P4 |
| Epidemic | P0 only, when no other algorithm achieves delivery in 30 seconds |

**REQ-ROUTE-F-011:** Algorithm selection MUST be per-message, not per-node. A single node may simultaneously route P0 messages via Epidemic and P4 messages via PRoPHET.

**REQ-ROUTE-F-012:** PRoPHET delivery predictability values MUST be initialized to 0.0 for unknown peers and updated using the transitive delivery predictability formula:

```
P(A,B) = P(A,B)_old + (1 - P(A,B)_old) × P_init
P(A,C) = P(A,C)_old + (1 - P(A,C)_old) × P(A,B) × P(B,C) × β
```

Where P_init = 0.75, β = 0.25 (IRIS defaults, configurable).

**REQ-ROUTE-F-013:** Spray-and-Wait L values MUST be configurable per priority class at runtime via the IRIS config bundle, without requiring node restart.

### 2.3 Store-Carry-Forward

**REQ-ROUTE-F-020:** The store-carry-forward subsystem MUST buffer messages when no forwarding opportunity exists.

**REQ-ROUTE-F-021:** Stored bundles MUST be persisted to non-volatile storage within 500 ms of acceptance, enabling recovery after crash or power loss.

**REQ-ROUTE-F-022:** Bundle expiry MUST be enforced. Expired bundles (TTL exceeded) MUST be deleted and a local expiry event emitted for metrics.

**REQ-ROUTE-F-023:** The routing engine MUST provide a prioritized forwarding queue per outbound transport. On contact with a peer, messages are dequeued in priority order with head-of-line scheduling for P0.

### 2.4 Contact Opportunity Management

**REQ-ROUTE-F-030:** The routing engine MUST maintain a contact history table mapping peer NodeId → last_seen timestamp, transport type, and average session duration.

**REQ-ROUTE-F-031:** Contact opportunities MUST trigger a bundle exchange negotiation within 2 seconds of transport link establishment.

**REQ-ROUTE-F-032:** During bundle exchange, the node MUST transmit its custody of bundle IDs (bloom filter representation) so the peer can determine what to send without duplicates.

---

## 3. Non-Functional Requirements

### 3.1 Memory Constraints (Raspberry Pi Zero Target)

**REQ-ROUTE-NF-001:** The routing engine MUST operate within 48 MB RSS on RPi Zero 2W (512 MB total system RAM, 64 MB reserved for OS, remaining shared with other IRIS subsystems).

**REQ-ROUTE-NF-002:** The PRoPHET delivery predictability table MUST use bounded memory. Maximum 1,000 peer entries. LRU eviction applies when limit is reached. Each entry: 32 bytes (NodeId) + 8 bytes (f64 predictability) + 8 bytes (timestamp) = 48 bytes. Total cap: 48 KB.

**REQ-ROUTE-NF-003:** The bundle store MUST enforce a configurable storage limit (default: 256 MB on RPi Zero). When the limit is reached, bundles are evicted lowest-priority-first, oldest-first within same priority. P0 bundles are never evicted.

**REQ-ROUTE-NF-004:** The Bloom filter for deduplication MUST be bounded at 64 KB per generation (two generations → 128 KB total), providing < 1% false positive rate. Implemented at ≈53,000 IDs per generation (k=7, m=512,000 bits) to meet the memory bound. **Note (ROUT-26, 2026-08-31):** The original "400,000 IDs at <1% FPR in 64 KB" is mathematically inconsistent — 400k IDs at 1% FPR requires ≈470 KB, not 64 KB. Resolution: the 64 KB memory constraint is authoritative; capacity is set to ≈53,000 per window.

### 3.2 Latency Budgets

**REQ-ROUTE-NF-010:** Routing decision latency (time from bundle arrival to forwarding decision) MUST be < 10 ms at P99 for P0–P2 messages, < 100 ms for P3–P7.

**REQ-ROUTE-NF-011:** Bundle serialization and transport hand-off MUST complete within 50 ms of the forwarding decision.

**REQ-ROUTE-NF-012:** PRoPHET table update on peer contact MUST complete within 200 ms.

### 3.3 Battery and Power Constraints

**REQ-ROUTE-NF-020:** The routing engine MUST expose battery level to the transport multiplexer. At battery ≤ 20%, multipath transmission is disabled for P3–P7. At battery ≤ 10%, multipath disabled for all. At battery ≤ 5%, only P0 transmissions are attempted.

**REQ-ROUTE-NF-021:** The routing background loop (contact scanning, PRoPHET decay, bundle expiry) MUST not exceed 2% CPU usage averaged over 60 seconds on RPi Zero 2W (1 GHz ARM Cortex-A53).

**REQ-ROUTE-NF-022:** When no contacts have been seen for ≥ 5 minutes and no P0 bundles are pending, the routing engine MUST enter a low-power idle mode, reducing wake-up polling to 30-second intervals.

### 3.4 Scalability

**REQ-ROUTE-NF-030:** The routing engine MUST handle simultaneous contacts with up to 20 peers per node without degradation in forwarding latency.

**REQ-ROUTE-NF-031:** In crowd scenarios (1,000+ nodes within BLE range), IRIS uses crowd mode. The routing engine MUST support rate limiting to 1 bundle per source per 5 minutes in crowd mode, enforced by source NodeId.

---

## 4. Constraint Requirements

### 4.1 LoRa Duty Cycle

**REQ-ROUTE-C-001:** LoRa transmissions MUST comply with India WPC regulations at 865–868 MHz (G.S.R. 853(E) 2021 SRD band, ≤25 mW e.r.p. ≈ 14 dBm), maximum 1% duty cycle (36 s per device per hour). The LoRa transport adapter MUST track airtime usage and refuse transmission requests from the routing engine when the duty cycle budget is exhausted.

**REQ-ROUTE-C-002:** LoRa airtime budget MUST be allocated proportionally by priority when contention exists. P0 receives 60% of available airtime, P1 receives 25%, P2 receives 10%, P3–P7 share 5%.

**REQ-ROUTE-C-003:** The routing engine MUST implement a LoRa transmission backlog queue. When duty cycle is exhausted, queued messages are held in priority order and transmitted as budget recovers (recovery rate: 1% per 100 seconds of silence).

### 4.2 BLE Constraints

**REQ-ROUTE-C-010:** BLE 5.x effective range in disaster environments MUST be assumed ≤ 50m (accounting for rubble, concrete, RF interference). The routing engine MUST NOT assume connectivity to a BLE peer if last contact was > 50m ago based on GPS coordinates, if available.

**REQ-ROUTE-C-011:** BLE advertising payload is limited to 255 bytes. Bundle fragments exceeding this MUST be handled by the BLE transport adapter using ATT fragmentation. The routing engine hands off complete bundles; fragmentation is below the routing layer.

### 4.3 Infrastructure Independence

**REQ-ROUTE-C-020:** The routing engine MUST NOT depend on any central server, DNS, NTP, or internet-connected service for its core function. All routing decisions MUST be made using locally available state.

**REQ-ROUTE-C-021:** Time synchronization for TTL calculations MUST use IRIS's distributed time synchronization protocol (peer offset exchange) when GPS/NTP is unavailable. Bundles MUST NOT be dropped solely because local time is uncertain; instead, a ±30 minute tolerance MUST be applied to expiry calculations.

---

## 5. Failure Requirements

### 5.1 Node Failure Tolerance

**REQ-ROUTE-F-040:** The network MUST maintain P0 delivery ratio ≥ 99% with up to 30% of nodes simultaneously failed or partitioned.

**REQ-ROUTE-F-041:** The network MUST maintain P1 delivery ratio ≥ 90% with up to 30% node failure.

**REQ-ROUTE-F-042:** A single node failure MUST NOT cause message loss of P0–P2 bundles that have been replicated (L≥2 copies in the network).

### 5.2 Partition Tolerance

**REQ-ROUTE-F-050:** The routing engine MUST correctly route messages across network partitions lasting up to 72 hours, provided the destination node exists in the same partition component at some point within the bundle TTL.

**REQ-ROUTE-F-051:** TTL for P0 bundles MUST default to 72 hours. TTL for P1–P2 is 24 hours. TTL for P3–P4 is 6 hours. TTL for P5–P7 is 2 hours.

**REQ-ROUTE-F-052:** On network re-unification (partition healed), the routing engine MUST NOT flood the network with all stored bundles simultaneously. A jittered re-transmission schedule MUST be applied: random delay 0–60 seconds before initiating bundle exchange.

### 5.3 Byzantine Fault Tolerance

**REQ-ROUTE-F-060:** The routing engine MUST detect and respond to nodes exhibiting blackhole behavior (accepting bundles, never forwarding). Detection criterion: delivery receipt rate < 10% for peer with ≥ 20 bundles forwarded through them.

**REQ-ROUTE-F-061:** Suspected blackhole nodes MUST be demoted: their PRoPHET predictability score set to 0.01. After 3 hours of normal behavior (receipt rate ≥ 50%), demotion is lifted.

---

## 6. Testability Requirements

### 6.1 Simulation Testing

**REQ-ROUTE-T-001:** Each routing algorithm MUST be testable in the IRIS simulation harness (`tools/sim/`) with configurable node count (10–500), mobility model (random waypoint, disaster evacuation), and failure injection.

**REQ-ROUTE-T-002:** Delivery ratio MUST be measurable by the simulator and compared against targets in Section 2.1. CI pipeline MUST fail if simulated P0 delivery ratio < 99% in the reference 50-node scenario.

### 6.2 Unit Tests

**REQ-ROUTE-T-010:** PRoPHET update logic MUST have unit tests covering: first encounter, repeated encounter, transitive update, aging decay, and eviction.

**REQ-ROUTE-T-011:** Bundle eviction policy MUST have unit tests covering: storage limit hit with mixed priorities, P0 protection, LRU within same priority.

**REQ-ROUTE-T-012:** Duty cycle enforcement MUST have unit tests with mock airtime tracker, verifying that the routing engine correctly respects refusals from the LoRa transport adapter.

### 6.3 Integration Tests

**REQ-ROUTE-T-020:** End-to-end integration tests MUST verify P0 delivery across: direct link, 2-hop relay, 5-hop store-carry-forward (simulated 30-minute carry time).

**REQ-ROUTE-T-021:** Routing security tests MUST verify blackhole detection triggers within 25 forwarding events when a peer drops 100% of bundles.

### 6.4 Requirement Traceability Matrix

| Requirement ID | Implementation Location | Test ID |
|---------------|------------------------|---------|
| REQ-ROUTE-F-001 | `crates/routing/src/metrics.rs` | UT-ROUTE-001 |
| REQ-ROUTE-F-002 | `crates/routing/src/queue.rs` | UT-ROUTE-002 |
| REQ-ROUTE-F-012 | `crates/routing/src/prophet.rs` | UT-ROUTE-010 |
| REQ-ROUTE-F-040 | Simulation | SIM-ROUTE-001 |
| REQ-ROUTE-C-001 | `crates/transport/src/lora.rs` | UT-LORA-001 |
| REQ-ROUTE-NF-001 | Performance test | PERF-ROUTE-001 |

---

## 7. Open Issues

| Issue ID | Description | Owner | Target Version |
|----------|-------------|-------|----------------|
| OI-ROUTE-001 | PRoPHET decay rate β=0.25 not validated against real disaster mobility data | Routing team | v0.4 |
| OI-ROUTE-002 | LoRa duty cycle enforcement not yet implemented in transport adapter | Transport team | v0.3 |
| OI-ROUTE-003 | 72-hour partition test requires long-running simulation; not yet in CI | QA team | v0.4 |
