# IRIS Routing Experiments

**Document ID:** IRIS-EXP-ROUTE-005  
**Version:** 1.0  
**Status:** Active — results pending simulation implementation  

---

## 1. Experiment Framework

All routing experiments are run using the IRIS network simulator (`tools/sim/`), a discrete-event simulation built on the ONE (Opportunistic Network Environment) mobility model framework, adapted for IRIS-specific transports and routing algorithms.

### 1.1 Simulator Parameters (Common)

| Parameter | Value |
|-----------|-------|
| Simulator | IRIS-Sim v0.2 (tools/sim/) |
| Time unit | Simulated seconds |
| Bundle generation | Poisson process, λ per experiment |
| Message size | P0: 256 B, P1: 1 KB, P2: 512 B, P3: 2 KB |
| Warm-up period | 3600 simulated seconds (discarded) |
| Measurement period | 86,400 simulated seconds (24 hours) |
| Repetitions | 30 runs per configuration (different random seeds) |
| Confidence interval | 95%, t-distribution |

### 1.2 Mobility Models

| Model | Description | Use Case |
|-------|-------------|----------|
| Random Waypoint (RWP) | Nodes move to random destinations at 1-5 km/h | Baseline comparison |
| Disaster Evacuation | Nodes move toward designated safe zones with obstacles | Primary evaluation model |
| Stationary Cluster | Groups of nodes stationary, occasional inter-group mobility | Remote shelter scenario |
| SWIM (Small World in Motion) | Social-force model, nodes preferentially near familiar peers | Urban scenario |

---

## 2. EXP-ROUTE-001: PRoPHET vs Epidemic in 50-Node Scenario

### 2.1 Objective

Compare delivery ratio and network overhead between PRoPHET and Epidemic routing across priority classes, in a representative disaster evacuation mobility scenario.

### 2.2 Hypothesis

PRoPHET will achieve comparable delivery ratio to Epidemic for P0–P2 while generating significantly less network overhead (fewer redundant transmissions). Epidemic will outperform PRoPHET when network contact patterns are too irregular for predictability to build up.

### 2.3 Parameters

| Parameter | Value |
|-----------|-------|
| Node count | 50 |
| Simulation area | 2 km × 2 km |
| Mobility model | Disaster Evacuation |
| Node speed | 1–4 km/h |
| BLE range | 50 m |
| Wi-Fi Direct range | 150 m |
| Bundle generation rate | 2 bundles/min total network, 70% P0/P1, 30% P2–P4 |
| Bundle TTL | P0: 72h, P1: 24h, P2/P3: 6h |
| PRoPHET P_init | 0.75 |
| PRoPHET β (aging) | 0.25 |
| PRoPHET γ (decay) | 0.98/encounter |

### 2.4 Metrics Collected

- **Delivery ratio** per priority class
- **Average delivery latency** (creation → receipt, for delivered bundles)
- **Network overhead ratio** = total transmissions / unique bundles delivered
- **Bundle replication count** = average copies of each bundle in network at peak
- **Buffer occupancy** = average % of 256 MB store occupied per node

### 2.5 Expected Results

| Metric | PRoPHET (predicted) | Epidemic (predicted) |
|--------|--------------------|--------------------|
| P0 delivery ratio | 97–99% | 99–100% |
| P1 delivery ratio | 92–96% | 96–99% |
| P2 delivery ratio | 85–92% | 90–96% |
| Average P0 latency | 45–120 s | 30–90 s |
| Network overhead ratio | 3–6× | 10–20× |
| Peak buffer occupancy | 15–25% | 40–70% |

**Expected conclusion:** PRoPHET is preferred for P1–P4 due to 3–4× lower overhead while maintaining acceptable delivery ratios. Epidemic is used for P0 as the delivery ratio gap justifies the overhead cost.

### 2.6 Actual Results

*Pending simulation run. Will be updated after EXP-ROUTE-001 execution in CI.*

```
Status: NOT YET RUN
Scheduled: v0.3 milestone
Runner: tools/sim/run_experiment.sh --exp EXP-ROUTE-001
```

### 2.7 Analysis Protocol

1. Run 30 repetitions of each algorithm configuration.
2. Compute mean and 95% CI for each metric.
3. Apply Mann-Whitney U test for statistical significance (p < 0.05).
4. If PRoPHET P0 delivery ratio < 95%: recommend Epidemic as mandatory P0 algorithm (already IRIS default).
5. If PRoPHET overhead ratio > 8×: investigate PRoPHET parameter tuning.

---

## 3. EXP-ROUTE-002: Spray-and-Wait L Value Optimization

### 3.1 Objective

Determine optimal L (copy count) for Spray-and-Wait routing across priority classes and network densities. Current IRIS default is L=5 for P0–P2 and L=3 for P3–P4.

### 3.2 Hypothesis

Optimal L increases with network sparsity and decreases with network density. A single L value across all densities is suboptimal; adaptive L based on observed contact frequency would outperform static L.

### 3.3 Parameters

| Parameter | Value |
|-----------|-------|
| Node count | 20, 50, 100, 200 |
| L values tested | 2, 3, 5, 7, 10, 15, 20 |
| Mobility model | Random Waypoint (for controlled ICT) |
| ICT target | Low density: ~30 min, Medium: ~10 min, High: ~3 min |
| Bundle TTL | P0: 72h (all runs use P0 for clarity) |
| Repetitions | 30 per (node_count, L) combination |

### 3.4 Expected Results

| Network Density | Optimal L | Delivery Ratio at Opt-L | Overhead at Opt-L |
|----------------|-----------|------------------------|-------------------|
| Sparse (20 nodes) | 10–15 | 85–92% | 12–18× |
| Medium (50 nodes) | 5–7 | 93–97% | 6–10× |
| Dense (100 nodes) | 3–5 | 96–99% | 4–7× |
| Very Dense (200 nodes) | 2–3 | 97–99% | 2–4× |

**Expected finding:** L=5 is a reasonable default for medium density (50-node disaster scenario). Adaptive L based on observed inter-contact time (ICT) would improve performance across densities.

### 3.5 Adaptive L Proposal

If results confirm the density-L relationship, the following adaptive formula will be proposed:

```rust
fn adaptive_spray_l(mean_ict_seconds: f64, ttl_seconds: f64) -> u8 {
    // L should give reasonable probability that at least one copy
    // encounters the destination before TTL
    let encounters_in_ttl = ttl_seconds / mean_ict_seconds;
    let l = (encounters_in_ttl.sqrt() as u8).clamp(2, 20);
    l
}
```

### 3.6 Actual Results

*Pending simulation run.*

```
Status: NOT YET RUN
Scheduled: v0.3 milestone
Runner: tools/sim/run_experiment.sh --exp EXP-ROUTE-002
```

---

## 4. EXP-ROUTE-003: Mobility Model Impact on Delivery Ratio

### 4.1 Objective

Quantify how different real-world mobility patterns affect IRIS routing performance. Disaster evacuation mobility differs substantially from the random waypoint model used in most DTN research.

### 4.2 Hypothesis

The Disaster Evacuation model will show lower delivery ratios than Random Waypoint (due to directional movement reducing opportunistic contacts) but higher than Stationary Cluster (due to eventual convergence at safe zones). SWIM will perform similarly to Disaster Evacuation.

### 4.3 Parameters

| Parameter | Value |
|-----------|-------|
| Node count | 50 |
| Mobility models | RWP, Disaster Evacuation, Stationary Cluster, SWIM |
| Routing algorithm | PRoPHET (primary), Epidemic (P0 override) |
| Bundle mix | 30% P0, 20% P1, 20% P2, 30% P3 |
| Simulation duration | 86,400 s (24 hours) |

### 4.4 Disaster Evacuation Model Specification

The Disaster Evacuation model is implemented in `tools/sim/mobility/disaster_evacuation.rs`:

```
- Initial node positions: randomly distributed in 2km × 2km area
- Obstacle grid: 20% of cells blocked (simulates building collapse)
- Safe zones: 3 designated areas at map edges
- Node behavior: move toward nearest safe zone using A* pathfinding
- Group formation: nodes within 10m for > 5 min form temporary groups
- Rest periods: nodes rest 10-30 min before resuming movement
- Speed: 0.5–3 km/h (slower than RWP due to debris)
```

### 4.5 Expected Results

| Mobility Model | P0 Delivery Ratio | P1 Delivery Ratio | Avg P0 Latency |
|---------------|------------------|------------------|----------------|
| Random Waypoint | 98–99% | 94–97% | 25–60 s |
| Disaster Evacuation | 94–97% | 88–93% | 60–180 s |
| Stationary Cluster | 80–88% | 70–80% | 300–600 s |
| SWIM | 92–96% | 85–92% | 90–240 s |

**Expected finding:** IRIS achieves acceptable P0 delivery ratios even in Disaster Evacuation model, but Stationary Cluster (relevant for remote shelters) shows significant degradation, motivating the LoRa long-range transport for inter-cluster links.

### 4.6 Actual Results

*Pending simulation run.*

```
Status: NOT YET RUN
Scheduled: v0.4 milestone
```

---

## 5. EXP-ROUTE-004: Duty Cycle Constraint Effect on LoRa Routing

### 5.1 Objective

Quantify the impact of the 1% duty cycle constraint on LoRa-based routing, specifically for inter-cluster long-range links in sparse networks. Determine the effective throughput envelope and its impact on delivery ratios.

### 5.2 Background

At 865 MHz with SF12 (maximum range, minimum data rate), LoRa achieves approximately:
- Data rate: ~250 bps
- Max airtime per 100 seconds: 1 second
- Max throughput sustained: ~2.5 bytes/second

A P0 SOS bundle at 256 bytes takes ~100 seconds to transmit, consuming the full 100-second duty cycle allowance. This severely limits LoRa's ability to handle multiple queued messages.

### 5.3 Parameters

| Parameter | Value |
|-----------|-------|
| Network topology | 5 clusters, each 10 nodes, LoRa inter-cluster links only |
| LoRa parameters | SF12, BW 125 kHz, CR 4/5, 865 MHz |
| Duty cycle | 1% (1 second per 100 seconds) |
| Intra-cluster transport | BLE (50m range) |
| Bundle generation | 0.5 P0/min per cluster, 1 P1/min per cluster |
| Cluster separation | 2–15 km |
| Simulation duration | 8 hours |

### 5.4 Airtime Budget Model

```
LoRa airtime per message (SF12, 256 bytes):
  Preamble: 8 symbols × symbol_time
  Header: 20 bytes
  Payload: 256 bytes
  Time-on-Air: ~1.9 seconds at SF12, BW=125kHz
  
Duty cycle budget: 1 second per 100 seconds
→ One 256-byte message every 190 seconds (~3 min)
→ 20 messages per hour on the LoRa link

With 5 clusters each generating 0.5 P0/min = 2.5 P0/min network-wide
= 150 P0/hour, but LoRa inter-cluster handles subset of traffic
```

### 5.5 Expected Results

| Metric | Value |
|--------|-------|
| Max sustainable P0 rate via LoRa | ~15 msgs/hour/link |
| P0 delivery ratio, low load (< 10 msgs/hour) | 95–98% |
| P0 delivery ratio, high load (> 20 msgs/hour) | 70–80% |
| Queue buildup time to saturation at 30 msgs/hour | ~45 minutes |
| P1 delivery ratio at saturation | 40–60% |

**Expected finding:** LoRa duty cycle is the primary bottleneck in multi-cluster scenarios. Prioritization (P0 gets 60% of airtime budget) is critical. Satellite fallback for inter-cluster communication at high load is justified by these numbers.

### 5.6 Priority-Based Airtime Allocation Test

A secondary measurement within EXP-ROUTE-004 tests the effect of the priority-based airtime allocation rule (P0: 60%, P1: 25%, P2: 10%, P3–P7: 5%):

**Scenario:** Mixed load: 10 P0/hour, 20 P1/hour, 30 P2/hour on single LoRa link.

Expected: P0 delivery ratio > 95%, P1 > 80%, P2 > 60%, even under total load exceeding LoRa capacity.

### 5.7 Actual Results

*Pending simulation run.*

```
Status: NOT YET RUN
Scheduled: v0.4 milestone
Dependencies: LoRa duty cycle enforcement implementation (OI-ROUTE-002)
Runner: tools/sim/run_experiment.sh --exp EXP-ROUTE-004 --lora-sf 12
```

---

## 6. Results Summary (When Complete)

This table will be updated as experiments complete:

| Experiment | Status | Key Finding | Recommendation |
|------------|--------|-------------|----------------|
| EXP-ROUTE-001 | Pending | — | — |
| EXP-ROUTE-002 | Pending | — | — |
| EXP-ROUTE-003 | Pending | — | — |
| EXP-ROUTE-004 | Pending | — | — |

---

## 7. Reproducing Experiments

```bash
# Build simulator
cargo build --release -p iris-sim

# Run specific experiment
./tools/sim/run_experiment.sh --exp EXP-ROUTE-001 --repetitions 30

# Run all experiments (long-running, use CI)
./tools/sim/run_all_experiments.sh

# Generate report
./tools/sim/generate_report.sh --output docs/routing/experiment_results/
```

Experiment configurations are defined in `tools/sim/experiments/` as TOML files. Each configuration file pins all random seeds for reproducibility.
