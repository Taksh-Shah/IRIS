# Model Evaluation — ML Routing Model Assessment

**Component:** `iris-ml/eval` (Python), CI pipeline (`eval` stage)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

Every ML routing model (gradient boosting, Q-learning, future models) must pass a formal evaluation before deployment to production builds. Evaluation is automated and runs in CI. A model that fails any acceptance criterion is rejected; the previous production model continues in service.

Evaluation covers: routing performance, computational overhead, battery impact, and regression against baseline. It does not evaluate LLM models (see `docs/intelligence/LLM_LAYER.md`).

---

## 2. Evaluation Scenarios

### Scenario Registry

| ID | Name | Description | Node Count | Duration | Topology |
|---|---|---|---|---|---|
| EVAL-001 | Urban Dense | High-density urban scenario, Ahmedabad city center | 500 | 6 hours | Random waypoint |
| EVAL-002 | Rural Sparse | Low-density rural scenario, Kutch district | 50 | 12 hours | Lévy flight |
| EVAL-003 | Disaster Onset | Sudden network fragmentation event | 200 | 2 hours | Clustered + splits |
| EVAL-004 | Mixed Transports | BLE + LoRa + WiFi heterogeneous network | 150 | 8 hours | Grid + mobility |
| EVAL-005 | Network Partition | Sustained partition with 3 isolated clusters | 300 | 6 hours | Partitioned |

All scenarios use standardized traffic models from `docs/simulation/TRAFFIC_MODELS.md`.

### Scenario YAML Format

```yaml
scenario:
  id: EVAL-003
  name: "Disaster Onset"
  seed: 42
  duration_seconds: 7200
  nodes:
    count: 200
    type_distribution:
      mobile_responder: 0.60
      edge_server: 0.05
      civilian: 0.30
      vehicle: 0.05
  mobility:
    model: random_waypoint
    speed_mps: [0.5, 2.0]
    pause_s: [0, 120]
  traffic:
    model: disaster_onset
    p0_rate_per_hour: 10
    p3_rate_per_hour: 500
  events:
    - at_second: 900
      type: partition
      affected_fraction: 0.4
```

---

## 3. Metrics

### 3.1 Primary Metrics

| Metric | Symbol | Definition |
|---|---|---|
| Delivery ratio | DR | messages delivered / messages sent |
| Delivery ratio by priority | DR_p | DR restricted to priority class p |
| Mean delivery latency | L̄ | mean time from send to delivery (delivered messages only) |
| P95 delivery latency | L_95 | 95th percentile delivery latency |
| Battery overhead | ΔB | additional battery drain vs baseline (%) |

### 3.2 Secondary Metrics

| Metric | Definition |
|---|---|
| False positive relay rate | Fraction of relay accepts that did not result in delivery |
| Routing decision latency | Time from contact event to routing decision |
| Model inference failure rate | Fraction of routing decisions where model inference failed |

---

## 4. Baseline

The baseline for all comparisons is **PRoPHET routing** with default parameters:

```python
PROPHET_PARAMS = {
    "p_init": 0.75,
    "beta": 0.25,
    "gamma": 0.98,
    "aging_interval_s": 300,
}
```

Baseline is evaluated on each scenario with 5 independent random seeds. The mean and standard deviation of each metric are recorded.

---

## 5. Acceptance Thresholds

A model is accepted for deployment only if **all** of the following criteria pass:

### 5.1 Improvement Criteria

| Criterion | Threshold | Applies to |
|---|---|---|
| DR improvement vs baseline | > 5% | All scenarios combined |
| DR_P0 improvement vs baseline | > 0% | Must not regress P0 delivery |
| DR_P1 improvement vs baseline | > 0% | Must not regress P1 delivery |
| Mean latency vs baseline | ≤ +10% | Latency may not worsen significantly |

### 5.2 Overhead Criteria

| Criterion | Threshold |
|---|---|
| Battery overhead vs baseline | < 10% additional drain |
| Routing decision latency | < 10 ms (p99) |
| RAM overhead (model + inference) | < 10 MB additional |

### 5.3 Regression Criteria

The model must not underperform the baseline on any individual scenario:

```
For each scenario s in {EVAL-001, ..., EVAL-005}:
    DR(model, s) >= DR(baseline, s) - 0.02
```

The 2% tolerance allows for statistical variation. A scenario failure triggers a detailed investigation before the model can proceed.

---

## 6. A/B Testing Protocol

In simulation, A/B testing splits the node population 50/50:

- Group A nodes use ML-assisted routing
- Group B nodes use PRoPHET baseline

Both groups operate in the same simulated environment and compete for relays. This measures the **in-network** impact of mixed deployment (which will occur during rollout).

```python
def run_ab_test(scenario: Scenario, seed: int) -> ABTestResult:
    sim = MeshSimulator(scenario, seed=seed)
    
    for node in sim.nodes:
        if node.id % 2 == 0:
            node.routing = MLRoutingAgent(model_path="iris_model.ubj")
        else:
            node.routing = PRoPHETRoutingAgent(**PROPHET_PARAMS)
    
    result = sim.run()
    return ABTestResult(
        ml_dr=result.delivery_ratio(group="A"),
        baseline_dr=result.delivery_ratio(group="B"),
        ml_battery=result.battery_drain(group="A"),
        baseline_battery=result.battery_drain(group="B"),
    )
```

A/B results must show ML group DR ≥ baseline group DR − 1% (within noise) before progressive rollout proceeds.

---

## 7. Evaluation Pipeline

### 7.1 CI Trigger

Evaluation runs automatically when:
- A new model file is pushed to `models/` in the repository
- A routing algorithm change is made in `iris-core/src/routing/`
- Manually triggered via CI dispatch

### 7.2 Pipeline Stages

```
Stage 1: Scenario execution
  - Run each EVAL-001..EVAL-005 with 5 seeds each (25 simulation runs)
  - Parallelized: 5 runs per CPU core
  - Estimated wall time: 20 minutes on 4-core CI runner

Stage 2: Metric collection
  - Parse simulation logs
  - Compute DR, latency, battery metrics per scenario

Stage 3: Acceptance check
  - Compare against thresholds (Section 5)
  - Generate pass/fail report

Stage 4: A/B test
  - Run A/B comparison across all scenarios
  - 10 seeds each

Stage 5: Report generation
  - HTML report with metric tables and plots
  - Stored in CI artifacts for 90 days
```

### 7.3 Report Format

```
IRIS Model Evaluation Report
Model: iris_routing_model_v1.2.0.ubj
Evaluated: 2026-08-11 14:00 UTC
Baseline: PRoPHET v1.0

SCENARIO RESULTS
┌────────────┬──────────┬──────────┬──────────┬────────────┐
│ Scenario   │ DR (ML)  │ DR (Base)│ Δ DR     │ Pass?      │
├────────────┼──────────┼──────────┼──────────┼────────────┤
│ EVAL-001   │ 0.847    │ 0.801    │ +5.7%    │ PASS       │
│ EVAL-002   │ 0.623    │ 0.589    │ +5.8%    │ PASS       │
│ EVAL-003   │ 0.712    │ 0.674    │ +5.6%    │ PASS       │
│ EVAL-004   │ 0.891    │ 0.832    │ +7.1%    │ PASS       │
│ EVAL-005   │ 0.541    │ 0.513    │ +5.5%    │ PASS       │
└────────────┴──────────┴──────────┴──────────┴────────────┘

OVERHEAD: Battery +6.3% | RAM +4.2 MB | Latency p99: 8.1 ms
OVERALL: ACCEPTED
```

---

## 8. Evaluation Dataset Provenance

| Dataset element | Source |
|---|---|
| Node mobility traces | Synthetic (SUMO simulator, Ahmedabad OSM map) |
| Contact timing | Derived from mobility traces + radio propagation model |
| Message traffic | Parameterized Poisson/bursty models (see `TRAFFIC_MODELS.md`) |
| Ground truth delivery | Simulation oracle (all messages tracked end-to-end) |
| Battery drain model | Empirical measurements from IRIS prototype hardware |

No real incident data is used in evaluation scenarios due to privacy constraints on field data. If real incident traces become available under appropriate consent, they will be added as EVAL-006+.

---

## 9. Model Registry

Accepted models are stored in the model registry with their evaluation metadata:

```json
{
  "model_id": "iris_routing_model_v1.2.0",
  "file": "iris_routing_model_v1.2.0.ubj",
  "sha256": "deadbeef...",
  "evaluation_run": "CI-2026-0811-042",
  "overall_dr_improvement": 0.060,
  "battery_overhead": 0.063,
  "status": "production",
  "deployed": "2026-08-11",
  "deprecated": null
}
```

---

## 10. References

- Simulation framework: `docs/simulation/SIMULATION_VALIDATION.md`
- Traffic models: `docs/simulation/TRAFFIC_MODELS.md`
- ML model spec: `docs/intelligence/ML_LAYER.md`
- RL model spec: `docs/intelligence/RL_LAYER.md`
- PRoPHET protocol: Lindgren et al. (2004)
