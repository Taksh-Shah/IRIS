# Python Layer

**Status:** Approved  
**Last updated:** 2026-08-11  
**Owner:** Research / Engineering  

---

## 1. Role of Python in IRIS

Python is **not used in production code.** No IRIS production system (Android app, iOS app, gateway firmware, edge server) is written in Python.

Python serves four specific roles in the IRIS project:

| Role | Description |
|------|-------------|
| Simulation | DTN routing simulations, mobility modeling, ONE simulator wrappers |
| ML training | Feature engineering, model training for experimental routing experiments |
| Test tooling | Test data generators, protocol corpus builders, benchmark harnesses |
| Research scripts | One-off analysis scripts for experiment data; plotting; statistics |

This separation is deliberate. Python's GIL, garbage collector, and lack of memory safety make it unsuitable for the IRIS production stack. But Python's scientific ecosystem (NumPy, pandas, scikit-learn, PyTorch, matplotlib) makes it ideal for research and offline analysis.

---

## 2. Package and Environment Management

### 2.1 Package Manager: uv

IRIS uses [uv](https://github.com/astral-sh/uv) for Python package management across all tools. `uv` is a Rust-based Python package manager and installer that replaces `pip`, `virtualenv`, and `pyenv` with a single, fast tool.

```bash
# Install uv (one-time)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create project environment
cd tools/iris-sim
uv sync          # installs from pyproject.toml + uv.lock

# Run a simulation
uv run python experiments/exp_route_001.py

# Add a dependency
uv add scikit-learn
```

### 2.2 Python Version

Minimum supported: Python 3.11 (required for `tomllib` standard library, `ExceptionGroup`, performance improvements to the dict implementation).

Target: Python 3.12+ for new tooling.

### 2.3 Project Structure

```
tools/
├── iris-sim/                    # Custom Python DTN simulator
│   ├── pyproject.toml           # uv dependencies
│   ├── uv.lock                  # Locked dependency versions
│   ├── iris_sim/                # Simulator library
│   │   ├── node.py              # Node state model
│   │   ├── routing/
│   │   │   ├── prophet.py       # PRoPHET implementation (mirrors Rust)
│   │   │   └── spray_wait.py    # Spray-and-Wait implementation
│   │   ├── mobility.py          # Mobility models (random waypoint, OSM-based)
│   │   ├── channel.py           # Channel model (BLE, LoRa, Wi-Fi path loss)
│   │   └── metrics.py           # Delivery ratio, overhead ratio, latency
│   ├── experiments/             # One file per EXP-* document
│   │   ├── exp_route_001.py     # PRoPHET vs Epidemic comparison
│   │   └── exp_route_002.py     # Spray-and-Wait L parameter sweep
│   └── tests/                   # pytest test suite for simulator correctness
│
├── one-simulator/               # ONE simulator integration
│   ├── scenarios/               # IRIS scenario configuration files (.txt)
│   ├── routing/                 # IRIS routing module (Java, for ONE)
│   └── scripts/                 # Python scripts to run ONE and parse results
│
├── ml/                          # ML training for routing experiments
│   ├── pyproject.toml
│   ├── data/                    # Training data (contact traces, delivery outcomes)
│   ├── features.py              # Feature engineering from contact history
│   ├── train.py                 # Model training (XGBoost, PyTorch)
│   ├── evaluate.py              # Model evaluation vs PRoPHET baseline
│   └── models/                  # Saved model artifacts (.pkl, .pt)
│
└── analysis/                    # Research analysis scripts
    ├── plot_experiment.py       # Standard plotting for experiment results
    ├── lora_link_budget.py      # LoRa link budget calculator
    └── battery_analysis.py     # Battery log analysis from EXP-BATTERY-*
```

---

## 3. Simulation Framework

### 3.1 Custom Python DTN Simulator (`iris-sim`)

The custom Python simulator is used for rapid parameter sweeps and protocol-layer experiments. It does not model the physical layer (radio propagation) — nodes either contact each other (with probability from a contact model) or do not.

**Design principles:**
- **Mirrors Rust implementation:** `iris_sim.routing.prophet.py` implements the same PRoPHET update rules as `crates/iris-core/src/routing/prophet.rs`. When the Rust implementation changes, the Python mirror is updated and the simulation re-run to validate.
- **Deterministic:** All simulations use a seeded RNG. The seed is recorded in experiment results for reproducibility.
- **Parameterized:** All experiment parameters are defined in TOML configuration files (Python reads them with `tomllib`); no magic constants in experiment scripts.

**Example experiment script pattern:**

```python
# experiments/exp_route_001.py
"""
EXP-ROUTE-001: PRoPHET delivery ratio vs epidemic in 100-node random waypoint scenario.
"""
import tomllib
from iris_sim import Network, Scenario
from iris_sim.routing import ProphetRouter, EpidemicRouter
from iris_sim.metrics import DeliveryMetrics

with open("configs/exp_route_001.toml", "rb") as f:
    cfg = tomllib.load(f)

for algorithm, RouterClass in [("prophet", ProphetRouter), ("epidemic", EpidemicRouter)]:
    results = []
    for seed in range(cfg["runs"]):
        net = Network(
            node_count=cfg["node_count"],
            scenario=Scenario.random_waypoint(seed=seed, **cfg["mobility"]),
            router=RouterClass(**cfg.get(algorithm, {})),
        )
        net.run(duration_s=cfg["duration_s"])
        results.append(DeliveryMetrics.from_network(net))
    
    # Write results to data/experiments/EXP-ROUTE-001/{algorithm}_results.csv
    DeliveryMetrics.write_csv(results, f"data/experiments/EXP-ROUTE-001/{algorithm}_results.csv")
```

### 3.2 ONE Simulator Integration

The Opportunistic Network Environment (ONE) simulator (Java-based) is used for higher-fidelity simulations that include physical-layer modeling and more sophisticated mobility models.

IRIS maintains:
- Custom routing modules for ONE (Java, implementing PRoPHET and Spray-and-Wait matching IRIS parameters)
- Scenario configuration files (`tools/one-simulator/scenarios/`) for Indian city mobility traces
- Python scripts to run ONE via subprocess, parse output, and produce analysis plots

ONE experiments are slower to run (each scenario: ~30 minutes on a 16-core workstation) but are the Level 3 evidence standard (see RESEARCH_METHODOLOGY.md).

---

## 4. ML Training (`tools/ml/`)

### 4.1 Purpose

Python ML tooling supports experimental research into ML-enhanced routing (Beta milestone, Month 18). The goal is to train a delivery probability prediction model that may outperform PRoPHET's hand-crafted update rules.

This is research, not production. The ML model, if it proves effective, will be:
1. Distilled into a simple lookup table or lightweight inference (no Python at runtime)
2. Re-implemented in Rust for production deployment
3. Validated against PRoPHET baseline in an A/B test

### 4.2 ML Libraries

| Library | Version | Purpose |
|---------|---------|---------|
| scikit-learn | 1.4+ | XGBoost wrapper, preprocessing, cross-validation |
| xgboost | 2.x | Gradient boosted trees for delivery probability prediction |
| PyTorch | 2.x | Reinforcement learning experiments (future) |
| pandas | 2.x | Contact trace data manipulation |
| numpy | 1.26+ | Numerical operations |
| matplotlib | 3.x | Result plotting |
| hypothesis | 6.x | Property-based testing of ML preprocessing pipeline |

### 4.3 Feature Engineering

Input features for the delivery probability model:

| Feature | Source | Type |
|---------|--------|------|
| Contact duration (normalized) | Contact history | float |
| Contact frequency (7-day) | Contact history | float |
| Time of day (sin/cos encoding) | Timestamp | float × 2 |
| Contact transport type | Contact event | categorical |
| Node mobility (speed estimate) | GPS delta if available | float |
| Destination contact recency | Contact history | float |
| PRoPHET P(a,b) value | Routing table | float |

The PRoPHET P value is included as a feature — the ML model learns when to trust PRoPHET and when to deviate.

### 4.4 Training Data

Training data is derived from ONE simulator contact traces (Level 3 evidence). Real field trial data (Level 5) will be incorporated after the Ahmedabad pilot.

Training data format: Parquet files in `tools/ml/data/` (not committed to Git for large datasets — use Git LFS or external storage for files >10 MB).

---

## 5. Test Tooling

### 5.1 pytest

All Python code in `tools/` is tested with pytest:

```bash
cd tools/iris-sim
uv run pytest tests/ -v --cov=iris_sim --cov-report=term-missing
```

Coverage target: ≥85% for `iris_sim` simulator code (routing algorithms must be thoroughly tested to be valid research tools).

### 5.2 Property-Based Testing (hypothesis)

The Python routing implementations (which mirror the Rust core) are property-tested with `hypothesis`:

```python
# tests/test_prophet.py
from hypothesis import given, strategies as st
from iris_sim.routing.prophet import ProphetState

@given(
    p_init=st.floats(min_value=0.1, max_value=0.9),
    gamma=st.floats(min_value=0.9, max_value=0.999),
    n_contacts=st.integers(min_value=0, max_value=1000),
)
def test_prophet_p_stays_in_unit_interval(p_init, gamma, n_contacts):
    """PRoPHET delivery probability must always be in [0, 1]."""
    state = ProphetState(p_init=p_init, gamma=gamma)
    for _ in range(n_contacts):
        state.contact("node_b")
        state.age(seconds=3600)
    assert 0.0 <= state.p("node_b") <= 1.0
```

### 5.3 Protocol Corpus Generation

`tools/analysis/corpus_gen.py` generates CBOR-encoded IRIS bundles for fuzzing the Rust parser:

```bash
uv run python tools/analysis/corpus_gen.py --output fuzz/corpus/bundle_parse/ --count 10000
```

This produces a diverse set of valid and edge-case bundles that seed the libFuzzer corpus for `cargo-fuzz`.

---

## 6. Research Scripts

One-off analysis scripts in `tools/analysis/` are not maintained as production code. They:
- Are documented with a comment block describing their purpose and the experiment they support
- Reference the EXP-* document they serve
- Produce plots saved to `data/experiments/EXP-*/figures/`
- Are committed to the repository so experiments are reproducible

```python
# tools/analysis/plot_experiment.py
"""Plot results for an EXP-* experiment.

Usage: uv run python tools/analysis/plot_experiment.py EXP-ROUTE-001
"""
```

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
