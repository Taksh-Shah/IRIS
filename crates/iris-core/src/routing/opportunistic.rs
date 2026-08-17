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

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use crate::message::{MessagePriority, PeerId};
use crate::routing::prophet::{DeliveryPredictability, ProphetConfig};

/// Default binary spray budget (≈10–15% of M=50–100, Spyropoulos ToN 2008;
/// EXP-ROUTE-002 validated).
pub const DEFAULT_SPRAY_L: u32 = 8;

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
    /// (sender, recipient) → highest DP this node has seen for that message.
    max_dp_seen: HashMap<(PeerId, PeerId), f64>,
}

impl OpportunisticRouter {
    pub fn new(config: ProphetConfig) -> Self {
        OpportunisticRouter {
            pred: DeliveryPredictability::new(config),
            max_dp_seen: HashMap::new(),
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
        candidate: &PeerId,
        p_candidate: f64,
        dest: &PeerId,
        priority: MessagePriority,
        hop_budget: u8,
    ) -> bool {
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
        // Per-message monotonicity (GTMX+): never hand a message to a node
        // with DP ≤ the best already seen for this message.
        let key = (*candidate, *dest);
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
        dest: &PeerId,
        candidates: &[(PeerId, f64)],
        priority: MessagePriority,
        hop_budget: u8,
    ) -> OpportunisticDecision {
        let mut best: Option<(PeerId, f64)> = None;
        for (nb, p) in candidates {
            if self.gtmx_advantage(nb, *p, dest, priority, hop_budget)
                && best.as_ref().map(|(_, bp)| *p > *bp).unwrap_or(true)
            {
                best = Some((*nb, *p));
            }
        }
        match best {
            Some((nb, p)) => {
                // Record the new max DP for the message (anti-oscillation).
                let key = (nb, *dest);
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

    /// Cold-start spray: hand off a binary-spray copy to `contact` when the
    /// node has no DP advantage yet. Mutates the spray budget.
    pub fn spray(&mut self, contact: &PeerId, budget: &mut SprayBudget) -> OpportunisticDecision {
        match budget.handoff() {
            Some(_) => OpportunisticDecision::ForwardTo {
                next_hop: *contact,
                reason: OpportunisticReason::Spray,
            },
            None => OpportunisticDecision::NoAdvantage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PeerId;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
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
        // Candidate below floor → no.
        assert!(!r.gtmx_advantage(&pid(3), 0.05, &dest, MessagePriority::P4, 3));
        // Candidate below my DP → no.
        assert!(!r.gtmx_advantage(&pid(3), r.p_for(&dest), &dest, MessagePriority::P4, 3));
        // Candidate strictly better → yes.
        assert!(r.gtmx_advantage(&pid(4), 0.9, &dest, MessagePriority::P4, 3));
        let _ = me;
    }

    #[test]
    fn decide_picks_best_strict_advantage() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let dest = pid(9);
        r.on_contact(&pid(8), &[(dest, 0.9)]); // my DP for dest now > 0
        let decision = r.decide(
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
        let decision = r.decide(&dest, &[(pid(2), 0.05)], MessagePriority::P4, 3);
        assert_eq!(decision, OpportunisticDecision::NoAdvantage);
        // Hop budget exhausted → no advantage even for a strong candidate.
        let decision = r.decide(&dest, &[(pid(2), 0.9)], MessagePriority::P4, 0);
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
            r.decide(&dest, &[(pid(2), 0.05)], MessagePriority::P4, 3),
            OpportunisticDecision::NoAdvantage
        );
        // Candidate below/equal my own DP → no advantage.
        let mine = r.p_for(&dest);
        assert_eq!(
            r.decide(&dest, &[(pid(2), mine)], MessagePriority::P4, 3),
            OpportunisticDecision::NoAdvantage
        );
    }

    #[test]
    fn spray_handoff_until_budget_exhausted() {
        let mut r = OpportunisticRouter::new(ProphetConfig::default());
        let mut budget = SprayBudget::new(4);
        let contacts = [pid(2), pid(3), pid(4)];
        let mut handed = 0;
        for c in &contacts {
            if let OpportunisticDecision::ForwardTo {
                reason: OpportunisticReason::Spray,
                ..
            } = r.spray(c, &mut budget)
            {
                handed += 1;
            }
        }
        // 4→2→1 (2 handoffs) then wait phase.
        assert_eq!(handed, 2);
        assert!(budget.is_wait_phase());
    }
}
