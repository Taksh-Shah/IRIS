# Opportunistic Routing Algorithms

## Overview

Opportunistic routing addresses the fundamental DTN challenge: deliver messages
when no end-to-end path exists. The approach exploits transient contacts —
brief moments of connectivity between nodes — to progressively move messages
closer to their destination.

The three algorithms implemented in IRIS (Epidemic, Spray-and-Wait, PRoPHET)
represent a spectrum from maximum delivery rate to maximum efficiency. Algorithm
selection depends on message priority, network density, and resource availability.

## Algorithm 1: Epidemic Routing

### Concept

Epidemic routing is maximally aggressive: forward the message to every node
encountered that does not already have a copy. Named after epidemic spread of
disease — each infected node infects new ones on contact.

### Delivery Guarantee

If the network is connected over time (any node can eventually reach any other
node), epidemic routing guarantees eventual delivery. This is the strongest
delivery guarantee in opportunistic routing.

### Algorithm

```rust
pub struct EpidemicRouter {
    pub message_id: MessageId,
    pub holders: HashSet<NodeId>,  // nodes known to have a copy
}

impl EpidemicRouter {
    /// Called when this node encounters a new contact
    pub fn on_contact(&mut self, contact: &NodeId, state: &NetworkState) -> bool {
        if self.holders.contains(contact) {
            return false;  // Already has it, skip
        }

        // Forward to this contact
        self.holders.insert(contact.clone());
        true  // Signal: send copy to contact
    }

    /// Effective number of copies grows exponentially until saturation
    /// k(t) ≈ N × (1 - exp(-β × k₀ × t))
    /// where N = network size, β = contact rate, k₀ = initial copies
}
```

### Overhead Analysis

| Network size | Copies generated | Bandwidth overhead |
|-------------|-----------------|-------------------|
| 10 nodes | Up to 10 copies | 10x single delivery |
| 100 nodes | Up to 100 copies | 100x |
| 1000 nodes | Up to 1000 copies | 1000x |

Epidemic routing is unsustainable for large networks or resource-constrained
links. It is reserved for P0 emergencies where delivery guarantee outweighs cost.

### IRIS Usage

P0 only. Activated automatically for SOS messages. Battery warning displayed
to user: "Emergency mode active — all resources dedicated to SOS relay."

---

## Algorithm 2: Spray-and-Wait

### Concept

Spray-and-Wait (Spyropoulos et al., 2005) limits the epidemic explosion:
- **Spray phase**: distribute exactly L copies of the message to L different nodes
- **Wait phase**: each copy-holder waits for direct delivery opportunity

This bounds the maximum overhead at L copies while maintaining good delivery
rates because L diverse nodes independently seek the destination.

### Binary Spray-and-Wait

```rust
pub struct SprayAndWaitRouter {
    pub message_id: MessageId,
    pub copies_remaining: u32,  // starts at L
}

impl SprayAndWaitRouter {
    /// Binary spray: node with n copies gives floor(n/2) to new contact
    /// Approaches L contacts faster than one-at-a-time
    pub fn on_contact_binary(
        &mut self,
        contact: &NodeId,
    ) -> Option<CopyAllocation> {
        if self.copies_remaining <= 1 {
            // In wait phase — only deliver directly to destination
            return None;
        }

        let give = self.copies_remaining / 2;
        self.copies_remaining -= give;

        Some(CopyAllocation {
            recipient: contact.clone(),
            copies: give,
        })
    }

    /// Simple spray: give exactly 1 copy per contact until exhausted
    pub fn on_contact_simple(&mut self, contact: &NodeId) -> Option<CopyAllocation> {
        if self.copies_remaining <= 1 {
            return None;
        }
        self.copies_remaining -= 1;
        Some(CopyAllocation { recipient: contact.clone(), copies: 1 })
    }
}
```

### L Value Selection

| Priority | L (copies) | Rationale |
|----------|-----------|-----------|
| P0 | Unlimited (epidemic) | No overhead constraint |
| P1 | 5 | High delivery rate, bounded cost |
| P2 | 3 | Balanced delivery vs overhead |
| P3 | 2 | Moderate delivery, low overhead |
| P4+ | 1 (direct only) | No spray, no overhead |

### Delivery Rate vs Overhead

For a 50-node network, contact rate 2/hour, 12-hour TTL:

| L | Delivery Rate | Message Copies Generated |
|---|--------------|------------------------|
| 1 (direct only) | ~30% | 1 |
| 2 | ~55% | 2 |
| 3 | ~72% | 3 |
| 5 | ~87% | 5 |
| ∞ (epidemic) | ~99% | 35–50 |

Source: Spyropoulos et al. simulation results, adapted for IRIS parameters.

---

## Algorithm 3: PRoPHET

### Concept

PRoPHET (Probabilistic Routing Protocol using History of Encounters and Transitivity,
Lindgren et al., 2003/2012) uses contact history to estimate delivery probability:

- P(a,b): probability that node a will deliver a message to node b
- If a and b meet often, P(a,b) is high
- P(a,b) degrades over time without contact
- Transitivity: if a knows b frequently meets c, then a can infer P(a,c)

### Delivery Probability Calculation

```rust
pub struct ProphetRouter {
    /// P(self, dest) for all destinations this node has contact history with
    delivery_predictions: HashMap<NodeId, f32>,
    last_encounter: HashMap<NodeId, SystemTime>,
}

const P_INIT: f32 = 0.75;   // Initial probability on first encounter
const BETA: f32 = 0.25;     // Transitivity scaling factor
const GAMMA: f32 = 0.98;    // Aging factor per interval
const INTERVAL_SECS: u64 = 3600; // 1 hour interval for aging

impl ProphetRouter {
    /// Called when we encounter node `other`
    pub fn update_on_encounter(&mut self, other: &NodeId) {
        let p_old = self.delivery_predictions.get(other).copied().unwrap_or(0.0);
        // Direct encounter formula: P(A,B) = P(A,B)_old + (1 - P(A,B)_old) × P_INIT
        let p_new = p_old + (1.0 - p_old) * P_INIT;
        self.delivery_predictions.insert(other.clone(), p_new);
        self.last_encounter.insert(other.clone(), SystemTime::now());
    }

    /// Aging: reduce all probabilities over time
    pub fn age_predictions(&mut self) {
        let now = SystemTime::now();
        for (node, p) in self.delivery_predictions.iter_mut() {
            let last = self.last_encounter.get(node)
                .copied()
                .unwrap_or(SystemTime::UNIX_EPOCH);
            let intervals_elapsed = now.duration_since(last)
                .unwrap_or_default()
                .as_secs() / INTERVAL_SECS;
            *p *= GAMMA.powi(intervals_elapsed as i32);
        }
    }

    /// Transitivity: when we meet `other`, update our estimates for their contacts
    pub fn update_transitivity(
        &mut self,
        other: &NodeId,
        other_predictions: &HashMap<NodeId, f32>,
    ) {
        let p_self_other = self.delivery_predictions.get(other).copied().unwrap_or(0.0);
        for (dest, p_other_dest) in other_predictions {
            if dest == &self.self_id() { continue; }
            let p_self_dest = self.delivery_predictions.get(dest).copied().unwrap_or(0.0);
            // Transitivity: P(A,C) = P(A,C)_old + (1 - P(A,C)_old) × P(A,B) × P(B,C) × BETA
            let p_new = p_self_dest + (1.0 - p_self_dest) * p_self_other * p_other_dest * BETA;
            self.delivery_predictions.insert(dest.clone(), p_new);
        }
    }

    /// Forwarding decision: forward to next_hop if it has higher P(next_hop, dest)
    pub fn should_forward(
        &self,
        msg: &IrisMessage,
        next_hop: &NodeId,
        next_hop_predictions: &HashMap<NodeId, f32>,
    ) -> bool {
        let p_self = self.delivery_predictions.get(&msg.recipient).copied().unwrap_or(0.0);
        let p_next = next_hop_predictions.get(&msg.recipient).copied().unwrap_or(0.0);
        p_next > p_self
    }
}
```

### PRoPHET Overhead vs Epidemic

PRoPHET requires exchanging prediction tables during each contact:
- Table size: O(N) entries per node
- Exchange: bidirectional
- For 100-node network: ~100 × 4 bytes = 400 bytes per contact
- Acceptable overhead on BLE or Wi-Fi; significant on LoRa

LoRa optimization: exchange compressed prediction table (only top-20 destinations
by P value), further reduced by Bloom filter comparison.

### Delivery Rate Comparison

| Algorithm | Delivery Rate | Bandwidth Overhead | Memory |
|-----------|-------------|-------------------|--------|
| Epidemic | 95–99% | 10–100x | Low |
| Spray-and-Wait (L=5) | 85–90% | 5x | Low |
| PRoPHET | 80–88% | ~2x | Medium (table) |
| Direct Only | 25–40% | 1x | Minimal |

*Simulation: 50 nodes, synthetic mobility, 6-hour TTL, random waypoint.*

---

## Algorithm 4: MaxProp

MaxProp (Burgess et al., 2006) prioritizes which messages to carry and forward:

**Forwarding order**: forward messages in decreasing order of delivery probability
to destination via next hop, considering:
1. P(next_hop, destination) — how likely next hop delivers it
2. Message age — older messages get slight priority boost (fairness)
3. Priority class — P0 always first regardless of probability

```rust
pub fn maxprop_forwarding_order(
    messages: &[IrisMessage],
    next_hop: &NodeId,
    predictions: &HashMap<(NodeId, NodeId), f32>,
) -> Vec<&IrisMessage> {
    let mut ranked: Vec<_> = messages.iter()
        .map(|msg| {
            let p = predictions.get(&(next_hop.clone(), msg.recipient.clone()))
                .copied().unwrap_or(0.0);
            let age_bonus = msg.age_hours().min(24.0) / 24.0 * 0.1;
            let priority_boost = (7 - msg.priority as u8) as f32 * 0.5;
            let score = p + age_bonus + priority_boost;
            (score, msg)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    ranked.into_iter().map(|(_, m)| m).collect()
}
```

MaxProp is used as a message ordering strategy within Spray-and-Wait and PRoPHET,
not as a standalone routing algorithm.

---

## Algorithm Selection by Priority

```rust
pub fn select_routing_algorithm(
    msg: &IrisMessage,
    state: &NetworkState,
) -> Box<dyn RoutingAlgorithm> {
    match msg.priority {
        Priority::P0 => Box::new(EpidemicRouter::new()),

        Priority::P1 => Box::new(SprayAndWaitRouter::new(5, SprayMode::Binary)),

        Priority::P2 => Box::new(SprayAndWaitRouter::new(3, SprayMode::Binary)),

        Priority::P3 => {
            if state.contact_history.has_sufficient_data() {
                Box::new(ProphetRouter::from_history(&state.contact_history))
            } else {
                Box::new(SprayAndWaitRouter::new(2, SprayMode::Simple))
            }
        }

        Priority::P4 => {
            // PRoPHET only, no spray
            Box::new(ProphetRouter::from_history(&state.contact_history))
        }

        Priority::P5 | Priority::P6 | Priority::P7 => {
            // Direct delivery only
            Box::new(DirectOnlyRouter::new())
        }
    }
}
```

## Algorithm Escalation

When a message has been stored for longer than expected without delivery, escalate:

```rust
pub fn check_escalation(msg: &mut IrisMessage, elapsed: Duration) {
    let escalation_threshold = match msg.priority {
        Priority::P3 => Duration::from_secs(3600),   // 1 hour → escalate to Spray
        Priority::P4 => Duration::from_secs(7200),   // 2 hours → escalate to PRoPHET
        Priority::P5 => Duration::from_secs(14400),  // 4 hours → escalate to Spray
        _ => Duration::MAX, // No escalation for P0-P2 (already at max aggression)
    };

    if elapsed > escalation_threshold {
        msg.routing_algorithm = RoutingAlgorithm::SprayAndWait { copies: 2 };
        msg.max_hops += 2;
        log::info!("Message {:?} escalated routing due to age", msg.id);
    }
}
```
