# LORA-001 VERIFICATION — AC-1..17 evidence table (AC-17)

**Document ID**: IRIS-LORA-001-VERIFICATION-001
**Date**: 2026-08-19 (iter ~166)
**Reproduced at**: git `1e73db5` + working tree (LORA-001 implementation,
uncommitted at VERIFY per node convention; commit lands at ACCEPT).
**Method**: independent-verifier-style reproduction — every runtime claim
re-executed THIS pass at the recorded revision; every AC-cited test name
re-grep'd in source (file:line below); every SECURITY_REVIEW disposition
re-located as real code. **Verifier verdict: APPROVE** (no findings).

---

## Live reproduction (THIS pass)

| Check | Command | Result |
|---|---|---|
| Workspace suite | `cargo test --workspace --all-features` | **695 passed / 0 failed / 1 ignored** (full sweep; iris-core lib 604 incl. proptests) |
| LoRa module | `cargo test -p iris-core --lib transport::lora` | **31 passed / 0 failed** (27 TEST-stage + 4 RT regressions) |
| Rate limiter (hermetic) | `cargo test -p iris-core --lib --all-features security::rate_limiter` | **12 passed / 0 failed** |
| Lint gate | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0 warnings** |
| Format gate | `cargo fmt --all -- --check` | **clean (exit 0)** |

## Evidence-integrity checks

- **18/18 AC-cited test names exist in source**: duty_bills_full_encoded_
  frame_not_just_payload_rt101 (lora.rs:2002), dead_dongle_attach_is_
  rejected_not_available_rt103 (:2089), tx_failure_refunds_duty_reservation_
  rt104 (:2034), malformed_inbound_frame_does_not_abort_poll_batch_rt105
  (:2061), manager_registers_selects_and_hot_unplugs_lora (:2217),
  duty_bucket_caps_enforced_proportionally (:1641),
  compliance_try_new_rejects_non_table_i_configs (:1587),
  duty_p0_switch_seam_reports_exhaustion_without_hold (:1701),
  backlog_holds_while_exhausted_and_drains_after_recovery (:2152),
  simulated_adapter_pair_round_trips_and_reports_status (:1868),
  airtime_matches_semtech_formula_anchors (:1722),
  link_budget_ranges_are_sane_and_ordered (:1931),
  hot_plug_attach_detach_and_shutdown_lifecycle (:2129),
  end_to_end_frame_crosses_the_simulated_link (:2190),
  rate_limiter_silent_drop_on_overflow (rate_limiter.rs:393),
  reassembles_two_fragment_adu (fragment.rs:304),
  dedup_reports_first_win (dedup.rs:290),
  m7_encrypted_signed_message_round_trips_e2e (crypto_e2e.rs:80).
- **SECURITY_REVIEW dispositions re-located as real code (file:line)**:
  RT-101 billing basis (lora.rs:1312), RT-102 monotonic clamp (:282),
  RT-104 refund (:305), RT-105 skip-and-count (:1419), RT-106 backlog cap
  (:523), RT-109 PHY debug_asserts (:492), RT-200 injectable clock
  (rate_limiter.rs:146).
- No fabricated identifiers: the earlier TEST-era drift class (BLE-RT-C002)
  was explicitly checked for — no AC row cites a nonexistent test.
- Count reconciliation: 695 total = iris-core lib 604 (567 default-feature
  incl. 31 LORA + 33 proptest-gated + 4 RT regressions) + crypto_e2e 3 +
  golden_vectors 6 + ml_experiments 8 + obs_telemetry 4 +
  protocol_conformance 10 + sim_scenarios 8 (+1 ignored route2_debug) +
  tokio_behavior 4 + desktop lib/main/commands/engine_roundtrip 5/0/3/3 +
  iris-ios 11 + storage lib/m3/pg_store 7/1/13.

## AC → evidence table

| AC | Verdict | Evidence (all re-verified THIS pass) |
|----|---------|----------|
| AC-1 | PASS | `manager_registers_selects_and_hot_unplugs_lora` (lora.rs:2217) — register/DuplicateId/select for 200-B/exclusion of 400-B unfragmentable; caps: max_message_size 237 (255-B PHY minus 18-B header), requires_special_hardware, !multicast, Free, regulatory_band contains "853(E)". |
| AC-2 | PASS | Duty budget enforcement w/ corrected RT-101 billing basis: `duty_bills_full_encoded_frame_not_just_payload_rt101` (1-B payload books 186 ms = airtime(SF9, PL=19); bulk flood bound 9 frames/h ≤ 1800 ms share) + rolling-window refusal/recovery tests (lora.rs:1587–1701 block). Exceeding 36 s/h refused w/ earliest-expiry window (RES-0008 R7). |
| AC-3 | PASS | Bucket caps 60/25/10/5 enforced independently of global budget (`duty_bucket_caps_enforced_proportionally`, :1641); NO override knob — `compliance_try_new_rejects_non_table_i_configs` proves the only constructor path rejects >Table-I configs (:1587). |
| AC-4 | PASS | `duty_p0_switch_seam_reports_exhaustion_without_hold` (:1701): P0 bucket exhaustion refuses immediately (never held) while `duty_cycle_remaining() > 0` exposes the switch seam; send path maps refusal → Busy, never panic. |
| AC-5 | PASS | `backlog_holds_while_exhausted_and_drains_after_recovery` (:2152) + FIFO priority-order unit test; depth now bounded (MAX_BACKLOG_ENTRIES=256, RT-106). |
| AC-6 | PASS | Adapter seam type-checked: SimulatedLoRaAdapter pair round-trip + status (:1868); AtSerial/SpiNative stubs return HardwareGated as `&dyn LoRaLinkAdapter`; post-close Closed contract documented on the trait (RT-110). |
| AC-7 | PASS | Semtech AN1200.13 pure fn anchors (:1722): SF9/255-B→1251 ms, PL=1→104 ms, monotone; profile selection pins P0→SF12; raw rates 1757/292 bps. Transport billing anchors (full frame, RT-101): SF9/255-B frame→1251 ms, SF12/255-B→9020 ms. Budget bound: one default-profile frame < 4% of hourly budget. BW500 noted-not-default (design §2.3). |
| AC-8 | PASS | Frame round-trip P0..P7 w/ control bytes; defensive parse rejects truncated/bad-version/bad-priority/oversized; 237-B payload encodes to exactly 255 B; SLIP round-trip + malformed rejection (dangling escape, invalid escape, missing END, alloc cap). |
| AC-9 | PASS (review assertion) | No new fragment/dedup code (grep-verifiable); >237-B rejected up front (`oversize_payload_is_rejected_up_front`) so ADUs fragment upstream via MSG-001 layer (`reassembles_two_fragment_adu` fragment.rs:304); envelope-hash dedup first-win (`dedup_reports_first_win` dedup.rs:290) — BLE+LoRa duplicates deduplicate once above transports. |
| AC-10 | PASS (review assertion) | OBS-001 counters asserted live (packets/bytes/airtime/duty_refusals/tx_failures/malformed_rx snapshots in send/poll tests) + tracing target iris.transport.lora.*; relay-cadence lowest-tier seam documented (CONGESTION_CONTROL §355, D-5); P0/P1 exempt upstream (EMERG-001 bypass). |
| AC-11 | PASS (review assertion) | lora.rs adds zero crypto primitives; envelopes opaque; crypto_e2e m7 suite green (`m7_encrypted_signed_message_round_trips_e2e` crypto_e2e.rs:80). DEC-LORA-0008 held. |
| AC-12 | PASS (SIMULATION_VALIDATED) | `link_budget_ranges_are_sane_and_ordered` (:1931): AN1200.13 anchors SF9 −129/SF12 −136 dBm; Okumura-Hata urban ranges ordered SF12 > SF9 within model validity; fade-margin PDR bounds. All sim/link-budget evidence tagged SIMULATION_VALIDATED (DEC-LORA-0007). |
| AC-13 | PASS | `hot_plug_attach_detach_and_shutdown_lifecycle` (:2129) + registry register/deregister→shutdown (:2217); RT-103 hardening verified (open-failure rejection :2089 regression; connect() status probe; shutdown slot take). |
| AC-14 | PASS | Doc reconciliation verified: FC-1..FC-4 applied (iter ~161); iter ~163 corrections (airtime figures, throughput 1_757 bps, 18-B header + ≤237-B envelope, caps 237) + iter ~165 billing-basis note present in design §2.3; graph AC rows aligned; REG-NOTES carry unchanged. |
| AC-15 | GATED/BLK-0005 | Physical SX1262 HAT / E22 link, board noise figure, real-airtime duty measurement = hardware-gated known_limitation (procurement + EXP-LORA-001 Month 8). Explicit documented gate, not a silent skip. |
| AC-16 | PASS | SECURITY_REVIEW complete: verdict FAIL→RESOLVED (records/LORA-001_SECURITY_REVIEW.md). RT-101 CRITICAL + RT-102..106 MEDIUM + RT-107/109 LOW FIXED (file:line above); RT-108/110 RECORDED; RT-111 INFO sim-honesty PASS; RT-200 rate_limiter clock injection landed (hermetic proptests 12/12). |
| AC-17 | PASS | This document: full evidence table + independent-verifier-style reproduction executed live at the recorded revision with zero findings; evidence-integrity greps clean. |

## Stage notes

- Baseline trajectory across the node (live measurements): 664 (bookkept,
  pre-node) → reconciled 686/0/1 at IMPLEMENT+TEST → **695/0/1 after
  SECURITY_REVIEW fixes** (net +4 lora RT regressions vs TEST stage;
  rate_limiter count unchanged, made hermetic).
- The iter ~164 transient flake is CLOSED by fix (not by retry policy):
  security/rate_limiter.rs now takes an injectable clock; the four property
  tests freeze time; the single real-clock refill unit test intentionally
  retains wall-clock semantics. Proptest regression seeds remain checked in
  and replay green under the frozen clock.
- Design-doc corrections traceability: §2.3 carries two dated correction
  notes (iter ~163 formula figures; iter ~165 RT-101 billing basis);
  PROJECT_GRAPH acceptance_criteria AC-1/AC-7 rows match the code anchors.

## Verdict

**APPROVE** — AC-1..17 satisfied (AC-15 explicitly GATED/BLK-0005). Next:
ACCEPT (iter ~167) — graph IMPLEMENTING→COMPLETE, PROJECT_STATE completed
29→30, CHANGELOG, NODE_TRANSITION → SAT-001.
