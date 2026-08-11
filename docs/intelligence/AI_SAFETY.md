# AI Safety — Constraints and Governance for AI Systems in IRIS

**Component:** All AI/ML subsystems
**Status:** Policy v1.0 — Mandatory for all AI feature development
**Last Updated:** 2026-08-11

---

## 1. Purpose and Scope

This document defines the safety constraints, governance rules, and adversarial threat model for all AI/ML systems within IRIS. It applies to:

- ML routing model (gradient boosting, Q-learning)
- Graph intelligence analytics
- LLM integration (edge servers only)
- Any future AI component

These rules are **non-negotiable** design constraints. They take precedence over performance optimizations. A feature that improves delivery ratio by 10% but violates these constraints must not be deployed.

**Key principle:** In an emergency communication system, a predictable wrong answer is safer than an unpredictable correct one. AI is used to improve outcomes, not to automate life-safety decisions.

---

## 2. Priority 0 (SOS) Constraints

### 2.1 No Autonomous P0 Routing Decisions

AI systems must not autonomously route, delay, suppress, or prioritize P0 SOS messages. The P0 delivery path is:

```
P0 message received
  → Hard-coded priority queue insertion (no ML check)
  → Immediate transport selection (best available, deterministic)
  → Transmit
```

The ML routing model is **bypassed entirely** for P0 messages. The routing engine's P0 fast path is implemented in Rust with no external calls:

```rust
pub fn route_message(msg: &Message, transports: &TransportSet) -> RoutingDecision {
    if msg.priority == Priority::P0 {
        // SAFETY: No ML model call. No RL agent. No AI of any kind.
        return RoutingDecision::TransmitImmediate {
            transport: transports.best_available(),
            copies: msg.spray_l_value,
        };
    }
    // ... ML-assisted routing for P1-P7
}
```

This is enforced by code review policy: any change to the P0 routing path requires sign-off from two senior engineers and a documented safety review.

### 2.2 P1 (Medical) AI-Assisted with Confirmation

For P1 messages, the ML model may suggest a routing improvement, but:
- The suggestion is applied automatically **only** if the suggested relay has a PRoPHET delivery probability > 0.5
- If the ML suggestion conflicts with PRoPHET by more than 0.3 probability units, the PRoPHET estimate wins
- All P1 ML routing decisions are logged with feature values

### 2.3 Coordinator Override for P0–P1

A coordinator (authenticated with coordinator role) can force-route any P0 or P1 message via the coordinator dashboard:

```
POST /api/v1/coordinator/force-route
{
  "message_id": "msg-abc123",
  "target_node_id": "node-xyz",
  "reason": "direct_comms_established"
}
```

This override is logged and cannot be automated or scripted.

---

## 3. Prohibited AI Behaviors

The following behaviors are explicitly prohibited and must be prevented by design:

| Prohibited behavior | Risk |
|---|---|
| ML model that can drop or suppress any message | Message loss without human awareness |
| AI system that modifies message content | Integrity violation |
| Automated routing decisions for P0–P1 without human in loop | SOS non-delivery |
| AI system that routes based on sender identity or content topic | Discrimination, censorship |
| ML model trained on message content | Privacy violation |
| AI system that disables or circumvents encryption | Security breach |
| RL agent that learns to exploit other nodes' relay capacity | Selfish routing attack |

### 3.1 Design Enforcement

Prohibited behaviors are enforced at multiple layers:

1. **API boundary:** No write API from LLM to message store or routing engine
2. **Code review:** Checklist item in all AI-adjacent PRs (see `CONTRIBUTING.md`)
3. **CI gate:** Automated test verifies P0 path has no ML calls (`tests/safety/test_p0_path.rs`)
4. **Audit log:** All AI routing decisions logged — any suppression is detectable

---

## 4. Audit Trail

Every AI-influenced routing decision is recorded in the audit log with sufficient detail for post-hoc analysis.

### 4.1 Log Format

```json
{
  "timestamp": "2026-08-11T14:32:15.003Z",
  "event": "ml_routing_decision",
  "message_id_hash": "sha256:a3f...",
  "priority": "P2",
  "node_id_hash": "sha256:b7c...",
  "feature_values": {
    "contact_count_24h": 12,
    "prophet_dp": 0.67,
    "battery_level": 0.82,
    "queue_depth": 45,
    "hour_of_day": 14
  },
  "ml_score": 0.73,
  "prophet_score": 0.67,
  "action_taken": "RELAY_ACCEPT",
  "action_source": "ml_model",
  "model_version": "1.2.0"
}
```

**Privacy:** `message_id_hash` and `node_id_hash` are pseudonymized — the original values are not in the log. Audit logs are retained for 30 days locally, 1 year on edge servers.

### 4.2 Audit Log Integrity

Audit logs are append-only. Each entry includes a chained BLAKE3 hash of the previous entry, making tampering detectable:

```
entry_n.hash = BLAKE3(entry_{n-1}.hash || entry_n.content)
```

---

## 5. Human Override

### 5.1 User-Level Disable

Any user can disable ML routing on their device:

```
Settings → Advanced → Routing → "Use AI routing suggestions: OFF"
```

When disabled:
- Q-table and gradient boosting models are not loaded
- Routing falls back to PRoPHET
- Battery icon shows a small indicator (routing is slightly less efficient)
- Setting persists across restarts

### 5.2 Coordinator-Level Disable

A coordinator can disable ML routing network-wide for their managed nodes:

```
POST /api/v1/coordinator/config
{ "ml_routing_enabled": false, "scope": "all_managed_nodes" }
```

This broadcasts a signed configuration update. Nodes honor it within one contact cycle.

### 5.3 Emergency Fallback

If the routing engine detects an anomaly suggesting ML interference with P0 delivery (detection heuristic: P0 delivery ratio < 0.5 over 10-minute window), it automatically disables ML routing and raises a `CRITICAL` alert:

```
CRITICAL component=routing event=ml_safety_fallback
         reason="p0_delivery_ratio_below_threshold"
         p0_dr_10min=0.41 threshold=0.50
         action="ml_routing_disabled_emergency"
```

Manual re-enable requires coordinator action.

---

## 6. Adversarial ML Threat Model

### 6.1 Contact History Poisoning

**Attack:** A malicious node fabricates a contact history that makes the ML model assign it a high delivery probability, causing the network to route valuable messages to it (where they are dropped or intercepted).

**Mechanism:** The attacker broadcasts false capability beacons claiming high battery, low queue, many past contacts. The ML model sees these features and recommends the attacker as a relay.

**Mitigations:**
- Feature validation: `battery_level` claims inconsistent with observed TX behavior are flagged
- Multi-source validation: delivery probability is cross-checked against PRoPHET (which is based on observed contacts, not reported ones)
- Reputation decay: a node that fails to deliver messages it accepted gets its PRoPHET DP reduced rapidly
- Anomaly detection: nodes reporting top-decile features on all dimensions simultaneously are treated with skepticism (logged as `SUSPICIOUS_CAPABILITY_REPORT`)

### 6.2 Model Inversion Attack

**Attack:** An adversary queries the ML routing engine repeatedly with crafted feature vectors to infer the training data (which contains contact history).

**Mitigations:**
- ML model API is local-only (no network exposure)
- Inference rate limiting: max 100 routing decisions per second per calling process
- Feature vectors do not contain raw node IDs (pseudonymized)

### 6.3 Gradient Attack (Federated Learning)

**Attack:** A malicious node contributes poisoned gradient updates to the federated learning aggregation, shifting the global model toward malicious behavior.

**Mitigations:**
- Federated learning not yet in production (deferred to Beta)
- When implemented: Byzantine-robust aggregation (Krum or coordinate-wise median)
- Gradient norm clipping before aggregation
- Contributed gradients from low-reputation nodes are weighted down

### 6.4 RL Reward Hacking

**Attack:** An adversary manipulates the RL agent's observed reward signal (e.g., by falsely acknowledging message delivery) to train the agent toward suboptimal behavior.

**Mitigations:**
- Online fine-tuning disabled by default
- Delivery acknowledgments are cryptographically signed; unsigned ACKs are ignored
- Reward signal validation: delivery claimed without corresponding signed ACK is treated as undelivered

---

## 7. Model Governance

| Activity | Required approvers | Frequency |
|---|---|---|
| Deploy new model version | 2 engineers + safety review | Per release |
| Change reward function (RL) | 2 engineers + documented safety analysis | Per change |
| Enable online fine-tuning | Engineering lead + safety review | Per release |
| Change P0 routing path | 2 senior engineers + safety review | Per change |
| Disable human override | Not permitted | Never |

---

## 8. Incident Response for AI Failures

If an AI system is suspected of contributing to P0 message non-delivery:

1. **Immediate:** Disable ML routing network-wide (`ml_routing_enabled: false`)
2. **Within 1 hour:** Collect audit logs for the affected time window
3. **Within 4 hours:** Root cause analysis using audit log feature values
4. **Before re-enable:** Safety review of identified failure mode with documented fix

See `docs/operations/INCIDENT_MANAGEMENT.md` for the general incident response procedure.

---

## 9. References

- IRIS AI governance principles (this document)
- Audit log design: `docs/operations/LOGGING.md`
- ML model spec: `docs/intelligence/ML_LAYER.md`
- RL model spec: `docs/intelligence/RL_LAYER.md`
- Incident management: `docs/operations/INCIDENT_MANAGEMENT.md`
- Adversarial ML: Biggio & Roli, "Wild Patterns: Ten Years After the Rise of Adversarial Machine Learning", 2018
- Byzantine-robust aggregation: Blanchard et al., "Machine Learning with Adversaries: Byzantine Tolerant Gradient Descent", NeurIPS 2017
