//! SOS verify/cancel policy — EMERG-001 (AC-4/6).
//!
//! SOS is the P0 signal: never encrypted, ≤ 84 B payload (255 B envelope on
//! LoRa), retried 30 s / 2× until TTL. This module owns the *verification
//! policy* applied to incoming envelope field 13 payloads:
//!
//! - decode + model validation (fail loud → drop + audit),
//! - `Sos`/`Test` accepted → rate-limit gate (AC-5, see `rate_limit`),
//! - `Cancel` must reference an original (`original_message_id`) and fall
//!   within the 60-minute cancel window (AC-6) — else reject + audit.

use crate::emergency::codec::{decode_sos, encode_sos, EmergencyCodecError};
use crate::emergency::model::{SosKind, SosMessage};

/// Cancel window: a CANCEL must arrive within 60 min of the original SOS.
pub const SOS_CANCEL_WINDOW_SECS: u64 = 3600;

/// SOS policy errors.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SosError {
    #[error("sos: payload decode/model failure: {0}")]
    Codec(String),
    #[error("sos: cancel references an unknown original_message_id")]
    UnknownOriginal,
    #[error("sos: cancel outside the 60-minute window")]
    CancelWindowExpired,
    #[error("sos: cancel missing original_message_id")]
    CancelMissingOriginal,
}

/// Decision for an incoming SOS envelope payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SosOutcome {
    /// Fresh SOS (or Test) — forward for rate-limit gate (P0).
    AcceptedSos {
        /// New 60-minute cancel deadline (timestamp when accepted).
        cancel_deadline: u64,
    },
    /// Verified cancellation of a previously accepted SOS.
    AcceptedCancel,
    /// Rejected (unknown original / expired window / missing original).
    Rejected(SosError),
}

/// Decode + classify an SOS payload (the engine's incoming-hook core step).
///
/// `now_unix` is the engine clock. `original_timestamp` is the `timestamp` of
/// the original SOS (resolved by the engine from its dedup/ledger when
/// `kind == Cancel`); a Cancel against an unknown original is rejected.
pub fn classify_sos(
    payload: &[u8],
    now_unix: u64,
    original_timestamp: Option<u64>,
) -> Result<SosOutcome, SosError> {
    let msg = decode_sos(payload).map_err(|e| SosError::Codec(e.to_string()))?;
    match msg.kind {
        SosKind::Sos | SosKind::Test => Ok(SosOutcome::AcceptedSos {
            cancel_deadline: now_unix.saturating_add(SOS_CANCEL_WINDOW_SECS),
        }),
        SosKind::Cancel => {
            if msg.original_message_id.is_none() {
                return Err(SosError::CancelMissingOriginal);
            }
            match original_timestamp {
                Some(t) if now_unix.saturating_sub(t) <= SOS_CANCEL_WINDOW_SECS => {
                    Ok(SosOutcome::AcceptedCancel)
                }
                Some(_) => Err(SosError::CancelWindowExpired),
                None => Err(SosError::UnknownOriginal),
            }
        }
    }
}

/// Decode-helper re-export for symmetry on the send path.
pub fn decode(payload: &[u8]) -> Result<SosMessage, EmergencyCodecError> {
    decode_sos(payload)
}

/// Encode-helper for building/persisting an SOS payload.
pub fn encode(msg: &SosMessage) -> Result<Vec<u8>, EmergencyCodecError> {
    encode_sos(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emergency::model::{LocationSource, SosReason};

    fn sos(ts: u32, kind: SosKind) -> SosMessage {
        SosMessage {
            format_version: 1,
            latitude: 23_022_500,
            longitude: 72_571_250,
            accuracy_m: 10,
            location_source: LocationSource::Gps,
            timestamp: ts,
            kind,
            original_message_id: None,
            reason: Some(SosReason::Injury),
        }
    }

    #[test]
    fn fresh_sos_accepted() {
        let p = encode(&sos(1_000, SosKind::Sos)).unwrap();
        let out = classify_sos(&p, 1_000, None).unwrap();
        assert_eq!(
            out,
            SosOutcome::AcceptedSos {
                cancel_deadline: 4_600
            }
        );
    }

    #[test]
    fn cancel_within_window_accepted() {
        let mut c = sos(1_100, SosKind::Cancel);
        c.original_message_id = Some([7u8; 16]);
        let p = encode(&c).unwrap();
        assert_eq!(
            classify_sos(&p, 1_100, Some(1_000)).unwrap(),
            SosOutcome::AcceptedCancel
        );
    }

    #[test]
    fn cancel_outside_window_rejected() {
        let mut c = sos(1_000, SosKind::Cancel);
        c.original_message_id = Some([7u8; 16]);
        let p = encode(&c).unwrap();
        assert_eq!(
            classify_sos(&p, 1_000 + SOS_CANCEL_WINDOW_SECS + 1, Some(1_000)),
            Err(SosError::CancelWindowExpired)
        );
    }

    #[test]
    fn cancel_unknown_original_rejected() {
        let mut c = sos(1_000, SosKind::Cancel);
        c.original_message_id = Some([7u8; 16]);
        let p = encode(&c).unwrap();
        assert_eq!(
            classify_sos(&p, 1_100, None),
            Err(SosError::UnknownOriginal)
        );
    }

    #[test]
    fn cancel_missing_original_rejected() {
        // A Cancel without original_message_id is fail-loud at the codec level:
        // encode refuses it and decode refuses a hand-crafted one, so the
        // policy's CancelMissingOriginal branch is defense-in-depth.
        let c = sos(1_000, SosKind::Cancel); // no original_message_id
        assert!(matches!(
            encode(&c),
            Err(EmergencyCodecError::Invalid(
                crate::emergency::model::ModelError::CancelMissingOriginal
            ))
        ));
        // Craft the wire bytes by hand (valid map, cancel kind, no original)
        // and confirm the decode/classify path is equally fail-loud.
        use ciborium::value::{Integer, Value};
        let wire = {
            let map = Value::Map(
                [
                    (1u64, Value::Integer(Integer::from(1))),
                    (2u64, Value::Integer(Integer::from(0))),
                    (3u64, Value::Integer(Integer::from(0))),
                    (4u64, Value::Integer(Integer::from(0))),
                    (5u64, Value::Integer(Integer::from(4))), // None
                    (6u64, Value::Integer(Integer::from(1_000))),
                    (7u64, Value::Integer(Integer::from(2))), // Cancel
                ]
                .into_iter()
                .map(|(k, v)| (Value::Integer(Integer::from(k)), v))
                .collect(),
            );
            let mut out = Vec::new();
            ciborium::ser::into_writer(&map, &mut out).unwrap();
            out
        };
        assert!(matches!(
            classify_sos(&wire, 1_100, Some(1_000)),
            Err(SosError::Codec(_))
        ));
    }

    #[test]
    fn garbage_payload_rejected() {
        assert!(matches!(
            classify_sos(&[0xde, 0xad, 0xbe, 0xef], 1_000, None),
            Err(SosError::Codec(_))
        ));
    }
}
