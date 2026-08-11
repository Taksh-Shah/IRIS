# IRIS Sybil Resistance

## The Sybil Problem in Mesh Networks

A Sybil attack occurs when a single adversary creates many fake identities (Sybil nodes) to gain disproportionate influence over a distributed system. In a mesh network, this is particularly severe because:

1. **No central identity authority:** IRIS is permissionless — anyone can generate an Ed25519 key pair and join as a new identity. Key generation takes milliseconds.
2. **Influence scales with node count:** Routing algorithms that weight by "number of nodes seen" or "centrality" can be gamed by creating many Sybil nodes.
3. **No physical cost to identity creation:** Unlike proof-of-work blockchains, there is no computational cost to Sybil identity creation in IRIS.

This document analyzes the impact of Sybil attacks on IRIS and describes the mitigations and their known limitations.

---

## Sybil Attack Impact Analysis

### Impact on Epidemic Routing

IRIS uses epidemic routing (controlled flood with hop limit) as the baseline routing strategy. In epidemic routing:
- Every node relays every message it has not seen before
- Delivery probability increases with network density

**Sybil attack on epidemic routing:**
- An attacker with 100 Sybil nodes in an area makes that area appear densely populated
- Legitimate nodes may prioritize routes through the Sybil cluster
- If the Sybil nodes are blackholes (drop messages), they create a routing black hole that appears attractive
- If the Sybil nodes relay messages, they can collect metadata on all traffic passing through

**Impact severity on epidemic routing:** **Medium** — epidemic routing has inherent redundancy. A message typically reaches its destination via multiple paths. Sybil nodes can reduce efficiency but complete blackholing is difficult when legitimate nodes also relay.

### Impact on PRoPHET/History-Based Routing

PRoPHET routing selects relays based on estimated delivery probability — nodes that have historically contacted a destination more often are preferred relays.

**Sybil attack on PRoPHET:**
- Attacker creates 100 Sybil nodes all claiming high delivery probability to the target
- Legitimate nodes route through Sybil cluster, which drops messages
- The Sybil nodes can "win" the routing competition by falsely reporting high delivery probability
- Falsely reported delivery probabilities cannot be verified (they are self-reported)

**Impact severity on PRoPHET:** **High** — PRoPHET routing is directly gameable by Sybil nodes claiming false probabilities.

**Mitigation specific to PRoPHET:** Delivery probability claims are weighted by observed delivery confirmation receipts. A node that claims high delivery probability but never produces confirmed deliveries is deprioritized over time.

### Impact on Emergency Systems

**Sybil attack scenario on SOS:**
- 100 Sybil nodes simultaneously trigger fake SOS messages
- Emergency responders receive 100 SOS signals, most fake
- Resources are misdirected; real emergencies may be overlooked

**Sybil attack on emergency broadcast:**
- Sybil nodes do not help in spoofing emergency broadcasts (requires valid EAC)
- Sybil nodes CAN amplify a legitimate but malicious broadcast by rapidly forwarding it

**Impact severity on emergency system:** **High** — Sybil spam of SOS has direct safety impact.

**Mitigation:** SOS rate limit per device (3 per hour). However, an attacker with 100 physical devices generates 300 SOS per hour, still a significant abuse vector.

### Impact on Routing Table Poisoning

**Sybil attack scenario:**
- Sybil cluster creates false route advertisements claiming short paths to many destinations
- Legitimate nodes update routing tables to prefer Sybil paths
- Attacker controls routing for a significant portion of the network

**Impact severity:** **Medium** — route advertisements are signed; Sybil nodes cannot forge signatures from legitimate nodes. They can only advertise their own (fake) paths.

### Impact on Community Channels

**Sybil attack scenario:**
- Attacker creates many fake identities to vote on moderation decisions
- "Democracy-based" moderation systems where majority rules are easily gamed
- Sybil nodes can artificially boost or suppress content in community channels

**Impact severity:** **Medium** for public channels. **Low** for private channels (requires contact relationship).

---

## Defense Approaches

### Defense 1: Proof of Work (Not Implemented)

**Concept:** Require each identity to perform a computational proof-of-work challenge before being accepted as a relay node.

**Analysis:**
- Proof of work creates a cost to identity creation
- Scales poorly with device capability — a modern laptop can create 1000 identities per hour at any PoW difficulty a phone can verify in reasonable time
- Battery cost on mobile devices is prohibitive (continuous PoW would drain battery)
- Does not prevent a well-resourced attacker with dedicated hardware

**Decision: NOT IMPLEMENTED.** PoW is too battery-intensive and provides insufficient protection against resourced attackers. It adds friction for legitimate users (emergency responders setting up new nodes) without preventing sophisticated attacks.

### Defense 2: Social Vouching (Partially Implemented)

**Concept:** An identity is trusted only if vouched for by an already-trusted identity. Forms a web of trust.

**Implementation in IRIS:**
- Users can mark contacts as "verified" (via safety number comparison)
- Messages from contacts are given higher trust than messages from strangers
- Messages from contacts-of-contacts receive medium trust
- Messages from strangers with no social path receive low trust

```
Trust Level Calculation:
  direct_contact:                      TRUST_HIGH (level 3)
  contact_of_contact (1 hop):          TRUST_MEDIUM (level 2)
  2 hops removed:                      TRUST_LOW (level 1)
  no social path or path > 2 hops:     TRUST_UNKNOWN (level 0)
```

**Limitations:**
- Depends on users actually verifying their contacts (most users don't)
- A new legitimate user in a disaster area (no existing contacts) gets TRUST_UNKNOWN
- In an emergency, rejecting messages from unknowns is dangerous (could miss real SOS)

**Social vouching is advisory, not blocking.** Messages from TRUST_UNKNOWN nodes are forwarded but deprioritized. SOS messages from TRUST_UNKNOWN nodes are forwarded at normal priority (safety takes priority over Sybil defense).

### Defense 3: Rate Limiting (Implemented)

**Concept:** Limit the rate at which any identity can send messages, SOS signals, or route advertisements.

**Implementation:**

```
Per-identity rate limits (enforced locally at each relay):
  Regular messages:        20 messages/minute, burst to 50
  Route advertisements:    1 per 60 seconds
  SOS signals:             3 per hour (device key bound)
  Emergency broadcasts:    1 per 10 minutes (EAC required anyway)
```

**Token bucket algorithm:**
```
struct RateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64,   // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    fn consume(&mut self, cost: f64) -> bool {
        self.refill();
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }
}
```

**Limitations:**
- Rate limits are enforced per relay node independently. A Sybil cluster of 100 nodes sending 20 messages/minute each generates 2000 messages/minute from the perspective of receiving nodes.
- An attacker willing to use 100 physical devices (not just software identities) can generate 100× the rate-limited volume.

**Effectiveness:** Effective against a single Sybil identity. Less effective against many coordinated Sybil identities.

### Defense 4: Anomaly Detection (Implemented, L2 Intelligence)

**Concept:** Statistical analysis of message patterns to identify Sybil behavior.

**Detection signals:**
- Many new identities appearing simultaneously in the same area (coordinated Sybil creation)
- Identities with zero history suddenly claiming high delivery probabilities
- Message patterns that are statistically unlikely for organic communication
- Identities that never receive messages (only send) — unusual for legitimate users
- Sudden surge in unknown-trust SOS messages from same physical area

**Implementation:**
```
SybilDetector:
  - Track new identity arrival rate by geographic cell
  - Alert threshold: > 10 new identities in < 60 seconds in same cell
  - Track send/receive ratio per identity
  - Alert threshold: send/receive ratio > 50:1 (bot-like behavior)
  - Track delivery success rate for identities claiming high delivery probability
  - Downweight identities with claimed high probability but low confirmed delivery
```

**Limitations:** Anomaly detection generates false positives. In a disaster area with many people downloading IRIS for the first time, many legitimate new identities appear simultaneously — the same signal as a Sybil attack. Thresholds must be tuned carefully.

### Defense 5: Reputation System (Partially Implemented)

**Concept:** Nodes accumulate reputation based on observed behavior. High-reputation nodes are preferred as relays.

**Reputation signals:**
- Confirmed message deliveries (positive)
- Failed deliveries where relay was responsible (negative)
- Time in network (positive — established nodes more trusted)
- Social vouching (positive)

**Limitations:**
- Reputation systems are themselves vulnerable to Sybil attacks (Sybil nodes can earn reputation slowly)
- Reputation requires a network of peers who can observe and rate behavior; sparse networks limit reputation data
- Privacy tension: reputation tracking requires logging node behavior, which is metadata

---

## Sybil Resistance Design Philosophy

IRIS does not claim strong Sybil resistance. This is an acknowledged limitation of the system. The design philosophy is:

1. **Make Sybil attacks expensive, not impossible.** Rate limits, social vouching, and anomaly detection increase the cost and complexity of Sybil attacks.

2. **Limit the impact of Sybil attacks.** Emergency messages cannot be forged by Sybil nodes (require valid keys or EAC). Sybil nodes can disrupt routing but cannot forge content or impersonate real identities.

3. **Don't break legitimate use to prevent Sybil attacks.** In a disaster area with many new users, Sybil defenses must not block legitimate first-time users from participating.

4. **Degrade gracefully.** Even under a Sybil attack, the system should continue to function (with reduced efficiency) rather than failing completely.

---

## Known Limitations

| Limitation | Severity | Mitigation | Residual Risk |
|---|---|---|---|
| Identity creation is free | High | Rate limiting, social vouching | Attacker with many phones can Sybil at scale |
| SOS can be flooded by Sybil nodes | High | Rate limit per device key | Attacker with 100 phones generates 300 SOS/hour |
| PRoPHET routing gameable by false probability claims | High | Delivery confirmation weighting | Attacker with patience can build false reputation |
| New users indistinguishable from Sybil nodes | Medium | Social trust, time-based reputation | False positives in anomaly detection |
| Reputation system requires network observation | Medium | Multi-relay consensus | Sparse networks limit reputation data |

---

## Comparison to Academic Solutions

Several academic proposals address Sybil resistance in mobile ad-hoc networks:

**SybilGuard / SybilLimit (social-network based):** Requires a well-connected social graph. Works poorly in disaster scenarios where social graph is sparse.

**Certifi-Gate (certificate-based):** Requires a PKI infrastructure, which IRIS avoids by design (offline-first).

**Mobility-based Sybil detection:** Detects nodes that move together (implying same physical device) — promising for IRIS's mobile context. Planned for L2 statistical intelligence layer.

**DAMON (density-based):** Detects Sybil clusters by anomalous density — similar to our anomaly detection approach.

None of these solutions is universally effective in the permissionless, offline, mobile mesh context. IRIS's approach is a pragmatic combination that accepts residual risk in exchange for usability.

---

## Research Agenda

- [ ] Evaluate mobility-based Sybil detection (same physical device running multiple identities has correlated movement)
- [ ] Evaluate resource fingerprinting (Sybil nodes running on the same hardware may share performance characteristics)
- [ ] Study Sybil attack patterns in real deployments to calibrate anomaly detection thresholds
- [ ] Evaluate zero-knowledge proof of physical uniqueness approaches (speculative)
