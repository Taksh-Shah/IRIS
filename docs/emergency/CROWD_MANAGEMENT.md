# IRIS Crowd Management

**Document ID:** IRIS-ARCH-EMRG-002  
**Version:** 1.0  
**Status:** Active  

---

## 1. Problem: High-Density Node Environments

IRIS mesh networking is designed for sparse disaster scenarios (10–200 nodes over km-scale areas). However, specific disaster contexts create temporary extremely high-density node environments:

- **Religious gatherings:** Kumbh Mela (50–100 million participants), Hajj, temple festivals
- **Stadium evacuations:** 50,000–80,000 people in 1–2 km² during an emergency
- **Urban disaster concentration:** Survivors converging on a rescue staging area
- **Evacuation bottlenecks:** Thousands of people at a bridge, ferry terminal, or checkpoint

In these scenarios, IRIS must handle 500–5,000 IRIS-enabled devices within BLE range (50m radius = ~7,850 m²) simultaneously. Standard mesh networking protocols are not designed for this density.

**Why this is hard:**
- BLE advertising uses 3 channels (37, 38, 39). At 1,000+ nodes all advertising, collision probability is extremely high.
- Each node discovers 100s of peers; routing tables explode.
- A single P0 SOS may be copied and re-broadcast by L=5 spray-and-wait × 1,000 concurrent senders, generating 5,000 redundant transmissions.
- LoRa duty cycle (1%) means only ~10 messages per link per hour; at 1,000 nodes this is completely saturated.

---

## 2. Crowd Mode Detection

IRIS automatically enters crowd mode when density thresholds are exceeded:

```rust
pub struct CrowdDetector {
    peer_count_window: SlidingWindowCounter,  // peers seen in last 60 seconds
    density_estimate: f64,                    // estimated nodes/m² (requires GPS)
}

impl CrowdDetector {
    pub fn assess(&self, peer_count: usize, area_m2: Option<f64>) -> CrowdMode {
        // Peer-count-based detection (always available)
        if peer_count > 200 {
            return CrowdMode::Extreme;
        }
        if peer_count > 100 {
            return CrowdMode::High;
        }
        if peer_count > 50 {
            return CrowdMode::Moderate;
        }
        
        // GPS-based density detection (when GPS available)
        if let Some(area) = area_m2 {
            let density = peer_count as f64 / area;
            if density > 1.0 {  // > 1 IRIS node per m²
                return CrowdMode::Extreme;
            }
        }
        
        CrowdMode::Normal
    }
}

pub enum CrowdMode {
    Normal,    // < 50 peers in range
    Moderate,  // 50-100 peers
    High,      // 100-200 peers
    Extreme,   // > 200 peers
}
```

---

## 3. BLE Advertisement Collision Avoidance

### 3.1 The Collision Problem

Standard BLE advertising sends packets on channels 37, 38, 39 in round-robin. With N devices advertising simultaneously, the probability of collision on a given channel in a given advertising interval is approximately:

```
P(collision) ≈ 1 - (1 - 1/N)^(N-1) → 1 - e^(-1) ≈ 0.63 at large N
```

At N=100, most advertisements collide. At N=1,000, the channel is effectively saturated and no advertisements are reliably received.

### 3.2 IRIS Adaptive Advertising Interval

In crowd mode, IRIS increases the BLE advertising interval and adds random jitter to spread transmissions across time:

| Crowd Mode | Base Interval | Jitter | Effective Rate per Node |
|-----------|--------------|--------|------------------------|
| Normal | 100 ms | ±20 ms | 10 adv/second |
| Moderate | 500 ms | ±100 ms | 2 adv/second |
| High | 2,000 ms | ±500 ms | 0.5 adv/second |
| Extreme | 5,000 ms | ±2,000 ms | 0.2 adv/second |

At Extreme mode with 1,000 nodes, effective advertisement rate = 0.2 × 1,000 = 200 advertisements/second across 3 BLE channels = ~67 per channel/second. At BLE 5.x with 2 Mbps PHY, channel capacity is ~200 1-byte advertisements/second, giving ~70% utilization — manageable.

```rust
fn advertising_interval(mode: CrowdMode) -> Duration {
    let base = match mode {
        CrowdMode::Normal => Duration::from_millis(100),
        CrowdMode::Moderate => Duration::from_millis(500),
        CrowdMode::High => Duration::from_millis(2000),
        CrowdMode::Extreme => Duration::from_millis(5000),
    };
    
    // Add random jitter: ±20% of base
    let jitter_range = base / 5;
    let jitter = rand::thread_rng().gen_range(0..jitter_range.as_millis() as u64);
    base + Duration::from_millis(jitter) - jitter_range / 2
}
```

### 3.3 BLE Scan Window Reduction

In addition to advertising interval, the scan window (time spent listening) is reduced in crowd mode to save power and reduce receiver saturation:

| Crowd Mode | Scan Window | Scan Interval | Duty Cycle |
|-----------|-------------|---------------|------------|
| Normal | 100 ms | 100 ms | 100% |
| Moderate | 50 ms | 200 ms | 25% |
| High | 30 ms | 500 ms | 6% |
| Extreme | 20 ms | 1,000 ms | 2% |

This reduces power consumption significantly in crowd mode and prevents receive buffer overflow.

---

## 4. Channel Partitioning

### 4.1 BLE Channel Hash Assignment

In Extreme crowd mode, IRIS uses a channel assignment scheme to spread nodes across the 3 BLE advertising channels:

```rust
fn preferred_advertising_channel(node_id: &NodeId) -> BleChannel {
    // Deterministic assignment based on NodeId
    let channel_index = node_id.as_bytes()[0] % 3;
    match channel_index {
        0 => BleChannel::Ch37,
        1 => BleChannel::Ch38,
        2 => BleChannel::Ch39,
        _ => unreachable!(),
    }
}
```

Each node primarily advertises on one channel, reducing per-channel utilization by approximately 3×. Scanning still covers all channels.

### 4.2 Wi-Fi Direct Channel Selection

Wi-Fi Direct (Wi-Fi P2P) operates on 2.4 GHz and 5 GHz bands. In crowd mode, IRIS attempts to negotiate channels that minimize interference with nearby nodes:

- Nodes scan for occupied Wi-Fi channels and prefer less-occupied ones
- 5 GHz is preferred over 2.4 GHz in crowd mode (more available channels, less interference from BLE)
- Group Owner negotiation in Wi-Fi Direct is biased toward nodes that have been operating longer (more stable group formation)

### 4.3 LoRa Channel Management in Crowds

In crowd scenarios, LoRa is effectively disabled for general mesh traffic (duty cycle makes it unusable at scale). LoRa is reserved exclusively for:
- P0 SOS bundles from nodes with no other connectivity
- Gateway uplink (gateway node aggregates local P0 messages and sends via LoRa to NDRF control)

---

## 5. Rate Limiting in Crowd Mode

### 5.1 Per-Device Rate Limits

In crowd mode, IRIS enforces rate limits to prevent any single node from dominating the shared radio channel:

| Priority | Normal Mode | Moderate Crowd | High Crowd | Extreme Crowd |
|----------|------------|----------------|------------|---------------|
| P0 SOS | Unlimited | 1 per 5 min | 1 per 10 min | 1 per 15 min |
| P1 Medical | 10 per hour | 5 per hour | 2 per hour | 1 per hour |
| P2 Location | 12 per hour | 4 per hour | 1 per hour | 1 per 2 hours |
| P3 Text | 30 per hour | 6 per hour | 2 per hour | 1 per 5 min |
| P4+ | 60 per hour | 10 per hour | 2 per hour | 1 per 10 min |

**P0 rate limiting:** P0 SOS in crowd mode is rate-limited to reduce false SOS flooding (SR-001 risk). The first SOS is always immediately transmitted. A second SOS within 5 minutes (in Moderate crowd) requires a UI confirmation: "You sent an SOS 3 minutes ago. Are you still in danger? [Send Again / Cancel]".

```rust
pub struct CrowdRateLimiter {
    mode: CrowdMode,
    // Per-NodeId per-priority: last_send_time
    last_sends: DashMap<(NodeId, Priority), SystemTime>,
}

impl CrowdRateLimiter {
    pub fn check(&self, node_id: NodeId, priority: Priority) -> RateLimitResult {
        let now = SystemTime::now();
        let min_interval = self.min_interval(priority, &self.mode);
        
        let last_send = self.last_sends
            .get(&(node_id, priority))
            .map(|t| *t);
        
        if let Some(last) = last_send {
            let elapsed = now.duration_since(last).unwrap_or_default();
            if elapsed < min_interval {
                return RateLimitResult::Throttled {
                    wait: min_interval - elapsed,
                    priority,
                };
            }
        }
        
        self.last_sends.insert((node_id, priority), now);
        RateLimitResult::Allowed
    }
}
```

### 5.2 Relay Rate Limits

Relay nodes also apply crowd-mode rate limits to bundles they are relaying. This prevents a single source from saturating the relay's outbound queue even if they bypass sender-side rate limits:

```rust
fn relay_rate_check(
    bundle: &Bundle,
    relay_limiter: &RelayRateLimiter,
) -> bool {
    // P0 always relayed regardless of rate
    if bundle.priority() == Priority::P0 {
        return true;
    }
    relay_limiter.check(bundle.source_node_id(), bundle.priority()).is_allowed()
}
```

---

## 6. Designated Relay Nodes (Crowd Infrastructure)

### 6.1 Infrastructure Node Role

In planned high-density events (festivals, large gatherings), IRIS supports pre-positioning of designated relay nodes. These are IRIS gateway devices (typically Raspberry Pi Zero 2W + LoRa + Wi-Fi) placed at strategic positions in the venue:

- One relay node per ~500 attendees
- Relay nodes have the `RELAY_PRIORITY` capability token
- They operate as Wi-Fi Direct Group Owners (persistent GO role)
- They maintain LoRa uplink to event control
- They have larger storage (8 GB vs typical phone storage allocation of 256 MB)

### 6.2 Relay Node Presence Advertisement

Relay nodes advertise their presence using a BLE advertisement extension that indicates they are a designated relay:

```
BLE Advertisement Extension (AD Type 0xFF, Company ID: IRIS):
  [1 byte: IRIS_AD_TYPE_RELAY_NODE]
  [1 byte: relay_capacity_percent]   ; 0-100, current queue occupancy
  [4 bytes: relay_node_id_prefix]    ; first 4 bytes of NodeId
```

End-user devices seeing a relay node advertisement prefer to route through the relay (higher bandwidth, more storage, LoRa uplink) rather than peer-to-peer.

### 6.3 Satellite-Connected Gateway

In events with satellite connectivity (Starlink terminal on-site), a single gateway node bridges IRIS mesh to internet:
- Aggregates all P0 SOS from the local mesh
- Uploads to IRIS cloud dashboard for emergency services
- Downloads authority broadcasts from IRIS cloud and injects into mesh
- Acts as time synchronization source for the local mesh

This provides a two-tier architecture: local DTN mesh for resilience + internet uplink for speed when available.

---

## 7. Crowd Mode Performance Targets

| Scenario | Node Count | Target P0 Delivery | Target P3 Delivery | Notes |
|----------|-----------|--------------------|--------------------|-------|
| Stadium emergency | 1,000 in 100m radius | > 95% within 60s | > 60% within 10 min | P0 via BLE flood |
| Festival (distributed) | 5,000 in 500m radius | > 90% within 5 min | > 50% within 30 min | Multiple relay nodes |
| Evacuation chokepoint | 500 in 50m radius | > 99% within 30s | > 70% within 5 min | Dense but manageable |
| Urban disaster staging | 200 in 200m radius | > 99% within 30s | > 80% within 5 min | Normal crowd mode |

These targets assume crowd mode is active and at least 1 designated relay node is present per 500 users for the festival scenario.

---

## 8. Crowd Mode Exit

IRIS automatically exits crowd mode when peer count drops below threshold:

| Exit Threshold | Hysteresis |
|---------------|-----------|
| < 30 peers → Normal | Requires 5 min below threshold (prevents oscillation) |
| < 75 peers → Moderate | 3 min below threshold |
| < 150 peers → High | 2 min below threshold |

Hysteresis prevents rapid mode switching as the peer count fluctuates near a threshold, which would cause unstable advertising interval behavior.
