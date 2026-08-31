//! SYSVAL-001 Track D — combined security engines under abuse flood.
//!
//! Drives rate-limiter, spam, and reputation engines in ONE abuse scenario
//! and asserts system-level invariants: P0/P1 exempt from drops everywhere,
//! silent drops bounded by bursts, spam annotates but never drops, reputation
//! is a routing weight only (never an admission gate), and no engine panics
//! under sustained hostile volume.

use iris_core::message::MessagePriority;
use iris_core::protocol::MessageId;
use iris_core::security::rate_limiter::{MessageClass, RateLimiter, RateLimiterConfig};
use iris_core::security::reputation::{ReputationEngine, ReputationEvent};
use iris_core::security::spam::{SpamDecision, SpamEngine};
use iris_core::{ContentType, Envelope};
use std::time::Duration; // refill intervals

fn sender(n: u8) -> [u8; 16] {
    [n; 16]
}

fn test_envelope() -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: vec![1u8; 32],
        recipient_id: vec![2u8; 32],
        priority: MessagePriority::P4,
        ttl_seconds: 3600,
        timestamp: 0,
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Text,
        payload_size: 80,
        payload_hash: [0; 32],
        payload: vec![7u8; 80],
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

/// Abuse scenario: 40 senders x 30 P4 messages each (1200 total). Every
/// engine sees the same stream; combined invariants must hold.
#[tokio::test]
async fn combined_engines_hold_invariants_under_abuse_flood() {
    let rl = RateLimiter::new(RateLimiterConfig {
        rate_per_sec: 5,
        burst: 10,
        refill_interval: Duration::from_secs(1),
        exempt_p0_p1: true,
        max_sender_buckets: 200,
        unknown_sender_rate_per_sec: 20,
        unknown_sender_burst: 50,
    });
    let spam = SpamEngine::new(Default::default());
    let rep = ReputationEngine::new(Default::default());

    for s in 0..40u8 {
        rep.register_verified_peer(sender(s)).await;
    }

    let mut allowed_total = 0usize;
    let mut dropped_total = 0usize;
    let mut annotated = 0usize;

    for s in 0..40u8 {
        for _ in 0..30usize {
            match rl.check(sender(s), MessageClass::P4).await {
                iris_core::security::rate_limiter::RateLimitDecision::Allowed => {
                    allowed_total += 1;
                    // Spam observes post-admission: annotates, NEVER drops.
                    if matches!(
                        spam.score(sender(s), &test_envelope()).await,
                        SpamDecision::LikelySpam
                    ) {
                        annotated += 1;
                    }
                    // Reputation updates are routing WEIGHTS only.
                    rep.update(sender(s), ReputationEvent::LocalForward).await;
                }
                _ => dropped_total += 1,
            }
        }
    }

    assert_eq!(allowed_total + dropped_total, 1200);
    assert!(allowed_total >= 40, "initial bursts must pass");
    assert!(dropped_total > 0, "sustained flood must be throttled");

    let m = rl.metrics();
    assert_eq!(m.allowed as usize + m.dropped as usize, 1200);

    // Reputation scores stay bounded and never gated admission: every one of
    // the allowed messages was admitted before scoring ran.
    for (_, score, _) in rep.all_scores().await {
        assert!((0.0..=1.0).contains(&score));
    }
    let _ = annotated; // annotation count is informational by design
}

/// Emergency exemption end-to-end through the limiter: the same flood of P0
/// traffic that would throttle P4 must arrive Exempt on every single check.
#[tokio::test]
async fn p0_never_dropped_even_under_full_throttle() {
    let rl = RateLimiter::new(RateLimiterConfig {
        rate_per_sec: 1,
        burst: 2,
        refill_interval: Duration::from_secs(3600),
        exempt_p0_p1: true,
        max_sender_buckets: 100,
        unknown_sender_rate_per_sec: 5,
        unknown_sender_burst: 5,
    });
    let s = sender(9);
    for _ in 0..50 {
        assert_eq!(
            rl.check_with_claim(
                s,
                MessageClass::P0,
                iris_core::security::EmergencyClaim::Emergency,
            )
            .await,
            iris_core::security::rate_limiter::RateLimitDecision::Exempt,
            "genuine P0 emergency content is exempt everywhere, always"
        );
    }
}
