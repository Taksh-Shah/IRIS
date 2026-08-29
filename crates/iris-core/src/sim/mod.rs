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
use std::hash::{Hash, Hasher};
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
use crate::routing::RoutingEngine;
use crate::sim::ml::{FeatureVec, ShadowDecision, ShadowRecorder};

pub mod metrics;
pub mod ml;
pub mod scenario;

/// Virtual epoch for the sim clock (arbitrary stable base so envelopes carry
/// plausible wall-clock timestamps).
const VIRTUAL_EPOCH_SECS: u64 = 1_700_000_000;

/// One simulated node: a peer carrying an SCF buffer plus its anti-loop cache.
pub struct SimNode {
    pub peer_id: PeerId,
    pub scf: ScfEngine<MemoryStorage>,
    pub forwarded: ForwardedCache,
    pub hops_by_msg: HashMap<MessageId, u8>,
    /// Sim-ms this node first buffered each message (SIM-4): the shared
    /// virtual clock (`Simulation::clock`) is whole-seconds because it also
    /// feeds `ScfEngine`/`ForwardedCache`/`DeliveryPredictability`, all of
    /// which are shared with production and treat it as Unix seconds — this
    /// per-node map gives `shadow_features`'s age/TTL-fraction computation
    /// sub-second precision without changing that shared contract.
    pub buffered_at_ms: HashMap<MessageId, u64>,
    pub delivered: Vec<(MessageId, u64)>, // (id, delivery sim-ms)
    pub evictions: u64,
    pub storage_max_bytes: u64,
    /// ROUTE-002 L2 opportunistic router (enabled via `with_opportunistic`).
    pub opp: Option<OpportunisticRouter>,
    /// Binary-spray budget remaining per message (cold-start L2).
    pub spray: HashMap<MessageId, SprayBudget>,
    /// Relays performed for delivery-ratio/overhead accounting.
    pub relays: u64,
    /// SIM-7: production routing engine for this node. Wired with the same
    /// opportunistic config as `opp` so `decide_sim` can be called by tests
    /// and future wiring without duplicating the cascade.
    ///
    /// `forward_to` does NOT yet delegate to `engine.decide_sim` because the
    /// sim's broadcast-to-all-contacts model shares one `engine.forward_cache`
    /// across simultaneous contacts — the dedup gate would block all but the
    /// first contact at a given timestamp. Full wiring requires the sim to
    /// switch to a unicast-per-contact routing model (SIM-7 scope note).
    #[allow(dead_code)]
    pub engine: RoutingEngine,
}

impl std::fmt::Debug for SimNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimNode")
            .field("peer_id", &self.peer_id)
            .field("delivered", &self.delivered)
            .field("evictions", &self.evictions)
            .field("relays", &self.relays)
            .field("engine", &"<RoutingEngine>")
            .finish_non_exhaustive()
    }
}

impl SimNode {
    pub fn new(peer_id: PeerId, clock: Arc<AtomicU64>) -> Self {
        SimNode {
            peer_id,
            scf: ScfEngine::new(MemoryStorage::new(), ScfConfig::default())
                .with_virtual_clock(Arc::clone(&clock)),
            // SIM-10: wire the same virtual clock into ForwardedCache so the
            // sim's anti-loop dedup window expires on virtual time, not wall
            // time.  Without this, a message with TTL > EXACT_WINDOW (1 h)
            // would never re-trigger once seen — the real dedup window lapses
            // but the sim's never does.
            forwarded: ForwardedCache::default().with_virtual_clock(Arc::clone(&clock)),
            hops_by_msg: HashMap::new(),
            buffered_at_ms: HashMap::new(),
            delivered: Vec::new(),
            evictions: 0,
            storage_max_bytes: ScfConfig::default().max_storage_bytes,
            opp: None,
            spray: HashMap::new(),
            relays: 0,
            engine: RoutingEngine::new(),
        }
    }
}

/// A scheduled contact: nodes `a` and `b` are in range at `at_ms` for
/// `duration_ms` milliseconds (SIM-9).
///
/// `duration_ms` makes the contact window representable so the sim can
/// enforce byte budgets (`bandwidth_bps × duration_ms`) and model mid-
/// transfer disconnects (SIM-8 scope). Set to 0 for a legacy instantaneous
/// contact (unbounded transfer, old behaviour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContactEvent {
    pub at_ms: u64,
    pub a: usize,
    pub b: usize,
    /// Duration of the contact window in sim-milliseconds. 0 = unbounded
    /// (instantaneous — legacy; enforcement deferred to SIM-8).
    pub duration_ms: u64,
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

/// Per-link loss and transport fidelity parameters (SIM-8).
///
/// `bandwidth_bps = 0` (default) → unlimited (legacy instantaneous transfer).
/// `mtu_bytes = 0` (default) → no size limit.
/// `latency_base_ms = 0` (default) → delivery at contact time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimLoss {
    /// Symmetric packet-loss rate [0,1]. Applied per-forward via the seeded RNG.
    pub rate: f32,
    /// Bandwidth ceiling in bits per second. Combined with `ContactEvent::duration_ms`
    /// (SIM-9) to compute per-contact byte budgets: bytes = bps * duration_ms / 8_000.
    /// 0 = unlimited (legacy behavior).
    pub bandwidth_bps: u64,
    /// Maximum message payload size in bytes. Messages larger than this are
    /// dropped at the transport layer (not buffered for later). 0 = no limit.
    pub mtu_bytes: u64,
    /// Base one-way latency added to every delivery timestamp (ms). Does not
    /// affect forwarding decisions — only the `delivered` record's timestamp.
    pub latency_base_ms: u64,
}

/// Result of one forward attempt during a contact.
///
/// SIM-11: `Blocked` used to be one variant covering four unrelated causes
/// (anti-loop backtrack, hop budget, link loss, L2 withholding) — the sim
/// computed this distinction and then discarded it at every call site, so a
/// low delivery ratio had no diagnosis beyond an `#[ignore]`d eprintln debug
/// test. Split so `run()` can tally *why* forwards were blocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForwardResult {
    Delivered,
    Relayed,
    BlockedBacktrack,   // no-backtrack: dst is where this copy came from
    BlockedHopBudget,   // hop budget for this priority is exhausted
    BlockedNoAdvantage, // L2 (GTMX+/spray) declined: no DP advantage, no spray budget
    BlockedLoss,        // link loss draw
    /// SIM-8: message payload exceeds the configured `mtu_bytes` ceiling —
    /// the radio cannot carry it in one frame, and the sim does not yet
    /// model fragmentation/reassembly.
    BlockedMtu,
    /// Malformed `recipient_id` (not 32 bytes) — defensive only; every sim
    /// code path builds `recipient_id` from a 32-byte `PeerId`, so this is
    /// unreachable in practice and deliberately not one of the four tallied
    /// counters `run()` exposes on `SimOutcome`.
    BlockedMalformed,
    Sinked, // dst already holds (or already delivered) this id
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
    /// SIM-11: why forwards were blocked — previously computed by
    /// `forward_to` and discarded at every call site, so a low delivery
    /// ratio had no diagnosis beyond an `#[ignore]`d eprintln debug test.
    blocked_backtrack: u64,
    blocked_hop_budget: u64,
    blocked_no_advantage: u64,
    blocked_loss: u64,
    /// SIM-29: relay count *per message* — `relays_total / injected_total`
    /// is a mean, which cannot catch one message exploding past a
    /// per-message bound (e.g. binary spray's L) while others stay quiet.
    relays_by_msg: HashMap<MessageId, u64>,
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
            blocked_backtrack: 0,
            blocked_hop_budget: 0,
            blocked_no_advantage: 0,
            blocked_loss: 0,
            relays_by_msg: HashMap::new(),
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

    /// Enable the ROUTE-002 L2 opportunistic layer on every node (PRoPHET v2
    /// DP + binary spray). Same seed → same L2 decisions (determinism).
    pub fn with_opportunistic(&mut self, config: ProphetConfig) -> &mut Self {
        for n in &mut self.nodes {
            if n.opp.is_none() {
                n.opp =
                    Some(OpportunisticRouter::new(config).with_virtual_clock(self.clock.clone()));
            }
            // SIM-7: also enable the production RoutingEngine so decide_sim
            // exercises Algorithm 2.5 through the same opportunistic router
            // config, keeping the two code paths in sync.
            let engine = std::mem::take(&mut n.engine);
            n.engine = engine.with_opportunistic(config);
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

    /// Configure SIM-8 transport fidelity parameters.
    /// `bandwidth_bps` limits bytes transferred per contact direction
    /// (combined with `ContactEvent::duration_ms` from SIM-9).
    /// `mtu_bytes` rejects oversized messages at the link layer.
    /// `latency_base_ms` adds one-way propagation delay to delivery timestamps.
    pub fn with_transport_model(
        &mut self,
        bandwidth_bps: u64,
        mtu_bytes: u64,
        latency_base_ms: u64,
    ) -> &mut Self {
        self.loss.bandwidth_bps = bandwidth_bps;
        self.loss.mtu_bytes = mtu_bytes;
        self.loss.latency_base_ms = latency_base_ms;
        self
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

    /// Read access to scheduled contacts (ML-001 GT timing evaluation needs
    /// the gateway node's contact times).
    pub fn contacts_public(&self) -> &[ContactEvent] {
        &self.contacts
    }

    /// SIM-4 scope note: `self.clock` truncates to whole seconds here by
    /// design, not oversight — it is the same `Arc<AtomicU64>` handed to
    /// `ScfEngine`/`ForwardedCache`/`DeliveryPredictability` via
    /// `with_virtual_clock`, and all three (plus their production,
    /// non-virtual code paths) treat that atomic as Unix seconds. Giving it
    /// millisecond resolution is a cross-cutting change to those shared,
    /// production routing modules, not a sim-local fix, so it is out of
    /// scope here. `self.now_ms` (below) stays millisecond-precise for the
    /// sim's own bookkeeping (`SimNode::buffered_at_ms`, event ordering).
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

    /// SIM-11: tally why a forward was blocked. `Delivered`/`Relayed` are
    /// already counted via `SimOutcome::delivered_total`/`relays_total`;
    /// `Sinked` (dst already had it) and `BlockedMalformed` (unreachable in
    /// practice — see the variant's doc comment) are not part of the four
    /// counters the finding asks for.
    fn tally(&mut self, r: ForwardResult) {
        match r {
            ForwardResult::BlockedBacktrack => self.blocked_backtrack += 1,
            ForwardResult::BlockedHopBudget => self.blocked_hop_budget += 1,
            ForwardResult::BlockedNoAdvantage => self.blocked_no_advantage += 1,
            ForwardResult::BlockedLoss => self.blocked_loss += 1,
            ForwardResult::BlockedMtu
            | ForwardResult::BlockedMalformed
            | ForwardResult::Delivered
            | ForwardResult::Relayed
            | ForwardResult::Sinked => {}
        }
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
        // SIM-4: sub-second age from the sim's own ms-precision bookkeeping
        // rather than the shared whole-second clock — see `advance_to`.
        // Falls back to the (degenerate) whole-second age if this message
        // somehow reached shadow_features without a buffered_at_ms entry.
        let age_s = match self.nodes[src].buffered_at_ms.get(&id) {
            Some(buffered_ms) => self.now_ms.saturating_sub(*buffered_ms) as f32 / 1000.0,
            None => {
                let now_s = self.clock.load(Ordering::Relaxed);
                now_s.saturating_sub(msg.envelope.timestamp) as f32
            }
        };
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

        // SIM-8: MTU check — reject messages that exceed the radio frame size.
        // The sim does not model fragmentation; a message that cannot fit in
        // one frame is never carried over this link.
        if self.loss.mtu_bytes > 0 {
            let payload_bytes = msg.envelope.payload.len() as u64;
            if payload_bytes > self.loss.mtu_bytes {
                return ForwardResult::BlockedMtu;
            }
        }

        // Terminal copies at src are inert (already delivered elsewhere).
        if msg.delivery_status.is_terminal() {
            return ForwardResult::Sinked;
        }
        // No-backtrack: never hand back to the node it came from.
        if msg.received_from == Some(dst_peer) {
            return ForwardResult::BlockedBacktrack;
        }
        // Anti-loop: dst has already seen/forwarded this id.
        if self.nodes[dst].forwarded.is_duplicate(&id) {
            return ForwardResult::Sinked;
        }
        // dst already carries this message, or has already been delivered
        // it in an earlier round (DTN-5: a delivered message is removed
        // from the SCF buffer immediately, so `delivery_status` alone can
        // no longer tell "never seen" apart from "already delivered and
        // cleaned up" — `delivered` is the sim's own permanent record,
        // unaffected by SCF buffer retention).
        if self.nodes[dst].scf.delivery_status(&id).is_some()
            || self.nodes[dst].delivered.iter().any(|(mid, _)| *mid == id)
        {
            return ForwardResult::Sinked;
        }
        // Hop budget (ROUTE-001 flood policy reused).
        let budget = max_hops_for_priority(msg.envelope.priority).max_hops;
        let hops = self.nodes[src].hops_by_msg.get(&id).copied().unwrap_or(0);
        if hops >= budget {
            return ForwardResult::BlockedHopBudget;
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
                return ForwardResult::BlockedMalformed;
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
                return ForwardResult::BlockedNoAdvantage;
            }
        }

        // Link loss is deterministic per (seed, attempt order).
        if self.is_lost() {
            return ForwardResult::BlockedLoss;
        }

        // Record dedup at dst before the copy lands (anti-loop).
        self.nodes[dst].forwarded.record(id);

        let mut new_env = msg.envelope.clone();
        let new_hops = hops + 1;
        new_env.hop_count = new_hops;
        self.nodes[dst].hops_by_msg.insert(id, new_hops);
        self.nodes[dst].buffered_at_ms.insert(id, self.now_ms);

        // SCF carry: land the copy at dst (idempotent via dedup).
        let src_peer = self.nodes[src].peer_id;
        // SIM-5: see the injection call site's comment — same before/after
        // length diff to attribute `buffer_message`'s internal capacity
        // eviction (DTN-1) to `evictions` without changing its signature.
        let before = self.nodes[dst].scf.len();
        let admitted = self.nodes[dst]
            .scf
            .buffer_message(new_env.clone(), Some(src_peer))
            .is_ok();
        let after = self.nodes[dst].scf.len();
        self.nodes[dst].evictions += (before + admitted as usize).saturating_sub(after) as u64;

        let is_recipient = msg.envelope.recipient_id.as_slice() == dst_peer.as_bytes();
        if is_recipient {
            // Terminal delivery. SIM-8: add one-way latency to the delivery
            // timestamp so latency percentiles reflect propagation time, not
            // just the contact schedule. Default 0 → no change (legacy).
            let delivery_ms = self.now_ms.saturating_add(self.loss.latency_base_ms);
            self.nodes[dst].delivered.push((id, delivery_ms));
            self.nodes[src].scf.mark_forwarded(&id, dst_peer, true);
            self.nodes[dst].scf.mark_forwarded(&id, dst_peer, true);
            // SIM-30: release per-message bookkeeping at the forwarding src and
            // receiving dst on terminal delivery.  Without this, hops_by_msg and
            // spray grow O(nodes × messages) and are never freed — blocking
            // large-scale runs.  Other nodes that still carry a copy keep their
            // entries (they need the hop budget); dedup + delivered-vec guards
            // prevent re-delivery at dst regardless.
            self.nodes[src].hops_by_msg.remove(&id);
            self.nodes[dst].hops_by_msg.remove(&id);
            self.nodes[src].spray.remove(&id);
            self.nodes[dst].spray.remove(&id);
            return ForwardResult::Delivered;
        }

        // Relay: src's copy is AckPending (retryable); the copy rides at dst.
        self.nodes[src].scf.mark_forwarded(&id, dst_peer, false);
        self.nodes[src].relays += 1;
        *self.relays_by_msg.entry(id).or_insert(0) += 1;
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
        // Event timeline: (sim_ms, kind) — 0 = injection, 1 = contact.
        // Injections sort BEFORE contacts at the same timestamp (SIM-12): a
        // message injected at time T is buffered before the T-contacts fire,
        // so it can be carried on the very first contact wave.  The previous
        // convention (0=contact, 1=injection) meant an injection at T always
        // missed the T-contacts and waited a full period — both unintuitive
        // and inconsistent with how scenario authors write tests.
        //
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
                timeline.push((c.at_ms, 1)); // SIM-12: contact sorts after injection
            }
        }
        // Injections keep one marker EACH (the cursor consumes one injection
        // per marker; deduping them would silently drop same-timestamp ones).
        for i in &self.injections {
            timeline.push((i.at_ms, 0)); // SIM-12: injection sorts before contact
        }
        timeline.sort_unstable_by_key(|(t, k)| (*t, *k));

        let mut injected: Vec<InjectedMsg> = Vec::new();
        let mut evictions_total: u64 = 0;
        let mut max_hops_seen: u8 = 0;
        // SIM-6: a running high-water mark, sampled every tick (not just
        // "after the run") — the old computation scanned `out.nodes` once
        // at the very end, which is the *final* usage, not a peak; a
        // partition that buffers everything then heals right before the
        // run ends would report near-empty storage despite a real spike.
        let mut peak_storage_bytes: u64 = 0;

        // Injections are consumed in schedule order (a cursor into a
        // time-sorted copy — a *find* on at_ms would collapse same-timestamp
        // injections onto the first one, silently dropping messages).
        let mut inj_copy = self.injections.clone();
        inj_copy.sort_unstable_by_key(|i| i.at_ms);
        let mut inj_head: &[Injection] = &inj_copy;

        for (sim_ms, kind) in &timeline {
            self.advance_to(*sim_ms);
            // SIM-5: proactively reap TTL-expired messages every tick.
            // Nothing else does this — `buffer_message`'s own TTL check
            // (DTN-6) only rejects an already-expired message at ADMISSION
            // time; it does not remove an existing buffered message that
            // expires while sitting there un-relayed.
            for n in &mut self.nodes {
                let reaped = n.scf.reap_expired();
                n.evictions += reaped.len() as u64;
            }
            match *kind {
                0 => {
                    // Injection (SIM-12: processes before contacts at same ms).
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
                    // SIM-5: `buffer_message` already enforces the byte
                    // ceiling internally (DTN-1's `evict_until` call on
                    // over-admission) — it just doesn't surface what got
                    // evicted. Diff buffer length before/after rather than
                    // changing `buffer_message`'s signature (used by ~30
                    // production/test call sites outside this crate's sim);
                    // `admitted` accounts for the new entry so a capacity
                    // eviction isn't miscounted as net occupancy change.
                    let before = self.nodes[inj.from].scf.len();
                    let admitted = self.nodes[inj.from].scf.buffer_message(env, None).is_ok();
                    let after = self.nodes[inj.from].scf.len();
                    self.nodes[inj.from].evictions +=
                        (before + admitted as usize).saturating_sub(after) as u64;
                    self.nodes[inj.from].hops_by_msg.insert(id, 0);
                    self.nodes[inj.from].buffered_at_ms.insert(id, *sim_ms);
                    injected.push(InjectedMsg {
                        id,
                        priority: inj.priority,
                        at_ms: *sim_ms,
                    });
                }
                1 => {
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
                        // SIM-8: per-contact byte budget from bandwidth × duration
                        // (SIM-9 supplies duration_ms). 0 bps or 0 duration_ms →
                        // unlimited (legacy instantaneous-transfer behaviour).
                        let byte_budget: Option<u64> =
                            if self.loss.bandwidth_bps > 0 && ev.duration_ms > 0 {
                                Some(
                                    self.loss
                                        .bandwidth_bps
                                        .saturating_mul(ev.duration_ms)
                                        / 8_000,
                                )
                            } else {
                                None
                            };
                        let mut a_bytes: u64 = 0;
                        for m in &a_msgs {
                            if let Some(budget) = byte_budget {
                                let sz = m.envelope.payload.len() as u64;
                                if a_bytes.saturating_add(sz) > budget {
                                    break; // window exhausted a→b
                                }
                                a_bytes += sz;
                            }
                            let r = self.forward_to(a, b, m);
                            self.tally(r);
                        }
                        let mut b_bytes: u64 = 0;
                        for m in &b_msgs {
                            if let Some(budget) = byte_budget {
                                let sz = m.envelope.payload.len() as u64;
                                if b_bytes.saturating_add(sz) > budget {
                                    break; // window exhausted b→a
                                }
                                b_bytes += sz;
                            }
                            let r = self.forward_to(b, a, m);
                            self.tally(r);
                        }
                    }
                }
                _ => {}
            }
            // Sampled every tick (not just contact ticks): an injection can
            // itself create a momentary high-water mark before the next
            // contact relays it away, which a contact-only sample would miss.
            for n in &self.nodes {
                max_hops_seen = max_hops_seen.max(*n.hops_by_msg.values().max().unwrap_or(&0));
                peak_storage_bytes = peak_storage_bytes.max(n.scf.usage_bytes());
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

        // SIM-2: `result_anchor` used to be `seed` + two counts, both of
        // which can stay identical across runs that took completely
        // different routes (injected_total is fixed by the scenario;
        // delivered_total saturates at 100% in most scenarios) — making
        // "same seed -> same anchor" near-tautological and unable to catch
        // a routing regression. Fold the full per-delivery trace
        // (message_id, delivering node, at_ms, hops), relays_total and the
        // latency vector into a digest so the anchor actually fingerprints
        // *how* the run happened, not just how many messages arrived.
        let mut delivery_trace: Vec<(MessageId, usize, u64, u8)> = Vec::new();
        for (idx, n) in self.nodes.iter().enumerate() {
            for (id, at) in &n.delivered {
                let hops = n.hops_by_msg.get(id).copied().unwrap_or(0);
                delivery_trace.push((*id, idx, *at, hops));
            }
        }
        // Sorted (not push-order) so the digest depends only on which
        // deliveries happened, not on the iteration order used to collect
        // them here.
        delivery_trace.sort_unstable();
        let mut trace_hasher = std::collections::hash_map::DefaultHasher::new();
        delivery_trace.hash(&mut trace_hasher);
        relays_total.hash(&mut trace_hasher);
        latencies.hash(&mut trace_hasher);
        let trace_digest = trace_hasher.finish();

        SimOutcome {
            injected_total,
            delivered_total,
            delivery_ratio,
            per_priority,
            latencies,
            evictions_total,
            max_hops_seen,
            peak_storage_bytes,
            nodes: self.nodes,
            seed: self.seed,
            relays_total,
            avg_hop_count,
            result_anchor: format!(
                "sim-{}-d{}-i{}-t{:016x}",
                self.seed, delivered_total, injected_total, trace_digest
            ),
            injected_ids: injected.iter().map(|i| i.id).collect(),
            injected_at: injected.iter().map(|i| i.at_ms).collect(),
            shadow_samples: self.shadow.map(|s| s.samples).unwrap_or_default(),
            blocked_backtrack: self.blocked_backtrack,
            blocked_hop_budget: self.blocked_hop_budget,
            blocked_no_advantage: self.blocked_no_advantage,
            blocked_loss: self.blocked_loss,
            relays_by_msg: self.relays_by_msg,
        }
    }

    fn build_envelope(
        &mut self,
        from: usize,
        to: PeerId,
        priority: MessagePriority,
        payload: &[u8],
        ttl_seconds: u64,
    ) -> Envelope {
        // SIM-1: mint the id from the sim's own seeded RNG, not
        // `MessageId::new_v7()` (wall clock + random bits) — the seed must
        // control everything the forward order depends on, and `StoreKey`
        // ties on `message_id` whenever priority/expiry coincide (the
        // common case within one sim run), so an unseeded id silently
        // reintroduces non-determinism the "same seed -> same result
        // anchor" acceptance criterion promises does not exist.
        let id_bytes: [u8; 16] = rand::Rng::gen(&mut self.rng);
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::from_bytes(id_bytes),
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
    /// SIM-6: true running high-water mark across the whole run, sampled
    /// every tick — not the final usage at the moment `run()` returned.
    pub peak_storage_bytes: u64,
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
    /// SIM-11: forward attempts blocked by the no-backtrack rule.
    pub blocked_backtrack: u64,
    /// SIM-11: forward attempts blocked by the priority's hop budget.
    pub blocked_hop_budget: u64,
    /// SIM-11: forward attempts the L2 layer declined (no DP advantage,
    /// no spray budget) — stayed with L0/SCF instead.
    pub blocked_no_advantage: u64,
    /// SIM-11: forward attempts dropped by the simulated link-loss draw.
    pub blocked_loss: u64,
    /// SIM-29: relay count per message — the per-message statistic a bound
    /// like binary spray's L actually constrains; `relays_total /
    /// injected_total` (a mean) cannot detect one message exceeding L
    /// while others stay quiet.
    pub relays_by_msg: HashMap<MessageId, u64>,
}

impl SimOutcome {
    pub fn delivered_ratio_for(&self, prio: MessagePriority) -> f64 {
        self.per_priority
            .get(&prio)
            .map(|(d, i)| if *i == 0 { 0.0 } else { *d as f64 / *i as f64 })
            .unwrap_or(0.0)
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

    #[test]
    fn scaffold_constructs_and_runs() {
        let mut sim = Simulation::new(3, 7);
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: 0,
            b: 1,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 2,
            duration_ms: 1000,
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
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 2,
            duration_ms: 1000,
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
        // is terminal at node 2. `delivered` (the sim's own permanent
        // record) confirms the delivery happened; the SCF buffer entry
        // itself is gone (DTN-5: a delivered message is removed
        // immediately rather than retained forever).
        assert!(out.nodes[2].delivered.len() == 1);
        let st = out.nodes[2]
            .scf
            .delivery_status(&out.nodes[2].delivered[0].0);
        assert_eq!(st, None);
    }

    #[test]
    fn message_ids_are_seeded_not_wall_clock() {
        // SIM-1: two fresh sims with the same seed must mint identical
        // message ids for equivalent injections — proving ids come from
        // the seeded RNG, not `MessageId::new_v7()` (wall clock + random
        // bits, uncontrollable by seed and thus different on every call
        // regardless of seed).
        let ids_for = |seed: u64| -> Vec<MessageId> {
            let mut sim = Simulation::new(2, seed);
            for i in 0..5 {
                sim.inject(Injection::new(
                    i * 100,
                    0,
                    node_id(1),
                    MessagePriority::P4,
                    b"x",
                    3600,
                ));
            }
            sim.run().injected_ids
        };
        let a = ids_for(42);
        let b = ids_for(42);
        assert_eq!(a.len(), 5);
        assert_eq!(a, b, "same seed must mint identical message ids");
        assert_ne!(
            a,
            ids_for(43),
            "different seeds must mint different message ids"
        );
    }

    #[test]
    fn shadow_feature_age_has_subsecond_precision() {
        // SIM-4: 100ms and 900ms both floor-divide to the same whole virtual
        // second (epoch+0s), which is exactly what made the old age_s
        // (derived from the shared whole-second clock) read 0.0 for any
        // pair of events under a second apart — degenerating FeatureVec
        // slot 5 for scenarios like vehicle_relay's +300ms links. With
        // ms-precision bookkeeping the age must reflect the true 0.8s gap.
        let mut sim = Simulation::new(2, 1);
        sim.with_opportunistic(ProphetConfig::default());
        sim.with_shadow(ShadowRecorder::new());
        sim.inject(Injection::new(
            100,
            0,
            node_id(1),
            MessagePriority::P4,
            b"x",
            3600,
        ));
        sim.add_contact(ContactEvent {
            at_ms: 900,
            a: 0,
            b: 1,
            duration_ms: 1000,
        });
        let out = sim.run();
        assert!(
            !out.shadow_samples.is_empty(),
            "contact must produce a shadow decision"
        );
        let age = out.shadow_samples[0].features.as_slice()[5];
        assert!(
            (age - 0.8).abs() < 0.01,
            "age_s should reflect the true 0.8s gap with ms precision, got {age}"
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
                duration_ms: 1000,
            });
            sim.add_contact(ContactEvent {
                at_ms: 1500,
                a: 1,
                b: 2,
                duration_ms: 1000,
            });
            sim.add_contact(ContactEvent {
                at_ms: 2000,
                a: 2,
                b: 3,
                duration_ms: 1000,
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
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 2,
            b: 3,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 4,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 3,
            b: 0,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 4,
            b: 2,
            duration_ms: 1000,
        });
        let out = sim.run();
        assert_eq!(
            out.injected_total, 5,
            "all 5 same-ms injections must be created"
        );
    }

    #[test]
    fn ttl_expiry_is_reaped_and_counted() {
        // SIM-5: nothing in the sim ever called `reap_expired` — a message
        // that outlived its TTL while sitting un-relayed in a buffer just
        // stayed there forever, still occupying space, still forwardable,
        // and `evictions_total` read 0 no matter how many messages expired.
        let mut sim = Simulation::new(2, 1);
        sim.inject(Injection::new(
            0,
            0,
            node_id(1),
            MessagePriority::P4,
            b"x",
            1, // 1-second TTL
        ));
        // No other tick until well past the TTL (virtual clock is seconds:
        // 5000ms = epoch+5s, vs. the message's epoch+0s + 1s expiry).
        sim.add_contact(ContactEvent {
            at_ms: 5000,
            a: 0,
            b: 1,
            duration_ms: 1000,
        });
        let out = sim.run();
        assert_eq!(
            out.delivered_total, 0,
            "expired message must never be delivered"
        );
        assert!(
            out.evictions_total >= 1,
            "TTL expiry must be reaped and counted, got {}",
            out.evictions_total
        );
    }

    #[test]
    fn peak_storage_bytes_captures_transient_high_not_final_state() {
        // SIM-6: the old computation scanned `nodes` once at the very end
        // of `run()` — the *final* usage, which for a message that gets
        // delivered (and removed, DTN-5) before the run ends reads back
        // near zero even though the buffer genuinely held it moments
        // earlier. Distinguishing test: peak must be nonzero even though
        // final usage across every node is exactly zero.
        let mut sim = Simulation::new(2, 1);
        sim.inject(Injection::new(
            0,
            0,
            node_id(1),
            MessagePriority::P4,
            b"hello world",
            3600,
        ));
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: 0,
            b: 1,
            duration_ms: 1000,
        });
        let out = sim.run();
        assert_eq!(out.delivered_total, 1);
        let final_usage: u64 = out.nodes.iter().map(|n| n.scf.usage_bytes()).sum();
        assert_eq!(
            final_usage, 0,
            "delivered message must be removed from both buffers (DTN-5)"
        );
        assert!(
            out.peak_storage_bytes > 0,
            "peak must capture the transient buffered state, got 0"
        );
    }

    #[test]
    fn blocked_reasons_are_tallied_and_exposed() {
        // SIM-11: `forward_to` already distinguished *why* a forward was
        // blocked; every call site discarded it, so a low delivery ratio
        // had no diagnosis beyond an `#[ignore]`d eprintln debug test. A
        // P4 chain one hop longer than its 3-hop budget (routing/flood.rs)
        // must now show up as `blocked_hop_budget`, not just silence.
        let mut sim = Simulation::new(5, 1);
        sim.inject(Injection::new(
            0,
            0,
            node_id(4),
            MessagePriority::P4,
            b"x",
            3600,
        ));
        sim.add_contact(ContactEvent {
            at_ms: 1000,
            a: 0,
            b: 1,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 2000,
            a: 1,
            b: 2,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 3000,
            a: 2,
            b: 3,
            duration_ms: 1000,
        });
        sim.add_contact(ContactEvent {
            at_ms: 4000,
            a: 3,
            b: 4,
            duration_ms: 1000,
        });
        let out = sim.run();
        assert_eq!(
            out.delivered_total, 0,
            "the 4th hop exceeds P4's 3-hop budget"
        );
        assert!(
            out.blocked_hop_budget >= 1,
            "hop-budget exhaustion must be tallied, got {}",
            out.blocked_hop_budget
        );
    }

    #[test]
    fn sim30_hops_by_msg_pruned_on_terminal_delivery() {
        // Regression: hops_by_msg and spray were append-only — every message ever
        // seen kept its entry for the whole run, making memory O(nodes × messages)
        // and blocking large-scale simulations (SIM-30).
        //
        // After the fix, the forwarding src and receiving dst nodes both have
        // their entries removed on terminal delivery.  Two-node direct-delivery
        // scenario: after the run the delivering src must have no hops_by_msg
        // entry for the delivered message.
        let mut sim = Simulation::new(2, 1);
        sim.add_contact(ContactEvent { at_ms: 500, a: 0, b: 1, duration_ms: 1000 });
        let inj = Injection::new(100, 0, node_id(1), MessagePriority::P4, b"hi", 3600);
        sim.inject(inj);
        let out = sim.run();
        assert_eq!(out.delivered_total, 1, "message must be delivered");
        // After delivery, both nodes must have released their hops_by_msg entry.
        let delivered_id = out.nodes[1].delivered[0].0;
        assert!(
            !out.nodes[0].hops_by_msg.contains_key(&delivered_id),
            "src node must release hops_by_msg on terminal delivery (SIM-30)"
        );
        assert!(
            !out.nodes[1].hops_by_msg.contains_key(&delivered_id),
            "dst node must not have a hops_by_msg entry after delivery (SIM-30)"
        );
    }

    #[test]
    fn sim10_forwarded_cache_uses_virtual_clock() {
        // Regression: SimNode::new() left ForwardedCache on wall time while
        // ScfEngine used the virtual clock.  A sim run finishes in microseconds
        // of real time, so the 1-hour EXACT_WINDOW never elapsed and the
        // anti-loop exact set was permanent — loop_free() was guaranteed by
        // construction (SIM-10).
        //
        // Fix: ForwardedCache::with_virtual_clock is now called in SimNode::new.
        // This verifies the wiring: advance virtual time past EXACT_WINDOW and
        // confirm the ring drains on the next record(), proving wall time is
        // NOT what drives expiry.
        use std::sync::atomic::Ordering;
        let clock = Arc::new(AtomicU64::new(0));
        let mut cache = crate::routing::dedup_cache::ForwardedCache::default()
            .with_virtual_clock(Arc::clone(&clock));
        cache.record(crate::protocol::MessageId::new_v7());
        assert_eq!(cache.ring_len(), 1);

        // Advance virtual time past 1-hour EXACT_WINDOW (3600 s).
        clock.store(3601, Ordering::Relaxed);
        // record() triggers evict_old() — the existing entry must be purged.
        cache.record(crate::protocol::MessageId::new_v7());
        assert_eq!(
            cache.ring_len(), 1,
            "the t=0 entry must be evicted by virtual-clock-driven EXACT_WINDOW; \
             only the new entry should remain (SIM-10)"
        );
    }
}
