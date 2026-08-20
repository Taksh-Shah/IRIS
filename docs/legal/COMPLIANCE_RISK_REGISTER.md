# Compliance Risk Register

**Status:** Living document — reviewed quarterly  
**Last updated:** 2026-08-19  
**Owner:** Legal / Compliance  
**Review cycle:** Quarterly; or immediately following a regulatory change  

---

## 1. Risk Rating Methodology

**Likelihood:** 1 (Remote) → 5 (Near-certain)  
**Impact:** 1 (Minor inconvenience) → 5 (Existential — criminal liability, forced shutdown)  
**Risk score:** Likelihood × Impact  
**Risk level:** Low (1–5) | Medium (6–12) | High (13–19) | Critical (20–25)

---

## 2. Risk Register

### CR-001: LoRa Relay Operation Under Telegraph Act

| Field | Detail |
|-------|--------|
| Risk | Operating LoRa mesh relay constitutes "establishing a telegraph" under Section 4 of the Telegraph Act 1885, requiring a license not held by IRIS |
| Likelihood | 2 (WPC de-licensing framework provides reasonable basis; active prosecution is unlikely in disaster context) |
| Impact | 4 (Cease-and-desist order; potential criminal liability under Telegraph Act Section 20) |
| Risk Score | 8 (Medium) |
| Current Mitigation | (1) WPC de-licensing compliance (865–868 MHz SRD band, G.S.R. 853(E) 2021 Table-I non-specific SRD: 25 mW e.r.p., duty cycle ≤1% — governs the future LoRa leg only; v1 pilot is BLE/Wi-Fi on unlicensed 2.4/5 GHz); (2) Legal research in progress (LQ-001); (3) Government pilot deployments structured under NDRF/NDMA authority; (4) Pending formal legal opinion |
| Residual Risk | Medium (pending counsel opinion) |
| Owner | Legal |
| Next Review | On receipt of legal counsel opinion |

---

### CR-002: DPDPA Consent for Relay Nodes

| Field | Detail |
|-------|--------|
| Risk | Relay nodes processing bundle metadata (source/destination EIDs, timestamps) are classified as Data Fiduciaries under DPDPA, creating obligations that IRIS relay operators have not fulfilled |
| Likelihood | 2 (Relay is a transit function; DPDPA likely applies to content fiduciaries, not conduits) |
| Impact | 3 (DPBI directive to cease relay operation; financial penalty under DPDPA) |
| Risk Score | 6 (Medium) |
| Current Mitigation | (1) Data minimization in relay logging (headers not logged beyond bundle TTL); (2) Relay nodes do not process content; (3) Legal research in progress (LQ-002); (4) Seeking counsel opinion on relay Data Fiduciary classification |
| Residual Risk | Low-Medium |
| Owner | Legal / Engineering |
| Next Review | On receipt of LQ-002 counsel opinion |

---

### CR-003: Emergency Broadcast Without Authority Verification

| Field | Detail |
|-------|--------|
| Risk | A malicious actor broadcasts a fake emergency alert through the IRIS mesh, causing panic, evacuation from safe areas, or interference with official emergency response |
| Likelihood | 3 (Social engineering attacks on any public communication system are well-documented) |
| Impact | 4 (Mass panic, potential deaths from false evacuation, criminal prosecution of IRIS for facilitating false alarm) |
| Risk Score | 12 (Medium) |
| Current Mitigation | (1) Authority broadcast feature (PRD-005) requires Ed25519 signature from a pre-provisioned authority key; (2) Authority keys provisioned by NDRF/SDRF/NDMA out-of-band; (3) IRIS app displays verification status prominently; (4) Unsigned broadcasts displayed with clear "UNVERIFIED" warning; (5) IRIS does not forward unverified P0-priority broadcasts beyond first hop |
| Residual Risk | Medium (social engineering of key provisioning process; key compromise) |
| Owner | Engineering / Security |
| Next Review | Before Alpha release (Month 12) |
| Escalation | Any key compromise must be reported to NDRF immediately; revocation process defined in key management spec |

---

### CR-004: Satellite Operation Without GMPCS License

| Field | Detail |
|-------|--------|
| Risk | IRIS directly operates satellite terminal equipment for communications without holding a GMPCS license, violating DoT satellite policy |
| Likelihood | 1 (IRIS does not hold and does not plan to hold a GMPCS license; IRIS uses licensed operators) |
| Impact | 5 (Criminal liability under Indian Wireless Telegraphy Act; equipment seizure) |
| Risk Score | 5 (Low — contingent on maintaining policy of using licensed operators only) |
| Current Mitigation | (1) IRIS satellite integration policy: use only licensed satellite operators (Iridium via Tata Communications; Starlink India for internet backhaul); (2) IRIS does not operate satellite spectrum directly; (3) Contract requirements with satellite operators must document their GMPCS license; (4) IRIS gateway operators are contractually required to use only licensed satellite connectivity |
| Residual Risk | Low (if policy maintained) |
| Owner | Legal / Operations |
| Next Review | Before satellite feature launch (Month 18) |

---

### CR-005: Cross-Border Data Flow via Satellite

| Field | Detail |
|-------|--------|
| Risk | IRIS bundles relayed via satellite pass through non-India ground stations, constituting a cross-border personal data transfer under DPDPA without adequate safeguards |
| Likelihood | 3 (LEO satellite ground stations are geographically distributed; some traffic may route through non-India gateways) |
| Impact | 3 (DPBI penalty; required to cease satellite relay; reputational harm) |
| Risk Score | 9 (Medium) |
| Current Mitigation | (1) Iridium India traffic routed through Tata Communications India-based gateway (contractually confirmed); (2) IRIS routing policy: prefer terrestrial paths; satellite only for P0/P1 bundles when terrestrial unavailable; (3) IRIS data residency policy: bundles marked with regional processing flag; (4) Starlink internet backhaul treated as internet transit (not specific personal data transfer) |
| Residual Risk | Medium (satellite ground station routing not fully controllable by IRIS) |
| Owner | Legal / Engineering |
| Next Review | Before satellite feature launch (Month 18) |

---

### CR-006: IT Act §69 Non-Compliance for Encrypted Content

| Field | Detail |
|-------|--------|
| Risk | Government issues Section 69 interception order for IRIS message content; IRIS cannot technically comply due to end-to-end encryption; government views non-compliance as willful obstruction |
| Likelihood | 2 (Disaster response context reduces likelihood; IRIS not primarily a privacy tool from government perspective) |
| Impact | 4 (Criminal liability under IT Act Section 69; potential forced discontinuation of encryption) |
| Risk Score | 8 (Medium) |
| Current Mitigation | (1) IRIS publicly documents encryption design and the technical impossibility of content decryption; (2) Template response to §69 orders prepared with counsel; (3) IRIS cooperates with orders for metadata within its possession; (4) IRIS positions encryption as essential for security (not as obstruction); (5) Engagement with DoT to educate on end-to-end encryption technical constraints |
| Residual Risk | Medium (government response to encryption non-compliance is unpredictable) |
| Owner | Legal |
| Next Review | Annually; or on receipt of a §69 order |

---

### CR-007: STQC Certification Gap for Government Procurement

| Field | Detail |
|-------|--------|
| Risk | Government entities require STQC (Standardisation Testing and Quality Certification) certification for software procurement. IRIS lacks STQC certification, blocking government sales |
| Likelihood | 4 (STQC or equivalent is required for most central government software procurement) |
| Impact | 2 (Lost revenue opportunity; does not create legal liability) |
| Risk Score | 8 (Medium) |
| Current Mitigation | (1) STQC certification planned as part of GA (Month 30) milestone; (2) Government pilot deployments (Month 24) may proceed without certification under pilot MoU; (3) IRIS architecture is security-conscious which should facilitate STQC review |
| Residual Risk | Medium (until certification obtained) |
| Owner | Product / Legal |
| Next Review | Month 20 (start STQC process) |

---

### CR-008: Open Source Dependency License Compliance

| Field | Detail |
|-------|--------|
| Risk | IRIS uses open source dependencies with copyleft licenses (GPL, LGPL) that require source disclosure or other obligations not currently met in distribution |
| Likelihood | 2 (IRIS performs license audits; Apache 2.0 and MIT are primary dependencies) |
| Impact | 2 (Copyright claim; forced source disclosure; reputational harm) |
| Risk Score | 4 (Low) |
| Current Mitigation | (1) IRIS uses `cargo deny` in CI to enforce allowed licenses (allowlist: Apache-2.0, MIT, BSD-2, BSD-3, ISC, Unlicense); (2) GPL dependencies are prohibited in the allowlist; (3) LGPL dependencies require case-by-case review; (4) License audit before each major release |
| Residual Risk | Low |
| Owner | Engineering |
| Next Review | Before each major release |

---

## 3. Risk Summary Dashboard

| ID | Risk | Likelihood | Impact | Score | Level | Status |
|----|------|-----------|--------|-------|-------|--------|
| CR-001 | Telegraph Act / LoRa relay | 2 | 4 | 8 | Medium | Counsel pending |
| CR-002 | DPDPA for relay nodes | 2 | 3 | 6 | Medium | Counsel pending |
| CR-003 | False emergency broadcast | 3 | 4 | 12 | Medium | Controls deployed |
| CR-004 | Satellite without GMPCS license | 1 | 5 | 5 | Low | Policy enforced |
| CR-005 | Cross-border satellite data flow | 3 | 3 | 9 | Medium | Contractual controls |
| CR-006 | IT Act §69 encryption non-compliance | 2 | 4 | 8 | Medium | Legal prepared |
| CR-007 | STQC certification gap | 4 | 2 | 8 | Medium | Planned Month 20 |
| CR-008 | OSS license compliance | 2 | 2 | 4 | Low | CI controls |

---

## 4. Review and Escalation

**Quarterly review:** Legal + Engineering review all risks quarterly; update likelihood/impact based on new information.

**Immediate escalation triggers:**
- Any CR-Critical item emerges
- Any government enforcement action (cease-and-desist, investigation, Section 69 order)
- Material change in India telecommunications law
- Any key compromise (CR-003 escalation)

**Escalation path:** Legal Counsel → CISO → CEO → Board (for Critical)

---

## 5. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document — 8 risks registered |
| 2026-08-19 | PILOT-001 AC-16: CR-001 mitigation → G.S.R. 853(E) 2021 (865–868 MHz, ≤25 mW e.r.p., duty ≤1%); LoRa scoped future-leg only |
