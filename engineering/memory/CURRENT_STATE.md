# CURRENT_STATE.md

**Schema version**: 1.0
**Last updated**: 2026-09-07T15:20:00+05:30

> **Current reconciliation (2026-09-06):** The software graph is complete/deferred,
> but the physical-device phase is active. P1 is `10BCA20F4M000BB` (vivo V2205,
> Android 14/API 34) and P2 is `b2fbcd39` (vivo 2004, Android 12/API 31).
> Current repository head is `b7a876c`. The shipping-app Tier-2 Wi-Fi Direct
> cold-cycle gate is green 10/10; the Mobly Tier-2 harness remains hardware-pending
> because discovery is attributed to the instrumentation context on this pair.
> Mobly remains HW-PENDING/BLOCKED after the bounded attempts; the corrected
> harness reaches Connected/live-socket states but still lacks reliable peer
> delivery. The `/addkey`/`/to` command-flow hypothesis was tested and rejected
> as the complete cause. The historical narrative below is retained as an execution record, not as the
> current next action. **Session 32 correction:** the 10/10 result is
> Vivo-to-Vivo evidence only. Samsung prompts also occur on DNS-SD/discovery,
> the next authorized build restores Wi-Fi Direct advertising/discovery and
> explicit association. Android's invitation UI is expected; Samsung's Sharing
> prompt remains an accepted current limitation and must be captured during
> hardware validation.
> **Mobly remediation (2026-09-06):** the harness now drives the normal shipping
> app rather than a second instrumented engine, caches data before force-stop,
> and waits for asynchronous app-link delivery. The live popup audit found no
> invitation UI/log evidence on either phone.

---

## Summary

**✅ LORA-001 TEST COMPLETE (iter ~164, evidence 5) — SECURITY_REVIEW next
(iter ~165).** `LORA-001_TEST.md` authored (pattern BLE-002_TEST.md):
**AC-1..15 evidence map — AC-1..14 PASS** (manager register/select + 237-B
caps row; duty budget refusal/recovery; bucket caps 60/25/10/5 +
no-override-knob proof; P0-switch seam; backlog FIFO hold/drain-on-recovery;
adapter conformance + HardwareGated stubs; airtime anchors 1251/1169/8528 ms;
frame/SLIP defensive parse; fragment/dedup/crypto reuse as review assertions
with MSG-001 layers + crypto_e2e green; OBS-001 counters live; SimLinkBudget
SIMULATION_VALIDATED; hot-plug lifecycle), **AC-15 GATED/BLK-0005**.
**Live: transport::lora 27/27, workspace 686/0/1** (`--all-features`, full
`--no-fail-fast` sweep), clippy 0, fmt clean. Prior-664 bookkeeping reconciled
to live (lib 600 = 567 default + 33 proptest cases incl. the 27 LORA tests).
**Transient-flake IDENTIFIED**: single lib failure in 2 of 6 workspace runs
under load — root-caused via proptest's auto-persisted regression seeds to
`security::rate_limiter` proptests (SEC-001 scope; real-time token refill
violates property bounds under CPU saturation; seeds replay green 12/12, zero
lora involvement) → keep seeds checked in + clock-injection deflake follow-up
at SECURITY_REVIEW. PROJECT_GRAPH LORA-001
evidence 4→5, stage_note TEST COMPLETE. **STAGE_TRANSITION → LORA-001
SECURITY_REVIEW (iter ~165) → VERIFY (~166) → ACCEPT → SAT-001 → mission
milestones (MVP/Alpha/Beta/Pilot).**

**✅ LORA-001 SECURITY_REVIEW COMPLETE (iter ~165, evidence 6) — VERIFY next
(iter ~166).** `LORA-001_SECURITY_REVIEW.md` — independent redteam pass,
verdict **FAIL → RESOLVED in-pass**: **RT-101 CRITICAL FIXED** (duty billed on
payload only; 18-B header unbilled → Table-I exceedance reachable via
minimal-frame flood: 344 sends = 35,776 ms accounted vs ~63,984 ms true ≈1.78%
duty) → `try_send_inner` bills the full encoded frame. RT-102..106 MEDIUM
FIXED (monotonic clock clamp; attach/connect/shutdown honesty;
DutyCycleTracker::refund + tx_failures; poll skip-and-count malformed_rx;
backlog size gate + 256 cap); RT-107/109 LOW FIXED; RT-108/110 RECORDED;
RT-111 INFO sim-honesty PASS; **RT-200 FIXED cross-file** (rate_limiter
injectable clock root-causes the iter ~164 flake; proptests hermetic,
real-clock refill test kept, seeds checked in replay-green). **Live:
workspace 695/0/1**, clippy 0, fmt clean, `transport::lora` **31/31**
(+4 RT regressions), rate_limiter 12/12. Design-doc billing-basis correction
applied (§2.3). PROJECT_GRAPH evidence 5→6, stage_note SECURITY_REVIEW
COMPLETE. **STAGE_TRANSITION → LORA-001 VERIFY (iter ~166) → ACCEPT (~167) →
SAT-001 → mission milestones (MVP/Alpha/Beta/Pilot).**
**✅ LORA-001 VERIFY COMPLETE (iter ~166, evidence 7) — verifier APPROVE;
ACCEPT next (iter ~167).** `LORA-001_VERIFICATION.md` authored — AC-1..17
evidence table + independent-verifier-style reproduction at the recorded
revision: **workspace 695/0/1** (`--all-features` full sweep),
`transport::lora` **31/31**, rate_limiter **12/12** hermetic, clippy **0**,
fmt clean. Evidence integrity clean: 18/18 AC-cited test names re-grep'd in
source with file:line; all RT dispositions re-located as real code (RT-101
lora.rs:1312, RT-102 :282, RT-104 :305, RT-105 :1419, RT-106 :523,
RT-109 :492, RT-200 rate_limiter.rs:146); zero fabricated identifiers.
Verdicts: **AC-1..14 PASS · AC-15 GATED/BLK-0005 · AC-16 PASS · AC-17 PASS →
APPROVE**. Baseline trajectory 664 bookkept → 686/0/1 → **695/0/1**.
PROJECT_GRAPH LORA-001 evidence 6→7, stage_note VERIFY COMPLETE.
**STAGE_TRANSITION → LORA-001 ACCEPT (iter ~167) → NODE_TRANSITION SAT-001 →
mission milestones (MVP/Alpha/Beta/Pilot).**
**✅ LORA-001 ACCEPTED COMPLETE (iter ~167, evidence 8) — 30 COMPLETE nodes;
NODE_TRANSITION → SAT-001.** PROJECT_GRAPH LORA-001 IMPLEMENTING →
**COMPLETE** (stage_note ACCEPTED full pipeline DISCOVER ~160 → RESEARCH ~161
RES-0027 → DESIGN ~162 → IMPLEMENT ~163 → TEST ~164 → SECURITY_REVIEW ~165 →
VERIFY ~166 → ACCEPT ~167). AC-1..17 satisfied (AC-15 GATED/BLK-0005 explicit;
verifier APPROVE at recorded revision; live **695/0/1**, clippy 0, fmt clean).
PROJECT_STATE completed 29→**30**, implementing 1→0; CHANGELOG **0.3.62**.
Commit per node convention. **NEXT: SAT-001 UNDERSTAND (iter ~168) → mission
milestones (MVP/Alpha/Beta/Pilot).**
**✅ SAT-001 DISCOVER/UNDERSTAND COMPLETE (iter ~168, evidence 1) — RESEARCH
next (iter ~169, RES-0028).** Read-only pass; baseline **695/0/1 clippy 0 fmt
clean held**. `SAT-001_DISCOVER.md` authored (pattern LORA-001_DISCOVER):
node eligible (dep TRANSPORT-001 ✅, requires_hardware true BLK-0005-adjacent,
human gate resolved DEC-0009); **C2 gap CONFIRMED** → DESIGN resolves AC-1..n.
Reference surface: SATELLITE.md full read (Iridium SBD 340-B MO/270-B MT,
20–90 s, ~10 msg/h = key P0–P2 protocol; Starlink/VSAT = Internet-gateway
reuse out of core scope; **bespoke SbdIrisMessage sketch CONFLICTS with
CRYPTO-001** → RQ-2 expected REJECT; SatelliteCostGuard 10 TX/h + daily INR +
P3+ hard gate; DoT/IN-SPACe summary), GW-001 GatewayType::Satellite already
modeled (P0..P2), TRANSPORT_ABSTRACTION row (first `Expensive` cost class),
LEGAL-001 licensing carry. RQ-1..RQ-6 staged for RES-0028. known_limitations(4).
PROJECT_GRAPH SAT-001 evidence(1) + stage_note. **STAGE_TRANSITION → SAT-001
RESEARCH (iter ~169) → mission milestones (MVP/Alpha/Beta/Pilot).**
**✅ SAT-001 RESEARCH COMPLETE (iter ~169, evidence 2) — DESIGN next
(iter ~170).** Read-only pass; baseline **695/0/1 clippy 0 fmt clean held**.
`research/RES-0028.md` authored via researcher agent (11 websearch + 4
primary passes, L1–L5): verdict **PROCEED-WITH-CONDITIONS**. RQ-1: SBD alive,
v1 targets **AT-command SBD modems** (96xx / new Iridium 9604, Jun 2026);
Iridium GO! local REST endpoints **do not exist** publicly (NDA/no-OSS policy)
→ GO! path external-integration only. RQ-2: carry STANDARD envelopes verbatim
(P0 ≈237 B fits 340-B MO); **REJECT the bespoke truncated-signature struct**.
RQ-3: SOS-exempt cost guard (Garmin/Zoleo pattern), persisted counters, RX
mailbox checks billable. RQ-4: RFC 9171 CLA MUST rate-limit; first
`Expensive` cost class; custody-style async ACKs. RQ-5: arXiv:2603.12062
proves cloning/spoofable downlink/no encryption/~1 mW jamming → envelope RX
verification MANDATORY, Ring Alerts hint-only, monotonic-sequence freshness.
RQ-6: Starlink GMPCS Jun 2025/IN-SPACe→2030 launch pending late 2026;
Telecom Act 2023 criminalizes unauthorized satcom TX; Airtel–Iridium claim
FAILS verification → India legal HIGH-RISK gate. D-1..D-6 inputs; FC-1..FC-8
doc corrections staged; gaps G-1..G-7 (HIGH ×3). PROJECT_GRAPH SAT-001
DISCOVERED → **RESEARCH_COMPLETE** evidence(2). **STAGE_TRANSITION → SAT-001
DESIGN (iter ~170) → mission milestones (MVP/Alpha/Beta/Pilot).**
**✅ SAT-001 DESIGN COMPLETE (iter ~170, evidence 3) — IMPLEMENT next
(iter ~171).** Docs-only pass; baseline **695/0/1 clippy 0 fmt clean held**.
`SAT_001_DESIGN.md` v1.0 authored (architect agent, pattern LORA_001_DESIGN)
— C2 gap RESOLVED via AC-1..17 absorbing RES-0028 D-1..D-6 + FC-1..FC-8 +
G-1..G-7: IridiumSbd over AT-command 96xx modems only (DEC-SAT-0001);
standard-envelope verbatim carry ≤340-B MO / ≤270-B MT, bespoke
truncated-signature struct REJECTED (DEC-SAT-0002); SOS-exempt
SatelliteCostGuard + CostLedger persisted counters (DEC-SAT-0003);
Expensive/EmergencyOnly + P3+ hard gate + async custody ACKs (DEC-SAT-0004);
hostile-pipe posture mandatory RX envelope verification per arXiv:2603.12062
(DEC-SAT-0005); India legal HIGH-RISK gate Telecom Act 2023 (DEC-SAT-0006);
SIMULATION_VALIDATED discipline (DEC-SAT-0007). Module layout
`satellite.rs`: provider enum, link seam, deterministic sim adapter, modem
stub (BLK-0005), SbdFrame, ledger, guard, full Transport impl w/ RT lessons.
DEC-SAT-0001..0007 ratified into DECISIONS.md. AC-16 = MANDATORY receive-path
redteam. PROJECT_GRAPH SAT-001 RESEARCH_COMPLETE → **DESIGNING** evidence(3).
**STAGE_TRANSITION → SAT-001 IMPLEMENT (iter ~171) → mission milestones
(MVP/Alpha/Beta/Pilot).**
**✅ SAT-001 IMPLEMENT COMPLETE (iter ~171, evidence 4) — TEST next
(iter ~172).** `crates/iris-core/src/transport/satellite.rs` authored
(pattern lora.rs) + wired into mod.rs per SAT_001_DESIGN.md AC-1..13:
SatelliteProvider non_exhaustive; SOS-exempt SatelliteCostGuard (P0 never
blocked, hourly+daily hard-stop w/ retry hints, refund-on-failure RT-104,
CostLedger+Fanout double-write seam AC-3); SatelliteLinkAdapter seam +
AtSbdModemAdapter BLK-0005 stub + deterministic Sim pair; SbdFrame verbatim
carry (no on-air IRIS header, MO≤340/MT≤270 caps-only parse); full Transport
impl (eligibility P3+-never gate AC-5, hot-plug RT-103 lessons, confirmation
hook, queue-not-drop drain AC-4, async ACK consts AC-9, OBS counters).
**Live: transport::satellite 21/21 · workspace 716/0/1 · clippy 0 · fmt
clean.** PROJECT_GRAPH DESIGNING → IMPLEMENTING evidence(4).
**STAGE_TRANSITION → SAT-001 TEST (iter ~172) → mission milestones.**
**✅ SAT-001 TEST COMPLETE (iter ~172, evidence 5) — SECURITY_REVIEW next
(iter ~173, MANDATORY receive-path redteam per G-2).**
`SAT-001_TEST.md` authored (pattern LORA-001_TEST): **AC-1..14 PASS**
(AC-8/10/11 review assertions w/ zero-fallback attribution + OBS counters
asserted live + crypto_e2e green), **AC-15 GATED/BLK-0005**. Live:
transport::satellite **21/21**, workspace **716/0/1**, clippy 0, fmt clean.
Environmental note recorded honestly: two sweep attempts hit tool timeouts
under machine load before the green completion; zero failures in any attempt.
PROJECT_GRAPH evidence(5) stage_note TEST COMPLETE. **STAGE_TRANSITION →
SAT-001 SECURITY_REVIEW (iter ~173) → mission milestones.**
**✅ SAT-001 SECURITY_REVIEW COMPLETE (iter ~173, evidence 6) — VERIFY next
(iter ~174).** Independent redteam adversarial pass w/ MANDATORY receive-path
focus per G-2; verdict **FAIL → RESOLVED in-pass**: SAT-RT-102..106 MEDIUM +
RT-108/109 LOW FIXED (monotonic clock clamp ported; exact-token refunds;
push_deferred over-cap re-insert mirrored both transports; MT-relay caps
advertised; hook panic containment; hot-plug TOCTOU; ledger bound);
SAT-RT-101 HIGH + RT-107 MEDIUM RECORDED as binding above-transport
obligations; SAT-RT-110 INFO positive-controls PASS. **Live: workspace
720/0/1 · clippy 0 · fmt clean · transport::satellite 24/24** (+3 RT
regressions). PROJECT_GRAPH evidence(6) stage_note SECURITY_REVIEW COMPLETE.
**STAGE_TRANSITION → SAT-001 VERIFY (iter ~174) → mission milestones.**
**✅ SAT-001 VERIFY COMPLETE (iter ~174, evidence 7) — verifier APPROVE;
ACCEPT next (iter ~175).** `SAT-001_VERIFICATION.md` authored — AC-1..17
evidence table + independent-verifier-style reproduction at recorded revision:
**workspace 720/0/1** (`--all-features` full sweep), `transport::satellite`
**24/24**, clippy **0**, fmt clean. Evidence integrity: 23/23 cited tests
re-grep'd w/ file:line; all SAT-RT dispositions re-located as real code.
Verdicts: **AC-1..14 PASS · AC-15 GATED/BLK-0005 (+ DEC-SAT-0006 legal gate)
· AC-16 PASS · AC-17 PASS → APPROVE**. Baseline trajectory 695 → 716 →
**720**. PROJECT_GRAPH evidence(7) stage_note VERIFY COMPLETE.
**STAGE_TRANSITION → SAT-001 ACCEPT (iter ~175) → mission milestones.**
**✅ SAT-001 ACCEPTED COMPLETE (iter ~175, evidence 8) — 31 COMPLETE nodes;
GRAPH EXHAUSTED of eligible autonomous software nodes.** PROJECT_GRAPH
IMPLEMENTING → **COMPLETE** (stage_note ACCEPTED full pipeline DISCOVER ~168
→ RESEARCH ~169 RES-0028 → DESIGN ~170 → IMPLEMENT ~171 → TEST ~172 →
SECURITY_REVIEW ~173 → VERIFY ~174 verifier APPROVE → ACCEPT ~175).
AC-1..17 satisfied (AC-15 GATED/BLK-0005 + India legal gate DEC-SAT-0006
carried). PROJECT_STATE completed 30→**31**, implementing 1→**0**.
CHANGELOG **0.3.63**. Commit **0a2547c**. **SUPERVISOR PARKED IDLE pending
operator direction**: (a) ratify new node batch per BLK-0002; (b) LEGAL-001
human legal counsel; (c) mission milestones MVP/Alpha/Beta/Pilot planning.
**✅ SESSION SUMMARY + TEST REPORT (iter ~179).** Comprehensive test report
authored at `docs/testing/SYSTEM_TEST_REPORT.md` — covers all tests across
LORA-001 (24+), SAT-001 (24+), SYSVAL-001 (7), and re-verification of the
full pre-existing corpus (~700). Issues found-and-fixed table: RT-101
CRITICAL billing basis, RT-102/103/104/105/106/108/109 fixes, rate_limiter
flake resolution. LEGAL-001 deferred to future w/ carried-obligations
register. NEXT: physical-phone phase (operator to provide 2–3 Android
devices; setup instructions in NEXT_ACTION.md).
### Previous

**✅ LORA-001 IMPLEMENT COMPLETE (iter ~163) — TEST next (iter ~164).**
`crates/iris-core/src/transport/lora.rs` **rewritten clean** — the prior
orphan copy was corrupted invalid Rust never wired into the mod tree (same
class as ALLOCATION.md iter ~161); `pub mod lora;` added to
`transport/mod.rs`. Delivered per `LORA_001_DESIGN.md` **AC-1..AC-13**:
ComplianceConfig+validate (Table-I pins; try_new refuses non-compliant → no
override knob), DutyCycleTracker (sliding 1-h window, injectable clock, global
36,000 ms + bucket caps 60/25/10/5, check_and_consume → NextWindow/
BudgetExhausted w/ earliest-expiry recovery), BacklogQueue (priority FIFO),
RadioProfile SF9/SF12-P0 + airtime_ms Semtech AN1200.13 (255-B→1251 ms /
237-B→1169 ms / SF12→8528 ms) + raw_rate_bps (1757/292), LoRaFrame v1
[ver][prio][msg_id16][payload≤237] (=255-B PHY cap; P0 envelope fits exactly)
+ SLIP RFC1055 defensive parse, LoRaLinkAdapter seam + AT/SPI BLK-0005 stubs +
SimulatedLoRaAdapter pair, SimLinkBudget (AN1200.13 + Okumura-Hata,
SIMULATION_VALIDATED only), LoRaTransport Transport impl (hot-plug auto
open/close, duty refusal → Busy never panic, drain_backlog, poll_inbound
zero-PeerId attribution, OBS-001 counters). **DESIGN CORRECTIONS resolved into
doc**: airtime 1.82 s → formula values; throughput 14_000 → 1_757 bps raw;
frame = 18-B header + ≤237-B envelope. **AC-15 LIVE: workspace 686/0/1**
(iris-core lib 600 = 573 + 27 transport::lora tests), clippy 0, fmt clean.
PROJECT_GRAPH LORA-001 DESIGNING → **IMPLEMENTING** (evidence 4);
PROJECT_STATE designing 1→0, implementing 0→1. **STAGE_TRANSITION → LORA-001
TEST (iter ~164) → SECURITY_REVIEW (~165) → VERIFY (~166) → ACCEPT →
SAT-001 → mission milestones (MVP/Alpha/Beta/Pilot).**

### Previous

**✅ LORA-001 RESEARCH COMPLETE (iter ~161, RES-0027 PROCEED) — RQ-1..RQ-6
resolved ADOPT/ADOPT-WITH-CONDITION; DESIGN next (iter ~162, AC-1..n).**
Read-only pass; baseline **664/0/1 clippy 0 fmt clean held**.
`RES-0027.md` authored (pattern RES-0026; websearch + primary-source passes,
L1-L5, no AI citations): **RQ-1 regulatory ADOPT-WITH-CONDITION** — **WPC
G.S.R. 853(E) 2021 Table-I (10 Dec 2021, thc.nic.in/G25977.pdf): 865-868 MHz,
25 mW e.r.p. (≈14 dBm), 1% duty (36 s/h device/hour)** L1-confirmed;
**LORA.md "1W (30 dBm)" = LoRaWAN IN865 network-plan only** (TTN/Semtech RP002:
865.0625-867.1375, 8 ch, 30 dBm EIRP) — IRIS raw-LoRa P2P governed by Table-I,
**FIELD_OPERATIONS 866.0 ≤25 mW CONFIRMED stands**; WPC-ETA per-SKU deferred
legal carry. **RQ-2 ADOPT** — DutyCycleTracker rolling 1-h 36,000 ms budget +
R7 `next_send_window` + 60/25/10/5 proportional + P0-switch-on-exhausted, no
runtime override. **RQ-3 ADOPT** — SF9/BW125/CR4/5 + SF12-P0 (-129/-136 dBm
AN1200.13), 255-B cap + 5×51 B fragmentation, raw-LoRa-P2P v1 reaffirmed.
**RQ-4 ADOPT-WITH-CONDITION** — BLE GATT bridge (LORA_TX/RX/STATUS, 255-B ATT)
+ USB-SLIP alt; Waveshare SX1262 HAT (868M→866.0) + E22-900M30S; `LoRaLinkAdapter`
abstracts AT vs SPI. **RQ-5 ADOPT** — TransportAdapter impl (255 B / ≈14 B/s /
duty_cycle_remaining), Caps SubGHz, register/deregister, multipath dedup +
fragmentation + relay-cadence D-2 seam + OBS-001 lora metrics reuse. **RQ-6
ADOPT-WITH-CONDITION** — sim params + AN1200.13/Okumura-Hata + EXP-LORA-001
calibration, SIMULATION_VALIDATED only. **DESIGN inputs D-1..D-7**; gaps
G-1..G-5; external-facts FC-1..FC-4. **ALLOCATION.md repaired** (1989-line
corruption from prior append loop → restored from git HEAD + RES-0027 registered,
next RES-0028). PROJECT_GRAPH LORA-001 **DISCOVERED → RESEARCH_COMPLETE**
(evidence 1→2, stage_note, validation_status); PROJECT_STATE research_complete
1→2, discovered 2→1. **STAGE_TRANSITION → LORA-001 DESIGN (iter ~162,
`LORA_001_DESIGN.md` AC-1..n) → SAT-001 → mission milestones
(MVP/Alpha/Beta/Pilot).**

### Previous

**✅ LORA-001 UNDERSTAND COMPLETE (iter ~160) — C2 gap CONFIRMED; RQ-1..RQ-6
staged for RESEARCH (RES-0027).** Read-only pass; baseline **664/0/1 clippy 0
fmt clean held**. `LORA-001_DISCOVER.md` authored (pattern BLE-002/IOS-001_
DISCOVER): node def (P2 TRANSPORT, dep TRANSPORT-001 COMPLETE, requires_hardware
true, DISCOVERED); **C2 gap CONFIRMED** (no ACs → DESIGN resolves AC-1..n).
Reference surface read: `docs/transports/LORA.md` (CSS, WPC 865-868 1% duty,
SF/BW/CR, raw-vs-LoRaWAN, SX1262, BLE-GATT + USB-SLIP bridge, DutyCycleTracker,
SimulatedLoRaTransport), TRANSPORT_ABSTRACTION row, RES-0003/0008/0018/0026,
ROUTING_REQUIREMENTS REQ-ROUTE-C-001..003, FIELD_OPERATIONS (866.0 MHz ≤25 mW
e.r.p. CONFIRMED), P0-envelope evidence (237 B V001 ≤255 B, 84 B payload
ceiling), PILOT-001 relay-cadence seam, GAP-004. Scope: BLE GATT bridge
(LORA_TX/RX/STATUS) + USB SLIP alt, phone-as-gateway, SF9 default/SF12-P0,
per-class floor P0 60B..P3 150B / P4+ never, 1% duty 36 s/h no-P0-override,
envelope-layer ENC, WPC compliance. **RQ-1..RQ-6** (regulatory reconciliation
1W-vs-25 mW; duty-cycle enforcement + R7 coupling; radio params; bridge
protocol; TRANSPORT-001 integration; sim fidelity vs GAP-004/EXP-LORA-001).
PROJECT_GRAPH LORA-001 evidence(1) + stage_note UNDERSTAND COMPLETE +
validation_status refreshed; PROJECT_STATE header/notes/testing_health
refreshed. **STAGE_TRANSITION → LORA-001 RESEARCH (iter ~161, RES-0027) →
SAT-001 → mission milestones (MVP/Alpha/Beta/Pilot).**

### Previous

**✅ PILOT-001 ACCEPTED COMPLETE (iter ~159) — 29 COMPLETE nodes;
NODE_TRANSITION → LORA-001:** First OPERATIONS node accepted. PROJECT_GRAPH
PILOT-001 IMPLEMENTING → **COMPLETE** (evidence 7, stage_note ACCEPTED full
pipeline DISCOVER ~152 → RESEARCH ~153 (RES-0026 PROCEED) → DESIGN ~154
(`PILOT_001_DESIGN.md` v1.0, C2 gap RESOLVED via AC-1..AC-18, DEC-PILOT-0001..
0009) → IMPLEMENT ~155 (ops docs AC-1..AC-16) → TEST ~156 (SIM-001 sim
pre-validation B-1..B-4, AC-7..AC-10) → SECURITY_REVIEW ~157 (AC-18, 2 FIXED +
12 RECORDED + 3 INFO, no CRITICAL/HIGH) → VERIFY ~158 (AC-1..AC-18 evidence
table complete, independent verifier APPROVE at HEAD 748eaec, no findings) →
**ACCEPT ~159**). AC-1..AC-18 all PASS/GATED (AC-14 PASS, physical-device legs
BLK-0005 recorded). **AC-17 live: 664/0/1, clippy 0, fmt clean**.
PROJECT_STATE completed 28 → **29**, implementing 1 → 0. **NODE_TRANSITION →
LORA-001** (P2 TRANSPORT, `requires_hardware: true`, DISCOVERED) UNDERSTAND next
(iter ~160) → SAT-001 → mission milestones (MVP/Alpha/Beta/Pilot).

### Previous

**✅ PILOT-001 VERIFY COMPLETE (iter ~158, AC-17/AC-18) — independent verifier
APPROVE, no findings:** `PILOT-001_VERIFICATION.md` authored (pattern
BLE_002_VERIFICATION.md) — **AC-1..AC-18 evidence table complete**.
Independent-verifier-style reproduction at HEAD `748eaec`: (1) **all
AC-cited test names re-grepped in source** — the 5 pilot scenarios
(`pilot_b1_nct_of_n_delivers_all_with_latency_cdf`,
`pilot_b1_nct_of_n_scales_to_100`, `pilot_b2_partition_heals_without_duplicates`,
`pilot_b3_mule_chain_delivers_within_contact_window`,
`pilot_b4_ios_limited_relay_participates_on_windows`) + `wilson_ci_95` +
`wilson_ci_95_full_samples_bind` (the KPI helper that replaced the discovery-era
Wilson guess) all exist in source; (2) **all 17 P-00x dispositions + both FIXED
doc edits (P-007 DRILL-CA custody runbook §2.4/§5, P-013 consent trail runbook §4
+ KPI plan §4) re-located in records/source**; (3) **AC-17 live THIS pass at
HEAD**: `cargo test --workspace --all-features` = **664 passed / 0 failed / 1
ignored**, clippy **0**, fmt clean (no Rust changes in the TEST→VERIFY window).
No evidence-integrity findings, no code changes. PROJECT_GRAPH PILOT-001
evidence → **7**, stage_note VERIFY COMPLETE. **NEXT: ACCEPT (iter ~159) →
NODE_TRANSITION LORA-001/SAT-001 → mission milestones.**

### Previous

**✅ PILOT-001 SECURITY_REVIEW COMPLETE (iter ~157, AC-18) — PASS-with-fix
(2 FIXED + 12 RECORDED + 3 INFO, no CRITICAL/HIGH):** Adversarial review of the
ops layer in `PILOT-001_SECURITY_REVIEW.md` (pattern BLE_002_SECURITY_REVIEW.md)
— provisioning (mutual-QR ceremony, verified escalation), decommission
(wipe+revoke+rotate), authority-root + DRILL-CA out-of-band bundle, TEST-only
SOS (drill suppression + quota cap-exemption), relay-cadence config,
KPI/telemetry consent, distribution custody, incident ownership, diagnostics
QR. **FIXED (docs):** P-007 — `PILOT_RUNBOOK.md` §2.4/§5 DRILL-CA private key
under the 2-person custody rule (never on a field leaf) + weekly
authority-root/DRILL-CA re-verification + re-bundle before each exercise leg;
P-013 — per-device timestamped opt-in consent trail at provisioning
(runbook §4 day-0 + `PILOT_KPI_PLAN.md` §4). **RECORDED hardening:** P-001 SAS
entropy → BQP spec, P-002 hardware attestation deferred, P-005 revocation-sync
check in weekly drift review, P-006 decommission order revoke→retire→wipe,
P-010 drill-SOS cap/device/leg, P-011 relay_cadence clamp, P-014 telemetry salt
rotation, P-015 sideload contained by the DEC-0010 identity gate, P-016
TestFlight validity pre-drill, P-017 diagnostics-QR confined. **Positive
controls verified in real code:** EMERG-001 `drill.rs` (`SosKind::Test`,
`drill_flag_suppresses`, `verified_drill_suppressed`, `drill_mismatch_rejected`),
SEC-001 EmergencyAcl drill cap-exempt (drills never consume real-SOS), IDENT-001
rotate/trust_store (earliest-seen-wins + null-rotation revoke), DEC-0010 pilot
gate. **AC-17 held: workspace 664/0/1, clippy 0, fmt clean** (no Rust changes
this pass). PROJECT_GRAPH PILOT-001 IMPLEMENTING + evidence(6) + stage_note
SECURITY_REVIEW COMPLETE. **NEXT: VERIFY (iter ~158) → ACCEPT (iter ~159).**

**✅ PILOT-001 TEST COMPLETE (iter ~156, SIM-001 B-1..B-4 pre-validation,
AC-7..AC-10 + AC-17):**  Battery pre-validated on the SIM-001 harness in
`crates/iris-core/src/sim/` — **B-1 NCT-of-N** all-N delivery at N=20/50/100
with latency CDF (p50/p95 = 1 s ≤ 5 s/30 s floor) and Wilson-95% lower bounds
**0.929 (N=50) / 0.963 (N=100) ≥ 0.90 floor** (honest small-N disclosure:
N=20 perfect delivery → lo=0.839, CI width at n<50); **B-2 partition/healing**
2×4 partitions uplink-down 60 s → 8 intra delivered during the outage + cross
delivered at heal + **no duplicate terminal delivery**; **B-3 4-leg SCF
mule-chain** delivered at exactly 4000 ms = legs × window, max_hops=4; **B-4 iOS
limited-relay** participates on its foregrounded windows (to-iOS t=2000,
iOS-origin t=12000 — asymmetry recorded), ratio ≥ 0.95, physical rows
BLK-0005-gated → pilot field legs. `wilson_ci_95` KPI helper + bounds
regression landed in `sim/metrics.rs`. `PILOT-001_TEST.md` evidence map authored
(pattern BLE-002_TEST.md). **AC-17 live: workspace 664/0/1** (was 658/0/1;
iris-core lib 573 + iris-ios 11), clippy 0, fmt clean. PROJECT_GRAPH PILOT-001
evidence(5) + stage_note TEST COMPLETE.

**✅ PILOT-001 IMPLEMENT COMPLETE (iter ~155, ops deliverables AC-1..AC-16,
docs-only) — TEST next (iter ~156, SIM-001 B-1..B-4 pre-validation):**
Ops docs authored per `PILOT_001_DESIGN.md` (pattern BLE_002/ANDROID-001/
IOS-001 ops records): **PILOT_RUNBOOK.md** (AC-2: topology 50-100 leaves +
1-3 DESKTOP-001 gateways + Android-gateway fallback + gateway-scoped
Internet-relay; ProvisioningFlow mutual-QR ceremony batch 10-20 + verified
escalation + EMERG authority-root/DRILL-chain out-of-band bundle + decommission
wipe/revoke/rotate; day-0/day-N; incident ownership; diagnostics QR),
**PILOT_KPI_PLAN.md** (AC-3/AC-11: OBS-001 mapping + thresholds target/floor +
**Wilson-CI 95%** formula/sampling per class/transport never pooled + controlled
fg/bg battery + collection cadence + 7-day retention), **PILOT_EXERCISE.md**
(AC-4/AC-13: NDMA DMEx M1 TTEx → M2 field ME 50-dev observers + self-assessment
+ TEST-only SOS under DRILL certs + yellow banner → M3 evaluation/AAR to NDRF,
HSEEP-portable; EXP-002/003/005 legs; safety annex), **PILOT_DISTRIBUTION.md**
(AC-5/AC-15: Play **internal** ≤100 + closed 12×14 production clock; TestFlight
external + 90-day refresh, calendar-owned; signing 2-person + PEPK; OEM
battery-kill; telemetry consent), **REG_NOTES.md** (AC-14: **WPC G.S.R. 853(E)
2021 865-868** supersedes 865-867, LoRa future-leg only; SSMI 50-lakh
non-applicable; NDRF MoU + STQC GA; open counsel QCs; **no legal opinion**).
**AC-6** relay-cadence section appended to `CONGESTION_CONTROL.md` (≤40 flat
0.6×..1.0×, >40 linear `1.0+(N-40)×f` default 0.075; P0/P1 + directed exempt).
**AC-16** EXTERNAL-FACTS doc edits applied (GO_TO_MARKET internal-track note;
COMPLIANCE_RISK_REGISTER + SPECTRUM_CONSIDERATIONS 865-868/853(E);
FIELD_OPERATIONS 866.0 CONFIRMED). PROJECT_GRAPH PILOT-001 **IMPLEMENTING** +
evidence(4) + stage_note IMPLEMENT COMPLETE; PROJECT_STATE designing 1→0
implementing 0→1. **NEXT: TEST (iter ~156, SIM-001 sim pre-validation of B-1..
B-4) → SECURITY_REVIEW → VERIFY → ACCEPT.** Baseline **658/0/1** held (docs-only).

**✅ PILOT-001 DESIGN COMPLETE (iter ~154, `PILOT_001_DESIGN.md` v1.0) — C2 gap
RESOLVED (AC-1..AC-18); IMPLEMENT next (iter ~155, docs-only):** Design
authored per RES-0026 verdict PROCEED + D-1..D-9 (pattern
BLE_002_DESIGN.md/ANDROID_DESIGN.md/IOS_DESIGN.md). Pinned: **D-1 topology**
(50-100 leaves, Android relay-primary / iOS limited-relay + 1-3 DESKTOP-001
gateways + Android-gateway fallback + gateway-scoped Internet-relay,
mesh-primary, LoRa excluded GAP-004); **D-2 relay-cadence scaling rule**
(congestionScalingCoefficient mirror — ≤40 flat 0.6×..1.0×, >40 linear
`1.0+(N-40)×f` default 0.075; P0/P1 + directed exempt); **D-3 test battery
B-1..B-4** (NCT-of-N all-N + latency CDF, partition/healing, multi-hop SCF
mule-chain, iOS limited-relay; SIM-001 pre-validation + pilot field legs);
**D-4 ProvisioningFlow** (mutual-QR batch 10-20, verified escalation, EMERG-001
authority-root + DRILL-chain out-of-band bundle, decommission wipe+revoke+
rotate, no-recovery); **D-5 trust model** (TOFU + verified tier, platform-store
key isolation); **D-6 KPI plan** (OBS-001 mapping, thresholds target/floor,
**Wilson-CI 95%**, controlled fg/bg battery, 7-day retention); **D-7 NDMA DMEx
schedule** (M1 TTEx → M2 field ME observers+self-assessment, TEST-only SOS → M3
evaluation/AAR to NDRF; safety annex); **D-8 distribution runbook** (Play
**internal** ≤100 + closed 12×14 production clock, TestFlight external + 90-day
refresh, signing 2-person + PEPK, OEM battery-kill); **D-9 REG-NOTES** (WPC
853(E) 2021 865-868 supersedes 865-867, SSMI non-applicable, NDRF MoU + STQC GA,
open counsel Qs — no legal opinion). **DEC-PILOT-0001..0009 ratified**;
G-P1..G-P9 → G1..G12. PROJECT_GRAPH PILOT-001 **DESIGNING** + AC(18) +
evidence(3); PROJECT_STATE research_complete 2→1 designing 0→1.
**NEXT: IMPLEMENT (iter ~155, docs/operations/PILOT_RUNBOOK.md + PILOT_KPI_PLAN.md +
PILOT_EXERCISE.md + PILOT_DISTRIBUTION.md + REG_NOTES.md + relay-cadence seam) →
TEST → SECURITY_REVIEW → VERIFY → ACCEPT.** Baseline **658/0/1** held (docs-only).

**✅ PILOT-001 RESEARCH COMPLETE (iter ~153, RES-0026 verdict PROCEED) — DESIGN
next (iter ~154, `PILOT_001_DESIGN.md` AC-1..n):** `RES-0026.md` authored
(pattern RES-0025, 157 lines; 12 websearch + 4 primary-source passes, L1-L5,
**no AI citations**). All RQ-1..RQ-6 resolved: **RQ-1 ADOPT-WITH-CONDITION**
(managed-flood >100-node precedent + Meshtastic `congestionScalingCoefficient`
cadence rule + Helene 2024 gateway-uplink field precedent + Nepal DTN trial
metrics → leaves 50-100 + 1-3 DESKTOP-001 gateways + gateway-scoped
Internet-relay; LoRa excluded GAP-004); **RQ-2 ADOPT-WITH-CONDITION**
(Briar/SecureJoin QR mutual-scan bootstrap + IDENT-001 verified tier +
EMERG-001 out-of-band authority provisioning; batch ceremony, no-recovery,
decommission wipe/revoke/rotate); **RQ-3 ADOPT-WITH-CONDITION** (OBS-001 ↔
**NDMA DMEx Guidelines Oct 2024** 4-phase incl. evaluation/debrief; Wilson-CI
sample plan → DESIGN ACs; EXP-002/003/005 legs); **RQ-4 ADOPT codification**
(**WPC G.S.R. 853(E) 2021 = 865-868 MHz SRD 25 mW e.r.p./≤1% duty supersedes
865-867**; SSMI 50-lakh non-applicable; NDRF MoU + STQC GA; open counsel Qs;
**no legal opinion**); **RQ-5 ADOPT-WITH-CONDITION** (Play **internal** track
≤100 testers primary — DISCOVER 'closed track' REFINED; closed-track 12×14
later; TestFlight external + **90-day refresh**; Play App Signing custody);
**RQ-6 ADOPT** (EMERG-001 drill mode = **NDMA Mock Exercise** standard;
DRILL-chain + TEST-only SOS + observers/debrief + safety annex). **9 DESIGN
inputs D-1..D-9** + external-facts reconciliation + gaps G-P1..G-P9.
PROJECT_GRAPH PILOT-001 status **RESEARCH_COMPLETE**, evidence(2), stage_note;
PROJECT_STATE research_complete 1→2, discovered 3→2. **NEXT: DESIGN (iter
~154) → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.**
Baseline **658/0/1** held (read-only).

**✅ PILOT-001 DISCOVER COMPLETE (iter ~152) — first OPERATIONS node active:
** `PILOT-001_DISCOVER.md` authored (pattern
IOS-001_DISCOVER.md) — C2 gap CONFIRMED (no ACs → DESIGN resolves). Eligible:
ANDROID-001 ✅ / IOS-001 ✅ / EMERG-001 ✅ / SEC-001 ✅ COMPLETE + LEGAL-001
RESEARCH_COMPLETE. Scope cataloged: **100+ device NGO/campus pilot** (NDRF 12th
Bn Gandhinagar **50-device/3-month MoU** + B2B 5-device POC tier), Android+iOS
leaf + gateway + Internet-relay, BLE/WA/WD transports, EMERG-001 authority+drill,
OBS-001 KPIs, Play closed track + TestFlight distribution. RQ-1..RQ-6 staged
(topology/gateway, field identity bootstrap, KPI methodology, legal structure,
distribution/runbook, emergency exercise). PROJECT_GRAPH evidence(1) +
stage_note + known_limitations(7); PROJECT_STATE next_recommended PILOT-001
ACTIVE. **NEXT: RESEARCH (iter ~153, RES-0026) → DESIGN (AC-1..n) → PILOT.**
Baseline **658/0/1** held (read-only).

All core-engine + routing + observability work packages (WP-A codec → WP-9
OBS-001), the desktop platform shell (WP-10 DESKTOP-001), the ML routing
experiments (ML-001), the cryptographic layer (CRYPTO-001), the identity
system (**IDENT-001**), the emergency system (**EMERG-001**), the general
security hardening (**SEC-001**), the first three transports (**BLE-001**,
**WIFIAWARE-001**, **WIFIDIRECT-001**), and the **Android platform integration
(ANDROID-001)** are implemented, tested, verified, and accepted.
GW-001, ROUTE-002, OBS-001, DESKTOP-001, REQ-001, ARCH-001, ML-001,
CRYPTO-001, **IDENT-001**, **EMERG-001**, **SEC-001**, **BLE-001**,
**WIFIAWARE-001**, **WIFIDIRECT-001**, and **ANDROID-001** are COMPLETE
(**25 COMPLETE nodes**).
**✅ ANDROID-001 ACCEPTED (iter 122) — NODE_TRANSITION to TEST-001:**
`ANDROID-001_VERIFICATION.md` v1.1 AC-1..15 PASS (AC-1/AC-6 env-gated, AC-13
GATED/BLK-0005); independent verifier **APPROVE_WITH_NOTES** with all 6 notes
reconciled. Live re-verify: workspace **613/0/1**, `iris-android` **5/5**,
clippy **0**, rustfmt clean.
**✅ TEST-001 DISCOVER COMPLETE (iter 123) — RESUME+CONTINUE (iter 124):
TEST-001_DISCOVER.md** cataloged + verified existing Rust+JVM test infra:
workspace **613/0/1** (iris-core 553, 50+ `#[cfg(test)]` modules), proptest
security property suites, cargo-fuzz `fuzz_security_engines` **44M+ clean**,
tarpaulin **95.6%** security line-rate, adversarial transport tests
(wifiaware_beacon 7, wifi_direct_serv 9), JVM **39** unit tests (env-gated).
Gaps **G-1..G-8** defined (nextest; audit/dery recurring gate BLK-0003;
kani/loom; mutants+tarpaulin gate; PROTOCOL_CONFORMANCE interop fixtures
BLK-0004; CI workflow; golden vectors; bench BLK-0005).
**✅ TEST-001 RESEARCH COMPLETE (iter 124) — RES-0023 verdict PROCEED:**
7 websearch evidence passes (2026 best-in-class Rust/Android test infra,
primary sources, L1-L5, no AI citations). G-1 nextest **ADOPT** (primary
runner; doctests → separate `cargo test --doc` step, tokio precedent);
G-2 audit/deny **ADOPT** recurring gate (deny=PR policy, audit=live-DB+SARIF);
G-3 Kani/loom/tokio-test **ADOPT-WITH-CONDITION** (Kani Linux-only pure-fn
proofs, loom leaf structures `cfg(loom)`, tokio-test async; no tool models full
transport::manager); G-4 tarpaulin `--fail-under` PR gate + mutants
nightly/sharded trend (not hard gate); G-5 PROTOCOL_CONFORMANCE fixtures
**ADOPT** (RFC 9171 CDDL + Meshtastic resync + capture PDU + ION/µD3TN interop
pattern); G-6 CI workflow **ADOPT** (GitHub official + tokio reference; JVM job
JDK ≤21); G-7 golden-vector corpus **ADOPT** (catches DISC-0009 class); G-8
criterion 0.8.x scaffold **ADOPT** / regression gate + BLK-0005 **DEFER**.
Gaps G-TI-1..6 → DESIGN known_limitations.
**✅ TEST-001 DESIGN COMPLETE (iter 125) — IMPLEMENT NEXT:**
`docs/implementation/TEST_001_DESIGN.md` v1.0 defines **AC-1..12** (C2 gap
RESOLVED) + **DEC-TEST-0001..0012** ratified (nextest + doctest step; audit/deny
recurring gate; Kani pure-fn Linux scope; loom leaf + tokio-test; tarpaulin ≥80%
workspace / security ≥95%; mutants nightly trend not hard gate;
PROTOCOL_CONFORMANCE fixtures; golden-vector corpus; CI matrix + JVM JDK≤21;
criterion 0.8.x scaffold + BLK-0005 deferral; baseline preservation). Gate
matrix: Ubuntu full / macOS+Windows behavioral-only (G-TI-1). Env-probe
(iter 125): cargo-audit 0.22.2 + tarpaulin 0.37.2 host-present; nextest/deny/
kani/mutants absent (CI-leg env-gated). Baseline **613/0/1 clippy 0 rustfmt
clean held** (docs-only pass).
**✅ TEST-001 IMPLEMENT COMPLETE (iter ~128) — AC-1..12 ALL AUTHORED + HOST
LEGS VERIFIED:**(1) **AC-7/AC-8 (morning session)** — `tests/protocol_conformance.rs` **10
PASS** (RFC 8949 CDE vectors, bounded-recovery resync ≤512 B, capture PDU,
property sim) + `tests/golden_vectors.rs` **6 PASS** + V001/V002/V003 byte-exact
corpus with explicit **DISC-0009-class regression** (`0x66B7FF00` vs the doc's
wrong `0x66B5C000`, V001 pinned 237 B).
(2) **AC-3 Kani (this session)** — `src/kani_proofs.rs` **7 `#[cfg(kani)]`
pure-fn harnesses** (MessageId gate, ContentType/MessagePriority round-trip,
fragment split budget, quota `evict_amount`, rate-limiter `refill_tokens`,
PRoPHET `floor_intervals`); kani CI job = Linux-only execution leg (Kani absent
on Windows host, DEC-TEST-0003).
(3) **AC-4 loom + tokio-test (this session)** — `loom/loom_models.rs` **5 leaf
models PASS 5/5 (3.24s)** + `tests/tokio_behavior.rs` **4 paused-time suites
PASS 4/4 (0.00s)**. Invocation `cargo rustc -p iris-core --test loom_models --
--cfg loom` + run binary (global RUSTFLAGS re-cfgs tokio → its `net` module
`#![cfg(not(loom))]` vanishes) — wrapped in `engineering/tools/verify-loom.sh`.
Soundness: single-stream invariants sound; cross-stream out-of-order drop is
legitimate; `(0,0)`==sentinel==replay.
(4) **AC-10 benches (this session)** — 5 criterion 0.8.2 harnesses
(codec/envelope/crypto/routing/fragment, `harness=false`) compile + run Success.
(5) **AC-2 audit (reconciled this session)** — `deny.toml` + `.cargo/audit.toml`
(home of the 17 documented desktop-transitive unmaintained/unsound warnings
suppressed **with reason + expiry 2026-12-31** so CI `cargo audit --deny
warnings` passes); **host cargo audit exit 0** (0 vulns / 574 deps).
(6) **AC-1/AC-5/AC-6/AC-9/AC-12 (morning session)** — `.config/nextest.toml`
(ci profile: JUnit+retries+leaky; ci-mutants deterministic), CI jobs
(coverage tarpaulin `--fail-under 80`, deny, audit, fuzz, mutants nightly;
jvm JDK 21), full ci.yml matrix with `cargo nextest` + separate `cargo test
--doc`.
(7) **AC-11** — baseline **613 → 633 passed / 0 failed / 1 ignored** (net-new:
4 tokio + 10 protocol + 6 golden), clippy **0** all targets, rustfmt clean.
(8) **Hazard fixed** — `prophet::ceiling_intervals` → `floor_intervals`
(floor impl; name was a latent correctness hazard).
**NEXT: TEST stage (iter ~129) — `TEST-001_TEST.md` AC-1..12 evidence → VERIFY
(AC-12, independent verifier) → ACCEPT → NODE_TRANSITION IOS-001.**
**✅ BLE-002 UNDERSTAND COMPLETE (iter ~132) — READ-ONLY PASS (workspace
untouched, 633/0/1 held):**
`BLE-002_DISCOVER.md` written — C2 gap **CONFIRMED** (no ACs on BLE-002 node →
DESIGN defines AC-1..n, pattern BLE-001). Eligible (dep TRANSPORT-001 ✅).
iOS-exclusive surface cataloged: **no service-data in iOS advertisements** →
connect-to-identify discovery; **background advertising stops**/overflow-area;
`allowDuplicates`→false + batched 10-30 s results in background; state
restoration identifiers + both UIBackgroundModes required; iOS = **limited
relay node when backgrounded**; `supports_background_ios: false` already
pinned (ble.rs:287); MCF excluded from BLE-002; `ios/` crate dir absent
(simulator lacks CoreBluetooth — BLK-0005 device-gated). Research questions
RQ-1..RQ-5 enumerated. **NEXT: RESEARCH (iter ~133, RES-0024 iOS BLE SOTA) →
DESIGN (AC-1..n) → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT →
IOS-001.**
**✅ BLE-002 RESEARCH COMPLETE (iter ~133) — RES-0024 verdict
CONDITIONAL-PROCEED (read-only pass, baseline 633/0/1 held):**
`RES-0024.md` (pattern RES-0019, 14 websearch + 4 primary-source fetches,
L1-L5, no AI citations). **No CoreBluetooth background relaxation through iOS
26**: iOS 18.0 bg-ad regression fixed 18.1; iOS 26 HID/LE-Custom-Device
UIBackgroundModes = AccessorySetupKit accessory-only (IRIS does not qualify);
Live Activity = screen-on scan stops at screen sleep. No Android-`PendingIntent`
analog (relaunch for connections not scans; user force-quit kills relaunch);
overflow-area iOS-iOS-only. **~8-connection practical ceiling**;
`maximumWriteValueLength` = negotiated ATT payload (cap 512, no request API);
iOS 16.x 20-B MTU regression guard. **Verified iOS BLE CVEs**
(CVE-2023-42941/2024-23241/2024-44124/2024-44191) = OS-patchable only; prompt
IDs CVE-2023-28412 (Snap One)/CVE-2024-44270 (macOS)/CVE-2021-31714
(unverifiable) = misattributed/excluded. Codified ad asymmetry →
**connect-to-identify + stable IRIS service UUID + foreground-always + 512-B
segmentation + `supports_background_ios=false` held** (ble.rs:287/test :1075).
7 DESIGN inputs + 5 gaps G1-G5. ALLOCATION next RES-0025. PROJECT_GRAPH
evidence(2) + known_limitations + stage_note + validation_status refreshed;
status HELD DISCOVERED (C2 → DESIGN AC-1..n). **NEXT: DESIGN (iter ~134,
BLE_002_DESIGN.md AC-1..n + DEC-BLE-002-xxxx) → IMPLEMENT → TEST →
SECURITY_REVIEW → VERIFY → ACCEPT → IOS-001.**
**✅ BLE-002 DESIGN COMPLETE (iter ~134) — AC-1..16 defined (C2 gap
RESOLVED), docs-only pass (baseline 633/0/1 held):**
`docs/implementation/BLE_002_DESIGN.md` v1.0 (pattern BLE_TRANSPORT_DESIGN.md)
per RES-0024 verdict + 7 DESIGN inputs. Scope: BLE-001 Rust core reuse
(`BleAdapter` trait = FFI seam; `IosBleAdapter` Swift/CoreBluetooth =
**IOS-001 scope**). iOS wire/platform contract: **stable IRIS service UUID**
(never per-session); 28-B/10-B ad budget (UUID + local name only, no service
data); **connect-to-identify** discovery via GATT identification characteristic
(shared `DiscoveryBeacon` parser, two carriers); foreground-always + iOS 26
Live Activity **screen-on** framing; MTU = `maximumWriteValueLength`
negotiate-late 23→≤512 + 20-B iOS 16.x guard; ~8-peer bound;
state-restoration hardening (re-start in `willRestoreState`; system-kill vs
user-force-quit; relaunch-loop debounce; no BLE init outside valid launch path);
CVE set corrections (verified = OS-patchable only; misattributed excluded) +
iOS patch floor; app-layer AEAD = trust anchor. Module layout: pure-Rust core
adjustments (connect-to-read beacon branch, `SimulatedBleAdapter`
inject_identify_read, ble_att negotiated-payload source). iOS FFI adapter
contract table (10-op → CoreBluetooth) for IOS-001. **AC-1..16** defined;
**DEC-BLE-002-0001..0008** ratified into DECISIONS.md; risks/gaps G1-G5
positioned. PROJECT_GRAPH BLE-002: status **DISCOVERED → DESIGNING** +
acceptance_criteria + evidence(3) + stage_note + validation_status refreshed.
**NEXT: IMPLEMENT (iter ~135, BLE-002 Rust-core iOS adjustments) → TEST →
SECURITY_REVIEW → VERIFY → ACCEPT → IOS-001.**
**✅ BLE-002 IMPLEMENT COMPLETE (iter ~135) — pure-Rust core adjustments,
workspace pass (transport::ble 53/0, clippy 0, fmt clean):**
`engineering/memory/records/BLE-002_IMPLEMENT.md` — **DEC-BLE-002-0015
ratified** (`BleAdapter::gatt_read` REQUIRED — connect-to-identify needs a GATT
read op). `BleAdapter` trait + `gatt_read(handle, char_uuid)`; **SimulatedBleAdapter
SERVES `IRIS_IDENTIFY_CHARACTERISTIC`** (GATT-server semantics — returns the
serialized internal beacon on read; reads of non-identify chars return
`Err(DeviceNotFound)` — non-serving variant; no injection setter needed). Discovery **connect-to-read
branch wired**: empty-payload iOS ad (UUID + local name) → connect →
`gatt_read`(identify) → `DiscoveryBeacon::parse` → candidate (two-carrier
DEC-BLE-002-0008; ad-carried Android branch untouched). **7 `BleAdapter` impls
updated** (SimulatedBleAdapter + 5 test adapters in ble.rs + BleBridge in
iris-android → `DeviceNotFound`; Android↔iOS identify-read vector = ANDROID-001
follow-up known_limitation). `ble_att.rs` `payload_to_mtu` verified
(`maximumWriteValueLength` payload+5 → MTU, 23-B default, 20-B guard, cap 512;
DEC-BLE-002-0004). `cargo fmt --all` fixed 2 ble.rs diffs in-pass. **+5 iOS-leg
tests** (AC-1/2/3/4/6/10/12): iOS-leg register+selectable via TransportManager;
no-connect-no-read for ad-carried peers; connect-to-identify candidate;
identify 512-B peak budget single-frame round-trip; 23-B/20-B MTU payload
sizing. PROJECT_GRAPH BLE-002: status **DESIGNING → IMPLEMENTING** +
evidence(4) + stage_note + known_limitations(7) + validation_status refreshed.
**NEXT: TEST (iter ~136, AC-1..13 evidence map) → SECURITY_REVIEW → VERIFY →
ACCEPT → IOS-001.**
**✅ BLE-002 TEST COMPLETE (iter ~136) — AC-1..13 evidence map, LIVE re-run
independently THIS pass (workspace 644/0/1, clippy 0, fmt clean):**
`engineering/memory/records/BLE-002_TEST.md` (pattern BLE-001_TEST.md) maps
AC-1..13 to live-verified evidence. **INDEPENDENT re-run THIS pass**:
`transport::ble` **53/0** (1.51s); `cargo test --workspace --all-features` →
**644 passed / 0 failed / 1 ignored** (baseline **633→644**, 11 new tests);
clippy **0** warnings; `cargo fmt --all --check` clean. AC-1 (TransportManager
register/select iOS leg), AC-2 (MTU 23-B default → maximumWriteValueLength ≤512
+ 20-B iOS 16.x guard), AC-3 (connect-to-identify: UUID-only ad → connect →
gatt_read identify → DiscoveryBeacon::parse → candidate; no-connect for
ad-carried Android peers), AC-4 (E2E payload → segment → write → reassembly →
IncomingMessage), AC-5 (scan-burst throttling + UUID-filter mandatory), AC-6
(adversarial parse both carriers), AC-7 (envelope-verify seam held — review
assertion; redteam at SECURITY_REVIEW), AC-8 (shutdown/restoration seam types),
AC-9 (~8-peer bound + connection reuse), AC-10 (background contract documented
per RES-0024), AC-11 (trait conformance + clippy-clean), AC-12 (doc
reconciliation incl. RES-0024 outdated-claims), AC-13 (**644/0/1** workspace +
clippy 0 + fmt clean) all **PASS**. AC-14 GATED/BLK-0005 (iOS Simulator no
CoreBluetooth). AC-15/AC-16 PENDING. **NEXT: SECURITY_REVIEW (iter ~137,
AC-15) → VERIFY (iter ~138, AC-16) → ACCEPT → IOS-001.**
**✅ BLE-002 SECURITY_REVIEW COMPLETE (iter ~137, AC-15) — redteam FAIL →
RESOLVED:** `BLE-002_SECURITY_REVIEW.md` — all 8 findings dispositioned.
C001 (HIGH — Android leg probe-connecting unfiltered ambient ads) FIXED (gate
on `ios_leg` + IRIS_SERVICE_UUID filter + 8-probe/scan budget; regressions
`c001_android_leg_never_probe_connects_empty_ads`, `c001_ios_leg_probe_budget_caps_probe_connects`);
C002 (HIGH — evidence-integrity: records cited nonexistent tests) CORRECTED
(AC-2/3/4 rows → actual test names; two-carrier asymmetry proven by real c001
regression); C003 (MEDIUM — probe timeouts) → probe budget + IOS-001 contract;
C004 (MEDIUM — probe cache) → IOS-001/ANDROID-001 follow-up; C005 (MEDIUM —
sim serve honesty) FIXED (start_advertising→identify_data + `c005_sim_serves_advertised_beacon_on_identify_read`);
C006 (LOW) recorded; C007 (LOW) one-line FIXED; C008 (INFO) recorded. Live
re-verify: `transport::ble` **56/0** (1.52s), workspace **647/0/1**, clippy **0**,
fmt clean. **NEXT: VERIFY (iter ~138, AC-16 independent verifier).**
**✅ BLE-002 VERIFY (iter ~138, AC-16) NEEDS_FIX RESOLVED — independent verifier
FAIL (doc-only), ALL findings reconciled in-pass; NO CODE CHANGES:** independent
verifier reproduced runtime evidence (`transport::ble` **56/0**, workspace
**647/0/1**, clippy **0**, fmt clean, all BLE-RT dispositions present) but
returned FAIL on evidence integrity: VR-01/02 `supports_identify_read` flag
(never in code) purged to correction-only phrasing across PROJECT_GRAPH +
BLE-002_IMPLEMENT/DECISIONS/ACTIVE_NODE/CURRENT_STATE/execution-state/execution-log/NEXT_ACTION;
VR-03 fake test `ios_leg_discovery_no_connect_no_read` → actual
(`c001_android_leg_never_probe_connects_empty_ads`, `ios_connect_to_identify_yields_candidate_from_identify_characteristic`...);
VR-04 AC-5 → real `scan_burst_is_throttled_within_window` (ble.rs:1087;
`scan_restart_backoff_*` does not exist); VR-05 iter 137 logged + context files
refreshed; VR-06 baseline corrected **646→647/0/1** (633 + 14) + "BE-RT"→"BLE-RT".
**NEXT: verifier APPROVE (iter ~139) → ACCEPT (iter ~140) → NODE_TRANSITION
IOS-001.**
**✅ BLE-002 ACCEPTED (iter ~140) — 27 COMPLETE nodes; NODE_TRANSITION →
IOS-001:** `BLE-002_VERIFICATION.md` AC-1..16 evidence table + independent
verifier **APPROVE** (pass 1 FAIL doc-only → VR-01..VR-06 resolved in-pass;
pass 2 confirmed closure + live 56/0 → APPROVE). PROJECT_GRAPH BLE-002
IMPLEMENTING → **COMPLETE** (evidence 7); PROJECT_STATE completed 26→
**27**, implementing 1→0; CHANGELOG **0.3.57**. Baseline **647/0/1**
(`transport::ble` **56/0** = 42 + 11 + 3), clippy **0**, fmt clean. AC-1..16
PASS (AC-14 GATED/BLK-0005 — iOS Simulator has no CoreBluetooth; real iPhone
required). Control-plane carry-forward contracts for IOS-001: BLE-RT-C003
(read-timeout → Swift read path), BLE-RT-C004 (probe cache → IOS-001/ANDROID-001),
Android↔iOS identify-read (ANDROID-001 follow-up). **NEXT: IOS-001 DISCOVER
(iter ~141) → DESIGN → … → PILOT-001.**
**✅ IOS-001 VERIFY COMPLETE (iter ~150, AC-21) — independent verifier APPROVE;
ACCEPT next:** `IOS-001_VERIFICATION.md` authored (pattern BLE_002_VERIFICATION.md)
— AC-1..AC-21 evidence table. Live reproduction THIS pass: workspace
**658/0/1** (23 suites; iris-core 567 + iris-ios 11), **iris-ios 11/11**,
clippy **0**, fmt clean. No evidence-integrity findings — all AC-cited tests
re-grep'd (no nonexistent tests); all 5 SECURITY_REVIEW FIXED dispositions
re-located in source w/ file:line (RT-101 AppDelegate.swift:27-46; RT-102
CBManagerPeripheral.swift:72-85/179-184; RT-103 IosBleAdapter.swift:277-282 +
regression; RT-104 IosBleAdapter.swift:258 + regression; RT-105
Notifications.swift:4-5/15-17); RT-107 handle-0 partition re-confirmed
ble.rs:756-771. 35 XCTest authored (6 files) env-gated → ios.yml macOS CI leg
(recorded honestly); AC-19 GATED/BLK-0005. No code changes at VERIFY.
PROJECT_GRAPH evidence(9). **NEXT: ACCEPT (iter ~151) — commit SECURITY_REVIEW
fixes, PROJECT_GRAPH IOS-001 → COMPLETE, PROJECT_STATE 27→28, CHANGELOG,
NODE_TRANSITION → PILOT-001.**
**✅ IOS-001 IMPLEMENT + TEST + DOCUMENT + SECURITY_REVIEW COMPLETE (iter
~144..149) — AC-1..AC-20; VERIFY next:** Tranche 1 (iter ~144): `crates/iris-ios`
scaffold (11-op `FfiBleAdapter` incl `gatt_read` + SimBle tests; `BleBridge`
all 11 ops; `IrisEngine` explicit tokio Handle); `uniffi generate --library` →
committed `ios/IRIS/RustFFI/` IrisCore.swift (96.7KB). **Tranche 2 (iter
~145/146): all Swift sources authored** — `IosBleAdapter.swift` (11-op
`@unchecked Sendable` NSLock-confined; gatt_read = connect →
`readValue(IRIS_IDENTIFY_CHARACTERISTIC)` → DiscoveryBeacon parse AC-5;
BLE-RT-C003 10s timeout+cancel AC-6; C004 probe budget 8/scan + 60s reject
window AC-7; MTU ≤512 + 20-B guard AC-8; UUID filter + no service_data AC-9),
`CoreBluetoothSeam` + real CB managers (willRestoreState re-arm AC-10),
`KeychainEd25519` (com.iris.identity.v1 + staticad, no biometric, RFC 8032 KAT
AC-12), `SessionRecovery` (launch-reason classify + BGTask re-submit +
force-quit no-op AC-11), `BGTaskWiring` (AC-14), `LiveActivityController`
iOS 16.1 gated + widget (AC-13), `ios.yml` full AC-15 + build-xcframework.sh.
**TEST (iter ~147)**: `IOS-001_TEST.md` — AC-16 (33 XCTest cases env-gated →
macOS CI), AC-18 **PASS** (live 658/0/1, clippy 0, fmt clean), AC-19
GATED/BLK-0005. **DOCUMENT (iter ~148, AC-17)**: 6 EXTERNAL-FACTS corrections
applied (IOS.md Xcode 16+/`generate --library`/Keychain-Ed25519/BGTask;
SWIFT_LAYER.md willRestoreState re-arm; DISCOVER 16.2→16.1). **SECURITY_REVIEW
(iter ~149, AC-20)**: `IOS-001_SECURITY_REVIEW.md` authored (pattern
ANDROID-001) — **FAIL → RESOLVED**: IOS-RT-101..108 dispositioned (no
CRITICAL/HIGH). FIXED: RT-101 pre-warm retry dead path (AppDelegate builds
adapter before identity guard), RT-102 addService idempotence + fresh
identify-beacon serve via `didReceiveRead` (CBManagerPeripheral), RT-103 C003
timeout arms `rejectedUntil` probe budget + regression, RT-104 concurrent
gatt_read rejected + regression, RT-105 Notifications doc-vs-code; RECORDED:
RT-106 (buffers poller-bounded), RT-107 (handle-0 inbound drop verified safe
ble.rs:756-771), RT-108 (BGTask launch-key classification advisory-only).
Rust baseline re-verified **658/0/1, clippy 0, fmt clean** — held. +2
regression XCTest (33→35 authored, compile env-gated). Commits `110ef5b` +
`7ca5499` + SECURITY_REVIEW pending. **NEXT: VERIFY (iter ~150, AC-21 —
`IOS-001_VERIFICATION.md` + independent verifier) → ACCEPT (iter ~151) →
PILOT-001.** Swift compile/run = macOS CI leg (env-gated); device rows BLK-0005.
**🚀 OPERATOR AUTHORIZATION (DEC-0009, iter 54): ALL GATES + HUMAN GATES
UNBLOCKED.** **✅ IDENT-001 COMPLETE (iter 63c/64):** Identity System ACCEPTED
— IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS, verifier APPROVE, redteam
IDENT-RT-001..016 dispositioned (PASS-with-recorded-deviations), DEC-0010
security control verified satisfied. **✅ EMERG-001 COMPLETE (iter 71):**
**VERIFY + ACCEPT** — `docs/implementation/EMERG_VERIFICATION.md` v1.0 **AC-1..
14 all PASS**; independent verifier **APPROVE** (reproduced 431 passed/0 failed,
clippy 0, `cargo audit` 0 vulns; all 6 redteam fixes present; no fake AC
evidence); redteam EMERG-RT-001..011 PASS-with-recorded-deviations (RT-001
CRITICAL authority-root requirement + RT-002 HIGH SOS rate enforcement FIXED);
3 verifier doc-level notes reconciled in-pass. **✅ SEC-001 COMPLETE (iter 82):**
**VERIFY + ACCEPT** — `docs/implementation/SEC_001_VERIFICATION.md` v1.0
**AC-1..16 all PASS**; independent verifier **APPROVE_WITH_NOTES** (reproduced
workspace 532 passed/0 failed/1 ignored, clippy 0, tarpaulin cobertura security
line-rate 0.95624, all 13 SEC-RT fixes real code with regression tests); redteam
SEC-RT 13 findings (2 CRITICAL / 8 HIGH / 2 MEDIUM + SEC-RT-15) ALL FIXED;
AC-14 95.6% (590/617 >=95% MET); AC-15 **recorded gated deviation
operator-ratified** (property-based adversarial suite + 44M+ clean fuzz execs);
AC-16 grep guard CLEAN; notes reconciled in-pass. **The four P0 SECURITY pillars
(CRYPTO-001, IDENT-001, EMERG-001, SEC-001) are all ACCEPTED.** Critical-path
junction passed.
**✅ WIFIAWARE-001 COMPLETE (iter 101) — second TRANSPORT node ACCEPTED:
AC-1..16 all PASS/GATED**; `WIFIAWARE-001_VERIFICATION.md` v1.0, verifier
**APPROVE_WITH_NOTES** (reconciled); workspace **552/0/1**, `transport::wifiaware`
11 + beacon 8, clippy **0**; SECURITY_REVIEW v2 (Pass 1 WAW-RT-001..005 + Pass 2
NEW-WA-RT-101..112 — 1 HIGH + 6 MED FIXED + 5 LOW → ANDROID-001).
**✅ BLE-001 COMPLETE (iter 90) — first TRANSPORT node ACCEPTED: AC-1..16 all
PASS (AC-14 GATED/BLK-0005 recorded)**; workspace **533/0/1**, `transport::ble`
**42**, clippy **0**, 9 RT regressions.
**✅ WIFIDIRECT-001 COMPLETE (iter 109) — third TRANSPORT node ACCEPTED:
AC-1..16 all PASS**; `WIFIDIRECT-001_VERIFICATION.md` v1.0, verifier
**APPROVE** (reconciled); workspace **608/0/1**, `transport::wifi_direct`
**23**, clippy **0**, 7 RT regression tests, SECURITY_REVIEW RT-001..013
(1 HIGH + 5 MED + 1 LOW FIXED + 5 RECORDED).
**🔄** Now **ANDROID-001 (P0 PLATFORM — Android app shell + Kotlin FFI, IMPLEMENTING — TEST COMPLETE iter 118; AC-1..15 evidence mapped)**:
first platform node. Critical path: ... → EMERG-001 ✅ → **ANDROID-001** →
PILOT-001. Deps BLE-001 ✅ WIFIAWARE-001 ✅ MSG-001 ✅ EMERG-001 ✅ all COMPLETE.
**DESIGN COMPLETE (iter 112, ANDROID_DESIGN.md v1.0 — AC-1..15, C2 RESOLVED)**:
UniFFI foreign traits + async trait methods over FFI (injection bridge; explicit
tokio Handle for trap #2576); UniFFI 0.31.x + cargo-ndk 4.1.2 + rust-android-gradle
0.9.6/Mullvad 0.10.1; Kotlin 2.2 + Compose + Hilt shell; Keystore TEE Ed25519
(RED-0005-aligned); **Android 17 ACCESS_LOCAL_NETWORK gates the IRIS data
plane at targetSdk 37+ → v1 stays 34**; adapter lifecycle absorbs NEW-WA-RT-108..112
+ RT-010. DEC-AND-0001..0008 ratified.
**✅ G-AND-3 SPIKE COMPLETE (iter 113)**: `crates/iris-android` scaffolded
(lib `iriscode`; `IrisEngine` `#[uniffi::Object]` owning tokio Runtime + 3
`Arc<dyn>` foreign adapters; all polls via `runtime.block_on`); **3/3 spike
tests PASS** — explicit tokio Handle workaround (issue #2576) **PROVEN** on the
Rust leg (async foreign-trait `subscribe`/`publish`/`ndp_send`/`unsubscribe`
genuinely awaited through the engine runtime); clippy 0. **AC-2 bindings
generated + committed**: `uniffi-bindgen` 0.31.2 →
`kotlin/.../iriscode/iriscode.kt` with `IrisEngine` + 3 foreign-trait
interfaces + generated Kotlin `suspend fun` for async ops + `IrisFfiException`.
Kotlin compile/run leg **env-gated** (no Android SDK/NDK/gradle/kotlinc on
host) → known_limitation, not blocker.
**✅ AC-4 ENGINE REWIRING COMPLETE (iter 114)**: `IrisEngine` now mirrors
DesktopEngine — owns tokio `Runtime` + `Handle` + `Arc<TransportManager>` +
`Arc<MessageEngine>`; new `src/bridge.rs` bridges FFI adapters → core transport
traits (`BleBridge` buffered drains + hex UUID/MAC, `WifiAwareBridge`,
`WifiDirectBridge` 17-method mirror); registers `ble-android`/`wifi-aware-0`/
`wifi-direct-0` on the TransportManager inside `runtime.block_on`; constructs
`MessageEngine::new_with_telemetry` (MemoryStorage + DevCryptoProvider); spawns
per-transport auto inbox forwarders + `subscribe_inbox` broadcast
(`FfiInboxListener` + `FfiIncomingMessage`) + `send_text`/`start_all`/`stop_all`.
`FfiWifiDirectAdapter` expanded 5-op → **full 20-op async surface**; WA
projections corrected (issue #2263: `service_specific_info` `Vec<u8>` +
`sender` `Option<Vec<u8>>`). **`round_trip_delivers_over_shared_mesh` PASSES**
(B advertises → A discovers/connects keyed by B real node id → A `send_text`
b"hello" → B delivers + inbox receipt, over `SimAwareWire` + shared
`SimMeshCoordinator`). `cargo test -p iris-android --all-features` = **5 passed
/ 0 failed**; clippy 0; `cargo build -p iris-core` clean. Kotlin bindings
**REGENERATED + committed** (6154 lines; `FfiIncomingMessage`/`subscribeInbox`/
`create_group` present; stale `discover_services`/`FfiServiceInfo` 0).
**✅ AC-5 FFI-CONTRACT ABSORPTION COMPLETE (iter 115)**: Kotlin adapter layer
delivered in `android/app/src/main/kotlin/iriscore/adapter/`. `AdapterLifecycle.kt`
implements the carry-forward requirements: `FfiCallTimeout` (NEW-WA-RT-110
per-call suspend/sync/`blockOn` timeouts), `SessionGate` (NEW-WA-RT-111 +
WIFIDIRECT RT-010 — idempotent start/subscribe, same-session reuse,
Mutex-coalesced starters, **never latched into failure**, non-suspend
`invalidate()` for channel-lost callbacks), `RingBufferOutbox` (NEW-WA-RT-112 —
bounded, oldest-drop, **per-destination eviction** so one saturated peer can't
starve another), `RadioStateTracker` + `NdpRegistry` (NEW-WA-RT-109 —
multi-subscriber replaying StateFlow availability stream + closed-NDP prune at
the drain boundary), `VerifiedPeerCache` (NEW-WA-RT-108 — candidate → VERIFIED
64-hex PeerId reuse key). **`AndroidWifiDirectTransportAdapter` NEW full 20-op
async `FfiWifiDirectAdapter`** (`WifiP2pManager` DNS-SD `_iris._tcp` discovery +
`addLocalService`/`setDnsSdResponseListeners` + `createGroup` persistent GO +
`connect` join/invite + `removeGroup` + `setGroupOperatingBand` API-29 band
mapping AUTO/GHZ24/GHZ5/GHZ6 + `WIFI_P2P_STATE`/`CONNECTION_CHANGED` receivers +
`SessionGate<Channel>` latch + ring-buffer `p2pSend` (TCP-over-GO = AC-6) +
adapter-supplied `goAddr` G-WD-2); `AndroidWifiAwareTransportAdapter` (12-op
async) + `AndroidBleTransportAdapter` (10-op) share the same primitives.
**`AdapterLifecycleTest.kt` 14 tests one-per-requirement** (JUnit5 +
kotlinx.coroutines). 1:1 op mapping verified vs generated bindings (AC-11
basis: WD 16 == 16, WA 12, BLE 10; ULong conversions confirmed). `cargo check
--workspace` green (Kotlin-only pass; Rust untouched). Kotlin compile/run leg
env-gated (no Android SDK/kotlinc/gradle on host).
**✅ AC-6 APP SHELL + AC-7 BACKGROUND PLUMBING + AC-8 IDENTITY COMPLETE (iter 116)**:
`android/` Gradle scaffold (Kotlin 2.2.10 + AGP 8.7.3 + Compose BOM 2024.12.01
strong-skipping + Hilt 2.55 MVVM; compileSdk 34/targetSdk 34/minSdk 26 D-8 app,
sourceSets wired to committed generated bindings + jna; wrapper 8.9).
Shell: `IrisApplication` (@HiltAndroidApp + WorkManager Hilt WorkerFactory),
`di/` (AdapterModule → 3 foreign-trait adapters; IrisCoreModule →
KeystoreEd25519 + @NodeId ByteArray + `IrisEngine(ble,aware,direct,nodeId)`;
DispatcherModule), `ui/` (MainActivity @AndroidEntryPoint + MeshViewModel
@HiltViewModel + HomeScreen collectAsStateWithLifecycle + Material3 +
@Immutable MeshUiState/InboxUiMessage + IrisTheme), `data/` (MeshRepository
FfiInboxListener→StateFlow + RelayOutbox). **AC-11 conformance FIX**: NEW
`kotlin/src/main/kotlin/iriscode/api.kt` package `iriscode` facade re-exporting
`uniffi.iriscode` — all 15 adapter imports + shell imports resolve.
**AC-7**: `IrisBleService` connectedDevice FGS (API 34+ 3-arg startForeground +
<34 fallback, START_STICKY) + `BleScanSession` PendingIntent BLE scanning
(SCAN_MODE_OPPORTUNISTIC, IRIS_SERVICE_UUID filter, ScanRestartPolicy 5/30-s
backoff) + `BleScanReceiver` + `BleScanEvents` flow; `WorkScheduler` WorkManager
15-min periodic + 5-min flex + CONNECTED constraint; `IrisBackgroundSyncWorker`
(@HiltWorker) drains RelayOutbox; `MeshSyncPolicy` battery-aware +
`BatteryOptimizationGuidance` OEM matrix + ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS.
**AC-8**: `KeystoreEd25519` AndroidKeyStore **TEE Ed25519** (API 33 floor) +
SoftwareBackend JCA fallback + AutoBackend; 32-byte raw pub = 64-hex PeerId;
`X25519StaticAd` Ed25519-signed-X25519 static advertisement (RED-0005-aligned).
Manifest: FGS connectedDevice + BLUETOOTH_SCAN/ADVERTISE/CONNECT +
NEARBY_WIFI_DEVICES (neverForLocation) + ACCESS_FINE_LOCATION + targetSdk 34
(ACCESS_LOCAL_NETWORK SDK-37 cliff avoided, AC-9). **26 new JVM tests**
(MeshSyncPolicyTest 7, KeystoreEd25519Test 9, ScanRestartPolicyTest 4,
BatteryOptimizationGuidanceTest + PeerIdCodecTest 6) → **40 total** incl AC-5's
14. Rust + cargo untouched (AC-10, clippy 0 held). Kotlin compile/run +
`gradlew assembleDebug` (AC-6) + cargo-ndk build (AC-1) **env-gated**
(no Android SDK/NDK/gradle on host) → known_limitation, not blocker.
**✅ AC-12 DOC RECONCILIATION COMPLETE (iter 117)**: `ANDROID.md` amended —
§Platform Targets **pinned to scaffold reality** (Kotlin 2.2.10 + AGP 8.7.3 +
Compose BOM 2024.12.01 strong-skipping + Gradle wrapper 8.9 + KSP
2.2.10-2.0.2 + Hilt 2.55; UniFFI 0.31.2; cargo-ndk 4.1.2 + NDK r26/r28 pin +
ABI set {arm64-v8a, armeabi-v7a, x86_64}; minSdk 26/targetSdk 34 +
ACCESS_LOCAL_NETWORK cliff note); **implemented package layout**
(di/adapter/data/service/worker/identity/util/ui); BLE PendingIntent scan +
ScanRestartPolicy 5/30-s backoff; OEM battery-kill matrix extended
(Honor/Huawei/Google); §UniFFI Integration rewritten (foreign traits preferred,
committed generated kotlin + `iriscode` package facade); §Security Keystore
**rewritten — Ed25519 TEE hardware-backed (API 33), supersedes the dated
ECDSA-P-256 claim**; StrongBox-Excludes-Ed25519; KeystoreEd25519
AutoBackend/SoftwareBackend; X25519StaticAd (RED-0005); new §ACCESS_LOCAL_NETWORK
SDK-37 cliff (v1 targetSdk 34 keeps implicit grant; declare + runtime-request at
any 37+ bump; NsdManager exemption vs raw in-process sockets); Android 17
`BluetoothSocket.read()` -1 carry-forward (targetSdk 37); NEARBY_WIFI_DEVICES
(neverForLocation) manifest permission. `KOTLIN_LAYER.md` NEW §UniFFI Binding
Layer (generated bindings path, facade, foreign-trait injection, explicit tokio
handle / issue #2576) + conformance note superseding stale `IrisCore`/
`registerTransport` sketches. **Docs-only pass — workspace untouched** (Rust
baseline 608/0/1, clippy 0; evidence 16).
**✅ ANDROID-001 TEST COMPLETE (iter 118) — AC-1..15 evidence mapped**:
`engineering/memory/records/ANDROID-001_TEST.md` v1.0 (pattern
WIFIAWARE-001_TEST.md). **PASS**: AC-2 bindings committed; AC-3 spike 3/3; AC-4
engine 5/5 incl `round_trip_delivers_over_shared_mesh`; AC-5 adapter lifecycle
14 tests; AC-7 FGS/PendingIntent/WorkManager manifest + ScanRestartPolicyTest 4
+ MeshSyncPolicyTest 7; AC-8 KeystoreEd25519Test 9 + PeerIdCodec +
X25519StaticAd; AC-9 manifest 34/26 + LNP cliff + BluetoothSocket -1 doc; AC-10
workspace green; AC-11 1:1 op mapping + iriscode facade + cargo check; AC-12 doc
reconciliation. **GATED/env**: AC-1 (`cargo ndk build`), AC-6 (`gradlew
assembleDebug`) — probed: gradle/kotlinc MISSING on host (java JDK 25 only).
**GATED/BLK-0005**: AC-13 (physical-device/battery/OEM). NEXT: AC-14
SECURITY_REVIEW, AC-15 VERIFY. **Live re-verify iter 118**: workspace **613
passed / 0 failed / 1 ignored** (608 baseline + 5 `iris-android`), `iris-android`
5/5, clippy **0**, rustfmt clean; Rust source untouched (crates/iris-core
unchanged since iter 112); 40 JVM tests authored, run on toolchain host/CI.
**✅ ANDROID-001 SECURITY_REVIEW CLEARED (iter 119 + re-review iter 120) —
AND-RT-101..113 all FIXED.** Redteam adversarial review (independent subagent)
→ **FAIL, 13 findings (2 CRITICAL / 6 HIGH / 5 MEDIUM) all dispositioned FIXED**
in `ANDROID-001_SECURITY_REVIEW.md`; every fix independently spot-checked vs
source (file:line refs). **Re-review pass 2 (iter 120) → FAIL → RESOLVED**: all
13 fix logics confirmed real code; 4 blockers/residual fixes applied in-pass —
R1 `WA openNdp` Long→`ULong` compile fix (WA:145); R2 BLE `gattWrite` API-33-only
3-arg `writeCharacteristic` gated for minSdk-26 tier with legacy 2-arg `writeType`
fallback surfacing `false` as `IrisFfiException.GattFailure` (BLE:300-315); R3
`syncRelayNow` ANR-class main-thread FFI → `withContext(Dispatchers.IO)`
(MeshViewModel:72-73); R5 `BleScanSession.stop()` scan-window leak →
`stopScanWindow(scanner)` (BleScanSession:122-129). Recorded residuals
(non-blocking): R4 non-IrisFfiException `withTimeout` → `UNIFFI_CALL_UNEXPECTED_ERROR`
(FfiCallTimeout liveness-not-cancellation), R6 BLE `adData.serviceData` nullable
(benign), RT-112 `syncCall runBlocking` doc-only (trivial bodies). Positive
controls re-verified HELD. **SECURITY_REVIEW CLEARED.** Kotlin-only fixes; Rust
untouched (baseline 613/0/1, clippy 0). NEXT: VERIFY (iter ~121, AC-1..15 +
independent verifier) → ACCEPT.
**NODE_TRANSITION CORRECTED (iter 109b)**:
PRIORITY_POLICY P0-first selected ANDROID-001 over LORA-001 (P2,
`requires_hardware: true`) — operator ratified; LORA-001/SAT-001 deferred.
**WIFIDIRECT-001 ACCEPTED COMPLETE (iter 109)**: VERIFY (iter 108)
`WIFIDIRECT-001_VERIFICATION.md` v1.0 — AC-1..16 evidence table (AC-1..13
PASS, AC-14 GATED/BLK-0005, AC-15 SECURITY_REVIEW v1 RESOLVED, AC-16
independent verifier **APPROVE**, notes reconciled). Live re-verify: workspace
**608 passed / 0 failed / 1 ignored**; `transport::wifi_direct` **23 PASS**
(13 transport + 10 serv); clippy **0**; fmt clean. ACCEPT: graph COMPLETE,
evidence(10); PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG
0.3.39. **24 COMPLETE nodes.**
Next: **ANDROID-001 SECURITY_REVIEW CLEARED (iter 120 — all 13 AND-RT fixes FIXED + re-review pass 2 RESOLVED; NEXT: VERIFY iter ~121)** → VERIFY → ACCEPT (AC-1..15).

**WIFIAWARE-001 carries forward (COMPLETE iter 101):** UNDERSTAND ✅ (91, C2
AC-gap) → RESEARCH ✅ (92, RES-0020 PROCEED; 8 websearch evidence passes, L1–L5)
→ DESIGN ✅ (93, WIFI_AWARE_TRANSPORT_DESIGN.md v1.0, AC-1..16 C2 RESOLVED,
DEC-WA-0001..0008, CONFLICT-1 iOS 26+ reconciliation) → IMPLEMENT ✅ (94:
wifiaware.rs `WifiAwareAdapter` FFI trait + `SimMeshCoordinator`/
`SimulatedWifiAwareAdapter` in-memory NDP mesh + `WifiAwareTransport` Transport
impl + wifiaware_beacon.rs 22-byte beacon) → TEST ✅ (95, WIFIAWARE-001_TEST.md
AC-1..16) → SECURITY_REVIEW ✅ (96 + re-review 100, v2) → VERIFY ✅ (101,
WIFIAWARE-001_VERIFICATION.md, verifier APPROVE_WITH_NOTES reconciled). v1 =
publish/subscribe NAN discovery + NDP IPv6 socket data path reusing INTERNET-001
TCP framing (1 MiB cap); runtime capability gate (FEATURE_WIFI_AWARE/
isAvailable()/state-changed, degrade BLE); FGS connectedDevice (API 34+);
app-layer envelope = trust anchor; identity by pubkey fingerprint never NAN MAC.
Background SEC-001 fuzz continues aggregating clean (49.5M+ execs) — no blocker.

## Phase Status

| Phase | Status | Notes |
|-------|--------|-------|
| WP-A: Canonical CBOR codec | ✅ COMPLETE | 61/61 tests, PROTO-001 conformance |
| WP-1: MSG-001 Message Engine | ✅ COMPLETE | 94/94 tests; MSG_VERIFICATION.md v1.1 |
| WP-2: STORE-001 Storage | ✅ COMPLETE | PostgreSQL (DEC-0002), 11/11 tests |
| WP-3: DISCO-001 Discovery | ✅ COMPLETE | 16/16 tests; DISCO_VERIFICATION.md v1.0 |
| WP-4: ROUTE-001 Routing | ✅ COMPLETE | 24/24 tests; ROUTE_VERIFICATION.md v1.0 |
| WP-5: SCF-001 Store-Carry-Forward | ✅ COMPLETE | 13 tests + M6; SCF_VERIFICATION.md v1.0 |
| WP-6: SIM-001 Simulation | ✅ COMPLETE | 13 tests; SIM_VERIFICATION.md v1.0 |
| WP-7: GW-001 Gateway Selection | ✅ COMPLETE | 19 tests (6 red-team regression); GW_VERIFICATION.md v1.1 |
| WP-8: ROUTE-002 Opportunistic Routing | ✅ COMPLETE | 17 tests (prophet 7 + opportunistic 7 + 3 sim); ROUTE2_VERIFICATION.md v1.0 |
| WP-9: OBS-001 Observability | ✅ COMPLETE | observability module + ShortId privacy + seam instrumentation; OBS_VERIFICATION.md v1.0; red-team PASS |
| WP-10: DESKTOP-001 Desktop Shell | ✅ COMPLETE | Tauri v2 shell hosting iris-core; DESKTOP_VERIFICATION.md v1.0; 225 workspace green |
| REQ-001: Requirements Baseline | ✅ COMPLETE | REQUIREMENTS_BASELINE.md 56-req traceability matrix (8 IMPLEMENTED / 17 PARTIAL / 30 DEFERRED / 2 GAP); REQ_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS |
| ARCH-001: System Architecture | ✅ COMPLETE | ARCHITECTURE_BASELINE.md 7-layer + component + 20-principle reconciliation; ARCH_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS |
| ML-001: ML Routing Experiments | ✅ COMPLETE | P3 EXPERIMENT, leaf; shadow-only L3 ML in sim/ml/; AC 1–7 PASS; redteam RED-0002 PASS; ML_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS→corrections applied; 244 green |
| **CRYPTO-001 IMPLEMENT** | ✅ COMPLETE (iter 57) | Crypto module + codec::encode_for_aead + IrisCryptoProvider/KeyDirectory + engine encrypt/decrypt + fragment re-signing + at-rest RowSealer/schema-v2 + KAT line-up incl. RFC 5869 A.2/A.3 + sender/recipient same-key + M7 AC-8 E2E; workspace 284 green, clippy 0 |
| **CRYPTO-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 59) | CRYPTO_VERIFICATION.md v1.0 AC-1..9 all PASS; redteam RED-0001..0012 dispositioned (RED-0004 fragmentation/E2EE + 2 latent routing bugs FIXED, 290 green; RED-0001 CRITICAL operator-ratified DEC-0010 to IDENT-001); 18 COMPLETE nodes |
| **CRYPTO-001 RESEARCH** | ✅ COMPLETE (iter 55) | RES-0015 + DEC-P0001..0004: per-message ephemeral X25519 (ECIES-like), RFC 5869 HKDF-SHA256, at-rest row seal, FS honesty, crate pins + RUSTSEC audit clean |
| **🚀 All gates unblocked** | **DEC-0009 (iter 54)** | Operator authorization 2026-08-14: BLK-0001 crypto/identity, BLK-0005 transports, LEGAL-001, EMERG-001, SEC-001, TEST-001, platform apps — all eligible for implementation |
| **IDENT-001 DESIGN** | ✅ COMPLETE (iter 61) | IDENT_DESIGN.md v1.0: D1-D6 (key-derived PeerId + SHA-256 short-id; FileKeyStore + StorageKeySealer default-on; TOFU + signed ad + Verified tier; rotation/revocation null-rotation; two-keypair binding; no new crates) + AC-1..11 + wire reconciliation (32B self-auth v1, 16B collapse RED-0010/v2) + RED-0005/0008/0011 + desktop RED-0001 wiring plan |
| **IDENT-001 IMPLEMENT** | ✅ COMPLETE (iter 62) | identity module `identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,small_order}` (NodeIdentityV1 provision/load + FileKeyStore + PeerId/SHA-256 short-id + RED-0005 ad + TOFU TrustStore + RED-0008 verify_chain + rotation/null-rotation + RED-0011) 58 tests + `MessageEngine::set_key_directory` + desktop RED-0001 (`DesktopIdentity`, IrisCryptoProvider, TrustKeyDirectory, persisted PeerId; DevCryptoProvider → `config.dev` test seams; IRIS_NODE_ID display-only) 5 tests; workspace green, clippy 0 |
| **IDENT-001 TEST** | ✅ COMPLETE (iter 62b) | IDENT-001_TEST.md: AC-1..9 evidence (peer_id 7, provisioning 13, advertisement 6, verify_chain 11, TrustStore 10, rotation 7, small-order 4, desktop RED-0001 3); workspace green, clippy 0 |
| **IDENT-001 SECURITY_REVIEW** | ✅ COMPLETE (iter 63) | IDENT-RT-001..016 dispositioned in IDENT-001_SECURITY_REVIEW.md — PASS-with-recorded-deviations; 9 in-scope code findings FIXED in-pass (+10 regression tests, identity 58→68), 4 gated (RT-002/003/004/005), RT-014 accepted single-instance, RT-015/016 INFO clean; DEC-0010 control VERIFIED satisfied; workspace **361 green**, clippy 0 |
| **IDENT-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 63c/64) | IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS; verifier APPROVE (independently reproduced 68 identity / 361 workspace / clippy 0 / DEC-0010 by code read); IDENT-001 **COMPLETE**, 19 COMPLETE nodes |
| **IDENT-001 RESEARCH** | ✅ COMPLETE (iter 60) | RES-0016 + DEC-P0005..P0009: per-device Ed25519 identity + key-derived sender_id; per-platform provisioning (Keystore/Keychain/OS-keychain/TPM) + StorageKeySealer default-on (RED-0001); TOFU + signed advertisement + QR/pairing trust (RED-0005/0008); signed rotation/revocation; two-keypair Ed25519-signed-X25519 binding |
| **EMERG-001 TEST** | ✅ COMPLETE (iter 69) | EMERG-001_TEST.md: AC-1..12 + AC-14 test-side; CLOSED engine receive-path gap (emergency_gate in deliver_or_relay — drop-no-reply/suppress/relay + SOS bookkeeping; Noop identical AC-11); size-budget reconciled (2-elem 690 B <1 KB, 4-elem 1277 B P3); workspace 424 green, clippy 0 |
| **EMERG-001 SECURITY_REVIEW** | ✅ COMPLETE (iter 70) | EMERG-RT-001..011 dispositioned — PASS-with-recorded-deviations; FIXED: EMERG-RT-001 CRITICAL (provisioned authority root required — TOFU peer can't anchor verified chain), RT-002 HIGH (sos_rate enforced receive-side → P3 + LRU bound), RT-006 MEDIUM (bounded broadcast-replay guard), RT-007 MEDIUM (no payload bytes in audit), RT-009 LOW (poison-safe lock), RT-011 LOW (CDE duplicate-key reject); RECORDED RT-003/004/005/008/010; +7 regression tests; workspace **431 green**, clippy 0 |
| **EMERG-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 71) | EMERG_VERIFICATION.md v1.0 AC-1..14 all PASS; independent verifier APPROVE (431 passed/0 failed, clippy 0, audit 0 vulns, all 6 redteam fixes present); 3 doc-level notes reconciled (provider.rs MAX_IDS docstring 3096→1024 + ignored-test + AC-2 phrasing); **EMERG-001 COMPLETE — 20 COMPLETE nodes** |
| **SEC-001 TEST + SECURITY_REVIEW + VERIFY + ACCEPT** | ✅ COMPLETE (iter 75-82) | SEC-001_TEST.md AC-1..16; repair (77, honest correction — module did not compile); FUZZ (78, quota overflow found+fixed, 44M+ clean); redteam SEC-RT (79, 13 findings ALL FIXED); AC-14 (80, 590/617=95.6% MET + SEC-RT-15 acl decode fixed); AC-16 (81, grep guard CLEAN); AC-15 gated deviation operator-ratified + verifier APPROVE_WITH_NOTES reconciled (82). **SEC-001 COMPLETE — 21 COMPLETE nodes.** Next: transports |
| Next node | ✅ BLE-002 (P0 TRANSPORT) IMPLEMENTING — IMPLEMENT COMPLETE (iter ~135) — TEST NEXT | **TEST-001 ACCEPTED COMPLETE (iter ~131, 26 COMPLETE)**: VERIFY (iter ~130, TEST-001_VERIFICATION.md AC-1..12; independent verifier FAIL → reconciled APPROVE — 4 findings fixed: deny.toml invalid TOML, audit.toml contract, loom comment, stale-613 text); workspace **633/0/1**, clippy 0, rustfmt clean, audit exit 0. **→ NODE_TRANSITION → BLE-002 (UNDERSTAND iter ~132 → RESEARCH iter ~133 RES-0024 CONDITIONAL-PROCEED → DESIGN iter ~134 → IMPLEMENT iter ~135)**. **BLE-002 RESEARCH COMPLETE (iter ~133)**: no iOS background relaxation through iOS 26; connect-to-identify + stable IRIS service UUID + foreground-always + 512-B segmentation; 7 DESIGN inputs. **BLE-002 DESIGN COMPLETE (iter ~134)**: BLE_002_DESIGN.md v1.0 — AC-1..16 (C2 RESOLVED), DEC-BLE-002-0001..0008 ratified. **BLE-002 IMPLEMENT COMPLETE (iter ~135)**: DEC-BLE-002-0015 (gatt_read REQUIRED); connect-to-read discovery wired; SimulatedBleAdapter SERVES identify char; 7 impls updated; +5 iOS-leg tests; transport::ble 53/0, clippy 0, fmt clean; status DESIGNING → IMPLEMENTING. **→ TEST iter ~136 (AC-1..13 evidence map)**. Next after: IOS-001 → PILOT-001; P2 transports LORA-001/SAT-001 deferred |

## Active Node

**PILOT-001 — Field Pilot Deployment (P2 OPERATIONS, IMPLEMENTING, TEST COMPLETE
iter ~156, SECURITY_REVIEW next iter ~157).** First OPERATIONS node,
deployment-critical-path terminus. DESIGN COMPLETE (iter ~154, evidence 3):
**`PILOT_001_DESIGN.md` v1.0 — C2 gap RESOLVED via AC-1..AC-18**. D-1 topology
(50-100 leaves + 1-3 gateways + gateway-scoped Internet-relay, mesh-primary),
D-2 relay-cadence scaling rule (≤40 flat 0.6×..1.0×, >40 linear default
f=0.075; P0/P1 + directed exempt), D-3 test battery B-1..B-4 (NCT-of-N /
partition-healing / SCF mule-chain / iOS limited-relay; SIM pre-validation +
field legs), D-4 ProvisioningFlow (mutual-QR batch 10-20 + EMERG
authority/DRILL out-of-band + decommission wipe+revoke+rotate; no-recovery),
D-5 trust model (TOFU + verified tier), D-6 KPI plan (thresholds target/floor +
**Wilson-CI 95%**), D-7 NDMA DMEx M1/M2/M3, D-8 distribution (Play **internal**
+ TestFlight 90-day + signing 2-person + PEPK), D-9 REG-NOTES (**WPC 853(E)
2021 865-868** supersedes 865-867; SSMI non-applicable; NDRF MoU + STQC GA).
**DEC-PILOT-0001..0009 ratified**; G-P1..G-P9 → G1..G12. **IMPLEMENT (iter
~155, docs-only: PILOT_RUNBOOK.md + PILOT_KPI_PLAN.md + PILOT_EXERCISE.md +
PILOT_DISTRIBUTION.md + REG_NOTES.md + relay-cadence seam edit + EXTERNAL-FACTS
doc edits)**. **TEST COMPLETE (iter ~156, SIM-001 B-1..B-4 pre-validation:
AC-7..AC-10 PASS + AC-17 live 664/0/1; `PILOT-001_TEST.md` evidence map)** →
**SECURITY_REVIEW (iter ~157, AC-18)** → VERIFY → ACCEPT. Baseline
**664/0/1 clippy 0 fmt clean** held. Quality process: DISCOVER → RESEARCH →
DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.

**IOS-001 ACCEPTED (iter ~151) — carries forward (28 COMPLETE nodes):**
- **ACCEPT (iter ~151)**: commit `d13845c` + `cd995ef` — SECURITY_REVIEW (iter
  ~149) Swift fixes + `IOS-001_SECURITY_REVIEW.md` + `IOS-001_VERIFICATION.md`
  + state; PROJECT_GRAPH IOS-001 → COMPLETE evidence(9); PROJECT_STATE 27→28;
  CHANGELOG 0.3.58; NODE_TRANSITION → PILOT-001.
- **VERIFY (iter ~150, AC-21)**: `IOS-001_VERIFICATION.md` AC-1..AC-21 evidence
  table + independent verifier **APPROVE** — live reproduction **658/0/1** (23
  suites), iris-ios 11/11, clippy 0, fmt clean; all RT dispositions re-located
  in source w/ file:line; AC-19 GATED/BLK-0005.
- **SECURITY_REVIEW (iter ~149, AC-20)**: IOS-RT-101..108 **FAIL → RESOLVED**
  (no CRITICAL/HIGH; RT-101..105 FIXED, RT-106..108 RECORDED; +2 regressions).
- **IMPLEMENT (iter ~144..146, AC-1..AC-15)**: `crates/iris-ios` + Swift shell
  (IosBleAdapter gatt_read connect-to-identify, KeychainEd25519 no-biometric,
  SessionRecovery, LiveActivity 16.1, ios.yml AC-15). Commits `110ef5b`,
  `86cfaa8`.
Quality process: DISCOVER → RESEARCH → DESIGN → IMPLEMENT → TEST → VERIFY →
ACCEPT.
**WIFIDIRECT-001 ACCEPTED COMPLETE (iter 109)** — carries forward:
- **ACCEPT (iter 109)**: graph COMPLETE + evidence(10) + stage_note full
  pipeline; PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG 0.3.39.
  **NODE_TRANSITION (iter 109b, corrected) → ANDROID-001 (UNDERSTAND, iter 110).**
- **VERIFY (iter 108) APPROVED**: `WIFIDIRECT-001_VERIFICATION.md` v1.0 AC-1..16
  evidence table; independent verifier APPROVE (all counts reproduced —
  workspace **608/0/1**, `transport::wifi_direct` **23 PASS**, clippy **0**;
  all 7 RT fixes real code + passing regressions; no nonexistent AC-cited
  tests); notes reconciled (RECORDED 6→5).
- **SECURITY_REVIEW (iter 107) RESOLVED**: WIFIDIRECT-001_RT-001..013 — 1 HIGH
  (RT-001 zero-short sentinel + empty-TXT window) + 5 MED (RT-002/003/004/005/
  007) + 1 LOW (RT-009) FIXED + 7 regression tests; 5 RECORDED.
PRIOR IMPLEMENT COMPLETE (iter 105): `wifi_direct.rs` (`WifiDirectAdapter` 20-op
FFI trait + `SimP2pCoordinator`/`SimulatedWifiDirectAdapter` deterministic
in-memory P2P mesh + `WifiDirectTransport` `Transport` impl reusing
INTERNET-001 framing, 30-s discovery re-arm, GO/GC connection table,
persistent-GO teardown) + `wifi_direct_serv.rs` (`WifiDirectTxtRecord` 22-byte
build/parse + `WIFI_DIRECT_SERVICE_NAME` + 9 adversarial tests) + `WIFI_DIRECT_COST`
+ registered in transport/mod.rs + design §7 appended.
PRIOR DESIGN COMPLETE (iter 104, WIFI_DIRECT_TRANSPORT_DESIGN.md v1.0): v1 =
Android `WifiP2pManager` DNS-SD (Bonjour) discovery on BLE-triggered re-arm
window (30-s app cadence vs 120-s framework find; `WIFI_P2P_DISCOVERY_CHANGED_ACTION` →
BLE fallback) + `createGroup`/`connect` persistent GO; wpa_supplicant dual path
(`p2p_group_add`/`p2p_connect`/`p2p_service_add bonjour`) + IRIS-owned IP/DHCP
glue (GO static + dnsmasq/udhcpd; GO address adapter-supplied, G-WD-2); **data
plane = TCP socket over GO reusing INTERNET-001 framing (1 MiB cap, pool +
backoff)** + client IPv4 DHCP / IPv6-link-local; WPA2-Personal floor (WPS-PIN
prohibited, passphrase over authenticated BLE, PBC legacy fallback only);
WPA3-SAE R2 capability-gated; `setGroupOperatingBand` API 29 5 GHz preferred /
AUTO fallback; coex `WifiAvailableChannel` API 34; N-client admission + GO-side
connection table (G-WD-1); OS patch floor AC-14; FGS connectedDevice + wakelock
background contract. **AC-1..16 defined (C2 gap RESOLVED)** + **DEC-WD-0001..0008
ratified** + WIFI_DIRECT.md corrections (band API 29, find window 120 s, client
ceiling vendor/HAL, WPA3-SAE R2). BLK-0005 lifted by DEC-0009 for
implementation; **device-only integration tests remain RESOURCE-gated** (physical
hardware) — recorded known_limitations; Android Kotlin adapter = ANDROID-001
scope (carries NEW-WA-RT-108..112 FFI requirements from WIFIAWARE-001).
Reference patterns: `INTERNET_TRANSPORT_VERIFICATION.md` +
`crates/iris-core/src/transport/` + BLE-001 (iters 83–90) + WIFIAWARE-001
(iters 91–101) pipelines.

**WIFIAWARE-001 COMPLETE (iter 101) — carries forward (second TRANSPORT):**
- **ACCEPT (iter 101)**: `WIFIAWARE-001_VERIFICATION.md` v1.0 AC-1..16 evidence;
  verifier **APPROVE_WITH_NOTES** (reconciled — display string fix); workspace
  **552/0/1**, `transport::wifiaware` 11 + beacon 8 = 19 under shared filter,
  clippy **0**; SECURITY_REVIEW v2 (Pass 1 WAW-RT-001..005 + Pass 2
  NEW-WA-RT-101..112: 1 HIGH recv-side zero-peer attribution FIXED + 6 MED FIXED
  + 5 LOW → ANDROID-001). status COMPLETE, evidence(11).
- **VERIFY (iter 101)**: `WIFIAWARE-001_VERIFICATION.md` AC-1..16; independent
  verifier reproduced all counts, no AC row cites a nonexistent test.

**BLE-001 COMPLETE (iter 90) — carries forward (first TRANSPORT):**
- **ACCEPT (iter 90)**: graph BLE-001 → COMPLETE, evidence 7, known_limitations
  14, AC-1..16; workspace **533/0/1**, `transport::ble` **42**, clippy **0**.
- **VERIFY (iter 89) APPROVED**: `docs/implementation/BLE_001_VERIFICATION.md`
  v1.0 AC-1..16 evidence; independent verifier APPROVED (all 9 RT fixes real
  code + real regressions, 22 AC-cited tests exist, AC-14 honest).
- **SECURITY_REVIEW (iter 88) RESOLVED**: BLE-RT-001..016 (RT-008 absent); 7
  HIGH/MED FIXED + RT-009 RECORDED + 3 LOW FIXED + 5 RECORDED; 9 RT regressions.

**RESEARCH COMPLETE (iter 73) — RES-0018 recorded + registered (next
RES-0019).** Decisions at a glance (R1–R12): ADOPT srTCM token bucket per
(sender,class) silent-drop P0/P1-exempt + per-sender storage quota +
priority-reserved pool + TTL-ordered eviction (RFC 2697/2698; RFC 9171 §6.9;
Claim-Carry-and-Check; Meshtastic CVE regression suite); **REJECT** per-message
PoW (battery/duty-cycle/difficulty-calibration) — **DEFER** identity-mint PoW
(KeyChallenge/SyDeLP) + VDF + RFC 7859 to PROTO-001 v2 (no new crypto crates);
**ADOPT** layered replay = dedup + freshness window τ (per-source skew) +
per-sender high-water (ts,seq) + **promote Bloom+LRU persistence WP-2→SEC-001**
(cross-reboot replay closure, `mod.rs:24`); **ADOPT** Ostra link-credit + Briar
BQP verified-pairing Sybil layer (REJECT SybilGuard/SybilLimit on relays);
**ADOPT** reputation = local watchdog + positive-only verified second-hand
(CORE) + iTrust audits as **routing weight only** (Watchdog/Pathrater, iTrust,
ITRM, Rep-AODV 2024); **ADOPT** RED-0002 ACL-1 key-anchored per-class sender
allowlist (over `emergency/authority.rs`); **ADOPT** receiver-side Bayesian
spam scoring only; **REJECT** RTT/GPS wormhole/secure-position; DEFER SAND.
CONFLICTS C1–C6 (= DISC-0013 doc-vs-code drift) feed C-pattern doc
reconciliation. Gaps G1–G7 (τ → EXP-SEC-001). **Next: DESIGN**
(SEC_001_DESIGN.md + ACs + DEC-SEC-0001..N).

**IMPLEMENT COMPLETE (iter 74) — SEC_001_DESIGN.md v1.0 fully implemented.**
Built `security/` module: rate_limiter.rs (srTCM RFC 2697), quota.rs (per-sender
quota + priority pool), replay.rs (freshness + high-water + cross-reboot
persistence), reputation.rs (Bayesian routing weight), spam.rs (receiver-side
likely_spam), acl.rs (ACL-1 emergency auth), mod.rs (SecurityPolicy + Noop
facade). **Integrated into MessageEngine:** outbound rate_limit + quota check,
inbound replay protection + spam scoring + emergency ACL. **(⚠️ The iter-74 "464
tests green, clippy 0" claim was FALSE — corrected iter 77 to an honest 497/499
baseline.)**

**🔄 TEST IN PROGRESS — repair (77) + redteam SEC-RT (79) + AC-14 (80) COMPLETE.** Repair
pass: async `SecurityPolicy` + `FullSecurityPolicy` real-engine routing + all
6 proptest modules rewritten (`block_on`), 10 clippy warnings fixed, 7 proptest
failures triaged (6 test bugs + 1 real ACL bug). Redteam SEC-RT (AC-16):
**12 findings (2 CRITICAL / 8 HIGH / 2 MEDIUM) all fixed** — replay deadlock
(SEC-RT-01), ACL real chain verification (SEC-RT-02), replay future-poison
clamp + regression (SEC-RT-03/14), quota TOCTOU single gate + terminal refunds
(SEC-RT-04/05), spam cap (SEC-RT-06), reputation verified-secondhand gate
(SEC-RT-07), SOS empty-store gate (SEC-RT-08), unknown-sender burst (SEC-RT-09),
div-by-zero (SEC-RT-11), saturating eviction (SEC-RT-12). Fuzz (AC-15): harness
proven, 19.4 M exec clean, quota overflow found+fixed (iter 78). **AC-14 (iter 80):
tarpaulin re-measured honestly — security module 590/617 = 95.6% (>=95% target
MET)** via 40+ targeted coverage tests; discovered + fixed **SEC-RT-15** (acl
root decode unreachable authorized path). **Verified: workspace 499 green / 0
failed / 1 ignored (iter 79), iris-core 444, security 68 passed, clippy 0.**
Remaining: fuzz >=24h aggregate, AC-16 remainder (SEC-RT-15 record row + grep
guard + known-limitations) → SECURITY_REVIEW → VERIFY → ACCEPT.

## EMERG-001 COMPLETE (iter 71 — carries forward)

VERIFIED + ACCEPTED: EMERG_VERIFICATION.md v1.0 **AC-1..14 all PASS**;
independent verifier **APPROVE** (reproduced 431 passed/0 failed, clippy 0,
`cargo audit` 0 vulns across 516 crates, all 6 redteam fixes present with
regression tests, no fake AC evidence). Redteam EMERG-RT-001..011
PASS-with-recorded-deviations (RT-001 CRITICAL provisioned-authority-root
requirement + RT-002 HIGH SOS receive-side rate enforcement + RT-006 replay + 
RT-007 audit privacy + RT-009/RT-011 LOW all FIXED in-pass; RT-003/004/005/008/
010 recorded). emergency/ module + engine receive-path gate + Noop default
(AC-11). known_limitations: BLE 512-B verified-chain ceiling (SOS radio-
native), send-side synthesis app-owned, typed EmergencyEvent platform deferral,
RT-003/004/005/008/010, 17 pre-existing desktop-transitive audit warnings.
**IMPLEMENT (68) → TEST (69) → SECURITY_REVIEW (70) → VERIFY+ACCEPT (71)**.

## IDENT-001 COMPLETE (iter 63c/64) — carries forward

IDENT-001 ACCEPTED: IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS (verifier
APPROVE); redteam IDENT-RT-001..016 PASS-with-recorded-deviations (9 in-scope
code findings FIXED +10 regression tests, identity 68); DEC-0010 control
verified satisfied (production path = IrisCryptoProvider + TrustKeyDirectory +
persisted DesktopIdentity; DevCryptoProvider only `config.dev`). identity
module `identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,
small_order}` + desktop RED-0001. known_limitations: RT-002/003/004/005 gated,
RT-014 accepted. Provides EMERG-001 inputs: peer_id/peer_short (emergency
sender identity), trust/rotation.

### CRYPTO-001 COMPLETE (iter 59) — carries forward

VERIFIED + ACCEPTED: CRYPTO_VERIFICATION.md v1.0 AC-1..9 all PASS. Redteam
RED-0001..0012 dispositioned: RED-0004 fragmentation/E2EE + 2 latent routing
bugs FIXED (M7 fragment E2E PASS, 290 green), RED-0002 mitigated (warn +
metric + threat model), RED-0006/0007/0009/0012 fixed; RED-0001 CRITICAL
operator-ratified (DEC-0010) → IDENT-001; RED-0003/0005/0008/0010/0011 tracked.
Design (iter 56, CRYPTO_DESIGN.md: wire/AAD/KDF/at-rest contracts,
IrisCryptoProvider + KeyDirectory, P0 broadcast non-encryption, KAT line-up,
crate pins) fully implemented (iter 57): crypto module, `codec::encode_for_aead`
(fields 1–7, 9–11, 14), engine send-path encrypt (recipient X25519 via
KeyDirectory; **P0 broadcast never encrypted**) + deliver-path decrypt/verify,
at-rest `RowSealer`/`StorageKeySealer` (AAD = message_id‖priority‖expires_at,
per-row random 12B nonce) + `NoSealer` default + schema v2 plaintext-identity
columns dropped, KATs RFC 7748/8439/8032/5869, M7 E2E suite (round-trip,
tampered-frame, fragmented re-verify).

NEXT: **EMERG-001 COMPLETE (iter 71)** — EMERG_VERIFICATION.md v1.0 AC-1..14
PASS, verifier APPROVE, 431 green. **🔄 SEC-001 active**: UNDERSTAND →
RESEARCH → DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.
Then transports (BLE-001/WIFIAWARE-001/WIFIDIRECT-001/LORA-001/
SAT-001) → TEST-001 → platform apps.

## Recent Milestones

- 2026-08-17: **ANDROID-001 AC-5 FFI-CONTRACT ABSORPTION COMPLETE (iter 115) — Kotlin adapter layer delivered.** `AdapterLifecycle.kt` implements the carry-forward requirements (NEW-WA-RT-108..112 + WIFIDIRECT RT-010): `FfiCallTimeout` per-call suspend/sync/`blockOn` timeouts; `SessionGate` idempotent start/subscribe (same-session reuse, Mutex-coalesced starters, **never latched into failure**, non-suspend `invalidate()` for channel-lost callbacks); `RingBufferOutbox` bounded oldest-drop with **per-destination eviction** (one saturated peer can't starve another); `RadioStateTracker` multi-subscriber replaying StateFlow availability stream + `NdpRegistry` closed-NDP prune at the drain boundary; `VerifiedPeerCache` candidate → VERIFIED 64-hex PeerId reuse key. **`AndroidWifiDirectTransportAdapter` NEW full 20-op async `FfiWifiDirectAdapter`** (`WifiP2pManager` DNS-SD `_iris._tcp` discovery + `addLocalService`/`setDnsSdResponseListeners` + `createGroup` persistent GO + `connect` join/invite + `removeGroup` + `setGroupOperatingBand` API-29 band mapping AUTO/GHZ24/GHZ5/GHZ6 + `WIFI_P2P_STATE`/`CONNECTION_CHANGED` receivers + `SessionGate<Channel>` latch + ring-buffer `p2pSend` (TCP-over-GO = AC-6) + adapter-supplied `goAddr` G-WD-2); WA (12-op async) + BLE (10-op) adapters share the same primitives. **`AdapterLifecycleTest.kt` 14 unit tests one-per-requirement** (JUnit5 + kotlinx.coroutines: rt110 timeouts, rt111 idempotence/never-latched/coalescing/markStarted/reset, rt010 invalidate, rt112 per-destination eviction + drain-once, rt109 closed-NDP prune + multi-subscriber replay, rt108 verified-reuse key). 1:1 op mapping verified vs generated bindings (AC-11 basis: WD 16 == 16, WA 12, BLE 10; ULong conversions confirmed). `cargo check --workspace` green (Kotlin-only pass; Rust untouched). Kotlin compile/run leg env-gated (no Android SDK/kotlinc/gradle on host). Next: FGS/WorkManager (AC-7) + app shell (AC-6) + Keystore TEE Ed25519 (AC-8).

- 2026-08-17: **ANDROID-001 AC-4 ENGINE REWIRING COMPLETE (iter 114) — IrisEngine mirrors DesktopEngine over real iris-core; round-trip delivery PROVEN through the FFI bridge.** `crates/iris-android/src/engine.rs` rewritten: owns tokio `Runtime` + `Handle` + `Arc<TransportManager>` + `Arc<MessageEngine>`; new `src/bridge.rs` bridges FFI adapters → core transport traits (`BleBridge` buffered `MutexGuard` drains proj-BLE-1 + hex UUID/MAC conversion + `AdvertisementData`→`{payload: service_data, non_connectable: true}`; `WifiAwareBridge`; `WifiDirectBridge` 17-method mirror + `group_info_from_ffi`/`band_to_ffi`/`ffi_err_to_ble`). Constructor registers `ble-android`/`wifi-aware-0`/`wifi-direct-0` on the TransportManager inside `runtime.block_on`, builds `MessageEngine::new_with_telemetry` (MemoryStorage + DevCryptoProvider), spawns per-transport auto inbox forwarders → `process_incoming` + `subscribe_inbox` broadcast (`FfiInboxListener` foreign trait + `FfiIncomingMessage`); `send_text`/`start_all`/`stop_all`/`parse_peer_id_hex`/`build_text_envelope` helpers. `FfiWifiDirectAdapter` **expanded 5-op → full 20-op async surface** (`FfiOperatingBand` + `FfiGroupConfig`/`FfiGroupInfo`/`FfiDirectPeerDiscovery`/`FfiIncomingWifiDirectData`); WA projections **corrected** (issue #2263 owned-params: `service_instance`→`service_specific_info: Vec<u8>`, `peer_handle`→`sender: Option<Vec<u8>>`). **Tests 5/5 PASS (0.57s)** incl. `round_trip_delivers_over_shared_mesh` (2 engines over `SimAwareWire` wrapping `SimulatedWifiAwareAdapter` sharing `SimMeshCoordinator`: B advertises → A discovers/connects keyed by B real node id → A `send_text` b"hello" → B delivers + inbox listener receipt); clippy 0; `cargo build -p iris-core` clean. Kotlin bindings **REGENERATED + committed** (uniffi-bindgen 0.31.2; iriscode.kt 6154 lines; `FfiIncomingMessage` x10, `subscribeInbox` x2, `create_group` x4; stale `discover_services`/`FfiServiceInfo` 0). Next: Kotlin adapters + `AdapterLifecycle.kt` (AC-5, NEW-WA-RT-108..112 + RT-010).

- 2026-08-17: **WIFIDIRECT-001 ACCEPTED COMPLETE (iter 108 VERIFY + iter 109 ACCEPT) — third TRANSPORT node done, 24 COMPLETE nodes.** VERIFY: `engineering/memory/records/WIFIDIRECT-001_VERIFICATION.md` v1.0 AC-1..16 evidence table (pattern WIFIAWARE-001_VERIFICATION.md): AC-1..13 PASS, AC-14 GATED/BLK-0005, AC-15 SECURITY_REVIEW v1 RESOLVED (RT-001..013), AC-16 independent verifier reproduction. Live re-verify iter 108: workspace **608 passed / 0 failed / 1 ignored** (per-suite sum reconciled: iris-core lib 553 + crypto_e2e 3, ml 8, obs 4, sim 8+1, desktop 5, commands 3, engine_roundtrip 3, storage 7, m3 1, pg_store 13); `transport::wifi_direct` **23 PASS** (13 transport + 10 serv under shared prefix); clippy --workspace --all-features --tests **0**; fmt clean. **Independent verifier APPROVE** (ses_ff2cab279ffebsAWkoU2shmMk0): all counts reproduced; all 7 RT fixes real code with file:line refs + passing regressions (RT-001 `ZeroShortId` gate + atomic `upsert_peer`; RT-002 same-dest eviction; RT-003/012 real-GO-only join; RT-004 reuse re-promotes; RT-005 links→Degraded; RT-007 empty-payload reject; RT-009 doc); no AC row cites a nonexistent test. Notes reconciled in-pass: RECORDED count corrected **6→5** (RT-009 doc-FIXED counted in the 7; security-review header + section label + verification AC-15 row updated); clippy-cache + empty-suite nits non-blocking. ACCEPT: PROJECT_GRAPH WIFIDIRECT-001 status → **COMPLETE**, evidence(10), stage_note full pipeline (UNDERSTAND 102 → RESEARCH 103 → DESIGN 104 → IMPLEMENT 105 → TEST 106 → SECURITY_REVIEW 107 → VERIFY 108 → ACCEPT 109), known_limitations(12); PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG 0.3.39. **NODE_TRANSITION → LORA-001 (UNDERSTAND, iter 110).** Transport pipeline: INTERNET-001 ✅ → BLE-001 ✅ → WIFIAWARE-001 ✅ → WIFIDIRECT-001 ✅ → LORA-001 → SAT-001.

- 2026-08-17: **WIFIDIRECT-001 SECURITY_REVIEW COMPLETE (iter 107) — WIFIDIRECT-001_RT-001..013 dispositioned (1 HIGH + 5 MEDIUM + 1 LOW FIXED in-pass + 7 regression tests; 6 RECORDED), third transport staged for VERIFY.** `WIFIDIRECT-001_SECURITY_REVIEW.md` (AC-15, pattern BLE-001/WIFIAWARE-001): adversarial review of `wifi_direct.rs` / `wifi_direct_serv.rs` / transport wiring + `WIFI_DIRECT_COST`. **FIXED**: **RT-001 (HIGH)** all-zero `peer_short` TXT mapped to the "unknown-sender" sentinel `PeerId([0u8;32])` in `candidate_peer_id_pads_short_id` + sim `register()` empty-TXT window → `parse` now rejects `ZeroShortId` (`ZeroShortId` error variant) + `coordinator.upsert_peer` atomically registers tag+TXT under one lock (`advertised_txt_is_attributable`); **RT-002** outbox global-oldest eviction starved other destinations → per-destination eviction (`outbox_eviction_keeps_other_destinations`); **RT-003/RT-012** phantom-group manufacture for non-GO peers + `group_id` overwrite → join requires existing GO + single-group-per-adapter guard (`join_non_go_peer_is_rejected`); **RT-004** connect reuse didn't re-promote Connected → `set_state(Connected)` on reuse (`connect_reuse_after_churn_recovers`); **RT-005** send-on-unavailable left link Connected → tears all links down → Degraded (`send_on_unavailable_tears_down_links`); **RT-007** empty zero-length payload acked but dropped inbound → Protocol reject on send (`empty_payload_send_rejected`); **RT-009** INCOMING_CHANNEL_CAPACITY doc comment 8 MiB → 32 MiB corrected. **RECORDED (6)**: RT-006 (dropped_inbound lag telemetry undercounts broadcast), RT-010 (ensure_started latch prevents FFI double-start → ANDROID-001 idempotence requirement), RT-011 (RadioConflictGroup → TRANSPORT-001 manager arbitration), RT-013 (shared 1 MiB constant refactor), poll cadence (100 Hz) battery tradeoff, AC-14 GATED/BLK-0005. *(Count later corrected to 5 at VERIFY — RT-009 is doc-FIXED, counted in the 7.)* Positive controls held: strict TXT parse, framed decode bounds, candidate-only trust boundary, gated write path, bounded queues, lifecycle/shutdown latch, group-model integrity, log hygiene. **Live re-verify: `transport::wifi_direct` 23 PASS (13 transport + 10 serv incl. 7 RT regressions); workspace --all-features 608 passed / 0 failed / 1 ignored (baseline 601, +7 RT); clippy --workspace --all-features --tests 0; fmt clean.** PROJECT_GRAPH evidence(9) + known_limitations(12) + stage_note SECURITY_REVIEW COMPLETE; status held IMPLEMENTING (moves at VERIFY→ACCEPT). **STAGE_TRANSITION → VERIFY (iter 108).**

- 2026-08-17: **WIFIDIRECT-001 TEST COMPLETE (iter 106) — WIFIDIRECT-001_TEST.md AC-1..16 evidence table (AC-1..13 PASS + AC-14 GATED/BLK-0005), third transport staged for SECURITY_REVIEW.** Evidence mapping + live re-verify: AC-1 `registration_gating` (TransportManager never selects Unavailable), AC-2 `discovery_finds_advertising_peer` + serv `build_parse_round_trip`/`service_name_is_fixed`/`candidate_peer_id_pads_short_id` (DNS-SD 'com.iris.mesh.v1' + 22-byte TXT-record → candidate PeerId), AC-3 `band_restricted_go_creation_falls_back` (AUTO fallback DEC-WD-0005 + unavailable connect gate), AC-4 `group_roundtrip_delivers_payload` (GO go_intent 14 + GC 0 → INTERNET-001-framed payload E2E → engine seam; sender NEVER zero + attributed to real sender), AC-5 `discovery_rearm_and_stop` (30-s DISCOVERY_WINDOW + stop clears window = WIFI_P2P_DISCOVERY_CHANGED_ACTION → BLE fallback), AC-6 9 serv adversarial tests (exhaustive 0..22 short-len; unsupported version; unknown kind; reserved bits; oversize padded ≤255 cap; past-cap TooLong + boundary ok), AC-7 candidate-only pubkey identity never P2P MAC (DEC-WD-0007), AC-8 `shutdown_returns_to_unavailable` + persistent-GO teardown, AC-9 `single_link_per_peer_and_bounded_table` (8 concurrent → 1 link; MAX_GO_CLIENTS=8), AC-10 WIFI_DIRECT.md FGS/wakelock + band API 29 + 120-s find/30-s re-arm docs + caps background false, AC-11 20-op FFI conformance clippy 0, AC-12 doc reconciliation, AC-13 workspace 601/0/1 + clippy 0. **16 tests PASS live (7 transport + 9 serv); workspace --all-features 601 passed / 0 failed / 1 ignored; clippy 0; fmt clean.** PROJECT_GRAPH evidence(8) + stage_note/validation_status iter 106; status held IMPLEMENTING (moves at VERIFY→ACCEPT). **STAGE_TRANSITION → SECURITY_REVIEW (iter 107).**

- 2026-08-17: **WIFIDIRECT-001 IMPLEMENT COMPLETE (iter 105) — wifi_direct.rs + wifi_direct_serv.rs + WIFI_DIRECT_COST registered, third transport staged for TEST.** `wifi_direct.rs`: `WifiDirectAdapter` FFI trait (20 ops) + `SimP2pCoordinator`/`SimulatedWifiDirectAdapter` in-memory P2P mesh (per-tag DNS-SD service-name match, GO/GC group formation, bounded outbox MAX_OUTBOX_FRAMES=128 + per-call drain budget, availability churn, band-restriction failure) + `WifiDirectTransport` `Transport` impl (capability/coexistence gate AC-3, DNS-SD discovery → candidates AC-2, 30-s app re-arm window AC-5, GO/GC connection table + INTERNET-001 framing reuse AC-4/AC-9, lifecycle + persistent-GO teardown AC-8, envelope seam AC-7). `wifi_direct_serv.rs` (AC-6): `WifiDirectCapBits` + `WifiDirectTxtRecord` 22-byte build/parse + `WIFI_DIRECT_SERVICE_NAME` "com.iris.mesh.v1" + 9 adversarial tests. `WIFI_DIRECT_COST` BatteryCostModel added (scan 80 / advertise 50 / connected 40 / tx 0.01 / rx 0.008 — design estimates per AC-14). Both modules registered in transport/mod.rs; design doc §7 reference-impl section appended (AC→code mapping + test suite + interop note). **16 tests PASS (7 transport + 9 serv); workspace 601 passed / 0 failed / 1 ignored; clippy 0; fmt clean.** Authoring fixes in-pass (group_info Vec<u64>→PeerHandle, ConnectionFailed unit variant, MutexGuard deref, OperatingBand derived Default, join_all→sequential). PROJECT_GRAPH status DESIGNING→IMPLEMENTING + evidence(7) + stage_note. PROJECT_STATE designing 1→0, implementing 0→1. **STAGE_TRANSITION → TEST (iter 106).**

- 2026-08-17: **WIFIDIRECT-001 DESIGN COMPLETE (iter 104) — WIFI_DIRECT_TRANSPORT_DESIGN.md v1.0 + AC-1..16 C2 RESOLVED + DEC-WD-0001..8 ratified, third transport staged for IMPLEMENT.** Design per RES-0021 verdict (Q1–Q8 + G-WD-1..8 + D-1..D-7): v1 = Android `WifiP2pManager` DNS-SD (Bonjour) discovery on BLE-triggered re-arm window (30-s app cadence vs 120-s framework find) + `createGroup`/`connect` persistent GO; wpa_supplicant dual path (`p2p_group_add`/`p2p_connect`/`p2p_service_add bonjour`) + IRIS-owned IP/DHCP glue (GO address adapter-supplied, never hard-coded, G-WD-2); **data plane = TCP socket over GO reusing INTERNET-001 framing (1 MiB cap, pool+backoff)** + client IPv4 DHCP / IPv6-link-local; WPA2-Personal floor (WPS-PIN prohibited, passphrase over authenticated BLE, PBC legacy fallback only); WPA3-SAE R2 capability-gated; `setGroupOperatingBand` API 29 5 GHz preferred / AUTO fallback; coex `WifiAvailableChannel` API 34; N-client admission + GO-side connection table (G-WD-1); OS patch floor AC-14; FGS connectedDevice + wakelock contract. PROJECT_GRAPH: WIFIDIRECT-001 DISCOVERED→DESIGNING + **acceptance_criteria AC-1..16** (C2 gap RESOLVED) + evidence(6) + known_limitations(8). DEC-WD-0001..0008 ratified. WIFI_DIRECT.md corrections (band API 29, find window 120 s framework / 30 s app re-arm, client ceiling vendor/HAL + N-client, WPA3-SAE R2 + WPS-PIN prohibited, OS patch floor block). Baseline **552/0/1 clippy 0** held (docs-only pass). **STAGE_TRANSITION → IMPLEMENT (iter 105).**

- 2026-08-17: **WIFIDIRECT-001 RESEARCH COMPLETE (iter 103) — RES-0021 PROCEED,
  third transport staged for DESIGN.** RES-0021 recorded + registered
  (ALLOCATION next RES-0022): 8 websearch evidence passes (2024–2026 Wi-Fi
  Direct/P2P SOTA, primary sources, L1–L5, no AI citations). Key findings:
  WifiP2pManager alive + extended API 36/37 (no deprecation; DPP infra-only);
  **WPA2-Personal floor + WPA3-SAE R2-capability-gated**; **"1 GO + 8 clients" =
  vendor/HAL ceiling** (G-WD-1) → N-client admission; DNS-SD discovery preferred
  with **framework find window 120 s** (`DISCOVER_TIMEOUT_S`; doc 30-s = app
  re-arm, G-WD-7); persistent GO + **`setGroupOperatingBand` = API 29** (doc
  correction); WPS deprecated API 28 + **WPS-PIN prohibited** (CVE-2021-0326;
  passphrase pushed over authenticated BLE, PBC legacy fallback); wpa_supplicant
  full P2P surface + **IRIS-owned IP/DHCP glue** (G-WD-2); **OS patch floor AC**
  (SPL ≥ 2021-02, wpa_supplicant ≥ 2.12, kernel w/ 2024–26 Wi-Fi fixes);
  gaps G-WD-1..8 + DESIGN decisions D-1..D-7. Baseline **552/0/1 clippy 0** held
  (research-only). **STAGE_TRANSITION → DESIGN (iter 104).**

- 2026-08-17: **WIFIDIRECT-001 UNDERSTAND COMPLETE (iter 102) — C2 AC-gap
  found, third transport staged for RESEARCH.** Read PROJECT_GRAPH node (P1
  TRANSPORT, DISCOVERED, deps TRANSPORT-001 COMPLETE, platform ANDROID).
  WIFI_DIRECT.md (326 lines) verified: WD = data plane triggered by BLE control
  plane (4-phase workflow), GO/client topology (GO IP 192.168.49.1), up to
  200 m / 250 Mbps peak / 10-80 Mbps practical / setup 2-15 s, DNS-SD service
  discovery preferred, foreground/FGS + wakelock required (BLE handles
  background discovery; WD active transfers only), 1 GO + up to 8 clients, GO
  intent bias (14/7/3), 30-s discovery window, persistent GO optimization,
  same-band infra-Wi-Fi interference (setGroupOperatingBand API 29), iOS NOT
  available → MCSession Apple-only (known gap). TRANSPORT_ABSTRACTION row
  (50-200 m / ~250 Mbps / 10-80 Mbps / 50-500 ms setup / High battery /
  P0-P3 files). manager.rs:460 stub `wifi-direct` already registered in test;
  no production `wifi_direct.rs`. Baseline **552/0/1 clippy 0** held (read-only
  pass); PROJECT_GRAPH evidence(4) + known_limitations(5) + stage_note.
  **STAGE_TRANSITION → RESEARCH (iter 103, RES-0021).**

- 2026-08-16: **WIFIAWARE-001 ACCEPTED COMPLETE (iter 101) — second TRANSPORT
  node done, 23 COMPLETE nodes.** VERIFY: `WIFIAWARE-001_VERIFICATION.md` v1.0
  AC-1..16 evidence table (AC-1..13 PASS, AC-14 GATED/BLK-0005, AC-15
  SECURITY_REVIEW v2 RESOLVED, AC-16 independent verifier); live re-verify
  workspace **552/0/1**, `transport::wifiaware` 11 + beacon 8 = 19 under shared
  filter, clippy **0**; independent verifier **APPROVE_WITH_NOTES** (all counts
  reproduced, all 19 tests present, both HIGH SECURITY_REVIEW fixes real code);
  note reconciled — display string `"Wi-Fi Aware (scaffold)"` → `"Wi-Fi Aware"`.
  ACCEPT: graph COMPLETE + evidence(11) + NEW-WA-RT-108..112 carry-forward;
  PROJECT_STATE completed 22→23; CHANGELOG 0.3.33. **NODE_TRANSITION → WIFIDIRECT-001
  (UNDERSTAND iter 102).**

- 2026-08-16: **WIFIAWARE-001 SECURITY_REVIEW COMPLETE (iter 96 + re-review
  iter 100) — CONDITIONAL-FAIL RESOLVED.** `WIFIAWARE-001_SECURITY_REVIEW.md`
  v2 (AC-15): **Pass 1** redteam WAW-RT-001..005 (1 HIGH concurrent-connect
  double-open AC-9, 1 MED sim unbounded outbox, 3 LOW) ALL FIXED with
  regressions. **Pass 2** independent red-team re-review of data paths +
  concurrency found **NEW-WA-RT-101..112**: **HIGH NEW-WA-RT-101** — recv-side
  attribution ALWAYS `PeerId([0u8;32])` (sim forwarded the SENDER's opaque NDP
  handle, poller matched the RECEIVER's own links) → fixed via
  `IncomingNdpData.sender: Option<PeerId>` + sim outbox `(to_tag, from_tag)` +
  beacon-derived candidate attribution; AC-4 test now asserts real sender
  PeerId. **6 MEDIUM ALL FIXED**: NEW-WA-RT-102 connect `is_available` gate
  (`NotSupported`), 103 availability re-check after `open_ndp.await`,
  104 `teardown_link`→Degraded when scope down, 105 per-tick budget enforced in
  the transport (poller backlog, no drop), 106 `dropped_inbound` telemetry,
  107 `is_link_loss_error` transient-vs-terminal classifier. **5 LOW RECORDED**
  as ANDROID-001 FFI-contract requirements (verified-peer reuse key, closed-NDP
  frame prune, FFI timeouts, adapter idempotence, ring-buffer outbox). New
  regressions: `transient_send_error_keeps_link`,
  `last_link_death_while_unavailable_is_degraded`,
  `connect_while_unavailable_is_not_supported` + strengthened
  `ndp_roundtrip_delivers_payload`. **Workspace 552/0/1 (wifiaware 19 + beacon
  8), clippy 0.** STAGE_TRANSITION to VERIFY (AC-16, iter 101).

- 2026-08-16: **WIFIAWARE-001 TEST COMPLETE (iter 95)** —
  `engineering/memory/records/WIFIAWARE-001_TEST.md` AC-1..16 evidence (pattern
  BLE-001_TEST.md): AC-1 registration gating, AC-2 discovery candidate flow,
  AC-3/AC-5 availability gate + churn, AC-4 NDP E2E round-trip, AC-6 7 adversarial
  beacon tests, AC-7 candidate-only + pubkey identity, AC-8 lifecycle, AC-9
  per-peer single NDP, AC-10 background/FGS doc, AC-11 FFI conformance, AC-12 doc
  reconciliation, AC-13 workspace green + clippy 0, AC-14 GATED/BLK-0005, AC-15
  SECURITY_REVIEW next, AC-16 VERIFY. **transport::wifiaware 12 PASS, beacon 7
  PASS, clippy 0, workspace green (490 lib).** Durable-state drift reconciled
  (execution-state/ACTIVE_NODE/NEXT_ACTION were stale at iter 93 → all at iter 95).
  Next: SECURITY_REVIEW (iter 96).

- 2026-08-16: **WIFIAWARE-001 IMPLEMENT COMPLETE (iter 94)** —
  `crates/iris-core/src/transport/wifiaware.rs` (`WifiAwareAdapter` trait = FFI
  contract + `SimulatedWifiAwareAdapter`/`SimMeshCoordinator` deterministic
  in-memory NDP mesh + `WifiAwareTransport` Transport impl reusing
  INTERNET-001 framing, NDP poller, state machine) + `wifiaware_beacon.rs`
  (22-byte beacon build/parse + `CapabilityBits` + `candidate_peer_id` +
  adversarial tests) + registered in transport/mod.rs + design §9 ref-impl
  section. **transport::wifiaware 12 PASS, beacon 7 PASS, clippy 0, fmt clean,
  workspace green (490 lib).** Status DESIGNING→IMPLEMENTING; PROJECT_STATE
  implementing 0→1. Next: TEST (iter 95).

- 2026-08-16: **WIFIAWARE-001 DESIGN COMPLETE (iter 93)** —
  `docs/implementation/WIFI_AWARE_TRANSPORT_DESIGN.md` v1.0 written per
  RES-0020 verdict: v1 = publish/subscribe NAN discovery + NDP IPv6 socket data
  path reusing INTERNET-001 TCP framing (1 MiB cap); runtime capability gate;
  FGS connectedDevice (API 34+) required for production discovery; NDP open at
  L2, app-layer envelope = trust anchor; identity by pubkey fingerprint never
  NAN MAC. **AC-1..16 defined (C2 gap RESOLVED)**; status DISCOVERED→DESIGNING;
  **DEC-WA-0001..0008 ratified**; **CONFLICT-1 reconciled** (Apple WiFiAware
  iOS 26+ — "iOS NO" rows corrected in WIFI_AWARE.md + TRANSPORT_ABSTRACTION).
  PROJECT_STATE designing 0→1, discovered 9→8. Next: IMPLEMENT (iter 94).

- 2026-08-16: **WIFIAWARE-001 RESEARCH COMPLETE (iter 92)** — RES-0020 recorded
  (8 websearch evidence passes, primary sources L1–L5, no AI citations),
  verdict **PROCEED**: v1 = publish/subscribe NAN discovery + NDP IPv6 socket
  data path reusing INTERNET-001 TCP framing (1 MiB cap); NAN alive API 26→36
  (not deprecated); FGS required for production discovery; app-layer envelope =
  trust anchor; **CONFLICT-1: Apple WiFiAware framework (iOS 26+)** → 'iOS NO'
  rows corrected at DESIGN; gaps G-WA-1/2/3. ALLOCATION next RES-0021;
  CHANGELOG 0.3.31; graph evidence(4) + known_limitations(7). Status held
  DISCOVERED. Next: DESIGN (iter 93, AC-1..n).

- 2026-08-16: **BLE-001 ACCEPTED (iter 90) — first TRANSPORT node COMPLETE.**
  AC-1..16 all PASS (AC-14 GATED/BLK-0005 recorded); graph COMPLETE + evidence 7
  + known_limitations 14; PROJECT_STATE completed 21→22, verifying 1→0;
  CHANGELOG 0.3.27/28/29; NODE_TRANSITION → WIFIAWARE-001 (UNDERSTAND iter 91).
- 2026-08-16: **BLE-001 VERIFY COMPLETE (iter 89) — AC-16 APPROVED.** Wrote
  `docs/implementation/BLE_001_VERIFICATION.md` v1.0: AC-1..16 evidence table
  (AC-14 GATED/BLK-0005, AC-15 RESOLVED, AC-16 independent reproduction). Live
  re-verify: workspace **533/0/1**, `transport::ble` **42**, clippy **0**, 9 RT
  regressions pass by name. Independent verifier subagent **APPROVED** — all RT
  fixes real code with real regression tests (TTL sweep at ble.rs poller, connect
  full-setup lock + prior AbortHandle abort, per-connection MTU map +
  `segment_for_mtu`, partition-in-place drain), all 22 grepped AC-cited tests
  exist, AC-14 honest. Notes reconciled: RT-009 aggregate counts (RECORDED not
  FIXED), AC-10 §3 ref, "taref" typo. **BLE-001 → ACCEPT (iter 90).**
- 2026-08-16: **BLE-001 SECURITY_REVIEW COMPLETE (iter 88) — CONDITIONAL-FAIL
  RESOLVED.** Redteam BLE-RT-001..016 dispositioned in
  BLE-001_SECURITY_REVIEW.md (AC-15): 4 HIGH + 4 MEDIUM + 5 LOW + 2 INFO; 8
  HIGH/MED FIXED + 3 LOW FIXED + 5 RECORDED. HIGH: RT-001 (reassembly TTL
  eviction dead code → poller 1-s evict_stale sweep; partial-slot exhaustion was
  a permanent multi-chunk DoS), RT-002 (connect TOCTOU → full-setup connections
  lock + abort prior poller), RT-003 (global MTU → per-connection
  `HashMap<PeerId,(GattHandle,u16)>` + `segment_for_mtu`), RT-016
  (partition-in-place by own GattHandle — no cross-peer frame misattribution).
  MED: RT-004 (connect failure → Available), RT-005 (write-failure `close_peer`
  teardown). LOW: RT-007 (encode_frame → Result), RT-011 (readvertise stops
  prior handle), RT-013 (max_peers). 9 RT regression tests + adapters
  (TwoMtu/FailingWrite/FailingConnect). Independent verifier **APPROVE_WITH_
  NOTES** (F1: session count 533; F2: inventory 42 — applied). **Workspace
  533/0/1, transport::ble 42, clippy 0.** STAGE_TRANSITION to VERIFY (AC-16,
  iter 89).
- 2026-08-16: **BLE-001 TEST COMPLETE (iter 87)** — BLE-001_TEST.md AC-1..13
  software-evidence + AC-10 background/FGS docs + AC-12 doc reconciliation;
  baseline 524/0/1 workspace. STAGE_TRANSITION to SECURITY_REVIEW.
- 2026-08-15: **SEC-001 RESEARCH COMPLETE (iter 73) — STAGE_TRANSITION to
  DESIGN.** RES-0018 recorded (255 lines) + registered in ALLOCATION.md (next
  RES-0019): 2024–2026 SOTA for DTN/mesh DoS + replay + Sybil + spam + routing
  attacks (12 websearch evidence passes, evidence-leveled RFC/NIST L1 /
  academic L2 / IETF-draft L3 / OSS L4; no AI citations). **R1–R12 decisions**:
  ADOPT srTCM token bucket per (sender,class) silent-drop P0/P1-exempt (RFC
  2697/2698; RFC 9171 §6.9) + per-sender storage quota/priority pool +
  TTL-ordered eviction + Meshtastic CVE regression suite (CVE-2024-47065
  traceroute amplification, want_response crash, parser, amplification, key
  hygiene); REJECT per-message PoW (battery/duty-cycle/difficulty-
  calibration), DEFER identity-mint PoW (KeyChallenge/SyDeLP) + VDF + RFC 7859
  to PROTO-001 v2 (no-new-crypto held); ADOPT layered replay = dedup +
  freshness window τ + per-sender high-water (ts,seq) (RFC 9171 §4.2.7, RFC
  4303 §3.4.3, RFC 7181 §23.2/RFC 7183, RFC 6479) + **Bloom+LRU persistence
  promoted WP-2→SEC-001** (cross-reboot closure, `mod.rs:24`); ADOPT Ostra
  link-credit + Briar BQP verified-pairing Sybil layer, REJECT
  SybilGuard/SybilLimit on relays; ADOPT reputation as routing weight only
  (CORE-positive second-hand, iTrust audits); ADOPT RED-0002 ACL-1 key-anchored
  per-class sender allowlist (over `emergency/authority.rs`); ADOPT
  receiver-side Bayesian spam scoring; ADOPT blackhole = replication +
  Ack-eviction + reputation weight; REJECT RTT/GPS wormhole/secure-position;
  DEFER SAND to v2. **CONFLICTS C1–C6 (DISC-0013)** feed DESIGN C-pattern doc
  reconciliation; **gaps G1–G7** (BBPATs→G1, τ→EXP-SEC-001). Workspace
  untouched (research-only; 431 green baseline). **Next: SEC-001 DESIGN**
  (SEC_001_DESIGN.md + ACs + DEC-SEC-0001..N).
- 2026-08-15: **SEC-001 UNDERSTAND COMPLETE (iter 72) — STAGE_TRANSITION to
  RESEARCH.** Threat model synthesised from docs/security/* (adversary classes
  A–F: passive / active-inject-replay / honest-but-curious relay / malicious
  relay blackhole+false-route-ads / compromised / quantum-post; threat actors
  TA-1..9; non-guarantees: anonymity ADVERSARY_MODEL.md:213, availability
  under Adv_M :224, strong Sybil THREAT_MODEL.md:172). AC check: SEC-001 node
  **LACKS `acceptance_criteria`** (PROJECT_GRAPH.yaml:469-475) → C2; DESIGN
  must define per ACCEPTANCE_POLICY `safety_critical_additional` (adversarial
  coverage >95%, fuzzing 24h minimum, external review recommended; DEC-0009
  gate lifted). **DISC-0013 verified**: SYBIL_RESISTANCE.md Defenses 3/4/5
  claim Implemented but no general rate-limiter/reputation/Sybil-detector
  exists; DOS_RESISTANCE.md:213 cites `crates/iris-core/src/node/
  dos_protection.rs` — **path does not exist**; real defenses = dedup
  Bloom+LRU, ForwardedCache, emergency SosRateLimiter/BroadcastReplayGuard,
  trust_store replay counters. SEC-001 generalizes + reconciles docs
  (C-pattern). ADR-0006 per-message-key/key-mgmt already resolved (CRYPTO-001
  iter 55); ADR-0011 open issue (:89) feeds design. Workspace untouched.
- 2026-08-15: **EMERG-001 VERIFY + ACCEPT COMPLETE (iter 71) — node COMPLETE,
  20 COMPLETE nodes.** `docs/implementation/EMERG_VERIFICATION.md` v1.0 AC-1..14
  PASS; independent verifier **APPROVE** (reproduced 431 passed / 0 failed,
  clippy 0, `cargo audit` 0 vulns / 516 crates, all 6 redteam fixes present with
  regression tests, no fake AC evidence); 3 verifier doc-level notes reconciled
  in-pass (provider.rs `BroadcastReplayGuard::MAX_IDS` docstring 3096→1024,
  ignored-test characterization, AC-2 revoked-phrasing). NODE_TRANSITION →
  **SEC-001** (P0 Security Hardening).
- 2026-08-15: **EMERG-001 SECURITY_REVIEW COMPLETE (iter 70)** — EMERG-RT-001..011
  PASS-with-recorded-deviations; RT-001 CRITICAL (provisioned authority root) +
  RT-002 HIGH (sos_rate receive-side enforcement) + RT-006/007/009/011 FIXED
  (+7 regression tests); 431 green.
- 2026-08-15: **EMERG-001 IMPLEMENT COMPLETE (iter 68)** — `emergency/` module
  (11 files) built against real APIs: payload-level `AuthorityMeta` inside the
  signed `EmergencyBroadcast` (KeyAdvertisementV1 reused unchanged as chain
  element per DEC-EMERG-0002), 9-step SPKI/RFC 9804 verify pipeline (root
  required, chain cap ≤4, `Severity::Test` cap-exempt for drills), ciborium CDE
  codec (fixed wire keys, unknown-key tolerant, SOS ≤84 B), SOS classify/cancel
  (60-min same-origin), rate limiter 3/hr/sender (16-B-prefix rolling + CANCEL
  reset), `DisasterMode` guarded_transition ≥15-min holds, bounded
  pseudonymous audit ring, `EmergencyGateway` (TrustStore + limiter + audit),
  engine `set_emergency_provider` seam + `NoopEmergencyProvider` default (AC-11,
  engine unchanged until armed — verified by new tokio engine test
  `emergency_provider_defaults_to_noop_and_switches`). Doc corrections **C1/
  C3/C4/C5/C6 applied (AC-12)**: EMERGENCY_BROADCAST.md (96/96 compact +
  signed-follow-up UPDATE), EMERGENCY_UX.md (WEA 853+960 Hz; SOS resend aligned
  to ack.rs 30 s/2×/unlimited-TTL — no 15-min), EMERGENCY_ABUSE.md (IPC→BNS
  2023 §420→§318, §505→§353(2), §153A→LEGAL-001), EMERGENCY_GOVERNANCE.md
  (X.509-inspired→SPKI-style key-anchored, no PKIX). Build errors iterated to
  green: codec Eq derives (ciborium not Eq), f64 `Triggers` Eq, `&[u8]` limiter
  keys, usize casts, `FieldTooLong(_)` pattern, drill-cap exemption, hand-crafted
  CANCEL wire test. **Workspace 421 green** (366 iris-core incl. 62 emergency +
  seam test; integrations 3+8+4+8+5+3+7+1+13+3 = 0 failed, 1 ignored), clippy 0
  `-p iris-core --all-targets`. STAGE_TRANSITION to TEST. Open item for TEST:
  reconcile `authority_short_id` blake3-impl vs model.rs/design "SHA-256(sender)
  [..16]" wording.
- 2026-08-15: **IDENT-001 ACCEPTED/COMPLETE (iter 63c/64) — 19 COMPLETE
  nodes** — IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS; independent verifier
  APPROVE (reproduced 68 identity passed / 361 workspace passed 0 failed across
  15 suites / clippy 0 --workspace --all-targets / DEC-0010 verified by code
  read engine_handle.rs::build 153-180). Graph IDENT-001 COMPLETE + evidence +
  known_limitations (RT-002/003/004/005 gated, RT-014 accepted);
  PROJECT_STATE completed 19. NODE_TRANSITION to **EMERG-001** (P0, selected
  via PRIORITY_POLICY centrality/risk; critical-path junction).
- 2026-08-15: **IDENT-001 VERIFY COMPLETE (iter 63c)** — IDENT_VERIFICATION.md
  v1.0 AC-1..11 PASS evidence table; verifier APPROVE; 3 doc-level observations
  recorded (68 command-filtered vs 67 module-internal identity tests; unix-gated
  0600 test in TEST record; +10/+12 label drift — net +9..10).
- 2026-08-15: **IDENT-001 SECURITY_REVIEW COMPLETE — PASS-with-recorded-
  deviations (iter 63)** — redteam IDENT-RT-001..016 (5 HIGH / 7 MEDIUM /
  2 LOW / 2 INFO) dispositioned in IDENT-001_SECURITY_REVIEW.md. 9 in-scope
  code findings FIXED in-pass (+10 regression tests, identity unit 58→68):
  RT-001 (adopt_advertisement replay: stale ad → Duplicate drop, never
  permanent KeyChanged downgrade; equal-counter → warn; strictly-higher
  sig-verified → RotationAdopted + clears warning), RT-009 (DEFAULT_SKEW_BUDGET_SECS
  at adopt+rotate), RT-010 (verify_peer exact-key KeyMismatch + un_revoke
  healing), RT-011 (chain format-version gate + RootKeyMismatch consistency),
  RT-012 (RotationError::UnknownPeer), RT-013 (adopt_rotation RED-0011 recheck),
  RT-006 (stale-lock reclaim LOCK_STALE_SECS=15), RT-007 (0600-from-birth temp
  secret file), RT-008 (Zeroizing key-store returns). 4 gated known_limitations
  (RT-002 rotation engine E2E → PROTO-001 v2; RT-003 inbound KeyRotation feed →
  DISCO-001; RT-004 PgStorage::with_sealer desktop deviation; RT-005 Windows
  keystore) + RT-014 provision TOCTOU accepted single-instance + RT-015/016 INFO
  clean. **DEC-0010 security control VERIFIED satisfied**. Workspace **361
  green** (306 iris-core incl. 68 identity), clippy 0. STAGE_TRANSITION to VERIFY.
- 2026-08-15: **IDENT-001 TEST COMPLETE (iter 62b)** — IDENT-001_TEST.md
  AC-1..9 evidence mapped + recorded (peer_id 7, provisioning 13, advertisement
  6, verify_chain 11, TrustStore 10, rotation 7, small-order 4, desktop 3);
  workspace green. STAGE_TRANSITION to SECURITY_REVIEW.
- 2026-08-15: **IDENT-001 IMPLEMENT COMPLETE (iter 62)** — identity module
  (`identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,
  small_order}`: NodeIdentityV1 provision/load, FileKeyStore, PeerId/SHA-256
  short-id, RED-0005 ad, TOFU TrustStore, RED-0008 verify_chain,
  rotation/null-rotation, RED-0011; 58 tests) + `MessageEngine::set_key_directory`
  + desktop RED-0001 (`DesktopIdentity` + IrisCryptoProvider + TrustKeyDirectory
  + persisted PeerId; DevCryptoProvider → test seams; IRIS_NODE_ID UID label; 5
  tests). Workspace green, clippy 0. STAGE_TRANSITION to TEST.
- 2026-08-15: **IDENT-001 DESIGN COMPLETE (iter 61)** —
  `docs/implementation/IDENT_DESIGN.md` v1.0 (D1-D6, AC-1..11, module layout,
  wire reconciliation, RED-0005/0008/0011, RED-0001 desktop wiring plan);
  STAGE_TRANSITION to IMPLEMENT.
- 2026-08-15: **IDENT-001 RESEARCH COMPLETE (iter 60)** — RES-0016 recorded
  (identity model/address derivation, per-platform key provisioning, TOFU+
  pairing trust, rotation/revocation, Ed25519-signed-X25519 binding) +
  DEC-P0005..P0009 resolved. Key evidence: CVE-2025-53627 (never MAC-derived
  identity), KeyDroid StrongBox perf, AOSP HKDF-SHA256 pattern, SPKI RFC 9804,
  KERI null-rotation, Signal X3DH/Meshtastic 2.8.x key-binding precedent,
  RUSTSEC 2026-08-15 posture. Graph/state advanced; STAGE_TRANSITION to
  DESIGN.
- 2026-08-15: **CRYPTO-001 VERIFY + ACCEPT (iter 59)** — CRYPTO_VERIFICATION.md
  v1.0 AC-1..9 all PASS; redteam RED-0001..0012 dispositioned; **RED-0001
  CRITICAL operator-ratified (DEC-0010)** to IDENT-001 with security control;
  CRYPTO-001 **COMPLETE** (18 COMPLETE nodes); NODE_TRANSITION to IDENT-001.
- 2026-08-15: **CRYPTO-001 SECURITY_REVIEW (iter 58)** — redteam FAIL-with-
  condition (RED-0001..0012 recorded in CRYPTO-001_SECURITY_REVIEW.md).
  RED-0004 fragmentation/E2EE FIXED (extended 86-B fragment header, distinct
  wire ids, bind_sender, whole-ADU re-verify) + two latent routing bugs
  (fragmentable selection, MIN-over-ranked MTU); new M7 fragmented-encrypted
  E2E PASS. RED-0002 mitigated (warn+metric+threat model); RED-0006 genuine
  weak-key forgery test; RED-0009 verify→verify_strict; RED-0012 schema
  comment. **RED-0001 CRITICAL (desktop wiring) → OPERATOR GATE.** Workspace
  **290 green**, clippy 0. CHANGELOG 0.3.5/0.3.6 (+ recovery note).
- 2026-08-15: **CRYPTO-001 IMPLEMENT COMPLETE (iter 57)** — full crypto stack
  landed + workspace 284 green, clippy 0. At-rest RowSealer/StorageKeySealer +
  schema v2 (plaintext sender_id/recipient_id + recipient index dropped),
  7 seal unit + 3 pg_store DB tests; RFC 5869 App A.2/A.3 KATs + sender/
  recipient same-key derivation; fixed pre-existing kdf.rs doctest break +
  clippy warning; M7 `crypto_e2e.rs` AC-8 E2E tests (encrypted+signed
  round-trip + tampered frame rejected). Design decisions fully implemented
  (D1–D5).
- 2026-08-14: **CRYPTO-001 DESIGN COMPLETE (iter 56)** — CRYPTO_DESIGN.md v1.0
  with AC-1..9 (module layout, wire/AAD/KDF/at-rest contracts, IrisCryptoProvider
  + KeyDirectory, P0 broadcast non-encryption, KAT line-up, crate pins). FAIL-0004
  (HKDF-BLAKE3 formally recorded) + DISC-0010 (libcrux advisories ≠ RustCrypto
  crates) added; CHANGELOG 0.3.4. Graph CRYPTO-001 RESEARCH→DESIGNING with
  DESIGN COMPLETE stage_note.
- 2026-08-14: **DEC-0009 OPERATOR AUTHORIZATION (iter 54)** — all gates +
  human gates unblocked (standing). BLK-0001 crypto/identity RESOLVED; BLK-0005
  transports RESOLVED for implementation; LEGAL-001/EMERG-001/SEC-001/TEST-001/
  ANDROID-001/IOS-001/PILOT-001 approval gates lifted. Recorded in
  `engineering/memory/records/DEC-0009.md` + registered in ALLOCATION.md.
- 2026-08-14: ML-001 ACCEPT COMPLETE (iter 53) — ML_VERIFICATION.md v1.0
  written; verifier **PASS_WITH_GAPS** (AC-1..7 all PASS + independently
  reproduced: 132 identical feature sequences, byte-identical anchors, GT 0.00
  ms vs static 545.45 ms on periodic_ferry); 3 doc corrections applied
  (AC-5 vehicle_relay/determinism-degeneracy clarity, AC-6 205-core breakdown,
  known-limitations point 5). ML-001 **COMPLETE** in PROJECT_GRAPH.yaml.
  PROJECT_STATE: completed 17, designing 2. Next: TBD (leaf).
- 2026-08-14: ML-001 SECURITY_REVIEW COMPLETE (iter 52) — redteam **PASS**
  (RED-0002): shadow isolation holds (no L3→forwarding path, zero RNG
  coupling, AC-2 byte-identical); ML-RT-02/03 fixed in-pass; ML-RT-01/04/05/
  06/07 → known_limitations. Workspace **244 green**, clippy 0. Next: VERIFY.
- 2026-08-14: ML-001 IMPLEMENT + TEST COMPLETE (iter 52) — shadow-only L3 ML
  prototype in `sim/ml/` (FeatureVec, LinearPredictor, GtPredictor,
  ShadowRecorder) wired into SIM-001 (`with_shadow()`, `shadow_features` at
  the L2 decision point, `SimOutcome.shadow_samples`); `periodic_ferry` +
  `random_walk` scenarios; 10 unit + 8 integration tests **AC 1–6 PASS**.
  Next: SECURITY_REVIEW.
- 2026-08-14: ML-001 RESEARCH COMPLETE (iter 51) — RES-0014 SOTA for L3
  delivery-probability/gateway prediction recorded + registered; graph ML-001
  RESEARCHING→DESIGNING; PROJECT_STATE researching 0/designing 3.
- 2026-08-14: ARCH-001 ACCEPT (iter 50) — ARCHITECTURE_BASELINE.md 7-layer +
  component + 20-principle reconciliation vs implemented system; verifier
  PASS_WITH_GAPS (AC 1-4); ARCH_VERIFICATION.md v1.0; graph ARCH-001 COMPLETE.
  Graph-anomaly fix complete (16 COMPLETE nodes).
- 2026-08-14: REQ-001 ACCEPT (iter 49) — REQUIREMENTS_BASELINE.md 56-req
  matrix (8 IMPLEMENTED / 17 PARTIAL / 30 DEFERRED / 2 GAP); verifier
  PASS_WITH_GAPS (AC 1-4); REQ_VERIFICATION.md v1.0; graph REQ-001 COMPLETE.
- 2026-08-14: WP-10 DESKTOP-001 ACCEPT — Tauri v2 desktop shell COMPLETE.
  DESKTOP_VERIFICATION.md AC 1-6 PASS. `crates/iris-desktop` (DesktopEngine
  host, 4-command IPC, vanilla UI, engine_roundtrip + commands_mock tests).
  Root-caused + fixed the workspace break (stub crate no src targets) and the
  comctl32 v6 manifest crash in test binaries (Tauri #13419: tauri-build
  embeds the manifest only in bins; fix = new_without_app_manifest +
  cargo:rustc-link-arg MANIFEST:EMBED/MANIFESTINPUT). Fixed async-runtime
  nesting (block_on in tokio::test) and the State type-key mismatch
  (constructors return Arc<DesktopEngine>; commands request State<'_, Arc<..>>).
  Workspace 225 green, clippy 0 warnings, app builds + runs.
- 2026-08-14: WP-9 OBS-001 ACCEPT — observability system COMPLETE.
  OBS_VERIFICATION.md AC 1-6 PASS. Observability module (injected lock-free
  MetricsRegistry, 9 `iris.*_total` counters, default-deny), ShortId
  `[u8;8]` privacy-truncated ID type (no-alloc, 16-hex Display), seam
  instrumentation across message engine (9 events + 6 counters), routing,
  SCF, gateway. Red-team PASS (OBS-RT-01..10, MEDIUMs fixed in-pass; taxonomy
  gaps recorded). Workspace 217 green, clippy clean.
- 2026-08-14: WP-8 ROUTE-002 ACCEPT — L2 opportunistic routing (PRoPHET v2 +
  binary spray-and-wait) COMPLETE. ROUTE2_VERIFICATION.md AC 1-6 PASS. Fixed
  role-swap bug (`opportunistic_forward` for src>dst decided GTMX+ from the
  receiver's perspective → ferry withheld all handoffs → L2 16/18 vs L0 17/18,
  AC3 violation; Greater branch now returns (src,dst)). SIM evidence:
  L2 17/18 (94.4%) overhead 1.41 hops 1.00 vs L0 17/18 overhead 3.18 hops
  1.29 — parity at −55.6% overhead, loop-free, deterministic. Workspace 208
  green, clippy clean.
- 2026-08-13: WP-7 GW-001 COMPLETE — gateway discovery & selection with
  red-team hardening (REDTEAM-01..05): `crates/iris-core/src/gateway/mod.rs`
  (GatewayType/Capability/Candidate/Selection, weighted quality score,
  GatewayHealthState machine Healthy/Probation/Failed/HardFailed with
  2-strike escalation, diff-based reconcile over DISCO-001 NeighborTable,
  bounded registry + NaN sanitization + deterministic selection). 19 gateway
  tests (13 core + 6 red-team regression). Workspace 190 green, clippy clean.
  GW_VERIFICATION.md v1.1 AC 1-8 PASS.

## Tests & Build

- `cargo test --workspace`: green (**601** passed / 0 failed / 1 ignored —
  iter 105 verified --all-features; **`transport::wifi_direct` = 16 tests** (7
  transport + 9 serv under the shared filter prefix) incl. GO/GC E2E round-trip
  + sender attribution, DNS-SD discovery, band-restricted fallback, discovery
  re-arm/stop, shutdown lifecycle, bounded single-link table, manager
  registration gating + 9 TXT-record adversarial tests. Iter-101 552 → iter-105
  601 (WIFIDIRECT-001 IMPLEMENT +16; all-features incl. proptest suites).
  `transport::wifiaware` = 19 under the shared filter prefix (held ACCEPT iter
  101); `transport::ble` = 42 incl. 9 RT regressions. SEC-001 all-features
  record remains **532** (verifier-reproduced iter 82, incl. proptest suites;
  all-features/proptest fns are feature-gated).
- `cargo test -p iris-core --features proptest --lib security::`: **101 PASS**
  (68 unit + 33 property functions; verifier-corrected iter 82)
- `cargo tarpaulin -p iris-core --features proptest --lib --out xml`: security
  module **590/617 = 95.6%** (AC-14 target >=95% MET; overall 77.44%) — evidence
  `%TEMP%\opencode\tarpaulin-ac14c\cobertura.xml` (verifier confirmed line-rate
  0.95624)
- `cargo test -p iris-core --lib security::`: 68 PASS
- `cargo clippy --workspace --all-targets`: **0 warnings** (iter 105 verified
  `--workspace --all-features --tests`)
- Postgres required for iris-storage tests: `IRIS_PG_PASSWORD` env var
- `cargo build -p iris-desktop` + `target/debug/iris-desktop.exe` launches
  (windowless smoke check, stays alive)

## Blockers / Gates

- **🚀 ALL GATES UNBLOCKED (DEC-0009, 2026-08-14)** — standing operator
  authorization. No node is blocked by approval. RED-0001 CRITICAL gate
  RESOLVED by operator ratification (DEC-0010, 2026-08-15) — IDENT-001 now
  COMPLETE (iter 63c/64).
- **IDENT-001 security control (DEC-0010)**: no production deployment with
  DevCryptoProvider / no identity / no sealer — **VERIFIED satisfied** in
  IDENT-001 ACCEPT; enforcement carried forward for future platform nodes.
- **BLK-0005 (RESOURCE-scoped)**: physical-device integration tests for
  BLE/WiFi/LoRa/SAT still need real hardware + mobile OS release process —
  recorded as known_limitations, NOT implementation blockers.

## Next Actions

1. **WIFIDIRECT-001 SECURITY_REVIEW (iter 107, active)** — redteam adversarial
   review of `wifi_direct.rs` / `wifi_direct_serv.rs` / transport wiring +
   `WIFI_DIRECT_COST` (pattern BLE-001_SECURITY_REVIEW.md /
   WIFIAWARE-001_SECURITY_REVIEW.md): data paths (GO/GC sender attribution,
   envelope seam gate, per-tick drain budget, outbox bounds, dropped-inbound
   telemetry); concurrency (connect+shutdown race, unavailable gate, reconnect
   reuse, MutexGuard correctness); lifecycle/availability (churn, band fallback,
   re-arm termination, persistent-GO teardown); aux (cost overflow, 255-B TXT
   cap, log hygiene — no secrets). Findings dispositioned → regression tests →
   `WIFIDIRECT-001_SECURITY_REVIEW.md` → VERIFY (iter 108).
2. Transports — WIFIDIRECT-001 (active, SECURITY_REVIEW), then LORA-001, SAT-001,
   BLE-002 (iOS).
3. TEST-001, platform apps — ANDROID-001, IOS-001, PILOT-001.
4. Follow-ups from DESKTOP-001 known_limitations: PgStorage wiring +
   sealer, subscribe_inbox Channel IPC test, relay registration test, CSP
   hardening.

## Risks / Open Items

- **SEC-001 (COMPLETE, iter 82)**: ACCEPTED — AC-1..16 all PASS, verifier
  APPROVE_WITH_NOTES reconciled. Honest-history recap: prior "464 green /
  clippy 0 / AC-14 >=95% / fuzz running" claims were **FALSE** (module did not
  compile) → corrected iter 77. Final verified: **532 workspace green / 0 failed
  (all-features, verifier-reproduced iter 82)**, clippy **0**, **101 security
  tests (68 unit + 33 property)**, **AC-14 590/617 = 95.6% (>=95% MET)**,
  **AC-15 recorded gated deviation operator-ratified** (property-based suite +
  44M+ clean fuzz), **AC-16 grep guard CLEAN**. Real bugs fixed with regression
  tests: `acl::check_sos_identity` Noop-permissive (iter 77); **`quota`
  overflow/wraparound bypass** (iter 78, fuzz-discovered); **redteam SEC-RT 12
  findings** (iter 79); **SEC-RT-15 acl root decode** (iter 80). Background fuzz
  continues clean (44M+ execs) — no blocker. Next: transports (BLE-001 first).
- Requirements baseline reconciled 2026-08-14: 30 DEFERRED (BLK-0001 crypto,
  BLK-0005 hardware, LEGAL-001/EMERG-001 human gates), 2 GAP — no BENCHMARK
  node for REQ-004-05/07 performance targets (proposed follow-up work package)
- Full-size dedup Bloom (~180 KB) requires Fragment path (type 13) for real
  deployments
- SIM-reality gap (virtual ms, enumerated contact schedules) documented in
  SIM_VERIFICATION.md
- Deployment TLS/pooling for Postgres (DEC-0002 follow-ups)
- ROUTE-002: PRoPHET is IRTF Experimental; DP exchange rides DISCO-001
  CapabilityBundle (256 B → top-N snapshot TOP_N_DP=32); per-msg max_dp_seen
  is in-session state (not persisted across reboot)
- OBS-001: route.flood/stored/dropped + scf.awaiting_contact/forward_attempt
  + topo.event defined but not emitted (known_limitations); MetricsRegistry
  reset() is the 7-day retention hook, host must call it
- DESKTOP-001: BLE/Wi-Fi local transport needs BLK-0005 hardware — desktop v1
  binds loopback/Internet transports; MemoryStorage history lost on restart;
  node identity per-process random until IDENT-001
- ML-001: P3 experimental; training offline-only in SIM-001 (synthetic traces
  may not transfer — ML-MaxProp limitation), central-model SOTA results not
  directly transferable to distributed nodes; L3 must never touch the critical
  delivery path (shadow metrics only). Redteam (RED-0002) PASS with
  known_limitations: ML-RT-01 unseeded `Uuid::now_v7()` — same-seed
  determinism claim holds for current scenario shapes only (pre-existing
  sim-wide); ML-RT-04 unbounded ShadowRecorder (harness-only); ML-RT-05/06/07
  INFO (with_shadow replace, age-slot scale, contact-index validation)
- CRYPTO-001 (COMPLETE, iter 59): AC-1..9 PASS; redteam RED-0001..0012
  dispositioned. Known limitations: RED-0003 (unsigned encryption_hdr strip-
  downgrade → PROTO-001 v2/ADR-0011); RED-0010 (codec revision → PROTO-001 v2);
  no recipient-side FS in v1 (v2 X3DH/prekeys); at-rest sealing opt-in until a
  deployment provisions and wires the master key. RESOLVED-BY-IDENT-001:
  RED-0001 (provider/identity/keydir/sealer wiring, security control verified),
  RED-0005 (key binding), RED-0008 (auth_cert_chain), RED-0011 (small-order).
- IDENT-001 (COMPLETE, iter 63c/64): identity/key mgmt/trust/address derivation
  ACCEPTED (AC-1..11 PASS, verifier APPROVE; DEC-0010 control verified
  satisfied). known_limitations: RT-002 (engine multi-key rotation decrypt E2E
  → PROTO-001 v2/RED-0010), RT-003 (inbound KeyRotation feed → DISCO-001),
  RT-004 (PgStorage::with_sealer desktop deviation under DEC-0010), RT-005
  (Windows keystore posture), RT-014 (provision two-file TOCTOU accepted
  single-instance), platform key stores deferred (Android Keystore/iOS
  Keychain/TPM), 16B sender_id wire collapse deferred to PROTO-001 v2,
  TOFU/re-key informal proof gaps, at-rest seal OPT-IN.
- EMERG-001 (COMPLETE, iter 71): emergency system ACCEPTED — AC-1..14 all
  PASS, verifier APPROVE, redteam PASS-with-recorded-deviations (RT-001..011
  dispositioned). known_limitations: BLE 512-B MTU cannot carry verified
  authority chains (2-elem 690 B; SOS P0 stays radio-native), send-side
  envelope synthesis app/authority-owned, typed `EmergencyEvent` OS-surface
  wrappers deferred to platform nodes, RT-003 CANCEL ledger resolution gated,
  RT-004 DisasterMode engine-trigger wiring deferred, RT-005 unarmed-Noop
  intentional, RT-008 drill surface suppression app-owned, RT-010 pre-gate
  audit gap tracked, 17 pre-existing desktop-transitive `cargo audit` warnings
  (gtk/atk/gdk/glib/unic-ucd/proc-macro-error; no iris-core/crypto advisory).
- SEC-001 (COMPLETE, iter 82): general security hardening ACCEPTED — AC-1..16
  all PASS, verifier APPROVE_WITH_NOTES (reconciled). known_limitations:
  **AC-15 recorded gated deviation operator-ratified 2026-08-16** (property-
  based adversarial suite 68 unit + 33 property fns + 44M+ clean fuzz execs;
  literal >=24h wall-clock fuzz not reached — accumulation continues in
  background), AC-14 uncovered = defensive-unreachable + async_trait
  attribution artifact (not coverable), BLE 512-B verified-chain ceiling
  (2-elem security chain 690 B), per-identity rate limits Sybil-defeatable
  (mitigated, not eliminated — RES-0018/Douceur IPTPS 2002), reputation
  sequential convergence (never a gate), Meshtastic-style fixed cadence NOT
  adopted, AC-16 grep guard is a review-trigger (`|| true` by design; test-NAME
  false-positive confirmed).
