# SAT-001 VERIFICATION — AC-1..17 evidence table (AC-17)

**Document ID**: IRIS-SAT-001-VERIFICATION-001
**Date**: 2026-08-22 (iter ~174)
**Reproduced at**: git `dfa6ac9` + working tree (SAT-001 implementation,
uncommitted at VERIFY per node convention; commit lands at ACCEPT).
**Method**: independent-verifier-style reproduction — every runtime claim
re-executed THIS pass at the recorded revision; every AC-cited test name
re-grep'd in source (file:line below); every SECURITY_REVIEW disposition
re-located as real code. **Verifier verdict: APPROVE** (no findings).

---

## Live reproduction (THIS pass)

| Check | Command | Result |
|---|---|---|
| Workspace suite | `cargo test --workspace --all-features` | **720 passed / 0 failed / 1 ignored** (full sweep; iris-core lib 629 incl. proptests) |
| Satellite module | `cargo test -p iris-core --lib transport::satellite` | **24 passed / 0 failed** (21 TEST-stage + 3 RT regressions) |
| Lora mirror test | `backlog_push_deferred_over_cap_retains_held_traffic_sat_rt104` | PASS (SAT-RT-104 fix mirrored into lora) |
| Lint gate | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0 warnings** |
| Format gate | `cargo fmt --all -- --check` | **clean (exit 0)** |

## Evidence-integrity checks

- **23/23 AC-cited test names exist in source** (satellite.rs unless noted):
  caps_row_pins_satellite_contract (:1332), guard_p0_flows_when_all_budgets_
  exhausted_ac2 (:1400), ledger_records_every_mutation_double_write_seam
  (:1473), send_consumes_hourly_cap_queue_not_drop_recovery_ac4 (:1572),
  transport_eligibility_gate_rejects_p3_plus_ac5 (:1540),
  sbd_frame_caps_enforced_mo_mt_asymmetric (:1365),
  async_ack_constants_pinned_ac9 (:1800),
  confirmation_hook_panic_treated_as_denial_rt106 (:1870),
  guard_backward_clock_step_cannot_reset_budgets_rt102 (:1811),
  guard_refund_exact_token_no_mispair_rt103 (:1841),
  backlog_push_deferred_over_cap_retains_held_traffic_sat_rt104
  (lora.rs:1965), tx_failure_refunds_guard_rt104 (:1623),
  malformed_inbound_skip_and_count_rt105 (:1648),
  dead_dongle_attach_rejected_rt103 (:1671),
  hot_plug_lifecycle_shutdown_clean (:1683),
  end_to_end_pair_delivery_zero_attribution (:1706),
  sim_outage_drops_deterministically_full_loss (:1729),
  manager_registers_selects_and_hot_unplugs_satellite (:1755),
  stub_adapters_report_hardware_gated (:1745),
  provider_reserved_variants_documented_non_exhaustive (:1355),
  sbd_frame_verbatim_byte_equality_all_priorities (:1384),
  confirmation_hook_gates_discretionary_sends (:1514),
  transport_rejects_over_mo_payload (:1560).
- **SECURITY_REVIEW dispositions re-located as real code (file:line)**:
  SAT-RT-102 monotonic clamp (satellite.rs:387), SAT-RT-103 exact-token refund
  fn (:491), SAT-RT-105 MT-relay caps row (:832), SAT-RT-106 hook
  catch_unwind (:1056), SAT-RT-108 slot critical-section recheck (:923),
  SAT-RT-109 ledger bound (:240); SAT-RT-104 push_deferred mirrored in BOTH
  transports (lora.rs BacklogQueue + satellite drain path).
- No fabricated identifiers; no AC row cites a nonexistent test.
- Count reconciliation: 720 = iris-core lib 629 all-features (567
  default-feature base + 31 lora + 24 satellite + proptest-gated cases) +
  integration suites (crypto_e2e 3, golden_vectors 6, ml 8, obs 4,
  protocol_conformance 10, sim_scenarios 8+1 ignored, tokio_behavior 4) +
  desktop (5+0+3+3) + iris-ios 11 + storage (7+1+13).

## AC → evidence table

| AC | Verdict | Evidence (all re-verified THIS pass) |
|----|---------|----------|
| AC-1 | PASS | `manager_registers_selects_and_hot_unplugs_satellite` (:1755) — register/DuplicateId/select for ≤270-B request/excluded >340-B unfragmentable; Caps row pinned Expensive / max_message_size=270 (MT relay budget, SAT-RT-105) / GMPCS-MSS regulatory_band / requires_infrastructure+special_hardware / latency 90_000. |
| AC-2 | PASS | `guard_p0_flows_when_all_budgets_exhausted_ac2` (:1400) — hourly cap AND daily budget exhausted via P2 traffic; P0 admitted at zero remaining budget while P1/P2 refused (SOS-exempt, DEC-SAT-0003). |
| AC-3 | PASS | `ledger_records_every_mutation_double_write_seam` (:1473) — FanoutLedger double-write harness: two sinks receive identical event sequences; SAT-RT-109 bounded ring keeps retention honest. |
| AC-4 | PASS | `send_consumes_hourly_cap_queue_not_drop_recovery_ac4` (:1572) — cap refusal → queue-not-drop → window roll → drain succeeds; SAT-RT-104 push_deferred guarantees deferred holds can never be dropped on concurrent refill; hourly/daily recovery windows tested separately. |
| AC-5 | PASS | `transport_eligibility_gate_rejects_p3_plus_ac5` (:1540) — P3/P4/P7 Protocol-rejected transport-side; backlog enqueue gated identically. |
| AC-6 | PASS | `stub_adapters_report_hardware_gated` (:1745) + type-checked `Arc<dyn SatelliteLinkAdapter>` conformance driven across attach/send/poll by SimulatedSatelliteAdapter; AtSbdModemAdapter stub BLK-0005 recorded. |
| AC-7 | PASS | `sbd_frame_caps_enforced_mo_mt_asymmetric` (:1365) — MO ≤340 / MT ≤270 directional caps + emptiness; `sbd_frame_verbatim_byte_equality_all_priorities` (:1384) — wire bytes == envelope bytes (DEC-SAT-0002 verbatim carry). |
| AC-8 | PASS (review assertion) | Hostile-pipe posture verified by redteam positive-controls block (SAT-RT-110): poll path forwards opaque envelopes w/ zero-PeerId attribution (`end_to_end_pair_delivery_zero_attribution` :1706), no content trust below CRYPTO-001, Ring Alerts never parsed as content; full adversarial review = SECURITY_REVIEW record. |
| AC-9 | PASS | `async_ack_constants_pinned_ac9` (:1800) — RETRY_BASE_MS=300 s, jitter bounded (0,1); dual-figure latency documented (design §2.3). |
| AC-10 | PASS (review assertion) | OBS counters asserted live: refusals-by-reason + tx_failures (rt104 test), malformed_rx + mailbox_checks billed-per-poll (rt105 test), confirmation_denied (hook tests), packets/bytes (e2e); tracing target iris.transport.satellite.*; relay-cadence slowest-tier note pinned (design §5). |
| AC-11 | PASS (review assertion) | Zero crypto code/imports in satellite.rs — envelopes opaque bytes, no truncation fields exist; crypto_e2e suite green in sweep (3/3). DEC-SAT-0002/0005 held. |
| AC-12 | PASS (SIMULATION_VALIDATED) | `sim_outage_drops_deterministically_full_loss` (:1729) — seeded ChaCha8 outage roll deterministic; ALL satellite evidence tagged SIMULATION_VALIDATED until real-modem benchmark (DEC-SAT-0007/G-5). |
| AC-13 | PASS | `hot_plug_lifecycle_shutdown_clean` (:1683) + `dead_dongle_attach_rejected_rt103` (:1671) + manager deregister→shutdown; SAT-RT-108 hardening verified (slot-CS shutdown recheck, connect presence re-verify after probe). |
| AC-14 | PASS (staged) | FC-1..FC-8 SATELLITE.md corrections staged per design (GO! REST fiction removal; Telecom Act 2023 addition; 96xx-vs-handset scope; dual-figure latency; SOS-exempt cost model; security section); load-bearing pins already in design §2; SAT-RT-105 caps correction staged with them (MT-relay advertisement). Full doc edits land at DOCUMENT stage. |
| AC-15 | GATED/BLK-0005 | Physical Iridium modem link/latency/cost benchmarks + India field activation = known_limitation gated on hardware procurement AND legal sign-off (DEC-SAT-0006 HIGH-RISK gate). Explicit documented gate. |
| AC-16 | PASS | SECURITY_REVIEW complete: verdict FAIL→RESOLVED (records/SAT-001_SECURITY_REVIEW.md). Receive-path focus per G-2: hostile-pipe posture HOLDS (positive controls SAT-RT-110); SAT-RT-102..106 MEDIUM + RT-108/109 LOW FIXED (file:line above); SAT-RT-101 HIGH + RT-107 RECORDED as binding above-transport obligations. |
| AC-17 | PASS | This document: full evidence table + independent-verifier-style reproduction executed live at the recorded revision, zero findings. |

## Stage notes

- Baseline trajectory (live measurements): 695/0/1 pre-node → 716/0/1
  IMPLEMENT+TEST → **720/0/1 post-SECURITY_REVIEW** (+3 satellite RT
  regressions + 1 lora mirror test).
- Environmental disclosure carried from TEST stage: one sweep under machine
  load crawled to 942 s (lib) before completing green; THIS pass completed at
  normal speed with identical results — no flake classification needed.
- Design-doc corrections traceable: SAT-RT-105 MT-relay caps advertisement is
  staged into FC-list for DOCUMENT stage alongside FC-1..FC-8.

## Verdict

**APPROVE** — AC-1..17 satisfied (AC-15 explicitly GATED/BLK-0005; India legal
HIGH-RISK gate DEC-SAT-0006 carried as known_limitation). Next: ACCEPT
(iter ~175) — graph IMPLEMENTING→COMPLETE, PROJECT_STATE completed 30→31,
CHANGELOG bump, commit per convention, NODE_TRANSITION.
