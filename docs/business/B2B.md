# Business-to-Business (B2B) Strategy

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Business  

---

## 1. Enterprise Market Overview

IRIS addresses enterprise customers who need resilient communication for high-risk operations where standard cellular infrastructure is unreliable, inadequate, or absent. Unlike government customers (who prioritize disaster response), enterprise customers prioritize worker safety, operational continuity, and regulatory compliance (Factories Act, Mines Act, OISD regulations for oil & gas).

---

## 2. Enterprise Customer Segments

### 2.1 Mining

**Market:** India has 1,400+ coal mines (Coal India, SCCL, private), 700+ metalliferous mines. Total mine workforce: ~500,000 workers underground or in remote open-cast sites.

**Problem:** Underground mines have no cellular coverage. Surface-to-underground communication relies on wired systems (subject to blast damage) or proprietary leaky-feeder radio systems (expensive, single-point-of-failure). Collapse scenarios: workers trapped underground cannot communicate.

**IRIS solution:**
- BLE mesh between miners underground (limited range per hop but multiple hops through narrow tunnels)
- LoRa gateway at mine entrance bridges underground-to-surface
- SOS from trapped miners reaches surface operations
- Surface LoRa covers open-cast sites without cellular

**Key buyers:** EHS Manager, Mine Manager, General Manager Safety  
**Procurement trigger:** Mines Act 1952 compliance; DGMS (Directorate General of Mines Safety) audit findings; mine accident near-miss events

**Target companies:** Coal India subsidiaries (SECL, BCCL, ECL), Vedanta (Zinc, Copper), Hindalco, NMDC, JSW Steel

### 2.2 Construction

**Market:** India's infrastructure boom creates large, temporary construction sites: highways (NHAI projects), metro rail, dams, industrial parks. These sites operate 3–5 years in locations without permanent cellular coverage.

**Problem:** Construction sites use consumer walkie-talkies (short range, unencrypted) or depend on temporary cellular towers (expensive, slow to provision). Large sites (>10 km²) cannot be covered by a single cellular tower; workers at far ends are unreachable.

**IRIS solution:**
- Wi-Fi Direct and BLE for short-range communication between workers
- LoRa gateway at site office with solar power for long-range relay
- SOS for accidents in remote areas of large sites
- Authority broadcast for safety alerts (blast warnings, crane operations)

**Key buyers:** EHS Head, IT Manager, Project Director  
**Target companies:** L&T Construction, Shapoorji Pallonji, Tata Projects, Afcons Infrastructure, AECOM India

### 2.3 Oil and Gas

**Market:** ONGC has offshore platforms (Mumbai High, KG Basin) and onshore fields (Assam, Gujarat). Private operators: Reliance, Cairn India (Vedanta). Offshore platforms are the highest-value use case.

**Problem:** Offshore platforms depend on satellite (expensive, ~₹200/MB), private microwave links, or single-channel radio. Platform mustering and evacuation relies on PA systems (no individual communication). If the PA fails during an emergency, no fallback exists.

**IRIS solution:**
- BLE + Wi-Fi Direct mesh across offshore platform (steel structure; propagation challenges — see EXP-BLE-001)
- Satellite fallback (Iridium SBD) for shore-to-platform emergency communication
- SOS with GPS for man-overboard scenarios (GPS works offshore)
- Authority broadcast for evacuation orders (verified by platform manager key)

**Key buyers:** Platform Manager, HSSE (Health, Safety, Security, Environment) Manager, OIM (Offshore Installation Manager)  
**Target companies:** ONGC (offshore divisions), Reliance Industries offshore, Seadrill (contractor), McDermott India

### 2.4 Large Events

**Market:** India hosts large gatherings: Kumbh Mela (50M+ attendance), BAPS events (100k–1M), IPL matches, religious festivals at pilgrimage sites (Vaishno Devi, Tirupati).

**Problem:** Cellular networks become severely congested during large gatherings (10M+ simultaneous users in a 10 km² area). Emergency coordination (crowd crush, medical emergencies, stampede risk) cannot rely on cellular during peak congestion.

**IRIS solution:**
- Event deployment: 50–100 gateway nodes covering event area
- Event staff and security use IRIS for coordination
- Medical teams trackable on IRIS map
- Authority broadcast for emergency instructions (crowd management, evacuation)

**Key buyers:** Event Director, Security Head, Municipal Corporation events cell  
**Target companies:** Inox Live Events, BookMyShow Live, religious trusts (BAPS, Tirumala Tirupati Devasthanams)

---

## 3. Enterprise Product Features

### 3.1 Private Network Isolation

Enterprise IRIS nodes operate on a private mesh — they do not relay public (civilian) IRIS traffic. Private network achieved by:
- Enterprise-provisioned signing key: only nodes signed by the enterprise key join the enterprise mesh
- Network ID: enterprise mesh uses a distinct network identifier; cross-mesh relay disabled by default
- IRIS enterprise admin provisions all devices with enterprise credentials

**Why this matters:** Enterprises need to know their communication stays within their deployment. A mine does not want a civilian's SOS message consuming relay bandwidth on their network.

### 3.2 Admin Console

Web-based admin console (React frontend + IRIS REST API):
- Device management: register/deregister devices, view online status, last-seen location
- Alert broadcasting: enterprise admin sends authority broadcasts to all enterprise devices
- Compliance reporting: message volume, SOS events, relay statistics
- Network health: gateway node status, battery levels, satellite link status
- User management: add/remove enterprise users, assign roles

### 3.3 Integration API

For enterprise IT integration:
- REST API: send authority broadcast, query device status, retrieve SOS events
- Webhook: push SOS events to enterprise safety management system (SAP EHS, IOSH-compliant systems)
- SCIM: enterprise user provisioning (Okta, Microsoft Entra ID integration)

### 3.4 Compliance Reporting

Enterprise deployments generate reports for:
- Factories Act / Mines Act audit compliance
- OISD (Oil Industry Safety Directorate) safety system requirements
- ISO 45001 (Occupational Health and Safety) documentation
- Custom reports for EHS dashboard integration

---

## 4. Pricing

### 4.1 Per-Device Software License

| Tier | Devices | Annual per device | Includes |
|------|---------|-----------------|---------|
| Standard | 50–199 | ₹1,500 | Admin console, email support |
| Professional | 200–999 | ₹3,000 | + Priority support, integration API, compliance reporting |
| Enterprise | 1,000+ | ₹5,000 (or negotiated) | + Dedicated support, custom integration, SLA |

### 4.2 Hardware Kits (Enterprise)

Hardware purchased separately (see BUSINESS_MODEL.md). Enterprise kit includes:
- 1-year warranty (extended from standard 6 months)
- IRIS pre-provisioned with enterprise credentials (no setup required at site)
- Rack-mount option for control room installation

### 4.3 Deployment Services

| Service | Price |
|---------|-------|
| Initial deployment setup (remote) | ₹1,00,000 |
| Initial deployment setup (on-site, per day) | ₹25,000/day |
| Admin training (remote) | ₹50,000 |
| Integration API setup | ₹2,00,000 |
| Custom development (per day) | ₹30,000/day |

---

## 5. Sales Process

### 5.1 Sales Cycle

Typical enterprise sales cycle: 3–9 months

```
Lead → Discovery call → Technical demo → Pilot proposal → Pilot (4–8 weeks) → Commercial negotiation → Contract → Deployment
```

Proof of concept: offer 30-day free pilot (5 devices + 1 gateway) for qualified leads. Conversion from POC: target 40%.

### 5.2 Channel Partners

IRIS sells directly in Year 1–2. From Year 2 onward, channel partner program:
- **System integrators:** Wipro, HCL Tech, Mphasis — for large enterprise IT-led deals
- **EHS consultants:** BSI India, Bureau Veritas India — for Factories/Mines Act compliance-driven deals
- **Industrial equipment distributors:** for mining and construction hardware kits

Channel partners receive: 15–20% reseller margin; IRIS-certified partner training; co-marketing support.

---

## 6. Competitive Differentiation vs Enterprise Alternatives

| Feature | IRIS | goTenna Pro | Motorola radio system | Proprietary mine comm |
|---------|------|------------|----------------------|----------------------|
| Per-device cost | ₹1,500/yr (software) | ~₹15,000/device hardware | ₹20,000–1,00,000/device | ₹50,000–5,00,000/device |
| Infrastructure cost | Gateway kit ₹45,000 | None | Repeater ₹5,00,000+ | Leaky feeder: ₹1cr+/km |
| Open source, auditable | Yes | No | No | No |
| India WPC compliance | Yes | No | Yes (licensed) | Yes (licensed) |
| DTN store-carry-forward | Yes | No | No | No |
| Android phone compatibility | Yes | No | No | No |

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
