# Gateway Architecture

## What Is a Gateway
A gateway node bridges between the IRIS mesh and an external network (Internet, LoRa backbone, satellite). Any node can be a gateway if it has external connectivity. Gateways are discovered dynamically — there is no central gateway registry, no DNS, no static configuration required.

Gateways are the points where a message enters or exits the IRIS mesh. Without gateways, IRIS operates as a fully self-contained mesh. With gateways, messages can reach the broader Internet, emergency services, or coordination centers.

## Gateway Types

| Type | External Network | Bandwidth | Latency | Cost | Availability |
|------|----------------|-----------|---------|------|-------------|
| Internet | TCP/IP Internet | 1-100Mbps | 50-200ms | Data plan cost | When Internet available |
| LoRa | LoRa radio network | 250bps-50kbps | 1-10s | Free (ISM band) | When LoRa hardware present |
| Satellite LEO | LEO constellation | 50-200Mbps | 20-40ms | High (subscription) | When terminal visible |
| Satellite GEO | GEO satellite | 1-10Mbps | 500-700ms | High (per message or subscription) | When terminal pointed correctly |
| Satellite Iridium | Iridium constellation | 2.4kbps | 1-3s | Very high (per message) | Global, near-continuous |
| Cellular | Mobile data (4G/5G) | 1-100Mbps | 50-100ms | Data plan cost | When cellular signal available |

### LoRa Gateway Detail
A LoRa gateway is not an Internet gateway — it bridges the local BLE/Wi-Fi mesh to a LoRa radio network.
- Range extension: LoRa can reach 5-15km in open terrain, 2-5km urban
- Capacity: very limited — use only for P0-P3 messages
- Deployment: edge nodes with LoRa module (RAK2287 or similar)
- India regulatory: ISM band 865-867MHz, no license required, duty cycle limits apply

### Satellite Gateway Types in India
- **GSAT (ISRO)**: Indian geostationary satellites, VSAT terminals, operator agreement required
- **Iridium**: LEO, global, handheld terminal possible, per-message cost ~$0.05-0.50
- **Starlink**: LEO constellation, high bandwidth, portable terminal, available in India (regulatory situation evolving)
- **OneWeb**: LEO, enterprise terminals, enterprise pricing

For disaster response, ISRO provides emergency VSAT connectivity to government agencies. NDRF and SDRF teams typically have Iridium or INSAT/GSAT terminals.

## Gateway Discovery

### Capability Advertisement
Gateways announce themselves in the CapabilityBundle exchanged on peer contact:
```yaml
# Example CapabilityBundle gateway section (as CBOR, represented as YAML for readability)
gateway_capabilities:
  - type: INTERNET
    quality: 0.87           # Reliability estimate 0.0-1.0
    bandwidth_class: HIGH   # LOW / MEDIUM / HIGH / VERY_HIGH
    latency_class: LOW      # LOW / MEDIUM / HIGH / VERY_HIGH
    last_tested: 1723276800 # Unix timestamp of last successful probe
  - type: LORA
    quality: 0.72
    bandwidth_class: VERY_LOW
    latency_class: HIGH
    lora_channel_plan: "IN865"
```

### Gossip Propagation
Gateway knowledge propagates via RoutingGossip (see CONTROL_FLOW.md):
- A node with an Internet gateway tells its neighbors
- Neighbors relay this information in their routing gossip
- Gateway knowledge propagates up to 3 hops (configurable)
- Hop limit prevents stale gateway information from propagating widely

### Gateway Quality Tracking
Gateway quality degrades dynamically:
- Successful message delivery via gateway: quality += 0.05 (capped at 1.0)
- Failed delivery (ACK timeout): quality -= 0.10
- No interaction for > 30 minutes: quality decays by 0.01 per minute
- Quality below 0.10: gateway removed from routing consideration

Quality tracks actual delivery performance, not just connectivity:
- Gateway that is connected but fails to deliver messages is penalized
- Gateway with intermittent Internet but high delivery rate is rewarded

## Gateway Selection Algorithm

When a message needs external delivery, gateway selection follows this priority order:

```rust
fn select_gateway(msg: &Message, available_gateways: &[GatewayInfo]) -> Option<GatewayInfo> {
    let candidates: Vec<&GatewayInfo> = available_gateways.iter()
        .filter(|g| g.quality > QUALITY_THRESHOLD && g.can_deliver(msg))
        .collect();
    
    if msg.priority <= Priority::P1 {
        // P0-P1: return ALL gateways for multi-path
        return Some(candidates.clone()); // All of them
    }
    
    // For P2+: score and select single best gateway
    candidates.iter()
        .max_by_key(|g| score_gateway(g, msg))
}

fn score_gateway(g: &GatewayInfo, msg: &Message) -> i64 {
    let bandwidth_score = match g.bandwidth_class {
        VERY_HIGH => 40, HIGH => 30, MEDIUM => 20, LOW => 10, VERY_LOW => 5,
    };
    let latency_score = match g.latency_class {
        LOW => 40, MEDIUM => 30, HIGH => 10, VERY_HIGH => 5,
    };
    let quality_score = (g.quality * 20.0) as i64;
    let hop_penalty = g.hops_away * 5;
    
    bandwidth_score + latency_score + quality_score - hop_penalty
}
```

### Priority Override for Scoring

| Priority | Strategy |
|---------|---------|
| P0, P1 | Use ALL available gateways simultaneously (multi-path) |
| P2, P3 | Use best two gateways (primary + backup) |
| P4 | Use single best gateway |
| P5, P6, P7 | Use Internet only (too large for LoRa/satellite) |

### Gateway Type Restrictions by Message Size

| Message Priority | Max Size | Internet | Cellular | LoRa | Satellite Iridium | Satellite LEO/GEO |
|-----------------|----------|---------|---------|------|------------------|-----------------|
| P0 (255B) | 255 bytes | Yes | Yes | Yes | Yes | Yes |
| P1 (512B) | 512 bytes | Yes | Yes | Yes | Yes | Yes |
| P2 (128B) | 128 bytes | Yes | Yes | Yes | Yes | Yes |
| P3 (1KB) | 1 KB | Yes | Yes | Yes | Possible ($$) | Yes |
| P4 (64KB) | 64 KB | Yes | Yes | No | No | Yes |
| P5 (2MB) | 2 MB | Yes | Yes | No | No | Limited |
| P6 (5MB) | 5 MB | Yes | Yes | No | No | No |
| P7 (50MB) | 50 MB | Yes | Yes | No | No | No |

## Gateway Failure Handling

### Detection
- Heartbeat timeout (60s without heartbeat from gateway node)
- Probe failure (test message to external endpoint fails)
- Gateway's own capability update (reports gateway_type removed)

### Failure Response
1. Mark gateway as `quality = 0.0` (immediately stops new routing to this gateway)
2. Re-evaluate all pending messages routed through failed gateway
3. Attempt re-routing via alternate gateways
4. If no alternate: store messages in `PENDING_GATEWAY` state
5. Notify routing gossip: update gateway sighting with `quality = 0.0, is_available = false`

### Recovery Response
1. Gateway re-appears in discovery (capability includes gateway_type)
2. Run gateway probe (send test message to known external endpoint)
3. If probe succeeds: restore quality to 0.5 (not immediately full trust)
4. Quality builds back up as successful deliveries accumulate
5. Flush all messages in `PENDING_GATEWAY` state through restored gateway

## Internet Gateway Protocol

A node acting as Internet gateway runs the gateway bridge service:

### Inbound (Internet → Mesh)
```
External sender → IRIS relay server (cloud) → Gateway node → Local mesh
                                                     │
                                              (if direct IP known)
External sender → Direct TCP to gateway IP → Gateway node → Local mesh
```

Gateway node listens on configured TCP port (default 47221).
Incoming messages are authenticated (signature verified) before injection into mesh.
Gateway does not decrypt message content (E2EE — gateway is not recipient).

### Outbound (Mesh → Internet)
```
Mesh node → Gateway node → IRIS relay server (cloud) → External recipient
                     │
               (if recipient is IRIS node with known IP)
Mesh node → Gateway node → Direct TCP to recipient → Delivered
```

Gateway forwards outbound messages to:
- IRIS cloud relay server (for IRIS-to-IRIS messaging via Internet)
- Emergency services endpoint (for SOS to NDRF/SDRF coordination center)
- Operator-configured endpoints (for disaster response coordination)

### Protocol for Cloud Relay (When Available)
```
POST https://relay.iris.example.in/api/v1/messages
Content-Type: application/cbor
X-Gateway-Node-Id: {gateway_node_id}
X-Gateway-Signature: {signature of request}
Body: CBOR-encoded message envelope
```

Cloud relay is optional infrastructure. IRIS functions without it. When available, it improves delivery to nodes with Internet access not on the local mesh.

## Multi-Gateway Load Balancing

When multiple Internet gateways are available simultaneously:

### P0-P1 (Emergency): All Gateways
Send via all available gateways simultaneously. First delivery wins. Duplicates filtered by recipient's deduplication. Cost: redundant transmissions. Acceptable for life-safety messages.

### P2-P4 (Normal): Round-Robin by Quality
```rust
fn select_for_normal(gateways: &mut Vec<GatewayInfo>) -> &GatewayInfo {
    // Weighted round-robin by quality
    gateways.sort_by(|a, b| b.quality.partial_cmp(&a.quality).unwrap());
    let total_weight: f32 = gateways.iter().map(|g| g.quality).sum();
    let mut rand_val = rand::random::<f32>() * total_weight;
    for g in gateways.iter() {
        rand_val -= g.quality;
        if rand_val <= 0.0 { return g; }
    }
    &gateways[0]
}
```

### P5-P7 (Bulk): Single Best Gateway
Avoid bandwidth waste from duplicate large transfers. Select highest-bandwidth gateway.

## Gateway Trust Model

### What Gateways Can See
- Message envelope metadata: sender_id, recipient_id, priority, size, timestamp
- They CANNOT see: message content (E2EE — payload is encrypted for recipient only)
- Gateway is a trusted forwarder, not a trusted reader

### What Gateways Cannot Do
- Decrypt message content (they lack recipient's private key)
- Modify message without invalidating Ed25519 signature
- Block messages selectively without detection (ACK timeouts reveal blocking)

### Gateway Accountability
- Every message forwarded via gateway records gateway_node_id in routing log
- If message fails to deliver via a gateway, the gateway_node_id is logged
- Patterns of failure can be reported to operator for investigation
- Cryptographic accountability: gateway cannot deny forwarding (relay leaves provable trace if ACK received downstream)

### Rogue Gateway Risk
A rogue node can falsely advertise gateway capability:
- It will receive messages routed to it as a gateway
- It can drop them (blackhole) or attempt to replay/MITM
- MITM is prevented by E2EE (cannot decrypt content)
- Replay is prevented by deduplication + TTL
- Blackhole is detected via ACK timeouts → gateway quality drops to 0 → removed from routing
- Harm: delay, not interception. Acceptable risk for a decentralized system.

## Gateway Deployment Recommendations for India

### District Level (Population 50,000-500,000)
- 2-4 Internet gateway nodes (multiple ISPs for redundancy)
- 1 satellite terminal (Iridium or GSAT VSAT)
- Internet gateways at: district collector office, hospital, police HQ, fire station

### Block Level (Population 20,000-50,000)
- 1-2 Internet gateway nodes (cellular broadband)
- LoRa gateway connecting to district LoRa backbone
- Location: block development office, community health center

### Village Level (Population 500-5,000)
- LoRa-capable edge node connecting to block LoRa
- No direct Internet gateway (rely on LoRa to block, then Internet at block)
- Solar powered for disaster resilience

### Mobile Gateway (Vehicle)
- Vehicle-mounted router with dual-SIM (BSNL + Jio for coverage diversity)
- Starlink terminal for satellite fallback
- Serves as mobile gateway for convoy operations
