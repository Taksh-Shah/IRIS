# Business-to-Government (B2G) Strategy

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Business  

---

## 1. Government Customer Landscape

### 1.1 Primary Targets

| Organization | Role | Budget Authority | Priority |
|-------------|------|-----------------|----------|
| NDRF (National Disaster Response Force) | 15 battalions; primary field responders | MHA / NDMA allocation | P0 — pilot partnership |
| NDMA (National Disaster Management Authority) | Policy, standards, coordination | Central government | P0 — endorsement |
| SDRF (State Disaster Response Forces) | 31 state forces | State government (SDRF fund) | P1 — state-by-state |
| District Disaster Management Authorities | 700+ district DDMAs | Collector's budget | P2 — after state |
| Municipal Corporations (at-risk cities) | Urban disaster management | Municipal budget | P2 |
| ISRO / SAC | Space applications; potential satellite integration | GoI | P3 — research partnership |

### 1.2 Government Use Cases

**Search and rescue coordination:** NDRF teams operating in debris fields need communication when cellular is down. IRIS mesh between NDRF team members and command post.

**Flood response:** SDRF personnel in boats in flooded areas. LoRa range (km) is appropriate; BLE between boats and on-shore relay.

**Evacuation management:** DDMAs coordinating with police, SDRF, and community volunteers during evacuation. Authority broadcast for official evacuation orders.

**Community resilience:** Aapda Mitras (community disaster volunteers trained by NDMA) equipped with IRIS for village-level coordination.

---

## 2. Procurement Process

### 2.1 GeM Portal (Government e-Marketplace)

**About GeM:** The Government e-Marketplace (gem.gov.in) is the mandatory procurement platform for central government entities since 2016. State governments use GeM or equivalent state portals.

**GeM registration steps for IRIS:**
1. Seller registration on GeM (Udyam MSME registration required if applicable)
2. Product listing under appropriate category (Emergency Communication Systems / Safety Equipment)
3. Quality certification submission (BIS, WPC type approval for hardware; for software: TEC approval or equivalent)
4. GeM seller panel listing: enables direct purchase orders from government buyers

**Advantages of GeM:** Faster procurement (no tender for orders below ₹25 lakh); transparent pricing; buyer's trust.

**Limitations:** Lowest-price competition; IRIS must differentiate on certification and capability, not price alone.

### 2.2 Tender (GFR / CVC Rules)

For purchases above ₹25 lakh (central government) or state-equivalent thresholds, a formal tender process is required:

- **Expression of Interest (EoI):** NDRF/NDMA issues EoI for disaster communication systems; IRIS responds to build relationship
- **Request for Proposal (RFP):** Technical specifications submitted by IRIS; evaluated against RFP criteria
- **Technical Evaluation:** IRIS product demo; field trial results submitted as evidence
- **Financial Bid:** IRIS competes on price after technical qualification
- **L1 award:** Lowest-qualified bidder typically wins

**IRIS strategy for tenders:**
- Get technical specifications written around IRIS capabilities (open source, India-manufactured hardware, WPC-compliant LoRa) via early NDMA engagement
- Pre-qualify through NDMA/NDRF pilot so IRIS has field trial evidence competitors lack
- Form consortium with established government IT vendor (e.g., Wipro, HCL, L&T Technology) for large tenders

### 2.3 Direct Purchase (Pilot Phase)

Government entities can make direct purchases without tender for:
- Emergency procurement during declared disaster
- Pilot programs below ₹10 lakh (varies by state)
- Research and evaluation purposes

IRIS targets direct purchase for initial pilot deployments, then transition to GeM/tender for scale.

---

## 3. Government Pricing

### 3.1 Software Pricing (License)

IRIS software is Apache 2.0 open source. Government entities can use it at no license fee. Commercial value is in:

**Implementation and deployment services:**
- Deployment project: ₹5–30 lakh depending on scope
- Customization (government-specific features, IDRN integration): ₹10–40 lakh

**Annual support and maintenance:**
- Tier 1 (software only, standard SLA): ₹15 lakh/year
- Tier 2 (24×7 NOC, field engineer access, custom SLA): ₹30–50 lakh/year

### 3.2 Hardware Pricing (Government Rate)

| Kit | Commercial Price | Government Rate | Discount |
|-----|-----------------|----------------|---------|
| IRIS Gateway Basic | ₹15,000 | ₹12,000 | 20% |
| IRIS Gateway Standard | ₹45,000 | ₹38,000 | 15% |
| IRIS Gateway Solar | ₹75,000 | ₹60,000 | 20% |

Government rates apply to NDRF, NDMA, SDRF, and DDMA procurement. Documentation required.

### 3.3 Typical Deal Size

| Deployment Scale | Scope | Revenue (Year 1) |
|----------------|-------|-----------------|
| District pilot (20 gateways) | 20 kits + setup + support | ₹20–30 lakh |
| State SDRF deployment (100 gateways) | 100 kits + edge server + training + support | ₹80–120 lakh |
| National NDRF deployment (all 15 battalions) | 500+ kits + NOC + integration | ₹3–5 crore |

---

## 4. Compliance Requirements for Government

### 4.1 Empanelment

Several central government departments require vendors to be empanelled before procurement. IRIS targets:
- **NIC (National Informatics Centre) empanelment:** For IT software/services
- **NDMA empanelment:** For disaster management equipment and services (in progress)
- **DGS&D (Directorate General of Supplies & Disposals):** For equipment standardization

**Timeline:** Empanelment processes take 6–18 months; begin immediately.

### 4.2 STQC Certification

Standardisation Testing and Quality Certification (STQC) Directorate under MeitY provides quality certification for software and electronics. Government procurement often requires STQC for security-critical software.

**IRIS STQC target:** STQC Software Quality Certification (based on ISO/IEC 25010) for IRIS Android app and edge server.

**Timeline:** Begin process Month 20; target certification by Month 28 (2 months before GA).

**Cost estimate:** ₹5–15 lakh for full STQC assessment.

### 4.3 Data Localization

Government entities require that data processed by their systems remain within India. IRIS compliance:
- Edge servers in India (AWS Mumbai or Azure India) — compliant by default
- Satellite relay (Iridium SBD): traffic routes through Tata Communications India gateway — compliant for text SBD
- No message content stored outside India — compliant by design

### 4.4 MeitY/CERT-In Security Audit

Large government deployments may require a security audit by CERT-In (Indian Computer Emergency Response Team) empanelled auditors. IRIS should proactively commission a security audit (EAL2 equivalent or CERT-In methodology) before government sales begin.

**Cost estimate:** ₹10–20 lakh for full audit by empanelled firm.  
**Timeline:** Commission Month 18; complete before Pilot phase.

---

## 5. Pilot to Production Path

### 5.1 Phase 1: Pilot (Month 18–24)

- 1 NDRF battalion (200 personnel) in Ahmedabad
- 5 gateway nodes deployed
- 3-month pilot; outcome evaluation
- NDRF evaluation report → NDMA endorsement

### 5.2 Phase 2: State-Level Rollout (Month 24–30)

- Gujarat SDRF (post-Ahmedabad pilot relationship)
- 100 gateway nodes, 2 edge servers
- GeM procurement (IRIS listed on GeM by this point)
- Training program: SDRF trainers trained; train-the-trainer model

### 5.3 Phase 3: National Rollout (Month 30+)

- NDRF all 15 battalions
- NDMA-endorsed as standard emergency communication platform
- Tender participation for national-scale procurement

---

## 6. Key Government Relationships

| Relationship | Current Status | Target Status | Next Action |
|-------------|---------------|--------------|-------------|
| NDRF | Cold outreach | Pilot MoU | Present to NDRF Inspector General |
| NDMA | Cold | Advisory/Endorsement | Submit concept paper to NDMA HQ |
| MeitY | Unknown | Awareness | Attend National e-Governance Conference |
| CERT-In | Unknown | Security audit | Submit audit request at Month 18 |
| DoT/WPC | Unknown | Type approval | Submit type approval application at Month 15 |

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
