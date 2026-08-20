# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:50:00Z

---

## Priority: PILOT-001 (P2 OPERATIONS) — SECURITY_REVIEW (iter ~157, AC-18)

**Pipeline POSITION: PILOT-001 TEST COMPLETE (iter ~156, evidence 5).**
SIM-001 sim pre-validation of battery B-1..B-4 per PILOT_001_DESIGN.md
AC-7..AC-10 landed in `crates/iris-core/src/sim/` (scenario.rs
pilot_b1_nct_of_n_delivers_all_with_latency_cdf N=20 + pilot_b1_nct_of_n_scales_to_100
N=50/100 + pilot_b2_partition_heals_without_duplicates +
pilot_b3_mule_chain_delivers_within_contact_window +
pilot_b4_ios_limited_relay_participates_on_windows) + metrics.rs wilson_ci_95
bounds regression. `PILOT-001_TEST.md` evidence map authored. AC-17 live:
workspace **664/0/1** (was 658/0/1), clippy 0, fmt clean. PROJECT_GRAPH
PILOT-001 evidence(5) + stage_note TEST COMPLETE.

## Next Action (iter ~157 — PILOT-001 SECURITY_REVIEW, AC-18)

1. **Adversarial review** (redteam subagent, pattern BLE_002_SECURITY_REVIEW.md)
   of the PILOT-001 design + ops surfaces: ProvisioningFlow
   (mutual-QR ceremony, verified escalation, EMERG authority-root + DRILL-chain
   out-of-band bundle, decommission wipe/revoke/rotate), TEST-only-SOS under
   DRILL certs (AC-13 — never consumes real-SOS quota), relay-cadence scaling
   rule (AC-6), KPI/telemetry opt-in consent, distribution custody
   (Play internal ≤100, TestFlight 90-day refresh, signing 2-person + PEPK),
   incident ownership, diagnostics QR.
2. **`PILOT-001_SECURITY_REVIEW.md`** record — findings dispositioned
   (PASS / FIXED / RECORDED / GATED), each with file:line/evidence.
3. **VERIFY (iter ~158, AC-17/18)**: independent verifier + `PILOT-001_VERIFICATION.md`
   evidence table; live re-run `cargo test --workspace --all-features` + clippy +
   fmt (baseline 664/0/1).
4. **ACCEPT (iter ~159)**: PROJECT_GRAPH PILOT-001 → COMPLETE, PROJECT_STATE
   completed 28 → 29, CHANGELOG, **NODE_TRANSITION → LORA-001/SAT-001**
   (P2 hardware) → mission milestones.
5. **Persist each iteration** in all durable artifacts + commit.

## Context

28 COMPLETE nodes (IOS-001 ACCEPTED iter ~151). Pipeline: **PILOT-001** (first
OPERATIONS node). DESIGN COMPLETE iter ~154 (C2 RESOLVED AC-1..AC-18);
IMPLEMENT COMPLETE iter ~155 (ops docs AC-1..AC-16); TEST COMPLETE iter ~156
(SIM-001 B-1..B-4 pre-validation, AC-7..AC-10 + AC-17). Env-gates: Swift/device
rows ios.yml-macOS-CI + BLK-0005; Android toolchain host; LEGAL-001 open
questions are a PILOT-001 legal carry (NDRF/NDMA structuring; WPC 853(E)/2021;
SSMI; DPDPA). Sim legs run on the crates/iris-core SIM-001 harness (no
hardware required).

## Recent state files touched

- `crates/iris-core/src/sim/scenario.rs` + `crates/iris-core/src/sim/metrics.rs`
  (iter ~156, +6 net-new tests)
- `engineering/memory/records/PILOT-001_TEST.md` (iter ~156, AC-7..AC-10 map)
- `engineering/PROJECT_GRAPH.yaml` (PILOT-001 IMPLEMENTING, AC(18), evidence(5),
  stage_note TEST COMPLETE)
- `engineering/PROJECT_STATE.yaml` (implementing 1 held; health/testing_health
  refreshed)
- `engineering/memory/execution-state.yaml` (stage → TEST → SECURITY_REVIEW next)
- `engineering/memory/records/execution-log.md` (iter ~156)