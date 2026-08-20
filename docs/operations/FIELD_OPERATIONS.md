# Field Operations — Deployment and Operations Guide

**Audience:** Disaster response team leads, IRIS network coordinators
**Status:** v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

This document covers the physical deployment of IRIS network infrastructure in disaster response scenarios. It assumes the reader is a trained responder familiar with basic electronics but not necessarily a network engineer.

**Before deployment:** All IRIS hardware must have been tested using the pre-deployment checklist in Section 2. Field deployment of untested hardware is not permitted.

---

## 2. Pre-Deployment Checklist

Complete this checklist at base before leaving for the field. Each item must be signed off by the team lead.

```
PRE-DEPLOYMENT CHECKLIST — IRIS Edge Server
Date: ____________  Operator: ____________  Unit ID: ____________

HARDWARE
[ ] RPi 4 (8 GB) powers on and IRIS software boots
[ ] LoRa module: AT command responds (AT+OK)
[ ] LoRa antenna: connected, no visible damage, SMA connector tight
[ ] Solar panel: Voc reads 18–22V in sunlight (or 12V nominal)
[ ] LiFePO4 battery: charge > 80% (verify with battery indicator)
[ ] Case: IP67 sealed, all cable glands tight
[ ] USB-C power cable: strain relief intact

SOFTWARE
[ ] IRIS version: _________________ (latest stable)
[ ] Node ID configured and recorded: _________________
[ ] Network name (incident name) configured: _________________
[ ] Admin PIN set: YES / NO (must be YES)
[ ] Clock synchronized (NTP or GPS): YES / NO
[ ] Self-test passes all CRITICAL checks: YES / NO
[ ] Self-test output photographed for records: YES / NO

RADIO COMPLIANCE
[ ] Frequency: 866.0 MHz (865–868 MHz SRD band, WPC G.S.R. 853(E) 2021 — CONFIRMED)
[ ] TX power: ≤ 14 dBm at modem (verify with AT+DRATE command)
[ ] EIRP: ≤ 25 mW (≤ 14 dBm + antenna gain ≤ 0 dBi net)
[ ] Duty cycle limiter: ENABLED in config

TEAM LEAD SIGN-OFF: ____________  DATE: ____________
```

---

## 3. Edge Server Deployment

### 3.1 Hardware Configuration

**Standard IRIS Edge Server:**

| Component | Specification |
|---|---|
| Compute | Raspberry Pi 4 Model B, 8 GB RAM |
| Storage | 64 GB microSD (Class 10, A2) or 256 GB USB SSD |
| LoRa module | Waveshare SX1262 LoRa HAT or EBYTE E22-900M30S |
| BLE/Wi-Fi | Built-in on RPi 4 (Broadcom BCM43455) |
| Battery | LiFePO4, 20 Ah, 12.8V nominal (Lithium Iron Phosphate — safe for field use) |
| Solar | 40W monocrystalline panel with MPPT charge controller |
| Case | IP67 ABS enclosure, 300×200×130 mm |
| Antenna (LoRa) | 3 dBi omni-directional, 866 MHz, N-connector |
| Antenna (BLE/WiFi) | Internal (RPi built-in) |

### 3.2 Antenna Placement

Antenna placement has the largest single impact on LoRa range. Follow these rules:

1. **Height:** Mount as high as safely possible. Every 6m of additional height doubles effective range in flat terrain. Rooftop (10–15m) is preferred over ground level.
2. **Line of sight:** LoRa at 866 MHz requires Fresnel zone clearance. For 5 km range, the first Fresnel zone radius at the midpoint is ~17m — keep high ground between antennas clear.
3. **Separation:** Keep LoRa antenna > 2m from BLE/Wi-Fi antennas (interference risk).
4. **Orientation:** Omni-directional vertical whip should be mounted vertically (vertical polarization matches mobile devices).
5. **Grounding:** In lightning-prone areas, connect antenna ground to an earth stake.

```
Typical deployment diagram:

    [LoRa Antenna] ← 3 dBi whip, mounted on 2m mast
         |
    [Edge Server Box] ← weatherproof enclosure
    [Solar Panel] ← south-facing, 30° tilt
    [Battery] ← in shaded location
```

### 3.3 Solar and Battery Setup

**Expected runtime estimates:**

| Condition | Solar input | Consumption | Runtime |
|---|---|---|---|
| Full sun (India summer) | 40W avg | 8W (RPi + LoRa TX) | Indefinite |
| Overcast (monsoon) | 8W avg | 8W | Battery-dependent |
| Night (no solar) | 0W | 8W | ~32 hours (20 Ah @ 12.8V) |
| Emergency mode (LoRa only) | 0W | 3W | ~85 hours |

**Battery management:**
- LiFePO4 safe temperature range: -20°C to +60°C (use in shade during summer)
- Never discharge below 20% (2.5V per cell) — this damages the battery
- Charge controller must be set for LiFePO4 profile (NOT lead-acid, NOT Li-ion)
- Expected LiFePO4 cycle life: 2,000+ cycles at 80% depth of discharge

---

## 4. Network Verification Procedure

After deploying an edge server, verify it is functioning by sending a test message.

### 4.1 Verification Steps

**Step 1:** On the edge server, open the IRIS coordinator dashboard:
```
http://localhost:8080/coordinator
```

**Step 2:** Verify the node appears on the network map. If it does not appear within 5 minutes, check transport status in the Diagnostics panel.

**Step 3:** From a mobile device within BLE range (~50m), open the IRIS app. The edge server should appear in the nearby nodes list.

**Step 4:** Send a P4 (normal priority) test message to any visible node:
```
Sender: Mobile device A
Recipient: Any discovered node
Priority: P4 (Normal)
Content: "IRIS NET TEST [timestamp]"
```

**Step 5:** Confirm delivery. The message should be acknowledged within 60 seconds if the recipient is in direct contact range. For relay via LoRa, allow up to 5 minutes.

**Step 6:** Check edge server logs for the test message relay event:
```bash
iris-cli logs tail --comp routing --evt message_relayed --last 10m
```

**Step 7:** If delivery fails, follow the troubleshooting table (Section 7).

---

## 5. Battery Management

### 5.1 Charging Status Indicators

The IRIS edge server shows battery status via LED (if present) and coordinator dashboard:

| Battery level | LED color | Dashboard | Action required |
|---|---|---|---|
| > 60% | Green | OK | None |
| 30–60% | Yellow | WARN | Monitor solar input |
| 15–30% | Orange | WARN | Reduce TX power to minimum |
| 5–15% | Red | ALERT | Enable emergency mode (LoRa only) |
| < 5% | Red blinking | CRITICAL | Shutdown imminent |

### 5.2 Battery-Aware Mode Switching

IRIS automatically adjusts operation based on battery level:

| Battery | Mode | Transports active | Routing |
|---|---|---|---|
| > 30% | Full | BLE + WiFi + LoRa | Normal |
| 15–30% | Reduced | BLE + LoRa | P0–P3 only |
| 5–15% | Emergency | LoRa only | P0–P1 only |
| < 5% | SOS only | LoRa only | P0 only, then shutdown |

---

## 6. Radio Frequency Compliance

### 6.1 India WPC Requirements

Operation in the 865–867 MHz band under India WPC rules:

| Requirement | Value | How to verify |
|---|---|---|
| Frequency | 865.0–867.0 MHz | AT+FREQ? command returns 866000000 |
| EIRP | ≤ 25 mW (≤ 14 dBm) | AT+TXPOWER? returns ≤ 14 |
| Duty cycle | ≤ 1% per channel | IRIS enforces in software; check logs |
| Authorization | ISM band (no license required) | Verify WPC current notification |

### 6.2 Pre-Transmission Compliance Check

```bash
iris-cli radio compliance-check
```

Output:
```
Radio Compliance Check
  Frequency:    866.0 MHz      ✓ (within 865-867 MHz)
  TX power:     14 dBm         ✓ (≤ 14 dBm)
  Antenna gain: 2 dBi          ✓ (EIRP = 16 dBm = 40 mW ≤ 25 mW net... WARN)
```

**Note:** If antenna gain exceeds 0 dBi, reduce TX power so that EIRP ≤ 14 dBm + (25 mW limit expressed in dBm ≈ 14 dBm). A 3 dBi antenna requires TX power reduced to 11 dBm to stay within the 25 mW EIRP limit.

---

## 7. Handover Procedure

When a response team transfers responsibility for an edge server to another team:

**Outgoing team:**
1. Export current network state: `iris-cli export network-state --output handover-$(date +%Y%m%d).json`
2. Record current battery level and solar status
3. Brief incoming team on any active incidents or unusual network conditions
4. Transfer admin credentials securely (face-to-face, not over radio)
5. Sign the handover log with unit ID, timestamp, battery level, active connections

**Incoming team:**
1. Import network state: `iris-cli import network-state handover-20260811.json`
2. Run network verification procedure (Section 4)
3. Confirm admin access with their own credentials
4. Record handover acceptance in the incident log

---

## 8. Troubleshooting

| Symptom | Likely cause | Action |
|---|---|---|
| No peers discovered | Antenna disconnected, transport disabled | Check antenna, run `iris-cli diagnostics transports` |
| LoRa range < 1 km | Antenna vertical alignment, TX power | Check antenna orientation, verify TX power ≤ 14 dBm |
| Battery draining faster than expected | Solar not charging (shade), high traffic | Move panel, check solar charge current |
| Node not appearing on coordinator map | Network partition, routing table not propagated | Check bridge nodes, send test message |
| Self-test FAIL on crypto | Corrupted keys or storage | Re-initialize node (warning: new NodeId generated) |
| Duty cycle errors in log | Too many LoRa transmissions | Check for message flood, reduce P4+ relay rate |

---

## 9. References

- WPC India spectrum: https://dot.gov.in/spectrum-management/2086
- LiFePO4 battery guide: internal hardware guide HW-2026-003
- RPi 4 datasheet: https://datasheets.raspberrypi.com/rpi4/raspberry-pi-4-datasheet.pdf
- IRIS diagnostics: `docs/operations/DIAGNOSTICS.md`
- Incident management: `docs/operations/INCIDENT_MANAGEMENT.md`
