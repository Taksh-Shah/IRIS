# Legal Research Log

**Status:** Living document — not legal advice  
**Last updated:** 2026-08-11  
**Owner:** Legal / Compliance  
**Review required:** Qualified Indian telecommunications legal counsel  

---

## Purpose

This document tracks specific legal questions relevant to IRIS design and deployment. For each question, it records: the question, research findings to date, confidence in the finding, and whether escalation to counsel is required.

Status values: `Open` | `Researched (Pending Counsel Review)` | `Answered (Counsel Opinion)` | `Escalated`

---

## LQ-001: Does LoRa Mesh Relay Constitute "Establishing a Telegraph" Under the Telegraph Act?

**Status:** Researched (Pending Counsel Review)  
**Priority:** Critical — affects IRIS's fundamental legality in India  
**Assigned to:** External counsel (firm name withheld pending engagement)

### Question

Indian Telegraph Act 1885, Section 4 grants the Central Government exclusive privilege to establish, maintain, work, manage, control, or connect telegraphs. "Telegraph" is defined in Section 3(1AA) to include any apparatus for electromagnetic transmission. WPC de-licensing notifications authorize operation of ISM-band devices but do not explicitly state whether such operation constitutes "establishing a telegraph" requiring a separate license.

Does operating an IRIS LoRa relay node (transmitting on 865 MHz within WPC de-licensed limits) constitute "establishing a telegraph" requiring a license under Section 4 of the Telegraph Act?

### Research Findings

**Argument for "not establishing a telegraph" (permissive reading):**

1. WPC de-licensing notifications are issued under the authority of the Wireless Telegraphy Act 1933, which is subordinate legislation to the Telegraph Act. If de-licensing authorizes spectrum use, and spectrum use without a telegraph license were still an offense, the de-licensing would be illusory — authorizing something that remains illegal.

2. The DoT has, in practice, not prosecuted ISM-band device users (WiFi, Bluetooth, baby monitors, RFID) under the Telegraph Act. The consistent administrative practice supports the permissive reading.

3. Several Delhi High Court judgments on related questions (primarily relating to cable television and IP telephony) have interpreted "telegraph" narrowly to exclude non-traditional communication means where regulatory intent did not extend to them.

4. WPC de-licensing notifications from 2005 onward explicitly state devices may be used "without a license" — this language should be read to include telegraph license requirements.

**Argument for "requiring telegraph license" (restrictive reading):**

1. The text of Section 4 is broad and has not been amended to exclude ISM-band communication.

2. The DoT's non-prosecution of WiFi users may reflect resource constraints rather than legal position.

3. India's telecommunications regulatory history shows government willingness to reassert control over communication infrastructure (e.g., VoIP regulatory interventions 2005–2010).

4. LoRa mesh networking, which creates a multi-hop relay network, is more clearly "establishing" a telegraph than a single ISM device.

**Research finding:** The permissive reading is more consistent with the regulatory framework as a whole, but the question has not been tested in court with respect to mesh networking specifically.

**Confidence level:** Medium (60%) that IRIS relay operation does not require a telegraph license beyond WPC de-licensing compliance.

### Escalation and Required Action

- Formal written opinion from qualified Indian telecom counsel required before commercial deployment
- Government pilot deployments should be conducted under NDRF/NDMA authority to avoid the question
- Monitor DoT guidance on mesh networking and ISM bands

---

## LQ-002: Does DPDPA Apply to Relay Nodes That Never Decrypt Messages?

**Status:** Researched (Pending Counsel Review)  
**Priority:** High — affects relay node operator obligations  
**Assigned to:** Internal legal research; external counsel review pending

### Question

IRIS relay nodes process encrypted bundle payloads. They read the bundle header (source EID, destination EID, TTL, bundle ID) for routing purposes but cannot access the encrypted payload. Does a relay node operator constitute a "Data Fiduciary" under DPDPA for the encrypted payload data it processes? Does the bundle header metadata (source/destination node identifiers) constitute "personal data"?

### Research Findings

**DPDPA Section 2(t) — "Personal data":** "any data about an individual who is identifiable by or in relation to such data." 

IRIS source/destination EIDs are Ed25519 public key hashes — 256-bit values with no direct human identifier. However, if a public key hash can be linked to a specific person (because the person has shared their IRIS ID), it becomes personal data for that linked person.

**DPDPA Section 2(i) — "Data Fiduciary":** "any person who alone or in conjunction with other persons determines the purpose and means of processing of personal data."

A relay node operator does not determine the purpose of data processing — the sender determines the purpose (delivering a message). The relay is a conduit. This is analogous to a telecommunications carrier, which is not typically a Data Fiduciary for user communication content.

**EU GDPR analogy (persuasive, not binding in India):** Under GDPR, Article 4(8) defines "processor" separately from "controller." A processor processes data on behalf of a controller. GDPR explicitly distinguishes this. DPDPA does not have an explicit "processor" category — it refers only to "Data Fiduciary" and "Consent Manager."

**Research finding:** Relay nodes processing only routing headers (not payload content) are likely not Data Fiduciaries under DPDPA for the payload content. Their status regarding routing metadata (EIDs, TTL, bundle IDs) is less clear. The DoT/DPBI (Data Protection Board of India) has not issued guidance on relay/transit processing.

**Confidence level:** Medium (65%) that relay nodes are not Data Fiduciaries for encrypted payload content; Low (40%) on routing metadata.

### Required Action

- Counsel opinion required
- IRIS to implement data minimization in relay logging (relay nodes should not log routing metadata beyond what is required for store-carry-forward; no persistent relay logs beyond TTL period)

---

## LQ-003: Are Emergency Communications Exempt from Lawful Intercept Under the IT Act?

**Status:** Open  
**Priority:** Medium — affects IRIS response to government interception orders  

### Question

Section 69 of the IT Act authorizes interception of "any information transmitted through any computer resource." Does the Disaster Management Act 2005 or any other legislation create an exemption from Section 69 interception requirements for emergency communication systems?

### Research Findings

No explicit exemption from Section 69 has been found for emergency communication systems. The IT Act §69 exemptions are limited to: information protected by privilege (attorney-client, etc.) as recognized by the Indian Evidence Act.

The Disaster Management Act 2005 grants powers to authorities to use communication infrastructure but does not limit government interception powers.

**Research finding:** No emergency exemption from Section 69 appears to exist. IRIS must handle Section 69 orders based on its technical capability (metadata disclosure possible; content decryption technically infeasible). See INDIA_COMPLIANCE.md Section 3.

**Confidence level:** High (80%) that no exemption exists.

### Required Action

- Counsel confirmation of no exemption
- Legal hold process documented in internal compliance procedures
- Template response to Section 69 orders (explaining technical infeasibility of content decryption) to be drafted with counsel

---

## LQ-004: Can Foreign Satellites Be Used for Civilian Emergency Communication in India?

**Status:** Researched (Pending Counsel Review)  
**Priority:** High — affects IRIS satellite fallback feature design  
**Relevant documents:** SATELLITE_REGULATION.md

### Question

IRIS roadmap includes satellite fallback (Month 18) for use when terrestrial mesh is insufficient. Target satellite networks include Iridium SBD and Starlink. Can Indian civilians use these networks for emergency communication?

### Research Findings

**Iridium India:** Iridium operates in India through a licensed Indian entity (Tata Communications is the commercial partner for Iridium GMPCS services in India as of 2024). Iridium SBD (Short Burst Data) service is available to Indian customers through licensed resellers. Use for emergency data communication is permissible under commercial service terms.

**Starlink India:** SpaceX received DoT approval for GMPCS (Global Mobile Personal Communications by Satellite) license in 2023. Starlink broadband service is licensed for operation in India. However, the Starlink license is for broadband internet service, not satellite messaging. Whether IRIS could use Starlink as a backhaul gateway is a question of service terms and regulatory classification.

**OneWeb India:** OneWeb (branded as Eutelsat OneWeb) received GMPCS license for India. Similar situation to Starlink — primarily internet backhaul.

**Emergency exemptions:** TRAI's regulations on GMPCS contain provisions for emergency service priority, but these are directed at network operators, not end users.

**Research finding:** Iridium SBD through a licensed Indian entity is the clearest path for IRIS satellite messaging. Starlink/OneWeb as backhaul gateways require regulatory assessment of whether IRIS gateway use fits within the broadband internet license. Direct-to-device satellite (Globalstar, future Starlink Direct-to-Cell) for end-user emergency messaging faces the most uncertain regulatory status.

**Confidence level:** High (85%) for Iridium SBD via licensed entity; Medium (55%) for Starlink gateway; Low (30%) for direct-to-device satellite.

### Required Action

- Engage DoT for informal guidance on GMPCS use for emergency communication applications
- Structure Iridium partnership through licensed Indian entity (not direct SpaceX/Iridium relationship)
- Do not deploy Starlink or direct-to-device satellite features until regulatory clarity obtained
- See SATELLITE_REGULATION.md for full analysis

---

## Research Log Summary

| ID | Question | Status | Confidence | Action |
|----|---------|--------|-----------|--------|
| LQ-001 | LoRa relay and Telegraph Act | Researched, Pending Counsel | 60% permissive | Counsel opinion required |
| LQ-002 | DPDPA for relay nodes | Researched, Pending Counsel | 65% not fiduciary | Counsel opinion required |
| LQ-003 | IT Act §69 emergency exemption | Open | 80% no exemption | Counsel confirmation |
| LQ-004 | Foreign satellite for emergency | Researched, Pending Counsel | Varies by provider | DoT engagement; Iridium partnership |

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document — 4 legal questions recorded |
