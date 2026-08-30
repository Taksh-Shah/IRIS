//! ACK + retransmission tracking — MSG-001 (R2: unified retry table).
//!
//! Implements the retry strategy from MSG_DESIGN.md §Retry Strategy (validated
//! by RES-0010 R2 against RFC 6298 backoff cap practice). One table, one source
//! of truth — the `MSG_DESIGN.md` numbers win over any conflicting RETRIES /
//! ACKNOWLEDGEMENTS doc.
//!
//! | Priority | Initial timeout | Backoff | Max attempts |
//! |----------|-----------------|---------|--------------|
//! | P0       | 30 s            | 2×      | unlimited (until TTL) |
//! | P1       | 60 s            | 2×      | 10 |
//! | P2       | 120 s           | 2×      | 5 |
//! | P3       | 300 s           | 2×      | 3 |
//! | P4–P7    | 600 s           | 2×      | 2 |

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::message::MessagePriority;
use crate::protocol::MessageId;

/// Retry parameters for a priority class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    /// Initial timeout before the first retry.
    pub initial_timeout: Duration,
    /// Multiplier applied to the timeout after each attempt (2× → exponential).
    pub backoff_factor: u64,
    /// Maximum attempts (`None` = retry until TTL / explicit failure).
    pub max_attempts: Option<u32>,
}

impl RetryPolicy {
    /// Timeout to use for the `n`-th attempt (0-based), capped at 1 h
    /// (RFC 6298-style cap: RTO must not exceed 60 s for TCP; we cap our
    /// longer DTN intervals at 1 h to avoid multi-day stallouts).
    pub fn timeout_for_attempt(&self, attempt: u32) -> Duration {
        let exp = attempt;
        let base_secs = self.initial_timeout.as_secs();
        let secs = base_secs.saturating_mul(self.backoff_factor.saturating_pow(exp));
        Duration::from_secs(secs.min(3600))
    }
}

/// Per-priority retry table (MSG_DESIGN.md §Retry Strategy).
pub fn retry_policy(priority: MessagePriority) -> RetryPolicy {
    match priority {
        MessagePriority::P0 => RetryPolicy {
            initial_timeout: Duration::from_secs(30),
            backoff_factor: 2,
            max_attempts: None, // until TTL
        },
        MessagePriority::P1 => RetryPolicy {
            initial_timeout: Duration::from_secs(60),
            backoff_factor: 2,
            max_attempts: Some(10),
        },
        MessagePriority::P2 => RetryPolicy {
            initial_timeout: Duration::from_secs(120),
            backoff_factor: 2,
            max_attempts: Some(5),
        },
        MessagePriority::P3 => RetryPolicy {
            initial_timeout: Duration::from_secs(300),
            backoff_factor: 2,
            max_attempts: Some(3),
        },
        MessagePriority::P4 | MessagePriority::P5 | MessagePriority::P6 | MessagePriority::P7 => {
            RetryPolicy {
                initial_timeout: Duration::from_secs(600),
                backoff_factor: 2,
                max_attempts: Some(2),
            }
        }
    }
}

/// Hard cap on pending entries (PM-3: P0 messages have no max_attempts so
/// they never exhaust; without a bound, a split-brain node accumulates entries
/// forever).
pub const MAX_PENDING_ACKS: usize = 8_192;

/// A message awaiting acknowledgment.
#[derive(Debug, Clone)]
struct PendingAck {
    priority: MessagePriority,
    attempts: u32,
    last_attempt_at: Instant,
    next_retry: Option<Instant>,
    /// Wire timestamp and TTL stored so the tracker can filter TTL-expired
    /// entries without touching storage (PM-3).
    timestamp: u64,
    ttl_seconds: u64,
}

/// Whether the wire TTL for a pending ACK has expired.
fn ack_ttl_expired(timestamp: u64, ttl_seconds: u64, now_unix: u64) -> bool {
    now_unix >= timestamp.saturating_add(ttl_seconds)
}

/// ACK/custody tracker: pending map + retry scheduling.
#[derive(Debug, Default)]
pub struct AckTracker {
    pending: HashMap<MessageId, PendingAck>,
}

impl AckTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a sent message as awaiting ACK. A re-sent message re-arms its
    /// wait window from `sent_at` but **keeps its attempt count**.
    ///
    /// Resetting `attempts` to 0 here would make the retry budget unreachable:
    /// every retransmission goes through the delivery path, which re-registers
    /// on success, so a message with a finite cap would never exhaust it and
    /// would retry forever.
    pub fn register_sent(
        &mut self,
        id: MessageId,
        priority: MessagePriority,
        sent_at: Instant,
        timestamp: u64,
        ttl_seconds: u64,
    ) {
        // PM-3: defense-in-depth cap. P0 messages (max_attempts = None) never
        // exhaust their budget, so a network-split node would grow this map
        // without bound. Evict the entry with the oldest last_attempt_at when
        // inserting a genuinely new id that would push us over the cap.
        if !self.pending.contains_key(&id) && self.pending.len() >= MAX_PENDING_ACKS {
            if let Some(oldest_id) = self
                .pending
                .iter()
                .min_by_key(|(_, p)| p.last_attempt_at)
                .map(|(k, _)| *k)
            {
                self.pending.remove(&oldest_id);
            }
        }
        let policy = retry_policy(priority);
        let attempts = self.pending.get(&id).map(|p| p.attempts).unwrap_or(0);
        let next_retry = sent_at + policy.timeout_for_attempt(attempts);
        self.pending.insert(
            id,
            PendingAck {
                priority,
                attempts,
                last_attempt_at: sent_at,
                next_retry: Some(next_retry),
                timestamp,
                ttl_seconds,
            },
        );
    }

    /// Stop tracking `id` without treating it as acknowledged (e.g. the stored
    /// copy is gone, so no retry can ever be dispatched for it).
    pub fn forget(&mut self, id: MessageId) {
        self.pending.remove(&id);
    }

    /// Number of messages awaiting acknowledgement.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    /// Record that a retry was dispatched for `id`; returns the *new* policy
    /// state for the caller. `None` if the message is no longer tracked.
    pub fn record_retry(&mut self, id: MessageId, at: Instant) -> Option<RetryState> {
        let entry = self.pending.get_mut(&id)?;
        entry.attempts += 1;
        entry.last_attempt_at = at;
        let policy = retry_policy(entry.priority);
        entry.next_retry = Some(at + policy.timeout_for_attempt(entry.attempts));
        Some(RetryState {
            attempts: entry.attempts,
            max_attempts: policy.max_attempts,
        })
    }

    /// Consume an ACK; returns `true` if a pending message was acknowledged.
    pub fn acknowledge(&mut self, id: MessageId) -> bool {
        self.pending.remove(&id).is_some()
    }

    /// Whether an ACK is still awaited for `id`.
    pub fn is_pending(&self, id: &MessageId) -> bool {
        self.pending.contains_key(id)
    }

    /// All pending messages whose retry window has passed at `now` and whose
    /// TTL has not yet expired at `now_unix`. Expired entries are excluded so
    /// that P0 messages (no attempt cap) stop being retried when their wire TTL
    /// has passed (PM-3).
    pub fn due_retries(&self, now: Instant, now_unix: u64) -> Vec<MessageId> {
        self.pending
            .iter()
            .filter(|(_, p)| {
                !ack_ttl_expired(p.timestamp, p.ttl_seconds, now_unix)
                    && p.next_retry.map(|t| t <= now).unwrap_or(false)
                    && p.max_attempts_remaining() > 0
            })
            .map(|(id, _)| *id)
            .collect()
    }

    /// Remove and return the ids of all pending entries whose wire TTL has
    /// expired at `now_unix` (PM-3). The GC task calls this so that TTL-dead
    /// P0 entries do not accumulate in the pending map indefinitely.
    pub fn drain_expired(&mut self, now_unix: u64) -> Vec<MessageId> {
        let expired: Vec<MessageId> = self
            .pending
            .iter()
            .filter(|(_, p)| ack_ttl_expired(p.timestamp, p.ttl_seconds, now_unix))
            .map(|(id, _)| *id)
            .collect();
        for id in &expired {
            self.pending.remove(id);
        }
        expired
    }

    /// Messages whose attempt budget is exhausted (candidates for
    /// `DeliveryFailed`).
    pub fn exhausted(&self) -> Vec<(MessageId, u32)> {
        self.pending
            .iter()
            .filter(|(_, p)| p.max_attempts_remaining() == 0)
            .map(|(id, p)| (*id, p.attempts))
            .collect()
    }

    /// Drop all tracking (e.g. on shutdown).
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

impl PendingAck {
    fn max_attempts_remaining(&self) -> u32 {
        match retry_policy(self.priority).max_attempts {
            Some(max) => max.saturating_sub(self.attempts),
            None => 1, // unlimited → always "has budget"
        }
    }
}

/// Snapshot of the retry state after a dispatch (for observability/tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryState {
    /// Number of attempts so far (including the just-recorded one).
    pub attempts: u32,
    /// Attempt cap (`None` = unlimited).
    pub max_attempts: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seq: u8) -> MessageId {
        MessageId::from_bytes([seq; 16])
    }

    #[test]
    fn retry_table_matches_msg_design() {
        assert_eq!(
            retry_policy(MessagePriority::P0).initial_timeout,
            Duration::from_secs(30)
        );
        assert_eq!(retry_policy(MessagePriority::P0).max_attempts, None);
        assert_eq!(
            retry_policy(MessagePriority::P1).initial_timeout,
            Duration::from_secs(60)
        );
        assert_eq!(retry_policy(MessagePriority::P1).max_attempts, Some(10));
        assert_eq!(
            retry_policy(MessagePriority::P2).initial_timeout,
            Duration::from_secs(120)
        );
        assert_eq!(retry_policy(MessagePriority::P2).max_attempts, Some(5));
        assert_eq!(
            retry_policy(MessagePriority::P3).initial_timeout,
            Duration::from_secs(300)
        );
        assert_eq!(retry_policy(MessagePriority::P3).max_attempts, Some(3));
        for p in [
            MessagePriority::P4,
            MessagePriority::P5,
            MessagePriority::P6,
            MessagePriority::P7,
        ] {
            assert_eq!(retry_policy(p).initial_timeout, Duration::from_secs(600));
            assert_eq!(retry_policy(p).max_attempts, Some(2));
        }
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        let p = retry_policy(MessagePriority::P2); // 120 s, 2×
        assert_eq!(p.timeout_for_attempt(0), Duration::from_secs(120));
        assert_eq!(p.timeout_for_attempt(1), Duration::from_secs(240));
        assert_eq!(p.timeout_for_attempt(2), Duration::from_secs(480));
        // cap at 1 h even for huge exponents
        assert!(p.timeout_for_attempt(10) <= Duration::from_secs(3600));
    }

    #[test]
    fn ack_clears_pending() {
        let mut t = AckTracker::new();
        let now = Instant::now();
        t.register_sent(id(1), MessagePriority::P1, now, 0, u64::MAX);
        assert!(t.is_pending(&id(1)));
        assert!(t.acknowledge(id(1)));
        assert!(!t.is_pending(&id(1)));
        assert!(!t.acknowledge(id(1)), "unknown ack returns false");
    }

    #[test]
    fn due_retries_respects_initial_window_and_budget() {
        let mut t = AckTracker::new();
        let now = Instant::now();
        t.register_sent(id(1), MessagePriority::P4, now, 0, u64::MAX); // 600 s window, max 2
        assert!(t.due_retries(now, 0).is_empty(), "not due at t=0");
        assert!(t
            .due_retries(now + Duration::from_secs(601), 0)
            .contains(&id(1)));

        // Simulate two retries → budget exhausted → no longer due.
        let t1 = now + Duration::from_secs(601);
        t.record_retry(id(1), t1);
        let t2 = t1 + Duration::from_secs(1201);
        t.record_retry(id(1), t2);
        assert!(
            t.due_retries(t2 + Duration::from_secs(1), 0).is_empty(),
            "budget exhausted"
        );
        assert_eq!(t.exhausted().len(), 1);
    }

    #[test]
    fn resend_preserves_attempt_budget() {
        // Regression: `register_sent` used to reset `attempts` to 0. Every
        // retry goes through the delivery path, which re-registers on success,
        // so a finite budget could never be reached and the message retried
        // forever.
        let mut t = AckTracker::new();
        let now = Instant::now();
        t.register_sent(id(3), MessagePriority::P4, now, 0, u64::MAX); // max 2 attempts

        let t1 = now + Duration::from_secs(601);
        t.record_retry(id(3), t1);
        // The retry is dispatched and the delivery path re-registers it.
        t.register_sent(id(3), MessagePriority::P4, t1, 0, u64::MAX);

        let t2 = t1 + Duration::from_secs(1_201);
        t.record_retry(id(3), t2);
        t.register_sent(id(3), MessagePriority::P4, t2, 0, u64::MAX);

        assert_eq!(
            t.exhausted().len(),
            1,
            "budget must be reachable across re-sends"
        );
        assert!(
            t.due_retries(t2 + Duration::from_secs(100_000), 0).is_empty(),
            "an exhausted message must stop being due"
        );
    }

    #[test]
    fn recorded_retry_is_not_immediately_due_again() {
        // Regression: the retry task never called `record_retry`, so
        // `next_retry` stayed in the past and `due_retries` returned the same
        // id on every one-second tick — for every message ever sent.
        let mut t = AckTracker::new();
        let now = Instant::now();
        t.register_sent(id(4), MessagePriority::P1, now, 0, u64::MAX); // 60 s window

        let due_at = now + Duration::from_secs(61);
        assert!(t.due_retries(due_at, 0).contains(&id(4)));

        t.record_retry(id(4), due_at);
        assert!(
            t.due_retries(due_at + Duration::from_secs(1), 0).is_empty(),
            "backoff must push the next retry into the future"
        );
        // Second window is 120 s (60 s doubled).
        assert!(t
            .due_retries(due_at + Duration::from_secs(121), 0)
            .contains(&id(4)));
    }

    #[test]
    fn forget_stops_tracking_without_acknowledging() {
        let mut t = AckTracker::new();
        t.register_sent(id(5), MessagePriority::P2, Instant::now(), 0, u64::MAX);
        assert_eq!(t.pending_len(), 1);
        t.forget(id(5));
        assert_eq!(t.pending_len(), 0);
        assert!(!t.is_pending(&id(5)));
    }

    #[test]
    fn p0_retries_never_exhaust() {
        let mut t = AckTracker::new();
        let now = Instant::now();
        t.register_sent(id(2), MessagePriority::P0, now, 0, u64::MAX);
        for i in 0..5u32 {
            let at = now + Duration::from_secs(60 * (i + 1) as u64);
            let st = t.record_retry(id(2), at).unwrap();
            assert_eq!(st.max_attempts, None, "P0 has no attempt cap");
        }
        assert!(t.exhausted().is_empty(), "P0 must never be exhausted");
    }

    #[test]
    fn p0_leak_stopped_by_ttl_drain() {
        // PM-3: a P0 message (max_attempts = None) with an expired wire TTL
        // must NOT appear in due_retries, and must be removed by drain_expired.
        let mut t = AckTracker::new();
        let now = Instant::now();
        // timestamp=1000, ttl_seconds=3600 → expires at unix 4600.
        t.register_sent(id(10), MessagePriority::P0, now, 1_000, 3_600);

        // Before TTL: appears in due_retries once the retry window passes.
        let past_retry = now + Duration::from_secs(31); // > 30 s initial P0 window
        assert!(
            t.due_retries(past_retry, 1_100).contains(&id(10)),
            "non-expired P0 must be due for retry"
        );

        // After TTL (now_unix = 5000 > 4600): must not appear in due_retries.
        let far_future = now + Duration::from_secs(100_000);
        assert!(
            !t.due_retries(far_future, 5_000).contains(&id(10)),
            "TTL-expired P0 must not be retried"
        );

        // drain_expired removes the entry.
        let drained = t.drain_expired(5_000);
        assert!(drained.contains(&id(10)), "expired P0 must be drained");
        assert_eq!(t.pending_len(), 0, "pending must be empty after drain");
    }

    #[test]
    fn max_pending_cap_evicts_oldest() {
        // PM-3: inserting MAX_PENDING_ACKS + 1 distinct ids must not grow the
        // pending map past the cap.
        let mut t = AckTracker::new();
        let base = Instant::now();
        for i in 0u64..=(MAX_PENDING_ACKS as u64) {
            let mut key = [0u8; 16];
            key[..8].copy_from_slice(&i.to_le_bytes());
            let msg_id = MessageId::from_bytes(key);
            let at = base + Duration::from_secs(i);
            t.register_sent(msg_id, MessagePriority::P1, at, 0, u64::MAX);
        }
        assert!(
            t.pending_len() <= MAX_PENDING_ACKS,
            "pending map must not exceed MAX_PENDING_ACKS"
        );
    }
}
