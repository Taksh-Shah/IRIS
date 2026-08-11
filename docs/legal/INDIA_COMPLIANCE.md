# India Regulatory Compliance

**Status:** Living document — not legal advice  
**Last updated:** 2026-08-11  
**Owner:** Legal / Compliance  
**Review required:** Legal counsel before any government deployment  

---

## Disclaimer

This document is an internal engineering reference summarizing the regulatory landscape. It is not a legal opinion. **Legal counsel review from qualified Indian telecommunications counsel is required before any commercial or government deployment.** Do not rely on this document as legal advice.

---

## 1. Wireless Planning and Coordination (WPC) — Spectrum

### 1.1 Authority

WPC Wing, Ministry of Communications, Government of India. WPC is the apex body for spectrum management in India, responsible for de-licensing notifications, frequency assignment, and enforcement.

### 1.2 LoRa Operation: 865–868 MHz SRD Band

**⚠️ Regulatory update required:** The 2021 WPC Gazette notification updated the India SRD band from the older 865–867 MHz specification to 865–868 MHz. All IRIS documents and hardware configurations must be validated against the current Gazette (2021) and DoT/WPC sources — not older secondary summaries. Pending legal counsel review, IRIS conservatively targets frequencies within 865–867 MHz which are within both the old and new band.

**De-licensing notification:** WPC de-licenses this band for Short Range Device (SRD) use under specified conditions. No individual license is required to operate within these conditions.

| Parameter | WPC Specification (revalidation required) | IRIS Target |
|-----------|------------------|------------|
| Frequency range | 865.0 – 868.0 MHz (2021 Gazette; revalidate) | 865.0625, 865.4025, 865.985 MHz |
| Maximum EIRP | 1 W (30 dBm) | ≤25 mW (14 dBm) — well within limit |
| Antenna gain | Not separately specified | Default module antenna: 2 dBi |
| License required | No (de-licensed) | N/A |
| Duty cycle restriction | None specified by WPC | IRIS self-imposes 1% duty cycle for congestion management |
| Type approval | Device must have WPC/TEC type approval if commercially sold | Required for IRIS hardware kits |

**Type approval requirement:** Commercial sale of LoRa hardware (including IRIS gateway kits) in India requires WPC type approval (Wireless Planning and Coordination type approval) and BIS registration under the Compulsory Registration Scheme (CRS). Third-party LoRa module manufacturers (Semtech, Hope RF) must provide type approval documentation.

### 1.3 BLE and Wi-Fi: 2.4 GHz

The 2.4 GHz band is de-licensed in India for low-power short-range devices. Bluetooth and Wi-Fi devices operate without individual spectrum license provided they:
- Comply with power limits (Bluetooth: 20 dBm maximum; Wi-Fi 2.4 GHz: 20 dBm maximum EIRP)
- Use devices with WPC/BIS type approval
- Android and iOS devices sold in India have this type approval as part of device certification

IRIS software running on commercially-sold smartphones does not require additional spectrum approval.

### 1.4 Satellite: See SATELLITE_REGULATION.md

---

## 2. Digital Personal Data Protection Act, 2023 (DPDPA)

**Reference:** Digital Personal Data Protection Act, 2023 (No. 22 of 2023). Received Presidential assent August 11, 2023.

### 2.1 Applicability to IRIS

DPDPA applies to processing of digital personal data within India. IRIS processes:
- User location data (GPS coordinates or cell-ID derived location)
- Contact history (record of BLE/Wi-Fi/LoRa encounters with other IRIS nodes)
- Message metadata (timestamps, source EID, destination EID, bundle IDs)

Message content is encrypted end-to-end; IRIS relay nodes process ciphertext only. Whether ciphertext constitutes "personal data" under DPDPA is analyzed in LEGAL_RESEARCH.md LQ-002.

### 2.2 Key DPDPA Obligations for IRIS

**Consent (Section 6):** IRIS must obtain consent from users before processing their personal data. Consent must be free, specific, informed, and unambiguous. IRIS obtains consent during app onboarding for: location sharing (when active), contact history accumulation (for routing). Consent is revocable; see data deletion (Section 13).

**Notice (Section 5):** A Data Fiduciary must give notice before consent. IRIS provides notice of: data collected, purpose, retention period, and user rights.

**Data minimization (Section 9(2)):** Only data necessary for the stated purpose may be processed. IRIS design principle: collect minimum necessary data. Contact history does not include message content. Location is shared only when user explicitly activates location sharing.

**Storage limitation (Section 9(3)):** Personal data must not be retained beyond the period necessary for the stated purpose. IRIS retention policy: messages deleted after TTL expiry; contact history retained for 30 days (adjustable by user); location history retained for session only.

**Security safeguards (Section 9(5)):** Reasonable security safeguards must be maintained. IRIS uses Ed25519 signing, ChaCha20-Poly1305 encryption, and BLAKE3 integrity checks. Key material stored in Android Keystore / iOS Secure Enclave.

**Data Principal Rights (Sections 11–14):** Users have rights of access, correction, erasure, and grievance redressal. IRIS implements: data export (own messages + contact history), data deletion (local + requested deletion from contacted nodes), correction (update own location/identity).

### 2.3 IRIS Data Fiduciary Classification

IRIS Technologies (the legal entity operating IRIS infrastructure) is a Data Fiduciary for data processed on edge servers and gateway nodes under its operation. Individual users who run the IRIS app are Data Fiduciaries for data processed on their own devices under the "for personal use" exemption in DPDPA.

Enterprise operators who deploy IRIS gateway infrastructure are independent Data Fiduciaries for data processed on their infrastructure.

---

## 2A. Digital Personal Data Protection Rules, 2025

**Reference:** Digital Personal Data Protection Rules, 2025 — formally notified by MeitY, November 2025, with staged commencement schedule.

The DPDP Rules 2025 operationalize the DPDPA 2023 and introduce specific obligations not in the Act itself. Key rules affecting IRIS:

| Rule | Obligation | IRIS Implication |
|------|-----------|-----------------|
| Data Fiduciary registration | Significant Data Fiduciaries (SDFs) must register with Data Protection Board | IRIS must monitor if it qualifies as SDF based on scale/sensitivity |
| Security safeguards specification | Rules specify technical safeguards (encryption, access controls) | IRIS's existing crypto suite likely compliant; formal audit required |
| Consent Manager | Mechanism for consent through registered Consent Managers | IRIS may need to integrate with MeitY-approved Consent Manager if required |
| Data localization | Rules may specify localization requirements for certain data classes | Edge server deployments must be India-hosted if localization applies |
| Children's data | Enhanced obligations for minors' data | IRIS must implement age verification or disable certain features for minors |
| Data Protection Officer | Mandatory DPO appointment for certain fiduciaries | IRIS Technologies must assess DPO requirement |
| Cross-border transfer | Rules specify approved jurisdictions for cross-border transfer | Satellite routing that transits foreign infrastructure needs analysis |

**Status:** DPDP Rules 2025 are a living regulatory instrument. IRIS legal compliance node must track commencement notifications and rule amendments. This section is a **RESEARCHED** claim, not a **VALIDATED** implementation — formal legal opinion required before deployment.

---

## 3. Information Technology Act, 2000 — Section 69: Lawful Interception

**Reference:** Information Technology Act, 2000, Section 69 (as amended by IT Amendment Act 2008).

### 3.1 The Requirement

Section 69 grants the Central Government and State Governments the power to direct interception, monitoring, or decryption of any information through any computer resource when necessary for: sovereignty, integrity, defense, public order, or investigation of offenses.

### 3.2 IRIS Response to §69 Requirements

**Technical impossibility for relay nodes:** IRIS relay nodes cannot comply with decryption requests because they do not hold decryption keys. Message content is encrypted with recipient's public key; neither IRIS Technologies nor relay node operators hold the private keys. Technical compliance with a decryption order for relay traffic is impossible.

**IRIS policy:**
1. IRIS will comply with lawful interception orders for metadata we hold (routing tables, contact history if logged on edge servers)
2. IRIS will inform the ordering authority that message content decryption is technically infeasible
3. IRIS will cooperate with investigations to the extent technically possible
4. IRIS will seek legal counsel before any disclosure to verify the order is lawful under Section 69

**Design implication:** End-to-end encryption is a core security property, not a feature that can be disabled by policy. Any key escrow or exceptional access mechanism would fundamentally undermine security against all adversaries. IRIS documents this design choice and its legal implications explicitly.

**Precedent reference:** The Puttaswamy judgment (2017) established privacy as a fundamental right; strong encryption is recognized as enabling privacy. This does not exempt IRIS from Section 69, but informs the proportionality analysis.

---

## 4. Indian Telegraph Act, 1885

**Reference:** Indian Telegraph Act, 1885 (as amended).

### 4.1 Relevance to IRIS

Section 3(1AA) defines "telegraph" broadly to include any apparatus for electromagnetic transmission. A LoRa radio node fits this definition.

Section 4 grants the Central Government exclusive privilege to establish, maintain, and work telegraphs in India, and allows licensing of others to do so.

### 4.2 De-Licensing and the Telegraph Act

The WPC de-licensing of 865–867 MHz is issued under the authority of the Telegraph Act via the Wireless Telegraphy Act 1933. The relationship between de-licensed spectrum and the Telegraph Act's licensing requirement is the subject of LQ-001 in LEGAL_RESEARCH.md.

**Current position:** IRIS takes the position that WPC de-licensing implicitly authorizes operation of ISM-band devices without a separate telegraph license, consistent with the intent of the de-licensing framework. This position has not been tested in court. Legal counsel review required before commercial deployment.

---

## 5. Disaster Management Act, 2005

**Reference:** Disaster Management Act, 2005 (No. 53 of 2005).

### 5.1 Emergency Communication Provisions

Section 65: The National Executive Committee (NEC) may take such measures as may be necessary for disaster management. This includes communication infrastructure deployment.

Section 44: The National Disaster Management Authority (NDMA) may lay down guidelines for minimum standards of communication during disasters.

### 5.2 Implication for IRIS

Government entities (NDRF, SDRF, district DMAs) operating IRIS under NDMA guidelines may have broader authority to deploy communication infrastructure than private entities. IRIS pilot deployments with NDRF can be structured under NDMA authorization, providing a cleaner regulatory basis than commercial deployment.

IRIS plans to seek NDMA recognition or endorsement for emergency communication use cases, which would provide regulatory cover for government-authorized deployments.

---

## 6. Compliance Summary Table

| Regulatory Area | Status | Risk | Action Required |
|----------------|--------|------|----------------|
| WPC 865–867 MHz operation | Compliant (within limits) | Low | Type approval for hardware kits |
| BLE / 2.4 GHz operation | Compliant | None | N/A |
| DPDPA consent | Design compliant | Low | Privacy audit before launch |
| DPDPA data minimization | Design compliant | Low | Audit relay metadata logging |
| IT Act §69 interception | Cannot technically comply for encrypted content | Medium | Formal legal opinion required |
| Telegraph Act | Gray area for relay operation | Medium | Legal opinion required |
| Disaster Management Act | Favorable for government deployments | Low | Seek NDMA endorsement |

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
