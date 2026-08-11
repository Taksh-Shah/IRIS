# Routing-Specific Attacks and Mitigations

## Overview

IRIS relies on cooperative routing: nodes volunteer to carry and forward messages for others.
This cooperation creates an attack surface where malicious nodes can manipulate routing to
their advantage — dropping messages, poisoning routing tables, or creating phantom shortcuts.
This document covers routing-specific attacks (distinct from DoS, spam, and Sybil attacks
covered in their respective documents).

## Attack 1: Blackhole Attack

### Description

A malicious relay node advertises high connectivity and willingness to forward messages,
but silently drops all messages it receives. Neighboring nodes continue routing through the
blackhole because routing tables show it as a "good" next hop.

Impact: Messages routed through the blackhole are lost silently. Senders are unaware. In
a sparse network, the blackhole may be the only path to a destination, causing message loss.

### Detection

Blackhole nodes are detected via delivery acknowledgement analysis:
- Sender sets a delivery acknowledgement request on P0-P3 messages
- ACK is sent by recipient when message is received
- ACK travels back through relay chain (or any available path)
- If sender does not receive ACK within `ack_timeout`, suspect blackhole on that path

### Mitigation

**Multi-path routing for P0-P1:**
```
When sending P0/P1:
  1. Select top-3 next hops by estimated delivery probability
  2. Send message on ALL 3 paths simultaneously
  3. First ACK received → cancel remaining sends
  
Cost: 3× bandwidth usage for P0-P1. Acceptable for emergency messages.
```

**ACK-based path blacklisting:**
```
If ACK not received within timeout AND message not found via alternative path:
  decrease delivery_probability[this_hop][destination] by 0.5
  If delivery_probability < 0.1: mark as suspicious, avoid for routing
  Reset after 24 hours (node may have been temporarily offline)
```

**Fallback routing:**
```
On ACK timeout:
  1. Retry via second-best next hop (if different from first)
  2. Wait for ACK
  3. If still no ACK: store and carry for direct delivery
```

### Limitations

Detection requires ACK round-trip, which may take minutes in sparse networks.
Blackhole cannot be definitively distinguished from a node that went offline.
Probabilistic mitigation: reduce use of suspected blackhole, not eliminate entirely.

## Attack 2: Wormhole Attack

### Description

Two malicious nodes far apart secretly communicate via an out-of-band channel (Internet,
private radio, etc.), creating a "wormhole" that makes them appear to be neighbors. This
allows them to:
- Attract traffic that thinks it found a short path
- Observe traffic flowing through the wormhole
- Selectively drop or delay messages

Example: Malicious node A in Mumbai and malicious node B in Chennai establish a wormhole.
A advertises to Mumbai nodes that it can reach Chennai in 2 hops. Traffic floods toward A,
tunnels to B, and arrives in Chennai — but through a path the routing algorithm did not
account for.

### Detection

Wormhole creates anomalies detectable by these signals:
- Hop count anomaly: message arrives at destination with hop_count=2, but geographic
  distance suggests >10 hops would be needed
- Timing anomaly: relay claims to have forwarded a message before it could have physically
  arrived via the declared path
- Delivery probability anomaly: a node claims extremely high delivery probability to a
  geographically distant destination

### Mitigation

**Geographic hop-count bounds checking:**
```
MAX_HOPS_EXPECTED = ceil(geographic_distance_km / 0.5km_per_hop) × 3  // generous factor

If hop_count < MAX_HOPS_EXPECTED / 4:
  log anomaly("Suspiciously short path: {} hops for {}km", hop_count, distance)
  weight delivery_probability lower for this path
```

This requires approximate geographic knowledge. If location is unknown, skip this check.

**Relay path timestamps:**
Each relay optionally records a timestamp when it forwarded the message. If relay_path is
available (future feature), verify that timestamps are monotonically increasing and physically
plausible (at least 100ms between relays assuming fastest possible RF propagation).

**Note**: wormhole attack is sophisticated and hard to fully prevent without
specialized hardware (distance bounding). Current mitigations are heuristic.

## Attack 3: Selective Forwarding

### Description

A malicious relay forwards most messages correctly but selectively drops specific messages —
for example, messages from a competitor's application, messages from certain NodeIds, or
messages with certain payload types.

The node appears functional to most traffic, making it hard to detect.

### Mitigation

Same as blackhole, applied per-message-type:
- ACK-based detection: senders notice ACK not received for dropped message categories
- Multi-path routing: P0-P1 sent via multiple paths, defeating selective forwarding
- Delivery probability tracking per message type (future enhancement)

## Attack 4: Route Poisoning

### Description

A malicious node advertises false routing information — specifically, claiming very high
delivery probability to destinations it cannot actually reach. Neighboring nodes update
their routing tables based on the false claim, routing traffic through the attacker.

In DTN protocols like PRoPHET, delivery probability is shared between neighbors and
propagated transitively. This makes route poisoning particularly effective.

### Mitigation

**Ignore relayed delivery probability claims:**
```
Rule: A node's delivery probability for destination D is computed ONLY from its own
      contact history with D. It NEVER adopts a neighbor's claimed probability.

// Correct implementation (own history only):
P(self → D) = own_contact_history[D].compute_probability()

// WRONG — vulnerable to route poisoning:
P(self → D) = max(own_history, neighbor.claimed_probability)  // DON'T DO THIS
```

This limits PRoPHET-style transitive probability to verified own contacts. Delivery
probability claims from untrusted nodes are ignored for routing computation.

**Consequence:** IRIS does not implement full PRoPHET (which relies on probability
propagation). Instead, IRIS uses a simplified version based only on own contact history.
This trades some routing efficiency for security. Acceptable tradeoff.

## Attack 5: Sybil-Based Routing Manipulation

A Sybil attacker creates many fake identities, making themselves appear as a large fraction
of the network, attracting most routing traffic. See SYBIL_RESISTANCE.md for full analysis.

Routing-specific impact: Sybil nodes can implement blackhole, selective forwarding, or
route poisoning at network scale.

## Attack 6: TTL Manipulation

### Description

A relay modifies the `ttl_seconds` field in a message to extend (or reduce) its lifetime.
- Extended TTL: message that should have expired is kept alive, wasting storage
- Reduced TTL: message expires before reaching destination

### Mitigation

`ttl_seconds` is part of the signed data (see MESSAGE_AUTHENTICATION.md). Any modification
to TTL invalidates the Ed25519 signature. Modification is detected at next relay.

However: the timestamp from which TTL is computed is also signed. Relay verifies:
```
message_expires_at = message.timestamp + message.ttl_seconds
If now > message_expires_at → DROP (expired)
```

A relay cannot extend TTL without breaking the signature.

## Attack 7: Priority Escalation

### Description

A relay modifies the `priority` field of a message from P4 (text) to P0 (SOS), giving it
undeserved routing priority and resource access.

### Mitigation

`priority` is in the signed data. Modification invalidates signature. Any relay detecting
a priority mismatch between the signed value and the envelope drops the message.

## Future Work: Cryptographic Proof of Forwarding

In current IRIS, there is no way to prove that a relay actually forwarded a message (only
that the recipient received it via ACK). A future enhancement:

```
ForwardingRecord {
  message_id: MessageId,
  relay_id: NodeId,
  timestamp: i64,
  next_hop: NodeId,
  signature: Ed25519Signature  // relay signs forwarding record
}
```

Recipients who receive the message can verify the chain of custody. Relays that drop messages
cannot fabricate forwarding records they did not create. This provides accountability but
requires significant protocol complexity.

## Current Security Limitations

Routing security in a purely opportunistic network (no infrastructure) is fundamentally
limited. Without:
- **Physical authentication**: nodes cannot prove they are who they say
- **Trusted infrastructure**: no authority can revoke a malicious routing node
- **Real-time monitoring**: no global view of routing behavior

The mitigations above reduce risk but cannot eliminate it. For high-security routing,
IRIS recommends using multiple redundant paths (multi-path routing) for critical messages,
and always using application-layer ACK for P0-P2.
