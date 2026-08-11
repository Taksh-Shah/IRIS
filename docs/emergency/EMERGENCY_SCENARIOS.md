# IRIS Emergency Scenarios

**Document ID:** IRIS-ARCH-EMRG-003  
**Version:** 1.0  
**Status:** Active  

---

## 1. Purpose

This document describes four reference emergency scenarios that IRIS must handle effectively. Each scenario represents a distinct network topology, disaster type, and set of coordination challenges. Scenarios are used for:

- **Simulation testing:** Reference configurations for `tools/sim/` experiments
- **Requirements validation:** Each requirement traces to at least one scenario
- **Stakeholder communication:** Concrete use cases for authority discussions (NDRF, Red Cross)
- **UX design:** Edge cases that must be handled gracefully in the UI

---

## 2. Scenario A: Ahmedabad → Kutch Earthquake (Reference Scenario)

### 2.1 Context

A magnitude 7.6 earthquake strikes the Bhuj–Kutch region of Gujarat, similar in scale to the 2001 earthquake. The epicenter is in rural Kutch district. Cellular towers in a 150 km radius are down. Electrical power is out. Roads are blocked. NDRF teams are staging from Ahmedabad (300 km from epicenter) and moving toward the disaster zone.

This is the **primary reference scenario** for IRIS design decisions.

### 2.2 Network Topology

```
[Ahmedabad NDRF HQ]
  ↕ Satellite (Iridium/Starlink)
[IRIS Cloud Dashboard] ← internet connected
  
[NDRF Convoy A] ─── [NDRF Convoy B]
(Ahmedabad → Bhuj)    (Surendranagar → Bhuj)
  ↕ LoRa 865 MHz         ↕ LoRa
  (15 km range, SF12)
  
[Bhuj District HQ]
  ├─ [Hospital A] (generator power, Wi-Fi)
  ├─ [Aid Station B] (mobile, BLE mesh)
  └─ [Search Team 1..10] (smartphones, BLE mesh)
  
[Village Cluster 1: Rapar] ←─ isolated ─→ [Village Cluster 2: Gandhidham]
   50–200 survivors per village             
   BLE mesh among survivors (50m range)
   Occasional contact with search teams
```

**Phase 1 (0–6 hours):** Complete isolation. No cellular, no internet. NDRF convoys in transit. Survivors relay SOS among themselves via BLE mesh.

**Phase 2 (6–24 hours):** NDRF arrives in Bhuj. LoRa links established to outlying areas. Satellite uplink at NDRF HQ. Multi-hop routing from villages to NDRF.

**Phase 3 (24–72 hours):** Cellular towers partially restored. Internet connectivity intermittent. IRIS operates in hybrid mode (mesh + cellular).

### 2.3 Node Types

| Node Type | Count | Transports | Role |
|-----------|-------|-----------|------|
| Survivor (smartphone) | 2,000–5,000 | BLE, Wi-Fi Direct | SOS, location, status |
| NDRF search team member | 100–200 | BLE, Wi-Fi Direct, LoRa | Coordination, relay |
| NDRF vehicle gateway | 10–20 | BLE, Wi-Fi Direct, LoRa, Satellite | Gateway, authority |
| Hospital gateway | 5 | Wi-Fi Direct, LoRa, Cellular (degraded) | Medical coordination |
| RPi Zero relay node | 20–50 (pre-positioned) | BLE, Wi-Fi Direct, LoRa | Relay, store-forward |

### 2.4 Message Flow (Phase 1)

```
1. Survivor Ravi (village A) trapped under rubble
   → Sends P0 SOS: "Trapped. House collapsed. 5 people. GPS: 23.1°N 70.0°E"
   → BLE broadcast to nearby survivors (15 nodes in range)
   
2. Survivor Priya (20m away) receives SOS
   → Her IRIS stores SOS in relay store (carry-forward)
   → She is walking toward the road (toward NDRF)
   
3. After 45 minutes, Priya meets NDRF search team at road
   → BLE contact: SOS relayed from Priya to NDRF device
   → NDRF device has LoRa: SOS relayed to Bhuj HQ
   → Bhuj HQ has Satellite: SOS uploaded to IRIS Cloud
   
4. NDRF HQ receives SOS, dispatches team to GPS coordinates
```

### 2.5 Message Flow (Phase 2 — LoRa Mesh Active)

```
Hospital needs blood supply (P1 Medical):
  [Hospital A] → [LoRa] → [NDRF HQ]
  Latency: ~30 seconds (1 hop LoRa)
  
NDRF HQ broadcasts evacuation route (P3 authority broadcast):
  [NDRF HQ] → [LoRa gateway] → BLE broadcast to all nodes in range
  → DTN carry-forward to villages not yet reached
  Latency: 2–8 hours to reach outlying villages
```

### 2.6 Expected Delivery Ratios

| Message Type | Phase 1 (DTN only) | Phase 2 (LoRa+DTN) | Phase 3 (Hybrid) |
|-------------|-------------------|--------------------|--------------------|
| P0 SOS (to NDRF) | 70–85% within 6h | 95% within 2h | 99% within 30min |
| P1 Medical (to hospital) | 60–75% within 12h | 90% within 4h | 98% within 15min |
| P2 Location | 65–80% | 90% | 99% |
| P3 Authority broadcast (to survivors) | 50–70% within 24h | 85% within 8h | 95% within 2h |

**Rationale for Phase 1 gap:** In 0–6 hours, NDRF has not yet arrived and the BLE mesh has limited reach. Many villages are isolated clusters of 20–50 nodes. The SOS delivery ratio of 70–85% reflects that some villages may have no path to NDRF until physical contact is established.

---

## 3. Scenario B: Mumbai Flash Flood, Coastal Evacuation

### 3.1 Context

Cyclone-driven storm surge inundates low-lying areas of Mumbai (Dharavi, Bandra east, Kurla). 300,000 people in the affected zone must evacuate within 6 hours. Cellular networks are operational but overloaded (calls failing, SMS delayed 30+ minutes). IRIS mesh provides local coordination where cellular is unavailable or overloaded.

### 3.2 Network Topology

```
[MCGM (Mumbai civic) Emergency Control] — Cellular/Internet — [IRIS Cloud]
  |
  |—— [Satellite Gateway @ BKC]
  |—— [SDRF Maharashtra Regional HQ]
  
[Evacuation Route Alpha: Dharavi → Sion Elevated Road]
  ↕ BLE mesh among pedestrians
  
Dense node cluster: 2,000 IRIS users in 500m evacuation corridor
  ↕ Wi-Fi Direct for voice messages
  ↕ BLE for SOS and location
  
[Marine Drive Assembly Point] ← [BLE relay chain 3km]
[Bandra Terminus] ← [BLE relay chain 5km]
```

**Key challenge:** Urban RF environment (concrete buildings, metal structures) reduces BLE effective range to 20–30m. 3 km of relay chain requires 100–150 intermediate nodes minimum.

### 3.3 Urban RF Considerations

Mumbai's dense urban environment creates significant RF challenges:
- BLE: 20–30m effective range (vs 50m in open air)
- Wi-Fi Direct: 50–100m effective range (vs 150m open air)
- LoRa: 500m–2km effective range from elevated relay nodes

IRIS deployment for this scenario requires:
- Pre-positioned LoRa relay nodes on elevated buildings (existing flood safety equipment)
- Dense BLE mesh along evacuation routes
- Crowd mode active throughout (> 100 peers visible at any point)

### 3.4 Message Flow

```
1. SDRF issues evacuation order (P0 authority broadcast, all channels):
   → BLE flood in Dharavi cluster: received by 80% of nodes within 5 min
   → LoRa relay: reaches elevated nodes quickly
   → DTN carry-forward: remaining 20% receive within 30 min as mesh fills in
   
2. Person stranded on flooded roof sends SOS (P0):
   → BLE broadcast to adjacent buildings
   → Relay via LoRa to SDRF helicopter coordination
   → Helicopter diverted
   → Receipt confirmation returned via LoRa + BLE
   
3. Evacuation route blocked (tree down, road flooded):
   → SDRF posts P3 route update: "Gokhale Road blocked, use Elphinstone Rd"
   → Propagates via BLE mesh from update-point outward
   → 60% coverage within 10 min, 90% within 30 min along corridor
```

### 3.5 Expected Delivery Ratios

| Message Type | Coverage Target | Expected Latency |
|-------------|----------------|-----------------|
| P0 SOS (from flooded roof to SDRF) | 95% | < 5 minutes |
| P0 Evacuation order (SDRF to all) | 95% within 1 hour | 5–45 minutes |
| P3 Route update | 85% within 30 min | 5–30 minutes along corridor |
| P2 Location (person sharing for rescue) | 90% direct delivery | 2–10 minutes |

**Crowd mode note:** With 2,000 nodes in 500m radius, crowd mode is Extreme. P3 message rate is limited to 1 per device per 5 minutes. SDRF (authority node) has higher rate limits: 10 per hour for P3.

---

## 4. Scenario C: Delhi Air Quality Emergency, Hospital Coordination

### 4.1 Context

Severe air quality event in Delhi (AQI > 500, hazardous). Mass hospitalizations of at-risk individuals (asthma, COPD, elderly). Hospitals reach capacity. The IRIS network is used for medical resource coordination between hospitals, triage centers, and AIIMS emergency command.

This scenario tests **P1 Medical** message delivery and authority-coordinated resource management. Cellular is operational but hospital-to-hospital communication is fragmented across different systems.

### 4.2 Network Topology

```
[AIIMS Emergency Command] — Internet — [IRIS Cloud Dashboard]
  |
  ├─ Wi-Fi Direct (300m range AIIMS campus)
  ├─ Cellular (operational)
  └─ IRIS mesh to nearby hospitals
  
[RML Hospital] — [Safdarjung Hospital] — [GTB Hospital]
  |                    |                    |
  BLE relay            BLE relay            BLE relay
  (along roads)        (along roads)        (along roads)
  
[Community Triage Points × 20]
  ↕ BLE mesh among patients and volunteers
  ↕ Wi-Fi Direct to hospital relay nodes
```

**Key difference from Scenarios A/B:** Cellular is OPERATIONAL in this scenario. IRIS provides an alternative, lower-latency path for structured medical data and coordinates among hospitals that lack direct integration.

### 4.3 IRIS Node Types

| Node Type | Count | Primary Use |
|-----------|-------|------------|
| Hospital gateway (tablet) | 30 | Medical resource bundles |
| Triage point tablet | 20 | Patient intake logging |
| Volunteer smartphone | 200 | Relay, location, general comm |
| Patient smartphone | 2,000+ | SOS if acute distress |

### 4.4 Message Flow

```
1. GTB Hospital ICU reaches capacity:
   → P1 Medical bundle: "GTB ICU FULL. Divert ventilator patients to Lok Nayak."
   → Sent to AIIMS Emergency Command (direct via cellular)
   → Also injected into IRIS mesh for redundancy
   → Received by all hospital gateways within 5 minutes
   
2. AIIMS allocates 5 additional oxygen concentrators to Safdarjung:
   → P1 Resource Allocation bundle: "5 O2 concentrators allocated, truck ETA 45 min"
   → Encrypted direct delivery to Safdarjung gateway
   → Delivery confirmed via receipt
   
3. Patient at triage point develops acute respiratory distress:
   → P0 SOS from patient's phone or volunteer's device
   → Relayed via BLE mesh to nearest hospital gateway
   → Hospital dispatches ambulance (within 5 minutes SOS receipt)
   
4. AIIMS broadcasts updated triage protocol to all points (P3 authority):
   → All triage tablets receive updated protocol within 10 minutes
   → DTN carry-forward ensures coverage even for triage points without
     direct Wi-Fi contact
```

### 4.5 Expected Delivery Ratios

| Message | Target | Mechanism |
|---------|--------|-----------|
| P1 Hospital-to-Hospital resource | 99% within 5 min | Cellular primary + IRIS mesh backup |
| P0 SOS from triage point | 99% within 2 min | BLE relay to hospital gateway |
| P3 Protocol update to all triage | 90% within 15 min | Wi-Fi Direct + BLE relay |
| P1 Resource allocation (direct) | 99% within 3 min | Wi-Fi Direct direct delivery |

**Note on cellular:** When cellular is available, IRIS uses it as an additional transport (if the device has a data plan). IRIS bundles are transmitted over cellular as standard IP packets via the cellular transport adapter. This does not increase cost for users with unlimited plans; for prepaid users, IRIS estimates cellular data usage and warns at > 10 MB.

---

## 5. Scenario D: Border Area Conflict, Civilian Communication

### 4.1 Context

Conflict in a border area results in civilian displacement. Internet and cellular are disabled or monitored. Civilians need to communicate evacuation status, coordinate safe passage, and send SOS. This scenario is distinct from the others: the adversary model includes a state-level actor with capability to monitor RF communications.

**Important constraint:** This scenario is in scope for IRIS communication capabilities but is NOT in scope for geopolitical comment. IRIS provides communication infrastructure; the political context is the deploying organization's responsibility.

### 5.2 Network Topology

```
[UN/Red Cross field camp] — Satellite (Iridium) — [External coordination]
  |
  ├─ BLE mesh: NGO workers (50 nodes)
  ├─ LoRa: long-range to scattered civilians (15 km range)
  └─ Wi-Fi Direct: within camp (short range, high bandwidth)
  
[Displaced civilians cluster A] — [Displaced civilians cluster B]
  |                                    |
  BLE mesh (30–80 nodes)               BLE mesh (20–60 nodes)
  Occasional LoRa contact with field camp
  
[Isolated family groups]
  1–5 nodes, LoRa if near field camp LoRa repeater
```

**Connectivity model:** Highly intermittent. Some clusters may be out of range for 12+ hours. LoRa is the primary inter-cluster transport. Satellite provides external uplift only (not intra-cluster).

### 5.3 Security Considerations

In this scenario, encryption and anonymity are more critical than in peacetime disaster response:

- **End-to-end encryption:** All personal messages encrypted (standard IRIS). Relay nodes see only ciphertext.
- **NodeId pseudonymity:** NodeIds are BLAKE3 hashes; not directly linkable to identity without correlation.
- **Location sharing:** Strongly advised against unless necessary. IRIS UI in this scenario should warn: "Sharing your location in a conflict area may be dangerous. Only share with trusted contacts."
- **Authority verification:** UN/Red Cross certificates are in the IRIS trust chain. Civilian nodes can verify UN broadcasts without sharing their own identity.
- **RF emission:** IRIS BLE advertising is detectable by RF monitoring. In high-risk environments, users can enable "silent mode" — reduces advertisement frequency to once per 5 minutes, significantly reducing RF detectability while maintaining SOS capability.

### 5.4 Message Flow

```
1. Family separated in displacement:
   → Father (in cluster A) sends P4 message to daughter (cluster B)
   → NodeId-addressed, encrypted direct delivery
   → Store-carry-forward: IRIS worker carries it to cluster B on next visit
   → Latency: 2–24 hours (depending on next contact between clusters)
   
2. Red Cross identifies medical emergency at cluster B:
   → P1 Medical bundle to field camp: "Child, 8yo, suspected appendicitis"
   → LoRa relay to field camp (if in range)
   → Field camp: satellite uplink to external medical coordination
   → Decision to evacuate received back via satellite + LoRa within 30 min
   
3. Safe passage route announced by UN (P3 authority broadcast):
   → LoRa broadcast from field camp
   → BLE carry-forward within each cluster
   → Coverage: 80% within 6 hours, 95% within 24 hours
   
4. Person captured by hostile actor:
   → P0 SOS sent before capture
   → Relayed via BLE to nearest IRIS node
   → LoRa relay to field camp
   → Satellite uplink to external authorities
   → Location included: GPS if available
```

### 5.5 Expected Delivery Ratios

| Message | Target | Expected Latency | Notes |
|---------|--------|-----------------|-------|
| P0 SOS to Red Cross field camp | 85% | 30 min – 4 hours | Highly dependent on LoRa coverage and cluster proximity |
| P1 Medical to field camp | 80% | 1–6 hours | Delayed by carry-forward without LoRa link |
| P3 UN authority broadcast | 75% within 24h | 6–24 hours | Multi-hop carry-forward |
| P4 Personal message (cross-cluster) | 60% within 48h | 4–48 hours | Dependent on human mobility between clusters |

**Lower targets rationale:** The disconnected topology (clusters separated by > LoRa range, irregular human mobility patterns) fundamentally limits delivery ratios. 85% P0 delivery in a conflict-area DTN scenario is the target; this is significantly lower than urban disaster scenarios because the network is more fragmented and adversarial conditions constrain RF emission.

---

## 6. Cross-Scenario Analysis

| Factor | Scenario A | Scenario B | Scenario C | Scenario D |
|--------|-----------|-----------|-----------|-----------|
| Cellular | Unavailable | Overloaded | Available | Unavailable/monitored |
| Internet | Unavailable | Available (degraded) | Available | Unavailable |
| Node density | Low–Medium | Very High | Medium | Low |
| Primary transport | BLE+LoRa | BLE+Wi-Fi Direct | All transports | BLE+LoRa |
| Partition duration | 0–72 hours | Hours | Minutes–Hours | Days–Weeks |
| Security threat level | Low | Low | Low | High |
| P0 delivery target | 90–99% | 95% | 99% | 85% |
| Key IRIS feature | Store-carry-forward | Crowd management | Medical P1 | E2E encryption + silent mode |

---

## 7. Simulation Configurations

Each scenario has a corresponding simulation configuration file:

```
tools/sim/scenarios/
├── earthquake_kutch.toml       # Scenario A
├── mumbai_flood_evacuation.toml # Scenario B
├── delhi_aq_hospital.toml      # Scenario C
└── border_conflict.toml         # Scenario D
```

Run with:

```bash
./tools/sim/run_scenario.sh --scenario earthquake_kutch --duration 86400
```

Scenarios are parameterized to allow sensitivity analysis (e.g., varying node count, connectivity thresholds, and message generation rates) without changing the fundamental topology model.
