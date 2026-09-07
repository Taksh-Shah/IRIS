//! Bounded outer protocol used by an IRIS relay.
//! The payload remains an opaque, end-to-end protected IRIS envelope.

use crate::message::PeerId;
use crate::TransportError;

pub const RELAY_VERSION: u8 = 1;
pub const RELAY_REGISTER: u8 = 1;
pub const RELAY_ROUTE: u8 = 2;
pub const RELAY_HEARTBEAT: u8 = 3;
pub const RELAY_HEADER_LEN: usize = 1 + 1 + 32 + 32 + 4;
pub const MAX_RELAY_PAYLOAD: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayFrame {
    Register {
        peer_id: PeerId,
    },
    Route {
        source: PeerId,
        target: PeerId,
        payload: Vec<u8>,
    },
    Heartbeat,
}

pub fn encode(frame: &RelayFrame) -> Result<Vec<u8>, TransportError> {
    let (kind, source, target, payload): (u8, PeerId, PeerId, &[u8]) = match frame {
        RelayFrame::Register { peer_id } => (RELAY_REGISTER, *peer_id, PeerId([0; 32]), &[]),
        RelayFrame::Route {
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
            (RELAY_ROUTE, *source, *target, payload)
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
    if payload_len > MAX_RELAY_PAYLOAD || bytes.len() != RELAY_HEADER_LEN + payload_len {
        return Err(TransportError::Protocol(
            "invalid relay payload length".into(),
        ));
    }
    let payload = &bytes[RELAY_HEADER_LEN..];
    match kind {
        RELAY_REGISTER if payload.is_empty() && target == [0; 32] => Ok(RelayFrame::Register {
            peer_id: PeerId(source),
        }),
        RELAY_ROUTE if payload_len > 0 => Ok(RelayFrame::Route {
            source: PeerId(source),
            target: PeerId(target),
            payload: payload.to_vec(),
        }),
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
        })
        .unwrap();
        register[66..70].copy_from_slice(&1u32.to_le_bytes());
        register.push(1);
        assert!(decode(&register).is_err());
    }
}
