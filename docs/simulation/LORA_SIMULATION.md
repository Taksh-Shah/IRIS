# LoRa Simulation — Radio Link Modeling for India

**Component:** `iris-sim/radio/lora.py`
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

LoRa is IRIS's primary long-range transport for scenarios where BLE and Wi-Fi Direct are insufficient. Accurate LoRa simulation requires modeling:

- Path loss (Free-Space Path Loss + terrain/urban corrections)
- Spreading factor, bandwidth, and coding rate effects on data rate and range
- India-specific duty cycle regulation (WPC 865–867 MHz band)
- Packet collision probability under multiple simultaneous transmissions
- Capture effect (stronger signal captures the receiver despite collision)

---

## 2. Frequency Band and Regulatory Parameters (India)

India's Wireless Planning and Coordination (WPC) wing authorizes LoRa operation in the 865–867 MHz ISM band:

| Parameter | Value |
|---|---|
| Frequency range | 865.0 – 867.0 MHz |
| Maximum EIRP | 25 mW (14 dBm) |
| Duty cycle limit | 1% per channel |
| Channel bandwidth | 125 kHz, 250 kHz, or 500 kHz |
| Number of channels | 8 (200 kHz spacing within band) |

**Duty cycle enforcement in simulation:**

```python
class DutyCycleTracker:
    def __init__(self, limit: float = 0.01, window_s: float = 3600.0):
        self.limit = limit
        self.window_s = window_s
        self.tx_log: list[tuple[float, float]] = []  # (start_time, duration_s)

    def can_transmit(self, now: float, tx_duration_s: float) -> bool:
        # Remove events outside the rolling window
        cutoff = now - self.window_s
        self.tx_log = [(t, d) for t, d in self.tx_log if t > cutoff]
        used = sum(d for _, d in self.tx_log)
        allowed = self.window_s * self.limit
        return (used + tx_duration_s) <= allowed

    def record_tx(self, now: float, duration_s: float):
        self.tx_log.append((now, duration_s))
```

---

## 3. Path Loss Model

### 3.1 Free-Space Path Loss (FSPL)

Used for line-of-sight scenarios (open terrain, rooftop-to-rooftop):

```
FSPL(dB) = 20·log₁₀(d) + 20·log₁₀(f) + 20·log₁₀(4π/c)
         = 20·log₁₀(d_km) + 20·log₁₀(f_MHz) + 32.44
```

At 866 MHz, 5 km: FSPL = 20·log₁₀(5) + 20·log₁₀(866) + 32.44 = 13.98 + 58.75 + 32.44 = **105.2 dB**

### 3.2 Okumura-Hata Model (Urban/Suburban India)

For urban and suburban environments (Ahmedabad, Mumbai):

```
L_urban(dB) = 69.55 + 26.16·log₁₀(f_MHz) - 13.82·log₁₀(h_b) 
              - a(h_m) + (44.9 - 6.55·log₁₀(h_b))·log₁₀(d_km)
```

where:
- `f_MHz` = frequency in MHz (866)
- `h_b` = base station (gateway/relay) antenna height in meters (default: 15 m)
- `h_m` = mobile antenna height in meters (default: 1.5 m, handheld)
- `a(h_m)` = mobile antenna correction factor

For medium-small cities (most Indian tier-2 cities):
```
a(h_m) = (1.1·log₁₀(f_MHz) - 0.7)·h_m - (1.56·log₁₀(f_MHz) - 0.8)
        = (1.1·log₁₀(866) - 0.7)·1.5 - (1.56·log₁₀(866) - 0.8)
        ≈ 0.0 dB  (negligible for 1.5m, 866 MHz)
```

For rural India (Okumura-Hata suburban correction):
```
L_suburban(dB) = L_urban - 2·(log₁₀(f_MHz/28))² - 5.4
```

### 3.3 Python Implementation

```python
import math

def path_loss_dB(
    distance_m: float,
    freq_mhz: float = 866.0,
    environment: str = "urban",
    h_base_m: float = 15.0,
    h_mobile_m: float = 1.5,
) -> float:
    d_km = distance_m / 1000.0
    if d_km < 0.001:
        d_km = 0.001

    a_hm = (1.1 * math.log10(freq_mhz) - 0.7) * h_mobile_m \
           - (1.56 * math.log10(freq_mhz) - 0.8)
    
    L_urban = (69.55 + 26.16 * math.log10(freq_mhz)
               - 13.82 * math.log10(h_base_m)
               - a_hm
               + (44.9 - 6.55 * math.log10(h_base_m)) * math.log10(d_km))

    if environment == "urban":
        return L_urban
    elif environment == "suburban":
        return L_urban - 2 * (math.log10(freq_mhz / 28)) ** 2 - 5.4
    elif environment == "rural":
        return L_urban - 4.78 * (math.log10(freq_mhz)) ** 2 \
               + 18.33 * math.log10(freq_mhz) - 40.94
    else:  # free space
        return 20 * math.log10(d_km) + 20 * math.log10(freq_mhz) + 32.44
```

---

## 4. LoRa Physical Layer Parameters

### 4.1 Spreading Factor, Bandwidth, Coding Rate

| SF | BW (kHz) | CR | Data rate (kbps) | Max range (rural) | Air time (64B) |
|---|---|---|---|---|---|
| SF7  | 125 | 4/5 | 5.47 | ~2 km | 46 ms |
| SF8  | 125 | 4/5 | 3.13 | ~4 km | 82 ms |
| SF9  | 125 | 4/5 | 1.76 | ~6 km | 144 ms |
| SF10 | 125 | 4/5 | 0.98 | ~9 km | 267 ms |
| SF11 | 125 | 4/5 | 0.54 | ~12 km | 483 ms |
| SF12 | 125 | 4/5 | 0.29 | ~15 km | 991 ms |

IRIS default: **SF9, 125 kHz, CR 4/5** — balances range (~6 km rural) with duty cycle compliance.

### 4.2 Air Time Calculation

```python
def lora_air_time_s(
    payload_bytes: int,
    sf: int = 9,
    bw_khz: float = 125.0,
    cr: int = 5,  # coding rate denominator (4/5 → 5)
    preamble_symbols: int = 8,
    header: bool = True,
    crc: bool = True,
) -> float:
    # Symbol duration
    t_sym = (2 ** sf) / (bw_khz * 1000)

    # Preamble time
    t_preamble = (preamble_symbols + 4.25) * t_sym

    # Payload symbols
    payload_bits = payload_bytes * 8
    n_sym_payload = 8 + max(
        math.ceil(
            (8 * payload_bytes - 4 * sf + 28 + 16 * crc - 20 * (not header))
            / (4 * (sf - 2))
        ) * cr,
        0,
    )
    t_payload = n_sym_payload * t_sym

    return t_preamble + t_payload
```

Example: 64-byte SOS message at SF9, 125 kHz, CR 4/5: air time ≈ **144 ms**

---

## 5. Receiver Sensitivity and Link Budget

| SF | Receiver sensitivity (dBm) | SNR threshold (dB) |
|---|---|---|
| SF7  | -123 | -7.5 |
| SF8  | -126 | -10.0 |
| SF9  | -129 | -12.5 |
| SF10 | -132 | -15.0 |
| SF11 | -134.5 | -17.5 |
| SF12 | -137 | -20.0 |

**Link budget at SF9 (IRIS default):**
```
TX power:         +14 dBm (WPC limit)
Antenna gain:     +2 dBi (simple whip)
EIRP:             +16 dBm
Path loss (5km rural): -109 dB  (Okumura-Hata rural)
RX antenna gain:  +2 dBi
Received signal:  +16 - 109 + 2 = -91 dBm
Receiver sens.:   -129 dBm
Link margin:      38 dB  (excellent)
```

At 15 km rural: received = +16 - 128 + 2 = -110 dBm, margin = 19 dB. Viable with a fade margin.

---

## 6. Packet Delivery Rate Model

### 6.1 SNR-Based PDR

PDR is modeled as a sigmoid function of received SNR relative to threshold:

```python
def pdr_from_snr(snr_dB: float, snr_threshold_dB: float) -> float:
    delta = snr_dB - snr_threshold_dB
    # Sharp transition: within 3 dB of threshold, PDR drops from 1 to 0
    return 1.0 / (1.0 + math.exp(-delta * 2.0))
```

### 6.2 Collision Model (Slotted ALOHA Approximation)

Multiple nodes transmitting simultaneously on the same channel cause collisions. IRIS uses a slotted ALOHA collision model:

```
P(success | n transmitting) = (1 - 1/N)^(n-1) ≈ e^{-(n-1)/N}
```

where `N` = number of available channels (8 for India 865–867 MHz band).

### 6.3 Capture Effect

If the signal from node A is 6 dB stronger than node B at the receiver, node A's packet is captured despite collision:

```python
def capture_success(rx_power_a: float, rx_power_b: float, threshold_dB: float = 6.0) -> bool:
    return (rx_power_a - rx_power_b) >= threshold_dB
```

---

## 7. Validation: Simulated vs. Measured PDR

IRIS LoRa simulation is validated against empirical measurements in rural Rajasthan (flat terrain, low vegetation):

| Distance (km) | Measured PDR | Simulated PDR | Delta |
|---|---|---|---|
| 1 | 0.98 | 0.99 | +0.01 |
| 3 | 0.94 | 0.96 | +0.02 |
| 5 | 0.87 | 0.91 | +0.04 |
| 8 | 0.71 | 0.78 | +0.07 |
| 10 | 0.55 | 0.62 | +0.07 |
| 12 | 0.31 | 0.41 | +0.10 |

Simulation overestimates PDR at long range by ~7–10%, likely due to terrain undulation not captured by flat-earth Okumura-Hata. A correction factor of 0.90× is applied to simulated PDR for distances > 8 km in rural scenarios.

**Measurement campaign:** Conducted Q1 2026, Barmer district, Rajasthan. SF9, 125 kHz, CR 4/5, 14 dBm TX, 2.5m antenna height, 64-byte packets, 100 packets per distance point. Reference: IRIS Field Test Report FTR-2026-001.

---

## 8. References

- Okumura-Hata model: Hata, M. (1980). "Empirical formula for propagation loss in land mobile radio services"
- LoRa modulation: Semtech AN1200.22, "LoRa Modulation Basics"
- Air time calculator: Semtech application note LoRa Modem Calculator Tool
- WPC India frequency allocation: https://dot.gov.in/spectrum-management
- IRIS field test: FTR-2026-001 (internal)
