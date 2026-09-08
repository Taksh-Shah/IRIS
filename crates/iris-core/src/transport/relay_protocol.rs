//! Bounded outer protocol used by an IRIS relay.
//! The payload remains an opaque, end-to-end protected IRIS envelope.

use crate::message::PeerId;
use crate::TransportError;

pub const RELAY_VERSION: u8 = 1;
pub const RELAY_REGISTER: u8 = 1;
pub const RELAY_ROUTE: u8 = 2;
pub const RELAY_HEARTBEAT: u8 = 3;
pub const RELAY_CHALLENGE: u8 = 4;
pub const RELAY_ACK: u8 = 5;
pub const RELAY_HEADER_LEN: usize = 1 + 1 + 32 + 32 + 4;
pub const MAX_RELAY_PAYLOAD: usize = 1024 * 1024;
pub const RELAY_SIGNATURE_LEN: usize = 64;
pub const RELAY_ROUTE_ID_LEN: usize = 16;
pub const RELAY_REGISTER_DOMAIN: &[u8] = b"IRIS-RELAY-REGISTER-V1\0";
pub const LAN_REGISTER_DOMAIN: &[u8] = b"IRIS-LAN-REGISTER-V1\0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RelayAckStatus {
    ForwardedToLiveSession = 1,
    TargetOffline = 2,
    TargetBackpressured = 3,
}

impl RelayAckStatus {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::ForwardedToLiveSession),
            2 => Some(Self::TargetOffline),
            3 => Some(Self::TargetBackpressured),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayFrame {
    Challenge {
        nonce: [u8; 32],
    },
    Register {
        peer_id: PeerId,
        signature: [u8; RELAY_SIGNATURE_LEN],
    },
    Route {
        route_id: [u8; RELAY_ROUTE_ID_LEN],
        source: PeerId,
        target: PeerId,
        payload: Vec<u8>,
    },
    Ack {
        route_id: [u8; RELAY_ROUTE_ID_LEN],
        target: PeerId,
        status: RelayAckStatus,
    },
    Heartbeat,
}

/// Domain-separated registration proof. The server-issued challenge makes a
/// captured proof unusable on any later TLS session.
pub fn registration_signable(nonce: &[u8; 32], peer_id: &PeerId) -> Vec<u8> {
    registration_signable_in_domain(RELAY_REGISTER_DOMAIN, nonce, peer_id)
}

/// Domain-separated proof for direct LAN sockets. It must never be accepted
/// by a WAN relay, preventing either protocol from becoming a signing oracle
/// for the other.
pub fn lan_registration_signable(nonce: &[u8; 32], peer_id: &PeerId) -> Vec<u8> {
    registration_signable_in_domain(LAN_REGISTER_DOMAIN, nonce, peer_id)
}

fn registration_signable_in_domain(domain: &[u8], nonce: &[u8; 32], peer_id: &PeerId) -> Vec<u8> {
    let mut out = Vec::with_capacity(domain.len() + 64);
    out.extend_from_slice(domain);
    out.extend_from_slice(nonce);
    out.extend_from_slice(peer_id.as_bytes());
    out
}

pub fn encode(frame: &RelayFrame) -> Result<Vec<u8>, TransportError> {
    let mut owned_payload = Vec::new();
    let (kind, source, target, payload): (u8, PeerId, PeerId, &[u8]) = match frame {
        RelayFrame::Challenge { nonce } => (RELAY_CHALLENGE, PeerId(*nonce), PeerId([0; 32]), &[]),
        RelayFrame::Register { peer_id, signature } => {
            (RELAY_REGISTER, *peer_id, PeerId([0; 32]), signature)
        }
        RelayFrame::Route {
            route_id,
            source,
            target,
            payload,
        } => {
            if payload.is_empty() || payload.len() > MAX_RELAY_PAYLOAD {
                return Err(TransportError::MessageTooLarge {
                    limit: MAX_RELAY_PAYLOAD,
                    actual: payload.len(),
                });
            }
            owned_payload.reserve(RELAY_ROUTE_ID_LEN + payload.len());
            owned_payload.extend_from_slice(route_id);
            owned_payload.extend_from_slice(payload);
            (RELAY_ROUTE, *source, *target, &owned_payload)
        }
        RelayFrame::Ack {
            route_id,
            target,
            status,
        } => {
            owned_payload.reserve(RELAY_ROUTE_ID_LEN + 1);
            owned_payload.extend_from_slice(route_id);
            owned_payload.push(*status as u8);
            (RELAY_ACK, PeerId([0; 32]), *target, &owned_payload)
        }
        RelayFrame::Heartbeat => (RELAY_HEARTBEAT, PeerId([0; 32]), PeerId([0; 32]), &[]),
    };
    let mut out = Vec::with_capacity(RELAY_HEADER_LEN + payload.len());
    out.extend_from_slice(&[RELAY_VERSION, kind]);
    out.extend_from_slice(&source.0);
    out.extend_from_slice(&target.0);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<RelayFrame, TransportError> {
    if bytes.len() < RELAY_HEADER_LEN {
        return Err(TransportError::Protocol(
            "relay frame header truncated".into(),
        ));
    }
    if bytes[0] != RELAY_VERSION {
        return Err(TransportError::Protocol(
            "unsupported relay protocol version".into(),
        ));
    }
    let kind = bytes[1];
    let mut source = [0; 32];
    source.copy_from_slice(&bytes[2..34]);
    let mut target = [0; 32];
    target.copy_from_slice(&bytes[34..66]);
    let mut len = [0; 4];
    len.copy_from_slice(&bytes[66..70]);
    let payload_len = u32::from_le_bytes(len) as usize;
    if payload_len > MAX_RELAY_PAYLOAD + RELAY_ROUTE_ID_LEN
        || bytes.len() != RELAY_HEADER_LEN + payload_len
    {
        return Err(TransportError::Protocol(
            "invalid relay payload length".into(),
        ));
    }
    let payload = &bytes[RELAY_HEADER_LEN..];
    match kind {
        RELAY_CHALLENGE if payload.is_empty() && target == [0; 32] && source != [0; 32] => {
            Ok(RelayFrame::Challenge { nonce: source })
        }
        RELAY_REGISTER
            if payload_len == RELAY_SIGNATURE_LEN && target == [0; 32] && source != [0; 32] =>
        {
            let mut signature = [0u8; RELAY_SIGNATURE_LEN];
            signature.copy_from_slice(payload);
            Ok(RelayFrame::Register {
                peer_id: PeerId(source),
                signature,
            })
        }
        RELAY_ROUTE
            if payload_len > RELAY_ROUTE_ID_LEN && source != [0; 32] && target != [0; 32] =>
        {
            let mut route_id = [0u8; RELAY_ROUTE_ID_LEN];
            route_id.copy_from_slice(&payload[..RELAY_ROUTE_ID_LEN]);
            Ok(RelayFrame::Route {
                route_id,
                source: PeerId(source),
                target: PeerId(target),
                payload: payload[RELAY_ROUTE_ID_LEN..].to_vec(),
            })
        }
        RELAY_ACK
            if payload_len == RELAY_ROUTE_ID_LEN + 1 && source == [0; 32] && target != [0; 32] =>
        {
            let mut route_id = [0u8; RELAY_ROUTE_ID_LEN];
            route_id.copy_from_slice(&payload[..RELAY_ROUTE_ID_LEN]);
            let status = RelayAckStatus::from_u8(payload[RELAY_ROUTE_ID_LEN])
                .ok_or_else(|| TransportError::Protocol("invalid relay ack status".into()))?;
            Ok(RelayFrame::Ack {
                route_id,
                target: PeerId(target),
                status,
            })
        }
        RELAY_HEARTBEAT if payload.is_empty() && source == [0; 32] && target == [0; 32] => {
            Ok(RelayFrame::Heartbeat)
        }
        _ => Err(TransportError::Protocol(
            "invalid relay frame kind or fields".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn route_round_trips_opaque_payload() {
        let frame = RelayFrame::Route {
            route_id: [3; 16],
            source: PeerId([1; 32]),
            target: PeerId([2; 32]),
            payload: vec![9, 8, 7],
        };
        assert_eq!(decode(&encode(&frame).unwrap()).unwrap(), frame);
    }
    #[test]
    fn rejects_truncation_and_length_smuggling() {
        assert!(decode(&[0; RELAY_HEADER_LEN - 1]).is_err());
        let mut bytes = encode(&RelayFrame::Heartbeat).unwrap();
        bytes[66..70].copy_from_slice(&1u32.to_le_bytes());
        assert!(decode(&bytes).is_err());
    }
    #[test]
    fn rejects_unknown_version_and_invalid_register_fields() {
        let mut bytes = encode(&RelayFrame::Heartbeat).unwrap();
        bytes[0] = 99;
        assert!(decode(&bytes).is_err());
        let mut register = encode(&RelayFrame::Register {
            peer_id: PeerId([1; 32]),
            signature: [2; 64],
        })
        .unwrap();
        register[66..70].copy_from_slice(&1u32.to_le_bytes());
        register.truncate(RELAY_HEADER_LEN + 1);
        assert!(decode(&register).is_err());
    }

    #[test]
    fn challenge_register_and_ack_round_trip() {
        let challenge = RelayFrame::Challenge { nonce: [7; 32] };
        assert_eq!(decode(&encode(&challenge).unwrap()).unwrap(), challenge);
        let register = RelayFrame::Register {
            peer_id: PeerId([9; 32]),
            signature: [5; 64],
        };
        assert_eq!(decode(&encode(&register).unwrap()).unwrap(), register);
        let ack = RelayFrame::Ack {
            route_id: [4; 16],
            target: PeerId([8; 32]),
            status: RelayAckStatus::TargetOffline,
        };
        assert_eq!(decode(&encode(&ack).unwrap()).unwrap(), ack);
    }

    #[test]
    fn registration_signable_is_domain_separated_and_bound() {
        let a = registration_signable(&[1; 32], &PeerId([2; 32]));
        let b = registration_signable(&[1; 32], &PeerId([3; 32]));
        let c = registration_signable(&[4; 32], &PeerId([2; 32]));
        assert!(a.starts_with(RELAY_REGISTER_DOMAIN));
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, lan_registration_signable(&[1; 32], &PeerId([2; 32])));
    }

    #[test]
    fn golden_route_wire_layout_is_stable() {
        let encoded = encode(&RelayFrame::Route {
            route_id: [0x44; 16],
            source: PeerId([0x11; 32]),
            target: PeerId([0x22; 32]),
            payload: vec![0xaa, 0xbb],
        })
        .unwrap();
        assert_eq!(&encoded[..2], &[RELAY_VERSION, RELAY_ROUTE]);
        assert_eq!(&encoded[2..34], &[0x11; 32]);
        assert_eq!(&encoded[34..66], &[0x22; 32]);
        assert_eq!(&encoded[66..70], &18u32.to_le_bytes());
        assert_eq!(&encoded[70..86], &[0x44; 16]);
        assert_eq!(&encoded[86..], &[0xaa, 0xbb]);
    }

    #[cfg(feature = "proptest")]
    mod properties {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
                let _ = decode(&bytes);
            }

            #[test]
            fn route_payload_round_trips(payload in proptest::collection::vec(any::<u8>(), 1..4096)) {
                let frame = RelayFrame::Route {
                    route_id: [7; RELAY_ROUTE_ID_LEN],
                    source: PeerId([1; 32]),
                    target: PeerId([2; 32]),
                    payload,
                };
                prop_assert_eq!(decode(&encode(&frame).unwrap()).unwrap(), frame);
            }
        }
    }
}
