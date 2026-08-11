# Business Model

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Business / Product  

---

## 1. Core Model: Open Source + Commercial Services

IRIS follows the open core model:
- **Open source core (Apache 2.0):** Android app, iOS app, Rust protocol core, edge server software, gateway firmware
- **Commercial layer:** enterprise support contracts, government services, hardware kits, managed cloud relay

The open source strategy is structural: IRIS competes with proprietary systems (goTenna, Bridgefy) by being the verifiably secure, auditable alternative. Governments and NGOs will not deploy critical emergency infrastructure they cannot audit. Apache 2.0 licensing enables this trust while allowing commercial services.

---

## 2. Revenue Streams

### 2.1 Enterprise Support Contracts

**Target:** Large enterprises deploying IRIS for worker safety (mining, construction, oil & gas)  
**Model:** Annual support contract per deployment  
**Pricing:**
- Base: ₹1,500/device/year (minimum 50 devices)
- Enterprise: ₹5,000/device/year (includes dedicated support, SLA, custom integration)
- Large enterprise (500+ devices): negotiated

**What the contract includes:**
- Priority support (4-hour response SLA)
- Private network isolation (enterprise nodes do not relay public mesh traffic)
- Admin console access (device management, alert broadcasting, usage reporting)
- Software updates and security patches for contract term
- IRIS-led onboarding training (remote; on-site at premium)

**Revenue model:** MRR (monthly recurring revenue) from annual contracts divided by 12.

### 2.2 Government Contracts

**Target:** NDRF, SDRF, district DMAs, municipal corporations  
**Model:** Project-based deployment + annual support  
**Pricing structure:**
- Deployment project: ₹10–50 lakh depending on scope (hardware, setup, training, customization)
- Annual support: ₹20–50 lakh (includes 24×7 NOC monitoring, field engineer support)

**Procurement path:** Government Marketplace (GeM) portal listing (seller registration), direct tender, NDMA empanelment.

**Key government customer criteria:**
- Data localization (India-based servers) — IRIS compliant by design
- STQC certification — planned Month 30
- Integration with IDRN (India Disaster Resource Network) — roadmap item

### 2.3 Hardware Kits

**Target:** NGOs, state disaster management authorities, enterprises needing turnkey deployment  
**Product:** IRIS Gateway Kit — preconfigured RPi4 + LoRa module + optional Iridium modem + power management  

**Kit variants:**

| Kit | Components | EIRP | Iridium | Price (est.) |
|-----|-----------|------|---------|-------------|
| Basic | RPi4 + SX1262 LoRa + 2.4 GHz | 14 dBm | No | ₹15,000 |
| Standard | Basic + Iridium 9603 modem | 14 dBm | Yes | ₹45,000 |
| Solar | Standard + 20W solar panel + LiFePO4 battery + charge controller | 14 dBm | Yes | ₹75,000 |

**Margin:** 30–40% gross margin on hardware (hardware is not the primary business; kits create deployment and support revenue)

**Distribution:** Direct (IRIS website/sales), NGO partners, system integrators

### 2.4 Managed Cloud Relay

**Target:** NGOs and smaller enterprises who want IRIS without managing edge server infrastructure  
**Model:** SaaS — managed edge server operated by IRIS  
**Pricing:**
- Starter (up to 50 nodes): ₹5,000/month
- Growth (up to 500 nodes): ₹20,000/month
- Enterprise (500+ nodes): ₹50,000+/month (custom)

**Infrastructure:** AWS Mumbai (ap-south-1) + Azure India Central; multi-region for redundancy  
**SLA:** 99.5% uptime for relay function; 99.9% for data storage

---

## 3. Unit Economics

### 3.1 Edge Server Deployment

| Item | Cost |
|------|------|
| RPi Gateway Kit (Solar) | ₹75,000 (one-time) |
| VPS for edge server (AWS t3.medium India) | ₹3,500/month |
| Iridium SBD service | ₹2,000/month (message allowance) |
| Annual software maintenance | ₹10,000/year |
| **Total Year 1 (hardware + hosting + SBD)** | **₹1,34,000** |
| **Total Year 2+ (hosting + SBD + maintenance)** | **₹67,000/year** |

At ₹5,000/device/year enterprise support pricing with 50 devices per deployment:
- Annual support revenue: ₹2,50,000
- Year 1 gross margin: ₹2,50,000 − ₹1,34,000 = ₹1,16,000 (~47%)
- Year 2+ gross margin: ₹2,50,000 − ₹67,000 = ₹1,83,000 (~73%)

### 3.2 Government Contract Unit Economics

Government contracts are project-priced (higher margin but lumpy revenue):
- Typical state SDRF deployment: 20 gateway kits + edge server + 12-month support
- Revenue: ₹30–40 lakh
- Cost: ₹15 lakh hardware + ₹5 lakh setup + ₹5 lakh year 1 support = ₹25 lakh
- Gross margin: ~35–40% Year 1; higher in subsequent years

---

## 4. Target Customers

### 4.1 Primary (Year 1–2): Government

- **NDRF:** 15 battalions across India; natural partner for field trial
- **SDRF:** 31 state disaster response forces; procure separately from NDRF
- **District Disaster Management Authorities:** 700+ districts; long sales cycle, smaller deals
- **Municipal Corporations:** Major cities (Mumbai, Chennai, Kolkata); direct risk from floods, cyclones

**Buyer profile:** District Collector, DDMA CEO, NDRF Commandant, State DMA Deputy Director  
**Budget source:** State Disaster Response Fund (SDRF), 13th/14th Finance Commission grants, NDMA budget

### 4.2 Secondary (Year 2–3): Enterprise

- **Mining:** Coal India (450,000 employees), NMDC, Vedanta — underground mine communication
- **Construction:** L&T Construction, Shapoorji Pallonji — remote site communication
- **Oil & Gas:** ONGC, Reliance Industries — offshore and remote operations
- **Large Events:** BAPS Swaminarayan (large religious gatherings), IPL venues

**Buyer profile:** EHS (Environment, Health & Safety) Manager, IT/OT Head, Operations VP

### 4.3 Tertiary (Year 3+): NGOs and Civil Society

- Oxfam India, World Vision India, ActionAid India — humanitarian response
- India Red Cross — disaster response operations
- State Aapda Mitras (community disaster volunteers) — grassroots deployment

**Pricing:** NGO pricing — 50% discount on hardware kits; software free  
**Volume:** Low revenue but high impact; strategic for credibility and use case validation

---

## 5. Open Source Strategy

Apache 2.0 license is chosen to:
- Allow government and enterprise use without copyleft obligations
- Enable commercial forks (no restriction) — IRIS competes on quality, not lock-in
- Meet standard open source requirement for many government procurement policies
- Enable security researchers to audit the protocol

IRIS does not "open core" in the traditional sense (proprietary features behind paywall). All features are open source. Commercial value comes from: support, deployment expertise, managed infrastructure, and hardware. This is the Canonical / Red Hat model for infrastructure software.

**Community strategy:** GitHub repository public from Day 1; IRIS devs active on Meshtastic forums (adjacent community); speaker slots at FOSSASIA, PyCon India, rootconf; academic engagement with IIT Bombay, IISc.

---

## 6. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
