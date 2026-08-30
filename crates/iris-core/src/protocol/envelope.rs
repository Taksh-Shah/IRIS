//! Envelope wire structure (MESSAGE_ENVELOPE.md fields 1–18).

use crate::message::MessagePriority;

use super::content_type::ContentType;
use super::message_id::MessageId;

/// Maximum size of a P0 SOS envelope on LoRa (physical payload limit).
pub const P0_MAX_ENVELOPE_BYTES: usize = 255;

/// Encryption header (MESSAGE_ENVELOPE.md field 16).
///
/// Per-message ephemeral X25519 key exchange, one-pass ECIES model
/// (RES-0009 R-a1 — per-message ephemeral keys for v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncryptionHdr {
    /// X25519 ephemeral public key (32 bytes).
    pub ephemeral_pubkey: [u8; 32],
    /// ChaCha20-Poly1305 nonce (12 bytes, random).
    pub nonce: [u8; 12],
    /// Recipient key id (4 bytes, optional — present when recipient rotates keys).
    pub key_id: Option<[u8; 4]>,
}

/// Routing hints (MESSAGE_ENVELOPE.md field 17) — untrusted, gossip-quality.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoutingHints {
    pub last_known_region: Option<String>,
    pub gateway_seen_via: Option<[u8; 32]>,
    pub delivery_prob: Option<f64>,
    pub requires_gateway: Option<bool>,
    pub preferred_relays: Option<Vec<[u8; 32]>>,
}

/// The IRIS message envelope — the atomic unit of the protocol.
///
/// A flat CBOR map with integer keys 1–18 (`MESSAGE_ENVELOPE.md`). Relays can
/// read and validate every field but never modify fields covered by the
/// signature (all except `hop_count` 8, signature 15, and extensions 16+).
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    /// 1: protocol version (must be `PROTOCOL_VERSION`).
    pub version: u8,
    /// 2: message id — 16-byte UUIDv7.
    pub message_id: MessageId,
    /// 3: sender node id — 32 bytes (16 bytes in P0 abbreviated form).
    pub sender_id: Vec<u8>,
    /// 4: recipient — 32 bytes, or 4 bytes for `EMERGENCY_BROADCAST`.
    pub recipient_id: Vec<u8>,
    /// 5: priority class (0=P0 SOS … 7=P7 video).
    pub priority: MessagePriority,
    /// 6: remaining TTL in seconds at origination.
    pub ttl_seconds: u64,
    /// 7: Unix epoch seconds at origination.
    pub timestamp: u64,
    /// 8: current relay hop count (starts 0; relays may increment — unsigned).
    pub hop_count: u8,
    /// 9: maximum hops permitted (omitted for P0 — effectively unlimited).
    pub max_hops: Option<u8>,
    /// 10: payload content type.
    pub payload_type: ContentType,
    /// 11: size in bytes of the (possibly encrypted) payload.
    pub payload_size: u64,
    /// 12: BLAKE3 hash of the (possibly encrypted) payload.
    pub payload_hash: [u8; 32],
    /// 13: payload — encrypted bytes for private user messages.
    pub payload: Vec<u8>,
    /// 14: fragment reference id — original message id when fragmented.
    pub payload_ref: Option<MessageId>,
    /// 15: Ed25519 signature over fields 1–7, 9–14, 16, 18 (excludes hop_count
    ///     and routing_hints; includes encryption_hdr and auth_cert_chain —
    ///     see codec.rs module doc for the security rationale).
    pub signature: Option<[u8; 64]>,
    /// 16: X25519 encryption header (present when payload encrypted).
    pub encryption_hdr: Option<EncryptionHdr>,
    /// 17: routing hints (untrusted).
    pub routing_hints: Option<RoutingHints>,
    /// 18: authority certificate chain (emergency broadcasts only).
    pub auth_cert_chain: Option<Vec<Vec<u8>>>,
}

impl Envelope {
    /// Compute the BLAKE3 hash over the payload bytes (field 12 definition).
    pub fn compute_payload_hash(payload: &[u8]) -> [u8; 32] {
        let h = blake3::hash(payload);
        *h.as_bytes()
    }

    /// Convenience accessor for the payload size field.
    pub fn payload_size(&self) -> u64 {
        self.payload_size
    }

    /// True for the P0 abbreviated form (16-byte sender, 4-byte recipient).
    pub fn is_p0_abbreviated(&self) -> bool {
        self.sender_id.len() == 16 && self.recipient_id.len() == 4
    }

    /// True when this envelope is addressed to the emergency broadcast constant.
    pub fn is_emergency_broadcast(&self) -> bool {
        self.recipient_id == crate::protocol::EMERGENCY_BROADCAST
    }
}
