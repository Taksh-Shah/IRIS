# SYSVAL-001 TEST/EVIDENCE — Whole-System Soundness Validation

**Document ID**: IRIS-SYSVAL-001-EVIDENCE-001
**Date**: 2026-08-22
**Stage**: IMPLEMENT+TEST combined (iter ~178)
**Status**: ALL TRACKS GREEN — Phase 2 gates green — device tier deferred to
physical-phone phase (operator confirmed hardware forthcoming).

---

## Track evidence

| AC | Track | Verdict | Evidence |
|----|-------|---------|----------|
| AC-1 | A: Cross-transport mesh | PASS | `three_node_cross_transport_relay_delivers_verbatim` — N1→N2 over LoRa sim pair, N2→N3 over satellite sim pair; P0 envelope arrives at N3 byte-for-byte verbatim. `hot_plug_chaos_mid_flow_keeps_state_sane` — 8-cycle register/deregister/select storm with per-cycle fresh transports; terminal state sane after every cycle. |
| AC-2 | B: DTN multi-hop | PASS | `four_leg_chain_delivers_with_priorities_preserved_and_no_loops` — 4-node chain N1→N2→N3→N4 with per-link transport instances; P0+P2 traverse all 3 legs; priorities preserved; no loops (re-poll all legs yields zero). |
| AC-3 | C: Crypto paths | PASS | Envelope bytes survive every transport byte-for-byte (AC-1 + AC-7 SbdFrame/LoRaFrame verbatim tests); replay/freshness engines green in sweep (`replay_cross_reboot_persistence`, `replay_highwater_*`); identity rotation tests green in sweep. |
| AC-4 | D: Security engines | PASS | `combined_engines_hold_invariants_under_abuse_flood` — rate-limiter + spam + reputation driven together on 1200-message abuse stream: P4 flood throttled by burst semantics, every message accounted exactly once, reputation scores bounded [0,1], spam annotates but never drops. `p0_never_dropped_even_under_full_throttle` — 50× P0 checks all Exempt even with 3600 s refill interval. |
| AC-5 | E: Emergency priority | PASS | `p0_multipath_includes_satellite_emergency_only` — manager multipath includes satellite for P0/P1; transport-side hard gate rejects P3–P7 sends regardless of routing; `confirmation_hook_gates_discretionary_sends` + `confirmation_hook_panic_treated_as_denial_rt106`. |
| AC-6 | F: Failure injection | PASS | `clock_step_injection_across_guards_preserves_budgets` — backward wall-clock step across day boundary: neither LoRa duty tracker nor SatelliteCostGuard resets budgets (monotonic clamp holds); forward step recovers legitimately. `tx_failure_refunds_guard_rt104` + `malformed_inbound_skip_and_count_rt105` + `dead_dongle_attach_rejected_rt103`. |

## Phase 2 system-level gates

| Gate | Result |
|------|--------|
| `cargo test --workspace --all-features` | **727 passed / 0 failed / 1 ignored** |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **0 warnings** |
| `cargo fmt --all -- --check` | **clean** |
| `cargo audit` | exit 0 (no vulnerabilities) |
| Criterion benches compile | yes (workspace build includes benches) |

## Baseline trajectory across the session

| Milestone | Tests green |
|---|---|
| Session start (LORA-001 pre-node) | 664 |
| LORA-001 accepted (~167) | 686 → 720 (incl. RT fixes) |
| SAT-001 accepted (~175) | 720 |
| SYSVAL-001 complete (~178) | **727** (+7 sysval integration tests) |

## Device tier (deferred)

Physical Android phone tests are deferred until the operator provides
hardware. The following GATED rows unlock then:
- ANDROID-001 device-tier acceptance (real BLE scanning/advertising between
  phones, FGS/background behaviour, OEM battery matrix)
- BLE-001/BLE-002 physical-device CoreBluetooth/CoreBluetooth tests
- LORA-001/SAT-001 real-modem link benchmarks (EXP-LORA-001 / EXP-SAT analog)
- EXP-003 battery measurement

All are recorded as known_limitations with explicit reactivation triggers.
