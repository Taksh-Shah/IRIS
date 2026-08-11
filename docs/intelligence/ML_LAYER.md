# ML Layer — Routing Intelligence

**Component:** `iris-ml` (Python training), `iris-core/ml` (Rust inference)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

The IRIS ML layer augments PRoPHET-based routing with learned contact predictions. The model runs entirely on-device — no network connectivity is required for inference. It improves delivery ratios in sparse, intermittently connected networks by predicting which encountered node is most likely to successfully relay a given message.

The ML layer is **advisory only**. It produces a ranked list of candidate relays. The core routing engine applies the result; the ML model itself cannot suppress or modify message content.

---

## 2. Problem Formulation

Given:
- A message `m` with priority `p` and TTL `t`
- A set of candidate relay nodes `N = {n1, n2, ..., nk}` currently in contact range
- Local contact history `H` (timestamped encounter log)

Output:
- Estimated delivery probability `P(delivery | relay = ni, m)` for each candidate
- Ranking of candidates, highest probability first

The routing engine uses this ranking alongside PRoPHET delivery estimates and current battery/queue state to make the final relay decision.

---

## 3. Feature Engineering

### 3.1 Contact History Features (per candidate node)

| Feature | Type | Description |
|---|---|---|
| `contact_count_1h` | int | Contacts in last 1 hour |
| `contact_count_24h` | int | Contacts in last 24 hours |
| `contact_count_7d` | int | Contacts in last 7 days |
| `avg_contact_duration_s` | float | Mean contact duration (seconds) |
| `last_contact_age_s` | float | Seconds since last contact |
| `inter_contact_mean_s` | float | Mean inter-contact time |
| `inter_contact_cv` | float | Coefficient of variation of inter-contact times |
| `prophet_dp` | float | Current PRoPHET delivery probability |

### 3.2 Temporal Features

| Feature | Type | Description |
|---|---|---|
| `hour_of_day` | int (0–23) | Current hour (sin/cos encoded) |
| `day_of_week` | int (0–6) | Day (sin/cos encoded) |
| `time_since_disaster_onset_h` | float | Hours since network entered disaster mode |

Cyclical encoding: `hour_sin = sin(2π × hour / 24)`, `hour_cos = cos(2π × hour / 24)`. This avoids discontinuity at midnight.

### 3.3 Node Type Features

| Feature | Type | Description |
|---|---|---|
| `node_type` | one-hot (5) | mobile_responder, edge_server, civilian, vehicle, fixed_relay |
| `has_lora` | bool | Node advertises LoRa capability |
| `has_satellite` | bool | Node has satellite uplink |
| `battery_level` | float (0–1) | Reported battery level (from capability beacon) |
| `queue_utilization` | float (0–1) | Reported queue fill fraction |

### 3.4 Message Features

| Feature | Type | Description |
|---|---|---|
| `priority` | int (0–7) | Message priority class |
| `ttl_remaining_s` | float | Remaining TTL in seconds |
| `size_bytes` | int | Message payload size |
| `hop_count` | int | Hops already taken |

**Total feature vector dimension: ~28 features**

---

## 4. Model Architecture

### 4.1 Algorithm: Gradient Boosted Decision Trees

IRIS uses **XGBoost** (training) compiled to a portable format for inference. On-device inference runs via a custom Rust binding to the model serialized as a decision tree ensemble in a compact binary format.

**Rationale for gradient boosting over neural networks:**
- No GPU required — runs on ARM Cortex-A53 (Pi Zero 2W) and mobile CPUs
- Interpretable: feature importance is inspectable for audit/debugging
- Robust to missing features (XGBoost handles NaN natively)
- Model size target: **< 500 KB** (100 trees, max depth 6)

### 4.2 Target Variable

Binary classification: `delivery_success` (1 = message delivered within TTL, 0 = expired or dropped). Calibrated probability output via Platt scaling applied after training.

### 4.3 Training Configuration

```python
import xgboost as xgb

params = {
    "objective": "binary:logistic",
    "n_estimators": 100,
    "max_depth": 6,
    "learning_rate": 0.1,
    "subsample": 0.8,
    "colsample_bytree": 0.8,
    "min_child_weight": 5,
    "scale_pos_weight": 3.0,  # delivery success is minority class in sparse nets
    "tree_method": "hist",
    "device": "cpu",
    "seed": 42,
}
```

### 4.4 Model Export

```python
# Export to compact format readable by Rust inference engine
model.save_model("iris_routing_model.ubj")  # UBJSON, ~400 KB typical

# Also export feature names and normalization params
import json
meta = {
    "feature_names": FEATURE_NAMES,
    "feature_means": scaler.mean_.tolist(),
    "feature_stds": scaler.scale_.tolist(),
    "model_version": "1.0.0",
    "training_date": "2026-08-11",
}
with open("iris_model_meta.json", "w") as f:
    json.dump(meta, f)
```

---

## 5. Inference

### 5.1 Rust Inference Path

```
ContactEncountered
    → FeatureExtractor::build(contact_history, node_caps, message)
    → ModelInference::score(feature_vector)    // <10ms target
    → DeliveryProbability(f32)
    → RoutingDecision::rank_candidates(scores, prophet_dp, battery)
```

### 5.2 Latency Budget

| Step | Target latency |
|---|---|
| Feature extraction | < 3 ms |
| Model inference (100 trees) | < 5 ms |
| Candidate ranking (up to 20 nodes) | < 2 ms |
| **Total** | **< 10 ms** |

Measured on Cortex-A53 @1 GHz (Raspberry Pi Zero 2W). Android mid-range (Snapdragon 680) is 3–5× faster.

### 5.3 Inference Implementation Sketch (Rust)

```rust
pub struct RoutingMLModel {
    trees: Vec<DecisionTree>,
    feature_meta: FeatureMeta,
}

impl RoutingMLModel {
    pub fn score(&self, features: &FeatureVector) -> f32 {
        let raw = self.trees.iter()
            .map(|t| t.predict(&features.normalized))
            .sum::<f32>();
        sigmoid(raw) // calibrated probability
    }
}

pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}
```

---

## 6. Federated Learning Concept

IRIS does not currently run a production federated learning system. The design intent is:

1. **Local training:** Each node accumulates its own contact history → delivery outcome pairs. After 30 days or 10,000 contact events (whichever comes first), it trains a local model update using the global model as the starting point.
2. **Gradient aggregation:** When a node connects to an edge server or Internet gateway, it uploads gradient deltas (not raw contact data). The server aggregates gradients using FedAvg.
3. **Privacy:** Raw contact identifiers are hashed before storage. Node IDs in contact history are pseudonymized per 24-hour epoch.

**Current status:** Federated training is implemented in the Python simulation (`iris-sim/ml/federated.py`). On-device training is deferred to Beta release due to battery cost constraints.

---

## 7. Fallback Behavior

The ML model is **optional**. If unavailable or disabled, the routing engine falls back to PRoPHET delivery probability rankings. Fallback triggers:

- Model file missing or fails integrity check (BLAKE3 hash mismatch)
- Inference latency exceeds 50 ms (model disabled for remainder of session)
- User or coordinator disables ML routing via in-app toggle
- Device battery < 15% (ML disabled to save CPU cycles)

Fallback is logged at `INFO` level with reason code.

---

## 8. Model Updates

| Channel | Mechanism | Frequency |
|---|---|---|
| App update | Bundled in APK/IPA | On app release |
| OTA delta | Compressed model diff via gateway | When Internet available |
| Edge server push | Model file via sync protocol | When edge server in range |

Model version is checked at startup. If the deployed model is older than 90 days, a `WARN` log is emitted but routing continues normally.

---

## 9. Privacy Constraints

- Contact history stored locally only; never transmitted without explicit opt-in
- Node IDs in training data pseudonymized using rotating daily keys
- ML model training data never includes message content
- Inference inputs are transient; not persisted beyond the routing decision

---

## 10. References

- XGBoost documentation: https://xgboost.readthedocs.io/
- PRoPHET protocol: Lindgren et al., 2004
- Federated Learning: McMahan et al., "Communication-Efficient Learning of Deep Networks from Decentralized Data", 2017
- IRIS simulation dataset: `docs/simulation/SIMULATION_VALIDATION.md`
- Model evaluation criteria: `docs/intelligence/MODEL_EVALUATION.md`
