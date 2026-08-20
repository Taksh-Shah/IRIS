# PILOT-001 TEST — Field Pilot Deployment AC-7..AC-10 evidence map

**Document ID**: IRIS-PILOT-001-TEST-001
**Date**: 2026-08-19
**Stage**: TEST (iter ~156) — SIM-001 sim pre-validation of test battery B-1..B-4
per `PILOT_001_DESIGN.md` §5 (D-3) + AC-7..AC-10 (pattern BLE-002_TEST.md).
**Status**: COMPLETE — SECURITY_REVIEW (AC-18) → VERIFY → ACCEPT next
**Live re-run THIS pass**: `cargo test -p iris-core --lib sim::` = **26/0**; full
`cargo test --workspace --all-features` = **664 passed / 0 failed / 1 ignored**
(23 suites; iris-core lib 573 + iris-ios 11 + integration/companion suites),
`cargo clippy --workspace --all-features --all-targets` **0 warnings**,
`cargo fmt --all --check` **clean**. Baseline updated **658/0/1 → 664/0/1**
(+6 net-new: 5 pilot scenario tests + 1 Wilson-CI bounds test).

---

## What was pre-validated

The B-1..B-4 battery runs on the **SIM-001 discrete-event network-simulator
harness** (`crates/iris-core/src/sim/`) — the same harness SIM-001 was verified
on (SIM_001_VERIFICATION.md). No hardware required: this is the simulation half
of the "sim pre-validation + field legs" evidence split that AC-7..AC-10
contract (PILOT_001_DESIGN.md §5 "BLK-0005 physical-device legs are **folded
into the pilot itself**" — the pilot field legs re-run B-1..B-4 on real
devices and feed the KPI evaluation; physical rows recorded BLK-0005-gated).

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-7 (B-1 NCT-of-N relay capacity) | **PASS (sim)** | Two regressions in `crates/iris-core/src/sim/scenario.rs`: **`pilot_b1_nct_of_n_delivers_all_with_latency_cdf`** (N=20: `delivered_total == injected_total` — all-N delivery; `loop_free()`; latency CDF p50 = 1 s / p95 = 1 s, i.e. ≤ §7 threshold floor 5 s / 30 s, 1 hop) and **`pilot_b1_nct_of_n_scales_to_100`** (N=50 **and** N=100 legs: all-N delivery + `loop_free()`; Wilson-95% lower bound 0.929 (N=50) / 0.963 (N=100) ≥ 0.90 delivery floor). **Honest small-N disclosure**: at N=20 perfect delivery the Wilson-95% lower bound is **0.839 < 0.90** — small-N CI width, exactly the non-normal behavior §7's Wilson plan exists to report; the floor is met *statistically* at N≥50, so the pilot's 50-100 device sample (D-1) is the KPI-bearing leg. Point estimate across all legs = 1.0. |
| AC-8 (B-2 partition/healing) | **PASS (sim)** | **`pilot_b2_partition_heals_without_duplicates`** — 2×4 internally-meshed partitions with **no inter-partition contact for 60 s** (the Internet-relay uplink "down" window — the uplink-down proxy at the SIM layer), then a full healing mesh: 9 P2 messages injected (4 intra-A + 4 intra-B + 1 cross); **8 intra-partition messages delivered during the partition (`delivered_at < 60 s`)**; the **cross-partition message delivered by the heal (`delivered_at ≥ 60 s`)**; P2 delivery ratio ≥ 0.9 (floor holds through the outage); **`loop_free()` → heal introduces no duplicate terminal delivery**. |
| AC-9 (B-3 multi-hop SCF mule-chain) | **PASS (sim)** | **`pilot_b3_mule_chain_delivers_within_contact_window`** — strict 4-leg store-carry-forward chain (node *i* meets *i+1* at `(i+1)*1000` ms) with a P3 message (hop budget 5) injected at node 0 for node 4: **delivered at exactly 4000 ms = segments × per-leg window** (contact-window expectation), **`max_hops_seen == 4`** (hop-count CDF = chain length), `loop_free()`. |
| AC-10 (B-4 iOS limited-relay) | **PASS (sim)** | **`pilot_b4_ios_limited_relay_participates_on_windows`** — 5-Android full mesh + 1 iOS leaf (index 5) participating **only in its two foregrounded windows** (t=2 s meets Android 0; t=12 s meets Android 4): delivery ratio ≥ 0.95 (§7 floor 0.90 held); **iOS-recipient message delivered at the first window (t=2 s)**; **iOS-origin message buffers and delivers at the next window (t=12 s)** — the limited-relay asymmetry (higher latency, floor still held) is the recorded behavior; `loop_free()`. Physical iOS rows (real backgrounded iPhone restore semantics) are **BLK-0005-gated** → carried to the pilot field legs per §5. |
| AC-17 (workspace baseline) | **PASS (live)** | Re-verified LIVE this pass: `cargo test --workspace --all-features` **664/0/1** (23 suites; iris-core lib 573 = 567 + 6 net-new, iris-ios 11); `cargo clippy --workspace --all-features --all-targets` **0 warnings / 0 errors**; `cargo fmt --all --check` **clean**. No Rust-core regressions introduced. |
| AC-11 (KPI plan + Wilson-CI) | PASS (carry) | Sample plan + thresholds + `wilson_ci_95` helper committed at PILOT_KPI_PLAN.md (iter ~155); `sim/metrics.rs::wilson_ci_95` bounds regression `wilson_ci_95_full_samples_bind` (perfect delivery lower bound monotone in n: 0.886 (n=30) < 0.929 (n=50) < 0.963 (n=100); hi clamped ≤ 1.0; k=0 keeps a finite small upper bound 0.071 (n=50); empty sample degenerates to (0,0) without panic). |
| AC-18 (SECURITY_REVIEW) | PENDING | Next stage — adversarial review of provisioning/decommission/authority-root/TEST-only-SOS/DRILL-cert/relay-cadence (iter ~157). |

## Stage notes

- **Sim harness reused, not extended beyond need**: B-1/B-3/B-4 scenarios +
  `wilson_ci_95` are small additive APIs on the existing SIM-001 `Simulation`
  (contact graph + injection + per-priority outcome slices); the core event
  loop, forwarding, anti-loop, and dedup semantics are untouched (SIM-001 AC
  evidence held).
- **B-3 priority choice recorded**: P4's anti-loop hop budget is 3
  (`flood::max_hops_for_priority`), so a 4-leg chain uses P3 (budget 5). The
  hop-budget gate is correct behavior (bounded flooding), not a defect; the
  test documents the choice so the field KPI evaluation reads hop-CDF against
  the right priority class.
- **B-1 latency CDF**: on the recurring full-mesh schedule every N=20 message
  rides the first 1 s contact round → p50/p95 = 1 s, well inside the §7 floor
  (5 s / 30 s) and target (2 s / 10 s). The 0.90 multi-hop floor and 0.95 P2P
  target map to the hop-count/mesh legs the pilot measures on 50-100 devices.

## Next

SECURITY_REVIEW (iter ~157, AC-18): redteam adversarial review of
provisioning/decommission/authority-root/TEST-only-SOS/DRILL-cert/relay-cadence
→ VERIFY (AC-17/18 independent evidence table) → ACCEPT + NODE_TRANSITION →
LORA-001/SAT-001.