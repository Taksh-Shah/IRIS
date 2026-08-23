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
| SBD data rate | 340 B MO / 270 B MT (96xx modules); 95xx handsets 1960/1890 B; ~10 messages/hour practical |
| SBD latency | network transit ~5–20 s by size; practical end-to-end 20–90 s+ incl. polling cadence |
| Terminal: Iridium GO! | ~₹40,000 purchase, ~₹1,500/month + ₹15–60/message |
| Terminal: Iridium 9575 | Handset, ~₹55,000 |
| IRIS integration | Iridium GO! provides Wi-Fi hotspot, smartphone connects |

### Iridium Short Burst Data (SBD)

SBD is the key protocol for IRIS P0–P2 satellite relay:

```
Maximum MO (Mobile Originated) message: 340 bytes (96xx modules only; 95xx handsets support 1960 B MO / 1890 B MT)
Maximum MT (Mobile Terminated) message: 270 bytes (96xx); 1890 bytes (95xx)
Latency: network transit ~5–20 s by size; practical end-to-end 20–90 s+ including polling cadence and relay processing
Billing: minimum billable message 10 B; RX mailbox checks cost ~$0.05 even when no MT message is queued (RECEIVING COSTS MONEY - FC-7)
```

**Envelope carry (SAT-001, DEC-SAT-0002)**: IRIS carries its standard CRYPTO-001 signed envelopes VERBATIM inside SBD payloads - the P0 envelope (~237 B incl. full 64-byte Ed25519 signature) fits the 340-B MO budget with headroom. There is NO bespoke satellite wire struct and NO signature truncation: crypto material is never shortened. Field-level parsing happens at the envelope layer above the transport; the transport enforces directional size caps only (MO ≤ 340 B, MT ≤ 270 B on 96xx-class links). Mesh-relayed traffic rides the MT leg, so IRIS advertises 270 B for relayed messages; the full MO budget is available for gateway egress.

### Iridium GO! / GO! exec

The GO! family provides a Wi-Fi hotspot over Iridium services. The ONLY
official programmatic interface is the Iridium GO! Application Developer
Program (ifp.iridium.com): NDA + signed agreement + mandatory application
certification, exposing SOAP-over-CSD and a SIP-based SBD connection.
**"Iridium GO! does not support open source projects."** No public REST API
exists at 192.168.0.1 (earlier sketches of `/api/sbd/send` were unverifiable
and have been REMOVED).

**IRIS posture**: GO!/GO!-exec deployments are EXTERNAL integrations only -
an operator-run GO! feeding an IRIS relay over IP. The IRIS core never talks
to GO! directly; v1 targets AT-command SBD modems instead.
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

## Iridium IoT Evolution (2024-2026)

Legacy SBD remains actively sold with **no published EOL**, but the ecosystem
is steering toward three forward paths (Iridium press, Feb 2026):

- **Certus 9704 / IMT** (Dec 2024): topic-based pub/sub messaging, MQTT via
  partners (RockREMOTE/Ground Control).
- **Iridium 9604 hybrid module** (commercial Jun 23, 2026): SBD + LTE-M
  Cat-M1 + GNSS, unified AT command set - the recommended v1 IRIS target
  alongside RockBLOCK 9603-class modems.
- **NTN Direct NB-IoT** (3GPP Rel-19 over Iridium L-band): trials Jan/Jun
  2026, service launching 2026 - not yet commercially mature.
- **CloudConnect**: native AWS IoT Core integration for SBD AND IMT messages
  (relay-side option).

All future backends slot in behind the SAT-001 adapter-injected seam without
core changes (DEC-SAT-0001).
## India Regulatory Status

**CORRECTED per RES-0028 (2026-08-22 research; supersedes earlier claims):**

1. **Starlink**: DoT Letter of Intent May 7, 2025; GMPCS licence granted
   ~Jun 5-6, 2025; IN-SPACe authorization Jul 9, 2025 (Gen1 constellation,
   valid to Jul 7, 2030); spectrum assignment Sep 2025; testing Oct 2025.
   Commercial launch still pending as of mid-2026 (targeted late 2026).
   (Earlier "DoT approved 2024" claim was WRONG.)

2. **Iridium**: available in India through licensed service partners.
   **REMOVED**: an earlier claim that "Bharti Airtel is Iridium's India
   partner" FAILED verification (Airtel's satellite relationships are with
   Starlink/OneWeb). The current authorized Iridium airtime channel post-
   Telecom Act 2023 requires LEGAL_REVIEW confirmation.

3. **Telecommunications Act 2023** (No. 44 of 2023, effective Dec 24, 2023)
   replaced the Telegraph Act framework: unauthorized possession/use of
   satellite phones and two-way satellite messengers is a CRIMINAL offense
   (penalties up to 3 years imprisonment). Customs Circular 37/2010 still
   governs imports (declare + DoT permission). Devices in the prohibited-
   without-permission class include Garmin inReach, SPOT, and Iridium GO!.

4. **IRIS gate (DEC-SAT-0006)**: NO field activation of TX-capable satellite
   units in India without legal sign-off AND an identified authorized
   GMPCS/MSS channel. Receive-only GPS remains unaffected.

5. **Uplink regulations**: transmitting from Indian soil always requires
   operator authorization via service agreements; TRAI satcom recommendations
   (May 2025): administrative assignment, 4% AGR, Rs 500/subscriber/yr urban.

See `legal/SATELLITE_REGULATION.md` and LEGAL-001 carry for the open counsel
question list.
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
6. **P0 exemption**: emergency sends NEVER wait for budgets or confirmation
   (industry SOS-exempt pattern: Zoleo/Garmin) - DEC-SAT-0003.
7. **RX costs count**: mailbox checks bill even when empty; poll cadence must
   be budget-aware (FC-7), and counters persist across reboots (CostLedger).
8. **No LZ4 in v1**: compression overhead dominates at <=340-B sizes;
   benchmark before adopting.

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

## Security Posture

**CORRECTED per RES-0028 / arXiv:2603.12062 (Mar 2026)**: the Iridium L-band
link provides NO effective authentication or confidentiality - SIM keys are
extractable (full device cloning), signaling is unencrypted, downlink
spoofing is demonstrable (devices accept fake Ring Alerts), recorded auth
bursts replay, and ~1 mW jamming disrupts reception regionally. Passive
geolocation to ~10 km is also possible (RECORD, USENIX Sec 2024).

**IRIS posture (DEC-SAT-0005)**: treat satellite as a HOSTILE PIPE -
- Every inbound envelope MUST pass CRYPTO-001 signature verification ABOVE
  this transport before any UI/alert/effect surface.
- Ring Alerts are wake-up hints ONLY, never content or authenticity signals.
- Freshness uses per-sender monotonic sequence numbers (timestamps alone are
  unsound under multi-hour store-and-forward).
- No transport trust anchors exist or will be added.
- Satellite is best-effort redundancy: never presented as guaranteed
  delivery (~1 mW jamming defeats it locally).
- Any satellite TX reveals approximate location (~10 km) - accepted for P0.
## Field Deployment Checklist

Before deploying satellite-capable IRIS node:

- [ ] Iridium GO! charged (battery 8+ hours)
- [ ] SIM card activated and account has credit
- [ ] Test SBD: send test message, verify receipt at relay
- [ ] Verify IRIS satellite mode enabled and P0 routing confirmed
- [ ] Know satellite hotspot Wi-Fi credentials (stored in IRIS)
- [ ] Backup: spare Iridium handset (9575) for voice if GO! fails
- [ ] Register terminal with local disaster management authority
