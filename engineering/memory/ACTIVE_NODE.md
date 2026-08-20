# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-19T13:50:00Z

## Active Node: PILOT-001 — Field Pilot Deployment

- **Type**: OPERATIONS (first real-world deployment)
- **Priority**: P2
- **Status**: **IMPLEMENTING** — DISCOVER (iter ~152) → RESEARCH (iter ~153,
  RES-0026, PROCEED) → DESIGN (iter ~154, `PILOT_001_DESIGN.md`, C2 gap
  RESOLVED) → IMPLEMENT COMPLETE (iter ~155, docs-only ops deliverables) →
  **TEST COMPLETE (iter ~156, SIM-001 sim pre-validation of B-1..B-4)** →
  **SECURITY_REVIEW next (iter ~157, AC-18)** → VERIFY → ACCEPT
- **Deps**: ANDROID-001 / IOS-001 / EMERG-001 / SEC-001 — **COMPLETE**;
  LEGAL-001 — RESEARCH_COMPLETE (open legal questions carry)
- **Description**: "First real-world deployment: NGO pilot, campus safety
  pilot" — 100+ devices (milestone Beta/Pilot); NDRF 50-device MoU + B2B
  5-device POC tier
- **C2 gap**: **RESOLVED** at DESIGN — AC-1..AC-18 defined; ops docs authored
  at IMPLEMENT (AC-1..AC-16); TEST evidence complete (AC-7..AC-10, AC-17);
  SECURITY_REVIEW + VERIFY evidence pending (AC-18)

## TEST COMPLETE (iter ~156, evidence 5) — SIM-001 pre-validation of B-1..B-4

`engineering/memory/records/PILOT-001_TEST.md` (pattern BLE-002_TEST.md) maps
AC-7..AC-10 to live sim evidence on the SIM-001 harness
(`crates/iris-core/src/sim/`):

| Battery | Test | Result |
|---------|------|--------|
| B-1 NCT-of-N (AC-7) | `pilot_b1_nct_of_n_delivers_all_with_latency_cdf` (N=20) + `pilot_b1_nct_of_n_scales_to_100` (N=50/100) | **PASS** — all-N delivery at every leg; latency p50/p95 = 1 s (≤ 5 s/30 s floor); loop-free. Wilson-95% lower bound **0.929 (N=50) / 0.963 (N=100) ≥ 0.90 floor**; honest small-N disclosure: N=20 perfect delivery → lo = 0.839 (< 0.90, CI width at n<50). |
| B-2 partition/healing (AC-8) | `pilot_b2_partition_heals_without_duplicates` | **PASS** — 2×4 partitions, uplink down 60 s: 8 intra-partition messages delivered during the outage, cross-partition delivered at heal, **no duplicate terminal delivery** (`loop_free()`), P2 ratio ≥ 0.9. |
| B-3 mule-chain (AC-9) | `pilot_b3_mule_chain_delivers_within_contact_window` | **PASS** — 4-leg SCF chain, P3 (budget 5): delivered at exactly **4000 ms = legs × 1000 ms window**; `max_hops_seen == 4`; loop-free. P3 chosen over P4 (budget 3) documented. |
| B-4 iOS limited-relay (AC-10) | `pilot_b4_ios_limited_relay_participates_on_windows` | **PASS** — iOS leaf participates only in foregrounded windows: to-iOS delivered at first window t=2000; iOS-origin delivered at next window t=12000 (asymmetry recorded); ratio ≥ 0.95; loop-free. Physical iOS rows **BLK-0005-gated** → pilot field legs. |

Also landed: `sim/metrics.rs::wilson_ci_95` (§7 KPI helper) + bounds regression
`wilson_ci_95_full_samples_bind` (monotone in n: 0.886 < 0.929 < 0.963; hi ≤
1.0; k=0 keeps finite upper bound 0.071 at n=50; empty → (0,0)).

**AC-17 live THIS pass**: `cargo test --workspace --all-features` = **664/0/1**
(23 suites; iris-core lib 573 = 567 + 6 net-new, iris-ios 11); clippy **0**;
fmt clean. Baseline **658/0/1 → 664/0/1**.

## Pipeline position

- **PILOT-001 TEST COMPLETE (iter ~156, evidence 5)** — 28 COMPLETE nodes.
  NEXT: **SECURITY_REVIEW (iter ~157, AC-18)**.

## SECURITY_REVIEW scope (iter ~157)

1. Adversarial review per AC-18: provisioning/decommission/authority-root
   (ProvisioningFlow D-4), TEST-only-SOS + DRILL-cert (AC-13), relay-cadence
   scaling rule (AC-6), KPI/telemetry consent, distribution custody (AC-15) —
   review record into `PILOT-001_SECURITY_REVIEW.md` (pattern
   BLE_002_SECURITY_REVIEW.md).
2. → VERIFY (AC-17/AC-18 independent evidence table) → ACCEPT +
   NODE_TRANSITION LORA-001/SAT-001.