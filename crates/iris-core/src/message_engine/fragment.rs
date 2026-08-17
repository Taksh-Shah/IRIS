//! Fragment reassembly — MSG-001 (R8: two-fragment model + DoS guard).
//!
//! Fragmentation only applies to P4–P7 bulk payloads (images/voice/video/file)
//! that exceed a transport's MTU. ACK/custody/control envelopes and **P0–P3**
//! are hard-excluded (R8 + INV-EMERG-002 — a P0 must never be split and
//! reassembled across contacts). Max 2 fragments per ADU.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::protocol::{ContentType, Envelope, MessageId};

/// Maximum fragments per ADU (R8).
pub const MAX_FRAGMENTS: u16 = 2;
/// Default reassembly window (MSG_DESIGN.md: 30 s).
pub const DEFAULT_FRAGMENT_TIMEOUT: Duration = Duration::from_secs(30);

/// Derive a **distinct** wire `message_id` for one fragment.
///
/// All fragments of an ADU must carry distinct message_ids, otherwise the
/// receiver's dedup (keyed on `message_id`) consumes fragment 0 and drops
/// fragment 1 as a duplicate (RED-0004 part 1). The *original* id still
/// travels inside the fragment payload header, and reassembly restores it on
/// the reconstructed envelope. Deterministic (original ‖ index → SHA-256
/// truncated to 16 bytes) so retransmits of the same fragment dedup cleanly.
pub fn derive_fragment_message_id(original: &MessageId, index: u16) -> MessageId {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(original.0);
    h.update(index.to_le_bytes());
    let digest = h.finalize();
    let mut id = [0u8; 16];
    id.copy_from_slice(&digest[..16]);
    MessageId::from_bytes(id)
}

/// Whether an envelope is allowed to be fragmented.
///
/// Control planes (ACK, gossip, capability…) and P0–P3 may never fragment.
pub fn is_fragmentable(envelope: &Envelope) -> bool {
    !matches!(
        envelope.payload_type,
        ContentType::Ack
            | ContentType::RoutingGossip
            | ContentType::Capability
            | ContentType::SyncRequest
            | ContentType::GroupKey
            | ContentType::KeyRotation
            | ContentType::EmergencyAlert
    ) && matches!(
        envelope.priority,
        crate::message::MessagePriority::P4
            | crate::message::MessagePriority::P5
            | crate::message::MessagePriority::P6
            | crate::message::MessagePriority::P7
    ) && !envelope.is_p0_abbreviated()
}

/// Split a frame payload into chunks of at most `mtu-effective` bytes.
pub fn split_payload(payload: &[u8], max_chunk: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = payload;
    while !rest.is_empty() {
        let take = rest.len().min(max_chunk);
        out.push(rest[..take].to_vec());
        rest = &rest[take..];
    }
    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

// Fragment envelope layout: payload = [frag_index u16 LE][total u16 LE]
//                                   [original_id 16 B][orig_payload_type u8]
//                                   [has_sig u8][orig_signature 64 B]
//                                   [chunk bytes]
//
// `orig_payload_type` is carried so reassembly can restore `payload_type`
// (AAD field 10) and `payload_ref` (field 14, always None for fragments) —
// without it the sender's AAD (computed over the *sealed* fields) would not
// match the reassembled envelope and E2EE decrypt would fail (RED-0004).
//
// `orig_signature`: the sealed envelope's Ed25519 signature covers the whole
// (encrypted) payload, but per-fragment re-signing overwrites it on each
// fragment. Carrying it in the header lets reassembly restore a signature
// that authenticates the FULL reassembled ADU (RED-0004 part 3 / RED-0007).

/// Serialized fragment-header overhead (everything before the chunk bytes).
pub const FRAGMENT_HEADER_LEN: usize = 86;

pub fn encode_fragment(
    index: u16,
    total: u16,
    original: MessageId,
    original_type: ContentType,
    original_signature: Option<[u8; 64]>,
    chunk: &[u8],
) -> Vec<u8> {
    let mut v = Vec::with_capacity(FRAGMENT_HEADER_LEN + chunk.len());
    v.extend_from_slice(&index.to_le_bytes());
    v.extend_from_slice(&total.to_le_bytes());
    v.extend_from_slice(&original.0);
    v.push(original_type.as_u8());
    match original_signature {
        Some(sig) => {
            v.push(1);
            v.extend_from_slice(&sig);
        }
        None => {
            v.push(0);
            v.extend_from_slice(&[0u8; 64]);
        }
    }
    v.extend_from_slice(chunk);
    v
}

pub struct FragmentHeader {
    pub index: u16,
    pub total: u16,
    pub original_id: MessageId,
    pub original_type: ContentType,
    pub original_signature: Option<[u8; 64]>,
}

fn decode_fragment(payload: &[u8]) -> Option<(FragmentHeader, &[u8])> {
    if payload.len() < FRAGMENT_HEADER_LEN {
        return None;
    }
    let index = u16::from_le_bytes([payload[0], payload[1]]);
    let total = u16::from_le_bytes([payload[2], payload[3]]);
    let mut id = [0u8; 16];
    id.copy_from_slice(&payload[4..20]);
    let original_type = ContentType::from_u8(payload[20])?;
    let has_sig = payload[21];
    let mut sig = [0u8; 64];
    sig.copy_from_slice(&payload[22..86]);
    let original_signature = if has_sig == 1 { Some(sig) } else { None };
    let hdr = FragmentHeader {
        index,
        total,
        original_id: MessageId::from_bytes(id),
        original_type,
        original_signature,
    };
    Some((hdr, &payload[FRAGMENT_HEADER_LEN..]))
}

/// In-progress fragment set for one original message.
#[derive(Debug, Clone)]
pub struct FragmentSet {
    pub total: u16,
    pub deadline: Instant,
    pub chunks: HashMap<u16, Vec<u8>>,
    /// `sender_id` of the first fragment seen — a cheap mixed-signer guard
    /// (RED-0007). Fragments from a different sender are refused.
    pub sender: Option<Vec<u8>>,
}

impl FragmentSet {
    fn new(total: u16, deadline: Instant) -> Self {
        FragmentSet {
            total,
            deadline,
            chunks: HashMap::with_capacity(total.min(MAX_FRAGMENTS) as usize),
            sender: None,
        }
    }

    /// Bind the fragment-set to its first-observed signer; `None` when a
    /// conflicting signer already owns the set (mix-public-key rejection).
    pub fn bind_sender(&mut self, sender: &[u8]) -> bool {
        match &self.sender {
            None => {
                self.sender = Some(sender.to_vec());
                true
            }
            Some(existing) => existing == sender,
        }
    }

    /// Returns the reassembled payload when all chunks are present.
    pub fn add(&mut self, index: u16, chunk: Vec<u8>) -> Option<Vec<u8>> {
        self.chunks.insert(index, chunk);
        if self.total > MAX_FRAGMENTS {
            return None; // R8: never reassemble >2 fragments
        }
        if (0..self.total).all(|i| self.chunks.contains_key(&i)) {
            let mut out = Vec::new();
            for i in 0..self.total {
                out.extend(&self.chunks[&i]);
            }
            Some(out)
        } else {
            None
        }
    }
}

/// Reassembly tracker: original message_id → fragment set.
#[derive(Debug, Default)]
pub struct FragmentAssembler {
    active: HashMap<MessageId, FragmentSet>,
    /// Default reassembly window.
    timeout: Duration,
}

impl FragmentAssembler {
    pub fn new(timeout: Duration) -> Self {
        FragmentAssembler {
            active: HashMap::new(),
            timeout,
        }
    }

    /// Feed one fragment envelope. Returns `Some(original envelope)` with the
    /// reassembled payload when complete, or `None` while incomplete/refused.
    ///
    /// Restores the AAD-relevant fields that fragmentation mutated
    /// (RED-0004): `message_id`, `payload_type`, `payload_ref` (=None,
    /// fragments never carry a ref), `payload_size` and `payload_hash` are set
    /// from the fragment header + reassembled payload so the recipient's
    /// `encode_for_aead` matches the sender's encryption-time AAD.
    pub fn feed(&mut self, fragment: Envelope) -> Option<Envelope> {
        let (hdr, chunk) = decode_fragment(&fragment.payload)?;
        if hdr.total > MAX_FRAGMENTS {
            return None; // R8 DoS guard
        }
        let now = Instant::now();
        self.gc_expired(now);
        let deadline = now + self.timeout;
        let set = self
            .active
            .entry(hdr.original_id)
            .or_insert_with(|| FragmentSet::new(hdr.total, deadline));
        if !set.bind_sender(&fragment.sender_id) {
            return None; // RED-0007: mixed-signer fragment set refused
        }
        if set.total != hdr.total {
            return None; // inconsistent total across fragments
        }

        let mut payload = set.add(hdr.index, chunk.to_vec())?;
        // Reconstruct the original (sealed) envelope: restore message_id,
        // payload_type, payload_ref (never set on a fragment), payload_size,
        // payload_hash AND the original whole-ADU signature (RED-0004 part 3 /
        // RED-0007) so the caller can re-verify the reassembled envelope.
        let mut original = fragment.clone();
        original.message_id = hdr.original_id;
        original.payload_type = hdr.original_type;
        original.payload_ref = None;
        original.signature = hdr.original_signature;
        original.payload.clear();
        original.payload.append(&mut payload);
        original.payload_size = original.payload.len() as u64;
        original.payload_hash = Envelope::compute_payload_hash(&original.payload);
        self.active.remove(&hdr.original_id);
        Some(original)
    }

    /// Drop fragment sets past their deadline; returns how many were dropped.
    pub fn gc_expired(&mut self, now: Instant) -> usize {
        let before = self.active.len();
        self.active.retain(|_, s| s.deadline > now);
        before - self.active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessagePriority;
    use crate::protocol::MessageId;

    fn orig_payload() -> Vec<u8> {
        (0..500u16).map(|i| (i % 251) as u8).collect()
    }

    fn mk_fragment(index: u16, total: u16, id: MessageId, chunk: &[u8]) -> Envelope {
        Envelope {
            version: 1,
            message_id: MessageId::from_bytes([0xAA; 16]),
            sender_id: vec![1],
            recipient_id: vec![2],
            priority: MessagePriority::P5,
            ttl_seconds: 3600,
            timestamp: 1_752_000_000,
            hop_count: 1,
            max_hops: None,
            payload_type: ContentType::Fragment,
            payload_size: (FRAGMENT_HEADER_LEN + chunk.len()) as u64,
            payload_hash: [0; 32],
            payload: encode_fragment(index, total, id, ContentType::Image, Some([7; 64]), chunk),
            payload_ref: Some(id),
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    #[test]
    fn reassembles_two_fragment_adu() {
        let id = MessageId::from_bytes([0x11; 16]);
        let payload = orig_payload();
        let parts = split_payload(&payload, 260);
        assert_eq!(parts.len(), 2);
        let mut a = FragmentAssembler::new(Duration::from_secs(30));

        let f0 = mk_fragment(0, 2, id, &parts[0]);
        assert!(a.feed(f0).is_none(), "one fragment alone is incomplete");
        let f1 = mk_fragment(1, 2, id, &parts[1]);
        let done = a.feed(f1).expect("both fragments reassemble");
        assert_eq!(done.payload, payload);
    }

    // RED-0004: reassembly must restore the AAD-relevant fields the sender
    // encrypted over — message_id, payload_type (field 10), payload_ref (must
    // be None for the original sealed envelope) — or ChaCha20-Poly1305 AAD
    // verification would fail on every fragmented E2EE message.
    #[test]
    fn reassembly_restores_aad_fields_from_fragment_header() {
        let id = MessageId::from_bytes([0x44; 16]);
        let parts = split_payload(b"0123456789abcdef012345", 12);
        assert_eq!(parts.len(), 2);
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        assert!(a.feed(mk_fragment(0, 2, id, &parts[0])).is_none());
        let done = a
            .feed(mk_fragment(1, 2, id, &parts[1]))
            .expect("reassembled");

        assert_eq!(done.message_id, id, "message_id restored");
        assert_eq!(
            done.payload_type,
            ContentType::Image,
            "payload_type restored"
        );
        assert_eq!(done.payload_ref, None, "payload_ref cleared (field 14)");
        assert_eq!(
            done.signature,
            Some([7; 64]),
            "original whole-ADU signature restored"
        );
        assert_eq!(done.payload_size, done.payload.len() as u64);
        assert_eq!(
            done.payload_hash,
            Envelope::compute_payload_hash(&done.payload)
        );
    }

    #[test]
    fn refuses_more_than_max_fragments() {
        let id = MessageId::from_bytes([0x22; 16]);
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let f0 = mk_fragment(0, 5, id, b"x"); // total = 5 > MAX 2
        assert!(a.feed(f0).is_none(), "over-split ADU refused (R8)");
    }

    // RED-0007: a fragment set is rejected when fragments arrive from
    // DIFFERENT signers — the reassembled envelope must not blend authorship.
    #[test]
    fn mixed_signer_fragment_sets_are_refused() {
        let id = MessageId::from_bytes([0x55; 16]);
        let parts = split_payload(b"mixed-signer rejection test payload", 14);
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let f0 = mk_fragment(0, 2, id, &parts[0]);
        let mut f1 = mk_fragment(1, 2, id, &parts[1]);
        f1.sender_id = vec![0xEE]; // different signer
        assert!(a.feed(f0).is_none(), "first fragment accepted");
        assert!(a.feed(f1).is_none(), "mixed-signer completion refused");
    }

    // RED-0004 part 1: distinct wire ids per fragment so dedup never consumes
    // fragment 1 as a duplicate of fragment 0.
    #[test]
    fn wire_fragment_ids_are_distinct_and_deterministic() {
        let id = MessageId::from_bytes([0x66; 16]);
        let f0 = derive_fragment_message_id(&id, 0);
        let f1 = derive_fragment_message_id(&id, 1);
        assert_ne!(f0, f1, "distinct fragments must not share a wire id");
        assert_ne!(f0, id, "fragment id must differ from the original id");
        // Deterministic: retransmit of the same fragment dedups cleanly.
        assert_eq!(derive_fragment_message_id(&id, 0), f0);
    }

    #[test]
    fn timeout_drops_partial_sets() {
        let id = MessageId::from_bytes([0x33; 16]);
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let f0 = mk_fragment(0, 2, id, b"first-half");
        assert!(a.feed(f0).is_none());
        let dropped = a.gc_expired(Instant::now() + Duration::from_secs(31));
        assert_eq!(dropped, 1);
        assert_eq!(a.active.len(), 0);
    }

    #[test]
    fn control_and_p0_never_fragment() {
        let mut e = Envelope {
            version: 1,
            message_id: MessageId::from_bytes([4u8; 16]),
            sender_id: vec![1],
            recipient_id: vec![2],
            priority: MessagePriority::P0,
            ttl_seconds: 300,
            timestamp: 1_752_000_000,
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Sos,
            payload_size: 30,
            payload_hash: [0; 32],
            payload: vec![0; 30],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        assert!(!is_fragmentable(&e));
        // An ACK must never fragment (R6/R8).
        e.priority = MessagePriority::P4;
        e.payload_type = ContentType::Ack;
        assert!(!is_fragmentable(&e));
    }
}
