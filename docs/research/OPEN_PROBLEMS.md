# Open Problems

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research  

---

## Purpose

This document catalogs problems that IRIS has not yet solved — not for lack of effort, but because the problems are genuinely hard and no satisfactory solution exists in the literature. Each entry describes why the problem is hard, what partial solutions exist, and what IRIS does today as a pragmatic substitute.

Open problems differ from research gaps (RESEARCH_GAPS.md): a research gap has a known experimental approach that would close it. An open problem has no known solution methodology — it requires fundamental research or a novel approach.

---

## OP-001: Sybil Resistance in Fully Decentralized DTN Without PKI

**Status:** Open  
**Impact:** Security — affects trustworthiness of PRoPHET delivery probability estimates  
**References:** Douceur (2002) "The Sybil Attack"; Newsome et al. (2004) "The Sybil Attack in Sensor Networks"

### Why It Is Hard

A Sybil attack occurs when one physical entity controls multiple virtual identities, allowing it to subvert reputation systems by manipulating its own apparent trustworthiness. In IRIS's PRoPHET routing:

- PRoPHET delivery probability P(a,b) is based on contact history
- A malicious node can create many identities and inflate its apparent delivery probability
- High P → other nodes forward bundles to the attacker → attacker can selectively drop or surveil bundles
- Without PKI (Certificate Authority), there is no authority to bind an identity to a physical entity

The fundamental hardness: in a fully decentralized system, any identity can be generated arbitrarily by any party. There is no "proof of person" equivalent in the physical layer.

**Known partial solutions:**

1. **Resource-based identity (Proof of Work):** Identity creation requires solving a computational puzzle. Cost: increases battery drain on first use; sophisticated attackers with many devices bypass it; does not prove a unique person.

2. **Social graph attestation:** Each identity requires attestation by N existing members. Cost: bootstrapping problem; isolates new users; doesn't work for first responders who don't know each other.

3. **Physical layer identity binding:** LoRa nodes have hardware identifiers (DevEUI); BLE has Bluetooth Device Address. These are MAC addresses, which are spoofable at the software layer.

4. **Reputation decay:** Penalize identities that fail to deliver at their claimed probability. Cost: slow to detect; attacker can selectively fail (deliver most, drop targeted bundles).

5. **Peer observation:** Multiple nodes independently measure contact probability and cross-check. Cost: effective only when honest nodes are in majority; attacker can partition the honest network.

### What IRIS Does Today

IRIS accepts a weaker threat model for v1: adversary is assumed to be passive (eavesdropper) not active (Sybil attacker). Rationale for disaster scenarios: in a disaster, the primary threat is network failure, not active adversaries.

IRIS identity is Ed25519 public key hash. Key generation is fast (no proof-of-work). Sybil attacks are possible in IRIS v1. The mitigation is: IRIS operator mode (government/NGO deployments) uses pre-provisioned identity keys distributed out-of-band.

**Open question:** Can physical layer characteristics (RF fingerprinting of BLE/LoRa transmitters) provide hardware-bound identity without PKI? Promising but not production-ready.

---

## OP-002: Optimal Routing in Highly Partitioned Indian Rural Terrain

**Status:** Open  
**Impact:** Delivery ratio in priority-2 scenarios (Himalayan, island, desert)  
**References:** Chaintreau et al. (2007) "Impact of Human Mobility on Opportunistic Forwarding Algorithms"

### Why It Is Hard

PRoPHET and Spray-and-Wait are derived under mobility assumptions that do not hold in highly partitioned environments:
- Random waypoint mobility: contacts are roughly Poisson-distributed
- Homogeneous node density: similar encounter rates across all node pairs

Indian rural disaster environments frequently violate both:
- **Andaman & Nicobar Islands:** Population on ~570 islands; contact between islands is ferry-dependent (1 contact/day or less). PRoPHET cannot estimate useful delivery probabilities with so few contact events.
- **Himalayan villages:** Settlements separated by high passes; contact is seasonal (passes snow-covered in winter). Node pairs have contact probability effectively zero for 6 months.
- **Thar Desert:** Low density (1–2 people/km²); random contact is rare; mobility follows predictable routes (known roads and water sources).

In these environments, PRoPHET's encounter-frequency model converges on P(a,b) ≈ 0 for most node pairs, making routing decisions essentially random. Spray-and-Wait with L copies is no better than epidemic when the network has fewer than L nodes.

**Known partial solutions:**

1. **Schedule-aware routing:** If node movement is periodic/predictable (ferry schedules, daily commutes), encode the schedule. DTN literature has schedule-aware routing (DTLSR, EDDL). Cost: requires accurate schedule data, which is not available during disasters.

2. **Social-aware routing:** Route through socially connected nodes (family, community leaders) who are likely to eventually contact destination. Cost: requires social graph, which IRIS does not collect for privacy reasons.

3. **Geographic routing with mobility prediction:** Use GPS trajectory to predict next location. Cost: GPS is power-hungry; prediction fails after evacuation (destination unknown).

4. **Utility functions with geographic component:** Route to nodes geographically closer to destination. Cost: requires destination's known location; location may be stale or unknown.

### What IRIS Does Today

IRIS uses PRoPHET with Spray-and-Wait fallback. In highly partitioned environments, delivery ratio will be lower than in urban scenarios. IRIS documentation for deployment in rural contexts should warn operators of reduced delivery guarantees.

**Open question:** Can satellite uplink (Iridium SBD) serve as a "super-node" in highly partitioned scenarios, connecting otherwise-isolated communities? This is the motivation for IRIS satellite fallback in the roadmap, but it does not solve the routing problem — it sidesteps it.

---

## OP-003: Privacy-Preserving Routing (Hiding the Contact Graph)

**Status:** Open  
**Impact:** Privacy — contact graph reveals social relationships  
**References:** Lu et al. (2010) "Privacy-preserving opportunistic routing in delay tolerant networks"

### Why It Is Hard

PRoPHET routing requires nodes to share their delivery probability vector with each encountered node (for transitivity calculation). This is a metadata leak:
- If node A shares P(A,B) = 0.9, a passive observer knows A and B have frequent contact
- Contact frequency reveals social relationships, daily routines, and associations
- In an activist or minority-rights context, this is dangerous information

The fundamental tension: routing efficiency requires sharing contact metadata; privacy requires hiding contact metadata. These goals directly conflict.

**Known partial solutions:**

1. **Garlic/onion routing:** Encrypt routing metadata so no single relay node knows both source and destination. Cost: significant overhead; not designed for DTN (requires path knowledge up front).

2. **Bloom filter representation:** Encode contact history as a Bloom filter rather than exact P values. Reduces precision of metadata leak but does not eliminate it.

3. **Private Information Retrieval (PIR):** Allow a node to query contact history without revealing what it is querying for. Cost: computationally expensive (polynomial-time but high constant); impractical on constrained devices.

4. **Differential privacy on P values:** Add calibrated noise to P(a,b) before sharing. Cost: reduces routing accuracy proportionally to privacy budget; hard to tune the tradeoff.

5. **Oblivious routing:** Route without contact metadata; use random walk or epidemic. Cost: reverts to epidemic overhead.

### What IRIS Does Today

IRIS leaks contact metadata to all nodes encountered during transitivity exchange. This is documented in the threat model (see `docs/security/THREAT_MODEL.md`). The threat model states: IRIS provides message content confidentiality and integrity but does not provide contact graph privacy.

For government and NGO use cases (the primary IRIS target), contact graph privacy is less critical than delivery reliability. The open problem is acknowledged; no solution is deployed.

---

## OP-004: Energy Harvesting Sufficiency for Permanent Deployment

**Status:** Open  
**Impact:** Infrastructure deployment viability for permanent rural relay nodes  
**References:** Kansal et al. (2007) "Power Management in Energy Harvesting Sensor Networks"

### Why It Is Hard

A permanent IRIS relay node (RPi-based, solar-powered) must balance:
- Solar input (intermittent, weather-dependent, seasonal)
- LoRa receive + transmit current (~15–150 mA depending on duty cycle)
- BLE advertisement current (~5 mA continuous)
- RPi compute current (~300–600 mA)
- Battery storage (finite; degradation over 2–3 years)

Indian deployment environments:
- **Himalayan high-altitude:** High irradiance but extreme cold (battery performance degrades below 0°C; LiFePO4 better than Li-Ion but still degraded)
- **Monsoon seasons:** 30–45 consecutive cloudy days in northeast India (Meghalaya, Assam); solar input near zero
- **Coastal:** High humidity, salt air; accelerated corrosion of solar panels and electronics

Published energy harvesting models (Kansal 2007, Hester 2017) are validated for temperate environments. Himalayan winter + monsoon cycles are not covered.

The fundamental hardness: monsoon duration exceeds typical battery autonomy for a solar + RPi system without very large battery banks (cost and weight prohibitive).

**Known partial solutions:**

1. **Aggressive duty cycling:** Sleep RPi when no contact for N minutes; reduce LoRa listen duty cycle. Cost: latency in relay wakes up slowly when a node arrives.

2. **Seasonal operation policy:** Deploy relay nodes that go offline during monsoon; document expected outage windows. Cost: availability reduction; reduces value during flood season (when most needed).

3. **Hybrid power:** Solar + small wind turbine for coastal/Himalayan sites. Cost: significantly higher hardware cost; moving parts failure.

4. **Low-power hardware alternatives:** Replace RPi4 with ESP32 or Nordic nRF9160 (10–100x lower idle power). Cost: reduced compute for ML routing; less storage for bundle store.

### What IRIS Does Today

IRIS hardware kit specification recommends solar panel sizing based on worst-case monsoon for the deployment region, with battery autonomy ≥ 7 days. This is a design guideline, not a validated specification. The open problem is energy harvesting sufficiency in extended zero-solar periods.

---

## OP-005: Legal Gray Area of Unlicensed LoRa for Emergency Communication

**Status:** Open (with partial answer)  
**Impact:** Legal risk — affects deployment authorization  
**References:** LEGAL_RESEARCH.md LQ-001; COMPLIANCE_RISK_REGISTER.md CR-001

### Why It Is Hard

India WPC de-licenses 865–867 MHz for ISM use. "ISM" (Industrial, Scientific, Medical) use is defined in WPC notifications. Emergency communication mesh networking is arguably:
- **ISM use:** It uses RF for communication (similar to remote controls, RFID, industrial sensors)
- **Not ISM use:** It is a communication network, which historically requires licensing under the Telegraph Act

The Telegraph Act 1885 (as amended) defines "telegraph" broadly. Section 3(1AA): "telegraph" means any appliance, instrument, material or apparatus used or capable of use for transmission or reception of signs, signals, writing, images and sounds or intelligence of any nature by wire, visual or other electro-magnetic emissions, Radio waves or Hertzian waves, galvanic, electric or magnetic means.

A LoRa mesh node clearly fits this definition. Section 4 of the Telegraph Act requires a license to "establish, maintain, work, manage, control, or connect" a telegraph. WPC de-licensing exempts the *spectrum use* but does not explicitly exempt *establishing a telegraph*.

**Why there is no clean answer:**
- The WPC de-licensing notification does not explicitly address whether ISM-band use constitutes "establishing a telegraph"
- DoT legal opinions on this question are not publicly available
- No precedent case law exists for LoRa mesh specifically in India
- The government's interest in this is mixed: they want emergency communication infrastructure but also want to control communication networks

**Partial answers:**

1. **Narrow read (permissive):** WPC de-licensing implicitly authorizes the telegraph use within the specified band and power limits; otherwise de-licensing would be meaningless.

2. **Broad read (restrictive):** De-licensing authorizes spectrum but not the communication service; a telegraph license is still required.

3. **Emergency exemption:** Disaster Management Act 2005, Section 65 grants powers to the National Executive Committee to take measures for emergency communication. This may provide a basis for authorized use by NDRF/SDRF without individual licenses.

### What IRIS Does Today

IRIS legal counsel has been engaged to obtain a formal opinion on this question. Until a formal opinion is received:
- IRIS does not claim LoRa relay constitutes a licensed activity
- IRIS does not claim LoRa relay is definitely unlicensed
- IRIS recommends government pilot deployments be conducted under NDRF authority, which has broader emergency communication powers
- Commercial deployments will wait for legal clarity

See COMPLIANCE_RISK_REGISTER.md CR-001 for risk assessment and LEGAL_RESEARCH.md LQ-001 for research status.

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document — 5 open problems recorded |
