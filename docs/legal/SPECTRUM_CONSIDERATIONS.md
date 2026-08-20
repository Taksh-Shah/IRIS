# Spectrum Considerations

**Status:** Living document  
**Last updated:** 2026-08-19  
**Owner:** Legal / Engineering  

---

## 1. India WPC (Wireless Planning and Coordination Wing)

### 1.1 Authority and Structure

WPC Wing, Ministry of Communications, Government of India, is India's national spectrum management authority. WPC is responsible for:
- Frequency allocation and assignment
- De-licensing notifications (authorizing use without individual license)
- Licensing of wireless telegraphy operators
- Type approval of wireless equipment (through TEC — Telecom Engineering Centre)

WPC implements ITU Radio Regulations in India and adapts them to national requirements through gazette notifications.

### 1.2 Relevant De-licensing Framework

WPC periodically issues de-licensing notifications that allow operation of specified short-range devices without individual wireless operating licenses. The key notifications affecting IRIS:

- **2005 De-licensing notification:** First notification to explicitly de-license ISM-band frequencies including 865–867 MHz
- **2018 Amendment:** Updated power limits and clarified outdoor/indoor use for 865–867 MHz
- **G.S.R. 853(E), 10-Dec-2021 (CONTROLLING — see §2):** "Use of Low Power Equipment in the Frequency Band 865–868 MHz for Short Range Devices (Exemption from Licence) Rules, 2021" — **supersedes** the earlier 865–867 MHz basis. SRD band **865–868 MHz**; Table-I non-specific SRD: **25 mW e.r.p., duty cycle ≤1%**, FHSS/timing constraints per EN 300 220.
- **2023 Revision (pending):** DoT consultation paper on updating de-licensing for IoT devices; outcome pending

---

## 2. ISM Band 865–868 MHz: LoRa (Future Leg Only)

> **Governing instrument: G.S.R. 853(E) 2021 (865–868 MHz SRD de-licensing).**
> This supersedes the earlier 865–867 MHz basis in §2.1/§2.4 below.
> **The v1 pilot uses BLE/Wi-Fi only (unlicensed 2.4/5 GHz) and does not
> operate 865 MHz at all.** This section governs the future LoRa leg
> (GAP-004; re-validated against TEC ER Annexure-gG5 + ETA before hardware).
> Codification: REG_NOTES.md §1.

### 2.1 Channel Plan

India's 865–867 MHz ISM band uses three primary channels recommended by TRAI for LoRa:

| Channel | Center Frequency | Bandwidth | Permitted Use |
|---------|-----------------|-----------|--------------|
| IN865_1 | 865.0625 MHz | 125/250 kHz | Uplink + Downlink |
| IN865_2 | 865.4025 MHz | 125/250 kHz | Uplink + Downlink |
| IN865_3 | 865.985 MHz | 125/250 kHz | Uplink + Downlink |

IRIS uses IN865_1 as primary channel and IN865_3 as secondary. IN865_2 is reserved for gateway use in LoRaWAN configurations (not applicable to IRIS core but relevant for gateway bridge mode).

### 2.2 Power Limits

| Parameter | WPC Limit | IRIS Target | Margin |
|-----------|----------|------------|--------|
| Maximum EIRP | 1 W (30 dBm) | 25 mW (14 dBm) | 16 dB below limit |
| Maximum antenna gain | Not separately specified | 2 dBi omnidirectional | N/A |
| Conducted power (before antenna) | Derived: 28 dBm for 2 dBi antenna | 12 dBm (SX1262 default) | Well within |

IRIS operates at 14 dBm EIRP to ensure compliance across all hardware configurations, provide a comfortable compliance margin, and reduce interference to other ISM-band users.

### 2.3 Duty Cycle

**WPC now imposes a duty cycle limit.** G.S.R. 853(E) 2021 Table-I (Non-Specific
SRD — telemetry, telecommand, alarms, data) caps duty cycle at **≤1%** (whole
transmission) — the earlier assumption that 865–867 MHz carried no duty-cycle
mandate (vs ETSI EN 300 220) is superseded. IRIS therefore aligns with the
**1% duty cycle** it had self-imposed:
- Avoid excessive channel occupancy that degrades network performance for all users
- Remain compliant with the enacted G.S.R. 853(E) 2021 Table-I limit
- Reduce interference to other ISM users (medical equipment, industrial sensors)

### 2.4 Comparison: India vs EU vs US LoRa Spectrum

| Parameter | India | EU (ETSI) | US (FCC) |
|-----------|-------|-----------|----------|
| Frequency | 865–868 MHz | 863–870 MHz | 902–928 MHz |
| Max EIRP (non-specific SRD) | 25 mW (14 dBm) G.S.R. 853(E) 2021 | 25 mW (14 dBm) | 30 dBm (915 MHz) |
| Duty cycle | 1% (Table-I, G.S.R. 853(E) 2021) | 1% (most sub-bands) | Not mandated |
| License required | No | No | No |
| Standard channel BW | 125 kHz | 125 kHz | 125/500 kHz |

**Implication for IRIS hardware:** LoRa modules must be frequency-configured for India's 865/868 MHz band. EU-default modules (868 MHz) and US-default modules (915 MHz) will not operate in the correct Indian band without reconfiguration. IRIS firmware enforces the correct frequency band at boot based on a compile-time region flag: `IRIS_REGION=IN`. Any RF leg must be re-validated under G.S.R. 853(E) 2021 + TEC ER Annexure-G5 + ETA before shipping (REG_NOTES.md §1).

### 2.5 Type Approval for LoRa Hardware

Commercial sale of LoRa hardware in India requires:
1. **WPC type approval:** Wireless equipment approval from WPC (WT number)
2. **BIS CRS registration:** Bureau of Indian Standards registration under Compulsory Registration Scheme (IS 13252 for electronics safety)
3. **TEC approval:** For certain categories of equipment sold to licensed operators

IRIS gateway kits (RPi + LoRa module) will require WPC/BIS approvals before commercial sale. LoRa module manufacturers (Semtech OEM partners: Heltec, REYAX) should provide existing WPC approvals or IRIS must obtain approval for the combined kit.

---

## 3. 2.4 GHz Band: BLE and Wi-Fi

### 3.1 De-licensing Basis

The 2.4 GHz band (2,400–2,483.5 MHz) is de-licensed in India for:
- Bluetooth (including BLE): any version, up to 20 dBm EIRP
- IEEE 802.11 b/g/n Wi-Fi: up to 20 dBm EIRP for indoor use
- Wi-Fi Direct and Wi-Fi Aware: same as Wi-Fi (they use the same radio)

Devices sold commercially in India with Bluetooth and Wi-Fi must have WPC/BIS type approval as part of device certification. Android phones sold in India already have this approval. IRIS software uses the device's certified radio hardware; no additional spectrum approval is needed.

### 3.2 BLE Specifics

BLE advertising channels: 37 (2402 MHz), 38 (2426 MHz), 39 (2480 MHz). Data channels: 0–36 (2402–2480 MHz in 2 MHz steps, avoiding center of Wi-Fi channels).

BLE 5.x LE Coded PHY: operates in the same 2.4 GHz band; no separate approval required.

BLE 5.x Long Range operation at -20 dBm TX power (common for Coded PHY energy optimization): well within limits.

### 3.3 Wi-Fi Direct and Wi-Fi Aware

Wi-Fi Direct and Wi-Fi Aware use the 2.4 GHz 802.11 channels. Standard de-licensing applies. No special provision required.

---

## 4. 5 GHz Wi-Fi

### 4.1 India 5 GHz Spectrum Status

The 5 GHz band (5150–5850 MHz) has complex licensing in India:

| Sub-band | Status | Relevant Rule |
|----------|--------|--------------|
| 5150–5250 MHz (UNII-1) | De-licensed, indoor only | WPC notification |
| 5250–5350 MHz (UNII-2A) | DFS required; limited use | WPC notification |
| 5470–5725 MHz (UNII-2C) | DFS required; limited use | WPC notification |
| 5725–5850 MHz (UNII-3) | De-licensed | WPC notification |

DFS (Dynamic Frequency Selection) channels require equipment to detect radar signals and vacate the channel. Disaster deployment scenarios may involve military radar operations; DFS compliance is essential.

### 4.2 IRIS Position on 5 GHz

IRIS does not use 5 GHz Wi-Fi for any transport in v1. Rationale:
- DFS compliance adds hardware and regulatory complexity
- 5 GHz range is shorter than 2.4 GHz at equal power (higher free-space path loss)
- Cheap Indian Android devices have inconsistent 5 GHz support (hardware varies by device tier)
- 2.4 GHz Wi-Fi Direct provides sufficient bandwidth for IRIS bundle transfer (4–72 Mbps)

5 GHz may be considered for IRIS desktop app (laptop-to-laptop) in v2.

---

## 5. Satellite Spectrum

Satellite spectrum in India is governed by DoT under the Satellite Communication Policy and requires GMPCS (Global Mobile Personal Communications by Satellite) licenses. See SATELLITE_REGULATION.md for full analysis.

IRIS does not itself operate satellite spectrum. IRIS gateway nodes connect to licensed satellite operators' terminals. The satellite operator holds the spectrum license; IRIS is an application running over the operator's licensed connectivity.

---

## 6. Monitoring WPC Regulatory Changes

IRIS monitors WPC regulatory developments through:
- DoT official gazette (https://dot.gov.in)
- TRAI consultation papers relevant to IoT and short-range devices
- Industry associations: Internet and Mobile Association of India (IAMAI), Cellular Operators Association of India (COAI)

Regulatory changes that would affect IRIS:
- Duty cycle re-imposition/re-scoping on 865–868 MHz (G.S.R. 853(E) 2021 already sets 1% — any amendment tracked)
- Power limit reduction (unlikely; current limit is already generous)
- Licensing requirement for mesh networking on ISM bands (see OPEN_PROBLEMS.md OP-005)
- 5 GHz de-licensing expansion (would benefit IRIS desktop)

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
| 2026-08-19 | PILOT-001 AC-16: 865/868 MHz + G.S.R. 853(E) 2021 governing instrument; duty cycle ≤1% enacted; LoRa scoped future-leg only |
