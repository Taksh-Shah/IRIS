# SAT-001 TEST — Satellite Gateway Transport AC-1..15 evidence map

**Document ID**: IRIS-SAT-001-TEST-001
**Date**: 2026-08-22
**Stage**: TEST (iter ~172)
**Status**: COMPLETE — SECURITY_REVIEW (AC-16, MANDATORY receive-path redteam
per RES-0028 G-2) → VERIFY (AC-17) → ACCEPT next
**Live re-run THIS pass**: `transport::satellite` **21/21**, workspace
**716 passed / 0 failed / 1 ignored** (`--all-features`, full sweep),
clippy `-D warnings` **0**, fmt **clean**. Baseline 695/0/1 → **716/0/1**
(+21 new SAT tests).
**Environmental note (honest)**: two earlier workspace-sweep attempts were
killed at tool timeouts under machine load before completing (the iris-core
lib suite crawled to 942 s in the final successful run); zero test failures
occurred in any attempt and the completed sweep is fully green.

---

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-1 | PASS | `manager_registers_selects_and_hot_unplugs_satellite` — register/DuplicateId/selectable for 300-B P1 request/excluded for 400-B unfragmentable; `caps_row_pins_satellite_contract` — Expensive class, max_message_size 340, GMPCS/MSS regulatory_band, requires_infrastructure+special_hardware, !multicast, latency 90_000. |
| AC-2 | PASS | `guard_p0_flows_when_all_budgets_exhausted_ac2` — hourly cap AND daily budget exhausted via P2 traffic; **P0 admitted at zero remaining budget while P1/P2 refused** (SOS-exempt, DEC-SAT-0003). |
| AC-3 | PASS | `ledger_records_every_mutation_double_write_seam` — FanoutLedger double-write harness: two sinks receive identical event sequences (TxAdmitted/TxRefusedDaily/TxRefunded/MailboxCheckBilled all observable). |
| AC-4 | PASS | `send_consumes_hourly_cap_queue_not_drop_recovery_ac4` — cap refusal → queue-not-drop (backlog holds) → window roll → drain succeeds; `guard_hourly_cap_refuses_then_recovers_after_window` + `guard_daily_budget_blocks_and_resets_next_window` prove per-window recovery w/ retry-at hints. Confirmation hook fires for discretionary P1/P2 only (`confirmation_hook_gates_discretionary_sends`). |
| AC-5 | PASS | `transport_eligibility_gate_rejects_p3_plus_ac5` — P3/P4/P7 sends Protocol-rejected transport-side regardless of routing; backlog enqueues gated identically. |
| AC-6 | PASS | `stub_adapters_report_hardware_gated` + type-checked conformance: SimulatedSatelliteAdapter drives the transport through `Arc<dyn SatelliteLinkAdapter>` across attach/send/poll tests; AtSbdModemAdapter stub present w/ BLK-0005 recorded. |
| AC-7 | PASS | `sbd_frame_caps_enforced_mo_mt_asymmetric` — MO ≤340 / MT ≤270 caps enforced; empty rejected; `sbd_frame_verbatim_byte_equality_all_priorities` — wire bytes == envelope bytes for P0–P2 (VERBATIM carry, DEC-SAT-0002; NO on-air IRIS header exists to truncate crypto material). |
| AC-8 | PASS (review assertion) | Hostile-pipe posture: receive path performs caps-only parsing HERE and forwards opaque envelopes to the CRYPTO-001 verify layer ABOVE (module docs §header + design §3); inbound attribution is the zero-PeerId fallback proven by `end_to_end_pair_delivery_zero_attribution`; Ring Alerts never parsed as content (no such field exists below the envelope layer). Full adversarial review = SECURITY_REVIEW AC-16 (next stage, MANDATORY per G-2). |
| AC-9 | PASS | `async_ack_constants_pinned_ac9` — RETRY_BASE_MS=300_000 s-base (≥5 min), RETRY_JITTER_FRACTION bounded (0,1); retry semantics documented on the transport (never assume RTT <90 s; transit 5–20 s vs practical 20–90 s dual-figure in design §2.3). |
| AC-10 | PASS (review assertion) | OBS counters asserted live across tests: guard refusals-by-reason + tx_failures (`tx_failure_refunds_guard_rt104`), malformed_rx + mailbox_checks billed-per-poll (`malformed_inbound_skip_and_count_rt105`), confirmation_denied (`confirmation_hook_gates_discretionary_sends`), packets/bytes rx (`end_to_end_pair_delivery_zero_attribution`); tracing target iris.transport.satellite.*; relay-cadence slowest-tier note pinned (design §5). |
| AC-11 | PASS (review assertion) | satellite.rs adds ZERO crypto code/imports — envelopes opaque bytes; no truncation anywhere (SbdFrame has no fields beyond direction+envelope). Live cross-check this sweep: crypto_e2e suite green 3/3. DEC-SAT-0002/0005 held. |
| AC-12 | PASS (SIMULATION_VALIDATED) | `sim_outage_drops_deterministically_full_loss` — seeded ChaCha8 outage roll reproduces deterministically; sim knobs documented (defaults fast, realistic 20–90 s noted); ALL satellite evidence tagged SIMULATION_VALIDATED until real-modem benchmark (DEC-SAT-0007/G-5). |
| AC-13 | PASS | `hot_plug_lifecycle_shutdown_clean` — Unavailable→attach(auto-open)→Available→detach(close)→Unavailable→shutdown flag+slot-take+sends refuse ShuttingDown; `dead_dongle_attach_rejected_rt103` (HardwareGated open failure stores nothing); manager deregister→shutdown verified. |
| AC-14 | PASS (staged) | FC-1..FC-8 SATELLITE.md corrections staged per design (GO! REST fiction removal; Telecom Act 2023 addition; 96xx-vs-handset scope; dual-figure latency; SOS-exempt cost model; security section). Load-bearing pins already in design §2 (budgets, latency dual-figure, cost model). Full doc edits land at DOCUMENT stage per loop convention. |
| AC-15 | GATED/BLK-0005 | Physical Iridium modem link/latency/cost benchmarks + India field activation = known_limitation gated on hardware procurement AND legal sign-off (DEC-SAT-0006 HIGH-RISK gate). Recorded honestly, not a silent skip. |

## Stage notes

- Suite inventory: `crates/iris-core/src/transport/satellite.rs` tests —
  **21 tests**, all host-runnable (injectable-clock guard, deterministic sim,
  pure consts); zero env-gated Rust rows.
- Count reconciliation: workspace 716 = prior 695 + 21 new satellite tests;
  iris-core lib 604 → 625 all-features.
- Environmental disclosure: two sweep attempts hit tool timeouts under load
  (lib suite up to 942 s in the final run vs ~290–300 s historical);
  standalone module runs stayed seconds-fast throughout; zero failures in any
  attempt. No flake follow-up needed (unlike LORA-001's rate_limiter finding).

## Next

SECURITY_REVIEW (iter ~173, AC-16): **MANDATORY receive-path redteam** per
RES-0028 G-2 (adversary model changed: cloning/spoofable-downlink proven) +
standard surfaces (cost-guard bypass, eligibility gate, hot-plug races,
backlog fairness, attribution honesty) → VERIFY (~174) → ACCEPT (~175).
