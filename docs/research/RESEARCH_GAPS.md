# Research Gaps

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research  

---

## Purpose

This document catalogs known unknowns in IRIS's technical design. Each gap is a question whose answer would materially affect a design choice, performance target, or operational parameter. Gaps are prioritized by their impact on MVP deliverables.

A gap is closed when: (a) a controlled experiment produces a result, (b) a peer-reviewed paper directly answers the question, or (c) a field trial produces sufficient data. Partial closure is recorded with confidence level.

---

## GAP-001: Optimal L Parameter for Spray-and-Wait in Indian Disaster Scenarios

**Status:** Open  
**Priority:** High (affects routing algorithm configuration for MVP)  
**Affects:** `crates/router/src/spray_wait.rs`, routing defaults in `config/defaults.toml`

### Background

Spray-and-Wait routing requires an L parameter (number of copies to spray). The original Spyropoulos et al. (2005) paper derives optimal L analytically under assumptions of:
- Uniform random mobility (random waypoint model)
- Homogeneous node density
- Stationary destination

Indian disaster scenarios violate all three assumptions:
- Mobility is clustered (people evacuate along roads, gather at relief camps)
- Node density varies by orders of magnitude (dense urban core vs sparse rural periphery)
- Destinations may be mobile (family members evacuating)

Published L values in literature range from 6 to 64 depending on scenario. The wrong L value causes either: (a) excessive replication wasting storage/bandwidth (L too high) or (b) low delivery rate (L too low).

### What Experiment Would Close This Gap

**Simulation experiment (EXP-ROUTE-003):**
1. Build mobility traces from OpenStreetMap data for three Indian cities: Ahmedabad (planned pilot), Chennai (coastal flood risk), Kedarnath (mountain disaster analog)
2. Simulate 1,000-node disaster scenarios using ONE simulator with IRIS PRoPHET + Spray-and-Wait
3. Sweep L from 2 to 64; measure delivery ratio, overhead ratio, latency
4. Repeat for three node density regimes: sparse (<0.1 nodes/km²), medium, dense (>10 nodes/km²)
5. Identify L values that achieve delivery ratio ≥ 0.8 with overhead ratio ≤ 3

**Physical experiment (follow-up):**
- Deploy 20 Android devices in Ahmedabad city park
- Run structured mobility scenario (simulated evacuation)
- Validate simulation L recommendation against physical results

**Estimated effort:** 4 weeks (2 weeks simulation, 2 weeks physical)  
**Dependencies:** ONE simulator setup (EXP-ROUTE-001 prerequisite), Ahmedabad partner for physical trial

---

## GAP-002: BLE Range in Dense Urban Indian Environments

**Status:** Open  
**Priority:** High (affects transport layer hop distance assumptions)  
**Affects:** `crates/transport/src/ble/mod.rs`, network planning guidance

### Background

BLE 5.x (Long Range / Coded PHY) specifies a nominal range of 100–1,000m in line-of-sight conditions. Published ranges for dense urban environments in Western cities (New York, London) show 30–150m effective range. Indian urban environments differ:

- **Concrete construction dominance:** Indian residential construction uses reinforced concrete rather than wood-frame, producing higher RF attenuation per wall
- **Building density:** Mumbai and Delhi have higher building density than comparable Western cities
- **Dust and particulate:** North Indian plains have high PM2.5/PM10 levels, which may attenuate 2.4 GHz signal in severe events (dust storms in Rajasthan regularly exceed 5,000 µg/m³)
- **Monsoon rain:** High humidity and rainfall affect 2.4 GHz propagation differently than temperate climates

Current IRIS path loss model uses ITU-R P.1238 (indoor) and Okumura-Hata (outdoor), calibrated for European environments. The error bound for Indian cities is unknown.

### What Experiment Would Close This Gap

**Controlled experiment:**
1. Select three sites: Mumbai high-rise residential area, Delhi mixed-use market, Ahmedabad old city (dense low-rise)
2. Deploy two Android phones (BLE 5.2 Coded PHY, S8 LE) at measured distances (10, 30, 50, 100, 150, 200, 300m)
3. Measure RSSI and PDR (packet delivery ratio) at each distance
4. Vary: wall count (0, 1, 2, 3+), floor offset (same floor, ±1, ±2)
5. Repeat under: dry season, monsoon, dust event (if available)

**Output:** Path loss model coefficients for IRIS's three target urban environments; BLE hop distance recommendation (95th percentile reliable range)

**Estimated effort:** 3 weeks field measurement + 1 week analysis  
**Dependencies:** Travel budget, Android devices with BLE 5.2 support, partner site access

---

## GAP-003: LoRa PDR on India-Specific Urban Terrain (Mumbai High-Rises)

**Status:** Open  
**Priority:** Medium (LoRa is Month 12 feature; needed before Alpha)  
**Affects:** `crates/transport/src/lora/mod.rs`, LoRa channel parameters

### Background

LoRa link budget models (Semtech AN1200.22) assume either line-of-sight or standard urban propagation (Okumura-Hata). Mumbai presents a specific challenge: 20–30 story residential towers in areas like Bandra, Andheri, and Worli create a "canyon" effect where:

- Direct path is blocked by buildings
- Reflected signals via tower facades create multipath
- LoRa's CSS modulation handles multipath reasonably, but range prediction is uncertain

Published LoRa urban measurements are primarily from European cities (Antwerp, Berlin, Amsterdam) with 3–7 story building stock. Mumbai's building profile is categorically different.

Additionally, Mumbai's geography (peninsula, harbor, creek systems) creates unusual propagation: water surfaces provide near-LOS paths between coastal nodes, potentially extending range significantly. This is not captured in standard models.

### What Experiment Would Close This Gap

**Field measurement:**
1. Deploy SX1262-based LoRa nodes on rooftops and ground level in Bandra (high-rise) and Dharavi (low-rise dense) areas
2. Measure RSSI, SNR, PDR at SF7, SF10, SF12 across measured distances 100m–5km
3. Map propagation anomalies: harbor LOS paths, building canyon effects
4. Compare measurements against Okumura-Hata predictions; derive correction factor

**Output:** Mumbai-specific path loss model; recommended SF for urban vs coastal deployment; validated link budget for gateway placement planning

**Estimated effort:** 2 weeks field + 2 weeks analysis  
**Dependencies:** WPC compliance verification for test transmission, rooftop access permissions

---

## GAP-004: Battery Drain of LoRa Duty-Cycle-Constrained Operation on Cheap Android Devices

**Status:** Open  
**Priority:** Medium  
**Affects:** Power management spec, minimum hardware requirements

### Background

IRIS targets cheap Android devices (₹8,000–₹15,000 price range) as primary hardware. These devices have:
- Battery: 3,000–4,000 mAh typically
- SoC: MediaTek Helio G85 or Qualcomm Snapdragon 680 (no AES-NI, limited DSP)
- USB: USB 2.0 (for LoRa module via OTG)
- Background processing: heavily throttled by MIUI, ColorOS, OneUI (aggressive doze modes)

A LoRa module connected via USB OTG and operated in duty-cycle-constrained mode draws additional current. The combination of:
- IRIS background service (BLE scanning + Wi-Fi Direct discovery)
- USB OTG power delivery to LoRa module (~50–150 mA depending on transmit)
- Screen-off aggressive power management on Indian OEM Android skins

...has not been characterized. Published battery measurements for LoRa + Android are limited and use older hardware or assume Western Android (stock Google) behavior.

### What Experiment Would Close This Gap

**Controlled battery test:**
1. Select 5 target devices: Redmi 12, Samsung Galaxy F14, Realme C55, Tecno Spark 20, Motorola G54 (representative ₹8,000–₹15,000 tier)
2. Instrument battery drain: ADB battery stats + IRIS internal power logging
3. Measure drain rates for: (a) IRIS idle (BLE scan only), (b) IRIS active relay (BLE + Wi-Fi Direct), (c) IRIS LoRa relay (BLE + LoRa module active)
4. Measure impact of OEM doze mode: how long before IRIS background service is killed?
5. Test battery-saving workarounds: foreground service notification, wake lock, alarm-based wake

**Output:** Battery life table per device per mode; minimum battery threshold for relay operation; foreground service requirement recommendation

**Estimated effort:** 2 weeks  
**Dependencies:** Device procurement (5 devices × ₹10,000 = ₹50,000), LoRa USB OTG modules

---

## GAP-005: User Behavior Under Disaster Stress

**Status:** Open  
**Priority:** Medium (informs UX and protocol design)  
**Affects:** `docs/product/EMERGENCY_UX.md`, SOS flow design, message priority scheme

### Background

IRIS protocol design makes implicit assumptions about user behavior in disasters:
- Users send SOS within 60 seconds of needing help
- Users can operate a smartphone UI under moderate stress
- Users understand what "relay" and "mesh" mean (or don't need to)
- SOS messages are genuine (not false alarms under stress)

These assumptions are unvalidated. Published literature on disaster communication behavior is limited. Relevant findings from existing research:
- Quarantelli (1954): people do not panic in disasters; behavior is purposive
- Mileti & Sorensen (1990): warning message receipt requires multiple repetitions
- Sutton et al. (2008): Twitter use in Peppermill wildfire — people sought confirmatory information before acting

What is not established:
- Smartphone operation accuracy under acute stress (elevated cortisol, reduced fine motor)
- Frequency of false SOS (accidental activation) in non-test conditions
- Time from onset of disaster event to first SOS send attempt
- Whether users correctly interpret mesh status indicators under stress

### What Experiment Would Close This Gap

**Simulated stress experiment (ethics board approval required):**
1. Recruit 40 participants; randomly assign to low-stress and moderate-stress conditions
2. Moderate-stress condition: mild time pressure + noise (fire alarm sound) + cognitive load task
3. Measure: time to send SOS from locked screen, error rate, comprehension of status indicator
4. Survey: post-experiment questionnaire on UI clarity and trust

**Field observation (disaster preparedness drill):**
1. Partner with NDRF or SDMA for an earthquake preparedness drill
2. Deploy IRIS (test version) to 20 participants
3. Observe: SOS activation rate, false alarm rate, UI interactions
4. Debrief interview on confusion points

**Estimated effort:** 6 weeks (ethics approval 2 weeks, experiment 2 weeks, analysis 2 weeks)  
**Dependencies:** Ethics committee approval (institutional partner required), NDRF partnership

---

## Gap Closure Criteria

A gap is marked **Closed** when a documented experiment (see `docs/experiments/`) produces a result meeting the following bar:
- Sample size sufficient for the claimed confidence interval
- Experiment documented with hypothesis, method, raw data, and conclusion
- Result reviewed by at least one researcher not involved in the experiment
- Architecture or configuration updated to reflect the result

A gap is marked **Partial** when:
- Literature provides a directional answer but not quantitatively specific to IRIS's context
- Simulation evidence exists but physical validation is pending

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document — 5 gaps recorded |
