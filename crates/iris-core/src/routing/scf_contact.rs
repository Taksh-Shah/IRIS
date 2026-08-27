//! SCF contact-opportunity handler — SCF-001 (WP-5).
//!
//! Per STORE_CARRY_FORWARD.md §Contact Opportunity Handler: when a new
//! neighbor is discovered (DISCO-001 event), evaluate all buffered messages,
//! rank forwardable candidates (priority first, then delivery probability),
//! and enqueue forwards respecting transport bandwidth.

use crate::message::PeerId;
use crate::message_engine::storage::MessageStorage;
use crate::routing::scf::{ScfEngine, ScfError};

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
    /// DTN-23: sourced directly from the envelope at candidate-construction
    /// time. Previously ranking re-derived this via a separate by-id
    /// buffer lookup (`ScfEngine::priority_rank_of`) that (a) cost an
    /// extra traversal per candidate and (b) silently demoted a message to
    /// "least urgent" (`unwrap_or(7)`) if the lookup ever failed to find
    /// it, rather than failing loudly.
    pub priority_rank: u8,
    /// 0..1 estimate that forwarding to this contact ultimately delivers.
    pub delivery_probability: f64,
    /// DTN-25: this message's accounted byte size
    /// (`ScfEngine::approx_bytes`) — what `on_new_contact`'s
    /// `bandwidth_limit_bytes` actually budgets against, instead of a raw
    /// message count blind to size.
    pub approx_bytes: u64,
}

impl ForwardCandidate {
    pub fn new(
        message_id: crate::protocol::MessageId,
        to: PeerId,
        priority_rank: u8,
        p: f64,
        approx_bytes: u64,
    ) -> Self {
        ForwardCandidate {
            message_id,
            to,
            priority_rank,
            delivery_probability: if p.is_finite() {
                p.clamp(0.0, 1.0)
            } else {
                0.0
            },
            approx_bytes,
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
    /// per-contact bandwidth budget measured in bytes.
    ///
    /// DTN-25: `bandwidth_limit_bytes` used to be a raw message COUNT
    /// (`Option<usize>`), consulting neither `approx_bytes`,
    /// `payload.len()`, the link's throughput, nor the predicted contact
    /// duration — two messages could be 200 bytes or 200 MB and cost the
    /// budget identically. Now bytes, truncating once the running total
    /// would exceed the limit rather than after a fixed count.
    pub fn on_new_contact(
        &self,
        contact: &PeerId,
        bandwidth_limit_bytes: Option<u64>,
    ) -> ContactOutcome {
        let candidates = self.messages_forwardable_to(contact);
        let mut ranked: Vec<ForwardCandidate> = candidates
            .iter()
            .map(|m| {
                let p = delivery_probability(&m.envelope, contact);
                ForwardCandidate::new(
                    m.envelope.message_id,
                    *contact,
                    m.envelope.priority.as_u8(),
                    p,
                    Self::approx_bytes(&m.envelope),
                )
            })
            .collect();

        // DTN-23: one explicit, fully deterministic comparator — priority
        // ASC (P0 first), then delivery probability DESC, then message_id
        // as a final tiebreak — instead of two separate sorts, the second
        // of which re-derived priority via a fallible O(n) by-id lookup
        // per candidate (`unwrap_or(7)` silently demoting anything it
        // could not find) rather than reading it off the candidate itself.
        ranked.sort_by(|a, b| {
            a.priority_rank.cmp(&b.priority_rank).then_with(|| {
                b.delivery_probability
                    .partial_cmp(&a.delivery_probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.message_id.cmp(&b.message_id))
            })
        });
        match bandwidth_limit_bytes {
            Some(limit) => {
                let mut used = 0u64;
                let mut cut = ranked.len();
                for (i, c) in ranked.iter().enumerate() {
                    if used.saturating_add(c.approx_bytes) > limit {
                        cut = i;
                        break;
                    }
                    used += c.approx_bytes;
                }
                let deferred = ranked.len() - cut;
                ranked.truncate(cut);
                ContactOutcome { ranked, deferred }
            }
            None => ContactOutcome {
                ranked,
                deferred: 0,
            },
        }
    }
}

/// Convenience method forwarding an envelope for later ack (used by callers on
/// the transport/engine path). Adds a stub so the module compiles standalone of
/// the engine wiring.
/// DTN-24: the previous version dropped `mark_forwarded`'s `Option` (its
/// only failure signal — `None` when the message is no longer in the
/// buffer) and unconditionally returned `Ok(())`, making the `Result` a lie
/// every caller's `?` would silently trust. `None` is real and reachable
/// here: `candidate` is typically a clone taken from
/// `messages_forwardable_to` earlier, so the underlying entry can have
/// been reaped or evicted in between. Takes `&MessageId` rather than a
/// whole cloned `StoredMessage` — the caller need not pay for a second
/// full-envelope clone just to name which message to mark.
pub fn enqueue_forward(
    engine: &mut ScfEngine<impl MessageStorage>,
    message_id: &crate::protocol::MessageId,
    contact: &PeerId,
) -> Result<crate::routing::scf::DeliveryStatus, ScfError> {
    engine
        .mark_forwarded(message_id, *contact, false)
        .ok_or(ScfError::NotFound)
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
        assert_eq!(outcome.ranked[0].priority_rank, 0);
    }

    #[test]
    fn dtn23_ranking_mixes_exact_match_and_relay_candidates_by_priority_then_probability() {
        // DTN-23 + DTN-4 interaction: now that messages_forwardable_to
        // includes relay candidates (not just exact-recipient matches,
        // DTN-4), delivery_probability is no longer uniformly 0.9 — a
        // same-priority contact must rank the exact-match candidate (0.9)
        // ahead of the relay-only one (0.6).
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        let exact = env(MessagePriority::P4, b"to-9", 9);
        scf.buffer_message(exact.clone(), None).unwrap();
        let relay = env(MessagePriority::P4, b"to-other", 8);
        scf.buffer_message(relay.clone(), None).unwrap();

        let outcome = scf.on_new_contact(&pid(9), None);
        assert_eq!(outcome.ranked.len(), 2);
        assert_eq!(outcome.ranked[0].message_id, exact.message_id);
        assert_eq!(outcome.ranked[0].delivery_probability, 0.9);
        assert_eq!(outcome.ranked[1].message_id, relay.message_id);
        assert_eq!(outcome.ranked[1].delivery_probability, 0.6);
    }

    #[test]
    fn bandwidth_limit_defers_excess() {
        // DTN-25: bandwidth_limit_bytes is bytes, not a message count. Five
        // 2-byte-payload P3 messages each account for
        // FIXED_OVERHEAD_BYTES(192) + 32 + 32 + 2 = 258 bytes; a 516-byte
        // budget fits exactly two.
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        for i in 0..5u8 {
            scf.buffer_message(env(MessagePriority::P3, &[i, 0], 9), None)
                .unwrap();
        }
        let outcome = scf.on_new_contact(&pid(9), Some(516));
        assert_eq!(outcome.ranked.len(), 2);
        assert_eq!(outcome.deferred, 3);
    }

    #[test]
    fn dtn25_bandwidth_limit_is_size_aware_not_count_aware() {
        // A single large message and several tiny ones must not cost the
        // budget identically — the old count-based limit could not tell
        // them apart.
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        // Lower priority (P7) than the tiny messages (P4) so ranking puts
        // it last regardless of message_id tiebreak — isolates the byte
        // accounting this test is actually about.
        let big = env(MessagePriority::P7, &[0u8; 10_000], 9);
        scf.buffer_message(big.clone(), None).unwrap();
        for i in 0..3u8 {
            scf.buffer_message(env(MessagePriority::P4, &[i], 9), None)
                .unwrap();
        }
        // Budget large enough for the three tiny messages but not the
        // 10 KB one — a count-based limit of "1" would have picked the big
        // message and stopped there; a byte-based limit correctly skips it.
        let outcome = scf.on_new_contact(&pid(9), Some(1000));
        assert!(
            !outcome.ranked.iter().any(|c| c.message_id == big.message_id),
            "the 10 KB message must not fit a 1000-byte budget"
        );
        assert_eq!(outcome.deferred, 1);
    }

    #[test]
    fn candidate_probability_clamped() {
        let c = ForwardCandidate::new(MessageId::new_v7(), pid(9), 4, 1.7, 0);
        assert_eq!(c.delivery_probability, 1.0);
    }

    #[test]
    fn dtn24_enqueue_forward_reports_not_found_for_missing_message() {
        // Regression: the old signature always returned Ok(()), even for a
        // message id no longer in the buffer (e.g. reaped/evicted between
        // candidate selection and the actual forward attempt) — every
        // caller's `?` was dead code.
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        let missing_id = MessageId::new_v7();
        let result = enqueue_forward(&mut scf, &missing_id, &pid(9));
        assert_eq!(result, Err(ScfError::NotFound));
    }

    #[test]
    fn dtn24_enqueue_forward_returns_the_resulting_status() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        let e = env(MessagePriority::P4, b"relay-me", 9);
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();
        let result = enqueue_forward(&mut scf, &id, &pid(9));
        assert_eq!(
            result,
            Ok(crate::routing::scf::DeliveryStatus::AckPending { sent_to: pid(9) })
        );
    }
}
