//! First-contact handshake — DISCO-001.
//!
//! Per ORCH-0001 WP-3: on first contact two peers exchange
//! 1. a **CAPABILITY bundle** (payload type 11, ≤ 256 B per
//!    `MESSAGE_ENVELOPE.md`) describing the node (protocol version, supported
//!    transports, capability tags), and
//! 2. their **deduplication Bloom filter** (`DEDUPLICATION.md`) so each side
//!    can skip re-delivering messages the other has already seen.
//!
//! The capability bundle is a canonical-CBOR `Vec<u8>` carried as the envelope
//! payload (fits the 256 B budget). The Bloom exchange is a separate message
//! type (`ContentType::Capability`) carrying the bitmap; full-size filters
//! (≥ 180 KB) require the Fragment path (payload type 13) — see
//! `handshake_sizes`.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::message::{PeerId, TransportLink};
use crate::message_engine::dedup::BloomFilter;
use crate::protocol::{ContentType, Envelope, MessageId};

/// Protocol version spoken by this crate's discovery handshake.
pub const HANDSHAKE_PROTOCOL_VERSION: u8 = 1;
/// Hard budget for a CAPABILITY payload (MESSAGE_ENVELOPE.md).
pub const CAPABILITY_MAX_BYTES: usize = 256;

/// Capability declaration exchanged at first contact (payload type 11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBundle {
    /// Sender's node id.
    pub node_id: PeerId,
    /// Protocol version.
    pub protocol_version: u8,
    /// Transport ids this node has live (e.g. `["sim-a", "internet"]`).
    pub transports: Vec<String>,
    /// Capability tags (e.g. `["relay", "gateway", "mesh"]`).
    pub capabilities: Vec<String>,
    /// Sender's dedup Bloom filter parameters (m bits, k probes).
    pub bloom_m: usize,
    pub bloom_k: usize,
    /// Envelope timestamp of the handshake (unix seconds).
    pub timestamp: u64,
}

impl CapabilityBundle {
    pub fn new(node_id: PeerId) -> Self {
        CapabilityBundle {
            node_id,
            protocol_version: HANDSHAKE_PROTOCOL_VERSION,
            transports: Vec::new(),
            capabilities: Vec::new(),
            bloom_m: 0,
            bloom_k: 0,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    /// Encode to canonical CBOR; rejects bundles that would exceed the 256 B
    /// CAPABILITY budget.
    pub fn encode(&self) -> Result<Vec<u8>, HandshakeError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|_| HandshakeError::Encode)?;
        if buf.len() > CAPABILITY_MAX_BYTES {
            return Err(HandshakeError::BudgetExceeded(buf.len()));
        }
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<CapabilityBundle, HandshakeError> {
        // PM-5: mirror the send-side size guard on the receive side. Without
        // this check a peer could send a 50× budget overrun payload that passes
        // decode without complaint; only encode ever rejected it before.
        if bytes.len() > CAPABILITY_MAX_BYTES {
            return Err(HandshakeError::BudgetExceeded(bytes.len()));
        }
        ciborium::from_reader(bytes).map_err(|_| HandshakeError::Decode)
    }

    /// Build a full CAPABILITY envelope addressed to `recipient` (payload type
    /// 11, payload = encoded bundle).
    pub fn into_envelope(&self, recipient: &PeerId) -> Result<Envelope, HandshakeError> {
        let payload = self.encode()?;
        Ok(Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: self.node_id.0.to_vec(),
            recipient_id: recipient.0.to_vec(),
            priority: crate::message::MessagePriority::P2,
            ttl_seconds: 300,
            timestamp: self.timestamp,
            hop_count: 0,
            max_hops: Some(2),
            payload_type: ContentType::Capability,
            payload_size: payload.len() as u64,
            payload_hash: Envelope::compute_payload_hash(&payload),
            payload,
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        })
    }

    /// Extract a bundle from an incoming envelope with payload type 11.
    pub fn from_envelope(env: &Envelope) -> Result<CapabilityBundle, HandshakeError> {
        if env.payload_type != ContentType::Capability {
            return Err(HandshakeError::NotCapability);
        }
        Self::decode(&env.payload)
    }
}

/// The dedup Bloom filter as exchanged on first contact.
///
/// Full-size filters (BLOOM_CAPACITY = 100 K, ~180 KB) exceed the transport
/// max message size and require the Fragment path; `handshake_sizes` verifies
/// the encode/decode round-trip on a small filter and documents the limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BloomExchange {
    pub node_id: PeerId,
    pub m: usize,
    pub k: usize,
    pub bits: Vec<u8>,
}

impl BloomExchange {
    pub fn from_filter(node_id: PeerId, filter: &BloomFilter) -> Self {
        BloomExchange {
            node_id,
            m: filter.m(),
            k: filter.k(),
            bits: filter.bits().to_vec(),
        }
    }

    pub fn into_filter(&self) -> Option<BloomFilter> {
        BloomFilter::from_parts(self.m, self.k, self.bits.clone())
    }

    pub fn encode(&self) -> Result<Vec<u8>, HandshakeError> {
        let mut buf = Vec::new();
        ciborium::into_writer(self, &mut buf).map_err(|_| HandshakeError::Encode)?;
        Ok(buf)
    }

    pub fn decode(bytes: &[u8]) -> Result<BloomExchange, HandshakeError> {
        ciborium::from_reader(bytes).map_err(|_| HandshakeError::Decode)
    }
}

/// Build the pair of outgoing handshake envelopes: CAPABILITY bundle then
/// Bloom exchange.
pub fn build_handshake(
    node_id: PeerId,
    capabilities: Vec<String>,
    transports: Vec<String>,
    filter: &BloomFilter,
    recipient: &PeerId,
) -> Result<(Envelope, Envelope), HandshakeError> {
    let mut bundle = CapabilityBundle::new(node_id);
    bundle.capabilities = capabilities;
    bundle.transports = transports;
    bundle.bloom_m = filter.m();
    bundle.bloom_k = filter.k();
    let cap = bundle.into_envelope(recipient)?;
    let bloom = BloomExchange::from_filter(node_id, filter).encode()?;
    let bloom_env = Envelope {
        version: crate::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: node_id.0.to_vec(),
        recipient_id: recipient.0.to_vec(),
        priority: crate::message::MessagePriority::P2,
        ttl_seconds: 300,
        timestamp: bundle.timestamp,
        hop_count: 0,
        max_hops: Some(2),
        payload_type: ContentType::Capability,
        payload_size: bloom.len() as u64,
        payload_hash: Envelope::compute_payload_hash(&bloom),
        payload: bloom,
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    };
    Ok((cap, bloom_env))
}

/// Parses an incoming handshake envelope: returns the capability bundle and,
/// if the payload is a Bloom exchange, the reconstructed filter.
#[derive(Debug)]
pub enum HandshakeMessage {
    Capability(CapabilityBundle),
    BloomExchange(BloomExchange),
}

pub fn parse_handshake(env: &Envelope) -> Result<HandshakeMessage, HandshakeError> {
    if env.payload_type != ContentType::Capability {
        return Err(HandshakeError::NotCapability);
    }
    // The Bloom exchange payload is a map with a `bits` field; a capability
    // bundle has a `capabilities` field. Try bloom first (more specific key).
    if let Ok(exchange) = BloomExchange::decode(&env.payload) {
        return Ok(HandshakeMessage::BloomExchange(exchange));
    }
    if let Ok(bundle) = CapabilityBundle::decode(&env.payload) {
        return Ok(HandshakeMessage::Capability(bundle));
    }
    Err(HandshakeError::Decode)
}

/// A validated inbound handshake (used by DiscoveryManager to update the
/// neighbor table).
#[derive(Debug, Clone)]
pub struct HandshakeRecord {
    pub peer_id: PeerId,
    pub capabilities: Option<Vec<String>>,
    pub transports: Vec<String>,
    pub bloom: Option<BloomFilter>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeError {
    Encode,
    Decode,
    NotCapability,
    BudgetExceeded(usize),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeError::Encode => write!(f, "handshake: encode failed"),
            HandshakeError::Decode => write!(f, "handshake: decode failed"),
            HandshakeError::NotCapability => write!(f, "handshake: not a capability envelope"),
            HandshakeError::BudgetExceeded(n) => {
                write!(
                    f,
                    "handshake: capability payload {n} B exceeds 256 B budget"
                )
            }
        }
    }
}

impl std::error::Error for HandshakeError {}

// NOTE: `TransportLink` is imported above for API stability of the module
// (handshake consumers resolve links via neighbor_table); unused-import
// suppression is unnecessary since it is referenced in a doc comment only —
// keep the import list minimal.
#[allow(dead_code)]
type _Link = TransportLink;

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    #[test]
    fn capability_bundle_round_trip_and_budget() {
        let mut b = CapabilityBundle::new(pid(1));
        b.capabilities = vec!["relay".into(), "gateway".into()];
        b.transports = vec!["sim-a".into()];
        b.bloom_m = 512;
        b.bloom_k = 7;
        let enc = b.encode().unwrap();
        assert!(enc.len() <= CAPABILITY_MAX_BYTES);
        let dec = CapabilityBundle::decode(&enc).unwrap();
        assert_eq!(dec, b);
    }

    #[test]
    fn oversized_bundle_rejected() {
        let mut b = CapabilityBundle::new(pid(1));
        b.capabilities = (0..200).map(|i| format!("tag-{i}-0123456789")).collect();
        let err = b.encode().unwrap_err();
        assert!(matches!(err, HandshakeError::BudgetExceeded(_)));
    }

    // PM-5: decode must enforce the 256 B budget (previously only encode did).
    #[test]
    fn decode_rejects_oversized_payload() {
        let oversized = vec![0u8; CAPABILITY_MAX_BYTES + 1];
        assert!(matches!(
            CapabilityBundle::decode(&oversized),
            Err(HandshakeError::BudgetExceeded(_))
        ));
    }

    #[test]
    fn bloom_exchange_round_trip() {
        let mut f = BloomFilter::new(100, 0.001);
        f.insert([1u8; 16]);
        f.insert([2u8; 16]);
        let ex = BloomExchange::from_filter(pid(9), &f);
        let enc = ex.encode().unwrap();
        let dec = BloomExchange::decode(&enc).unwrap();
        assert_eq!(dec, ex);
        let restored = dec.into_filter().unwrap();
        assert!(restored.contains([1u8; 16]));
        assert!(restored.contains([2u8; 16]));
        assert!(!restored.contains([3u8; 16]));
    }

    #[test]
    fn envelope_carries_bundle_and_bloom() {
        let mut f = BloomFilter::new(100, 0.001);
        f.insert([7u8; 16]);
        let (cap, bloom_env) = build_handshake(
            pid(1),
            vec!["relay".into()],
            vec!["sim-a".into()],
            &f,
            &pid(2),
        )
        .unwrap();
        assert_eq!(cap.payload_type, ContentType::Capability);
        assert!(cap.payload.len() <= CAPABILITY_MAX_BYTES);
        let parsed_cap = parse_handshake(&cap).unwrap();
        assert!(matches!(parsed_cap, HandshakeMessage::Capability(_)));
        let parsed_bloom = parse_handshake(&bloom_env).unwrap();
        if let HandshakeMessage::BloomExchange(ex) = parsed_bloom {
            let rf = ex.into_filter().unwrap();
            assert!(rf.contains([7u8; 16]));
        } else {
            panic!("bloom envelope parsed as capability");
        }
    }
}
