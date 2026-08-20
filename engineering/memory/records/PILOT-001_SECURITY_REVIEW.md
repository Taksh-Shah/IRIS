# PILOT-001 SECURITY_REVIEW — Adversarial disposition (PILOT-RT-P001..P017)

**Document ID**: IRIS-PILOT-001-SECURITY_REVIEW-001
**Date**: 2026-08-19
**Stage**: SECURITY_REVIEW (iter ~157, AC-18)
**Status**: **RESOLVED** (2 FIXED + 12 RECORDED + 3 INFO) — no CRITICAL/HIGH
finding requiring code change; strongest items addressed with concrete
doc/process fixes
**Pattern**: BLE_002_SECURITY_REVIEW.md / IOS-001_SECURITY_REVIEW.md
**Scope**: PILOT-001 DESIGN + ops surfaces (ops/process layer; platform
mechanisms already independently security-reviewed at their own nodes)

Adversarial review performed by supervisor in-pass (independent redteam subagent
not dispatchable in this environment; review conducted against source evidence,
not claims). Every disposition verified against the actual design/ops docs and
the underlying platform records (EMERG-001, SEC-001, IDENT-001 security
reviews/tests).

**Verdict: PASS-with-fix** — the ops-layer design is sound and the trust
mechanisms it depends on (TEST-only SOS drill suppression, drill quota
cap-exemption, key rotation/revocation, DEC-0010 gate) are **verified real
code**, not paper claims. Residual items are process hardening, not blockers.

---

## Findings & disposition

| ID | Sev | Finding | Disposition |
|----|-----|---------|-------------|
| **P-001** | LOW | Mutual-QR ceremony relies on in-person confirmation of a short auth string (BQP/short-auth-string spec detail is L5-only — RES-0026 G-P7); no minimum entropy/length bound is documented. | **RECORDED** — physical-observation constraint is the primary control (attacker must be at both screens AND MITM the QR exchange); bind SAS length/entropy to the BQP spec when it hardens; note in REG_NOTES QC-7 backlog. |
| **P-002** | MEDIUM | Identity is generated on-device at first run; a device compromised *before* the ceremony would present an attacker key that the operator's verified-tier escalation would cryptographically bless. | **RECORDED** — in-person provenance check + TEE/Keystore/Keychain isolation + DEC-0010 gate (reject identity-less/sealer-less) bound the surface; hardware attestation (Play Integrity/DeviceCheck) is deferred hardening, not a v1 blocker. |
| **P-003** | LOW | Node registry (device UID, peer_id, batch, operator) is a static roster with no specified integrity/anchor; compromise reveals the pilot roster (not keys). | **RECORDED** — roster leak does not enable forgery (keys stay on-device); recommend hash/signed registry export at weekly review (joins P-007 check). |
| **P-004** | INFO | Batch ceremony of 10-20: on-screen QRs are shoulder-surfable in public venues. | **RECORDED** — operate the ceremony in a private space; operational guidance already implies in-person; INFO. |
| **P-005** | MEDIUM | Revocation is TrustStore-synced (earliest-seen-wins, IDENT-001); an offline/partitioned node keeps trusting a revoked peer until next sync — in a disaster (the pilot scenario) that window lasts as long as the partition. | **RECORDED** — bounded by pilot scale + operator-controlled revocation + AEAD key removal on-device + rate limits; hardening: add revocation-sync check to the already-scheduled weekly node-registry drift review (runbook §5). |
| **P-006** | LOW | "No-recovery" (deliberate v1) makes a wrong-device wipe unrecoverable; runbook lists wipe (destructive) *first* in decommission. | **RECORDED** — recommend **revoke → retire → wipe** ordering so operator error before wipe stays recoverable; LOW process improvement. |
| **P-007** | HIGH (mitigated) | Authority-root + DRILL-CA trust is anchored at one time at provisioning; no re-verification/rotation cadence — a compromised DRILL CA can mint DRILL certs and mark traffic ca-exempt for drill bookkeeping. | **FIXED (runbook)** — weekly authority-fingerprint + DRILL-CA health re-verification added to runbook §5; DRILL CA key custody locked to the 2-person signing-key rule (never on a field leaf); re-bundle before each exercise leg. Mitigated by out-of-band provisioning + short 3-month MoU window. |
| **P-008** | MEDIUM | DRILL certs carry "verified authority" privileges and drill quota cap-exemption (SEC-001 ACL); custody of the DRILL CA private key was unspecified. | **RECORDED (fix folded into P-007)** — DRILL CA key custody now 2-person + hardware token (runbook §2.4 note); drill traffic remains TEST-severity / P-class-bounded regardless of the SOS-bookkeeping exemption. |
| **P-009** | — (positive control) | Suspected conflation: "drill quota cap-exempt" vs "never consumes real-SOS quota." | **NOT A FINDING** — verified real code: EMERG-001 `drill.rs` (`drill_flag_suppresses`, `drill_message_type_suppresses`, `verified_drill_suppressed`, `drill_mismatch_rejected`, `SosKind::Test`) + SEC-001 EmergencyAcl "Drill verified authority cap-exempt" → drills demonstrably never consume real-SOS capacity. |
| **P-010** | LOW | TEST-only SOS rides the real P0 delivery path; a buggy/malicious drill could inject P0 traffic mesh-wide. | **RECORDED** — EMERG drill suppression + quota/rate-limiters + TEST severity bound scope; hardening: cap drill-SOS per device per exercise leg (e.g., ≤5) in PILOT_EXERCISE.md before M2. |
| **P-011** | LOW-MEDIUM | `relay_cadence` is policy/config at the ROUTE-001/INTERNET-001 seam with no code clamp; extreme `f` (0.01..0.1) or a sub-1.0 multiplier misconfiguration could under-thin at 100 nodes or over-thin at small N. | **RECORDED** — bound the multiplier effect (never <0.6×, never >3×) + validate `f` at load at the seam when ROUTE-001 wires the param; sim harness pre-validates the *rule* (B-1) but not a misconfigured operator value — operator error is an S2/S3 incident per runbook §6. |
| **P-012** | INFO | Cadence thinning grows broadcast/telemetry latency at high N (75 min at 60 nodes) — intended airtime minimization, but could be mistaken for a latency-KPI regression. | **RECORDED** — KPI latency targets are for `msg.delivered` (directed, cadence-exempt); no conflict; INFO reference. |
| **P-013** | MEDIUM | Consent is a briefing "slide" with no per-device capture/record — a DPDPA audit needs a timestamped consent trail per device, not an agenda item. | **FIXED (runbook + KPI plan)** — per-device opt-in captured at provisioning with timestamped node-registry record, revertible; KPI §4 updated; legal sign-off remains LEGAL-001 (QC-3). |
| **P-014** | LOW | Telemetry uses HMAC-salted hashes + 8-byte truncated IDs; a static per-pilot salt allows offline cross-correlation of small-ID spaces if the salt leaks. | **RECORDED** — rotate salt per pilot cohort + document minimum salt entropy in TELEMETRY.md; LOW. |
| **P-015** | MEDIUM (contained) | Play internal-track build is an APK that could be skim-sideloaded off a leaked tester email; a real IRIS build in the wrong hands. | **RECORDED — positive control**: blast radius contained by DEC-0010 gate — a non-provisioned device (no valid identity + AEAD) is rejected at the mesh edge (SEC-001 ACL + IdentityGate). Keep tester list private; internal track is the only channel. |
| **P-016** | LOW | TestFlight 90-day expiry: a missed calendar refresh silently drops iOS during an active exercise window. | **RECORDED** — already S2-owned (runbook §6 / distribution §8); hardening: add build-validity check to the M1/M2 pre-drill checklist. |
| **P-017** | INFO | Diagnostics QR bundles travel outside the private relay (e.g., photo of a screen); shoulder-surf exposure of device metrics. | **RECORDED** — restrict diagnostics-QR decode to operator devices on the relay; note in runbook §7; content remains within consented telemetry definitions. |

## Positive controls verified (evidence, not claims)

1. **TEST-only SOS never consumes real-SOS quota** — EMERG-001 `drill.rs`
   (`SosKind::Test`, `drill_flag_suppresses`, `verified_drill_suppressed`,
   `drill_mismatch_rejected`; EMERG-001_TEST.md AC-8/AC-10) + SEC-001
   EmergencyAcl "Drill verified authority cap-exempt" (SEC-001_TEST.md).
2. **Decommission revoke is real** — IDENT-001 rotate/trust_store:
   earliest-seen-wins, null-rotation revoke blocks paths, small-order re-checks,
   un_revoke healing path (IDENT-001_TEST.md AC-6).
3. **DEC-0010 pilot gate documented** — provisioned identity + AEAD mandatory;
   DevCryptoProvider / identity-less / sealer-less devices rejected (runbook
   §2.1).
4. **Relay-cadence exemptions correct** — P0/P1 emergency + directed messages
   exempt; telemetry cadence never touches message-delivery latency KPI.
5. **Sideload blast radius contained** — identity gate at the mesh edge.
6. **Platform trust chains are audited code** — EMERG-001, SEC-001, IDENT-001
   each passed their own security reviews (incl. 2 CRITICAL/8 HIGH fixes at
   SEC-001, IDENT-RT fix set incl. RFC 9171 skew budgets, replay handling).

## Verification (this pass)

- AC-18 workspace gate re-run at this stage: `cargo test --workspace
  --all-features` = **664 passed / 0 failed / 1 ignored** (23 suites; iris-core
  573 + iris-ios 11); clippy 0; `cargo fmt --all --check` clean. Baseline
  **658/0/1 → 664/0/1** held from TEST (iter ~156, evidence 5).
- Doc fixes applied this pass (FIXED dispositions): `PILOT_RUNBOOK.md` §2.4
  (DRILL CA custody + re-verification), §4 day-0 item 5 (per-device consent),
  §5 weekly (authority-root/DRILL-CA check); `PILOT_KPI_PLAN.md` §4 (consent
  trail). Source re-verified: no Rust code changed this pass.

## Scope note

PILOT-001 is an OPERATIONS/ops-layer node. The implemented platform mechanisms
it depends on are code-owned by EMERG-001 / SEC-001 / IDENT-001 / DEC-0010
carry (each previously security-reviewed) plus BLE-001/002, WIFIAWARE-001,
WIFIDIRECT-001, INTERNET-001. Physical-device + live-exercise rows remain
BLK-0005 / pilot-field-gated (recorded known_limitations).

## Next

VERIFY (iter ~158, AC-17/AC-18): independent verifier doc with a full evidence
table reproducing 664/0/1 + clippy + fmt and confirming every P-00x disposition
+ both FIXED doc edits → ACCEPT (iter ~159) → NODE_TRANSITION LORA-001/SAT-001.