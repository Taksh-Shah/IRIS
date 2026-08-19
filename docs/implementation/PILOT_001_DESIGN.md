# PILOT-001 Design — Field Pilot Deployment v1

**Document ID**: IRIS-PILOT-001-DESIGN-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS, deps ANDROID-001 / IOS-001 / EMERG-001 /
SEC-001 all COMPLETE; LEGAL-001 RESEARCH_COMPLETE carry)
**Date**: 2026-08-19
**Stages**: DISCOVER (iter ~152) → RESEARCH (iter ~153, RES-0026) → **DESIGN (iter ~154)**
**Research basis**: `RES-0026.md` (2026-08-19: verdict **PROCEED** — RQ-1
ADOPT-WITH-CONDITION managed-flood/gateway topology; RQ-2
ADOPT-WITH-CONDITION QR mutual-scan bootstrap; RQ-3 ADOPT-WITH-CONDITION
OBS-001 ↔ NDMA DMEx 4-phase mapping; RQ-4 ADOPT codification-only WPC
853(E)/2021; RQ-5 ADOPT-WITH-CONDITION Play **internal** track + TestFlight;
RQ-6 ADOPT EMERG drill = NDMA Mock Exercise; **9 DESIGN inputs D-1..D-9**,
external-facts reconciliation, gaps G-P1..G-P9); `engineering/memory/records/PILOT-001_DISCOVER.md`
(UNDERSTAND, iter ~152 — C2 gap CONFIRMED, inherited surface, KPI draft, RQ-1..RQ-6);
`docs/implementation/BLE_002_DESIGN.md` + `ANDROID_DESIGN.md` + `IOS_DESIGN.md`
(AC-1..n + DEC-xxxx node-design pattern); `OBS_DESIGN.md` / `docs/operations/*`
(KPI vocabulary, telemetry opt-in, incident ladder, diagnostics); `EMERG_DESIGN.md` /
`SEC_001_DESIGN.md` / `docs/security/*` (authority root, drill mode, ACL/quota,
sos rate-limits); `docs/business/GO_TO_MARKET.md` + `B2G.md` + `B2B.md` (NDRF 12th
Bn Gandhinagar 50-device/3-month MoU Month 6-9; B2B 5-device 30-day POC);
`docs/legal/*` + `docs/research/RESEARCH_GAPS.md` (LEGAL-001 carry, GAP-004 LoRa).
**Absorbs**: RES-0026 D-1..D-9 (§8) + external-facts corrections (§9) + gaps
G-P1..G-P9 (§10); DISCOVER known-limitations baseline (§5) + DEC-0010 pilot gate.
**Reconciles**: `docs/business/GO_TO_MARKET.md` + `docs/operations/FIELD_OPERATIONS.md`
+ `docs/legal/COMPLIANCE_RISK_REGISTER.md` vs RES-0026 EXTERNAL-FACTS (Play
closed track → **internal**; WPC 865-867 → **G.S.R. 853(E) 2021 865-868**;
FIELD_OPS 866.0 MHz ≤25 mW CONFIRMED). Doc-file edits are a DOCUMENT-stage
follow-up (AC-18).

---

## 1. Scope

PILOT-001 is the **first real-world field deployment** — an OPERATIONS node that
produces the **deployment bundle**: topology, provisioning flow, KPI/measurement
plan, exercise schedule, distribution + ops runbook, and regulatory codification
that a 50-100 device government/NGO pilot (NDRF MoU) is run against. Unlike
code-platform nodes, IMPLEMENT delivers **ops deliverables** (runbooks, plans,
registers) on top of the already-COMPLETE platform + transport + security +
emergency + observability surface. No new Rust/Swift/Kotlin product code is
required by this node; where a config/flag seam is needed (relay-cadence
coefficient), the DESIGN locates it in existing seams.

**Deployment scope (pinned from DISCOVER §3 + RES-0026 §8)**:

- **NDRF MoU pilot** (primary): NDRF 12th Battalion, Gandhinagar — **50 devices,
  3 months (Month 6-9)** under GO_TO_MARKET/B2G MoU.
- **B2B POC tier**: 5 devices + 1 gateway, 30 days.
- **Campus/NGO tier**: community-pilot cohort at Ahmedabad (RESEARCH_GAPS
  pilot city; mobility traces EXP-002/003).
- Milestone anchor: "Pilot: Real-world NGO/campus pilot **100+ devices**".
- **Distribution**: Google Play **internal** track (Android cohort ≤100 testers)
  + TestFlight external (iOS cohort) with 90-day refresh.
- **LoRa excluded** (GAP-004 REQUIRES_HARDWARE) — v1 pilot is BLE + Wi-Fi Aware
  + Wi-Fi Direct + Internet-relay only.

**Deliverables at IMPLEMENT** (all docs; code touched only via existing seams):

1. `docs/operations/PILOT_RUNBOOK.md` — topology, provisioning ceremony,
   relay-cadence config, decommission, day-0/day-N operations, incident
   ownership, diagnostics flow.
2. `docs/operations/PILOT_KPI_PLAN.md` — OBS-001 metric mapping, thresholds,
   Wilson-CI sample plan, battery methodology, collection cadence.
3. `docs/operations/PILOT_EXERCISE.md` — NDMA DMEx schedule (TTEx → ME →
   evaluation), observers/debrief/AAR structure, drill-cert chain + TEST-only
   SOS, safety annex.
4. `docs/operations/PILOT_DISTRIBUTION.md` — Play internal track setup,
   TestFlight 90-day refresh, signing custody, OEM battery-kill matrix,
   telemetry consent, update path.
5. `docs/operations/REG_NOTES.md` — regulatory codification (WPC 853(E)/2021,
   SSMI memo, MoU authorization, STQC timing, open counsel questions).

**Explicitly deferred** (recorded, not blockers): physical-device legs beyond
the pilot itself (BLK-0005 — the pilot IS the device-gated leg); production app
store presence (closed track 12×14 clock starts at pilot close); LoRa/satellite
(P2 hardware nodes LORA-001/SAT-001); STQC certification (GA Month 30).

## 2. Design decisions (RES-0026 D-1..D-9 absorbed)

| # | Decision | Basis (RES-0026) |
|---|----------|-------------------|
| D-1 | **Topology** = 50-100 leaves (Android relay-primary; iOS limited-relay — single known limitation) + **1-3 DESKTOP-001 (Tauri) gateways** + Android-gateway field fallback; private IRIS relay endpoint; **gateway-scoped** uplink/downlink (never per-leaf); mesh-primary offline operation; LoRa excluded. | RQ-1 findings 1-6 (managed-flood >100-node precedent; Helene gateway-uplink; Nepal DTN metrics) |
| D-2 | **Relay-cadence scaling rule** at the INTERNET-001/ROUTE-001 seam: broadcast/telemetry cadence flat ≤40 online nodes, then **linear** above (mirror Meshtastic `congestionScalingCoefficient` — 0.6×≤10 … 1.0×31-40, then `1.0 + (over-40 × factor)`; default 0.075/node). Airtime minimization = DESIGN AC. | RQ-1 finding 3 (firmware `Default.{h,cpp}`, L1/L4) |
| D-3 | **Fixed test battery** (pre-validated in SIM-001 harness): NCT-of-N relay capacity (all-N delivery + latency CDF), partition/healing (uplink down), multi-hop SCF mule-chain with contact-window expectation; BLK-0005 physical-device legs folded into the pilot itself. | RQ-1 finding 5 + DISCOVER §3 |
| D-4 | **ProvisioningFlow** = first-run identity gen (Keystore/Keychain) → operator `Add device` mutual-QR ceremony in **batch sessions of 10-20** → optional gateway-verified escalation → EMERG-001 authority-root + DRILL-chain **out-of-band** provisioning bundle → decommission (wipe + revoke + rotate). No mesh-mediated first trust; **no-recovery documented** as deliberate v1 property. | RQ-2 findings 1-5 (Briar/SecureJoin QR; IDENT-001 two-tier) |
| D-5 | **Trust model** = TOFU first-trust + verified tier (RES-0016 D-2/DEC-P0006); key isolation in platform stores (Android TEE Keystore RES-0022 / iOS Keychain RES-0025); QR bootstrap per Briar/SecureJoin; app-lock carry. | RQ-2 findings 3/5 |
| D-6 | **KPI plan** = OBS-001 mapping: delivery ratio (per class/transport), latency p50/p95/p99, hops (`msg.delivered`), battery drain %/h (controlled foreground/background test at fixed cadence), coverage (neighbors.active, partition.detected), drill success (EMERG-001); thresholds **target/floor** + **Wilson-CI sample plan** (50-100 devices) as AC-1..n. | RQ-3 findings 4/5/6 |
| D-7 | **Exercise schedule** = Month 1 TTEx (injects) → Month 2 field **Mock Exercise** (observers + self-assessment) → Month 3 evaluation (debrief + NDRF report); EXP-002/003/005 as before/after legs; ≥1 drill/year cadence per NDMP 2019. | RQ-3 findings 1-3/7; RQ-6 finding 1-3 |
| D-8 | **Distribution + ops runbook** = Play **internal** track (Android ≤100 testers, no review, near-instant); **closed** track (12 testers × 14 days) starts the production clock at pilot close; TestFlight external (iOS) with **90-day refresh**; Play App Signing upload-key custody (2-person + PEPK backup); OEM battery-kill checklist; telemetry opt-in + bounded retention; incident severity ownership; diagnostics QR export. | RQ-5 findings 1-5 |
| D-9 | **REG-NOTES codification** = NDRF MoU (pilot authorization) + STQC at GA (Month 30); config ≤25 mW e.r.p. (866.0 MHz, LoRa future-leg only); SSMI non-applicability memo at pilot scale; open counsel questions (relay-as-telegraph §4 ITA, intermediary classification, DPDPA consent, LoRa ETA) — **no research legal opinion**; lawyer + operator review gate per LEGAL-001. | RQ-4 findings 1-6 |

## 3. Pilot topology + gateway architecture (D-1)

```
            ┌─────────────────────────── private IRIS relay (operator-hosted) ───────────────────────────┐
            │   IRIS relay endpoint  (TLS) — telemetry sink + cross-mesh bridge (gateway-scoped only)      │
            └───────────────────────────────────▲───────────────────────▲──────────────────────────────────┘
                                                │ uplink/downlink       │ uplink/downlink
                                   ┌────────────┴─────────────┐  ┌─────────┴──────────────┐
                                   │ DESKTOP-001 GATEWAY 1-3  │  │ Android-gateway fallback│
                                   │ (Tauri host, iris-core)  │  │ (field-deployable, ≤2)  │
                                   └────────────▲─────────────┘  └─────────▲──────────────┘
                                                │  BLE + Wi-Fi Aware + Wi-Fi Direct mesh
                 ┌──────────────────────────────┴──────────────────────────────┐
                 │                     IRIS MESH (primary)                      │
                 │  leaves 50-100: Android relay-primary (BLE/WA/WD),           │
                 │  iOS limited-relay (BLE, foreground + restoration)           │
                 └──────────────────────────────────────────────────────────────┘
```

- **Leaves (50-100)**: Android devices run full relay (BLE + Wi-Fi Aware +
  Wi-Fi Direct adapters COMPLETE); iOS devices run limited relay (BLE,
  foreground + `willRestoreState` re-arm, SessionRecovery — IOS-001 COMPLETE).
- **Gateways (1-3)**: DESKTOP-001 (Tauri) hosting `iris-core` (DesktopEngine),
  each an Internet-relay uplink to the **private IRIS relay endpoint**
  (gateway-scoped only — never per-leaf; DEC-AND/INTERNET-001 seam). Android
  gateway is a **field fallback** role (an Android leaf promoted with
  Internet-relay config) when no desktop is co-located.
- **Mesh-primary**: all messaging operates offline-first in the mesh; Internet
  relay is an optimization/uplink path, never the availability base
  (architect invariant: emergency P0/P1 path never depends on internet).
- **Offline/mesh-primary test design**: the fixed test battery (D-3, §5)
  exercises mesh-only delivery with the relay endpoint down (partition/healing).
- **Known limitation (single)**: iOS background relay asymmetry
  (RES-0024/RES-0025) — backgrounded iOS leaves are hidden/limited; foreground
  Android/Linux/Windows gateways carry relay-heavy work.

## 4. Relay-cadence scaling rule (D-2) — INTERNET-001/ROUTE-001 seam

Mirror Meshtastic `congestionScalingCoefficient` (verified at firmware source,
RES-0026 RQ-1 f.3). IRIS relay broadcast/telemetry cadence scales with
**online-node count** `N`:

| N (online) | Multiplier | Effect |
|-----------|-----------|--------|
| ≤10 | 0.6× | aggressive — sparse network, maximize discovery |
| 11-20 | 0.7× | early growth |
| 21-30 | 0.8× | growth |
| 31-40 | 1.0× | nominal |
| >40 | `1.0 + (N-40) × f` | linear backoff; default `f = 0.075/node`, configurable 0.01-0.1 |

- Applied to: telemetry heartbeat broadcast cadence, neighbor-discovery
  broadcast cadence, and non-critical gossip at the ROUTE-001 / INTERNET-001
  seam; **never** to P0/P1 emergency traffic (EMERG-001 priority bypass) nor to
  point-to-point directed messages (only broadcast/telemetry).
- Configuration seam: existing `iris-core` routing/telemetry config (OBS-001
  cadence) — a named `relay_cadence` policy parameter with the table above as
  the default; DOCUMENT-stage edit records the exact config key.
- **Airtime-minimization discipline** = DESIGN AC (AC-6): measured via OBS-001
  transport/queue diagnostics (bytes-persec, neighbors.active).

## 5. Fixed test battery (D-3) — pre-validated in SIM-001 harness

| ID | Scenario | Objective | Success criterion (KPI AC) |
|----|----------|-----------|----------------------------|
| B-1 | **NCT-of-N relay capacity** | all-N delivery + latency CDF at N = 20/50/100 synthetic nodes | all-N delivery ratio + per-class latency thresholds (AC-7) |
| B-2 | **Partition/healing** | uplink (Internet-relay endpoint) down for T minutes → mesh continues; heal on restore | P1/P2 delivery ratio floor during partition; resync no-duplicates (AC-8) |
| B-3 | **Multi-hop SCF mule-chain** | chained store-carry-forward legs with contact-window expectation | mule-chain delivery + hop-count CDF within window (AC-9) |
| B-4 | **iOS limited-relay** | backgrounded iOS leaf participates when foregrounded + restoration | limited-relay delivery ≥ floor; asymmetry recorded (AC-10) |

- Pre-validated in the SIM-001 network-simulator harness (existing corpus +
  failure-injection patterns, `docs/simulation/*`).
- BLK-0005 physical-device legs are **folded into the pilot itself** — the
  pilot field legs (TTEx/ME) re-run B-1..B-4 on real hardware and feed the
  KPI evaluation (AC-7..AC-10 evidence rows may cite sim pre-validation +
  field legs; physical rows recorded BLK-0005-gated).

## 6. ProvisioningFlow + trust model (D-4, D-5)

**ProvisioningFlow** (operator-run ceremony, batch sessions of 10-20 devices):

1. **First run** — leaf generates identity on-device: Android TEE Keystore
   Ed25519 (RES-0022) / iOS Keychain CryptoKit Ed25519 (RES-0025); `sender_id`
   = first 16 B of SHA-256(pubkey) (RES-0016 D2/DEC-P0006).
2. **Operator `Add device`** — mutual-QR ceremony (Briar/SecureJoin pattern):
   operator device displays a session QR; leaf scans (operator pubkey +
   nonce); leaf displays its own QR; operator scans (leaf pubkey); both sides
   verify short authentication string in person → TOFU flag set, operator-side
   verified-tier escalation.
3. **Gateway-verified escalation** — co-located gateway (or operator device)
   re-confirms identity in-person → `verified` trust tier (IDENT-001
   trust_store, TrustLevel).
4. **EMERG-001 authority-root + DRILL-chain provisioning** — operator pushes
   the authority-root bundle + DRILL certificate chain **out-of-band**
   (batch bundle via provisioning session, not mesh first-contact).
5. **Decommission** — wipe platform key (Keystore/Keychain delete) + TrustStore
   revoke + key rotation (IDENT-001 rotate.rs); device retired from node
   registry; no-recovery documented (no escrow in v1).

**Trust model (D-5)**: TOFU first-trust + verified tier; no mesh-mediated
first trust; QR bootstrap per Briar/SecureJoin precedent; key isolation in
platform stores; app-lock carry; DEC-0010 gate — pilot devices MUST run
provisioned identities + AEAD (no DevCryptoProvider / no identity / no
sealer deployments).

## 7. KPI + measurement plan (D-6) — OBS-001 mapping + Wilson-CI

**KPI set** (OBS-001 prefixed events, 7-day retention, telemetry opt-in):

| KPI | OBS-001 source | Target | Floor |
|-----|----------------|--------|-------|
| Delivery ratio — P2P / per transport | `msg.delivered` + transport counters | 0.95 (P2P), 0.90 (multi-hop) | 0.90 / 0.85 |
| Latency p50/p95/p99 | `msg.delivered` latency_ms | 2 s / 10 s / 30 s | 5 s / 30 s / 60 s |
| Hop count p50/p95 | `msg.delivered` hops | 2 / 5 | 3 / 8 |
| Battery drain %/h | controlled fg/bg test at fixed cadence | ≤3 %/h (fg), ≤1 %/h (bg) | ≤5 %/h, ≤2 %/h |
| Coverage | `neighbors.active`, `partition.detected` | active-neighbor floor per zone | no coverage holes > 500 m |
| Relay throughput | Wi-Fi Aware/Direct + BLE counters | per `docs/performance/*` budgets | 80% of budget |
| Drill success | EMERG-001 drill metrics | TTEx/ME objectives met | debrief + AAR completed |
| SOS delivery (drill, TEST-only) | EMERG-001 SOS path | P0 delivery ratio 1.0 in drill | 0.95 |

**Wilson-CI sample plan**: for 50-100 devices, count/ratio KPIs are reported
with a **Wilson score interval** at 95% confidence (small-N, non-normal):
p̂_wilson = p̂ ± z·√(p̂(1-p̂)/n) adjusted for n; AC-11 records the formula,
the n-per-leg sampling, and reporting format (per class/transport, not pooled).

**Battery methodology**: controlled per-device foreground/background drain test
at fixed telemetry cadence (not field-only); grounded in RES-0022/RES-0025
battery discipline; physical rows BLK-0005-gated into the pilot.

**Collection cadence**: telemetry flush at relay-cadence-multiplied interval
(§4) for broadcast/telemetry; directed metrics at `msg.delivered`; 7-day
bounded retention; opt-in consent (TELEMETRY.md).

## 8. Exercise schedule (D-7) — NDMA DMEx structure

| Month | Activity | NDMA mapping (RES-0026 RQ-3/RQ-6) | OBS-001 capture |
|-------|----------|-----------------------------------|-----------------|
| M1 | **TTEx** — tabletop exercise w/ scenario injects (NDRF 12th Bn), ESF/SOP testing | DMEx discussion-based Tabletop Exercise + Orientation-cum-Coordination Conference | dry-run metric capture, observers briefed |
| M2 | **Field Mock Exercise (50 devices)** — real deployments, observers w/ NDMA observation format, self-assessment forms, TEST-only SOS/broadcast under DRILL certs, yellow-banner UX | DMEx action-based Mock Exercise (Suraksha Chakra pattern) | full OBS-001 capture incl. drill-success + SOS-in-TEST; before/after legs EXP-002/003/005 |
| M3 | **Evaluation** — debrief, good-practices/gaps/weaknesses report, **AAR/IP to NDRF** (HSEEP-portable template) | DMEx Phase-4 evaluation/debrief | KPI evaluation vs Wilson-CI thresholds; report artifact |

- ≥1 drill/year cadence per NDMP 2019 (carry).
- **EXP legs**: EXP-002/003 (Ahmedabad mobility traces) and EXP-005 (iOS bg-BLE)
  run as before/after sub-experiments inside M1-M3.
- **Safety annex** (DRIS posture): communications-only drill, no live-hazard
  simulation, no movement of people/machinery beyond normal operations,
  observer oversight, public-notice window to avoid confusion with real
  emergencies; DRILL certificate chain ensures real-SOS capacity is never
  consumed by the drill (SEC-001 quotas + EMERG-001 drill suppression).

## 9. Distribution + ops runbook (D-8)

- **Android cohort**: Google Play **internal** track — up to 100 testers/app,
  no review, near-instant builds; tester list = pilot device emails; can run
  before full store setup. **Closed** track (12 testers × 14 days) is the
  production-access prerequisite — start that clock intentionally at pilot
  close (production path, not pilot).
- **iOS cohort**: TestFlight external — up to 10,000 testers; builds expire
  **90 days** → calendar-owned rolling refresh; first external build requires
  Beta App Review.
- **Signing**: Play App Signing — upload key RSA ≥2048 in `.jks`, dev custody
  **2-person rule** + PEPK backup; Google manages the app-signing key.
- **OEM battery-kill matrix** (ANDROID-001 carry) at provisioning; iOS
  SessionRecovery/BGTask discipline (IOS-001 carry).
- **Telemetry**: opt-in consent slide in the M1 briefing; bounded 7-day
  retention; diagnostics QR export (DIAGNOSTICS.md).
- **Incident**: severity ladder + ownership table (INCIDENT_MANAGEMENT.md);
  relay-operator on-call defined in runbook.
- **Update path**: internal-track builds push via Play console; iOS via
  TestFlight refresh; app auto-update accepted on pilot cohort only.

## 10. REG-NOTES codification (D-9)

Codified in `docs/operations/REG_NOTES.md` at IMPLEMENT:

1. **WPC G.S.R. 853(E) 2021** = governing SRD band 865-868 MHz (supersedes
   LEGAL-001's 865-867 2005 RFID carry); Table-I Non-Specific SRD: 25 mW e.r.p.,
   duty cycle ≤1%, FHSS ≥58 hop channels ≤50 kHz, EN 300 220. IRIS field config
   (866.0 MHz, ≤25 mW e.r.p. — FIELD_OPS) fits Table-I; **LoRa future-leg only**
   (v1 pilot = BLE/Wi-Fi, unlicensed — no RF-equipment trigger).
2. **IT Rules 2021 SSMI**: 50-lakh registered-user threshold is orders above a
   50-100-device pilot → **non-applicability memo** at pilot scale (re-confirm
   gazette S.O. 942(E) text at counsel review — G-P6).
3. **MoU authorization**: NDRF 12th Bn Gandhinagar MoU (50-device/3-month) is
   the pilot authorization mechanism; STQC certification planned at GA
   (Month 30) — pilot proceeds under MoU terms (LEGAL-001 carry).
4. **Open counsel questions** (tracked, no research legal opinion): relay-as-
   telegraph (§4 ITA 1885), intermediary classification under IT Rules, DPDPA
   fiduciary/consent for pilot operators, LoRa ETA/type-approval path (GAP-004
   future). Lawyer + operator review gate per LEGAL-001.

## 11. Acceptance criteria (AC-1..AC-18 — C2 gap RESOLVED, pattern BLE-002/ANDROID-001/IOS-001)

| AC | Criterion | Evidence / verifier check |
|----|-----------|---------------------------|
| AC-1 | **Design doc v1.0** exists with D-1..D-9, topology (§3), relay-cadence rule (§4), test battery (§5), ProvisioningFlow (§6), KPI plan + Wilson-CI (§7), exercise schedule (§8), distribution runbook (§9), REG-NOTES (§10) | This document; REVIEWED at DESIGN |
| AC-2 | **PILOT_RUNBOOK.md** — topology, gateway roles/fallback, ProvisioningFlow ceremony (batch 10-20, mutual-QR, verified escalation, authority-root/DRILL bundle), decommission (wipe+revoke+rotate), day-0/day-N ops, incident ownership, diagnostics QR | Runbook file; cross-check vs §6/§9 |
| AC-3 | **PILOT_KPI_PLAN.md** — OBS-001 metric mapping, thresholds target/floor table, **Wilson-CI** formula + sampling, battery methodology (controlled fg/bg at fixed cadence), collection cadence + 7-day retention | KPI plan file; formula spot-check |
| AC-4 | **PILOT_EXERCISE.md** — NDMA DMEx schedule (M1 TTEx → M2 field ME → M3 evaluation/AAR), observers + self-assessment, drill-cert chain + TEST-only SOS, safety annex (communications-only, public-notice window) | Exercise file; vs §8 |
| AC-5 | **PILOT_DISTRIBUTION.md** — Play **internal** track (≤100 testers) + closed 12×14 production clock, TestFlight external + 90-day refresh, Play App Signing custody (2-person + PEPK), OEM battery-kill matrix, telemetry consent, update path | Distribution file; vs §9 |
| AC-6 | **Relay-cadence rule implemented/documented**: broadcast/telemetry cadence scales with online-node count (≤40 flat table, >40 linear `1.0 + (N-40)×f`, default f=0.075; configurable 0.01-0.1); P0/P1 + directed messages exempt; airtime-minimization discipline recorded | Config seam edit + runbook table; code/config diff (OBS-001 cadence seam) |
| AC-7 | **B-1 NCT-of-N**: all-N delivery + latency CDF at N=20/50/100 (sim pre-validation + field legs); delivery + latency within §7 thresholds | SIM-001 run log + field leg capture |
| AC-8 | **B-2 partition/healing**: relay endpoint down T minutes → mesh P1/P2 delivery floor holds; heal with no duplicates | Sim + field leg evidence |
| AC-9 | **B-3 multi-hop SCF mule-chain**: delivery + hop CDF within contact window | Sim + field leg evidence |
| AC-10 | **B-4 iOS limited-relay**: backgrounded iOS participates on foreground/restore; asymmetry recorded as known_limitation | iOS leg evidence (BLK-0005-gated physical rows recorded) |
| AC-11 | **KPI sample plan**: Wilson-CI formula + per-leg sampling + report format committed; thresholds target/floor in KPI plan | Formula + report template in AC-3 file |
| AC-12 | **ProvisioningFlow verified**: mutual-QR ceremony, verified escalation, authority-root/DRILL out-of-band bundle, decommission (wipe+revoke+rotate) each have a runbook step + test/ rehearsal record | Runbook + rehearsal log |
| AC-13 | **EMERG drill integration**: TEST-only SOS under DRILL certs never consumes real-SOS quota (SEC-001 + EMERG-001 drill suppression); yellow-banner UX; observers + debrief | Exercise file + dry-run log |
| AC-14 | **REG-NOTES.md** codified: WPC 853(E)/2021 (865-868, ≤25 mW e.r.p. 866.0 LoRa-leg-only), SSMI non-applicability memo, MoU authorization + STQC GA timing, open counsel questions; no research legal opinion stated | REG_NOTES file; vs §10 |
| AC-15 | **Distribution runbook complete**: internal track + TestFlight refresh + signing custody (2-person + PEPK) all have owners + calendar-owned tasks | Distribution file + task ownership table |
| AC-16 | **Doc reconciliation**: RES-0026 EXTERNAL-FACTS applied — GO_TO_MARKET/B2G "closed track" → **internal** (G2P ref); COMPLIANCE_RISK_REGISTER/SPECTRUM "865-867" → **853(E) 2021 865-868**; FIELD_OPS 866.0 MHz ≤25 mW CONFIRMED note | Diff review vs RES-0026 §9 (pattern IOS AC-17) |
| AC-17 | Workspace green held: `cargo test --workspace --all-features` ≥ **658/0/1**; clippy 0; rustfmt clean (no Rust-core regressions; docs-only pass) | CI re-run at TEST |
| AC-18 | **SECURITY_REVIEW → VERIFY next**: adversarial review of provisioning + decommission + authority-root bundle + TEST-only SOS + relay-cadence config; then independent verifier doc (pattern EMERG/SEC/BLE) | Stage-transition evidence |

**Carry-forward mapping (every RES-0026 design input lands on ≥1 AC)**: D-1 →
AC-2/AC-7..10; D-2 → AC-6; D-3 → AC-7..10; D-4 → AC-2/AC-12; D-5 → AC-12;
D-6 → AC-3/AC-11; D-7 → AC-4/AC-13; D-8 → AC-5/AC-15; D-9 → AC-14/AC-16.

## 12. Declared decisions (DEC-PILOT-0001..0009 — ratified in DECISIONS.md)

- **DEC-PILOT-0001** Pilot topology: 50-100 leaves (Android relay-primary; iOS
  limited-relay) + 1-3 DESKTOP-001 gateways + Android-gateway field fallback;
  gateway-scoped Internet-relay to a private IRIS relay endpoint; mesh-primary
  offline operation (RES-0026 RQ-1).
- **DEC-PILOT-0002** Relay-cadence scaling rule mirrors Meshtastic
  `congestionScalingCoefficient` (≤40 flat, >40 linear, default f=0.075);
  applied to broadcast/telemetry only; P0/P1 + directed exempt (RES-0026 RQ-1
  f.3).
- **DEC-PILOT-0003** Field identity bootstrap = in-person mutual-QR ceremony
  (Briar/SecureJoin precedent) with batch sessions of 10-20; no mesh-mediated
  first trust; no-recovery is a deliberate v1 property (RES-0026 RQ-2).
- **DEC-PILOT-0004** EMERG-001 authority-root + DRILL certificate chain are
  provisioned **out-of-band** at provisioning; drill SOS/broadcast runs
  TEST-only under DRILL certs (never consumes real-SOS quota).
- **DEC-PILOT-0005** KPI reporting uses **Wilson-CI** sample plan at 95% for
  count/ratio KPIs; thresholds target/floor are AC rows (RES-0026 RQ-3).
- **DEC-PILOT-0006** Distribution: Google Play **internal** track for the v1
  Android cohort (≤100 testers); closed 12×14 starts the production clock at
  pilot close; iOS via TestFlight external with 90-day refresh (RES-0026 RQ-5).
- **DEC-PILOT-0007** Signing custody: Play App Signing upload key RSA ≥2048,
  2-person rule + PEPK backup; Google-managed signing key.
- **DEC-PILOT-0008** Exercise = NDMA DMEx structure (M1 TTEx → M2 field ME →
  M3 evaluation/AAR to NDRF); communications-only safety annex with public
  notice window (RES-0026 RQ-6).
- **DEC-PILOT-0009** REG-NOTES codification: WPC G.S.R. 853(E) 2021 (865-868)
  supersedes 865-867 carry; SSMI non-applicable at pilot scale; NDRF MoU is
  the authorization; open counsel questions tracked — **no research legal
  opinion** (RES-0026 RQ-4; LEGAL-001 carry).

## 13. Risks / gaps / known limitations (G-P1..G-P9 mapping — recorded, not blockers)

| # | Risk / gap (RES-0026) | Design position |
|---|------------------------|-----------------|
| G1 (G-P1) | DTN benchmark numbers not extracted (arXiv 2603.10153 full-text) | KPI thresholds stand on project budgets + field pilot data; DTN figures are reference only (extract at M3 eval if needed) |
| G2 (G-P2) | Meshtastic ">100 nodes" is qualitative precedent | Treat as precedent not proof — pilot generates IRIS PDR data (AC-7) |
| G3 (G-P3) | Play internal-track cap (100) + closed-track requirements to re-verify for IRIS account type | AC-15 records re-verification at DISTRIBUTION step |
| G4 (G-P4) | NDRF Mock Exercise SOP full text not pulled | Pull NDRF SOP at M1 for the MoU annex (AC-4) |
| G5 (G-P5) | Suraksha Chakra quantitative outputs not public | NDRF MoU report owns that data (M3 deliverable) |
| G6 (G-P6) | IT Rules SSMI threshold via press (L3); gazette S.O. 942(E) text to re-confirm | Counsel re-confirmation recorded in REG-NOTES open questions (AC-14) |
| G7 (G-P7) | BQP/short-auth-string spec detail L5-only | Cite project spec; physical-device verification BLK-0005-gated (AC-12) |
| G8 (G-P8) | Thresholds/sample-plan/exercise packaging are DESIGN decisions | Pinned here as AC-3/AC-11/AC-4 — done |
| G9 (G-P9) | Helene vendor message-count figures unverified | Qualitative precedent only (section §3) |
| G10 | Physical-device legs (on-air BLE/battery, iOS bg, live ME) — BLK-0005 | The pilot itself is the device-gated leg; sim pre-validation + field legs; recorded known_limitations |
| G11 | DEC-0010 pilot gate (no DevCryptoProvider/no-identity/no-sealer) | Provisioned identities + AEAD on ALL pilot devices (AC-12); platform nodes satisfy |
| G12 | LEGAL-001 carry: SSMI/relay/intermediary/DPDPA/LoRa-ETA open Qs | REG-NOTES codification + counsel review gate (AC-14) |

## 14. Test strategy

1. **Design-level evidence** (AC-1..AC-6, AC-11, AC-14..AC-16): file reviews,
   config-diff reviews, runbook rehearsal records (provisioning ceremony,
   decommission dry-run).
2. **Sim pre-validation** (AC-7..AC-10): SIM-001 harness runs B-1..B-4 with
   existing failure-injection corpus; delivery + latency CDF evidence.
3. **Field legs** (AC-7..AC-10, AC-13): M1 TTEx + M2 field ME re-run B-1..B-4
   and the TEST-only SOS drill on real hardware; observers + self-assessment;
   BLK-0005-gated rows recorded as known_limitations, not blockers.
4. **Workspace health** (AC-17): `cargo test --workspace --all-features`
   ≥ 658/0/1, clippy 0, rustfmt clean — docs-only pass must not regress the
   code baseline.
5. **AC-18 SECURITY_REVIEW** then **VERIFY** (independent verifier evidence
   table, pattern EMERG-001/SEC-001/BLE-001) before ACCEPT.

## 15. Doc corrections applied (RES-0026 §9 EXTERNAL-FACTS → file:line old→new)

Version acceptance: **PILOT_001_DESIGN.md v1.0 accepts all 5 RES-0026
EXTERNAL-FACTS corrections** (statuses: 1 REFINE, 1 SUPERSEDED, 1 CONFIRMED,
1 CONFIRMED+extended, 1 CONFIRMED). Actual doc-file edits deferred to DOCUMENT
stage (AC-16).

| # | File | Claim (old) | Correction (new) |
|---|------|-------------|-------------------|
| 1 | `docs/business/GO_TO_MARKET.md` / `B2G.md` | Distribution "Google Play **closed track**" | **REFINE** → **internal track** for the v1 pilot cohort (≤100 testers, no review; answer/9845334); closed track (≥12 × 14 days) is the later production-access prerequisite (answer/14151465) |
| 2 | `docs/legal/COMPLIANCE_RISK_REGISTER.md` / `SPECTRUM_CONSIDERATIONS.md` | "WPC de-licensing **865-867 MHz**" | **SUPERSEDED** → **G.S.R. 853(E) 2021 = 865-868 MHz SRD** (25 mW e.r.p., duty ≤1% Table-I), superseding 2005 RFID 865-867; TEC ER Annexure-G5 LoRa 865-867 to re-confirm for LoRa leg (GAP-004) |
| 3 | `docs/operations/FIELD_OPERATIONS.md` | config "866.0 MHz, ≤25 mW" | **CONFIRMED** → fits G.S.R. 853(E) Table-I Non-Specific SRD (L1-gazette-confirmed) |
| 4 | DISCOVER §3 KPI draft | "delivery ratio, per-class latency, battery, coverage, SOS success, relay throughput" | **CONFIRMED + extended** → add hops (OBS-001 `msg.delivered`), p50/p95/p99, Wilson-CI sample plan, controlled battery methodology, drill-success metrics |
| 5 | DISCOVER §4 RQ-6 "safety plan" | safety plan | **CONFIRMED** → embodied as exercise safety annex: communications-only, no live-hazard simulation, observers, public-notice window |

## 16. Next

IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT. IMPLEMENT scope (docs-
only): `docs/operations/PILOT_RUNBOOK.md`, `PILOT_KPI_PLAN.md`,
`PILOT_EXERCISE.md`, `PILOT_DISTRIBUTION.md`, `REG_NOTES.md` (AC-2..AC-5,
AC-14); relay-cadence config seam edit (AC-6); EXTERNAL-FACTS doc edits (AC-16).
Sim pre-validation of B-1..B-4 in SIM-001 harness (AC-7..AC-10) at TEST. Physical
device + live-exercise rows remain BLK-0005-gated / pilot-field-gated
(AC-13/AC-18). After ACCEPT: NODE_TRANSITION → LORA-001/SAT-001 (P2 hardware)
→ mission milestones. Baseline **658/0/1** clippy 0 fmt clean held (docs-only).
