# SAT-001 Design — Satellite Gateway Transport v1

**Document ID**: IRIS-SAT-001-DESIGN-001
**Version**: 1.0
**Node**: SAT-001 (P2 TRANSPORT, hardware-gated, deps TRANSPORT-001 COMPLETE;
human gate RESOLVED via DEC-0009)
**Date**: 2026-08-22
**Research basis**: `engineering/memory/records/research/RES-0028.md`
(iter ~169 — verdict **PROCEED-WITH-CONDITIONS**, RQ-1..RQ-6 resolved
ADOPT/ADOPT-WITH-CONDITION, design inputs D-1..D-6, external facts FC-1..FC-8,
gaps G-1..G-7); `engineering/memory/records/SAT-001_DISCOVER.md` (iter ~168);
`docs/transports/SATELLITE.md`; TRANSPORT_ABSTRACTION.md satellite row;
gateway/mod.rs (`GatewayType::Satellite`, serves_priority P0..P2);
CONGESTION_CONTROL.md §355 relay-cadence seam; source surface
`crates/iris-core/src/transport/mod.rs` + `manager.rs` gating +
`lora.rs` pattern (adapter-injected seam, RT-103/104/105 lessons);
ACCEPTANCE_POLICY.yaml TRANSPORT-type additions; BLK-0005 hardware gating.
**Absorbs**: RES-0028 verdict + D-1..D-6 + FC-1..FC-8 + gaps G-1..G-7
(design-positioned); REG-NOTES/LEGAL-001 satellite carry codified as
DEC-SAT-0006 legal gate.
**Reconciles**: `docs/transports/SATELLITE.md` vs RES-0028 — FC-1..FC-8
corrections STAGED here (GO! REST fiction removal; Telecom Act 2023 addition;
96xx-vs-handset SBD scope; dual-figure latency; SOS-exempt cost model; new
security section); applied to the doc at DOCUMENT stage unless load-bearing
at DESIGN (§2.3 latency + §2.2 budget figures are load-bearing and pinned
below).

---

## 1. Scope

Implement the **satellite gateway transport** on the existing `Transport`
trait as an **adapter-injected** node: a platform-neutral Rust core in
`crates/iris-core/src/transport/satellite.rs` plus a **`SatelliteLinkAdapter`**
trait whose implementations talk to **AT-command SBD modems** (RockBLOCK 9603 /
9602-N, Iridium 9604) over serial/USB owned by the adapter crate. The phone/RPi
becomes a satellite gateway node for **P0–P2 emergency messages ONLY** — the
first `TransportCostClass::Expensive` transport in the fleet. Physical modem
integration is **hardware-gated** (BLK-0005): this node delivers the Rust core,
the adapter seam, the **SOS-exempt `SatelliteCostGuard`**, the deterministic
simulated wiring, and AC-1..17 (C2 gap RESOLVED). No new crypto: envelopes are
carried VERBATIM; the hostile-pipe posture makes RX-side CRYPTO-001 verification
mandatory (RES-0028 RQ-5).

**v1 shape**: single provider **IridiumSbd** over AT-command 96xx-class modems.
Starlink / VSAT / BGAN = full-IP paths served by INTERNET-001 gateways
(out-of-core); Thuraya deferred; Iridium GO!/exec excluded (no-OSS policy +
unverifiable REST fiction removed per FC-3); Certus IMT + NTN Direct reserved
variants documented-not-built (post-v1 backends behind the same seam).

**Explicitly NOT relaxed**: P3+ hard gate transport-side (emergency-only by
construction); no signature/crypto-material truncation ever (bespoke
`SbdIrisMessage` struct REJECTED per FC-2); satellite never presented as
guaranteed delivery (~1 mW jamming defeats it locally — G-6).

---

## 2. Wire-constraints and platform contract (RES-0028 → ACs)

### 2.1 Provider/hardware target (RQ-1 / D-1)
- **v1 reference hardware**: AT-command SBD modems — RockBLOCK 9603 /
  9602-N, Iridium 9604 (commercial Jun 2026; unified AT set incl. legacy
  `AT+SBDWT/SBDRT/SBDIX` session commands). Adapter crates own serial/USB I/O;
  `iris-core` stays platform-neutral (no new deps).
- **Out-of-core**: Iridium GO!/exec (official API is NDA-gated SOAP/SIP with
  mandatory certification; "does not support open source" — ifp.iridium.com
  L1; the doc's local REST endpoints do not exist publicly — FC-3 removal),
  Certus IMT (9704), NTN Direct NB-IoT (trials-stage) — all future backends
  behind the SAME seam (`SatelliteProvider` reserved variants documented,
  non_exhaustive).
- SBD service life: actively sold, NO published EOL (G-4) — adapter seam is
  the mitigation; v2 migration obligation recorded.

### 2.2 SBD carrier framing (RQ-2 / D-2)
- Budgets: **MO ≤340 B / MT ≤270 B (96xx modules)**; handset class 95xx =
  1960/1890 B (documented, not targeted). Standard CRYPTO-001 envelopes are
  carried **VERBATIM** — P0 ≈237-B V001 fits MO with ~103-B headroom; P1/P2
  envelopes sized upstream must respect ≤340 B or fragment via the EXISTING
  MSG-001 layer BEFORE the transport (transport rejects oversize; never
  truncates).
- `SbdFrame` = minimal carrier wrapper (envelope bytes + integrity fields
  pinned at IMPLEMENT; zero crypto material added or removed). Defensive
  parse: malformed/truncated/oversized frames skipped-and-counted (RT-105
  lesson), never panic/alloc-blowup.
- No LZ4 compression in v1 (overhead dominates at these sizes — measured
  hypothesis pending own benchmark).
- Billing reality pins the cost model: min billable message 10 B; mailbox
  checks billable ~$0.05/event when no MT queued — **RX costs money** (FC-7).

### 2.3 Latency & ACK semantics (RQ-4 / D-4)
- Network transit ≈5 s short / ≈20 s max-length messages to gateway delivery;
  practical end-to-end 20–90 s+ including polling cadence and relay
  processing — BOTH figures stated (FC-6), scheduling uses the conservative
  one.
- ACKs are asynchronous custody-style: retries ≥300 s base ±20% jitter,
  never assuming RTT <90 s; delivery confirmation arrives via later sessions/
  mesh echo, never synchronous.

### 2.4 Cost-guard contract (RQ-3 / D-3)
- `CostGuardConfig { hourly_msg_cap: 10, daily_budget_inr: 200,
  mailbox_check_cost_usd: 0.05, confirm_discretionary_p1_p2: true }`.
- **P0 is NEVER blocked** — bypasses hourly cap and daily budget entirely
  (industry SOS-exempt pattern: Zoleo "SOS messages … do not count toward
  your monthly message allotment", Garmin quota tiers, both L1).
- P1/P2: hourly count + daily INR hard-stop with queue-not-drop (drains on
  window recovery); counters persisted on EVERY mutation via a `CostLedger`
  trait (in-memory default impl; durable impl deferred); confirmation hook
  fires for discretionary P1/P2 only.

---

## 3. Security posture (RES-0028 RQ-5 → ACs)

1. **HOSTILE PIPE — envelope verification is MANDATORY, not merely
   sufficient** (arXiv:2603.12062, Mar 2026, first public systematic analysis
   of Iridium radio auth): SIM key extractable → full device
   cloning/impersonation; nearly all signaling unencrypted (cleartext
   credentials across >186M captured frames); SDR **downlink spoofing
   demonstrated** — devices accept fake Ring Alerts (no network-to-device
   authentication); recorded auth bursts REPLAY successfully; jamming cheap
   (Ring Alerts arrive ≈−120 dBm; ≈1 mW disrupts regionally); RECORD
   (USENIX Sec 2024): passive geolocation to ≈10 km. The link therefore has
   **ZERO usable authenticity/confidentiality**: every inbound envelope MUST
   pass CRYPTO-001 verify ABOVE the transport before any UI/alert/effect
   surface. No transport trust anchors exist or will be added.
2. **Ring Alerts are wake-up hints only** — never content trust, never an
   authenticity signal (spoofable downlink); they may trigger a mailbox poll,
   nothing more.
3. **Freshness uses per-sender monotonic sequence numbers**, not timestamps
   alone — multi-hour store-and-forward makes timestamp windows unsound;
   coordination with the CRYPTO-001 owner is recorded as a follow-up (G-7);
   until landed, SEC-001 replay gates remain the only freshness layer.
4. **Zero-fallback attribution** (WAW-RT-007 precedent): unverified inbound
   frames are never attributed to a peer nor delivered; malformed/truncated/
   oversized frames are skipped-and-counted (RT-105), no panic/alloc blowup.
5. **Multipath dedup is the integrity net**: satellite-delivered envelopes
   deduplicate once at the envelope layer like any other path (MSG-001);
   satellite NEVER bypasses dedup/ACK/dispatch gates. Satellite is **never
   presented as guaranteed delivery** — ≈1 mW jamming defeats the link in
   exactly the scenarios IRIS targets (G-6); it is best-effort redundancy.
6. **Metadata honesty**: any satellite TX reveals approximate location
   (≈10 km demonstrated) — accepted for P0 SOS use, documented in user docs.
7. **Hardware-trust honesty**: all evidence from the simulated adapter is
   **SIMULATION_VALIDATED**, never field claims (G-5; physical-terminal rows
   stay gated, AC-15).

---

## 4. Module layout (`crates/iris-core/src/transport/satellite.rs`)

| Piece | Status | What SAT-001 delivers |
|---|---|---|
| `Transport` trait (mod.rs @316) | COMPLETE | implemented by `SatelliteTransport`; `&self` shared-Arc hot-plug register/deregister (manager.rs @84/@109) |
| `TransportCapabilities` (@151) | COMPLETE | Satellite row: `max_message_size: 340` (MO envelope capacity), broadcast false / unicast true / multicast false, `range_m_{min,max,typical}: 100_000/20_000_000/2_000_000` (nominal global service area), `typical_throughput_bps: 2_400` (session air rate; sustained ≈1 B/s under guard caps), `typical_latency_ms: 90_000` (conservative end-to-end; transit 5–20 s documented §2.3), `requires_infrastructure: true`, bg flags platform-bridge-mediated, `requires_special_hardware: true`, `cost_class: Expensive`, **`regulatory_band: Some("GMPCS/MSS operator authorization required")`** |
| `TransportCostClass` (@182) | COMPLETE | `Expensive` already names satellite (@188) |
| `SAT_COST` | NEW const | battery model (idle ≈35 mA, TX-burst modeled per-kbps) — DESIGN ESTIMATE, hardware bench pending (BLK-0005) |
| `CostGuardConfig` | **NEW** | `{ hourly_msg_cap: 10, daily_budget_inr: 200, mailbox_check_cost_usd: 0.05, confirm_discretionary_p1_p2: true }` — the analog of LoRa's `ComplianceConfig` (**no duty cycle** exists here; spend is the legal/commercial floor instead) |
| `SatelliteProvider` enum | **NEW** | `{ IridiumSbd }` constructible in v1; reserved variants DOCUMENTED-not-built: `CertusImt`, `NtnDirect`, `GoExternalGateway` (non_exhaustive) |
| `SatelliteLinkAdapter` trait | **NEW** | `open/close/tx/rx/status` — the AT-modem seam (D-1; BleAdapter/LoRaLinkAdapter lesson) |
| `AtSbdModemAdapter` | stub | RockBLOCK 9603 / 9602-N / 9604 AT-surface shape (**HardwareGated, BLK-0005**) |
| `SimulatedSatelliteAdapter` | **NEW** | deterministic in-memory adapter + paired-conformance harness; knobs: `latency_ms` range, outage probability, quota exhaustion |
| `SbdFrame` encode/decode | **NEW** | ≤340-B MO carrier carrying CRYPTO-001 envelopes VERBATIM + defensive parse + round-trip tests |
| `CostLedger` trait + `InMemoryLedger` | **NEW** | mutation-time persistence seam (AC-3); durable impl deferred |
| `SatelliteCostGuard` | **NEW** | SOS-exempt enforcement, hourly cap + daily INR budget hard-stop, queue-not-drop, RX-check cost modeling, confirmation hook |
| `SatelliteTransport` (Transport impl) | **NEW** | wires everything: eligibility gate, guard, adapter lifecycle, incoming stream; RT-103 lessons (open-failure rejected, `connect()` probes status, shutdown takes slot); RT-104 (guard consumption AFTER successful adapter tx OR refund-on-failure); RT-105 (malformed inbound skip-and-count) |
| OBS-001 metrics + tracing target | **NEW** | `iris.transport.satellite.*`: tx/rx counts, guard admit/refuse, ledger mutations, mailbox checks billed, outages, malformed-skipped |

---

## 5. Router/integration seam (RES-0028 RQ-4 / D-4/D-5)

- **Relay-cadence seam** (CONGESTION_CONTROL.md §355): satellite joins the
  cadence tiers at the SLOWEST tier; P0/P1 emergency + directed point-to-point
  exempt (EMERG-001 bypass); poll/mailbox cadence is budget-aware because RX
  checks bill (FC-7).
- **Multipath**: satellite enters the P0/P1 multipath candidate set ONLY when
  eligible (P0–P2 gate passes) AND affordable (`cost_snapshot()` surfaces
  guard state so selection prefers free transports first — `Expensive` is a
  last-resort class by construction).
- **GW-001 reconciliation AT DESIGN**: `GatewayType::Satellite` already exists
  (gateway/mod.rs @57, tag @50, `default_max_priority` → P2 @83,
  `serves_priority` tests @802–814) — gateway-level priority semantics match
  transport-level EmergencyOnly; identity reconciled here, **no GW code change
  in v1**.
- Queue-not-drop backlog: guard-refused P1/P2 drains when the hourly/day
  window recovers (REQ-ROUTE-C-003 pattern); P0 never queues behind budgets.

---

## 6. Acceptance criteria (SAT-001 AC-1..17)

Defined in `engineering/PROJECT_GRAPH.yaml` SAT-001 node; mapped to evidence at
TEST/VERIFY. Pattern BLE-001/BLE-002/LORA-001.

- **AC-1** `SatelliteTransport` registers + selects via `TransportManager`;
  Caps row pinned (Expensive class, `max_message_size=340`,
  `regulatory_band=Some("GMPCS/MSS operator authorization required")`);
  >340-B unfragmentable rejected by manager gating.
- **AC-2** CostGuard SOS-exemption: unit tests prove **P0 flows at zero
  remaining budget** (cap AND budget exhausted) while P1/P2 are blocked in
  the same state (D-3).
- **AC-3** Persisted-counters contract: `CostLedger` trait + mutation-time
  persistence seam tested with an in-memory double-write harness (every
  mutation observable; reboot-survival delegated to the future durable impl).
- **AC-4** Hourly cap + daily INR budget enforce **hard-stop +
  queue-not-drop**: refusal paths tested; queued items recover on window
  reset; confirmation hook fires for discretionary P1/P2 only.
- **AC-5** Eligibility hard gate: P3–P7 sends rejected transport-side
  regardless of manager routing (rejection tests; mirrors GW-001 semantics).
- **AC-6** `SatelliteLinkAdapter` conformance type-checked against
  `SimulatedSatelliteAdapter` (BLE-001 AC-11 pattern); `AtSbdModemAdapter`
  stub present with **BLK-0005 recorded** (D-1).
- **AC-7** `SbdFrame`: ≤340 MO / ≤270 MT caps enforced; envelope-verbatim
  round-trip byte-equality; defensive parse of malformed/truncated/oversized
  frames (no panic/alloc blowup; skip-and-count) (D-2).
- **AC-8** Hostile-pipe posture asserted in review: receive path REQUIRES
  envelope verification ABOVE the transport before any effect; zero-fallback
  attribution (WAW-RT-007 precedent); Ring Alerts consumed as hints only (D-5).
- **AC-9** Async ACK semantics documented; retry/jitter constants pinned
  (base ≥300 s ± 20% jitter; never assume RTT<90 s) (D-4).
- **AC-10** OBS-001 `iris.transport.satellite.*` metrics + tracing target;
  relay-cadence slowest-tier note recorded (§355).
- **AC-11** No crypto changes: envelopes opaque bytes to this transport;
  `crypto_e2e` suite stays green; no truncation anywhere (RQ-2/FC-2).
- **AC-12** Sim determinism: seeded knobs (latency range, outage probability,
  quota exhaustion) reproduce identical runs; ALL test evidence tagged
  **SIMULATION_VALIDATED** until a real modem exists (D-6/G-5).
- **AC-13** Hot-plug lifecycle + clean shutdown per RT-103 lessons:
  open-failure rejected, `connect()` probes adapter status, shutdown takes
  the conflict slot, register/deregister balanced.
- **AC-14** Doc reconciliation: SATELLITE.md corrections staged FC-1..FC-8
  (GO! REST fiction removed; Telecom Act 2023 added; 96xx-vs-handset scope;
  dual-figure latency; SOS-exempt cost model; new security section);
  SATELLITE_SIMULATION.md + SYNCHRONIZATION.md satellite rows aligned.
- **AC-15** GATED/BLK-0005 physical-terminal rows recorded honestly: real-
  modem throughput/latency/cost benchmarks + field activation = known
  limitation until hardware + legal gates clear.
- **AC-16** SECURITY_REVIEW stage: receive-path redteam MANDATORY per RES-0028
  G-2 (adversary model changed: cloning + spoofable downlink proven) —
  findings dispositioned/fixed/recorded.
- **AC-17** VERIFY stage: independent verifier evidence table + APPROVE
  (pattern EMERG-001/SEC-001/BLE-001/LORA-001).

---

## 7. Declared decisions (DEC-SAT-0001..0007 — ratified in DECISIONS.md)

- **DEC-SAT-0001** v1 provider = **IridiumSbd over AT-command 96xx modems
  only** (RockBLOCK 9603/9602-N, 9604); GO!/IMT/NTN Direct out-of-core; GO!
  REST fiction removed (RES-0028 RQ-1/D-1/FC-3).
- **DEC-SAT-0002** **Standard-envelope verbatim carry** into SBD payloads;
  bespoke struct + signature truncation REJECTED forever; stream-level
  fragmentation w/ per-fragment MAC if >340 B ever needed; no LZ4 in v1
  (RQ-2/D-2/FC-2).
- **DEC-SAT-0003** **SOS-exempt `SatelliteCostGuard`**: P0 unbudgeted; P1/P2
  capped (10/hour default, ₹200/day default INR) hard-stop + queue-not-drop;
  counters persisted on every mutation (`CostLedger`); RX mailbox checks
  cost-modeled; confirmation hook for discretionary P1/P2 (RQ-3/D-3/FC-7).
- **DEC-SAT-0004** Classification **Expensive/EmergencyOnly(P0–P2)** with
  transport-side P3+ hard gate; async custody-style ACKs, retries ≥ minutes
  jittered, never assume RTT<90 s (RQ-4/D-4).
- **DEC-SAT-0005** **Hostile-pipe posture**: mandatory RX envelope
  verification above the transport; Ring Alert hint-only; per-sender
  monotonic-sequence freshness (CRYPTO-001 owner coordination = recorded
  follow-up); satellite never guaranteed-delivery (RQ-5/D-5/G-7).
- **DEC-SAT-0006** India legal **HIGH-RISK gate**: Telecom Act 2023
  criminalizes unauthorized satcom TX (≤3 yr imprisonment); Customs Cir.
  37/2010 import controls; NO field activation of TX-capable units without
  legal sign-off + authorized GMPCS/MSS channel (LEGAL_REVIEW gap recorded);
  Starlink stays out-of-core (unavailable in India until late 2026)
  (RQ-6/D-6/FC-4).
- **DEC-SAT-0007** **SIMULATION_VALIDATED discipline**: all evidence simulated
  until HARDWARE_VALIDATED benchmark on a real modem (new EXP; "~10 msg/h" is
  folklore-grade and must not drive more than defaults) (G-5).

---

## 8. Risks / known limitations (G-1..G-7 dispositions)

- **G-1 (HIGH) v1 hardware ambiguity** → DISPOSITIONED: DEC-SAT-0001 pins
  AT-command SBD modems (RockBLOCK 9603/9602-N / 9604) as reference hardware;
  GO! excluded (no-OSS policy).
- **G-2 (HIGH) adversary-model change** (cloning + spoofable downlink proven,
  2026) → DISPOSITIONED structurally (hostile-pipe design, §3) and procedurally
  (AC-16 mandatory receive-path redteam before UNIT_VALIDATED claims).
- **G-3 (HIGH) India legal exposure** → OPEN, gated: DEC-SAT-0006 blocks field
  TX activation behind legal sign-off + identified authorized channel;
  LEGAL_REVIEW gap carried (not web-resolvable).
- **G-4 (MEDIUM) SBD longevity** (no published EOL; ecosystem steering toward
  IMT/NTN Direct) → MITIGATED by adapter-injected provider seam; v2 migration
  obligation recorded.
- **G-5 (MEDIUM) folklore-grade throughput figures** ("~10 msgs/h") → OPEN
  until EXP benchmark on real modem; defaults flagged provisional; AC-12/15.
- **G-6 (MEDIUM) ≈1 mW jamming defeats link locally** → ACCEPTED + documented:
  satellite positioned as best-effort redundancy, never sole lifeline, never
  guaranteed delivery.
- **G-7 (MEDIUM) replay windows vs multi-hour store-and-forward** → FOLLOW-UP
  filed with CRYPTO-001 owner (monotonic per-sender sequence numbers);
  interim: existing SEC-001 replay gates; tracked to closure post-node.

---

**Next**: IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT
scope = pure-Rust `crates/iris-core/src/transport/satellite.rs` (CostGuardConfig,
SatelliteProvider, SatelliteLinkAdapter + SimulatedSatelliteAdapter conformance,
AtSbdModemAdapter stub [HardwareGated], SbdFrame, CostLedger + InMemoryLedger,
SatelliteCostGuard, SAT_COST, SatelliteTransport impl, OBS metrics) —
platform-neutral, adapters own I/O, no new crate/deps, no other transports
touched; physical modem integration under BLK-0005 + India legal HIGH-RISK
gate (DEC-SAT-0006).
