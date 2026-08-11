# Satellite Simulation — Iridium and Starlink Contact Modeling

**Component:** `iris-sim/satellite` (Python)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

Satellite links provide IRIS's last-resort connectivity when all terrestrial transports have failed. The simulation must accurately model:

- Satellite pass timing (when is a satellite overhead?)
- Contact window duration (how long is the link available?)
- Link capacity (how many bytes can be transferred per pass?)
- Cost and latency characteristics

IRIS supports two satellite systems with very different operational profiles:

| System | Type | Primary use in IRIS |
|---|---|---|
| Iridium | LEO, short messages (SBD) | P0–P2 messages only, highest priority fallback |
| Starlink | LEO, broadband | P0–P4 when Starlink terminal is available |

---

## 2. Iridium Orbit Model

### 2.1 Constellation Parameters

| Parameter | Value |
|---|---|
| Active satellites | 66 (Iridium NEXT) |
| Orbit altitude | 780 km |
| Orbital planes | 6 planes × 11 satellites |
| Inclination | 86.4° (near-polar) |
| Orbital period | 100 minutes 28 seconds |
| Ground track repeat | Non-repeating (full Earth coverage) |

### 2.2 Contact Window Model

For a ground station at latitude φ, the expected contact parameters:

- **Contact frequency:** At mid-latitudes (India: 8°–37°N), Iridium provides approximately 1 satellite overhead every 9–11 minutes
- **Contact window duration:** 7–12 minutes per pass (depends on elevation angle cutoff; IRIS uses 5° minimum elevation)
- **Daily contact time:** ~80–100 minutes per day of usable contact windows

**Simplified contact schedule generation:**

```python
import math
from datetime import datetime, timedelta

IRIDIUM_ORBITAL_PERIOD_MIN = 100.47
IRIDIUM_SATELLITES = 66
IRIDIUM_PLANES = 6

def iridium_contact_schedule(
    lat_deg: float,
    lon_deg: float,
    start_time: datetime,
    duration_hours: float,
) -> list[tuple[datetime, datetime]]:
    """
    Returns list of (contact_start, contact_end) tuples.
    Simplified model: uniform contact spacing with Gaussian duration jitter.
    For production, use skyfield with TLE data.
    """
    contact_interval_min = IRIDIUM_ORBITAL_PERIOD_MIN / (IRIDIUM_SATELLITES / IRIDIUM_PLANES)
    contacts = []
    t = start_time + timedelta(minutes=np.random.uniform(0, contact_interval_min))
    
    while t < start_time + timedelta(hours=duration_hours):
        duration_min = np.random.normal(loc=9.0, scale=1.5)
        duration_min = max(3.0, min(12.0, duration_min))
        contacts.append((t, t + timedelta(minutes=duration_min)))
        t += timedelta(minutes=np.random.normal(
            loc=contact_interval_min, scale=0.5
        ))
    
    return contacts
```

**For high-fidelity simulation:** Use `skyfield` with current Iridium TLE files:

```python
from skyfield.api import load, wgs84, EarthSatellite

def precise_iridium_passes(lat, lon, start_jd, end_jd):
    ts = load.timescale()
    eph = load('de421.bsp')
    # Load TLE from Celestrak: https://celestrak.org/SOCRATES/query.php
    satellites = load.tle_file('iridium-NEXT.tle')
    observer = wgs84.latlon(lat, lon)
    # Find passes for each satellite, return windows with elevation > 5°
    # ... (standard skyfield pass-finding loop)
```

### 2.3 Link Budget — Iridium SBD

| Parameter | Value |
|---|---|
| Protocol | Short Burst Data (SBD) |
| Maximum MO (mobile-originated) message size | 340 bytes |
| Maximum MT (mobile-terminated) message size | 270 bytes |
| Data rate | 2.4 kbps (raw), ~1.0 kbps effective |
| Latency (MO to gateway) | 1–5 seconds |
| Latency (MT, gateway to device) | 20–60 seconds |
| Cost per MO message | ~$0.05 USD (at standard Iridium SBD rates) |
| Cost per MT message | ~$0.05 USD |
| Modem power (Rockblock 9603) | TX: 1.5 W, Standby: 1.5 mA |

**IRIS Iridium usage policy:** Only P0 and P1 messages are sent via Iridium SBD due to per-message cost. P0 message payload is designed to fit within 340 bytes (with header compression).

### 2.4 Message Encoding for Iridium

```rust
// P0 SOS message must fit in 340 bytes
pub struct IridiumSBDPayload {
    pub version: u8,         // 1 byte
    pub priority: u8,        // 1 byte  
    pub node_id: [u8; 8],    // 8 bytes (truncated Blake3 of full NodeId)
    pub location_lat: i32,   // 4 bytes (microdegrees)
    pub location_lon: i32,   // 4 bytes (microdegrees)
    pub timestamp: u32,      // 4 bytes (Unix seconds)
    pub message_hash: [u8; 8], // 8 bytes (truncated)
    pub payload: [u8; 312],  // 312 bytes of compressed message body
}
// Total: 340 bytes exactly
```

---

## 3. Starlink Pass Model

### 3.1 Constellation Parameters (Shell 1)

| Parameter | Value |
|---|---|
| Shell altitude | 550 km |
| Inclination | 53° |
| Satellites (operational, 2026) | ~4,400 |
| Orbital period | ~95.5 minutes |

### 3.2 Contact Frequency for India

India's latitude range (8°–37°N) is within the 53° inclination shell coverage:

- **Contact frequency:** Near-continuous for latitudes below 53°. At any given moment, 3–8 Starlink satellites are visible above 25° elevation.
- **Effective contact window:** Continuous (no gap periods below 53° latitude)
- **Caveat:** Starlink requires a terminal (dish); IRIS edge servers with Starlink are always-on when powered.

### 3.3 Link Budget — Starlink

| Parameter | Value |
|---|---|
| Downlink throughput | 50–250 Mbps (typical user terminal) |
| Uplink throughput | 20–40 Mbps |
| Latency | 20–40 ms (low Earth orbit) |
| Monthly data cap | Varies by plan; IRIS uses priority access tier |
| Terminal power consumption | ~100 W (Starlink standard dish) |

**IRIS Starlink usage:** Available only on fixed edge servers with power. Not suitable for handheld devices. Used as a backhaul link rather than a message relay transport.

---

## 4. Simulation Contact Schedule YAML Format

Satellite contacts are pre-computed and stored as YAML contact schedules for deterministic simulation runs:

```yaml
satellite_contacts:
  node_id: edge-server-001
  location:
    lat: 23.0225
    lon: 72.5714
    name: "Ahmedabad Edge Server"
  generated_at: "2026-08-11T00:00:00Z"
  simulation_duration_hours: 24

  iridium_contacts:
    - start: "2026-08-11T00:09:23Z"
      end:   "2026-08-11T00:18:47Z"
      max_elevation_deg: 72.3
      available_bytes_mo: 340
      available_bytes_mt: 270
      cost_usd: 0.10

    - start: "2026-08-11T00:19:51Z"
      end:   "2026-08-11T00:28:12Z"
      max_elevation_deg: 31.8
      available_bytes_mo: 340
      available_bytes_mt: 270
      cost_usd: 0.10

  starlink_contacts:
    # For Starlink, contact is effectively continuous
    - start: "2026-08-11T00:00:00Z"
      end:   "2026-08-12T00:00:00Z"
      throughput_mbps: 80
      latency_ms: 28
```

### 4.1 Schedule Generation Tool

```bash
# Generate Iridium contact schedule using TLE data
python -m iris_sim.satellite.schedule \
    --lat 23.0225 --lon 72.5714 \
    --start "2026-08-11T00:00:00Z" \
    --duration-hours 24 \
    --system iridium \
    --tle-file data/iridium-next.tle \
    --output schedules/ahmedabad-iridium.yaml
```

---

## 5. Simulation Integration with ONE Simulator

IRIS simulation can run in two modes:

**Mode 1: Native Python simulation** — uses the contact schedule YAML directly. Satellite contacts are injected as transport events at the scheduled times.

**Mode 2: ONE simulator integration** — IRIS exports contact schedule as ONE-compatible `ExternalMovement` files:

```
# ONE contact schedule format
0.0 edge-server-001 iridium-gateway-001 CONNECTED
547.0 edge-server-001 iridium-gateway-001 DISCONNECTED
614.0 edge-server-001 iridium-gateway-002 CONNECTED
```

The ONE simulator is used for algorithm comparison benchmarks only (PRoPHET validation). IRIS's native simulation is used for all other purposes.

---

## 6. Cost Modeling

Iridium message costs are tracked in simulation to enforce budget constraints:

```python
class IridiumCostTracker:
    def __init__(self, budget_usd_per_day: float = 10.0):
        self.budget = budget_usd_per_day
        self.spent = 0.0

    def can_send(self, priority: Priority) -> bool:
        # P0 always sends regardless of budget
        if priority == Priority.P0:
            return True
        # P1 sends if budget is not more than 90% exhausted
        if priority == Priority.P1:
            return self.spent < self.budget * 0.9
        return self.spent < self.budget

    def record_send(self, cost_usd: float):
        self.spent += cost_usd
```

---

## 7. Validation

| Validation check | Method | Target |
|---|---|---|
| Iridium contact frequency | Compare simulated vs. Iridium SBD SLA (1 satellite always visible) | Within ±5% of SLA |
| Iridium window duration | Compare vs. published orbital parameters | Within ±30 seconds per pass |
| Starlink latency | Compare simulated vs. published latency specs | Within ±10 ms |
| Message cost accounting | Verify cost tracking matches per-message rates | Exact match |

---

## 8. References

- Iridium SBD developer guide: https://www.iridium.com/products/iridium-short-burst-data/
- Skyfield orbital mechanics: https://rhodesmill.org/skyfield/
- Starlink technical overview: https://www.starlink.com/technology
- Celestrak TLE archive: https://celestrak.org/
- ONE simulator: Keranen et al. (2009), "The ONE simulator for DTN protocol evaluation"
