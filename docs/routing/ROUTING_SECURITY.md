# IRIS Routing Security

**Document ID:** IRIS-SEC-ROUTE-004  
**Version:** 1.0  
**Status:** Active  

---

## 1. Threat Model Overview

IRIS operates in adversarial conditions: disaster zones may include bad actors, compromised devices, or nodes operated by entities with conflicting interests. The routing layer is a high-value attack surface because a routing-level compromise can suppress emergency communications silently.

This document covers routing-layer attacks: attacks on the delivery of bundles rather than on their content (content attacks are in `docs/security/`). Routing attacks are particularly dangerous because they do not require breaking cryptography — they exploit the trust relationships that routing protocols inherently extend to peers.

**Attacker capabilities assumed:**
- Can operate one or more IRIS nodes in the network
- May have access to legitimate IRIS software (modified or unmodified)
- Cannot forge Ed25519 signatures without the private key
- Can selectively drop, delay, or replay bundles they receive
- Can advertise false routing metrics

**Out of scope:** physical capture of devices, side-channel attacks on cryptographic operations, OS-level compromise.

---

## 2. Attack Descriptions

### 2.1 Blackhole Attack

**Description:** A malicious node announces high routing utility (high PRoPHET delivery predictability for many destinations) to attract bundles from neighbors, then silently discards them instead of forwarding.

**Why it is dangerous:** The source node believes delivery is in progress. No error is returned. The bundle may not be retransmitted because the source thinks it has been forwarded. P0 SOS messages can be silently suppressed.

**Attack vector:**
```
Alice [P0 SOS] → Mallory (high P(Mallory, Hospital) announced)
                     ↓ drops bundle
                  Hospital never receives SOS
```

**Detectability:** Moderate. Delivery receipts reveal non-delivery, but only if Alice is eventually reachable by an ACK path. In a partitioned network, detection may be delayed hours.

### 2.2 Wormhole Attack

**Description:** Two colluding nodes (Mallory-1 and Mallory-2) at physically distant network regions create a high-bandwidth "wormhole" tunnel between themselves (e.g., via satellite or cellular). They advertise that they can reach distant destinations with extremely low hop count. Legitimate routing sees a "shortcut" and routes traffic through the wormhole. The attacker can read, modify-then-discard, or selectively forward traffic.

**Why it is dangerous:** Wormhole attacks can expose encrypted metadata (source/destination) even if content is encrypted. Selective forwarding can deny emergency communications to specific targets. In IRIS, the attack requires two nodes, but in disaster zones, this is plausible (e.g., two coordinated actors).

**Attack vector:**
```
Network:     A -- B -- C -- D -- E
Wormhole:    A sees: A -- M1 ~~tunnel~~ M2 -- E  (1 hop)
             Traffic flows through M1-M2; attacker reads routing metadata
```

### 2.3 Sinkhole Attack

**Description:** A malicious node advertises inflated routing quality to become a preferred relay. Unlike a blackhole (which selectively drops after acceptance), a sinkhole actively manipulates PRoPHET or other routing advertisements to attract the maximum possible traffic volume. The attacker may not always drop traffic — partial forwarding makes detection harder.

**Sinkhole advertisement manipulation:**
```
Legitimate node: P(node, hospital) = 0.62
Sinkhole node:   P(node, hospital) = 0.99  (forged high predictability)
```

Since PRoPHET predictability values are self-reported, a malicious node can set any value. IRIS lacks a mechanism to independently verify a node's claimed delivery predictability.

### 2.4 Replay Attack

**Description:** An attacker captures a legitimate bundle (e.g., a flood evacuation order) and retransmits it later, potentially after conditions have changed, causing confusion or panic.

**IRIS mitigation:** Bundle IDs include a creation timestamp. Bundles with creation timestamps more than TTL seconds in the past are rejected by the receiver. The deduplication Bloom filter also prevents re-acceptance within the active 72-hour window.

### 2.5 Routing Information Flooding

**Description:** Attacker floods the network with large numbers of fake bundle IDs or false PRoPHET updates, exhausting routing table memory and CPU on legitimate nodes.

**IRIS mitigation:** PRoPHET table is bounded (1,000 entries max). Incoming routing advertisements from a single peer are rate-limited to 100 updates per 60-second window. Fake bundles without valid Ed25519 signatures are rejected at the validated layer before entering the routing engine.

---

## 3. Detection Mechanisms

### 3.1 Delivery Receipt Tracking

Every forwarding event is logged locally:

```rust
pub struct ForwardingRecord {
    pub bundle_id: BundleId,
    pub forwarded_to: NodeId,
    pub forwarded_at: SystemTime,
    pub transport: TransportType,
    pub delivery_receipt: Option<DeliveryReceiptId>,
    pub receipt_received_at: Option<SystemTime>,
}
```

The anomaly scoring system computes, for each peer used as a relay:

```
receipt_rate(peer) = receipts_received / bundles_forwarded_through_peer
```

Computed over a rolling 6-hour window with minimum 10 samples.

**Blackhole detection threshold:** `receipt_rate < 0.10` (< 10% of forwarded bundles acknowledged).

Note: receipt rate is a lagging indicator. In a partitioned network, legitimate nodes may also show low receipt rates. The system applies context adjustment:

```rust
fn adjusted_receipt_rate(peer: &PeerStats, network: &NetworkState) -> f64 {
    let raw = peer.receipt_rate();
    let network_avg = network.average_receipt_rate();
    // Normalize: if network-wide rate is 30%, a peer at 15% is suspicious
    // If network-wide is 10%, a peer at 9% is normal
    raw / network_avg.max(0.05)
}
```

A node is flagged only if its adjusted receipt rate is < 0.25 (less than 25% of network average).

### 3.2 PRoPHET Consistency Checking

To detect sinkhole attacks (inflated predictability claims), IRIS cross-checks received predictability values against transitive expectations:

```
Expected P(peer, dest) ≤ max(P(self, dest) / 0.5)
```

If a peer claims P(peer, destination) = 0.99 but our own P(self, destination) via other paths is 0.10, the claim is implausibly high. Nodes with consistently implausible predictability claims are flagged.

```rust
fn check_prophet_plausibility(
    peer: NodeId,
    claimed_predictability: f64,
    destination: NodeId,
    our_best_path: f64,
) -> PlausibilityResult {
    let upper_bound = our_best_path / 0.3; // allow 3× our best as upper bound
    if claimed_predictability > upper_bound.min(0.95) {
        PlausibilityResult::Suspicious {
            claimed: claimed_predictability,
            expected_max: upper_bound,
        }
    } else {
        PlausibilityResult::Plausible
    }
}
```

### 3.3 Anomaly Score Accumulation

Each peer has an anomaly score (0.0 = trusted, 1.0 = blacklisted):

| Event | Score Increase |
|-------|---------------|
| receipt_rate < 0.10 sustained 2h | +0.4 |
| receipt_rate < 0.25 sustained 6h | +0.2 |
| PRoPHET plausibility check failed | +0.1 per failure |
| 3+ consecutive transmission failures | +0.05 |
| Delivery receipt received | −0.01 (gradual rehabilitation) |
| Normal operation 1 hour | −0.05 |

Score is clamped to [0.0, 1.0]. Scores decay at rate 0.02/hour during normal operation.

---

## 4. Response Mechanisms

### 4.1 Demotion

When a peer's anomaly score exceeds 0.3, it enters the **demoted** state:

- PRoPHET predictability score for this peer is capped at 0.2 (regardless of claimed value)
- The peer is deprioritized in path selection (path_score multiplied by 0.3)
- Multipath selection excludes the peer as a primary path (can still be secondary)
- Peer is still used for P0 if it is the only available contact

### 4.2 Blacklisting

When anomaly score exceeds 0.8, the peer is **blacklisted**:

- No bundles are forwarded to this peer
- Bundles received FROM this peer are accepted (the peer may be a legitimate receiver)
- The blacklist entry is recorded in local storage with timestamp
- An anomaly report bundle (P3 priority) is generated and propagated to allow other nodes to be warned

```rust
pub struct AnomalyReport {
    pub suspicious_node: NodeId,
    pub reporter_node: NodeId,
    pub anomaly_score: f64,
    pub evidence: AnomalyEvidence,
    pub reporter_signature: Ed25519Signature,
    pub created_at: SystemTime,
}
```

Anomaly reports are cryptographically signed by the reporter. Recipients apply an evidence-weight model: a single report increases the peer's local anomaly score by 0.1; 5+ independent reports from different nodes can trigger blacklisting without local observation.

### 4.3 Epidemic Re-Routing

When a blackhole or sinkhole is detected and P0 bundles may have been affected, IRIS triggers emergency epidemic re-routing:

1. All P0 bundles that were forwarded to the now-suspected node within the last 60 minutes are marked for re-transmission.
2. These bundles are re-queued with Epidemic routing (ignoring PRoPHET scores).
3. Epidemic mode for these bundles lasts 30 minutes or until delivery receipts arrive.
4. The originating application is notified that SOS delivery is uncertain (if the originating node is reachable).

### 4.4 Wormhole Mitigation

Wormhole detection is fundamentally difficult without geographic constraints. IRIS applies two partial mitigations:

**Geographic plausibility check (when GPS available):**
- If a peer is seen at GPS position X and claims to have contacts with nodes that are GPS-confirmed to be > 500 km away via a single hop, the contact is flagged.
- This catches wormholes where the tunnel endpoints are far apart and GPS is functional.

**Transmission time verification:**
- BLE and Wi-Fi Direct have inherent range limits (50m and 200m respectively). A node claiming to forward a message it received via BLE from a node the receiver can independently observe is not within BLE range is flagged.
- This requires 3-node coincidence to detect and is thus only a partial mitigation.

---

## 5. Security Properties and Limitations

| Property | Status | Notes |
|----------|--------|-------|
| Blackhole detection | Partial | Delayed by partition; false positives in poor networks |
| Sinkhole detection | Partial | Plausibility check helps; adversary can claim moderate values |
| Wormhole detection | Minimal | GPS-dependent; no cryptographic protection |
| Replay prevention | Strong | Bundle ID + TTL + Bloom filter dedup |
| Routing advertisement integrity | None | PRoPHET values are self-reported, not signed |
| Anonymity of routing metadata | Weak | Source/destination visible to relays |

**Known limitation:** IRIS does not implement signed routing advertisements. A node can claim any PRoPHET predictability value. Implementing signed routing advertisements would require a gossip protocol for routing state, adding complexity and overhead. This is tracked as a v0.6 improvement item.
