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
    // PM-16: max_chunk=0 caused rest.len().min(0)=0 → no progress → infinite loop.
    let max_chunk = max_chunk.max(1);
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

/// Conservative CBOR envelope encoding overhead per fragment (all non-payload
/// fields: map header, message_id, sender/recipient ids, priority, ttl,
/// timestamp, hop_count, payload_type, payload_size, payload_hash,
/// payload_ref, signature, plus the CBOR byte-array length prefix for the
/// payload field itself).  Used in MTU budgeting so encoded fragments always
/// fit within the transport MTU — without this the payload chunk fills the MTU
/// leaving no room for the envelope wrapper.
pub const ENVELOPE_CODEC_OVERHEAD: usize = 300;

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

/// Maximum concurrently-tracked partial fragment sets.
///
/// `original_id` is attacker-chosen, so without a ceiling a peer streaming
/// distinct first-fragments grows the table at `rate x timeout x MTU` with no
/// bound. Dedup does not help — each wire envelope in that stream carries a
/// distinct `message_id`.
const MAX_ACTIVE_SETS: usize = 256;

/// PM-4: per-sender cap on concurrent partial fragment sets.
///
/// Without this, one sender could claim all 256 slots with distinct original_ids
/// that never complete, starving every other sender of reassembly capacity.
const MAX_SETS_PER_SENDER: usize = 4;

/// Reassembly tracker: original message_id → fragment set.
#[derive(Debug)]
pub struct FragmentAssembler {
    active: HashMap<MessageId, FragmentSet>,
    /// Default reassembly window.
    timeout: Duration,
    /// PM-4: per-sender open-set counter (sender_id bytes → count).
    per_sender: HashMap<Vec<u8>, usize>,
}

impl Default for FragmentAssembler {
    fn default() -> Self {
        Self::new(DEFAULT_FRAGMENT_TIMEOUT)
    }
}

impl FragmentAssembler {
    pub fn new(timeout: Duration) -> Self {
        FragmentAssembler {
            active: HashMap::new(),
            timeout,
            per_sender: HashMap::new(),
        }
    }

    fn decrement_sender(&mut self, sender: &[u8]) {
        if let Some(count) = self.per_sender.get_mut(sender) {
            if *count <= 1 {
                self.per_sender.remove(sender);
            } else {
                *count -= 1;
            }
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
        // A fragment set must have at least two members — a single-chunk
        // "fragment" is a whole message and should never have been fragmented.
        //
        // Without this floor, one packet claiming `total <= 1` reassembles
        // instantly (for `total == 0` the completeness range is vacuously
        // satisfied, yielding an empty payload). `feed` then stamps the
        // reassembled envelope with `hdr.original_id` — a value read straight
        // out of the attacker-controlled fragment body — and the caller
        // registers that id in the dedup engine. That let one packet tombstone
        // any message id in the 2^128 space, including ids for messages that
        // had not been sent yet, silently censoring them.
        if hdr.total < 2 {
            return None;
        }
        if hdr.index >= hdr.total {
            return None; // index outside the declared set
        }
        let now = Instant::now();
        self.gc_expired(now);
        let deadline = now + self.timeout;

        let is_new_set = !self.active.contains_key(&hdr.original_id);

        // PM-4 part 1: per-sender quota. Before a new set is opened, check
        // whether this sender is at their per-sender cap. If so, evict their
        // oldest (nearest-deadline) set first rather than the global victim —
        // a sender that floods only hurts themselves, not other senders.
        if is_new_set {
            let sender_count = self
                .per_sender
                .get(&fragment.sender_id)
                .copied()
                .unwrap_or(0);
            if sender_count >= MAX_SETS_PER_SENDER {
                if let Some(victim) = self
                    .active
                    .iter()
                    .filter(|(_, s)| s.sender.as_deref() == Some(fragment.sender_id.as_slice()))
                    .min_by_key(|(_, s)| s.deadline)
                    .map(|(id, _)| *id)
                {
                    let victim_sender = self.active[&victim].sender.clone();
                    self.active.remove(&victim);
                    if let Some(s) = victim_sender {
                        self.decrement_sender(&s);
                    }
                }
            }
        }

        // Bound the partial-set table. `original_id` is attacker-chosen, so a
        // peer streaming distinct first-fragments — each opening a new set that
        // never completes — grew this map at `rate x timeout x MTU` bytes with
        // no ceiling at all. Dedup does not help: every wire envelope in that
        // stream carries a distinct `message_id`.
        //
        // When full, evict the set closest to its own deadline: it is the one
        // about to be discarded anyway, so this degrades to normal expiry under
        // pressure rather than dropping a set that just started arriving.
        if !self.active.contains_key(&hdr.original_id) && self.active.len() >= MAX_ACTIVE_SETS {
            if let Some(victim) = self
                .active
                .iter()
                .min_by_key(|(_, s)| s.deadline)
                .map(|(id, _)| *id)
            {
                let victim_sender = self.active[&victim].sender.clone();
                self.active.remove(&victim);
                // PM-4: keep per_sender in sync when the global cap evicts a set.
                if let Some(s) = victim_sender {
                    self.decrement_sender(&s);
                }
            }
        }

        // Scope the `set` borrow so self.per_sender can be accessed below.
        let complete_payload = {
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
            set.add(hdr.index, chunk.to_vec())
            // `set` borrow ends here at block close
        };

        // PM-4: track the new set after the borrow of self.active has ended.
        // A fresh set always binds (sender was None), so is_new_set → increment
        // is always reached when is_new_set = true.
        if is_new_set {
            *self
                .per_sender
                .entry(fragment.sender_id.clone())
                .or_insert(0) += 1;
        }

        let mut payload = complete_payload?; // None → FragmentBuffered (per_sender updated)

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
        // PM-4: set is complete and removed — decrement sender counter.
        self.decrement_sender(&fragment.sender_id);
        Some(original)
    }

    /// Drop fragment sets past their deadline; returns how many were dropped.
    pub fn gc_expired(&mut self, now: Instant) -> usize {
        let before = self.active.len();
        // PM-4: collect senders of expiring sets before retain() removes them.
        let expired_senders: Vec<Vec<u8>> = self
            .active
            .values()
            .filter(|s| s.deadline <= now)
            .filter_map(|s| s.sender.clone())
            .collect();
        self.active.retain(|_, s| s.deadline > now);
        for sender in expired_senders {
            self.decrement_sender(&sender);
        }
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

    #[test]
    fn single_chunk_fragment_cannot_forge_a_reassembled_message() {
        // Regression: a lone fragment declaring `total <= 1` completed
        // instantly and stamped the result with an attacker-chosen
        // `original_id`, which the caller then registered in dedup. One packet
        // could tombstone any message id — including ids not yet used — and
        // silently censor the real message when it arrived.
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let victim = MessageId::from_bytes([0x99; 16]);

        assert!(
            a.feed(mk_fragment(0, 1, victim, b"forged")).is_none(),
            "total = 1 must not reassemble"
        );
        assert!(
            a.feed(mk_fragment(0, 0, victim, b"")).is_none(),
            "total = 0 must not reassemble (range is vacuously complete)"
        );
        assert!(
            a.feed(mk_fragment(3, 2, victim, b"x")).is_none(),
            "index outside the declared set must be refused"
        );
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

    // PM-4: a single sender cannot hold more than MAX_SETS_PER_SENDER concurrent
    // reassembly slots; other senders are not affected.
    #[test]
    fn per_sender_quota_evicts_own_oldest_set() {
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let id = |n: u8| MessageId::from_bytes([n; 16]);

        let mut mk = |sender: u8, orig: u8, index: u16| {
            let mut e = mk_fragment(index, 2, id(orig), b"x");
            e.sender_id = vec![sender];
            e
        };

        // Sender A opens MAX_SETS_PER_SENDER sets (first fragments only)
        for n in 0..MAX_SETS_PER_SENDER as u8 {
            assert!(a.feed(mk(0xAA, n, 0)).is_none());
        }
        assert_eq!(
            a.per_sender.get(&vec![0xAA]).copied().unwrap_or(0),
            MAX_SETS_PER_SENDER
        );

        // Opening one more set from sender A evicts A's oldest (not any other sender's)
        assert!(a.feed(mk(0xAA, 0xFF, 0)).is_none());
        // Sender A still at cap (evicted one, added one)
        assert_eq!(
            a.per_sender.get(&vec![0xAA]).copied().unwrap_or(0),
            MAX_SETS_PER_SENDER
        );

        // Sender B is unaffected — can still open sets
        assert!(a.feed(mk(0xBB, 0x10, 0)).is_none());
        assert_eq!(a.per_sender.get(&vec![0xBB]).copied().unwrap_or(0), 1);
    }

    // PM-4: completing a set decrements the per-sender counter.
    #[test]
    fn per_sender_counter_decrements_on_completion() {
        let mut a = FragmentAssembler::new(Duration::from_secs(30));
        let orig_id = MessageId::from_bytes([0xCC; 16]);
        let payload = orig_payload();
        let parts = split_payload(&payload, 260);

        let mut f0 = mk_fragment(0, 2, orig_id, &parts[0]);
        f0.sender_id = vec![0xCC];
        let mut f1 = mk_fragment(1, 2, orig_id, &parts[1]);
        f1.sender_id = vec![0xCC];

        assert!(a.feed(f0).is_none());
        assert_eq!(a.per_sender.get(&vec![0xCC]).copied().unwrap_or(0), 1);
        let done = a.feed(f1).expect("should reassemble");
        assert_eq!(done.payload, payload);
        // Counter must be zero after completion
        assert_eq!(a.per_sender.get(&vec![0xCC]).copied().unwrap_or(0), 0);
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
