//! Message model — MSG-001.
//!
//! Minimal shared types used by the `Transport` trait signatures. Full message
//! lifecycle (priority queuing, TTL, dedup, ACK) is designed in
//! `docs/implementation/MSG_DESIGN.md` and lands in a later crate iteration.

use std::fmt;
use std::net::SocketAddr;
use std::time::Instant;

/// Priority classes (P0–P7) as defined in `docs/protocol/PRIORITY_MODEL.md`.
///
/// Semantics per PRIORITY_MODEL.md §Priority Levels: P0=SOS, P1=Medical,
/// P2=Location, P3=EmergencyText, P4=Normal, P5=Image, P6=Voice, P7=Video.
/// Ordering matters: `P0 < P1 < ... < P7` so lower discriminants are higher
/// priority and sort first.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum MessagePriority {
    /// P0 — SOS. Life-threatening emergency beacon. 255-byte LoRa budget.
    P0 = 0,
    /// P1 — Medical. Medical situation, triage request.
    P1 = 1,
    /// P2 — Location. GPS location share, evacuation route.
    P2 = 2,
    /// P3 — EmergencyText. Emergency text message, status update.
    P3 = 3,
    /// P4 — Normal. Standard text message.
    P4 = 4,
    /// P5 — Image. Photo, document scan.
    P5 = 5,
    /// P6 — Voice. Voice message recording.
    P6 = 6,
    /// P7 — Video. Video clip, 50 MB.
    P7 = 7,
}

impl MessagePriority {
    /// Convert a raw u8 to a priority; out-of-range returns `None`.
    pub fn from_u8(v: u8) -> Option<MessagePriority> {
        Some(match v {
            0 => MessagePriority::P0,
            1 => MessagePriority::P1,
            2 => MessagePriority::P2,
            3 => MessagePriority::P3,
            4 => MessagePriority::P4,
            5 => MessagePriority::P5,
            6 => MessagePriority::P6,
            7 => MessagePriority::P7,
            _ => return None,
        })
    }

    /// Raw numeric value.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// True for P0/P1 — the emergency classes that use multipath.
    pub fn is_emergency(self) -> bool {
        matches!(self, MessagePriority::P0 | MessagePriority::P1)
    }
}

impl fmt::Display for MessagePriority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{}", self.as_u8())
    }
}

/// Manual Arbitrary implementation for proptest (enabled via proptest feature)
#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for MessagePriority {
    type Parameters = ();
    type Strategy = proptest::strategy::Just<MessagePriority>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        proptest::strategy::Just(MessagePriority::P4) // Default to P4 for simplicity
    }
}

/// A node identifier: 32-byte BLAKE3 hash of the node's Ed25519 public key
/// (per `docs/protocol/ADDRESSING.md` and ADR-0002).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    /// Length in bytes.
    pub const LEN: usize = 32;

    /// Create from a 32-byte array.
    pub fn from_bytes(b: [u8; 32]) -> Self {
        PeerId(b)
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Truncated 8-byte hex prefix — the only form allowed in logs, metrics,
    /// or exported telemetry (privacy rule P2 / OBS_DESIGN.md). Full ID
    /// remains available in memory via `Display`. Allocation-free.
    pub fn short(&self) -> crate::observability::ShortId {
        crate::observability::ShortId::from(self)
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// A fully-encoded message ready for a transport's `send()`.
///
/// This is the wire unit handed to transports. The receiver deduplicates by
/// `message_id`; first arrival wins (per MSG-001 / `DEDUPLICATION.md`).
///
/// `message_id` is the protocol 16-byte UUIDv7 (RES-0008 C1 — all docs and
/// code normalized to the wire form; MESSAGE_ENVELOPE.md is authoritative).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerializedMessage {
    /// 16-byte UUIDv7 message id (resolves the 16/24/32-byte ID drift, RES-0008 C1).
    pub message_id: crate::protocol::MessageId,
    /// Priority class — used by transports for send-hint and by manager for selection.
    pub priority: MessagePriority,
    /// Framed/ciphertext payload bytes.
    pub payload: Vec<u8>,
}

/// Static identity + addressing info for a peer.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: PeerId,
    /// Candidate internet addresses (for INTERNET-001 direct/relay paths).
    pub addresses: Vec<SocketAddr>,
    /// Transport-specific addresses keyed by transport id (e.g. BLE MAC).
    pub transport_addresses: Vec<(String, String)>,
    /// Last confirmed contact.
    pub last_seen: Option<Instant>,
}

/// Configuration for a discovery scan.
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// How long to scan before reporting `DiscoveryTimeout`.
    pub timeout: std::time::Duration,
    /// Maximum peers to report.
    pub max_peers: usize,
    /// Only report these peers (None = all).
    pub filter: Option<Vec<PeerId>>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            timeout: std::time::Duration::from_secs(30),
            max_peers: 64,
            filter: None,
        }
    }
}

/// What a node advertises so other nodes can reach it.
#[derive(Debug, Clone)]
pub struct NodeAdvertisement {
    pub peer_id: PeerId,
    /// Public IP for direct QUIC/TCP (None → relay-only, typical behind CGNAT).
    pub public_ip_addr: Option<SocketAddr>,
    pub hostname: Option<String>,
    /// Arbitrary capability tags.
    pub tags: Vec<String>,
}

/// An established link to a peer.
#[derive(Debug, Clone)]
pub struct TransportLink {
    pub peer_id: PeerId,
    pub transport_id: String,
    pub established_at: Instant,
}

/// Acknowledgment that `send()` accepted the message.
#[derive(Debug, Clone)]
pub struct SendReceipt {
    pub peer_id: PeerId,
    pub bytes_sent: usize,
    pub sent_at: Instant,
}

/// A message arriving from a peer via a transport.
#[derive(Debug, Clone)]
pub struct IncomingMessage {
    pub peer_id: PeerId,
    pub transport_id: String,
    pub payload: Vec<u8>,
    pub received_at: Instant,
}

/// Per-link quality estimate reported to the routing engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LinkQuality {
    Excellent,
    Good,
    Fair,
    Poor,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_round_trips_p0_to_p7() {
        for v in 0..=7u8 {
            let p = MessagePriority::from_u8(v).expect("P0..P7 must decode");
            assert_eq!(p.as_u8(), v, "as_u8 round-trip for {v}");
            assert_eq!(MessagePriority::from_u8(p.as_u8()), Some(p));
        }
    }

    #[test]
    fn priority_rejects_out_of_range() {
        assert_eq!(MessagePriority::from_u8(8), None, "8 is past P7");
        assert_eq!(MessagePriority::from_u8(255), None, "255 must be rejected");
        assert_eq!(MessagePriority::from_u8(200), None, "200 must be rejected");
        assert_eq!(
            MessagePriority::from_u8(7),
            Some(MessagePriority::P7),
            "boundary P7"
        );
    }

    #[test]
    fn emergency_true_only_for_p0_p1() {
        assert!(MessagePriority::P0.is_emergency());
        assert!(MessagePriority::P1.is_emergency());
        for p in [
            MessagePriority::P2,
            MessagePriority::P3,
            MessagePriority::P4,
            MessagePriority::P5,
            MessagePriority::P6,
            MessagePriority::P7,
        ] {
            assert!(!p.is_emergency(), "{p} must not be emergency");
        }
    }

    #[test]
    fn priority_order_puts_emergency_first() {
        // Lower discriminant = higher priority; P0 sorts first.
        assert!(MessagePriority::P0 < MessagePriority::P1);
        assert!(MessagePriority::P0 < MessagePriority::P7);
        assert!(MessagePriority::P1 < MessagePriority::P2);
        assert!(MessagePriority::P6 < MessagePriority::P7);
    }
}
