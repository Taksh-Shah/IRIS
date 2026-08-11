# Simulation Architecture

## Overview

IRIS is a disruption-tolerant mesh network designed for environments where physical deployment is difficult, expensive, and potentially dangerous — disaster zones, remote areas, collapsed infrastructure. Before any hardware is involved, the protocol stack must be validated extensively in simulation. This document describes the simulation architecture that underpins all algorithm development, routing research, and performance validation.

## Why Simulate Before Physical Deployment

Physical testing of mesh networks at scale is impractical for several reasons:

**Cost and logistics.** Deploying 100+ physical devices with GPS coordination, RF isolation, and controlled scenarios requires facilities, personnel, and equipment budgets that are incompatible with rapid iteration cycles.

**Repeatability.** Physical tests suffer from environmental interference — RF noise, battery variance, ambient temperature. Identical experiments produce different results. Simulation gives deterministic reproducibility when needed, with controllable stochastic elements when randomness is desired.

**Failure scenario coverage.** Simulating "70% of nodes fail simultaneously" in a real deployment is not feasible. In simulation, arbitrary failure scenarios can be instantiated in milliseconds.

**Scale.** Physical testbeds rarely exceed 20-30 devices. Simulation can run 10,000 node scenarios. Algorithm behavior at scale often differs fundamentally from small-scale observations.

**Iteration speed.** A routing algorithm change takes 30 seconds to rebuild and deploy in simulation. Physical testing the same change takes hours of device preparation, test execution, log collection, and analysis.

**Safety.** Emergency communication protocols must work correctly when lives depend on them. Simulation allows exhaustive testing of edge cases — clock skew, message corruption, malicious nodes — that are difficult to stage safely in physical tests.

## Simulation Goals

### 1. Validate Routing Algorithms

The routing layer is the most complex part of IRIS. Multiple routing strategies are implemented (Epidemic, PRoPHET, spray-and-wait, geographic, hybrid). Simulation validates:

- Delivery ratio under various network topologies and mobility patterns
- Routing overhead (messages generated vs messages delivered)
- Convergence behavior as network partitions and merges
- Behavior at network boundaries (sparse vs dense transition)
- Performance with heterogeneous node capabilities

### 2. Measure Delivery Ratios

Key performance indicators validated in simulation before physical deployment:

| Scenario | Target Delivery Ratio | Latency Target |
|----------|----------------------|----------------|
| Dense urban, stable topology | >99% | <10s |
| Urban, moderate mobility | >95% | <60s |
| Sparse rural, occasional contact | >80% | <30min |
| Post-disaster (70% node failure) | >60% | <5min (P0 messages) |
| Isolated cluster, no gateway | >99% within cluster | <30s within cluster |

### 3. Test Failure Scenarios

Simulation exercises failure modes that would be difficult or dangerous to reproduce physically:

- Simultaneous node failure (earthquake model: 70% instant failure)
- Cascading gateway failure with failover
- Network partition with eventual merge
- Battery exhaustion cascade
- Message storm during panic (SOS bursts from many nodes simultaneously)
- Malicious node injection

### 4. Calibrate ML Models

IRIS uses machine learning for routing decisions — specifically, a contact prediction model that estimates the probability of encountering a given node within a time window. This model must be trained on realistic mobility data.

Simulation generates:
- Synthetic contact traces for ML training
- Ground truth labels (did the node actually deliver the message via that contact?)
- Feature sets (contact history, mobility pattern, time of day)
- Validation scenarios where ML routing is compared against baseline algorithms

### 5. Protocol Correctness Verification

Simulation provides an environment to verify protocol properties:

- No message duplication after deduplication
- Message ordering within priority class
- TTL decrements correctly across hops
- Signature verification at each hop
- Replay attack prevention

## Simulation Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    Simulation Controller                     │
│              (Python orchestration layer)                    │
│   - Scenario loading    - Result collection                  │
│   - Parameter sweeps    - Visualization                      │
├─────────────────────────────────────────────────────────────┤
│                    Event Engine                              │
│           (Rust discrete event simulator)                    │
│   - Event queue (priority heap)                              │
│   - Simulated clock                                          │
│   - Event dispatch                                           │
├──────────────────┬──────────────────────────────────────────┤
│   Node Model     │           Link Model                      │
│   - State        │   - Bandwidth                            │
│   - Buffers      │   - Latency                              │
│   - Battery      │   - Loss rate                            │
│   - Mobility     │   - Range                                │
├──────────────────┴──────────────────────────────────────────┤
│              Transport Simulation Layer                      │
│   BLESim │ WiFiDirectSim │ LoRaSim │ SatelliteSim           │
│   (All implement TransportTrait — same as production)        │
├─────────────────────────────────────────────────────────────┤
│               Iris Core (Production Code)                    │
│   iris-core routing engine runs UNCHANGED in simulation      │
│   iris-transport abstract trait                              │
│   iris-storage (in-memory backend for simulation)            │
│   iris-crypto (real crypto, not mocked)                      │
└─────────────────────────────────────────────────────────────┘
```

### Critical Design Principle: Same Core Code

The production Rust core (`iris-core`) runs **unchanged** in simulation. Transport implementations are swapped — `BLETransport` is replaced by `BLESimTransport` — but the routing engine, message engine, storage layer, and crypto layer are identical between simulation and production.

This eliminates an entire class of simulation-reality gaps. The simulation does not model what the code *would* do; it runs the actual code.

```rust
// Production transport (simplified)
pub struct BLETransport {
    adapter: BluetoothAdapter,
    // ...
}

// Simulation transport — same trait, different implementation
pub struct BLESimTransport {
    sim_clock: SimClock,
    link_model: BLELinkModel,
    event_queue: Arc<EventQueue>,
}

// Both implement the same trait
impl Transport for BLESimTransport {
    async fn scan(&self) -> Result<Vec<PeerId>> { /* simulated scan */ }
    async fn send(&self, peer: PeerId, data: Bytes) -> Result<()> { /* simulated send */ }
    // ...
}
```

## Abstract Transport Layer

The abstract transport trait is the key interface:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    fn transport_type(&self) -> TransportType;
    fn capabilities(&self) -> TransportCapabilities;

    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;

    async fn discover_peers(&self) -> Result<Vec<PeerInfo>>;
    async fn connect(&self, peer: &PeerInfo) -> Result<Connection>;
    async fn listen(&self) -> Result<Box<dyn Stream<Item = IncomingConnection>>>;

    fn bandwidth_estimate(&self) -> Bandwidth;
    fn latency_estimate(&self) -> Duration;
    fn is_available(&self) -> bool;
}
```

Simulation transports implement this trait with parameters driven by the link model rather than real radio hardware.

## Mobility Models (Pluggable)

Mobility is separated from the event engine. Each simulation scenario specifies a mobility model:

```python
# Scenario configuration
scenario = SimScenario(
    duration=3600,  # seconds
    nodes=[
        NodeGroup(count=50, mobility=RandomWaypointModel(area_km2=1.0, speed_mps=(0.5, 1.5))),
        NodeGroup(count=5, mobility=EmergencyResponderModel(base_coords=(28.6, 77.2))),
        NodeGroup(count=3, mobility=StaticModel(positions=[(28.601, 77.201), ...])),
    ],
    transports=[BLESim(), WiFiDirectSim(), LoRaSim()],
    traffic=DisasterTrafficModel(sos_rate=0.1, chat_rate=2.0),
    failures=EarthquakeFailureModel(failure_fraction=0.7, at_time=300),
)
```

## Message Injection

The traffic model controls how messages enter the simulation:

```rust
pub trait TrafficModel: Send + Sync {
    fn next_message(&mut self, sim_time: f64, nodes: &[NodeState]) -> Option<SimMessage>;
}

pub struct SimMessage {
    pub src: NodeId,
    pub dst: MessageDestination,  // Unicast, Broadcast, or Group
    pub priority: Priority,
    pub size_bytes: usize,
    pub injected_at: f64,
}
```

## Result Collection

Every simulation run emits structured metrics:

```rust
pub struct SimResult {
    pub scenario_id: String,
    pub duration_sim_seconds: f64,
    pub duration_wall_seconds: f64,
    pub node_count: usize,

    // Delivery metrics
    pub messages_injected: usize,
    pub messages_delivered: usize,
    pub messages_expired: usize,
    pub delivery_ratio: f64,
    pub mean_delivery_latency_s: f64,
    pub p50_latency_s: f64,
    pub p95_latency_s: f64,
    pub p99_latency_s: f64,

    // Overhead metrics
    pub total_transmissions: usize,
    pub overhead_ratio: f64,  // transmissions / delivered

    // Per-transport breakdown
    pub transport_stats: HashMap<TransportType, TransportStats>,

    // Node-level stats
    pub node_stats: Vec<NodeStats>,

    // Raw event log (optional, large)
    pub event_log: Option<Vec<SimEvent>>,
}
```

Results are written to a SQLite database for analysis:

```bash
# Run simulation
iris-sim run --scenario scenarios/earthquake_delhi.yaml --output results/

# Analyze results
python tools/analysis/delivery_analysis.py results/earthquake_delhi_20240101.db

# Plot delivery ratio vs time
python tools/viz/delivery_timeline.py results/earthquake_delhi_20240101.db
```

## Time Control

Three time control modes:

**Real-time mode**: Simulation events fire at wall-clock rate. Useful for integration tests that involve actual OS APIs (e.g., testing the real BLE stack against the simulator's message injection).

**Accelerated mode** (default for algorithm testing): Simulation runs as fast as possible. A 1-hour scenario completes in seconds. Used for parameter sweeps.

**Stepped mode**: Simulation pauses at each event, allowing interactive inspection. Useful for debugging specific routing behaviors.

```rust
pub enum TimeControl {
    RealTime,
    Accelerated { max_speed: Option<f64> },  // None = unlimited
    Stepped { breakpoints: Vec<SimTime> },
}
```

## Simulation vs Reality Gap Management

Simulation will never perfectly capture reality. Known gaps and mitigation strategies:

### RF Propagation Modeling

**Gap**: Real radio propagation is affected by multipath fading, obstacles, interference, and human bodies in ways that log-distance path loss models underestimate.

**Mitigation**: 
- Use pessimistic link models (assume 20% worse range than theoretical)
- Run validation experiments comparing simulated vs real BLE contact traces
- Apply correction factors derived from validation (see `SIMULATION_VALIDATION.md`)

### Mobility Model Accuracy

**Gap**: Real human mobility follows complex patterns (Levy flights, social clustering) that random waypoint models miss.

**Mitigation**:
- Use GPS traces from real pedestrian datasets (GeoLife, MDC dataset) to calibrate mobility
- Implement India-specific mobility patterns based on field observation
- Validate contact duration distributions against real traces

### Protocol Stack Differences

**Gap**: Real OS scheduling, interrupt latency, and power management affect timing in ways that simulation ignores.

**Mitigation**:
- Add jitter parameters to all simulated operations
- Model OS scheduling delays explicitly (BLE scan gaps, wakeup latency)
- Fuzz timing in simulation to expose timing-sensitive bugs

### Battery Model Accuracy

**Gap**: Battery behavior depends on temperature, age, and discharge history in complex ways.

**Mitigation**:
- Use conservative battery consumption estimates
- Test battery-triggered behavior (LOW_BATTERY, CRITICAL modes) in physical tests specifically

### Acceptable Error Bounds

Per `SIMULATION_VALIDATION.md`:
- Delivery ratio: simulation vs physical within ±10%
- Mean latency: simulation vs physical within ±20%
- Contact duration: simulation vs physical within ±30%

Values outside these bounds trigger simulation recalibration before relying on results.

## Simulation Infrastructure

### Directory Structure

```
iris/
├── crates/iris-sim/          # Rust simulation engine
│   ├── src/
│   │   ├── engine.rs         # Discrete event engine
│   │   ├── node.rs           # Node model
│   │   ├── link.rs           # Link model
│   │   ├── transports/       # Simulated transports
│   │   │   ├── ble.rs
│   │   │   ├── wifi_direct.rs
│   │   │   ├── lora.rs
│   │   │   └── satellite.rs
│   │   ├── mobility/         # Mobility models
│   │   ├── traffic/          # Traffic models
│   │   ├── failures/         # Failure models
│   │   └── metrics.rs        # Result collection
├── tools/
│   ├── simulation/           # Python orchestration
│   │   ├── scenarios/        # YAML scenario files
│   │   ├── runner.py         # Batch scenario runner
│   │   └── sweep.py          # Parameter sweep tooling
│   ├── analysis/             # Result analysis
│   └── viz/                  # Visualization
```

### Running Simulations

```bash
# Single scenario
cargo run --release --bin iris-sim -- run --scenario earthquake_delhi

# Parameter sweep (routing algorithm comparison)
python tools/simulation/sweep.py \
  --scenario base_urban \
  --param routing_algorithm epidemic,prophet,spray_wait \
  --param node_count 50,100,500 \
  --output sweep_results/

# CI smoke test (fast scenarios only)
cargo test --package iris-sim -- --test-threads=4
```

## Continuous Simulation in CI

Every PR triggers a baseline simulation suite:

1. **Correctness suite** (2 min): small scenarios verifying protocol correctness properties
2. **Performance regression suite** (5 min): compare delivery ratio against baseline, fail if >5% regression
3. **Failure scenario suite** (10 min): earthquake, partition, gateway failure scenarios
4. **Nightly extended suite** (2 hours): full parameter sweeps, 10k node scenarios, ML model validation

Results are stored in the CI database and visualized in the engineering dashboard.
