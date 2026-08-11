# Research Methodology

**Status:** Approved  
**Last updated:** 2026-08-11  
**Owner:** Research Lead  

---

## 1. Purpose

This document defines how IRIS conducts and evaluates research. It establishes the evidence hierarchy, experiment documentation standard, peer review process, and academic collaboration policy. All research that informs architecture or protocol decisions must follow this methodology.

---

## 2. Research Pipeline

IRIS follows a three-stage research pipeline: **simulation first, then physical lab, then field**.

```
Theory / Literature
       ↓
   Simulation          (ONE simulator, custom Python DTN sim)
       ↓
 Physical Lab Test     (controlled environment, known topology)
       ↓
  Field Pilot          (real geography, real users, real conditions)
       ↓
  Production Config    (parameter baked into defaults)
```

No result advances to the next stage without documented closure of the current stage. Skipping stages is permitted only for low-risk parameters (UI copy, non-safety-critical defaults) with explicit justification recorded in the experiment document.

---

## 3. Evidence Hierarchy

Research that informs IRIS design is classified by evidence level. Higher levels override lower levels when results conflict.

| Level | Name | Description | Example |
|-------|------|-------------|---------|
| 1 | Theoretical | Mathematical derivation; analytical model | PRoPHET delivery probability formula |
| 2 | Simulation (published) | Peer-reviewed simulation study | Spyropoulos et al. Spray-and-Wait results |
| 3 | Simulation (internal) | IRIS-internal simulation using ONE or custom sim | EXP-ROUTE-001 results |
| 4 | Physical lab | Controlled physical experiment in known environment | EXP-BLE-001 range test |
| 5 | Field trial | Experiment with real users in real geography | Ahmedabad pilot trial |

When an architecture decision references evidence, the evidence level must be stated. Example: "PRoPHET P_init = 0.75 based on Level 2 evidence (Lindgren 2004); to be validated by Level 3 before Alpha."

Design decisions may be made on Level 1–2 evidence for MVP features, but must specify the Level 4–5 validation plan and expected timeline.

---

## 4. Experiment Documentation Standard

Every experiment that produces evidence informing IRIS design must be documented in `docs/experiments/`. Each experiment document follows this structure:

### 4.1 Required Sections

**Experiment ID:** `EXP-{DOMAIN}-{NNN}` (e.g., `EXP-ROUTE-001`)  
**Title:** Short descriptive title  
**Status:** `Planned | In Progress | Complete | Abandoned`  
**Date:** Date of completion (or planned date)  
**Researcher:** Person(s) responsible  
**Closes Gap:** Reference to `RESEARCH_GAPS.md` gap ID (if applicable)

**Hypothesis:** A single falsifiable statement. Example: "PRoPHET routing achieves ≥10% higher delivery ratio than Spray-and-Wait at L=8 in a 100-node random waypoint simulation with 10% contact probability."

**Method:**
- Environment: software version, hardware, geography
- Independent variables: what is varied
- Dependent variables: what is measured
- Controls: what is held constant
- Sample size / repetitions: how many trials
- Statistical test: how results are compared (t-test, Wilcoxon, etc.)

**Raw Data:** Link to raw data file in `data/experiments/EXP-{ID}/`. Raw data is never edited; analysis scripts transform it.

**Result:** Quantitative statement of what was measured. No interpretation yet.

**Conclusion:** Interpretation of result relative to hypothesis. Explicitly state: hypothesis confirmed / refuted / inconclusive.

**Implication:** What IRIS design decision changes (or is confirmed) as a result. If none, state "No immediate design change; baseline established."

**Reviewer:** Person who reviewed the experiment (must not be the researcher for Level 4–5 experiments).

### 4.2 Data Management

- Raw data stored in `data/experiments/EXP-{ID}/raw/`
- Analysis scripts in `data/experiments/EXP-{ID}/analysis/`
- Plots in `data/experiments/EXP-{ID}/figures/`
- All data committed to the repository (Git LFS for large files)
- Data from field trials involving human participants: anonymized before commit; PII stored separately with restricted access

---

## 5. Simulation Framework

### 5.1 ONE Simulator

The Opportunistic Network Environment (ONE) simulator is the primary simulation tool for DTN routing experiments. IRIS maintains a fork at `tools/one-simulator/` with:
- IRIS mobility models: Indian city mobility traces derived from OSM
- IRIS routing modules: PRoPHET and Spray-and-Wait implementations matching Rust core
- IRIS message profiles: SOS message characteristics (size, TTL, priority distribution)

Simulation runs are deterministic (seeded RNG) and reproducible. Each simulation is run 10 times with different seeds; results are reported as mean ± standard deviation.

### 5.2 Custom Python DTN Simulator

For rapid parameter sweeps (e.g., sweeping PRoPHET β from 0.01 to 0.99), the Python simulator in `tools/iris-sim/` is faster than ONE. It does not model physical layer (radio propagation) and is suitable only for protocol-layer questions.

```
tools/iris-sim/
├── iris_sim/
│   ├── node.py          # Node state (contact history, bundle store)
│   ├── routing.py       # PRoPHET, Spray-and-Wait implementations
│   ├── mobility.py      # Mobility models
│   └── metrics.py       # Delivery ratio, overhead ratio, latency
├── scenarios/           # Pre-defined scenario configurations
└── experiments/         # Experiment scripts (one per EXP-* document)
```

### 5.3 Network Simulator 3 (ns-3)

For physical-layer questions (LoRa propagation, BLE range), ns-3 with LoRaSim module is used. ns-3 models are in `tools/ns3-models/`. ns-3 experiments are slower to set up but produce physically grounded results.

---

## 6. Peer Review Process

### 6.1 Internal Review

All research that informs a Level 3+ architecture decision must be internally reviewed before the decision is documented in an ADR or STANDARDS_REVIEW. 

Review checklist:
- [ ] Hypothesis was stated before the experiment (pre-registration)
- [ ] Method matches what was described before execution
- [ ] Raw data is available and unmodified
- [ ] Statistical test is appropriate for the sample size and distribution
- [ ] Conclusion follows from result (no overclaiming)
- [ ] Implication is correctly applied to the design decision

The reviewer must not have been the primary researcher on the experiment. For Level 5 (field trial) experiments, two reviewers are required.

### 6.2 External Review

Before any IRIS research finding is published (conference paper, blog post, whitepaper), it undergoes:
1. Internal review (as above)
2. Legal review: no disclosure of security vulnerabilities without coordinated disclosure process
3. One external reviewer (academic collaborator or recognized practitioner in DTN/mesh networking)

---

## 7. Academic Collaboration Policy

### 7.1 Principles

IRIS welcomes academic collaboration on open research questions. Collaboration is structured to:
- Protect ongoing IP where relevant
- Ensure research is reproducible and published
- Credit contributors accurately

### 7.2 Collaboration Agreement

Academic collaborators sign a research collaboration agreement covering:
- Data sharing: what data can be shared, what is restricted (user data from field trials)
- Publication rights: IRIS team reviews drafts for security disclosures; no veto on publication
- IP: research results are published openly; implementation IP remains with IRIS

### 7.3 Priority Research Questions for Academic Partners

1. Optimal PRoPHET parameters for Indian disaster mobility (see GAP-001)
2. Battery drain modeling for LoRa + BLE hybrid operation on Android
3. Privacy-preserving routing algorithms (OP-003 in OPEN_PROBLEMS.md)
4. Sybil resistance in decentralized DTN without PKI (OP-001)

### 7.4 Data for Research

IRIS will provide anonymized simulation traces and (with participant consent) anonymized field trial contact graphs to academic collaborators. Message content is never shared — encryption ensures IRIS itself cannot access content.

---

## 8. Research Lifecycle

```
Gap identified → Research proposal → Simulation → Physical lab → Field → Decision → Monitor
     ↑                                                                                 |
     └─────────────────────────── New evidence invalidates decision ──────────────────┘
```

Parameters derived from research are re-evaluated when:
- A Level 5 field trial produces results that conflict with the current Level 3 evidence base
- A published paper invalidates the model used
- A new deployment geography differs significantly from the modeled environment

---

## 9. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
