# IRIS Use Cases

**Document ID:** IRIS-UC-001  
**Version:** 1.0

Priority levels: P0 (critical life-safety) → P7 (lowest). Use case priority indicates the importance of IRIS supporting this scenario.

---

## UC-01: Earthquake — Building Collapse — SOS

**Priority:** P0 (highest)  
**Category:** Disaster Response

**Actors:**  
- Victim: trapped survivor with IRIS-enabled device  
- Relay: nearby devices carried by other survivors or responders  
- Coordinator: NDRF coordination center with IRIS gateway

**Scenario:**  
An earthquake collapses a multi-story residential building in a densely populated area. Three people are trapped on the 4th floor. All cellular connectivity is lost. One survivor has 23% battery remaining. She activates SOS.

**Constraints:**  
- Device battery: 23% (must preserve for continued operation)  
- Connectivity: BLE only (Wi-Fi disabled to save power)  
- Physical: Device may be underground, signal attenuated  
- Time: Golden hour — first 72 hours are critical for survival

**IRIS Behavior:**  
1. SOS message created with GPS coordinates, battery level, number of people, injury description
2. P0 priority: all relay devices must forward immediately; no drop policy
3. Message propagates via BLE hop chain through the disaster zone
4. Edge nodes and vehicle relays carry message to rescue coordination teams
5. Delivery receipt sent back when confirmed received by responder

**Success Condition:**  
P0 SOS message reaches emergency responder within 15 minutes. GPS coordinates are accurate. Message is not lost even if originating device battery dies after transmission.

---

## UC-02: Flood — Village Isolation — Group Coordination

**Priority:** P0  
**Category:** Disaster Response

**Actors:**  
- Village residents (50–200 people)  
- Village head / designated coordinator  
- District disaster management authority  
- NGO relief teams approaching by boat

**Scenario:**  
Heavy monsoon flooding isolates a village in Assam. All roads are submerged. The single cell tower 8 km away is flooded. The village head has IRIS on his phone. A flood monitoring NGO has pre-deployed an IRIS edge node (solar-powered, with LoRa) at the district headquarters.

**Constraints:**  
- No cellular, no internet for 72+ hours  
- Village has 15 IRIS-enabled phones  
- LoRa gateway is 12 km away  
- Edge node has 30-day battery backup (solar + LiFePO4)

**IRIS Behavior:**  
1. Village devices form local BLE/Wi-Fi Direct mesh automatically
2. Village head sends P1 medical emergency for 3 people with snakebites (requires antivenin)
3. Message stored at village; when flood water allows a boat to approach closer, message relays via LoRa to district HQ
4. District HQ receives medical priority list, dispatches boat with antivenin
5. Relief team approaching by boat has IRIS running — as they come within LoRa range, bidirectional communication established

**Success Condition:**  
Coordination messages between village and district travel via LoRa within 20 minutes of LoRa range being established. Medical needs are communicated before physical contact.

---

## UC-03: Cyclone — Evacuation Coordination

**Priority:** P0  
**Category:** Disaster Response

**Actors:**  
- Coastal population (thousands)  
- State government evacuation teams  
- Coast guard  
- Cyclone shelter managers

**Scenario:**  
Cyclone Amphan-class (Category 5) is approaching Odisha coast. Evacuation is underway. Communication is being disrupted 12 hours before landfall. Evacuation teams lose contact with coastal hamlets.

**Constraints:**  
- Time critical: 12-hour window before landfall  
- Geographic: 300 km of coastline  
- Population: 500,000 in evacuation zone  
- Infrastructure: Cell towers already losing power to storm surge

**IRIS Behavior:**  
1. Emergency broadcast sent by state authority via IRIS: evacuation route, shelter locations
2. P0 broadcast propagates through the entire coastal mesh
3. Evacuation team vehicles (trucks, boats) running IRIS act as mobile relays
4. People in shelters can communicate their arrival to coordinate transport allocation
5. Missing persons messages (P2 location) allow families to find each other at shelters

**Success Condition:**  
Evacuation coordination messages reach 90% of IRIS-enabled devices in the evacuation zone before landfall. Shelter census data is accurately communicated to coordinating authority.

---

## UC-04: Women's Safety — Personal SOS

**Priority:** P0  
**Category:** Women's Safety

**Actors:**  
- Woman in danger (victim)  
- Nearby bystanders with IRIS  
- Registered trusted contacts  
- Emergency services

**Scenario:**  
A woman in a moving vehicle senses danger (potential assault situation). She covertly triggers a silent SOS in IRIS. The app captures GPS coordinates and starts transmitting location updates every 30 seconds.

**Constraints:**  
- Silent operation required: no sound, no visible SOS screen (app appears normal)  
- Covert: perpetrator must not see SOS was triggered  
- GPS updates must continue as device moves  
- May transition in and out of cellular coverage

**IRIS Behavior:**  
1. Silent SOS triggered via shake gesture, volume button sequence, or wearable integration
2. P0 SOS with GPS + location update stream sent to pre-configured trusted contacts via all available transports
3. If cellular available: direct to trusted contacts via internet
4. If no cellular: via BLE/Wi-Fi mesh to any IRIS relay; stored for delivery when connectivity restores
5. Nearby community members (anonymous relay) carry the message without seeing its content (encrypted)
6. Trusted contacts receive real-time location track
7. If no confirmation from victim within 30 minutes: automatic escalation to secondary contacts

**Success Condition:**  
Trusted contact receives SOS and location track within 2 minutes. Location updates continue at 30-second intervals. Perpetrator is not alerted to SOS transmission.

**Privacy Note:**  
Location data in SOS is encrypted to recipient public keys only. Relay nodes carry the message but cannot read GPS coordinates.

---

## UC-05: Campus Safety — Active Threat

**Priority:** P0  
**Category:** Campus Safety

**Actors:**  
- Students and faculty  
- Campus security  
- Police

**Scenario:**  
An active threat situation (armed intruder) at a university campus. Campus security needs to coordinate evacuation and lockdown. Police are responding. Cellular networks are congested with 10,000 students calling simultaneously.

**Constraints:**  
- Cellular network: congested, calls failing  
- Time: minutes matter  
- Scale: 10,000+ people on campus  
- Coordination: security must direct movement without creating panic

**IRIS Behavior:**  
1. Campus security sends emergency broadcast via IRIS: threat location, evacuation routes, lockdown areas
2. Emergency broadcast propagates campus-wide in under 60 seconds via BLE mesh (campus buildings have natural relay density)
3. Students in lockdown can send P1 safety-confirmation messages without making phone calls
4. Security team receives real-time grid of "confirmed safe" check-ins
5. Police can join campus IRIS network via credentials and receive security team communications

**Success Condition:**  
Emergency broadcast reaches 95% of IRIS-enabled devices on campus within 60 seconds. Security team has bi-directional communication with police despite cellular congestion.

---

## UC-06: Crowd Management — Large Public Event

**Priority:** P1  
**Category:** Crowd Safety

**Actors:**  
- Event attendees (10,000–100,000)  
- Event management  
- Police crowd control  
- Medical teams

**Scenario:**  
Kumbh Mela scale event. Cellular networks are completely saturated. Medical emergency in the crowd. Medical team cannot communicate with the control center 500m away through the congested cellular network.

**Constraints:**  
- Cellular: completely saturated (no capacity)  
- Internet: effectively unavailable  
- Physical: extreme density (8 people/m² in some areas)  
- Time: medical emergencies are time-critical

**IRIS Behavior:**  
1. Medical team sends P1 medical priority via IRIS (bypasses congested cellular entirely)
2. BLE mesh through dense crowd: 8 people/m² means relay density is extremely high
3. Message reaches event medical coordination in under 30 seconds
4. Coordination team directs nearest medical unit via IRIS bidirectional channel
5. Crowd flow messages (P3): security can redirect crowd movement via mesh broadcast

**Success Condition:**  
Medical emergency message reaches coordination center in under 30 seconds despite 100% cellular saturation. Bidirectional coordination maintained.

---

## UC-07: Search and Rescue — Missing Person in Forest

**Priority:** P1  
**Category:** Search and Rescue

**Actors:**  
- Missing hiker  
- Search and rescue team  
- Forest department coordination

**Scenario:**  
A trekker goes missing in a forest reserve in Uttarakhand. No cellular coverage in the forest. The trekker has IRIS with a pre-configured mesh profile. SAR teams deploy with IRIS-enabled devices and a portable LoRa gateway.

**Constraints:**  
- No cellular throughout forest  
- Dense vegetation attenuates BLE significantly (50–100m range)  
- SAR team covers large area in multiple groups  
- Battery: trekker's phone battery at 12%

**IRIS Behavior:**  
1. Trekker activates SOS periodically (to conserve battery: transmit every 5 minutes)
2. Each SOS includes GPS + battery level + brief status
3. SAR teams form a moving mesh: as they sweep, their range extends into the forest
4. LoRa gateway (carried by base team at trailhead) extends range 15km into forest
5. When any SAR team member comes within BLE range of trekker: message relay established
6. Trekker receives direction/response from SAR: "stay in place, team 2 is 200m north"

**Success Condition:**  
Trekker's location is identified by SAR team via IRIS before battery failure. SAR-to-trekker communication established when within range.

---

## UC-08: Medical Emergency — Remote Area

**Priority:** P1  
**Category:** Medical Emergency

**Actors:**  
- Patient  
- Local health worker (ASHA worker, PHC nurse)  
- District hospital  
- Telemedicine doctor

**Scenario:**  
A pregnant woman in a tribal village in Chhattisgarh has a high-risk complication. The village ASHA worker has IRIS. The primary health center is 25 km away. No cellular coverage. IRIS edge node deployed at PHC.

**Constraints:**  
- No cellular coverage  
- LoRa is the only long-range transport available  
- Patient data (photos, vitals) too large for LoRa alone  
- Time: obstetric emergency

**IRIS Behavior:**  
1. ASHA worker sends P1 medical message with structured data: symptoms, vitals, gestational age
2. Message travels via LoRa to PHC edge node (25 km range)
3. PHC doctor receives structured message, responds with treatment guidance
4. If ASHA worker gets vehicle access to hilltop with partial cellular: IRIS bridges to internet, sends photos via P5 (image priority)
5. Ambulance dispatched with pre-notification of patient status

**Success Condition:**  
Medical advice reaches ASHA worker within 10 minutes via LoRa. Patient receives treatment guidance before ambulance arrives.

---

## UC-09: Infrastructure Failure — Power Grid Collapse

**Priority:** P1  
**Category:** Infrastructure Emergency

**Actors:**  
- Utility company operations center  
- Field engineers  
- Affected population  
- Government coordination

**Scenario:**  
Major power grid failure affecting 5 states (like the 2012 India power failure). Cell towers on backup power beginning to fail. Utility companies need to coordinate field teams for restoration.

**Constraints:**  
- Progressive connectivity loss over 12–24 hours as backup power depletes  
- Field engineers in geographically dispersed locations  
- Coordination requires bidirectional communication  
- No time to deploy special hardware

**IRIS Behavior:**  
1. IRIS mesh activated automatically on all enabled devices as cellular degrades
2. Utility field teams use P3 (high-priority text) for coordination
3. Teams moving between substations act as store-carry-forward mules
4. Edge nodes at substations (if pre-deployed) provide stable relay points
5. When any field team reaches area with residual cellular: buffered messages delivered

**Success Condition:**  
Field engineer coordination maintained throughout 24-hour grid restoration without complete communication blackout. Restoration time reduced vs. radio-only coordination.

---

## UC-10: Industrial Emergency — Factory Accident

**Priority:** P0  
**Category:** Industrial Safety

**Actors:**  
- Injured workers  
- Factory safety team  
- On-site medical  
- External emergency services

**Scenario:**  
Chemical plant explosion at an industrial facility in a dense industrial zone. Cellular networks jammed by emergency calls. Factory's internal radio system damaged. Safety team needs to coordinate evacuation of 3,000 workers.

**Constraints:**  
- Large physical area (500,000 m² industrial campus)  
- Metal-heavy environment (BLE attenuation)  
- Hazardous atmosphere (devices must not be additional ignition source — note: IRIS cannot address explosion-proof requirement, this is a limitation)  
- Time: toxic gas release possible

**IRIS Behavior:**  
1. Emergency broadcast from safety command: evacuation routes by zone, muster points
2. BLE mesh through worker phones propagates evacuation order
3. P0 SOS from workers in injured/trapped areas carried by mobile colleagues to safety command
4. Medical team gets priority list before reaching muster point
5. Headcount at muster points communicated back to safety command via IRIS

**Success Condition:**  
Evacuation order reaches 90% of IRIS-enabled worker devices within 2 minutes. All P0 SOS from injured workers received by medical team within 10 minutes.

**Limitation:** IRIS is not explosion-proof. In areas with hazardous atmosphere, standard devices cannot be used. Safety-rated devices with IRIS are a future variant.

---

## UC-11: Remote Area — Tribal/Rural Communication

**Priority:** P2  
**Category:** Remote Connectivity

**Actors:**  
- Tribal village residents  
- Block-level government offices  
- NGO field workers  
- Healthcare workers

**Scenario:**  
Ongoing: remote tribal villages in Jharkhand with no cellular coverage use IRIS for routine communication — medical referrals, supply requests, administrative coordination — not just in emergencies.

**Constraints:**  
- No cellular (permanent, not disaster-related)  
- Intermittent solar power at edge nodes  
- Mixed literacy (voice messages more useful than text for some users)  
- Low-end Android devices (Android 8, 1GB RAM)

**IRIS Behavior:**  
1. Village-level IRIS edge node (solar-powered Raspberry Pi + LoRa)
2. Government block office has IRIS gateway with intermittent internet
3. Routine P4 messages (medical referrals, supply requests) stored and forwarded
4. Government field workers visiting village are mobile data mules: carry stored messages to block office
5. Return messages (supply confirmations, test results) carried back on next visit

**Success Condition:**  
Messages between village and block office delivered with latency proportional to human travel (hours to days). No message loss. Delivery confirmation when message received.

---

## UC-12: Transportation — Railway/Highway Accident

**Priority:** P0  
**Category:** Transportation Emergency

**Actors:**  
- Accident victims  
- Train crew / bus driver  
- Nearby travelers  
- Emergency services

**Scenario:**  
Train derailment in rural area. Train crew cannot reach emergency services (cellular dead zone). Passengers begin using IRIS. First train that passes on adjacent track 20 minutes later has IRIS and can relay messages.

**Constraints:**  
- Remote location, no infrastructure  
- Injured passengers, limited time  
- First responders may be hours away

**IRIS Behavior:**  
1. Train crew sends P0 emergency broadcast with GPS and passenger count
2. Injured passengers send P0 SOS with medical status
3. All messages stored in train network
4. When relief train approaches: BLE contact, all stored messages relayed
5. Relief train has cellular (different location): messages delivered to emergency services
6. Return communication: emergency services can send instructions via the same chain

**Success Condition:**  
Emergency communication established with passing relief train within 20 minutes of accident. Emergency services informed within 30 minutes.

---

## UC-13: Government/Emergency Deployment — NDRF Operations

**Priority:** P1  
**Category:** Government Emergency

**Actors:**  
- NDRF teams  
- State Disaster Management Authority  
- District Collectors  
- NDMA (national level)

**Scenario:**  
NDRF deploys 5 teams to a flood disaster area. Teams are distributed across 200 km. Some teams lose satellite contact. Teams need to share intelligence: flood level data, victim locations, resource needs.

**Constraints:**  
- Government needs authenticated, logged communication  
- Multi-team coordination across large area  
- NDMA needs summary reporting  
- Field teams need real-time tactical communication

**IRIS Behavior:**  
1. NDRF field teams operate on dedicated IRIS group channel (cryptographically authenticated)
2. P2 location messages: team GPS positions shared continuously
3. P1 operational messages: victim finds, resource requests
4. Edge nodes at staging areas: stable relay points + internet bridges to NDMA
5. All messages logged with signatures for post-disaster reporting
6. NDMA coordination center receives real-time field summary via IRIS

**Success Condition:**  
All 5 NDRF teams maintain communication with coordination even when satellite link fails. All operational messages delivered within 5 minutes.

---

## UC-14: Humanitarian Operations — Refugee Camp

**Priority:** P2  
**Category:** Humanitarian

**Actors:**  
- Displaced persons  
- UNHCR field staff  
- NGO workers  
- Local government liaison

**Scenario:**  
Refugee camp in conflict-adjacent region. No reliable cellular. Camp has 50,000 people. NGOs need to coordinate distribution, medical services, and protection concerns. Individuals need to communicate with family in other locations.

**Constraints:**  
- High density facilitates BLE mesh  
- Long-range communication needed to reach other camps  
- Privacy critical (conflict context — communications must not identify individuals to actors outside camp)  
- Mixed languages and literacy

**IRIS Behavior:**  
1. Camp mesh: high density BLE network covers entire camp
2. Inter-camp: LoRa gateways at camp perimeter, linked to other camps via LoRa relay chain
3. Encrypted personal messages: families communicate privately (relay nodes cannot read content)
4. Broadcast channel: UNHCR administrative communications
5. Emergency priority: protection incidents trigger P0 with minimal identifying information

**Success Condition:**  
Personal messages between camps delivered within 30 minutes via LoRa chain. Camp administration reaches 95% of IRIS-enabled devices in <60 seconds.

---

## UC-15: Enterprise Environment — Campus Network Resilience

**Priority:** P3  
**Category:** Enterprise

**Actors:**  
- Employees  
- IT operations  
- Security team  
- Business continuity team

**Scenario:**  
Large technology campus (5,000 employees) has critical internet outage. All cellular networks are congested. Internal Wi-Fi infrastructure still operates. IRIS provides coordination channel.

**Constraints:**  
- Not a life-safety emergency (mostly)  
- Corporate environment: message logging required  
- Existing internal Wi-Fi available  
- Mixed device fleet (Android, iPhone, laptops)

**IRIS Behavior:**  
1. IRIS activates on all enterprise devices (MDM-deployed)
2. Campus Wi-Fi network bridged via IRIS local network transport
3. P4 messages: business continuity coordination
4. IT operations team coordinates incident response via IRIS
5. When internet restores: buffered messages delivered, coordination logs preserved

**Success Condition:**  
Business continuity coordination maintained via IRIS during outage. No message loss. Logs available for post-incident review.

---

## Use Case Priority Matrix

| Use Case | Life Safety | Priority | Mesh Required | LoRa Required | Gateway Required |
|----------|-------------|----------|--------------|--------------|-----------------|
| UC-01 Earthquake SOS | Critical | P0 | Yes | Optional | Optional |
| UC-02 Flood Isolation | Critical | P0 | Yes | Yes | Optional |
| UC-03 Cyclone Evacuation | Critical | P0 | Yes | Optional | Yes |
| UC-04 Women's Safety SOS | Critical | P0 | Yes | No | Optional |
| UC-05 Campus Active Threat | Critical | P0 | Yes | No | No |
| UC-06 Crowd Management | High | P1 | Yes | No | No |
| UC-07 SAR Missing Person | High | P1 | Yes | Yes | No |
| UC-08 Medical Remote | High | P1 | Yes | Yes | No |
| UC-09 Grid Failure | High | P1 | Yes | No | Optional |
| UC-10 Industrial Accident | Critical | P0 | Yes | No | Optional |
| UC-11 Remote Rural | Medium | P2 | Yes | Yes | Yes |
| UC-12 Transport Accident | Critical | P0 | Yes | No | No |
| UC-13 NDRF Operations | High | P1 | Yes | Yes | Yes |
| UC-14 Humanitarian | Medium | P2 | Yes | Yes | Optional |
| UC-15 Enterprise | Low | P3 | Optional | No | Optional |
