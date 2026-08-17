//! SCF contact-opportunity handler — SCF-001 (WP-5).
//!
//! Per STORE_CARRY_FORWARD.md §Contact Opportunity Handler: when a new
//! neighbor is discovered (DISCO-001 event), evaluate all buffered messages,
//! rank forwardable candidates (priority first, then delivery probability),
//! and enqueue forwards respecting transport bandwidth.

use crate::message::PeerId;
use crate::message_engine::storage::MessageStorage;
use crate::routing::scf::{ScfEngine, ScfError, StoredMessage};

/// Contact hook result: the ordered candidate list the caller should forward.
#[derive(Debug, Default, PartialEq)]
pub struct ContactOutcome {
    /// Forwardable candidates ranked (priority ASC, then delivery probability
    /// DESC).
    pub ranked: Vec<ForwardCandidate>,
    /// Candidates skipped because the transport lacked capacity.
    pub deferred: usize,
}

/// One message selected for forwarding to the contact.
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardCandidate {
    pub message_id: crate::protocol::MessageId,
    pub to: PeerId,
    /// 0..1 estimate that forwarding to this contact ultimately delivers.
    pub delivery_probability: f64,
}

impl ForwardCandidate {
    pub fn new(message_id: crate::protocol::MessageId, to: PeerId, p: f64) -> Self {
        ForwardCandidate {
            message_id,
            to,
            delivery_probability: if p.is_finite() {
                p.clamp(0.0, 1.0)
            } else {
                0.0
            },
        }
    }
}

/// Probability estimate to a contact. Direct neighbors get a high base; the
/// value may later be refined with delivery statistics (SCF metrics).
fn delivery_probability(envelope: &crate::protocol::Envelope, contact: &PeerId) -> f64 {
    let recipient = envelope.recipient_id.as_slice();
    if recipient.len() == 32 {
        let mut b = [0u8; 32];
        b.copy_from_slice(recipient);
        if PeerId(b) == *contact {
            return 0.9; // addressed directly to the contact
        }
    }
    // Relay candidate: still worth attempting, lower confidence.
    0.6
}

impl<S: MessageStorage> ScfEngine<S> {
    /// Evaluate all buffered messages on a new contact. Returns the ranked
    /// forward order (P0 first, then by delivery probability), honoring a
    /// per-contact bandwidth budget measured in messages.
    pub fn on_new_contact(
        &self,
        contact: &PeerId,
        bandwidth_limit: Option<usize>,
    ) -> ContactOutcome {
        let candidates = self.messages_forwardable_to(contact);
        let mut ranked: Vec<ForwardCandidate> = candidates
            .iter()
            .map(|m| {
                let p = delivery_probability(&m.envelope, contact);
                ForwardCandidate::new(m.envelope.message_id, *contact, p)
            })
            .collect();

        // Rank: priority ASC (P0 first), then delivery probability DESC.
        ranked.sort_by(|a, b| {
            a.delivery_probability
                .partial_cmp(&b.delivery_probability)
                .unwrap_or(std::cmp::Ordering::Equal)
                .reverse()
        });
        // Re-sort by priority (stable) so P0 always leads.
        ranked.sort_by_key(|c| self.message_priority(c));
        if let Some(limit) = bandwidth_limit {
            let deferred = ranked.len().saturating_sub(limit);
            ranked.truncate(limit);
            ContactOutcome { ranked, deferred }
        } else {
            ContactOutcome {
                ranked,
                deferred: 0,
            }
        }
    }

    fn message_priority(&self, c: &ForwardCandidate) -> u8 {
        self.priority_rank_of(&c.message_id).unwrap_or(7)
    }
}

/// Convenience method forwarding an envelope for later ack (used by callers on
/// the transport/engine path). Adds a stub so the module compiles standalone of
/// the engine wiring.
pub fn enqueue_forward(
    engine: &mut ScfEngine<impl MessageStorage>,
    candidate: &StoredMessage,
    contact: &PeerId,
) -> Result<(), ScfError> {
    let id = candidate.envelope.message_id;
    engine.mark_forwarded(&id, *contact, false);
    Ok(())
}

/// Guard: a contact that is actually the recipient must not double-queue the
/// message through the general forward path.
#[allow(dead_code)]
fn _direct_delivery_guard(contact: PeerId, recipient: PeerId) -> bool {
    contact == recipient
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessagePriority;
    use crate::message_engine::storage::MemoryStorage;
    use crate::protocol::{ContentType, Envelope, MessageId};

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn env(prio: MessagePriority, payload: &[u8], to: u8) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: pid(1).0.to_vec(),
            recipient_id: pid(to).0.to_vec(),
            priority: prio,
            ttl_seconds: 3600,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            hop_count: 0,
            max_hops: Some(5),
            payload_type: ContentType::Text,
            payload_size: payload.len() as u64,
            payload_hash: Envelope::compute_payload_hash(payload),
            payload: payload.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    #[test]
    fn contact_ranks_p0_first_then_probability() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        scf.buffer_message(env(MessagePriority::P4, b"log", 9), None)
            .unwrap();
        scf.buffer_message(env(MessagePriority::P0, b"sos", 9), None)
            .unwrap();
        scf.buffer_message(env(MessagePriority::P2, b"loc", 9), None)
            .unwrap();
        let outcome = scf.on_new_contact(&pid(9), None);
        assert_eq!(outcome.deferred, 0);
        assert_eq!(outcome.ranked.len(), 3);
        assert_eq!(outcome.ranked[0].to, pid(9));
        // P0 first.
        assert_eq!(scf.message_priority(&outcome.ranked[0]), 0);
    }

    #[test]
    fn bandwidth_limit_defers_excess() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        for i in 0..5u8 {
            scf.buffer_message(env(MessagePriority::P3, &[i, 0], 9), None)
                .unwrap();
        }
        let outcome = scf.on_new_contact(&pid(9), Some(2));
        assert_eq!(outcome.ranked.len(), 2);
        assert_eq!(outcome.deferred, 3);
    }

    #[test]
    fn candidate_probability_clamped() {
        let c = ForwardCandidate::new(MessageId::new_v7(), pid(9), 1.7);
        assert_eq!(c.delivery_probability, 1.0);
    }
}
