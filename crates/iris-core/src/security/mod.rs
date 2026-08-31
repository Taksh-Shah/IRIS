//! SecurityPolicy facade — per-message-class hardening across P0-P7.
//! Noop default preserves un-armed engine behavior (AC-12).
//! Module layout: security/{rate_limiter,quota,replay,reputation,spam,acl}.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::message::MessagePriority;
use crate::protocol::Envelope;
use async_trait::async_trait;

pub mod acl;
pub mod quota;
pub mod rate_limiter;
pub mod replay;
pub mod reputation;
mod sharded;
pub mod spam;

use crate::identity::TrustStore;

pub use acl::{AclDecision, AlertClass, EmergencyAcl};
pub use quota::{QuotaDecision, QuotaManager};
pub use rate_limiter::{EmergencyClaim, MessageClass, RateLimitDecision, RateLimiter};
pub use replay::{HighWaterMark, ReplayDecision, ReplayEngine, ReplaySnapshot};
pub use reputation::{ReputationEngine, ReputationEvent};
pub use spam::{SpamDecision, SpamEngine};

/// 16-byte sender short ID.
pub type SenderShort = [u8; 16];
/// 16-byte peer short ID.
pub type PeerShort = [u8; 16];

/// Security policy trait — defines the seams the message engine calls.
/// Noop implementation provides default-permissive behavior (AC-12).
#[async_trait]
pub trait SecurityPolicy: Send + Sync {
    /// Check rate limit for outbound/inbound message.
    /// Return Allow / SilentDrop / Exempt (P0/P1).
    async fn rate_limit(&self, sender: SenderShort, class: MessageClass) -> RateLimitDecision {
        let _ = (sender, class);
        RateLimitDecision::Allowed
    }

    /// Rate-limit check that also weighs whether the payload justifies the
    /// P0/P1 exemption. Prefer this over [`SecurityPolicy::rate_limit`] on any
    /// path where the content type is known.
    async fn rate_limit_with_claim(
        &self,
        sender: SenderShort,
        class: MessageClass,
        claim: EmergencyClaim,
    ) -> RateLimitDecision {
        let _ = claim;
        self.rate_limit(sender, class).await
    }

    /// Check rate limit for unknown sender (aggregate).
    async fn rate_limit_unknown(&self, class: MessageClass) -> RateLimitDecision {
        let _ = class;
        RateLimitDecision::Allowed
    }

    /// Check storage quota for a new message.
    async fn check_quota(
        &self,
        sender: SenderShort,
        size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        let _ = (sender, size, priority);
        QuotaDecision::Accepted
    }

    /// Record message added to storage.
    async fn add_message(
        &self,
        sender: SenderShort,
        size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        let _ = (sender, size, priority);
        QuotaDecision::Accepted
    }

    /// Record message removed from storage.
    async fn remove_message(&self, sender: SenderShort, size: u64) {
        let _ = (sender, size);
    }

    /// Check replay protection (freshness + high-water).
    /// Returns Accepted / TooOld / TooFuture / Replay.
    async fn check_replay(&self, sender: SenderShort, ts: u64, seq: u64) -> ReplayDecision {
        let _ = (sender, ts, seq);
        ReplayDecision::Accepted
    }

    /// Check freshness only (for untrusted senders).
    async fn check_freshness(&self, ts: u64) -> ReplayDecision {
        let _ = ts;
        ReplayDecision::Accepted
    }

    /// Update reputation based on event.
    async fn update_reputation(&self, peer: PeerShort, event: ReputationEvent) {
        let _ = (peer, event);
    }

    /// Get routing weight for a peer (0.0-1.0).
    async fn routing_weight(&self, peer: PeerShort) -> f64 {
        let _ = peer;
        0.5
    }

    /// Score message for spam (receiver-side only).
    /// Returns Clean / LikelySpam / Exempt (P0-P3).
    async fn score_spam(&self, sender: SenderShort, envelope: &Envelope) -> SpamDecision {
        let _ = (sender, envelope);
        SpamDecision::Clean
    }

    /// Check emergency ACL authorization.
    async fn check_emergency_acl(&self, envelope: &Envelope, now_unix: u64) -> AclDecision {
        let _ = (envelope, now_unix);
        AclDecision::Authorized
    }

    /// Generate replay snapshot for cross-reboot persistence (AC-5).
    async fn generate_replay_snapshot(&self) -> Option<ReplaySnapshot> {
        None
    }

    /// Load replay snapshot (cross-reboot restore).
    async fn load_replay_snapshot(&self, _snapshot: ReplaySnapshot) {}

    /// PRY-4: tick reputation decay toward neutral. The engine GC loop calls
    /// this every sweep; the implementation self-throttles to the configured
    /// `decay_interval`. Default = no-op (Noop policy has no reputation state).
    async fn decay_reputation(&self) {}

    /// Whether this policy is armed (non-Noop).
    fn armed(&self) -> bool {
        false
    }
}

/// Noop security policy — default permissive behavior identical to un-armed engine.
/// This is AC-12: engine un-armed behavior identical (Noop SecurityPolicy == current behavior).
#[derive(Default)]
pub struct NoopSecurityPolicy;

#[async_trait]
impl SecurityPolicy for NoopSecurityPolicy {}

/// Full security policy implementation — wires all sub-modules.
pub struct FullSecurityPolicy {
    rate_limiter: Arc<RateLimiter>,
    quota: Arc<QuotaManager>,
    replay: Arc<ReplayEngine>,
    reputation: Arc<ReputationEngine>,
    spam: Arc<SpamEngine>,
    acl: Arc<EmergencyAcl>,
    armed: AtomicBool,
    /// PRY-4: last time `decay_reputation` actually ran a decay tick — used to
    /// throttle the GC-driven calls to the configured `decay_interval`.
    last_decay: Mutex<Instant>,
}

impl FullSecurityPolicy {
    /// Create a new full security policy with default configurations.
    pub fn new(trust_store: Arc<TrustStore>) -> Self {
        let rate_limiter = Arc::new(RateLimiter::default());
        let quota = Arc::new(QuotaManager::default());
        let replay = Arc::new(ReplayEngine::default());
        let reputation = Arc::new(ReputationEngine::default());
        let spam = Arc::new(SpamEngine::default());
        let acl = Arc::new(EmergencyAcl::default(trust_store));

        FullSecurityPolicy {
            rate_limiter,
            quota,
            replay,
            reputation,
            spam,
            acl,
            armed: AtomicBool::new(true),
            last_decay: Mutex::new(Instant::now()),
        }
    }

    /// Create with custom configurations.
    pub fn with_configs(
        rate_limiter: RateLimiter,
        quota: QuotaManager,
        replay: ReplayEngine,
        reputation: ReputationEngine,
        spam: SpamEngine,
        acl: EmergencyAcl,
    ) -> Self {
        FullSecurityPolicy {
            rate_limiter: Arc::new(rate_limiter),
            quota: Arc::new(quota),
            replay: Arc::new(replay),
            reputation: Arc::new(reputation),
            spam: Arc::new(spam),
            acl: Arc::new(acl),
            armed: AtomicBool::new(true),
            last_decay: Mutex::new(Instant::now()),
        }
    }

    /// Get rate limiter for metrics/monitoring.
    pub fn rate_limiter(&self) -> &Arc<RateLimiter> {
        &self.rate_limiter
    }

    /// Get quota manager.
    pub fn quota(&self) -> &Arc<QuotaManager> {
        &self.quota
    }

    /// Get replay engine.
    pub fn replay(&self) -> &Arc<ReplayEngine> {
        &self.replay
    }

    /// Get reputation engine.
    pub fn reputation(&self) -> &Arc<ReputationEngine> {
        &self.reputation
    }

    /// Get spam engine.
    pub fn spam(&self) -> &Arc<SpamEngine> {
        &self.spam
    }

    /// Get ACL.
    pub fn acl(&self) -> &Arc<EmergencyAcl> {
        &self.acl
    }

    /// Disarm the policy (revert to Noop behavior).
    pub fn disarm(&self) {
        self.armed.store(false, Ordering::Relaxed);
    }

    /// Arm the policy.
    pub fn arm(&self) {
        self.armed.store(true, Ordering::Relaxed);
    }
}

#[async_trait]
impl SecurityPolicy for FullSecurityPolicy {
    async fn rate_limit(&self, sender: SenderShort, class: MessageClass) -> RateLimitDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return RateLimitDecision::Allowed;
        }
        self.rate_limiter.check(sender, class).await
    }

    async fn rate_limit_with_claim(
        &self,
        sender: SenderShort,
        class: MessageClass,
        claim: EmergencyClaim,
    ) -> RateLimitDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return RateLimitDecision::Allowed;
        }
        self.rate_limiter
            .check_with_claim(sender, class, claim)
            .await
    }

    async fn rate_limit_unknown(&self, class: MessageClass) -> RateLimitDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return RateLimitDecision::Allowed;
        }
        self.rate_limiter.check_unknown(class).await
    }

    async fn check_quota(
        &self,
        sender: SenderShort,
        size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return QuotaDecision::Accepted;
        }
        self.quota.check(sender, size, priority).await
    }

    async fn add_message(
        &self,
        sender: SenderShort,
        size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return QuotaDecision::Accepted;
        }
        self.quota.add_message(sender, size, priority).await
    }

    async fn remove_message(&self, sender: SenderShort, size: u64) {
        if self.armed.load(Ordering::Relaxed) {
            self.quota.remove_message(sender, size).await;
        }
    }

    async fn check_replay(&self, sender: SenderShort, ts: u64, seq: u64) -> ReplayDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return ReplayDecision::Accepted;
        }
        self.replay.check(sender, ts, seq).await
    }

    async fn check_freshness(&self, ts: u64) -> ReplayDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return ReplayDecision::Accepted;
        }
        self.replay.check_freshness_only(ts).await
    }

    async fn update_reputation(&self, peer: PeerShort, event: ReputationEvent) {
        if self.armed.load(Ordering::Relaxed) {
            self.reputation.update(peer, event).await;
        }
    }

    async fn routing_weight(&self, peer: PeerShort) -> f64 {
        if !self.armed.load(Ordering::Relaxed) {
            return 0.5;
        }
        self.reputation.routing_weight(peer).await
    }

    async fn score_spam(&self, sender: SenderShort, envelope: &Envelope) -> SpamDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return SpamDecision::Clean;
        }
        self.spam.score(sender, envelope).await
    }

    async fn check_emergency_acl(&self, envelope: &Envelope, now_unix: u64) -> AclDecision {
        if !self.armed.load(Ordering::Relaxed) {
            return AclDecision::Authorized;
        }
        self.acl.check(envelope, now_unix).await
    }

    async fn generate_replay_snapshot(&self) -> Option<ReplaySnapshot> {
        if !self.armed.load(Ordering::Relaxed) {
            return None;
        }
        Some(self.replay.generate_snapshot().await)
    }

    async fn load_replay_snapshot(&self, snapshot: ReplaySnapshot) {
        if self.armed.load(Ordering::Relaxed) {
            self.replay.load_snapshot(snapshot).await;
        }
    }

    async fn decay_reputation(&self) {
        if !self.armed.load(Ordering::Relaxed) {
            return;
        }
        // PRY-4: self-throttle to the configured cadence so a fast GC interval
        // does not over-decay. A peer floored by ~5 LocalDrop events had
        // routing_weight stuck at 0.0 forever because nothing called decay().
        {
            let mut last = self.last_decay.lock().unwrap();
            if last.elapsed() < self.reputation.decay_interval() {
                return;
            }
            *last = Instant::now();
        }
        self.reputation.decay().await;
    }

    fn armed(&self) -> bool {
        self.armed.load(Ordering::Relaxed)
    }
}

/// Security policy configuration for engine construction.
#[derive(Clone, Debug, Default)]
pub struct SecurityPolicyConfig {
    pub rate_limiter: crate::security::rate_limiter::RateLimiterConfig,
    pub quota: crate::security::quota::QuotaConfig,
    pub replay: crate::security::replay::ReplayConfig,
    pub reputation: crate::security::reputation::ReputationConfig,
    pub spam: crate::security::spam::SpamConfig,
    pub acl: crate::security::acl::AclConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::TrustStore;
    use std::sync::Arc;

    #[test]
    fn noop_policy_is_disarmed() {
        let policy = NoopSecurityPolicy;
        assert!(!policy.armed());
    }

    #[test]
    fn full_policy_arms_by_default() {
        let trust_store = Arc::new(TrustStore::new());
        let policy = FullSecurityPolicy::new(trust_store);
        assert!(policy.armed());
    }

    #[test]
    fn full_policy_can_disarm() {
        let trust_store = Arc::new(TrustStore::new());
        let policy = FullSecurityPolicy::new(trust_store);
        policy.disarm();
        assert!(!policy.armed());
        policy.arm();
        assert!(policy.armed());
    }

    #[test]
    fn full_policy_with_configs_and_accessors() {
        let rl = RateLimiter::default();
        let q = QuotaManager::default();
        let rp = ReplayEngine::default();
        let rep = ReputationEngine::default();
        let sp = SpamEngine::default();
        let acl = EmergencyAcl::default(Arc::new(TrustStore::new()));
        let policy = FullSecurityPolicy::with_configs(rl, q, rp, rep, sp, acl);
        assert!(policy.armed());
        assert!(Arc::ptr_eq(policy.rate_limiter(), policy.rate_limiter()));
        assert!(Arc::ptr_eq(policy.quota(), policy.quota()));
        assert!(Arc::ptr_eq(policy.replay(), policy.replay()));
        assert!(Arc::ptr_eq(policy.reputation(), policy.reputation()));
        assert!(Arc::ptr_eq(policy.spam(), policy.spam()));
        assert!(Arc::ptr_eq(policy.acl(), policy.acl()));
    }

    #[tokio::test]
    async fn full_policy_armed_routes_to_engines() {
        let trust_store = Arc::new(TrustStore::new());
        let policy = FullSecurityPolicy::new(trust_store);
        let sender = [8u8; 16];
        let peer = [9u8; 16];
        let env = test_envelope(MessagePriority::P4);
        let now = crate::message_engine::expiry::unix_now();

        assert_eq!(
            policy.rate_limit(sender, MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            policy.rate_limit_unknown(MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            policy.check_quota(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        assert_eq!(
            policy.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        policy.remove_message(sender, 1024).await;
        assert_eq!(
            policy.check_replay(sender, now, 1).await,
            ReplayDecision::Accepted
        );
        assert_eq!(policy.check_freshness(now).await, ReplayDecision::Accepted);
        policy
            .update_reputation(peer, ReputationEvent::LocalForward)
            .await;
        assert_eq!(policy.routing_weight(peer).await, 0.5);
        assert_eq!(policy.score_spam(sender, &env).await, SpamDecision::Clean);
        // PRY-10: an armed policy's SOS path now requires a valid envelope
        // signature; this unsigned test envelope is rejected. (PRY-1 similarly
        // tightened the Broadcast/Medical empty-allowlist path.)
        assert_eq!(
            policy.check_emergency_acl(&env, now).await,
            AclDecision::InvalidAuthority
        );
        let snapshot = policy.generate_replay_snapshot().await.expect("snapshot");
        assert!(policy.armed());
        policy.load_replay_snapshot(snapshot).await;
    }

    #[tokio::test]
    async fn full_policy_disarmed_short_circuits() {
        let trust_store = Arc::new(TrustStore::new());
        let policy = FullSecurityPolicy::new(trust_store);
        policy.disarm();
        let sender = [8u8; 16];
        let peer = [9u8; 16];
        let env = test_envelope(MessagePriority::P4);

        assert_eq!(
            policy.rate_limit(sender, MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            policy.rate_limit_unknown(MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            policy.check_quota(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        assert_eq!(
            policy.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        policy.remove_message(sender, 1024).await;
        assert_eq!(
            policy.check_replay(sender, 0, 0).await,
            ReplayDecision::Accepted
        );
        assert_eq!(policy.check_freshness(0).await, ReplayDecision::Accepted);
        policy
            .update_reputation(peer, ReputationEvent::LocalForward)
            .await;
        assert_eq!(policy.routing_weight(peer).await, 0.5);
        assert_eq!(policy.score_spam(sender, &env).await, SpamDecision::Clean);
        assert_eq!(
            policy.check_emergency_acl(&env, 0).await,
            AclDecision::Authorized
        );
        assert!(policy.generate_replay_snapshot().await.is_none());
        policy.load_replay_snapshot(ReplaySnapshot::default()).await;
    }

    #[tokio::test]
    async fn noop_all_default_methods_are_permissive() {
        let policy = NoopSecurityPolicy;
        let sender = [4u8; 16];
        let peer = [5u8; 16];
        let env = test_envelope(MessagePriority::P4);

        assert_eq!(
            policy.rate_limit_unknown(MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(policy.check_freshness(0).await, ReplayDecision::Accepted);
        policy
            .update_reputation(peer, ReputationEvent::LocalForward)
            .await;
        assert_eq!(policy.routing_weight(peer).await, 0.5);
        assert_eq!(
            policy.check_emergency_acl(&env, 0).await,
            AclDecision::Authorized
        );
        assert!(policy.generate_replay_snapshot().await.is_none());
        policy.load_replay_snapshot(ReplaySnapshot::default()).await;
        assert_eq!(
            policy.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        policy.remove_message(sender, 1024).await;
        let _ = sender;
    }

    fn test_envelope(priority: MessagePriority) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![2u8; 32],
            priority,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: crate::protocol::ContentType::Text,
            payload_size: 100,
            payload_hash: [0; 32],
            payload: vec![0u8; 100],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    #[tokio::test]
    async fn noop_always_allows() {
        let policy = NoopSecurityPolicy;
        assert_eq!(
            policy.rate_limit([0; 16], MessageClass::P4).await,
            RateLimitDecision::Allowed
        );
        assert_eq!(
            policy.check_quota([0; 16], 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        assert_eq!(
            policy.check_replay([0; 16], 0, 0).await,
            ReplayDecision::Accepted
        );
        let test_env = Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![2u8; 32],
            priority: MessagePriority::P4,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: crate::protocol::ContentType::Text,
            payload_size: 100,
            payload_hash: [0; 32],
            payload: vec![0u8; 100],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        assert_eq!(
            policy.score_spam([0; 16], &test_env).await,
            SpamDecision::Clean
        );
    }
}
