//! L0 deterministic routing — ROUTE-001.
//!
//! Implements `docs/routing/BASELINE_ROUTING.md` — four deterministic
//! algorithms executed in sequence per message:
//!
//! 1. **Direct** — destination is a current neighbor (O(1)).
//! 2. **KnownPath** — routing table has a path; next-hop reachability checked.
//! 3. **Flood** — hop-limited broadcast (no-backtrack, dedup via
//!    [`ForwardedCache`]).
//! 4. **Store** — delegate to persistent storage for store-carry-forward.
//!
//! Correctness guarantees (BASELINE_ROUTING.md §Correctness): no loops (hop
//! budget decreases monotonically), no duplicate delivery (dedup cache), no
//! zombie messages (TTL checked at every decision), eventual delivery within
//! TTL+hop budgets, priority preservation.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::discovery::neighbor_table::NeighborTable;
use crate::message::{LinkQuality, PeerId};
use crate::observability::event;
use crate::observability::metric;
use crate::observability::MetricsRegistry;
use crate::protocol::MessageId;

pub mod dedup_cache;
pub mod direct;
pub mod flood;
pub mod known_path;
pub mod opportunistic;
pub mod prophet;
pub mod scf;
pub mod scf_contact;
pub mod scf_eviction;
pub mod store;

pub use dedup_cache::ForwardedCache;
pub use direct::try_direct;
pub use flood::{max_hops_for_priority, recipients_for_flood, FloodPolicy};
pub use known_path::{try_known_path, RouteEntry, RoutingTable};
pub use opportunistic::{
    OpportunisticDecision, OpportunisticReason, OpportunisticRouter, SprayBudget,
};
pub use prophet::{DeliveryPredictability, ProphetConfig};
pub use scf::ScfEngine;
pub use scf_eviction::{DeviceClass, ScfEvictionPolicy};
pub use store::{store_or_drop, ShouldStore};

/// Which algorithm produced a forwarding decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForwardingAlgorithm {
    Direct,
    KnownPath,
    /// ROUTE-002: single-copy forward to the neighbor with the best delivery
    /// predictability (PRoPHET v2, RFC 6693) or a binary spray handoff.
    Opportunistic,
    // ROUT-34: `Flood` and `Store` were never removed — `ForwardingDecision`
    // has its own top-level `Flood { recipients }` and `Store` variants for
    // those outcomes, so `Forward { algorithm: Flood | Store }` was never
    // constructible anywhere in the crate (verified: zero construction
    // sites outside the now-deleted match arms below).
}

/// Result of running the routing engine on one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardingDecision {
    /// Forward to a single next hop over `transport` (Direct, KnownPath or
    /// Opportunistic). ROUT-21: `transport` is the best-quality live link to
    /// `next_hop` at decision time — selecting it here (rather than leaving
    /// the caller to re-derive or guess it) means a peer reachable only over
    /// a currently-unusable transport is a caller-visible fact, not silent
    /// message loss.
    Forward {
        next_hop: PeerId,
        transport: crate::transport::TransportId,
        algorithm: ForwardingAlgorithm,
    },
    /// Flood to the listed neighbors, each over its own best-quality
    /// transport (Algorithm 3). Per-recipient rather than a single shared
    /// transport because a flood's recipients are not all reachable over
    /// the same one on a multi-transport node.
    Flood {
        recipients: Vec<(PeerId, crate::transport::TransportId)>,
    },
    /// Store-carry-forward (Algorithm 4).
    Store,
    /// Discard: expired or no action possible.
    Drop,
}

impl ForwardingDecision {
    /// Allocation-free algorithm label for telemetry (OBS-001, OBS-RT-02).
    pub fn algorithm_label(&self) -> &'static str {
        match self {
            ForwardingDecision::Forward { algorithm, .. } => match algorithm {
                ForwardingAlgorithm::Direct => "Direct",
                ForwardingAlgorithm::KnownPath => "KnownPath",
                ForwardingAlgorithm::Opportunistic => "Opportunistic",
            },
            ForwardingDecision::Flood { .. } => "Flood",
            ForwardingDecision::Store => "Store",
            ForwardingDecision::Drop => "Drop",
        }
    }
}

/// How often to opportunistically prune the routing table inside `decide`.
const PRUNE_INTERVAL: Duration = Duration::from_secs(300); // 5 min

/// ROUT-21: select a live transport to `peer`, or `None` if no selectable
/// link exists (letting the caller fall through to the next algorithm
/// rather than returning a decision with nothing to send over).
async fn transport_to(
    peer: &PeerId,
    neighbor_table: &NeighborTable,
) -> Option<crate::transport::TransportId> {
    let links = neighbor_table.links_to(peer).await?;
    crate::discovery::neighbor_table::best_transport(&links)
}

/// Runtime state of the routing engine.
pub struct RoutingEngine {
    routing_table: RoutingTable,
    table_ttl: Duration,
    forward_cache: ForwardedCache,
    contact_log: HashMap<PeerId, Instant>,
    /// Optional ROUTE-002 L2 opportunistic layer. `None` when disabled → the
    /// engine is pure L0 (identical behavior to ROUTE-001).
    opportunistic: Option<crate::routing::opportunistic::OpportunisticRouter>,
    telemetry: MetricsRegistry,
    /// ROUT-11: last time prune() ran; drives opportunistic maintenance.
    last_pruned: Instant,
}

impl Default for RoutingEngine {
    fn default() -> Self {
        RoutingEngine {
            routing_table: RoutingTable::new(Duration::from_secs(600)),
            table_ttl: Duration::from_secs(600),
            forward_cache: ForwardedCache::default(),
            contact_log: HashMap::new(),
            opportunistic: None,
            telemetry: MetricsRegistry::new(),
            last_pruned: Instant::now(),
        }
    }
}

impl RoutingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inject a metrics registry (OBS-001). Defaults to an isolated registry.
    pub fn with_telemetry(mut self, telemetry: MetricsRegistry) -> Self {
        self.telemetry = telemetry;
        self
    }

    pub fn telemetry(&self) -> &MetricsRegistry {
        &self.telemetry
    }

    /// Enable the ROUTE-002 opportunistic layer with the given PRoPHET
    /// configuration.
    pub fn with_opportunistic(mut self, config: crate::routing::prophet::ProphetConfig) -> Self {
        self.opportunistic = Some(crate::routing::opportunistic::OpportunisticRouter::new(
            config,
        ));
        self
    }

    pub fn opportunistic(&self) -> Option<&crate::routing::opportunistic::OpportunisticRouter> {
        self.opportunistic.as_ref()
    }

    pub fn opportunistic_mut(
        &mut self,
    ) -> Option<&mut crate::routing::opportunistic::OpportunisticRouter> {
        self.opportunistic.as_mut()
    }

    pub fn routing_table(&self) -> &RoutingTable {
        &self.routing_table
    }

    pub fn routing_table_mut(&mut self) -> &mut RoutingTable {
        &mut self.routing_table
    }

    pub fn forward_cache(&self) -> &ForwardedCache {
        &self.forward_cache
    }

    /// Record a contact (e.g. from DISCO-001 `PeerDiscovered`). Feeds the
    /// routing table as a 1-hop path, logs contact for later analysis, and
    /// (when the L2 layer is enabled) exchanges DP snapshots with the peer.
    pub async fn record_contact(&mut self, peer: PeerId, transport: &str) {
        self.contact_log.insert(peer, Instant::now());
        self.routing_table
            .upsert(peer, peer, transport.to_string(), 1, LinkQuality::Good);
        if let Some(opp) = self.opportunistic_mut() {
            // Direct-contact DP update (Eq. 1). The peer's full DP snapshot is
            // applied at exchange time (SIM/transport wiring) — passing an
            // empty list bounds transitivity to explicitly-shared data.
            opp.predictions_mut().meet(&peer, &[]);
        }
    }

    /// Run the four-algorithm chain for a message addressed to `recipient`.
    ///
    /// `already_flooded` tracks no-backtrack neighbors; `sender` is the
    /// message source (excluded from flood). Hop count must not reach the
    /// priority-based cap, shared with the flood module.
    ///
    /// When the ROUTE-002 L2 layer is enabled, a single-copy opportunistic
    /// forward (best neighbor DP) is consulted between KnownPath and Flood —
    /// see [`RoutingEngine::decide_opportunistic`].
    pub async fn decide(
        &mut self,
        message_id: MessageId,
        now_unix: u64,
        timestamp: u64,
        ttl_seconds: u64,
        sender: PeerId,
        recipient: PeerId,
        hop_count: u8,
        priority: crate::message::MessagePriority,
        already_flooded: Vec<PeerId>,
        neighbor_table: &NeighborTable,
    ) -> ForwardingDecision {
        // ROUT-11: opportunistic maintenance — prune routing table and contact
        // log every PRUNE_INTERVAL without requiring a separate background task.
        if self.last_pruned.elapsed() >= PRUNE_INTERVAL {
            self.prune();
            self.last_pruned = Instant::now();
        }
        let decision = self
            .decide_inner(
                message_id,
                now_unix,
                timestamp,
                ttl_seconds,
                sender,
                recipient,
                hop_count,
                priority,
                already_flooded,
                neighbor_table,
            )
            .await;
        let algorithm = decision.algorithm_label();
        self.telemetry.increment(metric::ROUTING_DECISIONS_TOTAL);
        // ROUT-27: floods_total was registered but never incremented, so a
        // dashboard/alert built on it always read 0 — including during an
        // active flood storm.
        if matches!(decision, ForwardingDecision::Flood { .. }) {
            self.telemetry.increment(metric::ROUTING_FLOODS_TOTAL);
        }
        tracing::debug!(
            event = event::ROUTE_DECISION,
            dest = %recipient.short(),
            priority = priority.as_u8(),
            algorithm,
            "routing decision made"
        );
        decision
    }

    /// Four-algorithm chain (Algorithm 1 → 2 → 2.5 → 3 → 4).
    async fn decide_inner(
        &mut self,
        message_id: MessageId,
        now_unix: u64,
        timestamp: u64,
        ttl_seconds: u64,
        sender: PeerId,
        recipient: PeerId,
        hop_count: u8,
        priority: crate::message::MessagePriority,
        already_flooded: Vec<PeerId>,
        neighbor_table: &NeighborTable,
    ) -> ForwardingDecision {
        // ROUT-6: TTL gate — do not store or forward expired messages.
        if crate::routing::store::ttl_expired(now_unix, timestamp, ttl_seconds) {
            return ForwardingDecision::Drop;
        }
        // ROUT-4: dedup gate — drop messages we already forwarded (anti-loop).
        if self.forward_cache.is_duplicate(&message_id) {
            return ForwardingDecision::Drop;
        }
        // Algorithm 1: Direct delivery. Exempt from the hop-budget gate below:
        // delivering straight to the final recipient consumes no further hop,
        // so it may legitimately fire even at hop_count == max_hops.
        if let Some(decision) = try_direct(recipient, neighbor_table).await {
            self.forward_cache.record(message_id);
            return decision;
        }
        // ROUT-19: hop-budget gate. Every algorithm below this point forwards
        // the message onward (KnownPath, Opportunistic, Flood) and must not
        // exceed the priority's hop ceiling — previously only Flood checked
        // this, so a stale KnownPath route (e.g. a two-node routing loop set
        // up per ROUT-12) could relay forever with no budget enforcement.
        let policy = max_hops_for_priority(priority);
        if hop_count >= policy.max_hops {
            return ForwardingDecision::Store;
        }
        // Algorithm 2: Known path (validate next-hop reachability).
        if let Some(decision) =
            try_known_path(recipient, &mut self.routing_table, neighbor_table).await
        {
            self.forward_cache.record(message_id);
            return decision;
        }
        // Algorithm 2.5 (ROUTE-002): opportunistic DP forward — consulted with
        // zero candidates here; transport wiring may supply neighbor DPs via
        // [`RoutingEngine::decide_opportunistic`] and short-circuit the flood.
        //
        // ROUT-24 (blocked — see docs/bug-hunting/taksh_problems.md): a real
        // fix needs neighbor-supplied DP snapshots for `recipient`, which no
        // wire protocol in this crate currently carries (`CapabilityBundle`
        // has no DP field at all) — adding that exchange is a
        // security-relevant protocol change (peer-supplied data feeding a
        // routing decision) outside a routing-module wiring fix's scope.
        if let Some(nb) = self.decide_opportunistic(&message_id, recipient, priority, hop_count, &[]) {
            if let Some(transport) = transport_to(&nb, neighbor_table).await {
                self.forward_cache.record(message_id);
                return ForwardingDecision::Forward {
                    next_hop: nb,
                    transport,
                    algorithm: ForwardingAlgorithm::Opportunistic,
                };
            }
        }
        // ROUT-25: binary-spray cold-start fallback. Independent of ROUT-24
        // — it needs a live neighbor to hand a copy to, not that neighbor's
        // DP, so it works today even with ROUT-24 blocked.
        if let Some(nb) = self
            .spray_fallback(&message_id, sender, recipient, priority, policy.max_hops, neighbor_table)
            .await
        {
            if let Some(transport) = transport_to(&nb, neighbor_table).await {
                self.forward_cache.record(message_id);
                return ForwardingDecision::Forward {
                    next_hop: nb,
                    transport,
                    algorithm: ForwardingAlgorithm::Opportunistic,
                };
            }
        }
        // Algorithm 3: Flood. hop_count < policy.max_hops is already
        // guaranteed by the ROUT-19 gate above.
        {
            let recipients = recipients_for_flood(
                neighbor_table,
                sender,
                &already_flooded,
                hop_count,
                &recipient,
            )
            .await;
            if !recipients.is_empty() {
                self.forward_cache.record(message_id);
                return ForwardingDecision::Flood { recipients };
            }
        }
        // Algorithm 4: Store.
        ForwardingDecision::Store
    }

    /// ROUTE-002 L2 consult: pick a single neighbor whose delivery
    /// predictability for `recipient` strictly beats ours (GTMX+). `candidates`
    /// maps each current neighbor to their DP for the recipient; when the
    /// caller has no DP view (plain L0 wiring) this returns `None` and the L0
    /// chain continues unchanged.
    pub fn decide_opportunistic(
        &mut self,
        message_id: &MessageId,
        recipient: PeerId,
        priority: crate::message::MessagePriority,
        hop_count: u8,
        candidates: &[(PeerId, f64)],
    ) -> Option<PeerId> {
        let opp = self.opportunistic_mut()?;
        let budget = max_hops_for_priority(priority).max_hops;
        if hop_count >= budget {
            return None;
        }
        match opp.decide(message_id, &recipient, candidates, priority, budget) {
            crate::routing::opportunistic::OpportunisticDecision::ForwardTo {
                next_hop, ..
            } => Some(next_hop),
            crate::routing::opportunistic::OpportunisticDecision::NoAdvantage => None,
        }
    }

    /// ROUT-25: cold-start fallback, consulted after GTMX+
    /// (`decide_opportunistic`) finds no advantage. Hands a bounded
    /// binary-spray copy to the best-linked live neighbor instead of
    /// falling straight through to unbounded flood — but only when this
    /// node has no DP history at all for `recipient` (a genuine "never seen
    /// this destination" cold start, not "GTMX+ lost this round"). Does not
    /// need ROUT-24's blocked neighbor-DP sourcing: spray only needs *some*
    /// live contact, not that contact's delivery predictability.
    pub async fn spray_fallback(
        &mut self,
        message_id: &MessageId,
        sender: PeerId,
        recipient: PeerId,
        priority: crate::message::MessagePriority,
        hop_budget: u8,
        neighbor_table: &NeighborTable,
    ) -> Option<PeerId> {
        if !self.opportunistic.as_ref()?.predictions().has_entry(&recipient) {
            // Best-linked live neighbor other than the message's sender
            // (no-backtrack, same policy as Flood) and the recipient itself
            // (Direct already tried and failed).
            let contact = neighbor_table
                .neighbor_summaries()
                .await
                .into_iter()
                .filter(|nb| {
                    nb.peer_id != sender
                        && nb.peer_id != recipient
                        && nb.state == crate::discovery::neighbor_table::NeighborState::LinkedUp
                        && !nb.links.is_empty()
                })
                .min_by_key(|nb| nb.links.iter().map(|l| l.quality).min())
                .map(|nb| nb.peer_id);
            if let Some(contact) = contact {
                let opp = self.opportunistic.as_mut()?;
                if let crate::routing::opportunistic::OpportunisticDecision::ForwardTo {
                    next_hop,
                    ..
                } = opp.spray_fallback(message_id, &recipient, &contact, priority, hop_budget)
                {
                    return Some(next_hop);
                }
            }
        }
        None
    }

    /// Prune expired routing-table entries (10 min) and release stale lists.
    pub fn prune(&mut self) {
        let before = self.routing_table.len();
        self.routing_table.prune_expired(self.table_ttl);
        // Elapsed comparison — same boot-time panic guard as SYS-1 / neighbor_table.rs.
        let now = Instant::now();
        self.contact_log.retain(|_, t| now.duration_since(*t) < Duration::from_secs(3600));
        // ROUT-14: flush per-message GTMX+ monotonicity state every pruning
        // cycle — in-flight messages within a 5-min window are short-lived
        // enough that resetting the map at most causes one extra forward.
        if let Some(opp) = self.opportunistic.as_mut() {
            opp.prune_dp_seen();
        }
        if before > 0 {
            tracing::debug!(
                event = event::ROUTE_PRUNED,
                entries_before = before,
                reason = "expired",
                "routing table pruned"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessagePriority;
    use crate::transport::TransportId;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn peer_info(id: u8) -> crate::message::PeerInfo {
        crate::message::PeerInfo {
            peer_id: pid(id),
            addresses: vec![],
            transport_addresses: vec![],
            last_seen: None,
        }
    }

    #[tokio::test]
    async fn direct_when_neighbor() {
        let mut engine = RoutingEngine::new();
        let table = NeighborTable::new(Duration::from_secs(60));
        table
            .upsert(
                &peer_info(7),
                &TransportId::from("sim"),
                LinkQuality::Excellent,
            )
            .await;
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(7), 0, MessagePriority::P4, vec![], &table)
            .await;
        assert_eq!(
            decision,
            ForwardingDecision::Forward {
                next_hop: pid(7),
                transport: TransportId::from("sim"),
                algorithm: ForwardingAlgorithm::Direct,
            }
        );
    }

    #[tokio::test]
    async fn known_path_before_flood() {
        let mut engine = RoutingEngine::new();
        engine
            .routing_table
            .upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        let table = NeighborTable::new(Duration::from_secs(60));
        table
            .upsert(&peer_info(4), &TransportId::from("sim"), LinkQuality::Good)
            .await;
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(9), 0, MessagePriority::P4, vec![], &table)
            .await;
        assert_eq!(
            decision,
            ForwardingDecision::Forward {
                next_hop: pid(4),
                transport: TransportId::from("sim"),
                algorithm: ForwardingAlgorithm::KnownPath,
            }
        );
    }

    #[tokio::test]
    async fn rout19_known_path_respects_hop_budget() {
        // Same route as known_path_before_flood, but the message has already
        // exhausted its priority's hop budget (P4 => 3). Previously only the
        // Flood branch checked hop_count, so a stale/looping KnownPath route
        // would relay forever; it must now fall through to Store.
        let mut engine = RoutingEngine::new();
        engine
            .routing_table
            .upsert(pid(9), pid(4), "sim".into(), 2, LinkQuality::Good);
        let table = NeighborTable::new(Duration::from_secs(60));
        table
            .upsert(&peer_info(4), &TransportId::from("sim"), LinkQuality::Good)
            .await;
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(9), 3, MessagePriority::P4, vec![], &table)
            .await;
        assert_eq!(decision, ForwardingDecision::Store);
    }

    #[tokio::test]
    async fn floods_to_all_eligible_neighbors() {
        let mut engine = RoutingEngine::new();
        let table = NeighborTable::new(Duration::from_secs(60));
        for n in [2u8, 3, 4] {
            table
                .upsert(&peer_info(n), &TransportId::from("sim"), LinkQuality::Good)
                .await;
        }
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(99), 0, MessagePriority::P4, vec![], &table)
            .await;
        if let ForwardingDecision::Flood { recipients } = decision {
            assert_eq!(recipients.len(), 3);
            assert!(!recipients.iter().any(|(id, _)| *id == pid(1))); // no-backtrack to sender
        } else {
            panic!("expected flood");
        }
    }

    #[tokio::test]
    async fn rout25_cold_start_sprays_instead_of_flooding() {
        // ROUT-25: with the opportunistic layer enabled and genuinely no DP
        // history for the recipient (a fresh router — the default state),
        // decide() must hand off to the spray fallback (Forward/Opportunistic)
        // instead of falling straight through to Flood.
        let mut engine =
            RoutingEngine::new().with_opportunistic(crate::routing::prophet::ProphetConfig::default());
        let table = NeighborTable::new(Duration::from_secs(60));
        for n in [2u8, 3, 4] {
            table
                .upsert(&peer_info(n), &TransportId::from("sim"), LinkQuality::Good)
                .await;
        }
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(99), 0, MessagePriority::P4, vec![], &table)
            .await;
        assert!(
            matches!(
                decision,
                ForwardingDecision::Forward {
                    algorithm: ForwardingAlgorithm::Opportunistic,
                    ..
                }
            ),
            "expected a spray fallback Forward, got {decision:?}"
        );
    }

    #[tokio::test]
    async fn rout25_warm_destination_still_floods() {
        // Counterpart: once this node has real DP history for the recipient
        // (not cold-start), spray_fallback must decline and the chain falls
        // through to Flood as before ROUT-25 — spraying every warm message
        // would defeat GTMX+'s point.
        let mut engine =
            RoutingEngine::new().with_opportunistic(crate::routing::prophet::ProphetConfig::default());
        engine
            .opportunistic_mut()
            .unwrap()
            .predictions_mut()
            .meet(&pid(99), &[]);
        let table = NeighborTable::new(Duration::from_secs(60));
        for n in [2u8, 3, 4] {
            table
                .upsert(&peer_info(n), &TransportId::from("sim"), LinkQuality::Good)
                .await;
        }
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(99), 0, MessagePriority::P4, vec![], &table)
            .await;
        assert!(
            matches!(decision, ForwardingDecision::Flood { .. }),
            "expected Flood once DP history exists, got {decision:?}"
        );
    }

    #[tokio::test]
    async fn rout27_flood_decision_increments_floods_total() {
        let reg = MetricsRegistry::new();
        let mut engine = RoutingEngine::new().with_telemetry(reg.clone());
        let table = NeighborTable::new(Duration::from_secs(60));
        for n in [2u8, 3, 4] {
            table
                .upsert(&peer_info(n), &TransportId::from("sim"), LinkQuality::Good)
                .await;
        }
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(99), 0, MessagePriority::P4, vec![], &table)
            .await;
        assert!(matches!(decision, ForwardingDecision::Flood { .. }));
        let snap = reg.snapshot();
        assert_eq!(snap[metric::ROUTING_FLOODS_TOTAL], 1, "flood decision must count");
        assert_eq!(snap[metric::ROUTING_DECISIONS_TOTAL], 1);

        // A non-flood decision (Direct) must not touch the floods counter.
        let table2 = NeighborTable::new(Duration::from_secs(60));
        table2
            .upsert(&peer_info(7), &TransportId::from("sim"), LinkQuality::Excellent)
            .await;
        let direct = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(7), 0, MessagePriority::P4, vec![], &table2)
            .await;
        assert!(matches!(direct, ForwardingDecision::Forward { .. }));
        let snap2 = reg.snapshot();
        assert_eq!(snap2[metric::ROUTING_FLOODS_TOTAL], 1, "direct decision must not increment floods");
    }

    #[tokio::test]
    async fn hop_budget_exhausted_stores() {
        let mut engine = RoutingEngine::new();
        let table = NeighborTable::new(Duration::from_secs(60));
        table
            .upsert(&peer_info(2), &TransportId::from("sim"), LinkQuality::Good)
            .await;
        let decision = engine
            .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(99), 99, MessagePriority::P4, vec![], &table)
            .await;
        assert_eq!(decision, ForwardingDecision::Store);
    }

    #[tokio::test]
    async fn records_contact_as_one_hop() {
        let mut engine = RoutingEngine::new();
        engine.record_contact(pid(8), "ble").await;
        let entry = engine.routing_table().best_route(&pid(8)).unwrap();
        assert_eq!(entry.next_hop, pid(8));
        assert_eq!(entry.hop_count, 1);
    }

    // --- M4 integration: DiscoveryManager populates the neighbor table; the
    // RoutingEngine then resolves a direct delivery from it (ORCH-0001 M4:
    // "Discovery populates neighbor table → ROUTE-001 direct delivery"). ---

    #[tokio::test]
    async fn m4_discovery_feeds_routing_direct_delivery() {
        use crate::discovery::{DiscoveryConfig, DiscoveryManager};
        use crate::message::PeerInfo;
        use std::sync::Arc;
        use std::time::Duration;

        let dm = Arc::new(DiscoveryManager::new(
            pid(1),
            DiscoveryConfig {
                neighbor_ttl: Duration::from_secs(60),
                ..Default::default()
            },
        ));
        // DISCO-001 reports a discovered peer (what a scan produces).
        dm.report_peer(
            &PeerInfo {
                peer_id: pid(42),
                addresses: vec![],
                transport_addresses: vec![],
                last_seen: None,
            },
            &TransportId::from("sim"),
            LinkQuality::Good,
        )
        .await;
        assert!(dm.neighbors().is_up(&pid(42)).await);

        // ROUTE resolves the peer as a direct next hop.
        let mut engine = RoutingEngine::new();
        let decision = engine
            .decide(
                MessageId::new_v7(),
                0, 0, u64::MAX,
                pid(1),
                pid(42),
                0,
                MessagePriority::P4,
                vec![],
                dm.neighbors().as_ref(),
            )
            .await;
        assert_eq!(
            decision,
            ForwardingDecision::Forward {
                next_hop: pid(42),
                transport: TransportId::from("sim"),
                algorithm: ForwardingAlgorithm::Direct,
            }
        );
    }

    // --- M6 integration: MSG→STORE→ROUTE→SCF. A partitioned node buffers a
    // message (ROUTE-001 Algorithm 4 Store), the SCF engine carries it, and on
    // a new contact the contact handler ranks and forwards it (ORCH-0001 M6:
    // "partitioned delivery via carry; P0 never evicted"). ---

    #[tokio::test(flavor = "multi_thread")]
    async fn m6_scf_carries_through_partition_and_delivers_on_contact() {
        use crate::message_engine::storage::MemoryStorage;
        use crate::message_engine::MessageEngine;
        use crate::routing::scf::{ScfConfig, ScfEngine};
        use crate::routing::scf_contact::ForwardCandidate;
        use std::sync::Arc;

        fn envelope_for(
            recipient: [u8; 32],
            priority: crate::message::MessagePriority,
            payload: &[u8],
        ) -> crate::protocol::Envelope {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            crate::protocol::Envelope {
                version: crate::protocol::PROTOCOL_VERSION,
                message_id: crate::protocol::MessageId::new_v7(),
                sender_id: [0xAA; 32].to_vec(),
                recipient_id: recipient.to_vec(),
                priority,
                ttl_seconds: 3600,
                timestamp,
                hop_count: 0,
                max_hops: None,
                payload_type: crate::protocol::ContentType::Text,
                payload_size: payload.len() as u64,
                payload_hash: crate::protocol::Envelope::compute_payload_hash(payload),
                payload: payload.to_vec(),
                payload_ref: None,
                signature: None,
                encryption_hdr: None,
                routing_hints: None,
                auth_cert_chain: None,
            }
        }

        let alice = [0xAA; 32];
        let bob = [0xBB; 32];
        let storage = Arc::new(MemoryStorage::new());
        let engine = Arc::new(MessageEngine::new(
            crate::message_engine::MessageEngineConfig {
                node_id: alice,
                ..Default::default()
            },
            storage.clone(),
            Arc::new(crate::message_engine::crypto::DevCryptoProvider::new()),
            Arc::new(crate::transport::TransportManager::new()),
        ));

        // 1. MSG-001: Alice sends a P4 message to Bob.
        let env = envelope_for(bob, crate::message::MessagePriority::P4, b"carry me");
        engine.send_message(env.clone()).await.unwrap();

        // 2. ROUTE-001: no neighbors — Algorithm 4 returns Store.
        let mut rte = RoutingEngine::new();
        let table = NeighborTable::new(std::time::Duration::from_secs(60));
        let decision = rte
            .decide(
                env.message_id.clone(),
                env.timestamp,
                env.timestamp,
                env.ttl_seconds,
                crate::message::PeerId(alice),
                crate::message::PeerId(bob),
                env.hop_count,
                env.priority,
                vec![],
                &table,
            )
            .await;
        assert_eq!(decision, ForwardingDecision::Store);

        // 3. SCF-001: the message lands in the carry buffer (partition).
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        scf.buffer_message(env.clone(), None).unwrap();
        assert_eq!(scf.len(), 1);
        assert_eq!(
            scf.delivery_status(&env.message_id),
            Some(crate::routing::scf::DeliveryStatus::Stored)
        );

        // 4. Contact appears (DISCO-001 event): the handler ranks candidates.
        let contact = crate::message::PeerId(bob);
        let outcome = scf.on_new_contact(&contact, None);
        assert_eq!(outcome.ranked.len(), 1);
        assert_eq!(outcome.ranked[0].to, contact);

        // 5. Forward and confirm terminal delivery state.
        let fwd: Vec<ForwardCandidate> = outcome.ranked;
        let status = scf.mark_forwarded(&fwd[0].message_id, contact, true);
        assert_eq!(
            status,
            Some(crate::routing::scf::DeliveryStatus::Delivered {
                delivered_to: contact
            })
        );
        // DTN-5: a delivered message is removed from the buffer immediately
        // rather than retained forever.
        assert_eq!(scf.delivery_status(&env.message_id), None);

        // P0 never evicted under pressure (INV-ROUTE-003 via eviction policy).
        let p0 = envelope_for(bob, crate::message::MessagePriority::P0, b"priority");
        scf.buffer_message(p0.clone(), None).unwrap();
        scf.evict_to_fit(10_000_000);
        assert!(scf.delivery_status(&p0.message_id).is_some());
    }
}
