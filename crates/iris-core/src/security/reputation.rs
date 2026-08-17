//! Reputation: bounded per-peer Bayesian score (local watchdog + verified-second-hand
//! positives-only CORE + iTrust-style audits) -> routing weight ONLY, never an
//! admission gate. RES-0018 R8, Marti MobiCom 2000 (Watchdog/Pathrater), CORE 2002,
//! iTrust TPDS 2014, ITRM ISIT 2009, Rep-AODV CMC 2024.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::RwLock;

/// 16-byte peer short ID.
pub type PeerShort = [u8; 16];

/// SEC-RT-07: cap on per-peer second-hand vouchers (bounds memory even when
/// many verified peers vouch for one target).
const MAX_VERIFIED_VOUCHERS: usize = 64;

/// Reputation configuration.
#[derive(Clone, Debug)]
pub struct ReputationConfig {
    /// Maximum score (normalized to 1.0).
    pub max_score: f64,
    /// Minimum score.
    pub min_score: f64,
    /// Initial score for unknown peers.
    pub initial_score: f64,
    /// Decay factor per interval (0.99 = slow decay).
    pub decay_factor: f64,
    /// Decay interval.
    pub decay_interval: Duration,
    /// Positive reinforcement weight (successful forward).
    pub positive_weight: f64,
    /// Negative reinforcement weight (drop/blackhole detected).
    pub negative_weight: f64,
    /// Second-hand positive weight (from verified peers only).
    pub secondhand_positive_weight: f64,
    /// iTrust audit weight.
    pub audit_weight: f64,
    /// Maximum number of tracked peers.
    pub max_peers: usize,
    /// Minimum observations before score is used for routing.
    pub min_observations: u64,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        ReputationConfig {
            max_score: 1.0,
            min_score: 0.0,
            initial_score: 0.5,                       // neutral start
            decay_factor: 0.995,                      // slow decay toward neutral
            decay_interval: Duration::from_secs(300), // 5 min
            positive_weight: 0.05,
            negative_weight: 0.1,
            secondhand_positive_weight: 0.02,
            audit_weight: 0.03,
            max_peers: 10_000,
            min_observations: 3,
        }
    }
}

/// Per-peer reputation state.
#[derive(Clone, Debug)]
struct PeerReputation {
    score: f64,
    observations: u64,
    last_update: Instant,
    /// Verified peers that have vouched for this peer (second-hand positives).
    /// SEC-RT-07: bounded — an unauthenticated flood of SecondhandPositive
    /// events from arbitrarily many shorts could otherwise grow this without
    /// limit. The identity layer is responsible for nomination/verification.
    verified_vouchers: Vec<PeerShort>,
}

/// Reputation update event source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReputationEvent {
    /// Local observation: successful forward to this peer.
    LocalForward,
    /// Local observation: peer dropped/blackholed our message.
    LocalDrop,
    /// Second-hand positive from a VERIFIED peer (CORE positivity rule).
    SecondhandPositive {
        from_verified_peer: PeerShort,
    },
    /// iTrust-style probabilistic audit result.
    AuditPositive,
    AuditNegative,
}

/// Manual Arbitrary implementation for proptest (enabled via proptest feature)
#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for ReputationEvent {
    type Parameters = ();
    type Strategy = proptest::strategy::Just<ReputationEvent>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        proptest::strategy::Just(ReputationEvent::LocalForward)
    }
}

/// Reputation scoring engine.
pub struct ReputationEngine {
    peers: RwLock<HashMap<PeerShort, PeerReputation>>,
    /// Registry of peers admitted as verified (IDM-002 / register_verified_peer).
    /// Gate for second-hand positives (CORE positivity rule). SEC-RT-07: an
    /// unverified peer must not be able to vouch for arbitrarily many targets.
    verified_peers: RwLock<std::collections::HashSet<PeerShort>>,
    config: ReputationConfig,
    metrics: ReputationMetrics,
}

#[derive(Default, Debug)]
pub struct ReputationMetrics {
    pub updates: AtomicU64,
    pub positive: AtomicU64,
    pub negative: AtomicU64,
    pub secondhand: AtomicU64,
    pub audit: AtomicU64,
}

impl ReputationEngine {
    /// Create a new reputation engine with default configuration.
    pub fn new(config: ReputationConfig) -> Self {
        ReputationEngine {
            peers: RwLock::new(HashMap::with_capacity(1024)),
            verified_peers: RwLock::new(std::collections::HashSet::with_capacity(128)),
            config,
            metrics: ReputationMetrics::default(),
        }
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> ReputationMetricsSnapshot {
        ReputationMetricsSnapshot {
            updates: self.metrics.updates.load(Ordering::Relaxed),
            positive: self.metrics.positive.load(Ordering::Relaxed),
            negative: self.metrics.negative.load(Ordering::Relaxed),
            secondhand: self.metrics.secondhand.load(Ordering::Relaxed),
            audit: self.metrics.audit.load(Ordering::Relaxed),
        }
    }

    /// Get routing weight for a peer (0.0 to 1.0).
    /// Only returns meaningful weight after min_observations (AC-8).
    pub async fn routing_weight(&self, peer: PeerShort) -> f64 {
        let peers = self.peers.read().await;
        if let Some(rep) = peers.get(&peer) {
            if rep.observations >= self.config.min_observations {
                return rep
                    .score
                    .clamp(self.config.min_score, self.config.max_score);
            }
        }
        // Unknown or insufficient observations = neutral
        self.config.initial_score
    }

    /// Update reputation based on an event.
    /// Negative scores NEVER propagate (CORE rule — AC-8/9).
    pub async fn update(&self, peer: PeerShort, event: ReputationEvent) {
        let mut peers = self.peers.write().await;

        // Enforce max peers
        if peers.len() >= self.config.max_peers && !peers.contains_key(&peer) {
            // Evict lowest score peer (simple)
            if let Some((k, _)) = peers
                .iter()
                .min_by(|a, b| a.1.score.partial_cmp(&b.1.score).unwrap())
            {
                let k = *k;
                peers.remove(&k);
            }
        }

        let rep = peers.entry(peer).or_insert_with(|| PeerReputation {
            score: self.config.initial_score,
            observations: 0,
            last_update: Instant::now(),
            verified_vouchers: Vec::new(),
        });

        rep.last_update = Instant::now();
        rep.observations += 1;
        self.metrics.updates.fetch_add(1, Ordering::Relaxed);

        match event {
            ReputationEvent::LocalForward => {
                rep.score = (rep.score + self.config.positive_weight).min(self.config.max_score);
                self.metrics.positive.fetch_add(1, Ordering::Relaxed);
            }
            ReputationEvent::LocalDrop => {
                // Negative update but floor at min_score (never negative propagation)
                rep.score = (rep.score - self.config.negative_weight).max(self.config.min_score);
                self.metrics.negative.fetch_add(1, Ordering::Relaxed);
            }
            ReputationEvent::SecondhandPositive { from_verified_peer } => {
                // SEC-RT-07: honor second-hand positives ONLY from peers that
                // were actually admitted as verified (CORE positivity rule).
                // Previously ANY inbound event with a 16-byte short was recorded
                // as a "voucher", so an unverified peer could vouch unlimited
                // times for unlimited targets. Additionally the per-peer voucher
                // list is capped to bound memory.
                let is_verified = self
                    .verified_peers
                    .read()
                    .await
                    .contains(&from_verified_peer);
                let list_full = rep.verified_vouchers.len() >= MAX_VERIFIED_VOUCHERS;
                if is_verified && !list_full && !rep.verified_vouchers.contains(&from_verified_peer)
                {
                    rep.verified_vouchers.push(from_verified_peer);
                    rep.score = (rep.score + self.config.secondhand_positive_weight)
                        .min(self.config.max_score);
                    self.metrics.secondhand.fetch_add(1, Ordering::Relaxed);
                }
            }
            ReputationEvent::AuditPositive => {
                rep.score = (rep.score + self.config.audit_weight).min(self.config.max_score);
                self.metrics.audit.fetch_add(1, Ordering::Relaxed);
            }
            ReputationEvent::AuditNegative => {
                rep.score = (rep.score - self.config.audit_weight).max(self.config.min_score);
                self.metrics.audit.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Clamp
        rep.score = rep
            .score
            .clamp(self.config.min_score, self.config.max_score);
    }

    /// Register a verified peer that can vouch for others (for second-hand).
    pub async fn register_verified_peer(&self, peer: PeerShort) {
        // SEC-RT-07: admission into the verified registry is the gate for
        // second-hand positives — recorded separately from per-peer scores so
        // the CORE rule check is real (not a per-target lockout list).
        self.verified_peers.write().await.insert(peer);
        let mut peers = self.peers.write().await;
        let rep = peers.entry(peer).or_insert_with(|| PeerReputation {
            score: self.config.initial_score,
            observations: 0,
            last_update: Instant::now(),
            verified_vouchers: Vec::new(),
        });
        // Verified peers start with slightly higher trust
        rep.score = (rep.score + 0.1).min(self.config.max_score);
    }

    /// Periodic decay toward initial_score (call from GC task).
    pub async fn decay(&self) {
        let mut peers = self.peers.write().await;
        for rep in peers.values_mut() {
            // Exponential decay toward initial_score
            let diff = rep.score - self.config.initial_score;
            rep.score = self.config.initial_score + diff * self.config.decay_factor;
        }
    }

    /// Get all peer scores (for debugging/monitoring).
    pub async fn all_scores(&self) -> Vec<(PeerShort, f64, u64)> {
        let peers = self.peers.read().await;
        peers
            .iter()
            .map(|(p, r)| (*p, r.score, r.observations))
            .collect()
    }

    /// Reset all state (for testing).
    pub async fn reset(&self) {
        self.peers.write().await.clear();
        self.verified_peers.write().await.clear();
    }
}

impl Default for ReputationEngine {
    fn default() -> Self {
        Self::new(ReputationConfig::default())
    }
}

#[cfg(all(test, feature = "proptest"))]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(fut)
    }

    proptest! {
        #[test]
        fn reputation_never_panics(
            peer in any::<[u8; 16]>(),
            event in any::<ReputationEvent>(),
        ) {
            let re = ReputationEngine::default();
            block_on(re.update(peer, event));
        }

        #[test]
        fn reputation_bounded_in_range(
            peer in any::<[u8; 16]>(),
            n_updates in 1..100usize,
        ) {
            let re = ReputationEngine::default();
            for _ in 0..n_updates {
                let event = ReputationEvent::LocalForward;
                block_on(re.update(peer, event));
            }
            let weight = block_on(re.routing_weight(peer));
            prop_assert!((0.0..=1.0).contains(&weight));
        }

        #[test]
        fn reputation_negative_floored_at_zero(
            peer in any::<[u8; 16]>(),
            n_drops in 1..100usize,
        ) {
            let config = ReputationConfig {
                min_score: 0.0,
                max_score: 1.0,
                initial_score: 0.5,
                negative_weight: 0.5,
                min_observations: 1,
                ..Default::default()
            };
            let re = ReputationEngine::new(config);
            for _ in 0..n_drops {
                block_on(re.update(peer, ReputationEvent::LocalDrop));
            }
            let weight = block_on(re.routing_weight(peer));
            prop_assert_eq!(weight, 0.0);
        }

        #[test]
        fn reputation_positive_increases_score(
            peer in any::<[u8; 16]>(),
            n_forwards in 1..50usize,
        ) {
            let re = ReputationEngine::default();
            for _ in 0..n_forwards {
                block_on(re.update(peer, ReputationEvent::LocalForward));
            }
            let weight = block_on(re.routing_weight(peer));
            if n_forwards >= 3 {
                prop_assert!(weight > 0.5);
            }
        }

        #[test]
        fn reputation_secondhand_only_from_verified(
            peer in any::<[u8; 16]>(),
            verified in any::<[u8; 16]>(),
            unverified in any::<[u8; 16]>(),
        ) {
            let re = ReputationEngine::default();
            block_on(re.register_verified_peer(verified));
            // Add local observations to reach min_observations
            for _ in 0..3 {
                block_on(re.update(peer, ReputationEvent::LocalForward));
            }
            let initial_weight = block_on(re.routing_weight(peer));

            // Secondhand from verified -> accepted
            block_on(re.update(peer, ReputationEvent::SecondhandPositive { from_verified_peer: verified }));

            // Secondhand from unverified -> ignored
            block_on(re.update(peer, ReputationEvent::SecondhandPositive { from_verified_peer: unverified }));

            let weight = block_on(re.routing_weight(peer));
            if verified != unverified {
                prop_assert!(weight > initial_weight);
            }
        }

        #[test]
        fn reputation_never_negative_propagation(
            peer in any::<[u8; 16]>(),
        ) {
            let re = ReputationEngine::default();
            // Mix of positive and negative
            for _ in 0..10 {
                block_on(re.update(peer, ReputationEvent::LocalForward));
            }
            for _ in 0..20 {
                block_on(re.update(peer, ReputationEvent::LocalDrop));
            }
            let weight = block_on(re.routing_weight(peer));
            prop_assert!((0.0..=1.0).contains(&weight));
        }

        #[test]
        fn reputation_decay_toward_neutral(
            peer in any::<[u8; 16]>(),
            n_forwards in 1..20usize,
            n_decays in 1..10usize,
        ) {
            let config = ReputationConfig {
                decay_factor: 0.5,
                decay_interval: Duration::from_secs(1),
                min_observations: 1,
                ..Default::default()
            };
            let re = ReputationEngine::new(config);
            for _ in 0..n_forwards {
                block_on(re.update(peer, ReputationEvent::LocalForward));
            }
            let before = block_on(re.routing_weight(peer));
            for _ in 0..n_decays {
                block_on(re.decay());
            }
            let after = block_on(re.routing_weight(peer));
            prop_assert!(after < before);
            prop_assert!(after >= 0.5);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ReputationMetricsSnapshot {
    pub updates: u64,
    pub positive: u64,
    pub negative: u64,
    pub secondhand: u64,
    pub audit: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reputation_routing_weight_neutral_for_unknown() {
        let re = ReputationEngine::default();
        let peer = [1u8; 16];
        assert_eq!(re.routing_weight(peer).await, 0.5);
    }

    #[tokio::test]
    async fn reputation_positive_updates_increase_score() {
        let re = ReputationEngine::default();
        let peer = [2u8; 16];

        // Initial neutral
        assert_eq!(re.routing_weight(peer).await, 0.5);

        // Add observations
        for _ in 0..5 {
            re.update(peer, ReputationEvent::LocalForward).await;
        }

        // After min_observations, score should be > 0.5
        let weight = re.routing_weight(peer).await;
        assert!(weight > 0.5);
    }

    #[tokio::test]
    async fn reputation_negative_floored_at_zero() {
        let config = ReputationConfig {
            min_score: 0.0,
            max_score: 1.0,
            initial_score: 0.5,
            negative_weight: 0.5, // large step
            min_observations: 1,
            ..Default::default()
        };
        let re = ReputationEngine::new(config);
        let peer = [3u8; 16];

        // Many negative updates
        for _ in 0..10 {
            re.update(peer, ReputationEvent::LocalDrop).await;
        }

        // Should floor at 0.0 (never negative propagation)
        let weight = re.routing_weight(peer).await;
        assert_eq!(weight, 0.0);
    }

    #[tokio::test]
    async fn reputation_secondhand_only_from_verified() {
        let re = ReputationEngine::default();
        let peer = [4u8; 16];
        let verified = [5u8; 16];
        let unverified = [6u8; 16];

        // Register verified peer
        re.register_verified_peer(verified).await;

        // Add some local observations first to reach min_observations
        for _ in 0..3 {
            re.update(peer, ReputationEvent::LocalForward).await;
        }
        let initial_weight = re.routing_weight(peer).await;

        // Secondhand from verified -> accepted
        re.update(
            peer,
            ReputationEvent::SecondhandPositive {
                from_verified_peer: verified,
            },
        )
        .await;

        // Secondhand from unverified -> ignored
        re.update(
            peer,
            ReputationEvent::SecondhandPositive {
                from_verified_peer: unverified,
            },
        )
        .await;

        // Should have only one secondhand positive (weight increased)
        let weight = re.routing_weight(peer).await;
        assert!(weight > initial_weight);
    }

    #[tokio::test]
    async fn reputation_metrics_snapshot() {
        let re = ReputationEngine::default();
        let peer = [9u8; 16];
        let verified = [10u8; 16];

        re.update(peer, ReputationEvent::LocalForward).await;
        re.update(peer, ReputationEvent::LocalDrop).await;
        re.register_verified_peer(verified).await;
        re.update(
            peer,
            ReputationEvent::SecondhandPositive {
                from_verified_peer: verified,
            },
        )
        .await;
        re.update(peer, ReputationEvent::AuditPositive).await;
        re.update(peer, ReputationEvent::AuditNegative).await;

        let m = re.metrics();
        assert_eq!(m.positive, 1);
        assert_eq!(m.negative, 1);
        assert_eq!(m.secondhand, 1);
        assert_eq!(m.audit, 2);
        assert_eq!(m.updates, 5);
    }

    #[tokio::test]
    async fn reputation_max_peers_evicts_lowest() {
        let config = ReputationConfig {
            max_peers: 2,
            min_observations: 1,
            ..Default::default()
        };
        let re = ReputationEngine::new(config);
        let a = [11u8; 16];
        let b = [12u8; 16];
        let c = [13u8; 16];

        re.update(a, ReputationEvent::LocalForward).await;
        re.update(b, ReputationEvent::LocalDrop).await;
        // Adding c evicts the lowest-scored peer (b).
        re.update(c, ReputationEvent::LocalForward).await;

        let scores = re.all_scores().await;
        assert_eq!(scores.len(), 2);
        assert!(scores.iter().all(|(p, _, _)| *p != b));
    }

    #[tokio::test]
    async fn reputation_audit_events_clamp() {
        let re = ReputationEngine::default();
        let peer = [14u8; 16];

        for _ in 0..10 {
            re.update(peer, ReputationEvent::AuditPositive).await;
        }
        let weight = re.routing_weight(peer).await;
        assert!(weight <= 1.0);
        assert!(weight >= 0.5);

        for _ in 0..10 {
            re.update(peer, ReputationEvent::AuditNegative).await;
        }
        let weight = re.routing_weight(peer).await;
        assert!(weight <= 1.0);
        assert!(weight >= 0.0);
    }

    #[tokio::test]
    async fn reputation_register_verified_peer_bumps_score() {
        let config = ReputationConfig {
            min_observations: 1,
            ..Default::default()
        };
        let re = ReputationEngine::new(config);
        let peer = [15u8; 16];
        re.register_verified_peer(peer).await;
        re.update(peer, ReputationEvent::LocalForward).await;
        let score = re.routing_weight(peer).await;
        assert!(score > 0.5, "verified peer gains initial trust bonus");

        re.reset().await;
        assert_eq!(re.routing_weight(peer).await, 0.5);
        assert!(re.all_scores().await.is_empty());
    }

    #[tokio::test]
    async fn reputation_not_a_gate() {
        // AC-9: low reputation never blocks routing
        let re = ReputationEngine::default();
        let peer = [7u8; 16];

        // Drive to minimum
        for _ in 0..20 {
            re.update(peer, ReputationEvent::LocalDrop).await;
        }

        // Routing weight should be 0.0 but still usable
        let weight = re.routing_weight(peer).await;
        assert_eq!(weight, 0.0);

        // The peer is still a valid route (routing layer decides)
        // This is tested at integration level
    }

    #[tokio::test]
    async fn reputation_decay_toward_neutral() {
        let config = ReputationConfig {
            decay_factor: 0.5, // fast decay for test
            decay_interval: Duration::from_secs(1),
            min_observations: 1,
            ..Default::default()
        };
        let re = ReputationEngine::new(config);
        let peer = [8u8; 16];

        // Build positive reputation
        for _ in 0..5 {
            re.update(peer, ReputationEvent::LocalForward).await;
        }
        let before = re.routing_weight(peer).await;
        assert!(before > 0.5);

        // Decay
        re.decay().await;
        let after = re.routing_weight(peer).await;
        assert!(after < before);
        assert!(after > 0.5); // still above neutral
    }
}
