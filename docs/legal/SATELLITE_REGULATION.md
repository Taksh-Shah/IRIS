# Satellite Regulation: India

**Status:** Living document — not legal advice  
**Last updated:** 2026-08-11  
**Owner:** Legal / Compliance  

---

## 1. India DoT Satellite Policy Framework

### 1.1 Regulatory Authority

Department of Telecommunications (DoT), Ministry of Communications, Government of India, regulates satellite communications in India. The Indian Space Research Organisation (ISRO) and the newly formed Indian National Space Promotion and Authorisation Centre (IN-SPACe) regulate space segment activities. DoT regulates the ground segment and service provision.

### 1.2 Key Policy Documents

- **New Space Policy 2023:** Liberalizes commercial space activity; enables foreign satellite operators to seek licenses
- **Satellite Communication Policy 2000 (amended):** Base framework for satellite services in India
- **GMPCS Guidelines (DoT):** Specific guidelines for Global Mobile Personal Communications by Satellite services
- **TRAI Recommendations on Satellites (2022):** TRAI's recommendations on satellite broadband licensing; DoT has partially implemented

### 1.3 License Categories Relevant to IRIS

| License Type | Description | Relevance to IRIS |
|-------------|-------------|------------------|
| GMPCS License | Global Mobile Personal Communications by Satellite | Required for operators providing satellite messaging/voice services in India |
| ISP License | Internet Service Provider | Required for providers offering internet access (including via satellite) |
| VSAT License | Very Small Aperture Terminal | Required for operators of VSAT networks |
| NLD License | National Long Distance | For routing satellite traffic within India |

IRIS does not itself seek satellite licenses. IRIS uses services from licensed satellite operators. However, IRIS must use only licensed operators for India deployments.

---

## 2. GMPCS License: India

### 2.1 GMPCS License Basics

A GMPCS license is required for any entity offering global satellite mobile communication services in India. License requirements:
- Indian company (FDI limits apply: 100% FDI permitted in satellite services as of 2022 Cabinet decision)
- Security clearance from Ministry of Home Affairs
- Compliance with India's Lawful Interception requirements (Telecom Security Operations Centre must have access)
- Data localization: certain categories of traffic must be processed in India
- License fee: variable (application-based)

### 2.2 Who Holds GMPCS Licenses for India

As of 2026:

| Operator | Network | License Status | Services Available |
|----------|---------|---------------|-------------------|
| Hughes Communications India | Hughes (ViaSat/EchoStar) | Licensed | VSAT broadband |
| Tata Communications | Iridium (reseller) | Licensed | Iridium GMPCS services including SBD |
| Bharti Airtel | OneWeb/Eutelsat | Licensed | OneWeb broadband (commercial launch) |
| Jio Satellite Communications | SES | Licensed | GMPCS broadband |
| Starlink Internet Services India | SpaceX Starlink | Licensed (2023) | Starlink broadband internet |

---

## 3. Iridium India Operations

### 3.1 Iridium Network Overview

Iridium is a 66-satellite LEO constellation providing global satellite voice and data services. Iridium Short Burst Data (SBD) is particularly relevant to IRIS: it provides small-packet data transmission (up to 1960 bytes per message) via dedicated satellite modems.

### 3.2 India Availability

Iridium services are available in India through Tata Communications as the licensed GMPCS partner. Iridium SBD-capable modems (9603, 9602) are commercially available to Indian customers through licensed resellers.

**IRIS use case for Iridium SBD:**
- SBD payload: up to 1960 bytes per message (mobile-originated) / 1890 bytes (mobile-terminated)
- Latency: typically 20–60 seconds end-to-end
- Coverage: global including India
- Cost: ~$0.04–$0.10 per SBD message (depending on reseller and volume)

IRIS gateway nodes can use an Iridium 9603 module connected via serial (UART) or SPI to relay high-priority bundles (P0 SOS, P1 authority broadcast) via satellite when terrestrial mesh is insufficient. A SBD message can carry a compressed IRIS SOS bundle within the 1960-byte limit.

### 3.3 Regulatory Compliance for Iridium Use

Using Iridium SBD services through a licensed Indian entity (e.g., Tata Communications) is compliant with:
- GMPCS licensing: Tata holds the license; IRIS is a commercial customer
- Lawful interception: Iridium India (through Tata) has LI capabilities
- Data localization: Tata Communications routes Indian traffic through India-based gateways

IRIS must not import Iridium modems for re-export to other countries without appropriate export clearance.

---

## 4. Starlink India

### 4.1 License Status

SpaceX received a GMPCS license for Starlink Internet Services India Private Limited in 2023. Starlink broadband service is commercially available in India for fixed and maritime use.

### 4.2 Starlink for IRIS Gateway Use

IRIS gateway nodes (RPi-based) could use a Starlink dish as internet uplink for gateway-to-cloud relay. This is technically feasible and commercially available.

**Regulatory position:** Using Starlink as an internet uplink for an IRIS gateway is covered by Starlink's ISP license in India — this is standard internet connectivity, not a satellite messaging service. The IRIS application traffic rides over the internet, not over a specialized satellite messaging protocol. This is the lowest-risk regulatory approach for satellite backhaul.

**Limitation:** Starlink requires a fixed dish installation. Not suitable for mobile or handheld use. Suitable for permanent IRIS gateway nodes at fire stations, police posts, district DMA offices.

### 4.3 Starlink Direct-to-Cell (Future)

SpaceX's Starlink Direct-to-Cell aims to provide satellite connectivity to standard cellular-capable handsets without a specialized modem. The technology uses existing LTE protocols over satellite. India spectrum allocation and licensing for Direct-to-Cell has not been addressed (as of 2026). IRIS satellite integration plans do not depend on Direct-to-Cell.

---

## 5. Emergency Exemptions: TRAI Disaster Provisions

### 5.1 TRAI Priority Access in Emergency

TRAI has issued directions that licensed telecom operators must provide priority access for emergency services during disasters. These directives apply to licensed operators (Jio, Airtel, Vi, BSNL) and their satellite operations. They do not create a spectrum license exemption for unlicensed operators.

### 5.2 Disaster Management Act Section 65

The National Executive Committee under the Disaster Management Act can direct telecom operators to prioritize emergency communication. This is a regulatory direction to operators, not a new license category.

### 5.3 IRIS Position on Emergency Exemptions

IRIS does not rely on emergency exemptions for spectrum or satellite operation. IRIS uses:
- Licensed satellite services (Iridium via Tata Communications, Starlink internet uplink)
- De-licensed spectrum for terrestrial transports (865 MHz, 2.4 GHz)

This approach does not depend on emergency exemptions being invoked at the time of disaster — a critical resilience requirement. If emergency exemptions were required for operation, IRIS would be non-operational until the exemption was granted, which could take hours or days during a disaster.

---

## 6. Recommended Approach for IRIS Satellite Integration

1. **Primary:** Partner with Tata Communications (Iridium reseller) for Iridium SBD service. Sign commercial service agreement. Provision IRIS gateway nodes with Iridium 9603 modems.

2. **Secondary:** Use Starlink broadband (standard internet) as uplink for IRIS permanent gateway nodes where Starlink coverage and dish installation is feasible.

3. **Not recommended (v1):** OneWeb/Eutelsat — broadband internet focus, no SBD equivalent; less appropriate for IRIS message relay use case.

4. **Not recommended (any version without regulatory clarity):** Direct-to-device satellite messaging on unlicensed or ambiguously-licensed basis.

5. **Document gateway licenses:** Each IRIS gateway node that uses satellite connectivity must document the satellite operator's license number and the commercial service agreement. This documentation is required for government procurement compliance.

---

## 7. Cost Model for Satellite Fallback

| Service | Per-message cost | Monthly minimum | IRIS use case |
|---------|-----------------|----------------|--------------|
| Iridium SBD (via Tata) | ~₹3–8 per SBD message | ~₹2,000/month/modem | P0 SOS, P1 authority broadcast |
| Starlink internet | ₹3,500/month fixed | ₹3,500/month | Gateway backhaul (unlimited) |
| OneWeb (Bharti Airtel) | ₹4,000–8,000/month | ₹4,000/month | Gateway backhaul alternative |

For IRIS enterprise/government deployments, Iridium SBD for critical message relay + Starlink for gateway internet is the recommended combination.

---

## 8. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
