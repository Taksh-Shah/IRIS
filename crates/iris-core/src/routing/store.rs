//! Algorithm 4: Store — BASELINE_ROUTING.md §Algorithm 4.
//!
//! Delegates to the persistent store (STORE-001). Enforces TTL before storing
//! and relies on the store's priority eviction for capacity handling (P0 never
//! evicted unexpired — INV-ROUTE-003).

use crate::message::PeerId;

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

/// TTL-based expiry check (RFC 9171 §4.4.2 semantics: arrival-time lifetime
/// for skew-suspect messages). Exposed here so the routing layer and the store
/// share one definition.
pub fn is_expired(now_unix: u64, timestamp: u64, ttl_seconds: u64) -> bool {
    let lifetime_start = timestamp.min(now_unix);
    now_unix.saturating_sub(lifetime_start) > ttl_seconds
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

/// Marker for contact-aware routing (feed with DISCO contact events).
/// Retained so callers can attach the destination the next hop serves.
#[allow(dead_code)]
fn _destination_guard(recipient: PeerId) -> PeerId {
    recipient
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
        // timestamp in the future (skew) never counts against earlier.
        assert!(!is_expired(100, 200, 60));
    }

    #[test]
    fn store_action_holds_id_and_decision() {
        let id = crate::protocol::MessageId::new_v7();
        let action = build_store_action(id, false);
        assert_eq!(action.message_id, id);
        assert_eq!(action.decision, ShouldStore::Store);
    }
}