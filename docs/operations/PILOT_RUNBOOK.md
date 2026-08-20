# PILOT Runbook — Deployment and Operations (PILOT-001)

**Document ID**: IRIS-PILOT-RUNBOOK-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS — DESIGN COMPLETE iter ~154; IMPLEMENT iter ~155)
**Date**: 2026-08-19
**Status**: ACCEPTANCE TARGET — PILOT_001_DESIGN.md AC-2
**Authoritative inputs**: PILOT_001_DESIGN.md (D-1..D-9, §3-§10); RES-0026
(RQ-1..RQ-6); PILOT-001_DISCOVER.md §3 (scope surface); platform transports
BLE-001/002 + WIFIAWARE-001 + WIFIDIRECT-001 + INTERNET-001; EMERG-001
(authority root + drill); SEC-001 (ACL/quotas); OBS-001; IDENT-001
(trust_store/rotate).

---

## 1. Pilot topology (D-1 — pinned)

```
            ┌──────────── private IRIS relay endpoint (operator-hosted, TLS) ────────────┐
            │  telemetry sink + cross-mesh bridge — GATEWAY-SCOPED uplink/downlink only   │
            └───────────────────────────────▲──────────────────────────▲─────────────────┘
                                            │                          │
                       ┌────────────────────┴───────┐   ┌──────────────┴────────────┐
                       │ DESKTOP-001 GATEWAYS (1-3) │   │ Android-gateway FALLBACK  │
                       │ Tauri host + iris-core     │   │ (≤2, field-deployable)    │
                       └────────────────────▲───────┘   └──────────────▲────────────┘
                                            │                          │
                          ┌─────────────────┴──────────────────────────┴────────────────┐
                          │                       IRIS MESH (primary)                   │
                          │  50-100 leaves: Android relay-primary (BLE/WA/WD) +         │
                          │  iOS limited-relay (BLE foreground + restoration)          │
                          └─────────────────────────────────────────────────────────────┘
```

- **Leaves 50-100**: Android devices run full relay (BLE + Wi-Fi Aware +
  Wi-Fi Direct — ANDROID-001 COMPLETE). iOS devices run limited relay (BLE,
  foreground + `willRestoreState` re-arm; IOS-001 COMPLETE). iOS background
  asymmetry = single known limitation.
- **Gateways 1-3**: DESKTOP-001 (Tauri) hosting `iris-core` DesktopEngine, each
  terminating the Internet-relay uplink to the private IRIS relay endpoint.
  Android-gateway = field fallback role (leaf promoted with Internet-relay
  config) when no desktop is co-located.
- **Internet relay is gateway-scoped only** — never per-leaf. Mesh-primary
  offline-first: all messaging works with the relay endpoint fully down.
- **LoRa excluded** (GAP-004 REQUIRES_HARDWARE, P2 LORA-001). v1 pilot =
  BLE + Wi-Fi Aware + Wi-Fi Direct + Internet-relay. No RF-equipment trigger.

## 2. ProvisioningFlow (D-4 — operator ceremony, batch 10-20)

### 2.1 First run (leaf)
1. Install pilot build (see PILOT_DISTRIBUTION.md §2). First launch generates
   identity on-device: **Android** = TEE Keystore Ed25519 (RES-0022);
   **iOS** = Keychain CryptoKit Ed25519 (RES-0025). `sender_id` = first 16 B of
   SHA-256(pubkey) (RES-0016 D2/DEC-P0006).
2. **DEC-0010 pilot gate**: pilot devices MUST run a provisioned identity +
   AEAD everywhere. Reject any device in DevCryptoProvider / identity-less /
   sealer-less mode — it is NOT allowed into the pilot mesh.

### 2.2 Operator `Add device` — mutual-QR ceremony (Briar/SecureJoin pattern)
1. Operator opens `Add device` on the operator device → operator session QR
   (operator pubkey + nonce) on screen.
2. Leaf scans operator QR → leaf accepts → leaf displays its own QR (leaf
   pubkey).
3. Operator scans leaf QR → both sides show a **short authentication string**:
   confirm in person (match the string on both screens) → TOFU flag set on
   leaf; operator-side verified-tier escalation (IDENT-001 TrustLevel
   `verified`).
4. Record each pair in the **node registry** (device UID, peer_id short, batch
   session id, operator id, timestamp). No mesh-mediated first trust — ever.

### 2.3 Gateway-verified escalation (optional)
Co-located gateway (or operator device) re-confirms identity in person →
`verified` tier applied gateway-side. Used for gateway-managed leaves.

### 2.4 EMERG-001 authority-root + DRILL-chain provisioning (out-of-band)
- Operator pushes the **authority-root bundle** + **DRILL certificate chain**
  to every pilot device **out-of-band** as part of the batch session (never via
  mesh first-contact).
- Devices confirm authority-root fingerprint at provisioning; mismatch = abort
  session + device quarantined away from pilot.

### 2.5 Decommission
1. **Wipe**: delete platform key (Keystore/Keychain destroy) on-device.
2. **Revoke**: operator marks peer_id in the TrustStore revoke list (IDENT-001
   rotate.rs; earliest-seen-wins; no recovery).
3. **Retire/rotate**: remove device from node registry + relay allowlist
   (SEC-001 ACL); if the key rotates elsewhere, re-run the ceremony.
- **No-recovery is a deliberate v1 property** (no escrow). Documented to every
  operator: a wiped device cannot restore its identity.

## 3. Relay-cadence scaling rule (D-2 — AC-6, INTERNET-001/ROUTE-001 seam)

Named policy parameter: **`relay_cadence`** (default table below), applied to
broadcast/telemetry cadence as a function of **online-node count `N`**
(mirror of Meshtastic `congestionScalingCoefficient`, verified at firmware
source — RES-0026 RQ-1 f.3):

| N (online) | Cadence multiplier | Effect |
|-----------|--------------------|--------|
| ≤10 | 0.6× | aggressive — maximize discovery, sparse network |
| 11-20 | 0.7× | early growth |
| 21-30 | 0.8× | growth |
| 31-40 | 1.0× | nominal |
| >40 | `1.0 + (N-40) × f` | linear backoff; default `f = 0.075`, configurable 0.01-0.1 |

- **Applies to**: telemetry heartbeat broadcast, neighbor-discovery broadcast,
  non-critical gossip — at the ROUTE-001 / INTERNET-001 seam.
- **NEVER applied**: P0/P1 emergency traffic (EMERG-001 priority bypass) nor
  directed point-to-point messages.
- **Airtime-minimization discipline**: monitor OBS-001 transport/queue
  diagnostics (bytes/sec, `neighbors.active`); any sustained duty-cycle
  overload is a relief action (raise multiplier, segment the zone, add a
  gateway) — see §6 In-flight control.
- Example: 60 online nodes → base 30-min telemetry interval ×
  `1.0 + 20×0.075 = 2.5` → 75 min.

## 4. Day-0 (pilot entry)

1. Batch provision all leaves + gateways (§2), record registry.
2. Configure the private IRIS relay endpoint on gateways only (§1).
3. Deploy distribution builds (PILOT_DISTRIBUTION.md §2) — Play internal track
   Android + TestFlight iOS.
4. Distribute OEM battery-kill checklist + install "IRIS pilot" exception
   (PILOT_DISTRIBUTION.md §5).
5. Telemetry opt-in consent capture (PILOT_KPI_PLAN.md §4).
6. Calibrate `relay_cadence` baseline per §3 table at current N.
7. Pre-drill connectivity map (Ahmedabad pilot city zones).

## 5. Day-N operations

- **Daily**: gateway health check (uplink OK, telemetry flushing); mesh
  coverage scan (`neighbors.active`, partitions); battery cohort check.
- **Weekly**: node-registry drift review; incident queue review
  (INCIDENT_MANAGEMENT.md severity ladder); diagnostics bundle review
  (DIAGNOSTICS.md QR export) for outliers; `relay_cadence` re-calibration at
  new N.
- **Drill cadence**: ≥1 drill/year per NDMP 2019 — see PILOT_EXERCISE.md
  (M1/M2/M3 schedule).
- **Exp legs**: EXP-002/003 (Ahmedabad mobility traces) + EXP-005 (iOS bg)
  run as before/after sub-experiments inside the pilot timeline.

## 6. In-flight control / incident operations

- **Command**: relay operator owns the private relay endpoint; team lead owns
  field ops. Severity ownership table:
  | Severity | Example | Owner |
  |----------|---------|-------|
  | S0 (CRITICAL) | Pilot-wide delivery collapse; SOS path degraded | Relay operator + team lead; STOP field ops |
  | S1 (HIGH) | Gateway uplink down >1 h; cluster partition | Relay operator |
  | S2 (MEDIUM) | Telemetry gap; single-device loop | NOC/telemetry staff |
  | S3 (LOW) | Cosmetics; diagnostic drift | On-call tech |
- **Partition/healing**: relay endpoint down → mesh continues (B-2 test
  battery); on restore → gateway re-syncs, no duplicate delivery
  (dedup + high-water marks). Keep the fold-back ≤ target per KPI plan §Table 1.
- **Relief actions**: raise cadence multiplier; split zone (add gateway);
  move to Android-gateway fallback; toggle diagnostic QR collection.
- **Decommission mid-pilot**: §2.5 (wipe + revoke + retire) at any time;
  record reason + battery state in registry.

## 7. Diagnostics

- On-device: OBS-001 metrics flushed at cadence; 7-day bounded retention
  (TELEMETRY.md); users export a **diagnostics QR** (DIAGNOSTICS.md) — operator
  decodes and files as a diagnostics bundle.
- Relay: gateway + private-relay logs held per INCIDENT_MANAGEMENT.md retention
  (bounded; no payload above consented telemetry definitions).

## 8. Acceptance hooks

- Maps to PILOT_001_DESIGN.md AC-1/AC-2/AC-6/AC-12/AC-15 + PILOT_EXERCISE.md
  (AC-4/AC-13) + PILOT_KPI_PLAN.md (AC-3/AC-11). Evidence rows cite this
  runbook + rehearsal records at TEST/VERIFY.
- Known limitations carried: BLK-0005 physical-device rows (the pilot is the
  device-gated leg); LEGAL-001 open questions (REG_NOTES.md); env-gated iOS
  CI leg.

---

**Authority note**: operator ceremonies (mutual-QR, authority push, drill
bundling) are the mechanism that satisfies the DEC-0010 pilot gate and
resolves the IDENT-001/EMERG-001 out-of-band trust requirement. Never
substitute mesh-mediated first contact.