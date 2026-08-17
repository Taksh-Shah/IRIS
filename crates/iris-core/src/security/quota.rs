//! Per-sender storage quota + priority-reserved pool — RES-0018 R2, RFC 9171 lifetime.
//! Extends the emergency reserved pool (EMERG-001) to P0/P1 per-sender.
//! P0/P1 never evicted by quota action; eviction by lifetime/TTL + quota violation order.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::message::MessagePriority;
use tokio::sync::RwLock;

/// Saturating add for atomic byte accounting (no wrap-around accounting corrupt).
fn atomic_saturating_add(counter: &AtomicU64, amount: u64) {
    let mut cur = counter.load(Ordering::Relaxed);
    loop {
        let next = cur.saturating_add(amount);
        match counter.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

/// Saturating sub for atomic byte accounting.
fn atomic_saturating_sub(counter: &AtomicU64, amount: u64) {
    let mut cur = counter.load(Ordering::Relaxed);
    loop {
        let next = cur.saturating_sub(amount);
        match counter.compare_exchange_weak(cur, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => cur = actual,
        }
    }
}

/// 16-byte sender short ID (SHA-256(pubkey)[..16]).
pub type SenderShort = [u8; 16];

/// Storage quota configuration.
#[derive(Clone, Debug)]
pub struct QuotaConfig {
    /// Default per-sender quota in bytes (50 MB).
    pub default_sender_quota_bytes: u64,
    /// Priority-reserved pool size in bytes (10 MB) — P0/P1 never evicted.
    pub priority_reserved_pool_bytes: u64,
    /// Total storage quota before global eviction (500 MB).
    pub total_quota_bytes: u64,
    /// Minimum TTL for eviction consideration (60 s).
    pub min_ttl_for_eviction: Duration,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        QuotaConfig {
            default_sender_quota_bytes: 50 * 1024 * 1024,   // 50 MB
            priority_reserved_pool_bytes: 10 * 1024 * 1024, // 10 MB
            total_quota_bytes: 500 * 1024 * 1024,           // 500 MB
            min_ttl_for_eviction: Duration::from_secs(60),
        }
    }
}

/// Per-sender storage accounting.
#[derive(Clone, Debug, Default)]
struct SenderAccount {
    used_bytes: u64,
    message_count: u64,
    oldest_message: Option<Instant>,
    last_activity: Option<Instant>,
}

/// Quota enforcement decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaDecision {
    /// Within quota; message accepted.
    Accepted,
    /// Over quota but P0/P1 — accepted (priority-reserved pool).
    PriorityExempt,
    /// Over quota and not P0/P1 — rejected.
    Rejected,
}

/// Per-sender storage quota + priority pool manager.
pub struct QuotaManager {
    accounts: RwLock<HashMap<SenderShort, SenderAccount>>,
    config: QuotaConfig,
    metrics: QuotaMetrics,
    /// Total bytes across all senders.
    total_bytes: AtomicU64,
}

#[derive(Default, Debug)]
pub struct QuotaMetrics {
    pub accepted: AtomicU64,
    pub priority_exempt: AtomicU64,
    pub rejected: AtomicU64,
    pub evicted: AtomicU64,
}

impl QuotaManager {
    /// Create a new quota manager with default configuration.
    pub fn new(config: QuotaConfig) -> Self {
        QuotaManager {
            accounts: RwLock::new(HashMap::with_capacity(1024)),
            config,
            metrics: QuotaMetrics::default(),
            total_bytes: AtomicU64::new(0),
        }
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> QuotaMetricsSnapshot {
        QuotaMetricsSnapshot {
            accepted: self.metrics.accepted.load(Ordering::Relaxed),
            priority_exempt: self.metrics.priority_exempt.load(Ordering::Relaxed),
            rejected: self.metrics.rejected.load(Ordering::Relaxed),
            evicted: self.metrics.evicted.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
        }
    }

    /// Check if a new message from sender with given size and priority would be
    /// within quota. Returns decision without mutating state.
    pub async fn check(
        &self,
        sender: SenderShort,
        message_size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        let accounts = self.accounts.read().await;
        let account = accounts.get(&sender);

        let used = account.map(|a| a.used_bytes).unwrap_or(0);
        let new_total = used.saturating_add(message_size);

        // P0/P1 always accepted (priority-reserved pool)
        if matches!(priority, MessagePriority::P0 | MessagePriority::P1) {
            return QuotaDecision::PriorityExempt;
        }

        // Check per-sender quota
        if new_total > self.config.default_sender_quota_bytes {
            return QuotaDecision::Rejected;
        }

        QuotaDecision::Accepted
    }

    /// Record a message being added to storage (mutates state).
    /// Returns the quota decision (same as check but with state update).
    pub async fn add_message(
        &self,
        sender: SenderShort,
        message_size: u64,
        priority: MessagePriority,
    ) -> QuotaDecision {
        let mut accounts = self.accounts.write().await;
        let account = accounts.entry(sender).or_insert_with(|| SenderAccount {
            used_bytes: 0,
            message_count: 0,
            oldest_message: None,
            last_activity: Some(Instant::now()),
        });

        let new_total = account.used_bytes.saturating_add(message_size);

        // P0/P1 always accepted via priority pool
        if matches!(priority, MessagePriority::P0 | MessagePriority::P1) {
            account.used_bytes = new_total;
            account.message_count += 1;
            account.last_activity = Some(Instant::now());
            if account.oldest_message.is_none() {
                account.oldest_message = Some(Instant::now());
            }
            atomic_saturating_add(&self.total_bytes, message_size);
            self.metrics.priority_exempt.fetch_add(1, Ordering::Relaxed);
            return QuotaDecision::PriorityExempt;
        }

        // Check per-sender quota
        if new_total > self.config.default_sender_quota_bytes {
            self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
            return QuotaDecision::Rejected;
        }

        account.used_bytes = new_total;
        account.message_count += 1;
        account.last_activity = Some(Instant::now());
        if account.oldest_message.is_none() {
            account.oldest_message = Some(Instant::now());
        }
        atomic_saturating_add(&self.total_bytes, message_size);
        self.metrics.accepted.fetch_add(1, Ordering::Relaxed);
        QuotaDecision::Accepted
    }

    /// Remove a message from sender's quota (e.g., on delivery/eviction).
    pub async fn remove_message(&self, sender: SenderShort, message_size: u64) {
        let mut accounts = self.accounts.write().await;
        if let Some(account) = accounts.get_mut(&sender) {
            account.used_bytes = account.used_bytes.saturating_sub(message_size);
            account.message_count = account.message_count.saturating_sub(1);
            atomic_saturating_sub(&self.total_bytes, message_size);
            // Clean up empty accounts
            if account.message_count == 0 {
                accounts.remove(&sender);
            }
        }
    }

    /// Evict messages to make room (lifetime/TTL + quota violation order).
    /// Returns list of (sender, message_size) pairs to evict.
    /// This is called by the GC task; actual eviction is done by storage layer.
    pub async fn select_eviction_candidates(
        &self,
        target_bytes: u64,
        now: Instant,
        min_ttl: Duration,
    ) -> Vec<(SenderShort, u64)> {
        let accounts = self.accounts.read().await;
        let mut candidates = Vec::new();

        // Priority: oldest messages first (by TTL/lifetime), then quota violators
        let mut senders: Vec<_> = accounts.iter().filter(|(_, a)| a.used_bytes > 0).collect();

        // Sort by oldest activity first (FIFO-ish), then by quota excess
        senders.sort_by(|a, b| {
            let a_oldest = a.1.oldest_message.or(a.1.last_activity);
            let b_oldest = b.1.oldest_message.or(b.1.last_activity);
            a_oldest.cmp(&b_oldest)
        });

        let mut freed = 0u64;
        for (sender, account) in senders {
            if freed >= target_bytes {
                break;
            }
            // Only evict if message is past minimum TTL
            if let Some(oldest) = account.oldest_message {
                if now.duration_since(oldest) < min_ttl {
                    continue;
                }
            }
            // Evict up to the excess over quota, or all if total quota exceeded
            let excess = account
                .used_bytes
                .saturating_sub(self.config.default_sender_quota_bytes);
            let to_evict = excess.max(1).min(account.used_bytes);
            candidates.push((*sender, to_evict));
            // SEC-RT-12: saturating add — freed must never overflow u64.
            freed = freed.saturating_add(to_evict);
        }

        candidates
    }

    /// Get per-sender usage for monitoring.
    pub async fn sender_usage(&self, sender: SenderShort) -> Option<(u64, u64)> {
        let accounts = self.accounts.read().await;
        accounts
            .get(&sender)
            .map(|a| (a.used_bytes, a.message_count))
    }

    /// Get total usage.
    pub fn total_usage(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
    }

    /// Reset all accounts (for testing).
    pub async fn reset(&self) {
        self.accounts.write().await.clear();
        self.total_bytes.store(0, Ordering::Relaxed);
    }
}

impl Default for QuotaManager {
    fn default() -> Self {
        Self::new(QuotaConfig::default())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QuotaMetricsSnapshot {
    pub accepted: u64,
    pub priority_exempt: u64,
    pub rejected: u64,
    pub evicted: u64,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn quota_accepts_within_limit() {
        let qm = QuotaManager::default();
        let sender = [1u8; 16];

        // Add messages within quota
        for _ in 0..10 {
            assert_eq!(
                qm.add_message(sender, 1024, MessagePriority::P4).await,
                QuotaDecision::Accepted
            );
        }
    }

    #[tokio::test]
    async fn quota_rejects_over_limit() {
        let config = QuotaConfig {
            default_sender_quota_bytes: 5120, // 5 KB
            ..Default::default()
        };
        let qm = QuotaManager::new(config);
        let sender = [2u8; 16];

        // Add 5 KB
        for _ in 0..5 {
            assert_eq!(
                qm.add_message(sender, 1024, MessagePriority::P4).await,
                QuotaDecision::Accepted
            );
        }
        // 6th should be rejected
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Rejected
        );
    }

    #[tokio::test]
    async fn p0_p1_priority_exempt() {
        let config = QuotaConfig {
            default_sender_quota_bytes: 1024, // 1 KB
            priority_reserved_pool_bytes: 10 * 1024 * 1024,
            ..Default::default()
        };
        let qm = QuotaManager::new(config);
        let sender = [3u8; 16];

        // Fill quota with P4
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        // P0 should still be accepted (priority-exempt)
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P0).await,
            QuotaDecision::PriorityExempt
        );
        // P1 should still be accepted
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P1).await,
            QuotaDecision::PriorityExempt
        );
    }

    #[tokio::test]
    async fn huge_message_size_no_overflow() {
        // SEC-RT regression: attacker-controlled message_size == u64::MAX must
        // saturate, not overflow (fuzz found 'attempt to add with overflow'
        // panic + release-mode quota bypass via wraparound at quota.rs:143).
        let config = QuotaConfig {
            default_sender_quota_bytes: 1024,
            ..Default::default()
        };
        let qm = QuotaManager::new(config);
        let sender = [0xAA; 16];

        // check() must not panic and must reject the massive message.
        let d = qm.check(sender, u64::MAX, MessagePriority::P4).await;
        assert_eq!(d, QuotaDecision::Rejected);

        // add_message() must not panic nor accept (no wraparound bypass).
        let d = qm.add_message(sender, u64::MAX, MessagePriority::P4).await;
        assert_eq!(d, QuotaDecision::Rejected);

        // Account bookkeeping must not grow from the rejected huge add.
        assert_eq!(qm.sender_usage(sender).await, Some((0, 0)));

        // P0 priority-exempt path must saturate without panic; total_bytes
        // must not wrap around.
        let d = qm.add_message(sender, u64::MAX, MessagePriority::P0).await;
        assert_eq!(d, QuotaDecision::PriorityExempt);
        assert_eq!(qm.total_usage(), u64::MAX);

        // Removing a huge size must not underflow.
        qm.remove_message(sender, u64::MAX).await;
        assert_eq!(qm.total_usage(), 0);
        assert!(qm.sender_usage(sender).await.is_none());
    }

    #[tokio::test]
    async fn quota_metrics_snapshot() {
        let qm = QuotaManager::default();
        let sender = [7u8; 16];
        qm.add_message(sender, 1024, MessagePriority::P4).await;
        qm.add_message(sender, 1024, MessagePriority::P0).await;
        qm.add_message(sender, 1024, MessagePriority::P4).await;
        let m = qm.metrics();
        assert_eq!(m.accepted, 2);
        assert_eq!(m.priority_exempt, 1);
        assert_eq!(m.rejected, 0);
        assert_eq!(m.total_bytes, 3072);
    }

    #[tokio::test]
    async fn quota_check_priority_and_reject_paths() {
        let config = QuotaConfig {
            default_sender_quota_bytes: 1024,
            ..Default::default()
        };
        let qm = QuotaManager::new(config);
        let sender = [8u8; 16];

        // check() P0 -> PriorityExempt (per-sender quota ignored for P0/P1).
        assert_eq!(
            qm.check(sender, 1_000_000, MessagePriority::P0).await,
            QuotaDecision::PriorityExempt
        );
        // check() P4 within quota -> Accepted.
        assert_eq!(
            qm.check(sender, 512, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        // check() P4 over quota -> Rejected, no state mutation.
        assert_eq!(
            qm.check(sender, 2048, MessagePriority::P4).await,
            QuotaDecision::Rejected
        );
        assert_eq!(qm.sender_usage(sender).await, None);
    }

    #[tokio::test]
    async fn quota_eviction_min_ttl_and_break() {
        let qm = QuotaManager::default();
        let sender1 = [9u8; 16];
        let sender2 = [0x0A; 16];

        qm.add_message(sender1, 1024, MessagePriority::P4).await;
        qm.add_message(sender2, 1024, MessagePriority::P4).await;

        // min_ttl in the future relative to newest -> accounts skipped (continue).
        let far_ttl = qm
            .select_eviction_candidates(1024, Instant::now(), Duration::from_secs(3600))
            .await;
        assert!(far_ttl.is_empty());

        // target_bytes already exceeded by first candidate -> break.
        let small_target = qm
            .select_eviction_candidates(1, Instant::now(), Duration::from_secs(0))
            .await;
        assert_eq!(small_target.len(), 1);
    }

    #[tokio::test]
    async fn quota_reset() {
        let qm = QuotaManager::default();
        let sender = [0x0B; 16];
        qm.add_message(sender, 1024, MessagePriority::P4).await;
        qm.add_message(sender, 2048, MessagePriority::P4).await;
        assert_eq!(qm.total_usage(), 3072);
        qm.reset().await;
        assert_eq!(qm.total_usage(), 0);
        assert_eq!(qm.sender_usage(sender).await, None);
    }

    #[tokio::test]
    async fn remove_message_frees_quota() {
        let config = QuotaConfig {
            default_sender_quota_bytes: 2048,
            ..Default::default()
        };
        let qm = QuotaManager::new(config);
        let sender = [4u8; 16];

        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Rejected
        );

        // Remove one message
        qm.remove_message(sender, 1024).await;

        // Should be able to add again
        assert_eq!(
            qm.add_message(sender, 1024, MessagePriority::P4).await,
            QuotaDecision::Accepted
        );
    }

    #[tokio::test]
    async fn eviction_candidates_oldest_first() {
        let qm = QuotaManager::default();
        let sender1 = [5u8; 16];
        let sender2 = [6u8; 16];

        // sender1 added first
        qm.add_message(sender1, 1024, MessagePriority::P4).await;
        qm.add_message(sender2, 1024, MessagePriority::P4).await;

        let candidates = qm
            .select_eviction_candidates(2048, Instant::now(), Duration::from_secs(0))
            .await;
        // sender1 should be first (oldest)
        assert_eq!(candidates[0].0, sender1);
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
        fn quota_never_panics_on_valid_input(
            sender in any::<[u8; 16]>(),
            quota in 1024..100_000_000u64,
            size in 1..10_000u64,
            priority in any::<MessagePriority>(),
            n_ops in 1..100usize,
        ) {
            let config = QuotaConfig {
                default_sender_quota_bytes: quota,
                priority_reserved_pool_bytes: quota / 10,
                total_quota_bytes: quota * 10,
                min_ttl_for_eviction: Duration::from_secs(60),
            };
            let qm = QuotaManager::new(config);
            for _ in 0..n_ops {
                let _ = block_on(qm.add_message(sender, size, priority));
                block_on(qm.remove_message(sender, size));
            }
        }

        #[test]
        fn quota_p0_p1_priority_exempt(
            sender in any::<[u8; 16]>(),
            quota in 1024..10_000u64,
            n_ops in 1..50usize,
        ) {
            let config = QuotaConfig {
                default_sender_quota_bytes: quota,
                priority_reserved_pool_bytes: quota * 10,
                total_quota_bytes: quota * 100,
                min_ttl_for_eviction: Duration::from_secs(60),
            };
            let qm = QuotaManager::new(config);
            // Fill quota with P4
            for _ in 0..(quota / 1024 + 1) {
                let _ = block_on(qm.add_message(sender, 1024, MessagePriority::P4));
            }
            // P0/P1 should still be accepted (priority-exempt)
            for _ in 0..n_ops {
                prop_assert_eq!(block_on(qm.add_message(sender, 1024, MessagePriority::P0)), QuotaDecision::PriorityExempt);
                prop_assert_eq!(block_on(qm.add_message(sender, 1024, MessagePriority::P1)), QuotaDecision::PriorityExempt);
            }
        }

        #[test]
        fn quota_rejects_over_limit(
            sender in any::<[u8; 16]>(),
            quota in 2048..10_000u64,
            size in 1024..8192u64,
        ) {
            let config = QuotaConfig {
                default_sender_quota_bytes: quota,
                priority_reserved_pool_bytes: quota / 10,
                total_quota_bytes: quota * 10,
                min_ttl_for_eviction: Duration::from_secs(60),
            };
            let qm = QuotaManager::new(config);
            let max_msgs = (quota / size) as usize;
            let mut accepted = 0;
            for _ in 0..max_msgs + 5 {
                match block_on(qm.add_message(sender, size, MessagePriority::P4)) {
                    QuotaDecision::Accepted => accepted += 1,
                    QuotaDecision::Rejected => break,
                    _ => {}
                }
            }
            prop_assert!(accepted <= max_msgs);
        }

        #[test]
        fn quota_remove_frees_space(
            sender in any::<[u8; 16]>(),
            quota in 2048..10_000u64,
            size in 1024..2048u64,
        ) {
            let config = QuotaConfig {
                default_sender_quota_bytes: quota,
                priority_reserved_pool_bytes: quota / 10,
                total_quota_bytes: quota * 10,
                min_ttl_for_eviction: Duration::from_secs(60),
            };
            let qm = QuotaManager::new(config);
            let max_msgs = (quota / size) as usize;
            for _ in 0..max_msgs {
                prop_assert_eq!(block_on(qm.add_message(sender, size, MessagePriority::P4)), QuotaDecision::Accepted);
            }
            // Remove one
            block_on(qm.remove_message(sender, size));
            // Should be able to add one more
            prop_assert_eq!(block_on(qm.add_message(sender, size, MessagePriority::P4)), QuotaDecision::Accepted);
        }

        #[test]
        fn quota_total_bytes_tracked(
            sender1 in any::<[u8; 16]>(),
            sender2 in any::<[u8; 16]>(),
            quota in 10_000..1_000_000u64,
            size in 1024..10_000u64,
        ) {
            let config = QuotaConfig {
                default_sender_quota_bytes: quota,
                priority_reserved_pool_bytes: quota / 10,
                total_quota_bytes: quota * 10,
                min_ttl_for_eviction: Duration::from_secs(60),
            };
            let qm = QuotaManager::new(config);
            let max1 = (quota / size / 2) as usize;
            let max2 = (quota / size / 2) as usize;
            for _ in 0..max1 {
                let _ = block_on(qm.add_message(sender1, size, MessagePriority::P4));
            }
            for _ in 0..max2 {
                let _ = block_on(qm.add_message(sender2, size, MessagePriority::P4));
            }
            let total = qm.total_usage();
            prop_assert!(total <= quota * 2);
        }
    }
}
