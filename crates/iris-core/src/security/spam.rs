//! Spam scoring: receiver-side "likely_spam" annotation; NEVER relay hard-drop.
//! P0-P3 disjoint from spam scoring. RES-0018 Q4.2, Cormack CIKM 2007 FP risk.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::message::MessagePriority;
use crate::protocol::Envelope;
use tokio::sync::RwLock;

/// 16-byte sender short ID.
pub type SenderShort = [u8; 16];

/// Spam scoring configuration.
#[derive(Clone, Debug)]
pub struct SpamConfig {
    /// Minimum payload size for spam scoring (512 bytes).
    pub min_payload_for_scoring: usize,
    /// Maximum spam score (1.0 = definitely spam).
    pub max_score: f64,
    /// Threshold for "likely_spam" annotation.
    pub spam_threshold: f64,
    /// Bayesian prior (probability message is spam).
    pub prior_spam_prob: f64,
    /// Feature weights (simplified for v1).
    pub unknown_sender_penalty: f64,
    pub high_volume_penalty: f64,
    pub duplicate_content_penalty: f64,
    /// Max per-sender stats entries (SEC-RT-06: prevents unbounded memory
    /// growth from spoofed/high-entropy sender shorts).
    pub max_sender_stats: usize,
}

impl Default for SpamConfig {
    fn default() -> Self {
        SpamConfig {
            min_payload_for_scoring: 512,
            max_score: 1.0,
            spam_threshold: 0.7,
            prior_spam_prob: 0.1,
            unknown_sender_penalty: 0.3,
            high_volume_penalty: 0.2,
            duplicate_content_penalty: 0.15,
            max_sender_stats: 100_000,
        }
    }
}

/// Spam scoring result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpamDecision {
    /// Not spam (score below threshold).
    Clean,
    /// Likely spam (annotated for UI, NOT relay-dropped).
    LikelySpam,
    /// P0-P3: never spam-scored (disjoint classes).
    Exempt,
}

/// Per-sender spam statistics.
///
/// PRY-16: `last_content_hash` compared only the immediately-preceding message
/// and is dead weight under E2EE — a fresh per-message ephemeral X25519 key
/// makes identical plaintext hash differently every time. We keep a short ring
/// of recent hashes (still useful for the local/unencrypted case) and add a
/// **content-agnostic** signal — repeated payload *size* — which survives
/// encryption because AEAD overhead is fixed, so equal plaintext length ⇒
/// equal ciphertext length.
#[derive(Clone, Debug, Default)]
struct SenderSpamStats {
    message_count: u64,
    spam_hits: u64,
    recent_hashes: std::collections::VecDeque<[u8; 32]>,
    /// (payload size, recipient short) for the last N messages — the
    /// content-agnostic fan-out signal.
    recent_size_dst: std::collections::VecDeque<(usize, [u8; 8])>,
}

impl SenderSpamStats {
    const RECENT_HASHES_CAP: usize = 8;
    const RECENT_SIZE_DST_CAP: usize = 16;
}

/// Spam scoring engine (receiver-side only).
pub struct SpamEngine {
    stats: RwLock<HashMap<SenderShort, SenderSpamStats>>,
    config: SpamConfig,
    metrics: SpamMetrics,
}

#[derive(Default, Debug)]
pub struct SpamMetrics {
    pub scored: AtomicU64,
    pub clean: AtomicU64,
    pub likely_spam: AtomicU64,
    pub exempt: AtomicU64,
}

impl SpamEngine {
    /// Create a new spam engine with default configuration.
    pub fn new(config: SpamConfig) -> Self {
        SpamEngine {
            stats: RwLock::new(HashMap::with_capacity(1024)),
            config,
            metrics: SpamMetrics::default(),
        }
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> SpamMetricsSnapshot {
        SpamMetricsSnapshot {
            scored: self.metrics.scored.load(Ordering::Relaxed),
            clean: self.metrics.clean.load(Ordering::Relaxed),
            likely_spam: self.metrics.likely_spam.load(Ordering::Relaxed),
            exempt: self.metrics.exempt.load(Ordering::Relaxed),
        }
    }

    /// Score an inbound message for spam.
    /// Returns SpamDecision (Clean / LikelySpam / Exempt for P0-P3).
    /// NEVER drops — only annotates for UI.
    pub async fn score(&self, sender: SenderShort, envelope: &Envelope) -> SpamDecision {
        // P0-P3 never spam-scored (classes disjoint)
        if matches!(
            envelope.priority,
            MessagePriority::P0 | MessagePriority::P1 | MessagePriority::P2 | MessagePriority::P3
        ) {
            self.metrics.exempt.fetch_add(1, Ordering::Relaxed);
            return SpamDecision::Exempt;
        }

        // Minimum payload size for scoring
        if envelope.payload.len() < self.config.min_payload_for_scoring {
            self.metrics.scored.fetch_add(1, Ordering::Relaxed);
            self.metrics.clean.fetch_add(1, Ordering::Relaxed);
            return SpamDecision::Clean;
        }

        let mut stats = self.stats.write().await;
        // SEC-RT-06: bound the per-sender map so unbounded sender-spoofing
        // cannot grow memory without limit.
        // PRY-21: evict the least-established sender (lowest `message_count`)
        // rather than `HashMap::keys().next()` — arbitrary hash order is not
        // adversary-resistant (the exact pattern `replay.rs` was hardened away
        // from). A spoofed one-shot sender has count 1 and is evicted first;
        // genuine established senders are protected.
        if stats.len() >= self.config.max_sender_stats && !stats.contains_key(&sender) {
            if let Some(k) = stats
                .iter()
                .min_by_key(|(_, s)| s.message_count)
                .map(|(k, _)| *k)
            {
                stats.remove(&k);
            }
        }
        let stat = stats.entry(sender).or_insert_with(SenderSpamStats::default);

        stat.message_count += 1;

        // Simple heuristic scoring (v1 - can be enhanced with ML later)
        let mut score = self.config.prior_spam_prob;

        // Unknown sender penalty
        if stat.message_count == 1 {
            score += self.config.unknown_sender_penalty;
        }

        // High volume penalty
        if stat.message_count > 100 {
            score += self.config.high_volume_penalty;
        }

        // Duplicate-content detection.
        // (a) Exact-hash repeat against a short ring — catches plaintext floods
        //     including A,B,A,B. Inert on E2EE ciphertext (see SenderSpamStats).
        let content_hash = compute_content_hash(&envelope.payload);
        if stat.recent_hashes.contains(&content_hash) {
            score += self.config.duplicate_content_penalty;
        }
        stat.recent_hashes.push_back(content_hash);
        while stat.recent_hashes.len() > SenderSpamStats::RECENT_HASHES_CAP {
            stat.recent_hashes.pop_front();
        }
        // (b) Content-agnostic fan-out signal: many recent messages of the
        //     *same size* sent to *different* recipients (ABUSE_PREVENTION.md
        //     §216). Survives encryption (fixed AEAD overhead ⇒ equal plaintext
        //     length ⇒ equal ciphertext length). Requires both a size-majority
        //     AND recipient diversity, so a burst of retransmits to one peer
        //     (legitimate) does not trip it.
        let mut dst = [0u8; 8];
        let n = envelope.recipient_id.len().min(8);
        dst[..n].copy_from_slice(&envelope.recipient_id[..n]);
        if stat.recent_size_dst.len() >= SenderSpamStats::RECENT_SIZE_DST_CAP {
            let this_size = envelope.payload.len();
            let same_size = stat
                .recent_size_dst
                .iter()
                .filter(|(s, _)| *s == this_size)
                .count();
            let distinct_dsts: std::collections::HashSet<_> =
                stat.recent_size_dst.iter().map(|(_, d)| *d).collect();
            if same_size * 4 >= stat.recent_size_dst.len() * 3 && distinct_dsts.len() >= 4 {
                score += self.config.duplicate_content_penalty;
            }
        }
        stat.recent_size_dst.push_back((envelope.payload.len(), dst));
        while stat.recent_size_dst.len() > SenderSpamStats::RECENT_SIZE_DST_CAP {
            stat.recent_size_dst.pop_front();
        }

        self.metrics.scored.fetch_add(1, Ordering::Relaxed);

        if score >= self.config.spam_threshold {
            stat.spam_hits += 1;
            self.metrics.likely_spam.fetch_add(1, Ordering::Relaxed);
            SpamDecision::LikelySpam
        } else {
            self.metrics.clean.fetch_add(1, Ordering::Relaxed);
            SpamDecision::Clean
        }
    }

    /// Get per-sender spam stats for monitoring.
    pub async fn sender_stats(&self, sender: SenderShort) -> Option<(u64, u64)> {
        let stats = self.stats.read().await;
        stats.get(&sender).map(|s| (s.message_count, s.spam_hits))
    }

    /// Reset all state (for testing).
    pub async fn reset(&self) {
        self.stats.write().await.clear();
    }
}

impl Default for SpamEngine {
    fn default() -> Self {
        Self::new(SpamConfig::default())
    }
}

#[cfg(all(test, feature = "proptest"))]
mod proptest_tests {
    use super::*;
    use crate::message::MessagePriority;
    use crate::protocol::{ContentType, MessageId};
    use proptest::prelude::*;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(fut)
    }

    fn test_envelope(priority: MessagePriority, payload: &[u8]) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![2u8; 32],
            priority,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Text,
            payload_size: payload.len() as u64,
            payload_hash: [0; 32],
            payload: payload.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    proptest! {
        #[test]
        fn spam_never_panics(
            sender in any::<[u8; 16]>(),
            priority in any::<MessagePriority>(),
            payload in prop::collection::vec(any::<u8>(), 0..5000),
        ) {
            let se = SpamEngine::default();
            let env = test_envelope(priority, &payload);
            let _ = block_on(se.score(sender, &env));
        }

        #[test]
        fn spam_p0_p3_exempt(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let se = SpamEngine::default();
            for p in [MessagePriority::P0, MessagePriority::P1, MessagePriority::P2, MessagePriority::P3] {
                let env = test_envelope(p, &payload);
                prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::Exempt);
            }
        }

        #[test]
        fn spam_unknown_sender_penalty(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let config = SpamConfig {
                min_payload_for_scoring: 100,
                spam_threshold: 0.5,
                prior_spam_prob: 0.1,
                unknown_sender_penalty: 0.5,
                ..Default::default()
            };
            let se = SpamEngine::new(config);
            let env = test_envelope(MessagePriority::P4, &payload);
            // First message from unknown sender -> likely spam due to penalty
            prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::LikelySpam);
        }

        #[test]
        fn spam_duplicate_content_penalty(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let config = SpamConfig {
                min_payload_for_scoring: 100,
                spam_threshold: 0.5,
                prior_spam_prob: 0.1,
                unknown_sender_penalty: 0.5,
                duplicate_content_penalty: 0.5,
                ..Default::default()
            };
            let se = SpamEngine::new(config);
            let env = test_envelope(MessagePriority::P4, &payload);

            // First message: unknown sender penalty -> likely spam
            prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::LikelySpam);

            // Second message: same content -> duplicate penalty -> likely spam
            prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::LikelySpam);
        }

        #[test]
        fn spam_high_volume_penalty(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let config = SpamConfig {
                min_payload_for_scoring: 100,
                spam_threshold: 0.5,
                prior_spam_prob: 0.1,
                high_volume_penalty: 0.5,
                ..Default::default()
            };
            let se = SpamEngine::new(config);
            let env = test_envelope(MessagePriority::P4, &payload);

            // Send many messages
            for _ in 0..150 {
                let _ = block_on(se.score(sender, &env));
            }

            // Should now be likely spam due to high volume
            let env2 = test_envelope(MessagePriority::P4, &payload);
            prop_assert_eq!(block_on(se.score(sender, &env2)), SpamDecision::LikelySpam);
        }

        #[test]
        fn spam_never_drops_only_annotates(
            sender in any::<[u8; 16]>(),
            priority in any::<MessagePriority>(),
            payload in prop::collection::vec(any::<u8>(), 0..5000),
        ) {
            let se = SpamEngine::default();
            let env = test_envelope(priority, &payload);

            let decision = block_on(se.score(sender, &env));
            prop_assert!(matches!(
                decision,
                SpamDecision::Clean | SpamDecision::LikelySpam | SpamDecision::Exempt
            ));
        }

        #[test]
        fn spam_p0_p3_never_scored(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let se = SpamEngine::default();
            for p in [MessagePriority::P0, MessagePriority::P1, MessagePriority::P2, MessagePriority::P3] {
                let env = test_envelope(p, &payload);
                prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::Exempt);
            }
        }

        #[test]
        fn spam_clean_by_default(
            sender in any::<[u8; 16]>(),
            payload in prop::collection::vec(any::<u8>(), 512..5000),
        ) {
            let se = SpamEngine::default();
            let env = test_envelope(MessagePriority::P4, &payload);
            // Default config: threshold 0.7, prior 0.1 -> clean
            prop_assert_eq!(block_on(se.score(sender, &env)), SpamDecision::Clean);
        }
    }
}

/// Compute a simple content hash for duplicate detection.
fn compute_content_hash(payload: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(payload);
    hasher.finalize().into()
}

#[derive(Clone, Copy, Debug)]
pub struct SpamMetricsSnapshot {
    pub scored: u64,
    pub clean: u64,
    pub likely_spam: u64,
    pub exempt: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentType, Envelope, MessageId, MessagePriority};

    fn test_envelope(priority: MessagePriority, payload: &[u8]) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![2u8; 32],
            priority,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Text,
            payload_size: payload.len() as u64,
            payload_hash: [0; 32],
            payload: payload.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    #[tokio::test]
    async fn spam_p0_p3_exempt() {
        let se = SpamEngine::default();
        let sender = [1u8; 16];

        for p in [
            MessagePriority::P0,
            MessagePriority::P1,
            MessagePriority::P2,
            MessagePriority::P3,
        ] {
            let env = test_envelope(p, b"x".repeat(1000).as_slice());
            assert_eq!(se.score(sender, &env).await, SpamDecision::Exempt);
        }
    }

    #[tokio::test]
    async fn spam_small_payload_clean() {
        let se = SpamEngine::default();
        let sender = [2u8; 16];
        let env = test_envelope(MessagePriority::P4, b"small");
        assert_eq!(se.score(sender, &env).await, SpamDecision::Clean);
    }

    #[tokio::test]
    async fn spam_unknown_sender_penalty() {
        let config = SpamConfig {
            min_payload_for_scoring: 100,
            spam_threshold: 0.5,
            prior_spam_prob: 0.1,
            unknown_sender_penalty: 0.5, // large to trigger
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let sender = [3u8; 16];
        let env = test_envelope(MessagePriority::P4, b"x".repeat(200).as_slice());

        // First message from unknown sender -> likely spam due to penalty
        assert_eq!(se.score(sender, &env).await, SpamDecision::LikelySpam);
    }

    #[tokio::test]
    async fn spam_duplicate_content_penalty() {
        let config = SpamConfig {
            min_payload_for_scoring: 100,
            spam_threshold: 0.5,
            prior_spam_prob: 0.1,
            unknown_sender_penalty: 0.5,
            duplicate_content_penalty: 0.5,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let sender = [4u8; 16];
        let payload = b"x".repeat(200);
        let env = test_envelope(MessagePriority::P4, &payload);

        // First message: unknown sender penalty (0.5) -> likely spam
        assert_eq!(se.score(sender, &env).await, SpamDecision::LikelySpam);

        // Second message: same content -> duplicate penalty (0.5) -> likely spam
        assert_eq!(se.score(sender, &env).await, SpamDecision::LikelySpam);
    }

    /// PRY-16: the content-agnostic fan-out signal — same size to many distinct
    /// recipients — must flag even when payloads differ (the E2EE-ciphertext
    /// case). A same-size burst to ONE recipient must NOT trip it.
    #[tokio::test]
    async fn spam_same_size_fanout_flags_but_single_recipient_does_not() {
        let config = SpamConfig {
            min_payload_for_scoring: 10,
            spam_threshold: 0.5,
            prior_spam_prob: 0.1,
            unknown_sender_penalty: 0.0,
            high_volume_penalty: 0.0,
            duplicate_content_penalty: 0.5,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let blaster = [0xB1u8; 16];

        // 20 messages, each 100 bytes of DISTINCT random-ish content, each to a
        // different recipient — a classic broadcast-spam fan-out.
        let mut last = SpamDecision::Clean;
        for i in 0u8..20 {
            let mut e = test_envelope(MessagePriority::P4, &vec![i; 100]);
            e.recipient_id = vec![i; 32];
            last = se.score(blaster, &e).await;
        }
        assert_eq!(
            last, SpamDecision::LikelySpam,
            "same-size fan-out to many recipients must be flagged"
        );

        // Same size, distinct content, but all to ONE recipient (legit retransmit).
        let steady = [0x5Eu8; 16];
        let mut last2 = SpamDecision::Clean;
        for i in 0u8..20 {
            let mut e = test_envelope(MessagePriority::P4, &vec![i; 100]);
            e.recipient_id = vec![9u8; 32];
            last2 = se.score(steady, &e).await;
        }
        assert_eq!(
            last2, SpamDecision::Clean,
            "a same-size burst to a single recipient must not be flagged"
        );
    }

    #[tokio::test]
    async fn spam_high_volume_penalty() {
        let config = SpamConfig {
            min_payload_for_scoring: 100,
            spam_threshold: 0.5,
            prior_spam_prob: 0.1,
            high_volume_penalty: 0.5,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let sender = [5u8; 16];
        let env = test_envelope(MessagePriority::P4, b"x".repeat(200).as_slice());

        // Send many messages
        for _ in 0..150 {
            se.score(sender, &env).await;
        }

        // Should now be likely spam due to high volume
        let env2 = test_envelope(MessagePriority::P4, b"y".repeat(200).as_slice());
        assert_eq!(se.score(sender, &env2).await, SpamDecision::LikelySpam);
    }

    #[tokio::test]
    async fn spam_metrics_and_sender_stats() {
        let config = SpamConfig {
            min_payload_for_scoring: 100,
            spam_threshold: 0.7,
            prior_spam_prob: 0.1,
            unknown_sender_penalty: 0.3,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let sender = [7u8; 16];

        // P0 -> Exempt (never scored).
        assert_eq!(
            se.score(
                sender,
                &test_envelope(MessagePriority::P0, b"x".repeat(1000).as_slice())
            )
            .await,
            SpamDecision::Exempt
        );
        // Small payload -> Clean (recorded as scored+clean). score = 0.1 prior < 0.7.
        assert_eq!(
            se.score(
                sender,
                &test_envelope(MessagePriority::P4, "x".repeat(200).as_bytes())
            )
            .await,
            SpamDecision::Clean
        );

        let m = se.metrics();
        assert_eq!(m.scored, 1);
        assert_eq!(m.clean, 1);
        assert_eq!(m.exempt, 1);

        let stats = se.sender_stats(sender).await;
        assert_eq!(stats, Some((1, 0)));

        se.reset().await;
        assert_eq!(se.sender_stats(sender).await, None);
        assert_eq!(se.metrics().exempt, 1);
    }

    #[tokio::test]
    async fn spam_sender_stats_capped_at_max() {
        // SEC-RT-06: at max_sender_stats, a new sender evicts an old one.
        let config = SpamConfig {
            min_payload_for_scoring: 10,
            max_sender_stats: 2,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let payload = b"x".to_vec();
        let a = [9u8; 16];
        let b = [0x0A; 16];
        let c = [0x0B; 16];
        let env = |sender: SenderShort| -> Envelope {
            let mut e = test_envelope(MessagePriority::P4, &payload);
            e.sender_id = sender.to_vec();
            e
        };

        se.score(a, &env(a)).await;
        se.score(b, &env(b)).await;
        se.score(c, &env(c)).await;

        assert!(
            se.sender_stats(a).await.is_none()
                || se.sender_stats(b).await.is_none()
                || se.sender_stats(c).await.is_none()
        );
    }

    /// PRY-21: at the cap, eviction removes the least-established sender (lowest
    /// `message_count`), not an arbitrary `HashMap::keys().next()`. A spoofed
    /// one-shot sender is evicted before an established one.
    #[tokio::test]
    async fn spam_cap_evicts_least_established_sender() {
        let config = SpamConfig {
            min_payload_for_scoring: 1,
            max_sender_stats: 2,
            ..Default::default()
        };
        let se = SpamEngine::new(config);
        let payload = b"xx".to_vec();
        let established = [0xE1u8; 16];
        let one_shot = [0x01u8; 16];
        let newcomer = [0x0Cu8; 16];
        let env = |s: SenderShort| {
            let mut e = test_envelope(MessagePriority::P4, &payload);
            e.sender_id = s.to_vec();
            e
        };

        for _ in 0..8 {
            se.score(established, &env(established)).await;
        }
        se.score(one_shot, &env(one_shot)).await; // map now at cap (2)
        se.score(newcomer, &env(newcomer)).await; // triggers eviction

        assert!(
            se.sender_stats(established).await.is_some(),
            "established sender (count 8) must survive eviction"
        );
        assert!(
            se.sender_stats(one_shot).await.is_none(),
            "least-established sender (count 1) must be the one evicted"
        );
    }

    #[tokio::test]
    async fn spam_never_drops_only_annotates() {
        let se = SpamEngine::default();
        let sender = [6u8; 16];
        let env = test_envelope(MessagePriority::P4, b"x".repeat(1000).as_slice());

        // Score returns decision but NEVER drops
        let decision = se.score(sender, &env).await;
        // Only possible returns: Clean, LikelySpam, Exempt
        // None of these is "Drop"
        assert!(matches!(
            decision,
            SpamDecision::Clean | SpamDecision::LikelySpam | SpamDecision::Exempt
        ));
    }
}
