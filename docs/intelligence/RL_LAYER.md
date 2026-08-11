# RL Layer — Reinforcement Learning for Adaptive Routing

**Component:** `iris-ml/rl` (Python training), `iris-core/rl` (Rust inference)
**Status:** Design v1.0 — offline training only; online fine-tuning is experimental
**Last Updated:** 2026-08-11

---

## 1. Overview

The RL layer addresses routing decisions that gradient boosting cannot handle well: sequential decisions with delayed feedback, multi-hop relay chains, and resource allocation under uncertainty. Q-learning is used because:

- The Q-table (state × action) fits in < 1 MB even for discretized state spaces
- No backpropagation at inference time — lookup or linear interpolation only
- Debuggable: Q-values can be inspected directly to understand decision rationale
- Robust to missing features — action selection degrades gracefully

The RL agent is **advisory only**. It provides a recommended action; the routing engine enforces hard constraints (e.g., never drop P0 messages regardless of Q-value).

---

## 2. Markov Decision Process Formulation

### 2.1 State Space

The state is a tuple observed at each contact event:

```
s = (battery_level, queue_depth, contact_history_bin, time_bin, priority_pending)
```

| State component | Range | Discretization | Bins |
|---|---|---|---|
| `battery_level` | 0–100% | 20% intervals | 5 |
| `queue_depth` | 0–1000 messages | log scale | 5 |
| `contact_history_bin` | 0–1 (prophet_dp) | 0.2 intervals | 5 |
| `time_bin` | hour of day | 6-hour blocks | 4 |
| `priority_pending` | highest pending priority | P0–P7 | 8 |

**Total state space:** 5 × 5 × 5 × 4 × 8 = **4,000 states**

Q-table size: 4,000 states × 6 actions × 4 bytes (float32) = **96 KB**

### 2.2 Action Space

| Action ID | Action | Description |
|---|---|---|
| 0 | `RELAY_ACCEPT` | Accept relay request, forward message |
| 1 | `RELAY_REJECT` | Decline relay (queue full or poor delivery probability) |
| 2 | `TRANSPORT_BLE` | Prefer BLE for this relay |
| 3 | `TRANSPORT_WIFI` | Prefer Wi-Fi Direct for this relay |
| 4 | `TRANSPORT_LORA` | Prefer LoRa for this relay |
| 5 | `SPRAY_L_ADJUST` | Increase/decrease Spray-and-Wait L value |

Actions 2–4 are only available when the corresponding transport is present. Invalid actions receive a large negative reward during training.

**Note:** `SPRAY_L_ADJUST` encodes direction as part of state (current L-value), keeping the action binary.

### 2.3 Reward Function

```
R(s, a, s') = w_delivery × delivery_success
            - w_battery  × battery_cost(a)
            - w_drop     × message_dropped
            + w_priority × priority_bonus(priority)
            - w_latency  × latency_penalty(latency_s)
```

| Weight | Symbol | Value | Rationale |
|---|---|---|---|
| Delivery success | `w_delivery` | 10.0 | Primary objective |
| Battery cost | `w_battery` | 0.5 | Per-relay energy penalty (normalized) |
| Message dropped | `w_drop` | -20.0 | Heavy penalty for drops |
| Priority bonus | `w_priority` | 0–5 (by P0–P7) | P0 gives +5 bonus |
| Latency penalty | `w_latency` | 0.1 per minute | Penalizes slow delivery |

**Priority bonus mapping:**

```python
PRIORITY_BONUS = {
    Priority.P0: 5.0,  # SOS
    Priority.P1: 3.0,  # Medical
    Priority.P2: 2.0,  # Location
    Priority.P3: 1.5,  # Emergency text
    Priority.P4: 1.0,  # Normal
    Priority.P5: 0.5,
    Priority.P6: 0.2,
    Priority.P7: 0.0,
}
```

---

## 3. Q-Learning Algorithm

### 3.1 Update Rule

```
Q(s, a) ← Q(s, a) + α × [R + γ × max_a' Q(s', a') - Q(s, a)]
```

| Parameter | Symbol | Value | Notes |
|---|---|---|---|
| Learning rate | α | 0.1 | Decays to 0.01 over training |
| Discount factor | γ | 0.9 | Favors near-term delivery |
| Exploration (ε-greedy) | ε | 1.0 → 0.05 | Annealed over 100k episodes |

### 3.2 Training Loop (Python)

```python
import numpy as np
from iris_sim import MeshSimulator

Q = np.zeros((N_STATES, N_ACTIONS), dtype=np.float32)

def state_to_idx(battery, queue, contact_dp, hour, priority):
    b = int(battery / 20)      # 0-4
    q = min(int(np.log10(queue + 1)), 4)  # 0-4
    c = int(contact_dp / 0.2)  # 0-4
    t = hour // 6              # 0-3
    p = priority               # 0-7
    return b * 800 + q * 160 + c * 32 + t * 8 + p

sim = MeshSimulator(scenario="EVAL-003")  # disaster scenario
epsilon = 1.0

for episode in range(NUM_EPISODES):
    state = sim.reset()
    done = False
    while not done:
        s_idx = state_to_idx(*state)
        if np.random.rand() < epsilon:
            action = np.random.randint(N_ACTIONS)
        else:
            action = np.argmax(Q[s_idx])

        next_state, reward, done = sim.step(action)
        s_next_idx = state_to_idx(*next_state)

        Q[s_idx, action] += ALPHA * (
            reward + GAMMA * np.max(Q[s_next_idx]) - Q[s_idx, action]
        )
        state = next_state

    epsilon = max(0.05, epsilon * 0.9999)

np.save("iris_q_table.npy", Q)
```

### 3.3 Convergence Criteria

Training runs for a minimum of 500,000 episodes. Convergence is declared when the mean Q-value change over 10,000 episodes is < 0.001. Typical convergence: ~200,000 episodes on the disaster scenario mix.

---

## 4. Inference (Rust)

```rust
pub struct QAgent {
    table: Vec<f32>,   // flattened [N_STATES × N_ACTIONS]
    n_actions: usize,
}

impl QAgent {
    pub fn best_action(&self, state: &RLState) -> Action {
        let s_idx = state.to_index();
        let base = s_idx * self.n_actions;
        let q_values = &self.table[base..base + self.n_actions];
        let best = q_values.iter()
            .enumerate()
            .filter(|(a, _)| self.action_valid(*a, state))
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(a, _)| a)
            .unwrap_or(0);  // fallback: RELAY_ACCEPT
        Action::from(best)
    }
}
```

**Inference latency:** < 0.1 ms (table lookup + argmax over 6 values). Well within the 10 ms routing decision budget.

---

## 5. Safety Constraints

The RL agent operates under hard constraints enforced by the routing engine, not by the Q-table:

| Constraint | Enforcement |
|---|---|
| Never drop P0 (SOS) messages | `RELAY_REJECT` blocked for P0 regardless of Q-value |
| Never exceed queue hard limit | `RELAY_ACCEPT` blocked when queue is full |
| LoRa duty cycle respected | `TRANSPORT_LORA` blocked when duty cycle exhausted |
| Battery emergency mode | All relay actions blocked when battery < 5% |

When a high-Q action is blocked by a constraint, the agent falls back to the next highest valid Q-value action.

---

## 6. Online Fine-Tuning (Experimental)

Optional online fine-tuning allows the agent to adapt to observed contact patterns on deployed hardware. It is disabled by default and requires explicit user opt-in.

**Implementation constraints:**
- Fine-tuning runs only when battery > 50% and device is charging
- Maximum 1,000 Q-table updates per fine-tuning session
- Fine-tuning is sandboxed: a copy of the Q-table is updated; the original is not overwritten until the new table improves delivery ratio by > 2% in local validation
- Fine-tuned tables are never shared between nodes (privacy constraint)

---

## 7. Model Size and Storage

| Artifact | Size | Storage location |
|---|---|---|
| Q-table (`iris_q_table.npy`) | 96 KB | App bundle / internal storage |
| Feature normalization params | 2 KB | App bundle |
| Online fine-tune copy | 96 KB | Internal storage (optional) |
| **Total** | **~200 KB** | Well within 1 MB budget |

---

## 8. Evaluation

The RL agent is evaluated against PRoPHET baseline and ML-only baseline across the standard scenario suite (EVAL-001 through EVAL-005). See `docs/intelligence/MODEL_EVALUATION.md` for acceptance criteria.

Key metric: **delivery ratio improvement per unit battery overhead**.

Expected improvement (simulation): +3–8% delivery ratio for P0–P2, < 5% additional battery overhead.

---

## 9. Limitations and Known Issues

- **State discretization:** Continuous state variables are binned, causing boundary effects near bin edges. Mitigation: linear interpolation between adjacent Q-values (optional).
- **Non-stationary environment:** Disaster scenarios evolve rapidly; the pre-trained Q-table may be suboptimal in the first 30 minutes before the agent adapts (if online fine-tuning is enabled).
- **Multi-agent interference:** Q-learning assumes a stationary environment; multiple IRIS nodes simultaneously running RL creates non-stationarity. This is partially mitigated by each node treating others as part of the environment.

---

## 10. References

- Sutton & Barto, "Reinforcement Learning: An Introduction", 2nd ed. (2018)
- Delay-tolerant networking RL: Zhu et al., "RAPID: A Reinforcement Learning Model for Routing in Delay Tolerant Networks", 2019
- IRIS simulation scenarios: `docs/simulation/TRAFFIC_MODELS.md`
- AI safety constraints: `docs/intelligence/AI_SAFETY.md`
