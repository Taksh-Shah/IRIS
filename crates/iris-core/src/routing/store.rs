//! Algorithm 4: Store — BASELINE_ROUTING.md §Algorithm 4.
//!
//! Delegates to the persistent store (STORE-001). Enforces TTL before storing
//! and relies on the store's priority eviction for capacity handling (P0 never
//! evicted unexpired — INV-ROUTE-003).

/// Outcome of the store decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShouldStore {
    /// Buffer for later contact.
    Store,
    /// TTL expired — must not be stored.
    Expired,
}

/// Decide whether to store a message addressed to `recipient` that is not
/// currently forwardable. `expired` is the TTL check result.
pub fn store_or_drop(expired: bool) -> ShouldStore {
    if expired {
        ShouldStore::Expired
    } else {
        ShouldStore::Store
    }
}

/// TTL-based expiry check delegating to the hardened implementation in
/// `message_engine::expiry`. Skew-suspect messages (creation time beyond the
/// 300-second skew budget) fail closed — see `expiry::is_expired` for rationale.
///
/// # Argument order
/// Callers pass `(now_unix, timestamp, ttl_seconds)`; this shim re-maps them
/// to `expiry::is_expired(created_at_unix, ttl_seconds, now_unix)`.
pub fn is_expired(now_unix: u64, timestamp: u64, ttl_seconds: u64) -> bool {
    crate::message_engine::expiry::is_expired(timestamp, ttl_seconds, now_unix)
}

/// The routing layer's store hand-off: the message id plus the decision. The
/// actual insertion lands in the MessageStorage seam (MSG-001/STORE-001).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreAction {
    pub message_id: crate::protocol::MessageId,
    pub decision: ShouldStore,
}

pub fn build_store_action(message_id: crate::protocol::MessageId, expired: bool) -> StoreAction {
    StoreAction {
        message_id,
        decision: store_or_drop(expired),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_is_not_stored() {
        assert_eq!(store_or_drop(true), ShouldStore::Expired);
        assert_eq!(store_or_drop(false), ShouldStore::Store);
    }

    #[test]
    fn ttl_check_arrival_time() {
        // 1 min TTL, message arrived at t=0, now=120 s → expired.
        assert!(is_expired(120, 0, 60));
        // still within budget.
        assert!(!is_expired(59, 0, 60));
        // timestamp 100 s in the future (within the 300 s skew budget) → trusted clock,
        // expiry = 200 + 60 = 260, now = 100 → not expired.
        assert!(!is_expired(100, 200, 60));
    }

    #[test]
    fn rout5_far_future_timestamp_fails_closed() {
        // Regression: the old `min(timestamp, now)` implementation made a message with
        // timestamp far in the future immortal — `is_expired` always returned false.
        // Now it delegates to message_engine::expiry which treats skew-suspect timestamps
        // (beyond the 300-second budget) as expired (fail-closed).
        assert!(
            is_expired(100, u64::MAX, 1),
            "far-future timestamp must be treated as expired (ROUT-5)"
        );
        assert!(
            is_expired(100, 1_000_000_000, 1),
            "forged timestamp >> skew budget must not grant immortality"
        );
    }

    #[test]
    fn store_action_holds_id_and_decision() {
        let id = crate::protocol::MessageId::new_v7();
        let action = build_store_action(id, false);
        assert_eq!(action.message_id, id);
        assert_eq!(action.decision, ShouldStore::Store);
    }
}
