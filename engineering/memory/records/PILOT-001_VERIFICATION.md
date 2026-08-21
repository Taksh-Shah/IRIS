# PILOT-001 VERIFICATION — Field Pilot AC-1..AC-18 evidence + verifier verdict

**Document ID**: IRIS-PILOT-001-VERIF-001
**Date**: 2026-08-19
**Stage**: VERIFY (iter ~158, AC-17/AC-18) → ACCEPT (iter ~159)
**Pattern**: BLE_002_VERIFICATION.md / IOS-001_VERIFICATION.md

---

## Verifier verdict

**APPROVE** — independent-verifier-style reproduction performed in-pass at
HEAD `748eaec` (iter ~158): every AC-cited test name and every P-00x security
disposition re-located in source/records; runtime workspace gates re-run live
THIS pass; all acceptance-criterion claims trace to real artifacts (no
evidence-integrity findings). No code changes were required at VERIFY.

## Live reproduction (THIS pass, at HEAD 748eaec)

- `cargo test --workspace --all-features` → **664 passed / 0 failed / 1
  ignored** (23 suites; iris-core lib 573 = 567 + 6 net-new pilot tests,
  iris-ios 11, integration/companion suites) — exit 0.
- `cargo clippy --workspace --all-targets --all-features` → **0 warnings /
  0 errors**.
- `cargo fmt --all --check` → **clean**.
- Baseline **658/0/1 → 664/0/1** (+6 net-new: 5 pilot scenario tests + 1
  Wilson-CI bounds test) held from TEST (iter ~156); no Rust changes at
  SECURITY_REVIEW (iter ~157) or VERIFY (iter ~158), so the runtime evidence
  is exactly reproducible.

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-1 (Design doc v1.0, D-1..D-9 + §3-§10) | **PASS** | `docs/implementation/PILOT_001_DESIGN.md` v1.0 — D-1 topology, D-2 relay-cadence, D-3 test battery B-1..B-4, D-4 ProvisioningFlow, D-5 trust model, D-6 KPI + Wilson-CI, D-7 NDMA DMEx, D-8 distribution runbook, D-9 REG-NOTES; AC-1..AC-18 defined. |
| AC-2 (Runbook: topology, gateway roles, ProvisioningFlow ceremony, decommission, day-0/day-N, incident ownership, diagnostics QR) | **PASS** | `docs/operations/PILOT_RUNBOOK.md` — §1 topology + gateway/fallback, §2.1-2.5 ceremony (mutual-QR, verified escalation, authority-root/DRILL bundle, decommission wipe+revoke+rotate, no-recovery), §4 day-0, §5 day-N, §6 incident ownership, §7 diagnostics QR. **+ P-007/P-013 hardening (SECURITY_REVIEW)**. |
| AC-3 (KPI plan: OBS-001 mapping, thresholds target/floor, Wilson-CI, battery methodology, cadence, 7-day retention) | **PASS** | `docs/operations/PILOT_KPI_PLAN.md` — Table 1 KPI mapping + thresholds, Table 2 Wilson-CI 95% formula + per-class/per-leg sampling (never pooled), §2 controlled fg/bg battery, §3 relay_cadence-scaled cadence, §4 consent. **+ P-013 consent trail (SECURITY_REVIEW)**. |
| AC-4 (Exercise plan: NDMA DMEx M1/M2/M3, observers + self-assessment, DRILL-chain + TEST-only SOS, safety annex) | **PASS** | `docs/operations/PILOT_EXERCISE.md` — §1 M1 TTEx/M2 field ME (50 devices)/M3 evaluation-AAR to NDRF, §2 drill mechanics under DRILL certs, §3 cells/legs (EXP-002/003/005), §4 safety annex (communications-only, public-notice). |
| AC-5 (Distribution: Play internal ≤100 + closed 12×14, TestFlight + 90-day refresh, signing 2-person + PEPK, OEM battery-kill, telemetry consent, update path) | **PASS** | `docs/operations/PILOT_DISTRIBUTION.md` — §1 DEC-PILOT-0006/0007, §2-3 track setup, §4 signing custody (2-person + PEPK), §5 OEM battery-kill, §6 telemetry/consent, §7 incident table, §8 release cadence ownership, §9 acceptance hooks. |
| AC-6 (Relay-cadence rule at INTERNET-001/ROUTE-001 seam; P0/P1 + directed exempt; airtime discipline) | **PASS** | `docs/routing/CONGESTION_CONTROL.md` §"Relay-Cadence Scaling Rule (POLICY — PILOT-001, DEC-PILOT-0002 / AC-6)" — ≤40 flat table, >40 linear `1.0+(N-40)×f` default 0.075 (0.01-0.1), exemptions, example (60 nodes → 75 min). |
| AC-7 (B-1 NCT-of-N: all-N + latency CDF N=20/50/100) | **PASS (sim)** | `crates/iris-core/src/sim/scenario.rs::pilot_b1_nct_of_n_delivers_all_with_latency_cdf` (N=20 all-N, loop-free, p50/p95 = 1 s ≤ 5 s/30 s floor) + `pilot_b1_nct_of_n_scales_to_100` (N=50/100 all-N, loop-free, Wilson-95% lo 0.929/0.963 ≥ 0.90 floor). Honest small-N: N=20 lo = 0.839 < 0.90 (CI width; KPI floor met statistically at N≥50, pilot sample is the KPI leg). |
| AC-8 (B-2 partition/healing, no duplicates) | **PASS (sim)** | `pilot_b2_partition_heals_without_duplicates` — 2×4 partitions, 60 s outage: 8 intra delivered during outage, cross delivered at heal, `loop_free()` → no duplicate terminal delivery, P2 ratio ≥ 0.9. |
| AC-9 (B-3 multi-hop SCF mule-chain within contact window) | **PASS (sim)** | `pilot_b3_mule_chain_delivers_within_contact_window` — 4-leg chain, P3 (budget 5; P4=3 recorded): delivered at exactly 4000 ms = legs × window, max_hops_seen == 4, loop-free. |
| AC-10 (B-4 iOS limited-relay on foreground/restore) | **PASS (sim)** | `pilot_b4_ios_limited_relay_participates_on_windows` — iOS leaf only in foregrounded windows: to-iOS at t=2 s (first window), iOS-origin buffered → delivered at t=12 s (next window) — asymmetry recorded; ratio ≥ 0.95; loop-free. Physical rows BLK-0005-gated → pilot field legs. |
| AC-11 (KPI sample plan + thresholds committed) | **PASS (carry)** | `PILOT_KPI_PLAN.md` Table 2 + `sim/metrics.rs::wilson_ci_95` + bounds regression `wilson_ci_95_full_samples_bind` (monotone in n 0.886<0.929<0.963; hi ≤ 1.0; k=0 → finite hi 0.071; empty → (0,0)). |
| AC-12 (ProvisioningFlow verified — each step a runbook step + rehearsal record) | **PASS** | Runbook §2.1-2.5 steps match DESIGN D-4; ceremony + decommission rehearsed in M1 TTEx (PILOT_EXERCISE.md §1/§3); physical-device rows BLK-0005-gated. |
| AC-13 (EMERG drill integration — TEST-only SOS never consumes real-SOS quota) | **PASS** | Verified real code: EMERG-001 `drill.rs` (`SosKind::Test`, `drill_flag_suppresses`, `drill_message_type_suppresses`, `verified_drill_suppressed`, `drill_mismatch_rejected`) + SEC-001 EmergencyAcl drill cap-exempt; exercise plan §2/§4 + yellow banner + observers + debrief + safety annex. |
| AC-14 (REG-NOTES codified) | **PASS** | `docs/operations/REG_NOTES.md` — WPC G.S.R. 853(E) 2021 (865-868, 25 mW e.r.p., ≤1% duty), SSMI non-applicability memo, NDRF MoU + STQC GA timing, open counsel QC-1..5, **no legal opinion**. |
| AC-15 (Distribution runbook with owners + calendar tasks) | **PASS** | `PILOT_DISTRIBUTION.md` — tester lists (≤100 / external), TestFlight 90-day refresh calendar task (two named owners), signing 2-person + PEPK restore drill, release cadence ownership; G-P3 re-verify at closed-track step. |
| AC-16 (EXTERNAL-FACTS reconciliation) | **PASS** | GO_TO_MARKET §2.1 internal-track note; COMPLIANCE_RISK_REGISTER + SPECTRUM_CONSIDERATIONS 865-867 → 853(E) 2021 865-868; FIELD_OPERATIONS 866.0 MHz ≤25 mW CONFIRMED. |
| AC-17 (Workspace green ≥ 658/0/1) | **PASS (live, THIS pass)** | **664/0/1** at HEAD 748eaec; clippy 0; fmt clean (reproduced above). |
| AC-18 (SECURITY_REVIEW + VERIFY) | **PASS** | `PILOT-001_SECURITY_REVIEW.md` (iter ~157, evidence 6): PASS-with-fix (2 FIXED + 12 RECORDED + 3 INFO, no CRITICAL/HIGH); FIXED P-007 (DRILL-CA custody + weekly re-verification; runbook §2.4/§5) + P-013 (per-device consent trail; runbook §4 + KPI §4); all 17 dispositions re-located in source/records THIS pass (grep-verified). |

## Evidence-integrity audit (independent-style, THIS pass)

- **No fake citations**: every AC-cited test name re-grepped in source and
  found — `pilot_b1_nct_of_n_delivers_all_with_latency_cdf`,
  `pilot_b1_nct_of_n_scales_to_100`, `pilot_b2_partition_heals_without_duplicates`,
  `pilot_b3_mule_chain_delivers_within_contact_window`,
  `pilot_b4_ios_limited_relay_participates_on_windows` (scenario.rs);
  `wilson_ci_95`, `wilson_ci_95_full_samples_bind` (metrics.rs).
- **No fake dispositions**: all 17 P-00x rows present in
  PILOT-001_SECURITY_REVIEW.md; both FIXED doc edits confirmed in the runbook
  (P-007: "2-person custody" + "weekly review" + DRILL CA never on a field
  leaf) and KPI plan (P-013: timestamped per-device consent record).
- **Runtime evidence is current**: no Rust changes since the TEST-stage gate —
  only docs + durable state changed at iter ~157/~158 — so 664/0/1 at HEAD is
  exact.
- **Durable-state consistency**: PROJECT_GRAPH (evidence(6), stage_note
  SECURITY_REVIEW COMPLETE), PROJECT_STATE (implementing 1 held, health
  refreshed), execution-state (iter 158), ACTIVE_NODE/NEXT_ACTION/CURRENT_STATE/
  execution-log all agree on the sequence DISCOVER→RESEARCH→DESIGN→IMPLEMENT→
  TEST→SECURITY_REVIEW→**VERIFY**→ACCEPT.

## Findings dispositioned at VERIFY

None. Independent-style audit closed clean on the first pass (pattern
BLE-002; contrast: BLE-002 needed VR-01..VR-06 coercive record corrections).
No code changes required.

## Next

ACCEPT (iter ~159): PROJECT_GRAPH PILOT-001 → **COMPLETE** (evidence 7),
PROJECT_STATE completed 28 → **29**, CHANGELOG, NODE_TRANSITION →
**LORA-001/SAT-001** (P2 hardware) → mission milestones (MVP/Alpha/Beta/Pilot).