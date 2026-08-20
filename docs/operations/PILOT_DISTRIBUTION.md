# Pilot Distribution + Ops Runbook (PILOT-001)

**Document ID**: IRIS-PILOT-DISTRIBUTION-001
**Version**: 1.0
**Node**: PILOT-001 (P2 OPERATIONS — DESIGN COMPLETE iter ~154; IMPLEMENT iter ~155)
**Date**: 2026-08-19
**Status**: ACCEPTANCE TARGET — PILOT_001_DESIGN.md AC-5 / AC-15
**Inputs**: PILOT_001_DESIGN.md D-8 (§9); RES-0026 RQ-5 (Play internal/closed
testing answers 9845334/14151465, Play App Signing 9842756, TestFlight);
ANDROID-001 (OEM battery-kill matrix); IOS-001 (Keychain/SessionRecovery);
TELEMETRY.md; INCIDENT_MANAGEMENT.md; DIAGNOSTICS.md.

---

## 1. Decision summary (DEC-PILOT-0006/0007)

- **Android cohort**: Google Play **internal** testing track — up to **100
  testers per app**, no review, near-instant builds; usable before full store
  setup. Pilot device emails = internal-track tester list (≤100).
- **iOS cohort**: **TestFlight external** — up to 10,000 testers; first
  external build requires **Beta App Review**; **builds expire after 90 days**
  → calendar-owned rolling refresh (below).
- **Closed track (12 testers × 14 days)** is the production-access
  prerequisite — start that clock **intentionally at pilot close** (production
  path, not the pilot). Explicitly re-verify Play account-type closed-track
  requirements at that step (G-P3; answer/14151465).
- **Play App Signing**: upload key RSA ≥2048 in a `.jks`, **2-person custody**
  (issue/use requires two named operators); Google manages the app-signing key;
  **PEPK** export/transfer flow documented + backups under the same custody
  rule. iOS: Apple-managed signing; distribution certs + provisioning under
  same custody model.

## 2. Track setup (Android)

1. Create Pilot team in Play Console; admin adds tester emails (≤100).
2. Create **internal track** (name: `pilot-cohort`); upload build via Play
   Console (or Gradle install). Internal-track builds reach testers within
   seconds — no review.
3. Do NOT promote to closed/open during the pilot. At pilot close, start a
   **closed track** (12 tester groups) for the 14-day production clock.
4. Update path: re-upload build → testers auto-notified via Play; app
   auto-update accepted on the pilot cohort only.

## 3. TestFlight setup (iOS)

1. App Store Connect → TestFlight → **external** testing group `IRIS Pilot`
   → add tester emails.
2. First build: submit for **Beta App Review** (briefer than App Review).
3. **Refresh cadence OWNER = calendar task** (calendar-owned, weekly check):
   builds expire 90 days — re-archive+upload before expiry; notify testers on
   refresh; version the build explicitly (`0.4.x-pilot`).
4. iOS distribution uses Keychain identity + TestFlight build; state
   restoration per IOS-001 (SessionRecovery/BGTask carried).

## 4. Signing custody runbook

1. **Upload key** (`.jks`, RSA-2048+): generated on an air-gapped/encrypted
   workstation; keystore passphrase split across **two named operators**
   (2-person rule — no single-person access).
2. **PEPK backup**: export the upload key with PEPK; store encrypted copy under
   the same 2-person custody; document restore drill (tested once per
   quarter).
3. Google-managed signing key: do NOT hold the app-signing key itself.
4. Loss/rotation policy per answer/9842756 (reset request flow); revocation
   logged in the incident ladder (INCIDENT_MANAGEMENT.md).

## 5. OEM battery-kill checklist (provisioning carry, ANDROID-001)

- Add IRIS to battery-exception / app-not-suspended lists on: Xiaomi/Walk-U
  (MIUI AutoStart), Samsung (Sleeping/Kill-after-timeout), ColorOS/realme,
  Vivo/OriginOS, Honor/Magic, Huawei. iOS: BGTask + restoration discipline
  (IOS-001) — no OEM kill applies; document force-quit behavior.
- Check `neighbors.active` counters after onboarding — a silent device usually
  means battery-kill (S2 incident per runbook §6).

## 6. Telemetry, consent, and retention

- Opt-in consent slide at M1 briefing (PILOT_KPI_PLAN.md §4); per-device
  toggle; revertible.
- Telemetry definitions per TELEMETRY.md (allow-list, no payload/PII); 7-day
  bounded retention; diagnostics QR export (DIAGNOSTICS.md) for outliers.
- No telemetry leaves the private relay endpoint except consented aggregates.

## 7. Incident ownership table (INCIDENT_MANAGEMENT.md)

| Severity | Example | Owner |
|----------|---------|-------|
| S0 | Pilot-wide delivery collapse / SOS-path defect | Relay operator + team lead → STOP field ops |
| S1 | Gateway uplink >1 h; cluster partition | Relay operator |
| S2 | Telemetry gap; single-device loop; TestFlight build expiry missed | NOC / calendar-task owner |
| S3 | BTB cosmetics; diagnostics drift | On-call tech |

## 8. Release cadence ownership

- **Android**: internal-track push after each approved build; changelog to
  cohort; **no** production promotion during pilot.
- **iOS**: 90-day TestFlight refresh = **calendar-owned task** (two named
  owners); re-upload + notify before expiry.
- Both: signing custody gates every release (2-person rule, §4).

## 9. Acceptance hooks

- AC-5/AC-15 evidence at TEST/VERIFY: track setup + tester list + refresh
  task ownership + custody evidence (2-person + PEPK restore drill record) +
  battery-kill checklist + consent/retention record. Verified against
  RES-0026 RQ-5 external facts (internal ≠ closed).