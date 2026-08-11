# Intelligence Architecture

## Overview

IRIS routing and resource management improve over time through intelligence — algorithms
that learn from observed network conditions and optimize decisions. Intelligence is layered:
simpler, more reliable layers at the foundation; more complex, more powerful layers on top.
The system degrades gracefully when higher layers are unavailable or have insufficient data.

## Design Rules

**Rule 1: Lower layers must work without higher layers.**
L0 is the only required layer. L1-L5 are optional optimizations. A node running only L0
is a valid IRIS node. L0 must handle every case that L1-L5 would handle.

**Rule 2: Intelligence must not increase message delivery latency.**
All intelligence operations run off the critical path. Routing decisions may be informed
by intelligence results from a previous cycle, but never block on intelligence computation.
If intelligence is not ready: use the previous result or fall back to L0 default.

**Rule 3: Measurable improvement required before production activation.**
Each layer must demonstrate ≥5% improvement in delivery ratio, ≥10% latency improvement,
or ≥10% battery efficiency improvement over the layer below it, measured in controlled
simulation and physical tests. No layer is activated in production without this evidence.

**Rule 4: L5 (LLM) is an engineering tool, not a user-facing feature.**
Large language model capabilities are used for engineering (protocol design assistance,
anomaly investigation, test generation) — never in the message processing path.

## Intelligence Layers

### L0: Deterministic (Always Present)

The algorithmic foundation. No learning, no state beyond what protocol provides.
Pure functions: given the same inputs, always produces the same outputs.

**Algorithms:**
- Direct delivery (check neighbor list for direct contact)
- Hop-limited flood (send to all neighbors within hop limit)
- Shortest path (Dijkstra's algorithm when topology is known)
- TTL enforcement (discard expired messages)
- Priority queue management (priority-ordered queue with O(log n) insert/extract)
- Message deduplication (Bloom filter + LRU)
- Storage eviction (sort by priority + TTL + timestamp, evict lowest score)

**Compute budget:** <1ms per routing decision. Typically <100μs.

**Emergency mode:** L0 exclusively used. L1+ disabled to conserve compute.

### L1: Graph Algorithms

Operates on the contact graph (who has been in contact with whom, recently).

**Algorithms:**
- Connected component detection (which nodes are currently reachable)
- Centrality calculation (which nodes are the best relays by betweenness centrality)
- Community detection (identify clusters for community-based forwarding)
- Bridge node identification (nodes connecting otherwise separate clusters)

**Data required:** contact history (last 7 days of observed meetings between nodes)

**Compute budget:** <10ms per graph update cycle. Updates on graph change, not per-message.

**Value proposition:** identifies "important" relay nodes that should carry messages preferentially.
A central relay node that connects two communities is more valuable than a peripheral node.

### L2: Statistical Prediction

Predicts future network behavior from historical statistics.

**Algorithms:**
- Contact probability P(a,b,T): probability node a meets node b within next T minutes
- Delivery probability P(h→d): probability that routing via next-hop h delivers to destination d
- Link quality prediction: predict link survival probability from signal strength trend
- Congestion prediction: predict queue overflow from queue depth trend
- Battery prediction: estimate battery lifetime from drain rate

**Data required:** contact history, link quality history, queue depth history (last 7 days)

**Compute budget:** <5ms per prediction. Batch-computed for all likely routing decisions.

**Model:** Exponential weighted moving average (EWMA) for smoothing:
```
P_new = α × P_observed + (1 - α) × P_previous
  where α = 0.1 (weights recent observations more than old)
```

All computations in fixed-point arithmetic (no floating point) for embedded compatibility.

### L3: Machine Learning

On-device ML models for routing optimization. Requires measurable improvement over L2.

**Candidate tasks:**
1. Delivery time estimation (regression)
2. Gateway availability prediction (binary classification)
3. Mobility pattern recognition (clustering)

**Model constraints:**
- Size: <5MB per model (on-device storage)
- Inference: <50ms (off critical path, but must not significantly delay background work)
- No Internet required: fully local inference
- No GPU required: CPU inference, ONNX Runtime (ort crate for Rust)

**Training:**
- Offline: on collected routing telemetry (user-opted-in, anonymized)
- Federated (future): on-device training, aggregated updates without raw data sharing
- Update mechanism: model updates via IRIS message when Internet available

**Activation requirement:** A/B test showing ≥5% delivery ratio improvement over L2
baseline, validated in both simulation and 50-node physical deployment.

### L4: Reinforcement Learning

Adaptive routing policy learning from outcomes.

**Use case:** learn optimal routing strategy for the specific network topology and
usage pattern of a deployed IRIS community (village, NGO, enterprise).

**Algorithm:** Contextual bandit (not full RL — too slow for convergence in sparse networks)
- Context: network state (density, SOS count, battery levels, connectivity)
- Actions: routing algorithm selection, relay selection, storage eviction policy
- Reward: message delivery rate, latency, battery efficiency

**Training:** offline only. Policy updates applied between network operating sessions.
Never trained in real-time during emergency.

**Status:** Research phase. Not yet validated. Not planned for any production release
before v1.0.

### L5: LLM / Operator Intelligence

Large language models as engineering assistance tools.

**Uses:**
- Protocol design assistance (reviewing new message type proposals)
- Anomaly investigation (explain unusual routing patterns from logs)
- Test case generation (suggest edge cases for fuzz corpus)
- Incident response assistance (analyze telemetry during P0 incidents)

**Non-uses:**
- NOT in the message routing path (ever)
- NOT for user-facing features (IRIS users do not interact with LLM)
- NOT for content moderation (IRIS does not inspect message content)

**Implementation note:** L5 is an offline engineering tool. It accesses routing telemetry
(anonymized) and code, not user messages. Separate from production IRIS codebase.

## Layer Interaction

```
Message to route:

→ L0: Basic routing decision (always available)
   ↑
→ L2: Is delivery probability better via this next-hop? (if history available)
   ↑  
→ L1: Is this next-hop a bridge node? Should we avoid single points of failure? 
   ↑
→ L3: Model predicts better delivery via alternate next-hop? (if model available)

Final routing decision = L0 decision modified by L1-L3 signals
                       = L0 decision if L1-L3 unavailable
```

Each higher layer provides an adjustment signal, not a replacement decision.
L0's routing table is always the safety net.

## Compute Budget Enforcement

```rust
pub struct IntelligenceBudget {
    // Per-message routing: must not block delivery
    pub max_routing_decision_ms: u64,   // 1ms
    
    // Background intelligence cycles
    pub max_l1_cycle_ms: u64,           // 10ms per cycle
    pub max_l2_cycle_ms: u64,           // 5ms per prediction
    pub max_l3_inference_ms: u64,       // 50ms per inference
    
    // CPU budget fraction (don't hog processor)
    pub max_background_cpu_fraction: f32, // 0.05 (5% of one core)
}
```

Intelligence cycles are run on a dedicated background thread. If a cycle takes longer
than budget: log a warning, complete the cycle, but increase the interval before next cycle.

## Current Status and Roadmap

| Layer | Status | Release Target |
|-------|--------|---------------|
| L0 | ✅ Implemented | v0.1 (current) |
| L1 | 🔄 Design complete, implementing | v0.2 |
| L2 | 🔄 Design complete, implementing | v0.2 |
| L3 | 📋 Research phase, no implementation | v0.4+ |
| L4 | 📋 Research phase | Post-v1.0 |
| L5 | 🔧 Available as engineering tool | N/A (not user-facing) |
