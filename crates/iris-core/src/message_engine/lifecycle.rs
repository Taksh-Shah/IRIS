//! Message lifecycle state machine — MSG-001 (R1: unknown-time handling).
//!
//! State machine per `docs/implementation/MSG_DESIGN.md` §Message Lifecycle:
//!
//! ```text
//! CREATED → PENDING_SEND → IN_TRANSIT → DELIVERED → ACKNOWLEDGED
//!                          ↓              ↓
//!                        EXPIRED       DELIVERY_FAILED
//! ```
//!
//! Transitions are guarded — an illegal transition returns
//! [`LifecycleError::InvalidTransition`] instead of corrupting state.

use std::fmt;

/// Lifecycle status of a message in the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MessageStatus {
    /// Envelope accepted, persisted, queued (not yet handed to any transport).
    Created,
    /// Handed to a transport / sits in the outbound queue for the next flush.
    PendingSend,
    /// Accepted by at least one transport's `send()` (forwarded out of node).
    InTransit,
    /// Received by the final recipient (inbound delivery path).
    Delivered,
    /// Positive acknowledgment received (end-to-end / custody / GW).
    Acknowledged,
    /// TTL elapsed before delivery, or expired on the queue.
    Expired,
    /// Attempt budget exhausted / all paths failed (never silently dropped).
    DeliveryFailed,
}

impl MessageStatus {
    /// Whether this status is terminal (no further transitions allowed).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            MessageStatus::Acknowledged | MessageStatus::Expired | MessageStatus::DeliveryFailed
        )
    }

    /// Guarded transition. Returns `Ok(new_status)` on success, or an error
    /// describing the illegal transition.
    pub fn transition(self, to: MessageStatus) -> Result<MessageStatus, LifecycleError> {
        let allowed = match self {
            MessageStatus::Created => matches!(to, MessageStatus::PendingSend),
            MessageStatus::PendingSend => matches!(
                to,
                MessageStatus::InTransit | MessageStatus::Expired | MessageStatus::DeliveryFailed
            ),
            MessageStatus::InTransit => matches!(
                to,
                MessageStatus::Delivered | MessageStatus::Expired | MessageStatus::DeliveryFailed
            ),
            MessageStatus::Delivered => matches!(to, MessageStatus::Acknowledged),
            // Terminal states are absorbing; nothing may leave them.
            MessageStatus::Acknowledged
            | MessageStatus::Expired
            | MessageStatus::DeliveryFailed => false,
        };
        if allowed {
            Ok(to)
        } else {
            Err(LifecycleError::InvalidTransition { from: self, to })
        }
    }
}

/// Error from a guarded lifecycle transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleError {
    /// `from → to` is not in the allowed transition table.
    InvalidTransition {
        from: MessageStatus,
        to: MessageStatus,
    },
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LifecycleError::InvalidTransition { from, to } => {
                write!(f, "message lifecycle: illegal transition {from:?} → {to:?}")
            }
        }
    }
}

impl std::error::Error for LifecycleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions_in_order() {
        let mut s = MessageStatus::Created;
        s = s.transition(MessageStatus::PendingSend).unwrap();
        s = s.transition(MessageStatus::InTransit).unwrap();
        s = s.transition(MessageStatus::Delivered).unwrap();
        s = s.transition(MessageStatus::Acknowledged).unwrap();
        assert_eq!(s, MessageStatus::Acknowledged);
    }

    #[test]
    fn illegal_skips_are_rejected() {
        // Created → Delivered is illegal (must pass through the queue).
        assert!(matches!(
            MessageStatus::Created.transition(MessageStatus::Delivered),
            Err(LifecycleError::InvalidTransition { .. })
        ));
        // InTransit → Acknowledged without Delivered is illegal.
        assert!(matches!(
            MessageStatus::InTransit.transition(MessageStatus::Acknowledged),
            Err(LifecycleError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn terminal_states_are_absorbing() {
        for terminal in [
            MessageStatus::Acknowledged,
            MessageStatus::Expired,
            MessageStatus::DeliveryFailed,
        ] {
            assert!(terminal.is_terminal());
            assert!(matches!(
                terminal.transition(MessageStatus::PendingSend),
                Err(LifecycleError::InvalidTransition { .. })
            ));
        }
    }

    #[test]
    fn failure_and_expiry_reachable_from_transit() {
        assert_eq!(
            MessageStatus::InTransit
                .transition(MessageStatus::Expired)
                .unwrap(),
            MessageStatus::Expired
        );
        assert_eq!(
            MessageStatus::InTransit
                .transition(MessageStatus::DeliveryFailed)
                .unwrap(),
            MessageStatus::DeliveryFailed
        );
    }
}
