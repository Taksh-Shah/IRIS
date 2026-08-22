# SAT-001 DISCOVER / UNDERSTAND - Satellite Gateway Transport

**Document ID**: IRIS-SAT-001-DISCOVER-001
**Version**: 1.0
**Node**: SAT-001 (P2 TRANSPORT, deps TRANSPORT-001 COMPLETE, requires_hardware
true, requires_human_approval false - gate RESOLVED via DEC-0009)
**Date**: 2026-08-19
**Iteration**: ~168
**Stage**: DISCOVER -> UNDERSTAND COMPLETE (read-only pass; workspace baseline
695/0/1 clippy 0 rustfmt clean **held** - no code touched)

---

## 1. Node definition (PROJECT_GRAPH lines ~446-455)

- **SAT-001** - "Satellite Gateway Transport", type **TRANSPORT**, priority
  **P2**, status **DISCOVERED**, description "Satellite (Iridium/Starlink)
  last-resort gateway for P0-P2 emergency".
- **dependencies**: `["TRANSPORT-001"]` - **COMPLETE** -> node **eligible**.
- **requires_hardware**: `true` - external satellite terminal (Iridium GO!
  hotspot / Starlink flat-panel / BGAN) reachable over Wi-Fi/USB from the
  IRIS device; BLK-0005-adjacent device gating applies (DEC-0009 authorizes
  the software/protocol implementation; physical terminal integration tests
  remain RESOURCE-gated known_limitations).
- **requires_human_approval**: `false` (reason field: human gate RESOLVED
  2026-08-14 via DEC-0009; regulatory compliance remains a research matter -
  LEGAL-001 carry).
- **C2 gap**: **CONFIRMED** - no `acceptance_criteria` present on the node ->
  **RESOLVE at DESIGN** (AC-1..n, pattern BLE-001/WIFIAWARE-001/
  WIFIDIRECT-001/BLE-002/LORA-001).

## 2. Reference surface read THIS pass

| Source | What it pins |
|---|---|
| `docs/transports/SATELLITE.md` (264 lines, full read) | Provider evaluations (Starlink / Iridium SBD / Thuraya / VSAT-BGAN); Iridium SBD = key protocol for P0-P2 relay: **340-B MO / 270-B MT**, 20-90 s latency, ~10 msgs/hour practical, ~$0.05-0.15/msg; bespoke `SbdIrisMessage` struct sketch (286 B incl. **truncated 32-B signature** - CONFLICT with CRYPTO-001/IDENT-001 full Ed25519 signatures, flag for RESEARCH); Iridium GO! local REST API sketch (`http://192.168.0.1/api/sbd/send` + `/api/sbd/inbox`, base64 payload); Starlink/VSAT = **full IP -> treat as Internet gateway (INTERNET-001 reuse), no satellite-specific code**; Thuraya reserved P0-P1 >340-B rare; India regulatory summary (Starlink DoT-approved 2024; Iridium via Bharti Airtel, no separate user license; uplink authorization via operator agreements; detailed analysis deferred to `legal/SATELLITE_REGULATION.md`); priority routing P0->SBD-capable, P1/P2->IP-capable else SBD; **cost controls**: P3+ hard gate, satellite-path dedup, LZ4 compression, default 10 TX/hour rate limit, user confirmation for non-P0, `SatelliteCostGuard {hourly_count, hourly_limit=10, daily_cost_inr, daily_budget_inr=200}`; `SimulatedSatelliteTransport` sketch (IridiumSBD effective ~1 B/s, no duty cycle, outage simulation knobs); field deployment checklist |
| `crates/iris-core/src/gateway/mod.rs` | `GatewayType::Satellite` EXISTS (:57), `GATEWAY_TAG_SATELLILE` tag const (:50 "gateway:satellite"), `serves_priority(Satellite) => P0..P2` (:83) - **GatewayManager already models satellite gateway candidates**; SAT-001 must reconcile Transport-level identity with GW-001 candidate semantics |
| `docs/transports/TRANSPORT_ABSTRACTION.md` | Satellite row present (caps/cost-class expectations; `TransportCostClass::Expensive` is the designed classification - first Expensive transport in the fleet) |
| `docs/legal/` + REG-NOTES carry | LEGAL-001 open questions include satellite licensing (IN-SPACe/DoT) - research matter, not a software gate |
| `engineering/memory/records/research/` | Next ID = **RES-0028** (ALLOCATION.md line 10) |
| LORA-001 pipeline records | Pattern source: adapter-injected seam (`LoRaLinkAdapter` lesson), regulatory-analog guard (duty tracker -> **cost/budget guard** is the satellite analog), SIMULATION_VALIDATED tagging discipline |

## 3. Scope surface (candidate shape - DESIGN decides)

- **Provider abstraction**: IridiumSBD (constrained data, primary P0 path) vs
  IP-based providers (Starlink/VSAT/Thuraya-IP = Internet-gateway reuse,
  likely OUT of SAT-001 core scope beyond gateway tagging).
- **`SatelliteTransport`** Transport impl: adapter-injected link client seam
  (HTTP-REST to Iridium GO! or serial/modem AT variants), P0-P2-only
  eligibility hard gate, cost-guard consumption before TX, Caps row with
  `cost_class: Expensive` + max_message_size 340 (MO) / receive 270 (MT)
  asymmetry handling.
- **SBD framing**: carrier frame <=340 B carrying STANDARD IRIS envelopes
  (237-B V001 fits) - NOT the doc's bespoke truncated-signature struct;
  fragmentation NOT planned (P0-P2 envelopes fit; P3+ excluded anyway).
- **`SatelliteCostGuard`**: hourly-count + daily-INR budget enforcement,
  non-P0 confirmation hook, per-message cost estimate surfaced to OBS/UI.
- **Simulated**: deterministic provider sim (latency 20-90 s, outage
  probability, quota exhaustion) - SIMULATION_VALIDATED discipline like
  LORA-001.
- **OBS-001**: `iris.transport.satellite.*` (messages_tx/rx, cost_inr,
  quota_remaining, queue depth).

## 4. Research questions staged (RQ-1..RQ-6 -> RES-0028)

- **RQ-1 Provider/API reality check (2026)**: doc's Iridium GO! REST sketch +
  legacy SBD vs current Iridium IoT offerings (Certus/IMT continuity,
  SBD sunset risk); which surface does v1 target?
- **RQ-2 Envelope conflict resolution**: bespoke `SbdIrisMessage`
  (truncated 32-B signature, 16-B sender IDs, plaintext fields) CONFLICTS with
  CRYPTO-001 signed envelopes + IDENT-001 PeerIds - expected verdict: carry
  standard envelopes verbatim (<=340 B budget), REJECT bespoke wire struct;
  confirm against ADDRESSING/MESSAGE_ENVELOPE docs.
- **RQ-3 Cost-guard semantics**: hourly/daily budgets, confirmation UX hook,
  dedup interaction, P3+ enforcement point (transport vs manager vs engine),
  offline-budget persistence.
- **RQ-4 Integration**: TransportManager registration + Caps row (first
  `Expensive` cost class), selection interplay (prefer-low-cost suppresses
  satellite except emergency), GW-001 GatewayType::Satellite reconciliation,
  P0/P1 multipath inclusion rules, latency ceiling effects on ACKs.
- **RQ-5 Security**: 20-90 s store-and-forward latency vs replay/freshness
  windows (SEC-001 replay engine skew budgets), hostile SBD injection ->
  envelope verification unchanged, no new trust anchors, spoofing of
  gateway:satellite tags.
- **RQ-6 Sim fidelity + hardware gating**: deterministic sim parameters,
  calibration experiment analog (EXP-SAT-001?), BLK-0005 physical-terminal
  rows, monetary-cost honesty labeling.

## 5. Known limitations baseline (record, not blockers)

- Physical Iridium GO!/terminal integration tests = RESOURCE-gated
  (BLK-0005-adjacent); software core + sims are host-verifiable.
- LEGAL-001 carry: satellite licensing/operator agreements = open legal
  questions; no legal opinion rendered.
- Monetary cost figures in SATELLITE.md are planning estimates, not quotes.
- Doc-vs-reality gap on the Iridium API surface is EXPECTED (2023-era doc) -
  RQ-1 resolves it; no code trusts the sketch yet.

## 6. Stage outcome

UNDERSTAND COMPLETE. PROJECT_GRAPH SAT-001 evidence(1) + stage_note DISCOVER
COMPLETE + known_limitations(4). NEXT: **RESEARCH (iter ~169, RES-0028)** per
ALLOCATION next-ID; then DESIGN (AC-1..n resolves the C2 gap) -> IMPLEMENT ->
TEST -> SECURITY_REVIEW -> VERIFY -> ACCEPT. Mission milestones
(MVP/Alpha/Beta/Pilot) tracked separately.
