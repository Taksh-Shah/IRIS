# LORA-001 TEST — LoRa Gateway Transport AC-1..15 evidence map

**Document ID**: IRIS-LORA-001-TEST-001
**Date**: 2026-08-19
**Stage**: TEST (iter ~164)
**Status**: COMPLETE — SECURITY_REVIEW (AC-16, iter ~165) → VERIFY (AC-17,
iter ~166) → ACCEPT next
**Live THIS pass**: `transport::lora` **27/27**, workspace **686 passed /
0 failed / 1 ignored** (`--all-features`, full `--no-fail-fast` sweep),
clippy `-D warnings` **0**, `cargo fmt --all --check` **clean**. Baseline:
prior bookkeeping 664 reconciled to live measurement **686/0/1** (iris-core
lib 600 = 573 cited baseline + 27 new LORA tests).
**Transient-flake IDENTIFIED**: 2 of 6 full-workspace invocations this pass
showed iris-core lib 599/1 under post-compile load. Root cause found via
proptest's auto-persisted regression seeds
(`crates/iris-core/proptest-regressions/security/rate_limiter.txt`, two new
`cc` entries: rate=5/burst=7/n=28 and rate=1/burst=9/n=23) — the failing tests
are `security::rate_limiter::proptest_tests::*` (SEC-001 scope), whose token
refill uses real elapsed time and transiently violates property bounds under
CPU saturation. **Not LORA-001 code.** Replay of the persisted seeds is green
(all 12 `security::rate_limiter` unit+prop tests PASS with regressions
replayed first); standalone lib suite green 3×; no `transport::lora` test
failed in any run. Disposition: keep seeds checked in (proptest best
practice); proper deflake = injectable clock in RateLimiter property tests
(same pattern as DutyCycleTracker `now_fn`) — carried to SECURITY_REVIEW
(iter ~165); CI exposure already mitigated by nextest retry policy
(.config/nextest.toml ci profile, TEST-001 AC-1).

---

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-1 | PASS | `manager_registers_selects_and_hot_unplugs_lora` — registers via TransportManager (DuplicateId rejected), selectable for 200-B request, excluded for 400-B unfragmentable (manager gating), caps asserted: `max_message_size == 237` (envelope capacity inside the 255-B PHY frame per iter ~163 design correction), `requires_special_hardware`, `!supports_multicast`, `cost_class == Free`, `regulatory_band` contains "853(E)". |
| AC-2 | PASS | `duty_tracker_accepts_within_budget` (NextWindow delay == airtime; remaining-fraction math) · `duty_global_exhaustion_refuses_and_reports_window` (36,000 ms filled exactly → refusal w/ `remaining_fraction == 0.0` + correct earliest-expiry window) · `duty_rolling_window_recovers_after_one_hour` (window roll restores fraction to 1.0). Sliding window enforced by `prune_locked`; injectable ms clock makes all duty math deterministic. |
| AC-3 | PASS | `duty_bucket_caps_enforced_proportionally` — Bulk cap 1,800 ms and Emergency cap 21,600 ms enforced independently of the global budget (60/25/10/5, REQ-ROUTE-C-002) · `compliance_try_new_rejects_non_table_i_configs` — `DutyCycleTracker::try_new` refuses duty 2% / 30 dBm / off-band / budget-mismatch configs ⇒ **no runtime override knob exists** (Meshtastic override NOT followed, DEC-LORA-0002) · `compliance_default_pins_wpc_table_i` pins 866.0 MHz / 14 dBm / 1% / 36,000 ms. |
| AC-4 | PASS | `duty_p0_switch_seam_reports_exhaustion_without_hold` — P0 bucket exhausted while global budget remains → immediate refusal (never held) + `duty_cycle_remaining() > 0` seam value available to the routing engine for P0-switch · `send_consumes_duty_then_refuses_then_recovers_after_roll` proves refusal surfaces as `TransportError::Busy` (never panic) + `duty_cycle_remaining()` public R7 seam. |
| AC-5 | PASS | `backlog_drains_in_priority_order_fifo_within_class` (P0,P0,P2,P2,P4 pop order; FIFO within class) · `backlog_holds_while_exhausted_and_drains_after_recovery` — entry queued while Location bucket short → drain refuses + stays queued → after window roll drains successfully. Recovery ≈1%/100 s remains the routing-layer property (D-5); queue drains strictly in priority order. |
| AC-6 | PASS | `simulated_adapter_pair_round_trips_and_reports_status` — paired SimulatedLoRaAdapter SLIP round-trip + LinkStatus fields · `simulated_adapter_full_loss_drops_every_frame` (loss=1.0 deterministic ChaCha8 fade) · `stub_adapters_report_hardware_gated` — AtSerialAdapter/SpiNativeAdapter type-checked as `&dyn LoRaLinkAdapter` (conformance guarantee, BLE-001 AC-11 pattern) returning `HardwareGated` (BLK-0005 shapes present per design §4). |
| AC-7 | PASS | `airtime_matches_semtech_formula_anchors` — Semtech AN1200.13 pure fn: SF9/255-B → **1251 ms**, SF9/237-B envelope frame → **1169 ms** (via transport accounting test), SF12/237-B → **8528 ms**, PL=1 → 104 ms, monotone in payload, one frame < 4% of hourly budget · `profile_selection_pins_p0_to_sf12` · `raw_rate_matches_lora_math` (1757 bps SF9 / 292 bps SF12). Research-era "1.82 s" figure corrected into design doc at iter ~163. BW500 noted-not-default held (design §2.3). |
| AC-8 | PASS | `lora_frame_round_trips_all_priorities_with_control_bytes` (P0..P7 + 0xC0/0xDB payloads, 18-B header) · `lora_frame_defensive_parse_rejects_malformed` (truncated / bad version / bad priority / oversized — no panic) · `lora_frame_enforces_255_byte_cap` (237-B payload encodes to exactly 255 B; 238 refused) · `slip_round_trips_special_bytes` + `slip_decode_rejects_malformed` (RFC 1055 END/ESC handling, leading-END tolerance, dangling escape, missing END, alloc cap). |
| AC-9 | PASS (review assertion + reuse) | No new fragmentation/dedup code in lora.rs (grep-verifiable). `oversize_payload_is_rejected_up_front` proves the transport rejects >237-B payloads so ADUs fragment upstream via the existing MSG-001 5×51 B layer — live-green this pass: `message_engine::fragment::tests::reassembles_two_fragment_adu`, `control_and_p0_never_fragment`, `refuses_more_than_max_fragments`. Multipath dedup stays at the envelope hash: `message_engine::dedup::tests::dedup_reports_first_win` + `message_engine::tests::dedup_drops_second_arrival` green. A BLE+LoRa duplicate therefore deduplicates once above the transports (RES-0027 RQ-5 f.3). |
| AC-10 | PASS (review assertion) | OBS-001 seam live: `LoRaMetricsSnapshot` counters asserted in `send_consumes_duty_then_refuses_then_recovers_after_roll` (packets_tx, airtime_consumed_ms, duty_refusals) + `end_to_end_frame_crosses_the_simulated_link` (packets_rx, bytes_rx); `tracing` target `iris.transport.lora.*` emits on tx-admit/refusal. Relay-cadence: LoRa joins CONGESTION_CONTROL §355 tiers as lowest-bandwidth tier (PILOT-001 AC-6 section; D-5) — P0/P1 + directed exempt upstream (EMERG-001 bypass), duty tracker remains the hard floor beneath cadence. Physical RSSI/SNR telemetry arrives via LORA_STATUS with bridge firmware (BLK-0005). |
| AC-11 | PASS (review assertion) | lora.rs adds **no crypto code/imports** — envelopes (X25519+ChaCha20-Poly1305 sign+encrypt, CRYPTO-001) are carried byte-opaque; P0-never-encrypted policy lives above the transport (FIELD_OPERATIONS). Live cross-check this pass: crypto_e2e suite green 3/3 (`m7_encrypted_signed_message_round_trips_e2e`, `m7_tampered_wire_frame_never_surfaces_plaintext`, `m7_fragmented_encrypted_message_reassembles_and_verifies`). DEC-LORA-0008 satisfied. |
| AC-12 | PASS (SIMULATION_VALIDATED) | `link_budget_ranges_are_sane_and_ordered` — sensitivity anchors SF9 −129 / SF12 −136 dBm (AN1200.13), Okumura-Hata urban max-range SF9 ≈ 3.04 km < SF12 ≈ 4.81 km @14 dBm (validity-clamped), delivery probability 1.0 inside margin → 0.05 floor beyond; EXP-LORA-001 calibration hooks = parameter swap behind these pure fns. Module docs + DEC-LORA-0007 tag ALL sim/link-budget evidence **SIMULATION_VALIDATED only** (BLK-0005/GAP-004). |
| AC-13 | PASS | `hot_plug_attach_detach_and_shutdown_lifecycle` — Unavailable → attach(auto-open) → Available → detach(close) → Unavailable → shutdown flag + Unavailable + sends refuse ShuttingDown · `manager_registers_selects_and_hot_unplugs_lora` — registry register/deregister (deregister invokes shutdown cleanly, id removed from list). Dongle plug/unplug surface per manager.rs register@84/deregister@109. |
| AC-14 | PASS | Doc reconciliation verified this pass: FC-1..FC-4 applied at iter ~161 (LORA.md Table-I figures, TRANSPORT_ABSTRACTION 865–868 band column, ROUTING_REQUIREMENTS band wording, FIELD_OPERATIONS 866.0 CONFIRMED) + iter ~163 corrections resolved INTO `LORA_001_DESIGN.md` (§2.3 airtime formula values + message-size restatement 18-B header + ≤237-B envelope; §4 caps row throughput 1_757 raw) + PROJECT_GRAPH AC-1/AC-7 rows aligned. REG-NOTES WPC carry unchanged. No drift found. |
| AC-15 | GATED/BLK-0005 | Physical SX1262 HAT / E22-900M30S link tests, board-level noise figure, duty-cycle behavior under real airtime = hardware-gated known_limitation (procurement + EXP-LORA-001, Month 8). Recorded in graph known_limitations per ACCEPTANCE_POLICY — explicit documented gate, not a silent skip. |

## Stage notes

- Suite inventory this node: `crates/iris-core/src/transport/lora.rs`
  `#[cfg(test)] mod tests` — **27 tests**, all unit-runnable on the Windows
  host (fake-clock duty tracker; deterministic seeded sim adapters; pure-fn
  airtime/link-budget math). No env-gated Rust rows; only AC-15 physical rows
  are gated.
- Live verification sequence (iter ~164): (i) full workspace sweep — green
  686/0/1; (ii) isolated lib re-runs — green; (iii) clippy
  `--workspace --all-targets --all-features -- -D warnings` = 0 warnings;
  (iv) `cargo fmt --all --check` clean; (v) `cargo test -p iris-core --lib
  transport::lora` = 27/27. Two intermediate workspace invocations exhibited a
  single-test lib failure (599/1) that never reproduced with output capture
  attached and never involved a `transport::lora` test — disclosed above and
  carried as deflake follow-up rather than hidden.
- Count reconciliation for the verifier: iris-core lib = **600** with
  `--all-features` (567 default-feature tests incl. the 27 LORA tests + 33
  proptest-gated cases; `cargo test -p iris-core --lib transport::lora`
  filters 540 → 27 run); workspace total = **686 passed / 0 failed /
  1 ignored** across 23 targets (crypto_e2e 3, golden_vectors 6, loom_models
  0, ml_experiments 8, obs_telemetry 4, protocol_conformance 10,
  sim_scenarios 8+1 ignored, tokio_behavior 4, desktop 5+0+3+3, iris-ios 11,
  storage 7+1+13).
- DESIGN-correction traceability: AC-7 anchors (1251/1169/8528 ms) and AC-1
  caps figure (237) match the corrected `LORA_001_DESIGN.md` §2.3/§4/§6 and
  PROJECT_GRAPH acceptance_criteria rows updated at iter ~163 — doc, graph,
  and tests agree byte-for-byte.

## Next

SECURITY_REVIEW (iter ~165, AC-16): redteam adversarial review of
`transport/lora.rs` (duty-tracker circumvention surface, defensive parse
bounds, hot-plug races, backlog starvation/priority-inversion, sim-adapter
honesty, zero-PeerId attribution) + identify/deflake the transient lib flake →
VERIFY (iter ~166, AC-17 independent verifier) → ACCEPT → NODE_TRANSITION
SAT-001.
