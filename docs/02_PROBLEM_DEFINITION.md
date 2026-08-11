# IRIS Problem Definition

**Document ID:** IRIS-PROB-001  
**Version:** 1.0

---

## 1. Problem Statement

Human beings lose the ability to communicate precisely when they need it most. When a building collapses, a flood cuts off a village, or a wildfire traps hikers — the moment of maximum danger is also the moment when cellular networks, internet connectivity, and even landlines most frequently fail.

In India, this failure is not hypothetical. It is documented, recurring, and lethal.

**The core problem:** Existing communication infrastructure is fragile by design. It is centralized, dependent on power, and optimized for normal operation — not for the worst-case scenarios where communication determines survival.

**The secondary problem:** No viable, open, offline communication system exists that works across the full continuum of disaster severity — from "3G signal is weak" to "every tower in the district is down."

---

## 2. India Infrastructure Context

### 2.1 Cellular Network Fragility

India has 1.17 billion mobile subscribers (TRAI, 2024) served by approximately 700,000 cellular towers. This appears robust until we examine the failure modes:

**Power dependency:** 33% of India's cell towers run on diesel generators with ≤8 hours of fuel reserve. During floods and earthquakes, fuel resupply is impossible. Towers go dark within a day of grid failure.

**Backhaul concentration:** 70% of India's inter-district traffic runs over optical fiber. A single fiber cut — which any flood, earthquake, or construction accident can cause — can sever connectivity for an entire district. The Uttarakhand floods (2021) severed backhaul for 12 districts simultaneously.

**Tower density in disaster-prone regions:**

| Region | Tower Density | Primary Disaster Risk |
|--------|--------------|----------------------|
| Kutch district, Gujarat | 0.8 towers/km² | Earthquake, drought |
| Sundarbans, West Bengal | 0.3 towers/km² | Cyclone, flood |
| Arunachal Pradesh hills | 0.1 towers/km² | Landslide, earthquake |
| Lakshadweep islands | 0.05 towers/km² | Cyclone, tsunami |
| Mumbai (normal) | 45 towers/km² | Flood (monsoon) |

When towers are sparse, any failure creates immediate dark zones. When towers are dense (Mumbai), simultaneous user spike during emergency overwhelms capacity.

**Network overload:** On 26 July 2005 (Mumbai floods), 26 July 2001 (Gujarat earthquake), 26 December 2004 (tsunami), cellular networks across affected regions crashed within minutes due to call volume spikes — 100× normal — overwhelming switching infrastructure. No amount of tower density prevents this. The problem is architectural.

### 2.2 Internet Infrastructure Fragility

India's internet backbone follows river valleys and highway corridors — exactly where floods concentrate. The submarine cable landing stations (Mumbai, Chennai, Cochin, Tuticorin) are coastal — exactly where cyclones hit. Internet infrastructure co-fails with disaster events at rates far higher than engineering margins suggest.

**Documented outages (selected):**
- **Kerala floods, August 2018:** 14 districts lost mobile internet for 3–12 days. 483 people killed; 1.5 million displaced. Communication failures directly impeded rescue coordination.
- **Cyclone Fani, Odisha, May 2019:** 4,000+ towers damaged; 80% of Puri district lost connectivity. NDRF coordination was conducted via satellite and runners.
- **Chamoli disaster, Uttarakhand, February 2021:** Gletscher burst. Communication infrastructure destroyed in Chamoli and Tapovan regions. Rescue teams operated on radio silence for 6 hours.
- **Assam floods, 2022:** 3,300+ villages affected; connectivity lost for 10+ days in 28 districts. Relief coordination via HF radio (ARISS amateur radio network) — 1970s technology as emergency fallback.

### 2.3 The Last-Mile Problem

Even when infrastructure is functioning, 200 million Indians live in areas with no reliable cellular coverage. These regions — tribal areas, island territories, high-altitude zones, deep forest — have exactly the terrain that makes rescue operations difficult and communication most necessary.

In Kutch district — the scenario central to IRIS's design — approximately 15% of the population is in areas with intermittent-to-no cellular coverage under normal conditions. In a disaster that destroys towers, that figure approaches 100%.

---

## 3. Current Solutions and Their Failures

### 3.1 Cellular Networks

**Failure mode:** Centralized infrastructure + power dependency + overload susceptibility.

| Failure Scenario | Cellular Response | Impact |
|-----------------|-------------------|--------|
| Tower power failure | Coverage dead zone | No communication in zone |
| Backhaul fiber cut | Complete district outage | No communication across region |
| Mass casualty event | Network congestion, all calls blocked | Emergency calls fail |
| Tower structural collapse | Permanent outage until rebuilt | Weeks to months of silence |

**Why cellular cannot solve this:** The architecture requires a functioning tower, functioning backhaul, and non-overloaded switch. All three must be true simultaneously. In a major disaster, all three are simultaneously false.

### 3.2 WhatsApp and Internet Messaging

WhatsApp has 500 million users in India. When internet works, it works. The problem is the dependency chain:

```
WhatsApp message → requires:
  ✗ Active internet connection
  ✗ WhatsApp servers online and reachable
  ✗ Recipient's internet connection
```

During every major Indian disaster, WhatsApp becomes unavailable when it is needed most. During the 2018 Kerala floods, WhatsApp status updates from trapped survivors were delayed 4–6 hours due to internet disruptions — making real-time rescue coordination impossible.

WhatsApp is an internet application, not a communication infrastructure. The distinction is fundamental.

### 3.3 SMS / USSD

SMS has lower infrastructure requirements than internet but still requires:
- Active cellular connection
- Functioning SMS gateway
- Non-overloaded network

During network overload events (earthquake, cyclone landfall), SMS delivery rates drop to 10–30% in affected areas. SMS is marginally better than internet messaging during disasters but still fails catastrophically at the moments of maximum need.

### 3.4 Amateur Radio (HAM)

HAM radio operators are the heroes of Indian disaster communication. During every major disaster, HAM operators provide communication backbone when everything else fails. But:

- **Coverage:** ~600,000 licensed HAM operators in India. Density in disaster-prone rural areas is low.
- **Equipment:** HAM equipment is non-trivial to acquire, license, and operate. Cannot be democratized to the general population.
- **Latency:** Voice-only (primarily). No structured data. No GPS coordinate transmission. No priority queuing.
- **Interoperability:** No integration with digital systems. Cannot interface with modern emergency response software.

HAM radio is a critical gap-filler, not a scalable solution. IRIS should complement, not replace, HAM infrastructure.

### 3.5 Satellite Phones / Terminals (Iridium, GSAT)

Satellite communication bypasses terrestrial infrastructure entirely. But:

- **Cost:** ₹50,000–₹5,00,000 per terminal. Ongoing airtime costs.
- **Availability:** India has very few satellite terminals deployed outside government and military contexts.
- **Coverage:** Iridium is global. GSAT requires ground station connectivity. In disaster, even satellite systems can be overwhelmed.
- **Democratization:** Cannot be deployed to general population. Only suitable for emergency response coordination teams.

Satellite communication is essential for emergency coordination hubs (NDRF bases, hospitals, government centers). It cannot serve as the communication layer for 400,000 affected civilians.

---

## 4. Existing Mesh/DTN Solutions and Their Insufficiency

### 4.1 Bridgefy

**What it does:** BLE mesh messaging app. Used during Hong Kong protests, Indian farmer protests, Cuba internet shutdowns.

**Why it's insufficient for IRIS use case:**

| Dimension | Bridgefy | IRIS Requirement |
|-----------|----------|-----------------|
| Protocols | BLE only | BLE + Wi-Fi Direct + LoRa + Satellite |
| Range per hop | ~100m (BLE) | 100m (BLE) to 30km (LoRa) |
| Store-carry-forward | Limited | Full DTN with persistence |
| Priority system | None | P0–P7 with hard guarantees |
| SOS emergency mode | None | P0 mandatory relay |
| Security model | Proprietary, previously broken (2021) | Noise Protocol, auditable, open |
| Open source | No | Yes |
| LoRa support | No | Yes |
| Battery optimization | Minimal | Aggressive power management |
| Gateway architecture | No | Yes |
| Disaster-specific features | None | Emergency broadcast, SAR coordinates |

Bridgefy was found to have critical security vulnerabilities in 2021 (Albrecht et al.) — including unauthenticated relay attacks, message forgery, and sender tracking. An app used for safety communication during political protests had security failures that would have been catastrophic if exploited. IRIS cannot replicate these failures.

### 4.2 Meshtastic

**What it does:** LoRa-based mesh communication system. Popular with outdoor enthusiasts, emergency preparedness community.

**Why it's insufficient:**

| Dimension | Meshtastic | IRIS Requirement |
|-----------|-----------|-----------------|
| Primary transport | LoRa only | Multi-transport (BLE primary, LoRa for range) |
| Smartphone integration | Via BLE bridge | Native smartphone SDK |
| User base | Requires dedicated hardware | Works on existing smartphones |
| iOS support | Limited | Full native support |
| DTN capability | Basic | Full store-carry-forward with routing algorithms |
| Security | AES-128 (limited key management) | Full Noise Protocol + Ed25519 identity |
| Emergency priority | None | P0–P7 |
| Scalability | ~100 nodes practical | Thousands of nodes |
| India deployment | None | India-first design |

Meshtastic is excellent for LoRa-enthusiast communities with dedicated hardware. It cannot serve as mass-deployment infrastructure for a country of 1.4 billion people.

### 4.3 Briar

**What it does:** Privacy-focused messaging over Tor, Bluetooth, Wi-Fi. Used in high-censorship environments.

**Why it's insufficient:**

| Dimension | Briar | IRIS Requirement |
|-----------|-------|-----------------|
| Focus | Privacy/anonymity | Safety/reliability |
| Transport | Bluetooth, Wi-Fi, Tor | BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite |
| LoRa support | No | Yes |
| Emergency priority | None | P0–P7 |
| Battery optimization | Poor (Tor is expensive) | Aggressive |
| DTN | Basic | Full |
| iOS support | No | Yes |
| Scale | Small groups | City-scale |
| Disaster-specific | None | Core design |

Briar's anonymity-first design trades performance and scale for unlinkability. For disaster communication where GPS coordinates must be transmitted with SOS messages, anonymity-at-all-costs is counterproductive.

### 4.4 GoTenna

**What it does:** Proprietary LoRa/BLE mesh device + smartphone app.

**Why it's insufficient:**
- **Proprietary hardware:** Requires purchase of GoTenna device (~$179). Cannot use existing smartphones.
- **Proprietary protocol:** Closed protocol, cannot be audited, cannot be independently extended.
- **Not available in India:** US-focused product. LoRa frequencies (915 MHz) conflict with Indian spectrum allocation (865–867 MHz).
- **No edge node architecture:** No concept of dedicated infrastructure nodes.

### 4.5 Summary: The Gap

No existing solution provides:
1. Multi-transport (BLE + Wi-Fi Direct + LoRa + Satellite) on existing smartphones
2. Full DTN store-carry-forward with persistence
3. Emergency priority (P0–P7) with mandatory relay guarantees
4. Open, auditable security with cryptographic identity
5. India-specific deployment (frequency bands, language support, device tier support)
6. Edge node + gateway architecture for scaling
7. iOS + Android + Desktop on a single open protocol

This is the gap IRIS fills.

---

## 5. Quantified Problem Statement

### 5.1 Lives at Risk

**Annual disaster-affected population in India (5-year average):**

| Disaster Type | Annual Affected (avg) | Communication Gap Duration |
|---------------|----------------------|---------------------------|
| Floods | 33 million | 2–14 days per event |
| Cyclones | 5 million | 1–7 days per event |
| Earthquakes | Variable (0–200K major events) | 1–30 days |
| Industrial accidents | 500,000 | Hours to days |
| Landslides | 2 million | 1–14 days |
| **Total at risk annually** | **~40 million** | **Variable** |

### 5.2 Economic Cost of Communication Failure

- **Rescue delay cost:** Each hour of delayed rescue in earthquake scenarios reduces survival probability of trapped victims by 10–15%. Communication failures causing 6-hour rescue delays translate directly to preventable deaths.
- **Coordination overhead:** Without digital communication, emergency responders use runners, radio, and physical travel. NDRF estimates coordination overhead increases 3–5× during communication outages.
- **Economic disruption:** Communication outages in disaster zones extend recovery time for businesses. The 2001 Gujarat earthquake caused ₹21,000 crore in economic losses; communication failures contributed significantly to coordination delays.

### 5.3 Technical Coverage Gap

If IRIS were deployed on 10% of Indian smartphones (75 million devices) in disaster-prone regions:
- Average mesh density in affected areas: 1 device per 200m² (urban), 1 device per 2km² (rural)
- BLE hop chain range: 500m–5km (urban/suburban)
- LoRa gateway coverage: 10–30km radius per gateway
- Store-carry-forward coverage: Anywhere a human goes

**Minimum viable deployment for meaningful coverage:** 50,000 active devices in a district during a disaster event.

This is achievable through smartphone app deployment alone — no new hardware required for the BLE/Wi-Fi layer. LoRa gateways provide rural long-range coverage but are not required for the base case.

---

## 6. Why This Problem Has Not Been Solved

1. **No commercial incentive.** Emergency communication is not a profit center. Telecom companies optimize for normal operations, not edge cases. The 0.001% of time when a major disaster strikes does not drive commercial infrastructure decisions.

2. **Coordination failure.** The solution requires cross-layer coordination (radio hardware, OS APIs, application protocols) that no single commercial actor controls or is incentivized to invest in.

3. **Technical complexity.** Multi-transport DTN with security is genuinely hard. Previous attempts (Bridgefy) took shortcuts that created security vulnerabilities. Doing it correctly requires significant engineering investment.

4. **Regulatory fragmentation.** Different countries have different frequency allocations, encryption laws, and emergency services regulations. A globally deployable solution must navigate this complexity.

5. **The "good enough" fallacy.** When infrastructure works, cellular + internet messaging feels sufficient. The need for IRIS is not visible until infrastructure fails. Investment decisions made during normal times consistently underweight catastrophic-but-infrequent failure scenarios.

IRIS is the answer to all five of these structural failures.
