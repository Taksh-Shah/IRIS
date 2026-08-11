# Traffic Models — Message Generation for IRIS Simulation

**Component:** `iris-sim/traffic` (Python)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

Traffic models define how messages are generated in simulation. IRIS uses a layered approach:

1. A **base process** defines inter-arrival times (Poisson, bursty, periodic)
2. A **profile** sets parameter values for each disaster phase
3. A **priority distribution** determines what fraction of messages belong to each priority class

All traffic models are parameterized via YAML and instantiated by the simulation framework at startup.

---

## 2. Base Message Generation Processes

### 2.1 Poisson Process (Normal / Background Traffic)

Message arrivals follow a Poisson process with rate λ (messages per hour per node):

```
P(k arrivals in time t) = (λt)^k × e^{-λt} / k!
Inter-arrival time ~ Exponential(λ)
```

Appropriate for: steady-state background traffic, recovery phase, normal operations.

```python
import numpy as np

class PoissonTrafficSource:
    def __init__(self, rate_per_hour: float, priority: Priority):
        self.rate = rate_per_hour / 3600  # convert to per-second
        self.priority = priority

    def next_arrival_s(self) -> float:
        return np.random.exponential(1.0 / self.rate)
```

### 2.2 Bursty Process (Disaster Onset)

Disaster onset generates a burst of SOS and emergency messages. Modeled as a Hawkes process (self-exciting):

```
λ(t) = λ₀ + Σ_{t_i < t} α × exp(-β × (t - t_i))
```

where α is the excitation magnitude (each message arrival increases the rate of subsequent arrivals) and β is the decay rate.

**Intuition:** An earthquake triggers SOS messages; the SOS messages alert responders who generate more P1 medical messages; the cascade decays over ~30 minutes.

```python
class HawkesTrafficSource:
    def __init__(self, base_rate: float, alpha: float, beta: float, priority: Priority):
        self.base_rate = base_rate
        self.alpha = alpha
        self.beta = beta
        self.priority = priority
        self.history: list[float] = []

    def rate_at(self, t: float) -> float:
        excitation = sum(
            self.alpha * np.exp(-self.beta * (t - ti))
            for ti in self.history if ti < t
        )
        return self.base_rate + excitation
```

### 2.3 Periodic Process (Heartbeat / Location Beacons)

Location beacons and status updates are sent on a fixed interval with small jitter:

```
t_next = t_last + interval + Uniform(-jitter, +jitter)
```

```python
class PeriodicTrafficSource:
    def __init__(self, interval_s: float, jitter_s: float, priority: Priority):
        self.interval_s = interval_s
        self.jitter_s = jitter_s
        self.priority = priority

    def next_arrival_s(self) -> float:
        return self.interval_s + np.random.uniform(-self.jitter_s, self.jitter_s)
```

---

## 3. Traffic Profiles by Disaster Phase

### Phase 1: Pre-Disaster (Normal)

Low-rate background traffic. SOS rate is near zero.

| Priority | Process | Rate | Message size |
|---|---|---|---|
| P0 (SOS) | Poisson | 0.01 / node / hour | 64 bytes |
| P2 (Location) | Periodic | 1 / node / 5 min | 128 bytes |
| P3 (Emergency text) | Poisson | 0.1 / node / hour | 512 bytes |
| P4 (Normal) | Poisson | 2 / node / hour | 1024 bytes |

### Phase 2: Disaster Onset (0–30 min)

Sudden burst of SOS and medical messages. Network may be fragmenting simultaneously.

| Priority | Process | Rate | Excitation α | Decay β |
|---|---|---|---|---|
| P0 (SOS) | Hawkes | 5 / node / hour base | 2.0 | 0.1 |
| P1 (Medical) | Hawkes | 10 / node / hour base | 1.5 | 0.08 |
| P2 (Location) | Periodic | 1 / node / 1 min | — | — |
| P3 (Emergency text) | Poisson | 20 / node / hour | — | — |

### Phase 3: Sustained Disaster (30 min – 12 hours)

Ongoing operations. SOS rate stabilizes. Responder coordination traffic increases.

| Priority | Process | Rate | Message size |
|---|---|---|---|
| P0 (SOS) | Poisson | 1 / node / hour | 64 bytes |
| P1 (Medical) | Poisson | 5 / node / hour | 256 bytes |
| P2 (Location) | Periodic | 1 / node / 2 min | 128 bytes |
| P3 (Emergency text) | Poisson | 10 / node / hour | 512 bytes |
| P4 (Normal) | Poisson | 5 / node / hour | 1024 bytes |

### Phase 4: Recovery (12 hours+)

Network partially restored. Traffic shifts toward logistics and media.

| Priority | Process | Rate |
|---|---|---|
| P0 (SOS) | Poisson | 0.1 / node / hour |
| P2 (Location) | Periodic | 1 / node / 5 min |
| P4 (Normal) | Poisson | 10 / node / hour |
| P5–P7 (Media) | Poisson | 2 / node / hour |

---

## 4. Message Size Distributions

Message sizes follow log-normal distributions parameterized per priority class:

| Priority | Mean size (bytes) | Std dev | Max (bytes) | TTL |
|---|---|---|---|---|
| P0 (SOS) | 64 | 16 | 256 | 72 hours |
| P1 (Medical) | 256 | 64 | 1024 | 24 hours |
| P2 (Location) | 128 | 0 (fixed) | 128 | 1 hour |
| P3 (Emergency text) | 512 | 256 | 4096 | 12 hours |
| P4 (Normal) | 1024 | 512 | 65536 | 6 hours |
| P5 (Photo) | 512000 | 256000 | 2097152 | 2 hours |
| P6 (Audio) | 262144 | 131072 | 1048576 | 1 hour |
| P7 (Video) | 5242880 | 2621440 | 20971520 | 30 min |

---

## 5. India-Specific Population Density Models

### 5.1 Ahmedabad Urban (EVAL-001 basis)

Node placement uses actual population density from census data, sampled at district level:

```python
AHMEDABAD_DISTRICTS = {
    "Maninagar":    {"density_per_km2": 45000, "area_km2": 8.2},
    "Satellite":    {"density_per_km2": 12000, "area_km2": 15.1},
    "Naroda":       {"density_per_km2": 28000, "area_km2": 12.3},
    "Chandkheda":   {"density_per_km2": 8000,  "area_km2": 22.4},
    "Vatva":        {"density_per_km2": 18000, "area_km2": 9.7},
}
```

Node density in simulation is 1 IRIS user per 200 residents (conservative responder penetration estimate).

### 5.2 Mumbai (High-Density Scenario)

Mumbai's Dharavi and surrounding areas represent extreme density conditions:

| Area | Density (nodes/km²) | Scenario use |
|---|---|---|
| Dharavi core | 850 | Stress test BLE discovery |
| Bandra West | 120 | Mixed transport test |
| Thane outskirts | 45 | Suburb scenario |

At 850 nodes/km², BLE simultaneously discovers >50 peers — used to test discovery storms.

### 5.3 Rural Kutch (Low-Density, Post-Earthquake)

The 2001 Bhuj earthquake informs the rural sparse scenario:

```yaml
kutch_rural:
  area_km2: 45652
  node_count: 50
  node_density_per_km2: 0.0011
  primary_transport: lora  # BLE useless at these distances
  mobility_model: levy_flight
  speed_mps: [0, 1.5]  # foot/motorcycle
```

LoRa is the primary transport. Average inter-node distance: ~30 km. Multi-hop LoRa chains of 3–5 hops are expected.

---

## 6. Traffic Configuration YAML Schema

```yaml
traffic:
  seed: 42
  phase: disaster_onset  # pre_disaster | disaster_onset | sustained | recovery
  sources:
    - type: hawkes
      priority: P0
      base_rate_per_node_per_hour: 5.0
      alpha: 2.0
      beta: 0.1
      message_size_bytes:
        distribution: lognormal
        mean: 64
        std: 16
        max: 256
      ttl_hours: 72

    - type: periodic
      priority: P2
      interval_s: 120
      jitter_s: 10
      message_size_bytes: 128
      ttl_hours: 1

    - type: poisson
      priority: P4
      rate_per_node_per_hour: 5.0
      message_size_bytes:
        distribution: lognormal
        mean: 1024
        std: 512
        max: 65536
      ttl_hours: 6
```

---

## 7. Simulation Validation Against Real-World Data

Where available, simulation traffic parameters are calibrated against field data:

| Data source | Parameter calibrated |
|---|---|
| 2015 Nepal earthquake HAM radio logs | P0 arrival rate during disaster onset |
| 2001 Gujarat earthquake field reports | Inter-contact times in rural sparse networks |
| Operational DTN deployments (DakNet, rural India) | P4 traffic volumes, message sizes |
| NDMA India incident database | Disaster phase transitions, duration distributions |

**Known gap:** No ground-truth data exists for IRIS-style multi-transport mesh in an Indian disaster context. Simulation parameters are best estimates pending field deployment data.

---

## 8. References

- Hawkes process: Hawkes, A.G. (1971). "Spectra of some self-exciting and mutually exciting point processes"
- DTN traffic modeling: Kaur et al., "Traffic Characterization in Delay-Tolerant Networks", 2018
- India census data: censusindia.gov.in (2011, district-level)
- NDMA incident database: ndma.gov.in
- ONE simulator traffic modules: https://github.com/akeranen/the-one
