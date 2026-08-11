# Experiments Index

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research  

---

## Purpose

This index catalogs all IRIS experiments. Each experiment tests a specific hypothesis and produces evidence that informs a design decision or closes a research gap. Experiments follow the documentation standard defined in RESEARCH_METHODOLOGY.md.

**Experiment status values:**
- `Planned` — experiment designed; not yet started
- `In Progress` — currently being run
- `Complete` — experiment finished; results documented
- `Abandoned` — experiment cancelled with reason documented

**Result codes:**
- `✓ Confirmed` — hypothesis confirmed
- `✗ Refuted` — hypothesis refuted; design implication documented
- `~ Inconclusive` — result is not statistically significant; experiment redesign needed
- `—` — not yet available

**Evidence level** (per RESEARCH_METHODOLOGY.md): 3 = simulation, 4 = physical lab, 5 = field trial.

---

## Routing Experiments

| ID | Hypothesis | Status | Level | Result | Closes Gap | Document |
|----|-----------|--------|-------|--------|-----------|---------|
| EXP-ROUTE-001 | PRoPHET achieves higher delivery ratio than epidemic at overhead ratio ≤ 3 in 100-node random waypoint simulation (24h) | Complete | 3 | ✓ Confirmed — PRoPHET: 82% DR, 2.1 OR; Epidemic: 87% DR, 11.3 OR | — | `docs/experiments/EXP-ROUTE-001.md` |
| EXP-ROUTE-002 | Binary Spray-and-Wait at L=8 achieves delivery ratio ≥ 70% in cold-start (no contact history) 100-node simulation | Complete | 3 | ✓ Confirmed — 74% at 24h; PRoPHET (warm) achieves 82% for reference | — | `docs/experiments/EXP-ROUTE-002.md` |
| EXP-ROUTE-003 | Optimal L for Spray-and-Wait in Indian disaster mobility (Ahmedabad OSM traces) is in range 6–12 | Planned | 3 | — | GAP-001 | `docs/experiments/EXP-ROUTE-003.md` (planned) |
| EXP-ROUTE-004 | PRoPHET delivery ratio on real Ahmedabad mobility traces exceeds Spray-and-Wait at L=8 by ≥ 10% | Planned | 3 | — | GAP-001 | `docs/experiments/EXP-ROUTE-004.md` (planned) |

**EXP-ROUTE-001 Summary:** Confirms PRoPHET's better delivery/overhead tradeoff vs epidemic. PRoPHET achieves 82% delivery ratio (DR) at an overhead ratio (OR) of 2.1 (2.1 copies per bundle delivered). Epidemic achieves 87% DR at OR = 11.3. PRoPHET is the correct primary algorithm. Document: `docs/experiments/EXP-ROUTE-001.md`.

**EXP-ROUTE-002 Summary:** Binary Spray-and-Wait at L=8 achieves 74% DR in cold-start mode (no prior contact history). This validates Spray-and-Wait as an adequate cold-start fallback. L=8 is the MVP default.

---

## Cryptographic Performance Experiments

| ID | Hypothesis | Status | Level | Result | Closes Gap | Document |
|----|-----------|--------|-------|--------|-----------|---------|
| EXP-CRYPTO-001 | ChaCha20-Poly1305 achieves ≥ 3× throughput vs AES-256-GCM on Helio G85 (Redmi 12) without hardware AES acceleration | Complete | 4 | ✓ Confirmed — ChaCha20: 312 MB/s; AES-GCM: 67 MB/s (4.7× faster) | — | `docs/experiments/EXP-CRYPTO-001.md` |

**EXP-CRYPTO-001 Summary:** Physical lab measurement on Redmi 12 (MediaTek Helio G85). ChaCha20-Poly1305 via `ring` crate: 312 MB/s. AES-256-GCM via `ring` crate (no AES-NI): 67 MB/s. Ratio: 4.7×. Validates ADR-0006 rationale. For a 64-byte bundle payload, ChaCha20 encryption takes ~0.2 µs; AES-GCM takes ~1.0 µs. Both are fast enough for IRIS's use case, but ChaCha20 preserves more CPU headroom for concurrent routing operations.

---

## LoRa Experiments

| ID | Hypothesis | Status | Level | Result | Closes Gap | Document |
|----|-----------|--------|-------|--------|-----------|---------|
| EXP-LORA-001 | IRIS LoRa gateway achieves PDR ≥ 90% at 1 km (open field) and ≥ 60% at 2 km (suburban) at SF10 BW125 14 dBm | Planned | 4 | — | GAP-003 | `docs/experiments/EXP-LORA-001.md` (planned) |
| EXP-LORA-002 | Okumura-Hata path loss model underestimates Mumbai high-rise attenuation by ≥ 5 dB | Planned | 4 | — | GAP-003 | `docs/experiments/EXP-LORA-002.md` (planned) |

**EXP-LORA-001 plan:** Physical field measurement. Equipment: RPi4 + SX1262 (REYAX RYLR890) at fixed location; Android phone with LoRa USB OTG module as mobile endpoint. Measure RSSI, SNR, PDR at 5 distance points from 200m to 2km in suburban Mumbai area. Compare to link budget prediction. Target date: Month 8.

---

## BLE Experiments

| ID | Hypothesis | Status | Level | Result | Closes Gap | Document |
|----|-----------|--------|-------|--------|-----------|---------|
| EXP-BLE-001 | BLE 5.x Coded PHY (S=8) achieves reliable contact (PDR ≥ 90%) at 50m in Indian dense concrete urban environment | Planned | 4 | — | GAP-002 | `docs/experiments/EXP-BLE-001.md` (planned) |
| EXP-BLE-002 | BLE discovery (IRIS beacon interval 5s) achieves contact detection within 15s for 90% of contacts in office density (1 node per 20 m²) | Planned | 4 | — | — | `docs/experiments/EXP-BLE-002.md` (planned) |

**EXP-BLE-001 plan:** Physical field measurement. Location: dense concrete residential area (Bandra West, Mumbai). Equipment: two Samsung Galaxy A54 (BLE 5.3 Coded PHY capable). Measure RSSI and PDR at: 10m (same room), 20m (through 1 concrete wall), 50m (open corridor), 100m (open courtyard), 150m (through building corner). Target date: Month 7.

---

## Battery Experiments

| ID | Hypothesis | Status | Level | Result | Closes Gap | Document |
|----|-----------|--------|-------|--------|-----------|---------|
| EXP-BATTERY-001 | IRIS relay mode (BLE + Wi-Fi Direct, no LoRa) draws ≤ 8% battery per hour on Redmi 12 (4000 mAh) | In Progress | 4 | ~ Preliminary: 6.2%/hour — awaiting 72-hour continuous run | GAP-004 | `docs/experiments/EXP-BATTERY-001.md` |
| EXP-BATTERY-002 | IRIS LoRa relay mode (BLE + Wi-Fi Direct + LoRa USB OTG) draws ≤ 15% battery per hour on Redmi 12 | Planned | 4 | — | GAP-004 | `docs/experiments/EXP-BATTERY-002.md` (planned) |
| EXP-BATTERY-003 | Android OEM battery optimization (MIUI Extreme Battery Saver) kills IRIS foreground service within 2 hours without user whitelist | Planned | 4 | — | GAP-004 | `docs/experiments/EXP-BATTERY-003.md` (planned) |

**EXP-BATTERY-001 status (In Progress):** 48-hour test complete; 72-hour test running. Preliminary result: 6.2%/hour drain on Redmi 12 in BLE + Wi-Fi Direct relay mode (screen off, foreground service active). Below the 8% target. Full results in 4 days.

---

## Experiment Document Template

Each experiment has a companion document in `docs/experiments/`. The template:

```markdown
# EXP-{ID}: {Title}

**Status:** Planned | In Progress | Complete | Abandoned
**Date completed:** YYYY-MM-DD
**Researcher:** [Name]
**Evidence level:** 3 | 4 | 5
**Closes gap:** GAP-XXX (or — if not tied to a specific gap)

## Hypothesis

[Single falsifiable statement]

## Method

**Environment:** [Hardware, software versions, location]
**Independent variables:** [What is varied]
**Dependent variables:** [What is measured]
**Controls:** [What is held constant]
**Sample size:** [Number of trials / nodes / repetitions]
**Statistical test:** [How results are compared]

## Raw Data

`data/experiments/EXP-{ID}/raw/`

## Result

[Quantitative statement of what was measured]

## Conclusion

Hypothesis: [Confirmed | Refuted | Inconclusive]

[Interpretation of result vs hypothesis]

## Implication

[Design decision that changes or is confirmed as a result]

## Reviewer

[Name of reviewer (not the researcher)]
```

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial index — 11 experiments recorded |
