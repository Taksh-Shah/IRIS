# Mobility Models

## Overview

Mobility models determine how nodes move through the simulation environment. Node movement drives contact opportunities — when and for how long nodes come within radio range of each other. Realistic mobility is therefore critical for accurate routing algorithm evaluation.

IRIS implements a library of mobility models, from simple mathematical abstractions to scenario-specific models calibrated for Indian disaster contexts. All models implement a common trait:

```rust
pub trait MobilityModel: Send + Sync {
    /// Initialize node's starting position and state
    fn initialize(&mut self, rng: &mut SimRng, bounds: &GeoBounds) -> GeoPoint;

    /// Given current position and sim time, return next waypoint and expected arrival time
    fn next_waypoint(&mut self, current: GeoPoint, sim_time: f64, rng: &mut SimRng)
        -> (GeoPoint, f64);  // (position, arrival_time)

    /// Called when waypoint is reached — model can update internal state
    fn on_waypoint_reached(&mut self, position: GeoPoint, sim_time: f64);
}
```

## Random Waypoint Model (RWP)

The classical mobility model. Each node independently chooses a random destination within the simulation area, travels to it at a random speed, pauses for a random duration, then repeats.

### Parameters

```rust
pub struct RandomWaypointModel {
    pub speed_min_mps: f64,   // Minimum speed (m/s)
    pub speed_max_mps: f64,   // Maximum speed (m/s)
    pub pause_min_s: f64,     // Minimum pause time at waypoint
    pub pause_max_s: f64,     // Maximum pause time at waypoint
    pub bounds: GeoBounds,    // Geographic bounds
}
```

### Known Issues

Random Waypoint has well-documented problems:
- **Density wave**: nodes cluster near center of bounds over time
- **Speed decay**: mean speed decreases over time as nodes pausing pull down the average
- **No social structure**: real people cluster and move in groups

**Mitigation**: Use steady-state distribution for initial positions (not random uniform). Apply speed correction. For social movement, use group models.

### Typical Configuration

```yaml
mobility:
  model: random_waypoint
  speed_range_mps: [0.8, 1.8]     # Walking speed
  pause_range_s: [0, 120]
```

For vehicles:
```yaml
mobility:
  model: random_waypoint
  speed_range_mps: [5.0, 15.0]    # 18-54 km/h (urban traffic)
  pause_range_s: [10, 60]         # Stopping at lights
```

## Random Direction Model

Addresses the density-wave problem of RWP. Each node chooses a random direction, moves in that direction for a random time, reflects off boundaries, pauses, then picks a new direction.

```rust
pub struct RandomDirectionModel {
    pub speed_min_mps: f64,
    pub speed_max_mps: f64,
    pub move_duration_range_s: (f64, f64),
    pub pause_duration_range_s: (f64, f64),
    current_direction_rad: f64,
    current_speed_mps: f64,
}
```

**Produces**: more uniform spatial distribution than RWP. Better for modeling sparse environments where nodes roam widely.

## Gauss-Markov Model

Adds temporal correlation to velocity — nodes don't instantly change direction. Speed and direction evolve smoothly over time, creating realistic curved paths.

```rust
pub struct GaussMarkovModel {
    pub alpha: f64,           // Memory parameter: 0=memoryless, 1=constant direction
    pub mean_speed_mps: f64,
    pub mean_direction_rad: f64,
    pub speed_variance: f64,
    pub direction_variance: f64,
    pub update_interval_s: f64,  // How often to update velocity

    // State
    current_speed: f64,
    current_direction: f64,
}

impl GaussMarkovModel {
    fn update_velocity(&mut self, rng: &mut SimRng) {
        // Gauss-Markov update: v(t+1) = alpha*v(t) + (1-alpha)*mean + sqrt(1-alpha^2)*N(0,sigma)
        self.current_speed = self.alpha * self.current_speed
            + (1.0 - self.alpha) * self.mean_speed_mps
            + (1.0 - self.alpha.powi(2)).sqrt() * rng.normal(0.0, self.speed_variance);

        self.current_direction = self.alpha * self.current_direction
            + (1.0 - self.alpha) * self.mean_direction_rad
            + (1.0 - self.alpha.powi(2)).sqrt() * rng.normal(0.0, self.direction_variance);
    }
}
```

**Best for**: Modeling vehicle movement on roads (high alpha ~0.9) or pedestrian wandering (alpha ~0.5).

## Disaster Scenario Mobility

Purpose-built for disaster response scenarios. Models the two dominant movement patterns during a disaster:

### Evacuation Model (Civilian Fleeing)

```rust
pub struct EvacuationModel {
    pub disaster_epicenter: GeoPoint,
    pub evacuation_routes: Vec<EvacuationRoute>,
    pub safe_zones: Vec<SafeZone>,
    pub crowd_factor: f64,         // 0-1: how crowded routes are
    pub panic_fraction: f64,       // Fraction that move randomly (panicked)
}

pub struct EvacuationRoute {
    pub waypoints: Vec<GeoPoint>,
    pub capacity: u32,             // Max nodes on this route
    pub current_load: u32,
}
```

Behavior:
- Non-panicked civilians identify nearest evacuation route and follow it
- Panicked civilians use Random Direction for 5-30 minutes, then find route
- Speed is congestion-adjusted: slower as route load increases
- Clustering at safe zones creates high-density stationary groups

### Responder Arrival Model

```rust
pub struct ResponderArrivalModel {
    pub staging_area: GeoPoint,
    pub arrival_rate_per_hour: f64,   // How many responders arrive per hour
    pub assignment_zones: Vec<GeoPoint>,  // Areas to be assigned to

    // Phase-based behavior
    pub current_phase: ResponderPhase,
}

pub enum ResponderPhase {
    Staging,                    // At staging area, waiting for assignment
    Transit { destination: GeoPoint },   // Moving to assigned zone
    PatrolZone { center: GeoPoint, radius_m: f64 },  // Systematic area coverage
    Extraction { victim_loc: GeoPoint, hospital: GeoPoint },  // Transporting victim
}
```

## Vehicle Mobility

### Emergency Vehicle Model

```rust
pub struct EmergencyVehicleModel {
    pub vehicle_type: VehicleType,   // Ambulance, Fire, Police
    pub base_station: GeoPoint,
    pub road_network: RoadNetwork,   // Graph of roads
    pub speed_mps: f64,              // Emergency speed (faster than traffic)

    // Dispatch pattern
    pub dispatch_rate_per_hour: f64,
    pub mission_duration_s: (f64, f64),  // Range of mission durations
}
```

Emergency vehicles follow road network, move fast (40-80 km/h), and have hub-spoke pattern (base → incident → base).

### Bus/Public Transit Model

Buses create periodic, predictable contact opportunities — important for scheduled DTN routing.

```rust
pub struct BusRouteModel {
    pub route: Vec<BusStop>,
    pub schedule: Vec<f64>,         // Departure times from first stop
    pub stop_duration_s: f64,
    pub inter_stop_travel_s: Vec<f64>,
    pub passengers: Vec<PassengerModel>,  // Passengers who board/alight
}

pub struct BusStop {
    pub position: GeoPoint,
    pub daily_boarding: u32,
    pub daily_alighting: u32,
}
```

**Key insight**: Bus routes create predictable contact opportunities that routing algorithms can exploit. PRoPHET performs better than Epidemic in scenarios with scheduled transportation.

## Pedestrian Group Mobility

Real pedestrians move in social groups — families, colleagues, rescue teams. Individual random motion poorly models this.

### Reference Point Group Mobility (RPGM)

```rust
pub struct RPGMModel {
    pub group_center: GroupLeaderModel,  // Leader follows some mobility model
    pub group_radius_m: f64,             // Max distance from leader
    pub cohesion_factor: f64,            // 0=independent, 1=always at leader
    pub member_speed_mps: (f64, f64),

    // Group can split and merge
    pub split_probability: f64,
    pub merge_probability: f64,
}
```

Members follow the group leader with some random offset. Groups can split (leader picks new destination, some members stay) and merge (two groups converge on same location).

### Disaster-Specific Group Patterns

```yaml
# Family evacuation group
- model: rpgm
  leader: evacuation_model
  group_size_range: [2, 6]
  group_radius_m: 15
  cohesion_factor: 0.8

# Rescue team
- model: rpgm
  leader: responder_model
  group_size_range: [3, 8]
  group_radius_m: 30
  cohesion_factor: 0.9
  movement: systematic_search  # Leader uses grid-search pattern
```

## Static Node Placement

Some nodes are stationary infrastructure: LoRa gateways, Raspberry Pi relay nodes, deployed repeaters.

```rust
pub struct StaticModel {
    pub position: GeoPoint,
}

pub struct StaticClusterModel {
    // Multiple static nodes deployed together (e.g., command post)
    pub center: GeoPoint,
    pub radius_m: f64,
    pub count: u32,
}
```

Static nodes use deterministic placement patterns:
- **Grid**: evenly spaced coverage
- **Hilbert curve**: space-filling curve for area coverage
- **Road following**: nodes placed along major roads
- **Rooftop**: placed at building/rooftop positions for LoRa elevation benefit

## India-Specific Mobility Patterns

### Crowded Urban (Delhi, Mumbai, Kolkata)

High node density, slow movement, complex social clustering:

```yaml
mobility:
  model: composite
  components:
    # Dense pedestrian crowd
    - model: rpgm
      weight: 0.5
      params:
        leader: gauss_markov
        mean_speed_mps: 0.5     # Very slow in crowd (1.8 km/h)
        group_size: [2, 20]     # Large groups in markets, transit
        cohesion: 0.6
    # Auto-rickshaw/scooter traffic
    - model: road_constrained_gauss_markov
      weight: 0.3
      speed_mps: [3.0, 8.0]    # 10-30 km/h in congested traffic
    # Static street vendors/shops
    - model: static_cluster
      weight: 0.2
      radius_m: 5
```

**Key characteristics**:
- Very high BLE contact density (many nodes within 10m)
- Short contact durations (people pass each other)
- Long contact durations at gathering points (market, bus stop, temple)
- High BLE collision rate in dense areas — model accordingly

### Rural Sparse (Rajasthan, Bihar, UP districts)

Low node density, larger travel distances:

```yaml
mobility:
  model: composite
  components:
    # Village inhabitants
    - model: home_point
      weight: 0.6
      params:
        home_radius_m: 500      # Move within 500m of home village
        travel_probability: 0.1 # 10% chance of traveling to another village per day
        travel_distance_km: [2, 20]
    # Inter-village bus routes
    - model: bus_route
      weight: 0.2
    # Agricultural workers (field to home)
    - model: scheduled
      weight: 0.2
      schedule:
        - at: "06:00", move_to: field_location
        - at: "12:00", move_to: home_location
        - at: "14:00", move_to: field_location
        - at: "18:00", move_to: home_location
```

**Key characteristics**:
- Contacts are rare (nodes far apart most of time)
- Contact opportunities concentrate at bus stops, markets, water sources
- Long contact durations when contact occurs (shared bus ride, market)
- LoRa critical for sparse rural — BLE range insufficient

### Vehicle Convoy (Military/NGO Logistics)

Multiple vehicles moving together in convoy formation:

```rust
pub struct ConvoyModel {
    pub lead_vehicle: VehicleModel,
    pub followers: Vec<ConvoyFollower>,
    pub inter_vehicle_distance_m: f64,
    pub convoy_speed_mps: f64,
    pub waypoints: Vec<GeoPoint>,
}

pub struct ConvoyFollower {
    pub offset_m: f64,   // Distance behind leader
    pub lateral_jitter_m: f64,  // Side-to-side variation
}
```

Convoys create prolonged contact opportunities between convoy members. They are important for store-carry-forward routing — vehicles carry messages between disconnected zones.

## Calibration from GPS Traces

Real-world GPS traces are used to calibrate mobility model parameters.

### Data Sources

1. **GeoLife dataset** (Microsoft Research): 182 users, Beijing, 17,621 trajectories
2. **MDC dataset** (EPFL): 185 users, 18-month trace, Switzerland
3. **Cabspotting**: San Francisco taxi traces
4. **India-specific**: Will collect from field deployments; currently use manual observation

### Calibration Methodology

```python
# tools/analysis/mobility_calibration.py

def calibrate_rwp(traces: List[GPSTrace]) -> RandomWaypointParams:
    """Fit RWP parameters to GPS traces using MLE"""

    # Extract inter-waypoint distances
    distances = []
    speeds = []
    pauses = []

    for trace in traces:
        waypoints = extract_waypoints(trace, speed_threshold_mps=0.3)
        for i in range(len(waypoints) - 1):
            distances.append(haversine(waypoints[i].pos, waypoints[i+1].pos))
            speeds.append(mean_speed_between(waypoints[i], waypoints[i+1]))
        pauses.extend(extract_pause_durations(trace))

    # Fit distributions
    speed_dist = fit_truncated_normal(speeds)
    pause_dist = fit_exponential(pauses)
    distance_dist = fit_levy(distances)  # Levy flight for human mobility

    return RandomWaypointParams(
        speed_min=speed_dist.mean - 2*speed_dist.std,
        speed_max=speed_dist.mean + 2*speed_dist.std,
        pause_min=pause_dist.scale * 0.1,
        pause_max=pause_dist.scale * 3.0,
    )

def validate_contact_statistics(
    sim_result: SimResult,
    real_traces: List[GPSTrace]
) -> CalibrationReport:
    """Compare contact inter-contact times between simulation and real traces"""

    sim_contacts = extract_contact_statistics(sim_result)
    real_contacts = extract_contact_statistics_from_traces(real_traces, range_m=10)

    return CalibrationReport(
        ict_ks_test=ks_test(sim_contacts.inter_contact_times,
                             real_contacts.inter_contact_times),
        contact_duration_ks_test=ks_test(sim_contacts.durations,
                                          real_contacts.durations),
        mean_contacts_per_hour_ratio=sim_contacts.mean_rate / real_contacts.mean_rate,
    )
```

### Key Statistics to Match

| Statistic | Why It Matters |
|-----------|----------------|
| Inter-contact time distribution | Determines how often routing opportunities arise |
| Contact duration distribution | Determines how much data can be transferred per contact |
| Mean degree (avg neighbors at time t) | Determines routing algorithm performance |
| Spatial clustering coefficient | Determines effectiveness of geographic routing |
| Speed distribution | Affects contact duration and churn rate |

### Acceptable Calibration Thresholds

- Inter-contact time: KS statistic < 0.15 (distributions similar)
- Contact duration: KS statistic < 0.20
- Mean contact rate: simulation within ±25% of real traces
- Spatial clustering: within ±30%

Traces failing these thresholds require model parameter adjustment before use in algorithm evaluation.
