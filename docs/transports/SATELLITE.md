# Satellite Transport

## Overview

Satellite communication is IRIS's ultimate fallback — the transport that works when
all terrestrial infrastructure (cellular, Wi-Fi, LoRa) has failed. Satellite coverage
is independent of ground infrastructure and survives the disasters that destroy it.

**Design principle**: satellite is expensive, power-hungry, and has very limited
bandwidth. IRIS uses it **exclusively** for P0–P2 emergency messages. No status updates,
no logistics, no video — only life-safety messages transit satellite.

## Satellite Options Evaluated

### Starlink

**Technology**: Low Earth Orbit (LEO) constellation, Ku/Ka band.

| Parameter | Value |
|-----------|-------|
| Altitude | 340–570 km (LEO) |
| Latency | 20–40 ms |
| Throughput | 100–200 Mbps download, 20–40 Mbps upload |
| Terminal cost | ~₹30,000 (Starlink Roam kit) |
| Monthly cost | ₹3,000–₹5,000 (Roam plan) |
| Terminal size | ~30 cm dish |
| IP connectivity | Full IPv4/IPv6 |
| India regulatory | Licensed by DoT (2024 approval for mobility use) |

Starlink is the best option for data bandwidth but requires a dish antenna that is
not trivially portable. The Starlink Roam flat-panel is vehicle-mountable.

**IRIS integration**: Starlink provides full IP connectivity. IRIS runs over TCP/IP/QUIC
exactly as with cellular Internet. No Starlink-specific code required — it appears as
an Internet gateway.

**Use case**: disaster command centers, mobile command vehicles, base camps.
Not suitable for individual rescuers (dish not portable enough).

### Iridium

**Technology**: LEO constellation, 66 satellites, L-band (1.6 GHz).

| Parameter | Value |
|-----------|-------|
| Altitude | 780 km |
| Coverage | Truly global including poles |
| Voice | 2.4 kbps CODEC (narrow but comprehensible) |
| SBD data rate | ~340 bytes per message, ~10 messages/hour practical |
| SBD latency | 20–90 seconds (polling interval) |
| Terminal: Iridium GO! | ~₹40,000 purchase, ~₹1,500/month + ₹15–60/message |
| Terminal: Iridium 9575 | Handset, ~₹55,000 |
| IRIS integration | Iridium GO! provides Wi-Fi hotspot, smartphone connects |

### Iridium Short Burst Data (SBD)

SBD is the key protocol for IRIS P0–P2 satellite relay:

```
Maximum MO (Mobile Originated) message: 340 bytes
Maximum MT (Mobile Terminated) message: 270 bytes
Latency: 20–90 seconds from transmission to delivery
Cost: ~$0.05–0.15 per message (depends on plan)
```

**340 bytes is perfectly sized for IRIS P0–P2 messages**:

```rust
// IRIS P0 SOS packed for SBD (target: ≤ 340 bytes)
pub struct SbdIrisMessage {
    pub header: [u8; 4],       // "IRIS" magic
    pub version: u8,           // protocol version
    pub priority: u8,          // 0 for P0
    pub sender_id: [u8; 16],   // truncated NodeId (first 16 bytes)
    pub recipient_id: [u8; 16], // or broadcast [0xFF; 16]
    pub timestamp: u32,        // Unix timestamp, 32-bit (sufficient until 2106)
    pub sequence: u16,
    pub location_lat: i32,     // fixed-point, microdegrees (divide by 1e6)
    pub location_lon: i32,
    pub message_type: u8,
    pub payload_len: u8,
    pub payload: [u8; 200],    // UTF-8 message text, max 200 bytes
    pub signature: [u8; 32],   // Ed25519 signature truncated to 32 bytes
    // Total: 4+1+1+16+16+4+2+4+4+1+1+200+32 = 286 bytes — fits in SBD
}
```

### Iridium GO! Integration

The Iridium GO! device creates a Wi-Fi hotspot that phones connect to:

```
Rescue Team Phone (IRIS app)
    ↓ Wi-Fi to 192.168.0.1
Iridium GO! Device
    ↓ Iridium L-band satellite link
Iridium ground station
    ↓ Internet
IRIS Relay Server (receives P0–P2 from satellite)
    ↓ Distributes via Internet/mesh
Destination Node
```

IRIS connects to Iridium GO! via its REST API (local HTTP on 192.168.0.1):

```rust
pub struct IridiumGoClient {
    base_url: String, // "http://192.168.0.1"
    session_cookie: Option<String>,
}

impl IridiumGoClient {
    pub async fn send_sbd_message(&self, payload: &[u8]) -> Result<SbdMessageId> {
        assert!(payload.len() <= 340, "SBD MO max 340 bytes");
        let resp = self.http.post(&format!("{}/api/sbd/send", self.base_url))
            .json(&SbdSendRequest { payload: BASE64.encode(payload) })
            .send().await?;
        Ok(resp.json::<SbdSendResponse>().await?.message_id)
    }

    pub async fn check_inbox(&self) -> Result<Vec<SbdIncomingMessage>> {
        let resp = self.http.get(&format!("{}/api/sbd/inbox", self.base_url))
            .send().await?;
        Ok(resp.json::<SbdInboxResponse>().await?.messages)
    }
}
```

### Thuraya

Regional geostationary satellite covering Asia, Middle East, Africa:

| Parameter | Value |
|-----------|-------|
| Coverage | India: full coverage |
| Data | up to 444 kbps (IP, XT-LITE terminal) |
| Latency | 600–800 ms (geostationary) |
| Terminals | ~₹60,000–₹1,20,000 |
| Cost | ~₹400–₹800/MB of data |

Thuraya is too expensive per MB for general IRIS use. Reserve for P0–P1 only where
Iridium SBD is insufficient (when >340 bytes needed, rare).

### VSAT (BGAN / ViaSat)

Geostationary broadband satellite:

| Parameter | Value |
|-----------|-------|
| Throughput | 500 kbps – 2 Mbps (BGAN M2M) |
| Latency | 500–700 ms (geostationary) |
| Terminal | Inmarsat BGAN: ~₹1,50,000 |
| Suitable for | Fixed disaster command centers only |

VSAT is for base camp and command center use, not field nodes. IRIS treats VSAT
as an Internet gateway (same as Starlink, but higher latency).

## India Regulatory Status

### SATCOM Policy

The Department of Telecommunications (DoT) and IN-SPACe govern satellite operations:

1. **Starlink**: Approved by DoT in 2024 for mobility/maritime services. Individual
   consumer terminals do not require separate license — covered by operator's license.

2. **Iridium**: Licensed in India via Bharti Airtel (Bharti is Iridium's India partner).
   Iridium terminals can be used by individuals; no separate user license needed.

3. **Thuraya**: Licensed through Indian operators. User terminal operation: legal with
   registered terminal.

4. **Uplink regulations**: transmitting from Indian soil via satellite always requires
   operator authorization — which Iridium/Starlink/Thuraya provide to end customers
   through their service agreements.

See `legal/SATELLITE_REGULATION.md` for detailed DoT/IN-SPACe policy analysis.

## Priority Routing to Satellite

Only P0–P2 messages are eligible for satellite routing:

```rust
pub fn satellite_eligible(msg: &IrisMessage) -> bool {
    match msg.priority {
        Priority::P0 | Priority::P1 | Priority::P2 => true,
        _ => false,
    }
}

pub fn select_satellite_provider(msg: &IrisMessage) -> Option<SatelliteProvider> {
    let available = SatelliteRegistry::available_providers();
    // Prefer Iridium SBD for P0 (lowest latency, global coverage)
    // Prefer Starlink/VSAT for P1–P2 if available (more bandwidth)
    match msg.priority {
        Priority::P0 => available.iter().find(|p| p.sbd_capable()),
        Priority::P1 | Priority::P2 => {
            available.iter().find(|p| p.ip_capable())
                .or_else(|| available.iter().find(|p| p.sbd_capable()))
        }
        _ => None,
    }
}
```

## Cost Control

Satellite transmission has real monetary cost. IRIS enforces:

1. **Priority gate**: P3+ messages cannot use satellite, period (enforced at API level)
2. **Deduplication**: never send same message twice via satellite (SBD is expensive)
3. **Compression**: LZ4 compress payload before satellite encoding
4. **Rate limit**: max 10 satellite transmissions per hour (configurable)
5. **User confirmation**: for non-P0 satellite use, UI shows "This will use satellite
   (estimated cost: ~₹10). Confirm?"

```rust
pub struct SatelliteCostGuard {
    hourly_count: u32,
    hourly_limit: u32,  // default: 10
    daily_cost_inr: f64,
    daily_budget_inr: f64, // default: 200.0
}
```

## Simulated Satellite Transport

```rust
pub struct SimulatedSatelliteTransport {
    pub provider: SatelliteProvider,
    pub latency_ms: u32,
    pub max_payload_bytes: usize,
    pub messages_per_hour: u32,
    pub simulate_outages: bool,
    pub outage_probability: f32,
}

impl TransportAdapter for SimulatedSatelliteTransport {
    fn estimated_bandwidth_bps(&self) -> u64 {
        // Iridium SBD: 340 bytes × 10/hour = 3400 bytes/hour ≈ 1 byte/second
        match self.provider {
            SatelliteProvider::IridiumSBD => 1,
            SatelliteProvider::Starlink => 10_000_000, // 10 Mbps
            SatelliteProvider::Thuraya => 50_000,      // 50 kbps
        }
    }

    fn estimated_latency_ms(&self) -> u32 { self.latency_ms }
    fn max_message_size_bytes(&self) -> usize { self.max_payload_bytes }
    fn duty_cycle_remaining(&self) -> f32 { 1.0 } // no duty cycle on satellite
}
```

## Field Deployment Checklist

Before deploying satellite-capable IRIS node:

- [ ] Iridium GO! charged (battery 8+ hours)
- [ ] SIM card activated and account has credit
- [ ] Test SBD: send test message, verify receipt at relay
- [ ] Verify IRIS satellite mode enabled and P0 routing confirmed
- [ ] Know satellite hotspot Wi-Fi credentials (stored in IRIS)
- [ ] Backup: spare Iridium handset (9575) for voice if GO! fails
- [ ] Register terminal with local disaster management authority
