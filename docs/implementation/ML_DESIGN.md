# ML-001 DESIGN — L3 Delivery-Probability & Gateway Prediction: Experimental Evaluation Plan

**Node**: ML-001 (P3 EXPERIMENT, deps ROUTE-002 + SIM-001 COMPLETE)
**Version**: 1.0
**Date**: 2026-08-14
**Status**: DESIGN (accepted for IMPLEMENT)
**Sources**: RES-0014 (SOTA, 2026-08-14); ROUTE2_DESIGN.md (L2 baseline);
  SIM_VERIFICATION.md (harness acceptance); ARCHITECTURAL_PRINCIPLES.md #5/#6

---

## 1. Scope & governing principles

ML-001 evaluates, inside the SIM-001 discrete-event harness, whether a
*learned* L3 predictor of (a) delivery probability and (b) gateway/egress
opportunity can match or beat the RFC 6693 PRoPHET v2 L2 baseline on
delivery-ratio and overhead — **and when it cannot**.

Non-negotiable constraints (from RES-0014 §4 + ARCHITECTURAL_PRINCIPLES):

1. **P3 experimental only.** No code path from L3 output to actual P0/P1
   forwarding. All L3 decisions are recorded as *shadow metrics* beside the
   L2 decisions that are actually taken.
2. **Training is offline, in-sim only.** No on-device training; no external
   ML runtime dependency (no tch, ndarray-backed nets, or Python). The L3
   predictor uses pure-Rust arithmetic over the in-band feature vocabulary.
3. **No new features or sensors.** ML-001 consumes features already produced
   by iris-core (DP tables, contact events, buffer state, hop budgets, gateway
   link state). No protocol changes, no wire-format changes.
4. **Determinism.** Same seed → same result anchor for both arms (the SIM-001
   `result_anchor` pattern). A/B comparisons are same-seed.
5. **Publishable at parity.** A null result (L3 ≈ L2) is a valid P3 outcome:
   it bounds *when* ML does not help, which is itself the experiment.

## 2. Baseline recap (L2, what L3 is compared against)

- **L2 = PRoPHET v2 (RFC 6693) + binary Spray-and-Wait (L=8)** implemented in
  `crates/iris-core/src/routing/prophet.rs` + `opportunistic.rs`; armed in the
  sim via `Simulation::with_opportunistic(ProphetConfig)`.
- DP is exchanged on every contact (`exchange_on_contact`, PRoPHET §5.3.3);
  forwarding uses GTMX+ with cold-start binary-spray fallback
  (`opportunistic_forward`, ROUTE2_VERIFICATION: L2 17/18 = 94.4%, overhead
  1.41 vs L0 3.18 → −55.6% overhead at parity).
- The L3 predictor does NOT replace this forwarding layer. It re-derives a
  *second* per-message "should this copy be handed to this neighbor" score
  from the same features and reports what the L3 decision *would have been*
  (shadow), plus its score vs the actual L2 GTMX+ outcome.

## 3. Candidate L3 predictors (both shadow-only)

### 3.1 Learned delivery-probability predictor (LP)

Online logistic-regression-style scorer over a fixed feature vector, trained
**offline** on a held-out slice of the sim trace, applied **online** during
the evaluation run. Per RES-0014, this mirrors the lightweight supervised
family (ML-MaxProp XGBoost, Dudukovich classifier, RL-PRoPHET) that the
literature reports as practical on constrained nodes — but we deliberately use
an interpretable linear model so IRIS can explain the result without a model
server (contra RouteNet-TGNN's central-controller framing).

Features `x` (all already in-band, per hop candidate `b` for recipient `r`):

| # | Feature | Source in iris-core |
|---|---------|--------------------|
| 1 | PRoPHET DP `P(b,r)` | `predictions().p_for(recipient)` |
| 2 | My DP `P(self,r)` | same table |
| 3 | Contact frequency (encounters with `b` in window) | DP table / contact log |
| 4 | Hop count of message so far | `hops_by_msg` |
| 5 | Buffer occupancy of `b` (bytes / cap) | `scf.usage_bytes()` / `max_storage_bytes` |
| 6 | Message age (now − timestamp) | virtual clock |
| 7 | TTL remaining fraction | `ttl_seconds` |
| 8 | Priority rank | `MessagePriority` |

Training target `y`: "delivered within TTL" for each relay decision in the
train slice (supervision comes from the *same* simulation's delivery log —
this is why the train/test split must be temporal, see §6).

Scoring: `score = σ(w·x)`; threshold at the point that maximizes delivery at
equal overhead to L2 on the train slice; applied as shadow during test.

### 3.2 Gateway-egress timing predictor (GT)

Predicts the next sim-time a *gateway-capable* node will be reachable (egress
opportunity), scored against GW-001's static quality weights. Feasible only in
scenarios where gateway contacts are periodic (vehicle_relay / community_ferry
/ new periodic-ferry). In random scenarios the predictor's expected behavior is
parity-or-worse — that is an explicit hypothesis, not a bug.

Feature vector: recent inter-contact interval for the gateway node, contact
periodicity estimate (mean/std of last intervals), link health state
(GW-001 `GatewayHealthState`), current sim time.

Shadow output: `egress_t̂` (predicted next gateway contact) + per-message
"hold for gateway vs relay now" recommendation; compared to GW-001's
static-weight selection for the same decision points.

## 4. Feature & score plumbing

To keep L3 strictly out of the critical path, the sim gains a *sidecar*:

- `src/sim/ml/features.rs` — `FeatureVec([f32; 8])`, built from the existing
  `SimNode`/`ScfEngine`/`OpportunisticRouter` accessors (no new mutable state
  on the forwarding path).
- `src/sim/ml/predictor.rs` — `LinearPredictor { w: [f32; 8], bias: f32 }`
  with `score(&FeatureVec) -> f32`; pure arithmetic, no allocations on the
  score path.
- `src/sim/ml/experiment.rs` — the shadow harness: given a train slice and a
  test slice, fits `w` (offline, simple gradient-descent on the training
  deliveries), then during the test run records `ShadowDecision { msg, node,
  l3_score, l2_gtmx_outcome }` for every contact where L2 made a relay/no-relay
  call.

The forwarding path (`Simulation::forward_to` / `opportunistic_forward`) is
**not modified** except for an optional `record_shadow` flag that appends to an
observer list. Default off → zero cost, byte-identical runs to ROUTE-002.

## 5. Scenario spectrum (periodic vs random — bounds the learning gain)

Per RES-0014 §2/§5, learned prediction helps where contacts are predictable.
The experiment spans both extremes:

| Scenario | Contact structure | Expected L3 edge |
|----------|-------------------|------------------|
| `dense_mesh` | all-pairs, direct | ≈ 0 (everything delivers; baseline ceiling) |
| `partition_carry` | 2 islands + ferry, 2 legs | small (few decisions) |
| `community_ferry` | 2 communities + periodic ferry (ROUTE-002 seed 7) | **positive** — L3 should match L2 delivery at ≤ overhead |
| `vehicle_relay` | line of stations + moving relay, periodic | positive for GT predictor |
| `periodic_ferry` *(new)* | dedicated scheduled gateway ferry | strongest GT edge (scheduled-DTN regime, RES-0014 §2) |
| `random_walk` *(new)* | contacts drawn from ChaCha8Rng (no periodicity) | parity-or-worse (bounds the no-help regime) |

`periodic_ferry` and `random_walk` are new scenario builders added in IMPLEMENT.

## 6. Determinism, statistical protocol & leakage guard

- **Same-seed A/B**: each scenario × seed runs twice — L2 arm and L2+L3-shadow
  arm — with identical contact/injection schedules. The L3 arm's actual
  forwarding is *identical* to the L2 arm (shadow), so any difference in
  measured delivery/overhead is by construction zero; the meaningful comparison
  is L3's *shadow* decisions vs L2's actual GTMX+ decisions (decision agreement
  + predicted-delivery accuracy).
- **Statistical protocol** (ML-MaxProp methodology, RES-0014 §5): ≥10 seeds per
  scenario; paired t-test on delivery ratio where normal, Wilcoxon signed-rank
  otherwise; report mean ± std.
- **Leakage guard**: the train slice is the **first half of the timeline**
  (early contacts/injections); the test slice is the **second half**. No
  per-destination held-out trick is needed because a contact schedule repeated
  for testing is the same distribution the predictor must generalize across —
  but the *fit* must never see test-window deliveries. Both slices are emitted
  from one `Simulation::run` pass; the predictor is frozen before test-time
  shadow decisions begin.

## 7. Metrics

Primary (both arms, per scenario): delivery ratio, overhead ratio, p50/p95
latency, loop-free — all existing `SimMetrics`.

L3-specific (shadow): 
- **Decision agreement** = fraction of relay/no-relay decisions where L3 agrees
  with L2 GTMX+ (calibration of the learned score vs PRoPHET DP).
- **Predicted-delivery accuracy** = Brier score of `σ(w·x)` against actual
  test-window delivery.
- **GT timing error** = |`egress_t̂` − actual next gateway contact|, compared to
  the error of the GW-001 static selection (which ignores timing).

## 8. Module layout

```
crates/iris-core/src/sim/ml/mod.rs        (types, ShadowDecision, observer)
crates/iris-core/src/sim/ml/features.rs   (FeatureVec builder — read-only)
crates/iris-core/src/sim/ml/predictor.rs  (LinearPredictor, fit/score)
crates/iris-core/src/sim/ml/experiment.rs (train/test split, shadow runner, metrics)
crates/iris-core/src/sim/scenario.rs      (+ periodic_ferry, random_walk)
crates/iris-core/tests/ml_experiments.rs  (integration tests, AC 1-6)
```

No changes to `Cargo.toml` dependencies (pure std; `rand` already present).

## 9. Acceptance criteria (for VERIFY)

- **AC-1** — Feature vector is built from in-band state only and is
  deterministic (same seed → identical `FeatureVec` sequence); no new fields on
  `SimNode`/`ScfEngine`/`OpportunisticRouter` that alter the forwarding path.
- **AC-2** — Shadow-only guarantee: with `record_shadow=false`, the L2+L3 run's
  `result_anchor`, delivery ratio, and overhead are byte-identical to the L2-only
  run (proves L3 never touches forwarding).
- **AC-3** — LP decision agreement with L2 GTMX+ ≥ 0.70 on `community_ferry`
  (seed 7) test window; LP Brier score reported for all scenarios.
- **AC-4** — GT predictor: on `periodic_ferry`, |egress_t̂ error| strictly below
  the GW-001 static-selection error; on `random_walk`, GT does NOT beat static
  selection (parity-or-worse — the no-help bound).
- **AC-5** — ≥10 seeds × both scenario regimes (periodic: community_ferry +
  periodic_ferry + vehicle_relay; random: random_walk) run reproducibly; results
  reported as mean ± std with paired test.
- **AC-6** — `cargo test --workspace` green (prior 225 + new ML tests), `cargo
  clippy --workspace --all-targets` 0 warnings.
- **AC-7** — `docs/implementation/ML_VERIFICATION.md` written with evidence:
  tables per scenario, decision-agreement, Brier, GT timing error, and the
  conclusion "when ML helps / when it does not."

## 10. Gaps / risks / out-of-scope

- **Synthetic traces only** — results do not transfer to real mobility without
  a real-contact dataset (RES-0014 §6; out of scope, BLK-0005).
- **Linear model ceiling** — an MLP might score higher than the logistic
  scorer; we deliberately ship the interpretable model first (P3, cheap).
- **No on-device inference** — the predictor is a sim artifact; distillation to
  on-node inference is a future work item tied to IDENT-001/BLK-0005.
- **Central-model SOTA not testable** — GNN/MARL/TGNN results (RouteNet-TGNN,
  lunar MARL) assume central knowledge or known contact plans; flagged
  non-transferable in RES-0014 and excluded here.
- **Gateway prediction is scenario-bound** — no gain expected under random
  mobility; the experiment documents the boundary rather than forcing it.

## 11. Next

IMPLEMENT in `crates/iris-core/src/sim/ml/` (features → predictor →
experiment) + new scenario builders; then TEST (`tests/ml_experiments.rs`,
AC 1-7), SECURITY_REVIEW (shadow-isolation re-check), VERIFY
(`docs/implementation/ML_VERIFICATION.md`), ACCEPT.
