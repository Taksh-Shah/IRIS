//! Opportunistic routing layer — ROUTE-002 (WP-8).
//!
//! Combines the RFC 6693 PRoPHET delivery-predictability table
//! ([`DeliveryPredictability`]) with:
//!
//! - **GTMX+ forwarding** (RFC §3.6, RES-0011 §b2): forward to neighbor `B`
//!   for destination `D` only when `P(B,D) > P(A,D)` (strict) and above the
//!   `p_first_threshold`; per-message `max_dp_seen` monotonicity prevents
//!   oscillation independent of the L0 dedup cache.
//! - **Binary spray-and-wait** cold start (RES-0011 §b3): [`SprayBudget`]
//!   bounded copies (L=8) then wait-phase direct delivery.
//!
//! The layer is advisory: [`OpportunisticDecision::NoAdvantage`] falls
//! through to the L0 engine unchanged (Direct→KnownPath→Flood→Store).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use crate::message::{MessagePriority, PeerId};
use crate::protocol::MessageId;
use crate::routing::prophet::{DeliveryPredictability, ProphetConfig};

/// Default binary spray budget (≈10–15% of M=50–100, Spyropoulos ToN 2008;
/// EXP-ROUTE-002 validated).
pub const DEFAULT_SPRAY_L: u32 = 8;

/// Maximum number of (message_id, destination) entries in `max_dp_seen`
/// (ROUT-14 capacity bound). Matches the PRoPHET table limit.
pub const MAX_DP_SEEN: usize = 4_096;

/// DPs are advice only — a neighbor must clear this floor to be consulted.
pub const DP_FLOOR_EPS: f64 = 1e-6;

/// Why an opportunistic forward is recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpportunisticReason {
    /// Neighbor's DP for the destination exceeds ours (GTMX+).
    Prophet,
    /// Cold start: handing a spray copy to a fresh node (binary spray).
    Spray,
}

/// What the opportunistic layer recommends for one message at a contact.
#[derive(Debug, Clone, PartialEq)]
pub enum OpportunisticDecision {
    /// Forward to this single neighbor (single-copy, best DP).
    ForwardTo {
        next_hop: PeerId,
        reason: OpportunisticReason,
    },
    /// No DP advantage; no spray budget left; fall through to L0.
    NoAdvantage,
}

/// Per-message copy budget for binary spray-and-wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SprayBudget {
    /// Copies still allocatable (spray phase). 1 = wait phase.
    pub remaining: u32,
}

impl SprayBudget {
    pub fn new(l: u32) -> Self {
        SprayBudget {
            remaining: l.max(1),
        }
    }

    /// L per priority band: P0 → epidemic (unlimited), P1–P2 → L,
    /// P3 → half-L floor 2, P4–P6 → 3 (IRIS standard multi-hop class — PAL
    /// L0 `max_hops_for_priority` is 3, so direct-only would strand the
    /// store-carry class), P7 → 1. Tied to hop budgets: copies never exceed
    /// the L0 priority hop budget.
    pub fn l_for_priority(p: MessagePriority, hop_budget: u8) -> u32 {
        match p {
            MessagePriority::P0 => return u32::MAX, // epidemic semantics
            MessagePriority::P1 | MessagePriority::P2 => DEFAULT_SPRAY_L,
            MessagePriority::P3 => 4,
            MessagePriority::P4 | MessagePriority::P5 | MessagePriority::P6 => 3,
            MessagePriority::P7 => 1,
        }
        .min(hop_budget.max(1) as u32)
        .max(1)
    }

    /// Binary handoff: `give = floor(remaining/2)` when still spraying.
    /// Returns `None` in the wait phase (only direct delivery allowed).
    pub fn handoff(&mut self) -> Option<u32> {
        if self.remaining <= 1 {
            return None;
        }
        let give = self.remaining / 2;
        self.remaining -= give;
        Some(give)
    }

    pub fn is_wait_phase(&self) -> bool {
        self.remaining <= 1
    }
}

/// The opportunistic router: one `DeliveryPredictability` table plus
/// per-message GTMX+ state (max DP already seen for the message).
#[derive(Debug)]
pub struct OpportunisticRouter {
    pred: DeliveryPredictability,
    /// (message_id, destination) → highest DP this node has seen for that message.
    max_dp_seen: HashMap<(MessageId, PeerId), f64>,
    /// ROUT-22: (message_id, peer) pairs this node has already handed a
    /// spray copy to. Without this, repeated contact with the same peer
    /// (e.g. a flapping link) burns the whole copy budget on one node,
    /// defeating spray-and-wait's copy-diversity delivery guarantee.
    sprayed_to: HashSet<(MessageId, PeerId)>,
    /// ROUT-25: per-message binary-spray budget for the cold-start fallback
    /// (`spray_fallback`). Lives on the router (not passed in by the
    /// caller) so repeated fallback calls for the same message share one
    /// shrinking budget instead of each getting a fresh L.
    spray_budgets: HashMap<MessageId, SprayBudget>,
}

impl OpportunisticRouter {
    pub fn new(config: ProphetConfig) -> Self {
        OpportunisticRouter {
            pred: DeliveryPredictability::new(config),
            max_dp_seen: HashMap::new(),
            sprayed_to: HashSet::new(),
            spray_budgets: HashMap::new(),
        }
    }

    pub fn predictions(&self) -> &DeliveryPredictability {
        &self.pred
    }

    pub fn predictions_mut(&mut self) -> &mut DeliveryPredictability {
        &mut self.pred
    }

    /// Attach the SIM virtual clock to the inner DP table (SIM-001
    /// determinism).
    pub fn with_virtual_clock(mut self, clock: Arc<AtomicU64>) -> Self {
        self.pred = std::mem::replace(
            &mut self.pred,
            DeliveryPredictability::new(ProphetConfig::default()),
        )
        .with_virtual_clock(clock);
        self
    }

    pub fn p_for(&self, dst: &PeerId) -> f64 {
        self.pred.p_for(dst)
    }

    /// Record a contact with a neighbor, exchanging DP snapshots (Eq. 1 + 3).
    pub fn on_contact(&mut self, other: &PeerId, other_predictions: &[(PeerId, f64)]) {
        let _ = self.pred.meet(other, other_predictions);
    }

    /// GTMX+ consult: is `candidate` (with DP `p_candidate` for the
    /// destination) a strictly better forwarder than this node, and does it
    /// beat the best DP seen for this message so far?
    pub fn gtmx_advantage(
        &mut self,
        message_id: &MessageId,
        candidate: &PeerId,
        p_candidate: f64,
        dest: &PeerId,
        priority: MessagePriority,
        hop_budget: u8,
    ) -> bool {
        // ROUT-8: NaN/∞ DPs bypass all <= guards (NaN comparisons are always false).
        if !p_candidate.is_finite() {
            return false;
        }
        if p_candidate <= self.pred.config().p_first_threshold {
            return false;
        }
        let p_self = self.p_for(dest);
        if p_candidate <= p_self + DP_FLOOR_EPS {
            return false;
        }
        if hop_budget == 0 {
            return false;
        }
        // ROUT-7: per-message monotonicity (GTMX+): key is (message_id, dest),
        // not (candidate, dest) — each message has its own monotonicity state.
        let key = (*message_id, *dest);
        let seen = self.max_dp_seen.get(&key).copied().unwrap_or(0.0);
        if p_candidate <= seen + DP_FLOOR_EPS {
            return false;
        }
        let _ = priority; // spray tie-break reserved for message priority tiers
        true
    }

    /// Decide forwarding for a single message to `dest`.
    ///
    /// `candidates` = current neighbors with their DP for `dest` (the
    /// neighbor's view). Returns the best candidate with a strict GTMX+
    /// advantage, else `NoAdvantage`.
    pub fn decide(
        &mut self,
        message_id: &MessageId,
        dest: &PeerId,
        candidates: &[(PeerId, f64)],
        priority: MessagePriority,
        hop_budget: u8,
    ) -> OpportunisticDecision {
        let mut best: Option<(PeerId, f64)> = None;
        for (nb, p) in candidates {
            // ROUT-8: clamp wire DP to [0,1] before any comparison.
            let p = p.clamp(0.0_f64, 1.0_f64);
            if self.gtmx_advantage(message_id, nb, p, dest, priority, hop_budget)
                && best.as_ref().map(|(_, bp)| p.total_cmp(bp) == std::cmp::Ordering::Greater).unwrap_or(true)
            {
                best = Some((*nb, p));
            }
        }
        match best {
            Some((nb, p)) => {
                // ROUT-7: record per-message max DP under the (message_id, dest) key.
                // ROUT-14: evict an arbitrary entry when at the capacity bound so
                // the map stays ≤ MAX_DP_SEEN entries regardless of message count.
                let key = (*message_id, *dest);
                if !self.max_dp_seen.contains_key(&key)
                    && self.max_dp_seen.len() >= MAX_DP_SEEN
                {
                    if let Some(evict) = self.max_dp_seen.keys().next().copied() {
                        self.max_dp_seen.remove(&evict);
                    }
                }
                let e = self.max_dp_seen.entry(key).or_insert(0.0);
                *e = p.max(*e);
                OpportunisticDecision::ForwardTo {
                    next_hop: nb,
                    reason: OpportunisticReason::Prophet,
                }
            }
            None => OpportunisticDecision::NoAdvantage,
        }
    }

    /// Length of the `max_dp_seen` map — exposed for testing only.
    #[cfg(test)]
    pub fn dp_seen_len(&self) -> usize {
        self.max_dp_seen.len()
    }

    /// True when the `max_dp_seen` map is empty — exposed for testing only.
    #[cfg(test)]
    pub fn dp_seen_is_empty(&self) -> bool {
        self.max_dp_seen.is_empty()
    }

    /// Periodic maintenance: drop the entire `max_dp_seen`, `sprayed_to` and
    /// `spray_budgets` maps.
    ///
    /// Called by `RoutingEngine::prune()` every PRUNE_INTERVAL (5 min) so
    /// in-flight message entries do not accumulate forever (ROUT-14).
    /// Losing `max_dp_seen` causes at most one extra forward per message per
    /// period — the monotonicity property resets cleanly at each prune.
    /// Losing `sprayed_to` (ROUT-22) can cause at most one repeat handoff to
    /// an already-sprayed peer per period, bounded the same way. Losing
    /// `spray_budgets` (ROUT-25) resets an in-flight message's cold-start
    /// copy count to a fresh L on the next fallback call — at most one
    /// extra spray-and-wait cycle's worth of overhead per period, same
    /// bounded-blast-radius trade-off the other two maps already make.
    pub fn prune_dp_seen(&mut self) {
        self.max_dp_seen.clear();
        self.sprayed_to.clear();
        self.spray_budgets.clear();
    }

    /// Cold-start spray: hand off a binary-spray copy to `contact` when the
    /// node has no DP advantage yet. Mutates the spray budget.
    ///
    /// ROUT-22: returns `NoAdvantage` for a peer this message has already
    /// been sprayed to — without this, a flapping link (repeated contact
    /// with the same peer) concentrates every copy on one node instead of
    /// distributing L copies to L distinct nodes, which is equivalent to
    /// L=1 and defeats spray-and-wait's delivery guarantee.
    pub fn spray(
        &mut self,
        message_id: &MessageId,
        contact: &PeerId,
        budget: &mut SprayBudget,
    ) -> OpportunisticDecision {
        let key = (*message_id, *contact);
        if self.sprayed_to.contains(&key) {
            return OpportunisticDecision::NoAdvantage;
        }
        match budget.handoff() {
            Some(_) => {
                self.sprayed_to.insert(key);
                OpportunisticDecision::ForwardTo {
                    next_hop: *contact,
                    reason: OpportunisticReason::Spray,
                }
            }
            None => OpportunisticDecision::NoAdvantage,
        }
    }

    /// ROUT-25: cold-start fallback. Consulted by the engine when GTMX+
    /// (`decide`) finds no DP advantage *and* this node has no DP history at
    /// all for `dest` (`!predictions().has_entry(dest)` — truly cold, not
    /// just "history decayed to 0.0"). Without this, a fresh node with an
    /// unknown destination falls straight through every message to
    /// unbounded flood, which is exactly the "bounded overhead" behaviour
    /// spray-and-wait exists to avoid (ROUTE2_DESIGN.md: *"When the routing
    /// table has no useful history, L2 falls back to L0 binary
    /// spray-and-wait"*).
    ///
    /// Owns a per-message `SprayBudget`, created with `l_for_priority` on
    /// first use and shared across every fallback call for that message, so
    /// repeated cold-start contacts for the same message spray from one
    /// shrinking L rather than each getting a fresh one. Delegates to
    /// `spray()` (ROUT-22's repeat-contact dedup applies here too).
    pub fn spray_fallback(
        &mut self,
        message_id: &MessageId,
        dest: &PeerId,
        contact: &PeerId,
        priority: MessagePriority,
        hop_budget: u8,
    ) -> OpportunisticDecision {
        if self.pred.has_entry(dest) {
            // Not truly cold-start — GTMX+ has a real basis to compare
            // against (even if it just lost this round); let flood/store
            // handle it rather than spraying an unbounded number of
            // messages that do have DP history.
            return OpportunisticDecision::NoAdvantage;
        }
        let mut budget = *self
            .spray_budgets
            .entry(*message_id)
            .or_insert_with(|| SprayBudget::new(SprayBudget::l_for_priority(priority, hop_budget)));
        let decision = self.spray(message_id, contact, &mut budget);
        self.spray_budgets.insert(*message_id, budget);
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PeerId;
    use crate::protocol::MessageId;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn mid() -> MessageId {
        MessageId::new_v7()
    }

    #[test]
    fn spray_l_by_priority_respects_hop_budget() {
        assert_eq!(
            SprayBudget::l_for_priority(MessagePriority::P0, 3),
            u32::MAX
        );
        assert_eq!(SprayBudget::l_for_priority(MessagePriority::P1, 5), 5);
        assert_eq!(SprayBudget::l_for_priority(MessagePriority::P3, 5), 4);
        assert_eq!(SprayBudget::l_for_priority(MessagePriority::P4, 3), 3);
        assert_eq!(SprayBudget::l_for_priority(MessagePriority::P7, 3), 1);
        // Budget never exceeds hop budget.
        assert!(SprayBudget::l_for_priority(MessagePriority::P1, 2) <= 2);
    }

    #[test]
    fn binary_handoff_halves_copies() {
        let mut b = SprayBudget::new(8);
        assert_eq!(b.handoff(), Some(4));
        assert_eq!(b.remaining, 4);
        assert_eq!(b.handoff(), Some(2));
        assert_eq!(b.remaining, 2);
        assert_eq!(b.handoff(), Some(1));
        assert_eq!(b.remaining, 1);
        assert_eq!(b.handoff(), None); // wait phase
        assert!(b.is_wait_phase());
    }

    #[test]
    fn gtmx_requires_strict_advantage_above_floor() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        let me = pid(1);
        // Prime my own DP for dest via a meet (P(A,dest)=0.495 first contact).
        r.on_contact(&pid(8), &[(dest, 0.9)]);
        let msg = mid();
        // Candidate below floor → no.
        assert!(!r.gtmx_advantage(&msg, &pid(3), 0.05, &dest, MessagePriority::P4, 3));
        // Candidate below my DP → no.
        assert!(!r.gtmx_advantage(&msg, &pid(3), r.p_for(&dest), &dest, MessagePriority::P4, 3));
        // Candidate strictly better → yes.
        assert!(r.gtmx_advantage(&msg, &pid(4), 0.9, &dest, MessagePriority::P4, 3));
        let _ = me;
    }

    #[test]
    fn decide_picks_best_strict_advantage() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        r.on_contact(&pid(8), &[(dest, 0.9)]); // my DP for dest now > 0
        let decision = r.decide(
            &mid(),
            &dest,
            &[(pid(2), 0.3), (pid(3), 0.8), (pid(4), 0.75)],
            MessagePriority::P4,
            3,
        );
        assert_eq!(
            decision,
            OpportunisticDecision::ForwardTo {
                next_hop: pid(3),
                reason: OpportunisticReason::Prophet,
            }
        );
    }

    #[test]
    fn decide_no_advantage_when_candidates_not_better() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        // Empty DP table: a candidate below the P_first_threshold floor is
        // never consulted (no DP advantage claimed).
        let decision = r.decide(&mid(), &dest, &[(pid(2), 0.05)], MessagePriority::P4, 3);
        assert_eq!(decision, OpportunisticDecision::NoAdvantage);
        // Hop budget exhausted → no advantage even for a strong candidate.
        let decision = r.decide(&mid(), &dest, &[(pid(2), 0.9)], MessagePriority::P4, 0);
        assert_eq!(decision, OpportunisticDecision::NoAdvantage);
    }

    #[test]
    fn decide_requires_candidate_above_self_and_floor() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        // Prime my own DP via a stronger transitive path to dest.
        r.on_contact(&pid(8), &[(dest, 0.9)]);
        // Candidate 0.05 is below the discard floor → no advantage.
        assert_eq!(
            r.decide(&mid(), &dest, &[(pid(2), 0.05)], MessagePriority::P4, 3),
            OpportunisticDecision::NoAdvantage
        );
        // Candidate below/equal my own DP → no advantage.
        let mine = r.p_for(&dest);
        assert_eq!(
            r.decide(&mid(), &dest, &[(pid(2), mine)], MessagePriority::P4, 3),
            OpportunisticDecision::NoAdvantage
        );
    }

    #[test]
    fn spray_handoff_until_budget_exhausted() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let mut budget = SprayBudget::new(4);
        let msg = mid();
        let contacts = [pid(2), pid(3), pid(4)];
        let mut handed = 0;
        for c in &contacts {
            if let OpportunisticDecision::ForwardTo {
                reason: OpportunisticReason::Spray,
                ..
            } = r.spray(&msg, c, &mut budget)
            {
                handed += 1;
            }
        }
        // 4→2→1 (2 handoffs) then wait phase.
        assert_eq!(handed, 2);
        assert!(budget.is_wait_phase());
    }

    #[test]
    fn rout22_spray_does_not_reflood_same_contact() {
        // Regression: spray() had no memory of prior recipients, so repeated
        // contact with the same peer (e.g. a flapping link) burned the whole
        // copy budget on one node instead of distributing L copies to L
        // distinct nodes (equivalent to L=1).
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let mut budget = SprayBudget::new(8);
        let msg = mid();
        let peer = pid(2);

        let first = r.spray(&msg, &peer, &mut budget);
        assert!(matches!(
            first,
            OpportunisticDecision::ForwardTo {
                reason: OpportunisticReason::Spray,
                ..
            }
        ));
        assert_eq!(budget.remaining, 4, "first handoff still halves the budget");

        // Same peer contacts again (link flap) — must not receive a second
        // copy or consume further budget.
        let second = r.spray(&msg, &peer, &mut budget);
        assert_eq!(second, OpportunisticDecision::NoAdvantage);
        assert_eq!(budget.remaining, 4, "budget must be untouched by a repeat contact");

        // A genuinely different peer is unaffected.
        let other = pid(3);
        let third = r.spray(&msg, &other, &mut budget);
        assert!(matches!(
            third,
            OpportunisticDecision::ForwardTo {
                reason: OpportunisticReason::Spray,
                ..
            }
        ));
    }

    #[test]
    fn rout22_prune_clears_sprayed_to() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let mut budget = SprayBudget::new(8);
        let msg = mid();
        let peer = pid(2);
        r.spray(&msg, &peer, &mut budget);
        // Without a prune, a repeat is rejected.
        assert_eq!(r.spray(&msg, &peer, &mut budget), OpportunisticDecision::NoAdvantage);
        r.prune_dp_seen();
        // After a prune, the dedup memory is gone and the budget (still 4,
        // untouched by the rejected repeat) allows another handoff — this
        // only matters in practice because prune_dp_seen runs on a
        // 5-minute cadence, long after any single message's real spray
        // phase completes.
        assert!(matches!(
            r.spray(&msg, &peer, &mut budget),
            OpportunisticDecision::ForwardTo { .. }
        ));
    }

    #[test]
    fn rout25_spray_fallback_fires_when_cold_start() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        let msg = mid();
        let contact = pid(2);

        // Genuinely cold: no DP entry for dest at all.
        assert!(!r.predictions().has_entry(&dest));
        let decision = r.spray_fallback(&msg, &dest, &contact, MessagePriority::P4, 3);
        assert_eq!(
            decision,
            OpportunisticDecision::ForwardTo {
                next_hop: contact,
                reason: OpportunisticReason::Spray,
            }
        );
    }

    #[test]
    fn rout25_spray_fallback_shares_one_budget_across_calls() {
        // l_for_priority(P1, hop_budget=5) = min(DEFAULT_SPRAY_L=8, 5) = 5
        // (ROUT-23, still open, means the hop budget clamps L here).
        // SprayBudget::new(5).handoff() sequence: 5->3 (give 2), 3->2
        // (give 1), 2->1 (give 1), then wait phase (remaining<=1) — three
        // successful handoffs to three distinct contacts, then the fourth
        // distinct contact must get NoAdvantage. This only holds if the
        // budget is genuinely shared/shrinking across fallback calls for
        // the same message, not reset fresh on each call.
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        let msg = mid();

        let first = r.spray_fallback(&msg, &dest, &pid(2), MessagePriority::P1, 5);
        assert!(matches!(first, OpportunisticDecision::ForwardTo { .. }));
        let second = r.spray_fallback(&msg, &dest, &pid(3), MessagePriority::P1, 5);
        assert!(matches!(second, OpportunisticDecision::ForwardTo { .. }));
        let third = r.spray_fallback(&msg, &dest, &pid(4), MessagePriority::P1, 5);
        assert!(matches!(third, OpportunisticDecision::ForwardTo { .. }));
        let fourth = r.spray_fallback(&msg, &dest, &pid(5), MessagePriority::P1, 5);
        assert_eq!(fourth, OpportunisticDecision::NoAdvantage);
    }

    #[test]
    fn rout25_spray_fallback_declines_when_dp_history_exists() {
        // Not cold-start: this node has a real (if weak) DP entry for dest.
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        r.on_contact(&pid(8), &[(dest, 0.9)]);
        assert!(r.predictions().has_entry(&dest));

        let decision = r.spray_fallback(&mid(), &dest, &pid(2), MessagePriority::P4, 3);
        assert_eq!(decision, OpportunisticDecision::NoAdvantage);
    }

    #[test]
    fn rout8_nan_inf_dp_never_wins() {
        // Regression: NaN bypasses all <= guards (NaN comparisons are always false).
        // An attacker advertising NaN would be forwarded to and honest candidates ignored.
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        r.on_contact(&pid(8), &[(dest, 0.9)]); // my DP > 0 so gtmx can fire
        let msg = mid();

        assert!(!r.gtmx_advantage(&msg, &pid(5), f64::NAN, &dest, MessagePriority::P4, 3));
        assert!(!r.gtmx_advantage(&msg, &pid(5), f64::INFINITY, &dest, MessagePriority::P4, 3));
        assert!(!r.gtmx_advantage(&msg, &pid(5), f64::NEG_INFINITY, &dest, MessagePriority::P4, 3));

        // NaN/Inf candidate loses to an honest one in decide().
        let decision = r.decide(
            &mid(),
            &dest,
            &[(pid(2), f64::NAN), (pid(3), 0.95)],
            MessagePriority::P4,
            3,
        );
        assert_eq!(
            decision,
            OpportunisticDecision::ForwardTo {
                next_hop: pid(3),
                reason: OpportunisticReason::Prophet,
            },
            "honest candidate should win over NaN"
        );
    }

    #[test]
    fn rout14_max_dp_seen_bounded_by_capacity() {
        // Regression: max_dp_seen was unbounded; an attacker advertising
        // many destinations could exhaust RAM (ROUT-14).
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(99);

        // Insert exactly MAX_DP_SEEN + 10 unique (message_id, dest) pairs.
        for _ in 0..(MAX_DP_SEEN + 10) {
            let m = mid();
            // Give a strong candidate so a ForwardTo is returned and the map is written.
            r.on_contact(&pid(8), &[(dest, 0.9)]);
            let _ = r.decide(&m, &dest, &[(pid(5), 0.95)], MessagePriority::P4, 3);
        }

        assert!(
            r.dp_seen_len() <= MAX_DP_SEEN,
            "max_dp_seen must not exceed MAX_DP_SEEN (got {})",
            r.dp_seen_len()
        );
    }

    #[test]
    fn rout14_prune_dp_seen_clears_map() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        r.on_contact(&pid(8), &[(dest, 0.9)]);
        let _ = r.decide(&mid(), &dest, &[(pid(5), 0.95)], MessagePriority::P4, 3);
        assert!(!r.dp_seen_is_empty(), "expected entry before prune");
        r.prune_dp_seen();
        assert!(r.dp_seen_is_empty(), "prune_dp_seen must clear the map");
    }
}
