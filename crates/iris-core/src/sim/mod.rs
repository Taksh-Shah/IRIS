//! Discrete-event simulation scaffold — SIM-001 (WP-6).
//!
//! Drives the real ROUTE-001 (hop budgets, `ForwardedCache` anti-loop) and
//! SCF-001 (`ScfEngine`) logic over a *virtual clock* so every run is
//! reproducible (same seed → same result anchor, acceptance criterion).
//!
//! Model: a set of [`SimNode`]s (each with its own SCF buffer), a contact
//! schedule (discrete "in range" events), and message injections. The
//! [`Simulation::run`] advances virtual time, applies contacts so nodes
//! forward buffered messages (store-carry-forward), and reports
//! [`SimOutcome`] metrics: delivery ratio per priority, latency, storage
//! utilization, eviction counts, and no-loop evidence.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use crate::message::{MessagePriority, PeerId};
use crate::message_engine::storage::MemoryStorage;
use crate::protocol::{ContentType, Envelope, MessageId};
use crate::routing::dedup_cache::ForwardedCache;
use crate::routing::flood::max_hops_for_priority;
use crate::routing::opportunistic::{OpportunisticDecision, OpportunisticRouter, SprayBudget};
use crate::routing::prophet::ProphetConfig;
use crate::routing::scf::{ScfConfig, ScfEngine};
use crate::sim::ml::{FeatureVec, ShadowDecision, ShadowRecorder};

pub mod metrics;
pub mod ml;
pub mod scenario;

/// Virtual epoch for the sim clock (arbitrary stable base so envelopes carry
/// plausible wall-clock timestamps).
const VIRTUAL_EPOCH_SECS: u64 = 1_700_000_000;

/// One simulated node: a peer carrying an SCF buffer plus its anti-loop cache.
#[derive(Debug)]
pub struct SimNode {
    pub peer_id: PeerId,
    pub scf: ScfEngine<MemoryStorage>,
    pub forwarded: ForwardedCache,
    pub hops_by_msg: HashMap<MessageId, u8>,
    pub delivered: Vec<(MessageId, u64)>, // (id, delivery sim-ms)
    pub evictions: u64,
    pub storage_max_bytes: u64,
    /// ROUTE-002 L2 opportunistic router (enabled via `with_opportunistic`).
    pub opp: Option<OpportunisticRouter>,
    /// Binary-spray budget remaining per message (cold-start L2).
    pub spray: HashMap<MessageId, SprayBudget>,
    /// Relays performed for delivery-ratio/overhead accounting.
    pub relays: u64,
}

impl SimNode {
    pub fn new(peer_id: PeerId, clock: Arc<AtomicU64>) -> Self {
        SimNode {
            peer_id,
            scf: ScfEngine::new(MemoryStorage::new(), ScfConfig::default())
                .with_virtual_clock(clock),
            forwarded: ForwardedCache::default(),
            hops_by_msg: HashMap::new(),
            delivered: Vec::new(),
            evictions: 0,
            storage_max_bytes: ScfConfig::default().max_storage_bytes,
            opp: None,
            spray: HashMap::new(),
            relays: 0,
        }
    }
}

/// A scheduled contact: nodes `a` and `b` are in range at `at_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactEvent {
    pub at_ms: u64,
    pub a: usize,
    pub b: usize,
}

/// A message injection: `from` sends to `to` at `at_ms`.
#[derive(Debug, Clone)]
pub struct Injection {
    pub at_ms: u64,
    pub from: usize,
    pub to: PeerId,
    pub priority: MessagePriority,
    pub payload: Vec<u8>,
    pub ttl_seconds: u64,
}

impl Injection {
    pub fn new(
        at_ms: u64,
        from: usize,
        to: PeerId,
        priority: MessagePriority,
        payload: &[u8],
        ttl_seconds: u64,
    ) -> Self {
        Injection {
            at_ms,
            from,
            to,
            priority,
            payload: payload.to_vec(),
            ttl_seconds,
        }
    }
}

/// Per-link loss applied during a contact forward (deterministic via RNG).
#[derive(Debug, Clone, Copy, Default)]
pub struct SimLoss {
    pub rate: f32,
}

/// Result of one forward attempt during a contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardResult {
    Delivered,
    Relayed,
    Blocked, // anti-loop, budget, loss, or no contact target
    Sinked,  // dst already holds the message
}

/// A discrete-event simulation over the virtual clock.
pub struct Simulation {
    nodes: Vec<SimNode>,
    contacts: Vec<ContactEvent>,
    injections: Vec<Injection>,
    loss: SimLoss,
    seed: u64,
    rng: ChaCha8Rng,
    clock: Arc<AtomicU64>,
    now_ms: u64,
    /// ROUTE-002 L2 opportunistic routing enabled on every node.
    opportunistic_enabled: bool,
    /// ML-001 L3 shadow observer (default None → zero cost, runs byte-identical
    /// to ROUTE-002; see ML-001 AC-2).
    shadow: Option<ShadowRecorder>,
}

impl Simulation {
    /// New simulation with `count` nodes and the given RNG seed. Same seed →
    /// same forward/loss decisions (acceptance: reproducible result anchor).
    pub fn new(count: usize, seed: u64) -> Self {
        let clock = Arc::new(AtomicU64::new(VIRTUAL_EPOCH_SECS));
        let mut nodes = Vec::with_capacity(count);
        for i in 0..count {
            nodes.push(SimNode::new(node_id(i), clock.clone()));
        }
        Simulation {
            nodes,
            contacts: Vec::new(),
            injections: Vec::new(),
            loss: SimLoss::default(),
            seed,
            rng: ChaCha8Rng::seed_from_u64(seed),
            clock,
            now_ms: 0,
            opportunistic_enabled: false,
            shadow: None,
        }
    }

    /// Attach the ML-001 L3 shadow observer. Default off: `None` makes runs
    /// byte-identical to ROUTE-002 (ML-001 AC-2). When attached, every L2
    /// decision point records a [`ShadowDecision`] — the L3 predictor's score
    /// is computed but never affects forwarding.
    pub fn with_shadow(&mut self, recorder: ShadowRecorder) -> &mut Self {
        self.shadow = Some(recorder);
        self
    }

    pub fn shadow_enabled(&self) -> bool {
        self.shadow.is_some()
    }

    /// Enable the ROUTE-002 L2 opportunistic layer on every node (PRoPHET v2
    /// DP + binary spray). Same seed → same L2 decisions (determinism).
    pub fn with_opportunistic(&mut self, config: ProphetConfig) -> &mut Self {
        for n in &mut self.nodes {
            if n.opp.is_none() {
                n.opp =
                    Some(OpportunisticRouter::new(config).with_virtual_clock(self.clock.clone()));
            }
        }
        self.opportunistic_enabled = true;
        self
    }

    pub fn opportunistic_enabled(&self) -> bool {
        self.opportunistic_enabled
    }

    pub fn set_loss(&mut self, loss: SimLoss) -> &mut Self {
        self.loss = loss;
        self
    }

    /// Loss reference for a forward (kept close to SimConfig semantics).
    pub fn loss_rate(&self) -> f32 {
        self.loss.rate
    }

    pub fn nodes(&self) -> &[SimNode] {
        &self.nodes
    }

    pub fn node(&self, i: usize) -> &SimNode {
        &self.nodes[i]
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn add_contact(&mut self, event: ContactEvent) -> &mut Self {
        self.contacts.push(event);
        self
    }

    pub fn inject(&mut self, injection: Injection) -> &mut Self {
        self.injections.push(injection);
        self
    }

    /// Read access to scheduled injections (debug/location tracing).
    pub fn injections_public(&self) -> &[Injection] {
        &self.injections
    }

    /// Read access to scheduled contacts (ML-001 GT timing evaluation needs
    /// the gateway node's contact times).
    pub fn contacts_public(&self) -> &[ContactEvent] {
        &self.contacts
    }

    fn advance_to(&mut self, sim_ms: u64) {
        self.now_ms = sim_ms;
        self.clock
            .store(VIRTUAL_EPOCH_SECS + sim_ms / 1000, Ordering::Relaxed);
    }

    fn is_lost(&mut self) -> bool {
        if self.loss.rate <= 0.0 {
            return false;
        }
        let r: f32 = rand::Rng::gen_range(&mut self.rng, 0.0..1.0);
        r < self.loss.rate
    }

    /// ML-001 L3 shadow feature vector for one candidate relay decision:
    /// src (self) handing `msg` (for `recipient`) to dst (neighbor `b`).
    /// Slots (see `FeatureVec` docs): 0=P(b,r), 1=P(self,r), 2=P(b,self),
    /// 3=hop count, 4=dst buffer occupancy, 5=message age (s), 6=TTL fraction
    /// remaining, 7=priority rank. Pure function of node state → deterministic
    /// (ML-001 AC-1).
    fn shadow_features(
        &self,
        src: usize,
        dst: usize,
        id: MessageId,
        msg: &crate::routing::scf::StoredMessage,
        recipient: &PeerId,
    ) -> FeatureVec {
        let src_peer = self.nodes[src].peer_id;
        let src_opp = self.nodes[src].opp.as_ref();
        let dst_opp = self.nodes[dst].opp.as_ref();
        // DP slots are clamped to [0,1] at the feature boundary (ML-RT-02):
        // p_for returns stored PRoPHET DPs, which are bounded under the default
        // config but must not be trusted un-clamped into the predictor.
        let p_b_r = dst_opp.map(|o| o.p_for(recipient)).unwrap_or(0.0) as f32;
        let p_self_r = src_opp.map(|o| o.p_for(recipient)).unwrap_or(0.0) as f32;
        let p_b_self = dst_opp.map(|o| o.p_for(&src_peer)).unwrap_or(0.0) as f32;
        let p_b_r = p_b_r.clamp(0.0, 1.0);
        let p_self_r = p_self_r.clamp(0.0, 1.0);
        let p_b_self = p_b_self.clamp(0.0, 1.0);
        let hops = self.nodes[src].hops_by_msg.get(&id).copied().unwrap_or(0) as f32;
        let cap = self.nodes[dst].storage_max_bytes.max(1) as f32;
        let buf_occ = (self.nodes[dst].scf.usage_bytes() as f32 / cap).clamp(0.0, 1.0);
        let now_s = self.clock.load(Ordering::Relaxed);
        let age_s = now_s.saturating_sub(msg.envelope.timestamp) as f32;
        let ttl = msg.envelope.ttl_seconds.max(1) as f32;
        let ttl_frac = (1.0 - age_s / ttl).clamp(0.0, 1.0);
        let prio_rank = msg.envelope.priority.as_u8() as f32;
        FeatureVec::new([
            p_b_r, p_self_r, p_b_self, hops, buf_occ, age_s, ttl_frac, prio_rank,
        ])
    }

    /// Try to forward one buffered message from `src` to `dst` during a
    /// contact. Mutates both nodes' SCF state and the sim bookkeeping.
    fn forward_to(
        &mut self,
        src: usize,
        dst: usize,
        msg: &crate::routing::scf::StoredMessage,
    ) -> ForwardResult {
        let dst_peer = self.nodes[dst].peer_id;
        let id = msg.envelope.message_id;

        // Terminal copies at src are inert (already delivered elsewhere).
        if msg.delivery_status.is_terminal() {
            return ForwardResult::Sinked;
        }
        // No-backtrack: never hand back to the node it came from.
        if msg.received_from == Some(dst_peer) {
            return ForwardResult::Blocked;
        }
        // Anti-loop: dst has already seen/forwarded this id.
        if self.nodes[dst].forwarded.is_duplicate(&id) {
            return ForwardResult::Sinked;
        }
        // dst already carries this message (dedup at the buffer).
        if self.nodes[dst].scf.delivery_status(&id).is_some() {
            return ForwardResult::Sinked;
        }
        // Hop budget (ROUTE-001 flood policy reused).
        let budget = max_hops_for_priority(msg.envelope.priority).max_hops;
        let hops = self.nodes[src].hops_by_msg.get(&id).copied().unwrap_or(0);
        if hops >= budget {
            return ForwardResult::Blocked;
        }

        // ROUTE-002 L2: exchange DP snapshots on contact, then consult GTMX+
        // / spray before deciding to relay. When L2 is off, behavior is pure
        // SCF/ROUTE-001 (unchanged).
        if self.opportunistic_enabled {
            let recipient = msg.envelope.recipient_id.as_slice();
            let mut rb = [0u8; 32];
            if recipient.len() == 32 {
                rb.copy_from_slice(recipient);
            } else {
                return ForwardResult::Blocked;
            }
            let recipient_peer = PeerId(rb);

            // ML-001: L3 shadow feature vector (in-band state, deterministic).
            let features = if self.shadow.is_some() {
                Some(self.shadow_features(src, dst, id, msg, &recipient_peer))
            } else {
                None
            };

            // Exchange + decide with both nodes borrowed disjointly.
            let decide_forward =
                self.opportunistic_forward(src, dst, id, msg, budget, recipient_peer);

            // Record the shadow decision (score never affects forwarding).
            if let (Some(rec), Some(fx)) = (self.shadow.as_mut(), features) {
                rec.record(ShadowDecision {
                    msg_id: id,
                    at_ms: self.now_ms,
                    src,
                    dst,
                    features: fx,
                    l2_decided: decide_forward,
                });
            }

            if !decide_forward {
                // No DP advantage, no spray budget → stay with L0/SCF.
                return ForwardResult::Blocked;
            }
        }

        // Link loss is deterministic per (seed, attempt order).
        if self.is_lost() {
            return ForwardResult::Blocked;
        }

        // Record dedup at dst before the copy lands (anti-loop).
        self.nodes[dst].forwarded.record(id);

        let mut new_env = msg.envelope.clone();
        let new_hops = hops + 1;
        new_env.hop_count = new_hops;
        self.nodes[dst].hops_by_msg.insert(id, new_hops);

        // SCF carry: land the copy at dst (idempotent via dedup).
        let src_peer = self.nodes[src].peer_id;
        self.nodes[dst]
            .scf
            .buffer_message(new_env.clone(), Some(src_peer))
            .ok();

        let is_recipient = msg.envelope.recipient_id.as_slice() == dst_peer.as_bytes();
        if is_recipient {
            // Terminal delivery.
            self.nodes[dst].delivered.push((id, self.now_ms));
            self.nodes[src].scf.mark_forwarded(&id, dst_peer, true);
            self.nodes[dst].scf.mark_forwarded(&id, dst_peer, true);
            return ForwardResult::Delivered;
        }

        // Relay: src's copy is AckPending (retryable); the copy rides at dst.
        self.nodes[src].scf.mark_forwarded(&id, dst_peer, false);
        self.nodes[src].relays += 1;
        ForwardResult::Relayed
    }

    /// Single-relay availability helper is not needed — contacts always exchange
    /// DP snapshots. This records the encounter so DPs age accordingly.
    fn exchange_on_contact(&mut self, a: usize, b: usize) {
        use std::cmp::Ordering;
        if a == b {
            return;
        }
        let (x, y) = match a.cmp(&b) {
            Ordering::Less => {
                let (lo, hi) = self.nodes.split_at_mut(b);
                (&mut lo[a], &mut hi[0])
            }
            Ordering::Greater => {
                let (lo, hi) = self.nodes.split_at_mut(a);
                (&mut lo[b], &mut hi[0])
            }
            Ordering::Equal => return,
        };
        if let (Some(ao), Some(bo)) = (x.opp.as_mut(), y.opp.as_mut()) {
            let a_snap = ao.predictions().snapshot();
            ao.on_contact(&y.peer_id, &bo.predictions().snapshot());
            bo.on_contact(&x.peer_id, &a_snap);
        }
    }

    /// ROUTE-002 L2 exchange + decision for a `src → dst` contact. Returns
    /// `true` when the message should be relayed.
    ///
    /// Cold-start rule (AC 5 — no regression): when *neither* node has learned
    /// DPs for the recipient yet, the L2 layer falls through to the L0/SCF
    /// relay (do what L0 would have done). Only when DPs are *known* does L2
    /// enforce GTMX+ selectivity: relay to the best-informed carrier, withhold
    /// from carriers worse than the best seen — this is where overhead drops.
    fn opportunistic_forward(
        &mut self,
        src: usize,
        dst: usize,
        id: MessageId,
        msg: &crate::routing::scf::StoredMessage,
        budget: u8,
        recipient: PeerId,
    ) -> bool {
        use std::cmp::Ordering;
        // Disjoint mutable borrows (src != dst by construction). `a` MUST be
        // the forwarding (src) node and `b` the receiving (dst) node for the
        // GTMX+ decision below — for src > dst the Greater branch must return
        // (src, dst) as (a, b), not (dst, src), or the decide runs from the
        // receiver's perspective and every ferry handoff is withheld.
        let (a, b) = match src.cmp(&dst) {
            Ordering::Less => {
                let (lo, hi) = self.nodes.split_at_mut(dst);
                (&mut lo[src], &mut hi[0])
            }
            Ordering::Greater => {
                let (lo, hi) = self.nodes.split_at_mut(src);
                (&mut hi[0], &mut lo[dst])
            }
            Ordering::Equal => return false, // self-contact: nothing to learn
        };
        if a.opp.is_none() || b.opp.is_none() {
            return true; // L2 not armed on both ends → L0 relay
        }
        // Direct delivery: the contact IS the recipient — never withhold
        // (recipient DP for itself is not a table predicate, so GTMX+ must
        // not gate the terminal handoff).
        if b.peer_id.as_bytes() == recipient.as_bytes() {
            return true;
        }
        let src_opp = a.opp.as_mut().unwrap();
        let dst_opp = b.opp.as_mut().unwrap();

        // DPs were already exchanged by exchange_on_contact() at the start of
        // this contact (run() does it once per contact event, independent of
        // message flow). Do NOT re-meet here: per-message re-meeting within a
        // single contact event applies Eq. 1 repeatedly at the same virtual
        // time, which inflates DPs toward (1−delta) and makes GTMX+ withhold
        // from carriers that could actually deliver (ferry hoarding).

        let thr = src_opp.predictions().config().p_first_threshold;
        let my_dp = src_opp.predictions().p_for(&recipient);
        let their_dp = dst_opp.predictions().p_for(&recipient);
        let informed = my_dp > thr || their_dp > thr;
        if informed {
            // Informed: forward toward the carrier with a strictly better DP
            // (GTMX+), withhold from worse carriers than we've seen for this
            // msg — this is where overhead drops vs epidemic.
            let decision = src_opp.decide(
                &id,
                &recipient,
                &[(b.peer_id, their_dp)],
                msg.envelope.priority,
                budget,
            );
            if matches!(decision, OpportunisticDecision::ForwardTo { .. }) {
                return true;
            }
            return false;
        }

        // Cold start: neither node has learned DPs for the recipient. Do NOT
        // blind-relay (that is epidemic — unbounded overhead). Instead use
        // binary spray-and-wait: hand off a bounded copy, then wait-phase
        // direct delivery only (ROUTE2_DESIGN.md §Spray, AC-4). The receiving
        // copy CARRIES the halved `give` budget (binary spray invariant: total
        // copies in the network never exceed the original L).
        let l = SprayBudget::l_for_priority(msg.envelope.priority, budget);
        let mut b_state = a.spray.get(&id).copied().unwrap_or(SprayBudget::new(l));
        let (give, keep) = match b_state.handoff() {
            Some(give) => (give, b_state.remaining),
            None => return false, // wait phase: direct delivery only
        };
        a.spray.insert(id, SprayBudget::new(keep));
        // The receiver carries the `give` budget (halved) — no re-spray of L.
        b.spray.insert(id, SprayBudget::new(give));
        true
    }

    /// Run all contacts and injections on the timeline, returning the outcome.
    pub fn run(mut self) -> SimOutcome {
        // Event timeline: (sim_ms, kind) — 0 = contact, 1 = injection.
        // Contacts are DEDUPLICATED by timestamp: the contact branch below
        // processes every contact at `sim_ms` in one pass, so one marker per
        // timestamp (not per contact). A per-contact marker would re-run all
        // contacts at that timestamp once per contact, over-meeting the PRoPHET
        // tables (each redundant meet applies Eq. 1 at the same virtual
        // instant, climbing DPs toward (1−delta) and breaking GTMX+).
        let mut timeline: Vec<(u64, u8)> = Vec::new();
        let mut contact_ts: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for c in &self.contacts {
            if contact_ts.insert(c.at_ms) {
                timeline.push((c.at_ms, 0));
            }
        }
        // Injections keep one marker EACH (the cursor consumes one injection
        // per marker; deduping them would silently drop same-timestamp ones).
        for i in &self.injections {
            timeline.push((i.at_ms, 1));
        }
        timeline.sort_unstable_by_key(|(t, k)| (*t, *k));

        let mut injected: Vec<InjectedMsg> = Vec::new();
        let mut evictions_total: u64 = 0;
        let mut max_hops_seen: u8 = 0;

        // Injections are consumed in schedule order (a cursor into a
        // time-sorted copy — a *find* on at_ms would collapse same-timestamp
        // injections onto the first one, silently dropping messages).
        let mut inj_copy = self.injections.clone();
        inj_copy.sort_unstable_by_key(|i| i.at_ms);
        let mut inj_head: &[Injection] = &inj_copy;

        for (sim_ms, kind) in &timeline {
            self.advance_to(*sim_ms);
            match *kind {
                1 => {
                    // Injection.
                    let (inj, rest) = inj_head
                        .split_first()
                        .expect("timeline injection marker implies a pending injection");
                    inj_head = rest;
                    let inj = inj.clone();
                    let mut env = self.build_envelope(
                        inj.from,
                        inj.to,
                        inj.priority,
                        &inj.payload,
                        inj.ttl_seconds,
                    );
                    env.timestamp = self.clock.load(Ordering::Relaxed);
                    let id = env.message_id;
                    self.nodes[inj.from].scf.buffer_message(env, None).ok();
                    self.nodes[inj.from].hops_by_msg.insert(id, 0);
                    injected.push(InjectedMsg {
                        id,
                        priority: inj.priority,
                        at_ms: *sim_ms,
                    });
                }
                0 => {
                    // Contact: forward each direction (collect first to avoid
                    // borrowing self while mutating nodes).
                    let evs: Vec<ContactEvent> = self
                        .contacts
                        .iter()
                        .filter(|c| c.at_ms == *sim_ms)
                        .copied()
                        .collect();
                    for ev in evs {
                        let (a, b) = (ev.a, ev.b);
                        // ROUTE-002: exchange DP predictions on EVERY contact
                        // (PRoPHET §5.3.3), independent of message flow — this
                        // is how the ferry learns foreign-community DPs and
                        // transitivity builds.
                        if self.opportunistic_enabled {
                            self.exchange_on_contact(a, b);
                        }
                        let a_msgs: Vec<_> = self.nodes[a].scf.ordered_buffered();
                        let b_msgs: Vec<_> = self.nodes[b].scf.ordered_buffered();
                        for m in &a_msgs {
                            self.forward_to(a, b, m);
                        }
                        for m in &b_msgs {
                            self.forward_to(b, a, m);
                        }
                    }
                    for n in &self.nodes {
                        max_hops_seen =
                            max_hops_seen.max(*n.hops_by_msg.values().max().unwrap_or(&0));
                    }
                }
                _ => {}
            }
        }

        for node in &self.nodes {
            evictions_total += node.evictions;
        }

        let delivered_total = self.nodes.iter().map(|n| n.delivered.len()).sum::<usize>();
        let injected_total = injected.len();

        // ROUTE-002 acceptance metrics (ONE methodology parity, RES-0011 §b8):
        // overhead ratio = relay transmissions / delivered; avg hop count over
        // delivered+hops-tracked messages.
        let relays_total: u64 = self.nodes.iter().map(|n| n.relays).sum();
        let mut hop_sums: Vec<u8> = Vec::new();
        for n in &self.nodes {
            hop_sums.extend(n.hops_by_msg.values().copied());
        }
        let avg_hop_count = if hop_sums.is_empty() {
            0.0
        } else {
            hop_sums.iter().map(|h| *h as f64).sum::<f64>() / hop_sums.len() as f64
        };

        let mut per_priority: HashMap<MessagePriority, (usize, usize)> = HashMap::new();
        let mut latencies: Vec<u64> = Vec::new();
        for inj in &injected {
            let delivered_at = self
                .nodes
                .iter()
                .flat_map(|n| &n.delivered)
                .find(|(id, _)| id == &inj.id)
                .map(|(_, at)| *at);
            let d = delivered_at.is_some() as usize;
            let e = per_priority.entry(inj.priority).or_insert((0, 0));
            e.0 += d;
            e.1 += 1;
            if let Some(at) = delivered_at {
                latencies.push(at.saturating_sub(inj.at_ms));
            }
        }
        let delivery_ratio = if injected_total == 0 {
            0.0
        } else {
            delivered_total as f64 / injected_total as f64
        };

        SimOutcome {
            injected_total,
            delivered_total,
            delivery_ratio,
            per_priority,
            latencies,
            evictions_total,
            max_hops_seen,
            nodes: self.nodes,
            seed: self.seed,
            relays_total,
            avg_hop_count,
            result_anchor: format!("sim-{}-d{}-i{}", self.seed, delivered_total, injected_total),
            injected_ids: injected.iter().map(|i| i.id).collect(),
            injected_at: injected.iter().map(|i| i.at_ms).collect(),
            shadow_samples: self.shadow.map(|s| s.samples).unwrap_or_default(),
        }
    }

    fn build_envelope(
        &self,
        from: usize,
        to: PeerId,
        priority: MessagePriority,
        payload: &[u8],
        ttl_seconds: u64,
    ) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: self.nodes[from].peer_id.0.to_vec(),
            recipient_id: to.0.to_vec(),
            priority,
            ttl_seconds,
            timestamp: self.clock.load(Ordering::Relaxed),
            hop_count: 0,
            max_hops: Some(max_hops_for_priority(priority).max_hops),
            payload_type: ContentType::Text,
            payload_size: payload.len() as u64,
            payload_hash: Envelope::compute_payload_hash(payload),
            payload: payload.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }
}

struct InjectedMsg {
    id: MessageId,
    priority: MessagePriority,
    at_ms: u64,
}
/// Aggregated simulation results.
#[derive(Debug)]
pub struct SimOutcome {
    pub injected_total: usize,
    pub delivered_total: usize,
    pub delivery_ratio: f64,
    pub per_priority: HashMap<MessagePriority, (usize, usize)>,
    pub latencies: Vec<u64>,
    pub evictions_total: u64,
    pub max_hops_seen: u8,
    pub nodes: Vec<SimNode>,
    pub seed: u64,
    /// ROUTE-002: total relay transmissions (overhead baseline).
    pub relays_total: u64,
    /// ROUTE-002: average hop count across tracked messages.
    pub avg_hop_count: f64,
    pub result_anchor: String,
    /// Message ids actually created (post injection-cursor fix).
    pub injected_ids: Vec<MessageId>,
    pub injected_at: Vec<u64>,
    /// ML-001 L3 shadow decisions recorded during the run (empty when no
    /// shadow observer attached).
    pub shadow_samples: Vec<ShadowDecision>,
}

impl SimOutcome {
    pub fn delivered_ratio_for(&self, prio: MessagePriority) -> f64 {
        self.per_priority
            .get(&prio)
            .map(|(d, i)| if *i == 0 { 0.0 } else { *d as f64 / *i as f64 })
            .unwrap_or(0.0)
    }

    pub fn avg_latency_ms(&self) -> f64 {
        if self.latencies.is_empty() {
            0.0
        } else {
            self.latencies.iter().sum::<u64>() as f64 / self.latencies.len() as f64
        }
    }

    /// No-loop evidence: every delivered message is delivered exactly once
    /// (no duplicate terminal delivery across the network).
    pub fn loop_free(&self) -> bool {
        let mut ids: Vec<&MessageId> = Vec::new();
        for n in &self.nodes {
            ids.extend(n.delivered.iter().map(|(id, _)| id));
        }
        let total = ids.len();
        ids.sort();
        ids.dedup();
        ids.len() == total
    }
}

fn node_id(i: usize) -> PeerId {
    let mut b = [0u8; 32];
    b[0] = 0x10;
    b[1] = (i & 0xff) as u8;
    PeerId::from_bytes(b)
}

/// Stable peer id for a sim node index (shared with `scenario` builders).
pub fn sim_peer(i: usize) -> PeerId {
    node_id(i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::scf::DeliveryStatus;

    #[test]
    fn scaffold_constructs_and_runs() {
        let mut sim = Simulation::new(3, 7);
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: 0,
            b: 1,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 2,
        });
        sim.inject(Injection::new(
            500,
            0,
            node_id(2),
            MessagePriority::P4,
            b"hello",
            3600,
        ));
        let out = sim.run();
        assert_eq!(out.injected_total, 1);
        assert_eq!(out.delivered_total, 1, "carry relay must deliver");
        assert!(out.loop_free());
        assert!(out.result_anchor.starts_with("sim-7-"));
    }

    #[test]
    fn relay_marks_relay_node_carry_state() {
        let mut sim = Simulation::new(3, 1);
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: 0,
            b: 1,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 2,
        });
        sim.inject(Injection::new(
            500,
            0,
            node_id(2),
            MessagePriority::P4,
            b"relay",
            3600,
        ));
        let out = sim.run();
        // The relay node (1) no longer holds the delivered copy; the message
        // is terminal at node 2.
        assert!(out.nodes[2].delivered.len() == 1);
        let st = out.nodes[2]
            .scf
            .delivery_status(&out.nodes[2].delivered[0].0);
        assert_eq!(
            st,
            Some(DeliveryStatus::Delivered {
                delivered_to: out.nodes[2].peer_id
            })
        );
    }

    #[test]
    fn same_seed_same_result_anchor() {
        let run = |seed: u64| {
            let mut sim = Simulation::new(4, seed);
            sim.add_contact(ContactEvent {
                at_ms: 1000,
                a: 0,
                b: 1,
            });
            sim.add_contact(ContactEvent {
                at_ms: 1500,
                a: 1,
                b: 2,
            });
            sim.add_contact(ContactEvent {
                at_ms: 2000,
                a: 2,
                b: 3,
            });
            sim.set_loss(SimLoss { rate: 0.3 });
            for i in 0..10 {
                sim.inject(Injection::new(
                    200 + i as u64 * 100,
                    i as usize % 4,
                    node_id((i as usize + 1) % 4),
                    MessagePriority::P4,
                    &[i as u8],
                    3600,
                ));
            }
            sim.run().result_anchor
        };
        assert_eq!(run(99), run(99));
    }

    #[test]
    fn same_timestamp_injections_all_processed() {
        // Regression: run() previously used find(at_ms) for injections, which
        // collapsed same-timestamp injections onto the first one. All
        // scheduled injections must be created (SIM-001 acceptance).
        let mut sim = Simulation::new(5, 1);
        // All 5 nodes inject to a rotating peer at the SAME sim-ms.
        for i in 0..5 {
            sim.inject(Injection::new(
                1000,
                i,
                node_id((i + 1) % 5),
                MessagePriority::P4,
                &[i as u8],
                3600,
            ));
        }
        // Dense contacts so everything can be delivered.
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 0,
            b: 1,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 2,
            b: 3,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 4,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 3,
            b: 0,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 4,
            b: 2,
        });
        let out = sim.run();
        assert_eq!(
            out.injected_total, 5,
            "all 5 same-ms injections must be created"
        );
    }
}
