# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-09-06T10:30:00+05:30

## Current hardware phase

The graph node `WIFIDIRECT-001` is software-complete; its physical verification
follow-up is active. The two-phone bench is connected and identified in
`docs/bug-hunting/hardware_verification/hardware_fix_log.md`.

- Shipping-app Tier-2 cold-cycle gate: **HW-verified 10/10**.
- Conditions: Bluetooth disabled; exactly one Wi-Fi Direct group; bidirectional
  delivery; persistent keys; no OS invitation dialog.
- Mobly Tier-2 test: **HW-PENDING** on this Vivo/OriginOS pair because
  the corrected same-ID harness still fails reliable peer delivery after three
  bounded attempts; the `/addkey`/`/to` hypothesis was also tested and rejected
  as the complete cause; see `FAIL-0008.md`.
- Gate of record: `docs/bug-hunting/hardware_verification/tier2_wifidirect_cold_cycle.sh`.
- No production source implementation change is authorized by this blocked
  evidence; fresh failure-focused research/design is required first.

## Active Node: SAT-001 — Satellite Gateway Transport

- **Type**: TRANSPORT (Iridium SBD constrained-data path; IP providers =
  Internet-gateway reuse)
- **Priority**: P2 · **requires_hardware**: true (BLK-0005-adjacent) ·
  human gate: RESOLVED (DEC-0009)
- **Status**: **UNDERSTANDING** (iter ~168, evidence 1, DISCOVER COMPLETE);
  **Status**: RESEARCH_COMPLETE (iter ~169, evidence 2 — RES-0028 PROCEED-WITH-CONDITIONS)
- **C2 gap**: CONFIRMED — DESIGN resolves AC-1..n
- **Key facts**: Iridium SBD 340-B MO / 270-B MT, 20–90 s latency, ~10 msg/h;
  P0–P2 only; first `Expensive` cost-class transport; bespoke doc struct
  conflicts w/ CRYPTO-001 → RQ-2; GatewayType::Satellite already modeled in
  GW-001.
## Active Node: LORA-001 — LoRa Gateway Transport
## NODE_TRANSITION → SAT-001 (iter ~167)

LORA-001 ACCEPTED: 30 COMPLETE nodes. Pipeline: DISCOVER ~160 → RESEARCH ~161
(RES-0027) → DESIGN ~162 → IMPLEMENT ~163 (lora.rs rewrite + corruption repair,
AC-1..13) → TEST ~164 (AC-1..15 map, live 686/0/1) → SECURITY_REVIEW ~165
(redteam FAIL→RESOLVED incl. RT-101 CRITICAL billing basis + RT-200 rate_limiter
clock injection; live 695/0/1) → VERIFY ~166 (AC-1..17 table, verifier APPROVE,
zero findings) → ACCEPT ~167. AC-15 GATED/BLK-0005 explicit known_limitation.
CHANGELOG 0.3.62. **NEXT: SAT-001 UNDERSTAND (iter ~168)**
(`records/SAT-001_DISCOVER.md`, pattern LORA-001_DISCOVER.md).

- **Type**: TRANSPORT (long-range radio transport via external LoRa module + BLE
  bridge)
- **Priority**: P2
- **Status**: **COMPLETE — ACCEPTED (iter ~167, evidence 8)**. NODE_TRANSITION
  done: active node is now **SAT-001** (P2 TRANSPORT, requires_hardware,
  DISCOVERED) — UNDERSTAND next (iter ~168).
- **Deps**: TRANSPORT-001 — **COMPLETE**
- **Requires hardware**: true (external LoRa module; BLK-0005-adjacent device
  gating applies — GAP-004)
- **Description**: "LoRa long-range transport via external LoRa module + BLE
  bridge" — 866.0 MHz ISM, ≤25 mW e.r.p. (WPC G.S.R. 853(E) 2021 865–868 band,
  carved out of the v1 PILOT scope as GAP-004 REQUIRES_HARDWARE)
- **C2 gap**: RESOLVED-VIA-RESEARCH+DESIGN (AC-1..17 defined by
  `LORA_001_DESIGN.md` v1.0; IMPLEMENT resolved AC-1..13+15, TEST mapped AC-1..15)
- **Evidence**: 6 (`LORA-001_DISCOVER.md` iter ~160 + `research/RES-0027.md`
  iter ~161 + `docs/implementation/LORA_001_DESIGN.md` iter ~162 +
  `transport/lora.rs` iter ~163 + `records/LORA-001_TEST.md` iter ~164 +
  `records/LORA-001_SECURITY_REVIEW.md` iter ~165)

## VERIFY COMPLETE (iter ~166) — verifier APPROVE

`engineering/memory/records/LORA-001_VERIFICATION.md` — AC-1..17 evidence
table + independent-verifier-style reproduction at the recorded revision:

- **Live**: workspace **695/0/1** (`--all-features` full sweep),
  `transport::lora` **31/31**, rate_limiter **12/12** hermetic, clippy
  `-D warnings` **0**, fmt clean.
- **Evidence integrity**: 18/18 AC-cited test names re-grep'd in source with
  file:line (zero fabricated identifiers); all RT dispositions re-located as
  real code (RT-101 lora.rs:1312, RT-102 :282, RT-104 :305, RT-105 :1419,
  RT-106 :523, RT-109 :492, RT-200 rate_limiter.rs:146).
- **Verdicts**: AC-1..14 PASS · AC-15 GATED/BLK-0005 recorded · AC-16 PASS
  (FAIL→RESOLVED) · AC-17 PASS → **verifier APPROVE, zero findings**.

**STAGE_TRANSITION → ACCEPT (iter ~167)** — then NODE_TRANSITION → SAT-001.
## SECURITY_REVIEW COMPLETE (iter ~165) — FAIL → RESOLVED

`engineering/memory/records/LORA-001_SECURITY_REVIEW.md` — independent
redteam adversarial pass over `transport/lora.rs` (+ rate_limiter flake
disposition). **RT-101 CRITICAL FIXED**: duty was billed on payload only —
the 18-B header is on-air too; a minimal-frame flood was admitted at 35,776 ms
accounted vs ~63,984 ms true (~1.78% duty, 78% over Table-I).
`try_send_inner` now bills the full encoded frame. **RT-102..106 MEDIUM
FIXED**: monotonic clock clamp; attach_adapter Result (dead dongles rejected);
connect() status probe before Connected; shutdown slot take;
`DutyCycleTracker::refund` on TX failure (+tx_failures metric); poll_inbound
skips+counts malformed frames; backlog size gate + 256-entry cap.
**RT-107/109 LOW FIXED**, **RT-108/110 RECORDED** (receipt handed-to-radio
contract; one-frame-per-rx firmware contract), **RT-111 INFO PASS** (sim
honesty). **RT-200 FIXED cross-file**: rate_limiter injectable clock — the
iter ~164 transient flake root-caused to real-time refill under load;
proptests hermetic via frozen clock, real-clock refill test kept, seeds stay
checked in (replay green).

**Live after fixes**: workspace **695 passed / 0 failed / 1 ignored**
(`--all-features` full sweep), clippy `-D warnings` **0**, fmt clean,
`transport::lora` **31/31** (+4 RT regressions), rate_limiter **12/12**.

**STAGE_TRANSITION → VERIFY (iter ~166)** → ACCEPT (~167) → NODE_TRANSITION
SAT-001.

## TEST COMPLETE (iter ~164) — AC-1..15 evidence map

`engineering/memory/records/LORA-001_TEST.md` authored (pattern
BLE-002_TEST.md). **Live THIS pass**: `transport::lora` **27/27**, workspace
**686 passed / 0 failed / 1 ignored** (`--all-features`, full `--no-fail-fast`
sweep green end-to-end), clippy `-D warnings` **0**, fmt clean. Prior-664
bookkeeping reconciled to live measurement.

- **AC-1..14 PASS** — manager register/select/caps (237-B envelope capacity +
  WPC regulatory_band); duty budget refusal/recovery windows; bucket caps
  60/25/10/5 + no-override-knob proof; P0-switch seam exhaustion-without-hold;
  backlog priority FIFO hold/drain-on-recovery; adapter conformance pair +
  HardwareGated stubs; airtime anchors 1251/1169/8528 ms + SF12-P0 profile +
  raw rates; frame round-trip/defensive-parse/255-B cap + SLIP malformed
  rejection; fragment/dedup/crypto reuse as explicit review assertions
  (MSG-001 layers + crypto_e2e green — no new code); OBS-001 counters live;
  SimLinkBudget ranges sane + SIMULATION_VALIDATED tagging; hot-plug lifecycle.
- **AC-15 GATED/BLK-0005** — physical SX1262/E22 legs recorded honestly.
- **Transient-flake IDENTIFIED**: single lib-test failure in 2 of 6 workspace
  invocations under post-compile load — root-caused via proptest's
  auto-persisted regression seeds
  (`proptest-regressions/security/rate_limiter.txt`, two new `cc` entries) to
  `security::rate_limiter` proptests (SEC-001 scope; real-time token refill
  under CPU saturation). Persisted seeds replay green (12/12); zero lora
  involvement → keep seeds checked in + clock-injection deflake follow-up at
  SECURITY_REVIEW (~165).

**STAGE_TRANSITION → SECURITY_REVIEW (iter ~165)** → VERIFY (~166) → ACCEPT →
NODE_TRANSITION SAT-001.

## IMPLEMENT COMPLETE (iter ~163) — lora.rs rewritten clean (AC-1..AC-13)

**Corruption repaired first**: the pre-existing `lora.rs` orphan was invalid
Rust (`by_bucket[b]? 0 else 0`, `Instant::EpochMilliseconds` casts, phantom
duplicated methods, truncated mid-file) and was never wired into the mod tree —
same corruption class as ALLOCATION.md (iter ~161). Fully rewritten + `pub mod
lora;` added to `transport/mod.rs`.

Delivered per design:

- **ComplianceConfig** + `validate()` — Table-I pins (866.0 MHz / ≤25 mW e.r.p.
  = 14 dBm / ≤1% duty / 3,600,000 ms window / 36,000 ms budget);
  `DutyCycleTracker::try_new` refuses non-compliant configs → **no override
  knob exists** (DEC-LORA-0002).
- **DutyCycleTracker** — sliding 1-h window over an injectable ms clock;
  global budget + bucket caps Emergency 21,600 / Medical 9,000 / Location
  3,600 / Bulk 1,800 ms; `check_and_consume` → `NextWindow | BudgetExhausted`
  with earliest-expiry recovery delay (R7 seam); `duty_cycle_remaining()`
  exposed on the transport for P0-switch.
- **BacklogQueue** — stable priority FIFO (`pop_highest`, REQ-ROUTE-C-003).
- **RadioProfile** SF9/BW125/CR4/5@866 default + SF12-P0; `airtime_ms` Semtech
  AN1200.13 pure fn — anchors: 255-B→1251 ms / 237-B→1169 ms @SF9,
  237-B→8528 ms @SF12; `raw_rate_bps` 1757/292.
- **LoRaFrame** v1 `[ver][prio][msg_id16][payload≤237]` = exactly the 255-B PHY
  cap with the 237-B P0 envelope fitting; SLIP RFC-1055 encode/decode with
  defensive parse (leading-END tolerance, escape validation, alloc cap).
- **LoRaLinkAdapter** seam (`open/close/tx/rx/status`) + AtSerial/SpiNative
  HardwareGated stubs (BLK-0005) + SimulatedLoRaAdapter deterministic pair.
- **SimLinkBudget** — AN1200.13 sensitivity (−129/−136 dBm anchors) +
  Okumura-Hata urban (hb 30 m / hm 1.5 m, validity clamp) + fade-margin PDR;
  **SIMULATION_VALIDATED only**.
- **LoRaTransport** Transport impl — caps row (envelope capacity 237, WPC
  regulatory_band), hot-plug attach/detach auto open/close, duty refusal →
  `Busy` never panic, `drain_backlog`, `poll_inbound` (zero-PeerId attribution,
  WAW-RT-007 precedent), OBS-001 counters.

**Design corrections resolved into doc** (docs are the contract): airtime
"1.82 s" → formula values; throughput 14_000 bps → 1_757 raw SF9 rate; frame =
18-B header + ≤237-B envelope; caps report 237.

**AC-15 verified LIVE this pass**: workspace **686 passed / 0 failed /
1 ignored** (iris-core lib 600 = 573 baseline + 27 new `transport::lora`
tests), clippy `-D warnings` **0**, fmt clean. Physical-device rows
GATED/BLK-0005 recorded.

**STAGE_TRANSITION → TEST (iter ~164)** → SECURITY_REVIEW (~165) → VERIFY
(~166) → ACCEPT → NODE_TRANSITION SAT-001.
