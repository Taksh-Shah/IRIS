//! Emergency payload codec — canonical CBOR (CDE), RFC 8949 §4.2.
//!
//! Mirrors the envelope codec's deterministic-encoding machinery
//! (`ciborium::Value`, `(u8, Value)` in-progress maps sorted at emission,
//! definite-length items). Enumerations use **fixed wire codes** — never
//! derived from Rust discriminants — so a future enum reorder never breaks the
//! wire.
//!
//! Decoding is strict about *shape* (unknown top-level keys tolerated for
//! append-only extension, matching ADR-0011) but out-of-range enum codes and
//! oversized fields are rejected loudly ("drop + audit, never truncate").

use ciborium::value::{Integer, Value};
use thiserror::Error;

use super::model::{
    AlertMessageType, AuthorityMeta, Certainty, EmergencyBroadcast, LocationSource, ModelError,
    Severity, SosKind, SosMessage, SosReason,
};

/// Codec errors. A relay treats every decode error as "DROP + audit": a
/// malformed emergency payload must never be forwarded.
#[derive(Debug, Error)]
pub enum EmergencyCodecError {
    #[error("model validation failed: {0}")]
    Invalid(#[from] ModelError),
    #[error("CBOR encode failed: {0}")]
    Encode(String),
    #[error("CBOR decode failed: {0}")]
    Decode(String),
    #[error("root value is not a CBOR map")]
    NotAMap,
    #[error("missing required field {0}")]
    MissingField(&'static str),
    #[error("field {0} has invalid type")]
    InvalidField(&'static str),
    #[error("invalid enum code {0} in {1}")]
    InvalidEnumCode(u64, &'static str),
}

// ---------------------------------------------------------------------------
// EmergencyBroadcast wire keys (fixed; EMERG_DESIGN.md §4 "Wire format").
// ---------------------------------------------------------------------------

const B_FORMAT_VERSION: u8 = 1;
const B_BROADCAST_ID: u8 = 2;
const B_REPUBLISH_ID: u8 = 3;
const B_ISSUED_AT: u8 = 4;
const B_EXPIRES_AT: u8 = 5;
const B_SEVERITY: u8 = 6;
const B_CERTAINTY: u8 = 7;
const B_MESSAGE_TYPE: u8 = 8;
const B_AREA_CODE: u8 = 9;
const B_LANGUAGE: u8 = 10;
const B_HEADLINE: u8 = 11;
const B_INSTRUCTIONS: u8 = 12;
const B_AUTHORITY: u8 = 13; // nested AuthorityMeta map
const B_AUTHORITY_PEER_SHORT: u8 = 14;
const B_DRILL: u8 = 15;

// AuthorityMeta wire keys (nested map under field 13).
const A_ISSUER_NAME: u8 = 1;
const A_GEO_SCOPE: u8 = 2;
const A_FUNCTIONAL_SCOPE: u8 = 3;
const A_MAX_SEVERITY: u8 = 4;

// ---------------------------------------------------------------------------
// SosMessage wire keys (fixed; EMERG_DESIGN.md §5).
// ---------------------------------------------------------------------------

const S_FORMAT_VERSION: u8 = 1;
const S_LATITUDE: u8 = 2;
const S_LONGITUDE: u8 = 3;
const S_ACCURACY_M: u8 = 4;
const S_LOCATION_SOURCE: u8 = 5;
const S_TIMESTAMP: u8 = 6;
const S_KIND: u8 = 7;
const S_ORIGINAL_MESSAGE_ID: u8 = 8;
const S_REASON: u8 = 9;

// ---------------------------------------------------------------------------
// Map building (mirrors protocol::codec; CDE = sort at emission).
// ---------------------------------------------------------------------------

type Map = Vec<(u8, Value)>;

fn insert_u64(map: &mut Map, k: u8, v: u64) {
    map.push((k, Value::Integer(Integer::from(v))));
}

fn insert_i64(map: &mut Map, k: u8, v: i64) {
    map.push((k, Value::Integer(Integer::from(v))));
}

fn insert_bytes<const N: usize>(map: &mut Map, k: u8, v: Option<[u8; N]>) {
    if let Some(b) = v {
        map.push((k, Value::Bytes(b.to_vec())));
    }
}

fn insert_text(map: &mut Map, k: u8, v: &str) {
    map.push((k, Value::Text(v.to_string())));
}

fn insert_opt_text(map: &mut Map, k: u8, v: &Option<String>) {
    if let Some(s) = v {
        map.push((k, Value::Text(s.clone())));
    }
}

fn insert_bool(map: &mut Map, k: u8, v: bool) {
    map.push((k, Value::Bool(v)));
}

fn insert_value(map: &mut Map, k: u8, v: Value) {
    map.push((k, v));
}

fn map_to_value(mut map: Map) -> Value {
    map.sort_by_key(|(k, _)| *k);
    let pairs = map
        .into_iter()
        .map(|(k, v)| (Value::Integer(Integer::from(u64::from(k))), v))
        .collect();
    Value::Map(pairs)
}

fn write_map(map: Map) -> Result<Vec<u8>, EmergencyCodecError> {
    let mut out = Vec::new();
    ciborium::ser::into_writer(&map_to_value(map), &mut out)
        .map_err(|e| EmergencyCodecError::Encode(e.to_string()))?;
    Ok(out)
}

fn encode_authority_meta(a: &AuthorityMeta) -> Value {
    let mut m = Map::new();
    insert_opt_text(&mut m, A_ISSUER_NAME, &a.issuer_name);
    insert_opt_text(&mut m, A_GEO_SCOPE, &a.geo_scope);
    insert_opt_text(&mut m, A_FUNCTIONAL_SCOPE, &a.functional_scope);
    insert_u64(&mut m, A_MAX_SEVERITY, u64::from(a.max_severity));
    map_to_value(m)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

/// Encode an `EmergencyBroadcast` to canonical CBOR.
pub fn encode_broadcast(b: &EmergencyBroadcast) -> Result<Vec<u8>, EmergencyCodecError> {
    b.validate()?;
    let mut m = Map::new();
    insert_u64(&mut m, B_FORMAT_VERSION, u64::from(b.format_version));
    insert_bytes(&mut m, B_BROADCAST_ID, Some(b.broadcast_id));
    insert_bytes(&mut m, B_REPUBLISH_ID, b.republish_id);
    insert_u64(&mut m, B_ISSUED_AT, b.issued_at);
    insert_u64(&mut m, B_EXPIRES_AT, b.expires_at);
    insert_u64(&mut m, B_SEVERITY, u64::from(b.severity.as_u8()));
    insert_u64(&mut m, B_CERTAINTY, u64::from(b.certainty.as_u8()));
    insert_u64(&mut m, B_MESSAGE_TYPE, u64::from(b.message_type.as_u8()));
    insert_text(&mut m, B_AREA_CODE, &b.area_code);
    insert_text(&mut m, B_LANGUAGE, &b.language);
    insert_text(&mut m, B_HEADLINE, &b.headline);
    insert_opt_text(&mut m, B_INSTRUCTIONS, &b.instructions);
    insert_value(&mut m, B_AUTHORITY, encode_authority_meta(&b.authority));
    insert_bytes(&mut m, B_AUTHORITY_PEER_SHORT, b.authority_peer_short);
    insert_bool(&mut m, B_DRILL, b.drill);
    write_map(m)
}

/// Encode an `SosMessage` to canonical CBOR.
pub fn encode_sos(s: &SosMessage) -> Result<Vec<u8>, EmergencyCodecError> {
    s.validate()?;
    let mut m = Map::new();
    insert_u64(&mut m, S_FORMAT_VERSION, u64::from(s.format_version));
    insert_i64(&mut m, S_LATITUDE, i64::from(s.latitude));
    insert_i64(&mut m, S_LONGITUDE, i64::from(s.longitude));
    insert_u64(&mut m, S_ACCURACY_M, u64::from(s.accuracy_m));
    insert_u64(
        &mut m,
        S_LOCATION_SOURCE,
        u64::from(s.location_source.as_u8()),
    );
    insert_u64(&mut m, S_TIMESTAMP, u64::from(s.timestamp));
    insert_u64(&mut m, S_KIND, u64::from(s.kind.as_u8()));
    insert_bytes(&mut m, S_ORIGINAL_MESSAGE_ID, s.original_message_id);
    if let Some(r) = s.reason {
        insert_u64(&mut m, S_REASON, u64::from(r.as_u8()));
    }
    write_map(m)
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

fn get(entries: &[(Value, Value)], k: u8) -> Option<&Value> {
    let want = Value::Integer(Integer::from(u64::from(k)));
    entries.iter().find(|(key, _)| *key == want).map(|(_, v)| v)
}

/// Reject maps containing duplicate integer keys (RT-011). Canonical CBOR
/// (RFC 8949 §4.2) requires unique keys; a hostile encoder could otherwise
/// smuggle two values for one field and create parser-disagreement (first-vs-
/// last-wins) between IRIS and a foreign decoder.
fn no_duplicate_keys(entries: &[(Value, Value)], ctx: &str) -> Result<(), EmergencyCodecError> {
    let mut seen: Vec<i128> = Vec::new();
    for (key, _) in entries {
        let Value::Integer(i) = key else {
            continue; // non-integer keys are tolerated-but-ignored (append-only)
        };
        let n: i128 = (*i).into();
        if seen.contains(&n) {
            return Err(EmergencyCodecError::Decode(format!(
                "duplicate key {n} in {ctx} (CDE violation)"
            )));
        }
        seen.push(n);
    }
    Ok(())
}

fn expect_map(value: &Value) -> Result<&[(Value, Value)], EmergencyCodecError> {
    match value {
        Value::Map(m) => Ok(m.as_slice()),
        _ => Err(EmergencyCodecError::NotAMap),
    }
}

fn as_u64(value: &Value) -> Result<u64, EmergencyCodecError> {
    match value {
        Value::Integer(i) => {
            let n: i128 = (*i).into();
            u64::try_from(n).map_err(|_| EmergencyCodecError::InvalidField("unsigned integer"))
        }
        _ => Err(EmergencyCodecError::InvalidField("unsigned integer")),
    }
}

fn as_i64(value: &Value) -> Result<i64, EmergencyCodecError> {
    match value {
        Value::Integer(i) => {
            let n: i128 = (*i).into();
            i64::try_from(n).map_err(|_| EmergencyCodecError::InvalidField("signed integer"))
        }
        _ => Err(EmergencyCodecError::InvalidField("signed integer")),
    }
}

fn as_text(value: &Value, field: &'static str) -> Result<String, EmergencyCodecError> {
    match value {
        Value::Text(s) => Ok(s.clone()),
        _ => Err(EmergencyCodecError::InvalidField(field)),
    }
}

fn as_bytes<const N: usize>(
    value: &Value,
    field: &'static str,
) -> Result<[u8; N], EmergencyCodecError> {
    match value {
        Value::Bytes(b) => b
            .clone()
            .try_into()
            .map_err(|_| EmergencyCodecError::InvalidField(field)),
        _ => Err(EmergencyCodecError::InvalidField(field)),
    }
}

fn as_bool(value: &Value, field: &'static str) -> Result<bool, EmergencyCodecError> {
    match value {
        Value::Bool(b) => Ok(*b),
        _ => Err(EmergencyCodecError::InvalidField(field)),
    }
}

/// Decode a required u8 enum code; out-of-range codes reject loudly.
fn enum_code<F>(v: u64, field: &'static str, from_u8: F) -> Result<u8, EmergencyCodecError>
where
    F: Fn(u8) -> Option<u8>,
{
    let code = u8::try_from(v).map_err(|_| EmergencyCodecError::InvalidEnumCode(v, field))?;
    from_u8(code).ok_or(EmergencyCodecError::InvalidEnumCode(v, field))
}

fn decode_authority_meta(m: &[(Value, Value)]) -> Result<AuthorityMeta, EmergencyCodecError> {
    no_duplicate_keys(m, "authority")?;
    let issuer_name = match get(m, A_ISSUER_NAME) {
        Some(v) => Some(as_text(v, "authority.issuer_name")?),
        None => None,
    };
    let geo_scope = match get(m, A_GEO_SCOPE) {
        Some(v) => Some(as_text(v, "authority.geo_scope")?),
        None => None,
    };
    let functional_scope = match get(m, A_FUNCTIONAL_SCOPE) {
        Some(v) => Some(as_text(v, "authority.functional_scope")?),
        None => None,
    };
    let max_severity = match get(m, A_MAX_SEVERITY) {
        Some(v) => u8::try_from(as_u64(v)?)
            .map_err(|_| EmergencyCodecError::InvalidField("authority.max_severity (0-255)"))?,
        None => 0,
    };
    Ok(AuthorityMeta {
        issuer_name,
        geo_scope,
        functional_scope,
        max_severity,
    })
}

/// Decode an `EmergencyBroadcast`. Unknown top-level keys are ignored
/// (append-only extension, ADR-0011).
pub fn decode_broadcast(bytes: &[u8]) -> Result<EmergencyBroadcast, EmergencyCodecError> {
    let value: Value =
        ciborium::de::from_reader(bytes).map_err(|e| EmergencyCodecError::Decode(e.to_string()))?;
    let map = expect_map(&value)?;
    no_duplicate_keys(map, "EmergencyBroadcast")?;

    let get_u = |k: u8| match get(map, k) {
        Some(v) => as_u64(v),
        None => Err(EmergencyCodecError::MissingField(field_name(k))),
    };

    let severity = enum_code(get_u(B_SEVERITY)?, "severity", |c| {
        Severity::from_u8(c).map(|s| s.as_u8())
    })?;
    let certainty = enum_code(get_u(B_CERTAINTY)?, "certainty", |c| {
        Certainty::from_u8(c).map(|x| x.as_u8())
    })?;
    let message_type = enum_code(get_u(B_MESSAGE_TYPE)?, "message_type", |c| {
        AlertMessageType::from_u8(c).map(|x| x.as_u8())
    })?;

    let authority_map = match get(map, B_AUTHORITY) {
        Some(Value::Map(m)) => m.as_slice(),
        Some(_) => return Err(EmergencyCodecError::InvalidField("authority (map)")),
        None => return Err(EmergencyCodecError::MissingField("authority")),
    };

    let b = EmergencyBroadcast {
        format_version: u8::try_from(get_u(B_FORMAT_VERSION)?)
            .map_err(|_| EmergencyCodecError::InvalidField("format_version"))?,
        broadcast_id: match get(map, B_BROADCAST_ID) {
            Some(v) => as_bytes(v, "broadcast_id (16 bytes)")?,
            None => return Err(EmergencyCodecError::MissingField("broadcast_id")),
        },
        republish_id: match get(map, B_REPUBLISH_ID) {
            Some(v) => Some(as_bytes(v, "republish_id (16 bytes)")?),
            None => None,
        },
        issued_at: get_u(B_ISSUED_AT)?,
        expires_at: get_u(B_EXPIRES_AT)?,
        severity: Severity::from_u8(severity).ok_or(EmergencyCodecError::InvalidEnumCode(
            u64::from(severity),
            "severity",
        ))?,
        certainty: Certainty::from_u8(certainty).ok_or(EmergencyCodecError::InvalidEnumCode(
            u64::from(certainty),
            "certainty",
        ))?,
        message_type: AlertMessageType::from_u8(message_type).ok_or(
            EmergencyCodecError::InvalidEnumCode(u64::from(message_type), "message_type"),
        )?,
        area_code: match get(map, B_AREA_CODE) {
            Some(v) => as_text(v, "area_code")?,
            None => return Err(EmergencyCodecError::MissingField("area_code")),
        },
        language: match get(map, B_LANGUAGE) {
            Some(v) => as_text(v, "language")?,
            None => return Err(EmergencyCodecError::MissingField("language")),
        },
        headline: match get(map, B_HEADLINE) {
            Some(v) => as_text(v, "headline")?,
            None => return Err(EmergencyCodecError::MissingField("headline")),
        },
        instructions: match get(map, B_INSTRUCTIONS) {
            Some(v) => Some(as_text(v, "instructions")?),
            None => None,
        },
        authority: decode_authority_meta(authority_map)?,
        authority_peer_short: match get(map, B_AUTHORITY_PEER_SHORT) {
            Some(v) => Some(as_bytes(v, "authority_peer_short (16 bytes)")?),
            None => None,
        },
        drill: match get(map, B_DRILL) {
            Some(v) => as_bool(v, "drill")?,
            None => false,
        },
    };
    b.validate()?;
    Ok(b)
}

/// Decode an `SosMessage`. Unknown top-level keys are ignored.
pub fn decode_sos(bytes: &[u8]) -> Result<SosMessage, EmergencyCodecError> {
    let value: Value =
        ciborium::de::from_reader(bytes).map_err(|e| EmergencyCodecError::Decode(e.to_string()))?;
    let map = expect_map(&value)?;
    no_duplicate_keys(map, "SosMessage")?;

    let get_u = |k: u8| match get(map, k) {
        Some(v) => as_u64(v),
        None => Err(EmergencyCodecError::MissingField(sos_field_name(k))),
    };

    let location_source = enum_code(get_u(S_LOCATION_SOURCE)?, "location_source", |c| {
        LocationSource::from_u8(c).map(|x| x.as_u8())
    })?;
    let kind = enum_code(get_u(S_KIND)?, "kind", |c| {
        SosKind::from_u8(c).map(|x| x.as_u8())
    })?;

    let s = SosMessage {
        format_version: u8::try_from(get_u(S_FORMAT_VERSION)?)
            .map_err(|_| EmergencyCodecError::InvalidField("format_version"))?,
        latitude: match get(map, S_LATITUDE) {
            Some(v) => i32::try_from(as_i64(v)?)
                .map_err(|_| EmergencyCodecError::InvalidField("latitude (i32)"))?,
            None => return Err(EmergencyCodecError::MissingField("latitude")),
        },
        longitude: match get(map, S_LONGITUDE) {
            Some(v) => i32::try_from(as_i64(v)?)
                .map_err(|_| EmergencyCodecError::InvalidField("longitude (i32)"))?,
            None => return Err(EmergencyCodecError::MissingField("longitude")),
        },
        accuracy_m: match get(map, S_ACCURACY_M) {
            Some(v) => u16::try_from(as_u64(v)?)
                .map_err(|_| EmergencyCodecError::InvalidField("accuracy_m (u16)"))?,
            None => 0,
        },
        location_source: LocationSource::from_u8(location_source).ok_or(
            EmergencyCodecError::InvalidEnumCode(u64::from(location_source), "location_source"),
        )?,
        timestamp: match get(map, S_TIMESTAMP) {
            Some(v) => u32::try_from(as_u64(v)?)
                .map_err(|_| EmergencyCodecError::InvalidField("timestamp (u32)"))?,
            None => return Err(EmergencyCodecError::MissingField("timestamp")),
        },
        kind: SosKind::from_u8(kind).ok_or(EmergencyCodecError::InvalidEnumCode(
            u64::from(kind),
            "kind",
        ))?,
        original_message_id: match get(map, S_ORIGINAL_MESSAGE_ID) {
            Some(v) => Some(as_bytes(v, "original_message_id (16 bytes)")?),
            None => None,
        },
        reason: match get(map, S_REASON) {
            Some(v) => {
                let code = u8::try_from(as_u64(v)?)
                    .map_err(|_| EmergencyCodecError::InvalidField("reason (0-255)"))?;
                Some(
                    SosReason::from_u8(code).ok_or(EmergencyCodecError::InvalidEnumCode(
                        u64::from(code),
                        "reason",
                    ))?,
                )
            }
            None => None,
        },
    };
    s.validate()?;
    Ok(s)
}

fn field_name(k: u8) -> &'static str {
    match k {
        B_FORMAT_VERSION => "format_version",
        B_BROADCAST_ID => "broadcast_id",
        B_REPUBLISH_ID => "republish_id",
        B_ISSUED_AT => "issued_at",
        B_EXPIRES_AT => "expires_at",
        B_SEVERITY => "severity",
        B_CERTAINTY => "certainty",
        B_MESSAGE_TYPE => "message_type",
        B_AREA_CODE => "area_code",
        B_LANGUAGE => "language",
        B_HEADLINE => "headline",
        B_INSTRUCTIONS => "instructions",
        B_AUTHORITY => "authority",
        B_AUTHORITY_PEER_SHORT => "authority_peer_short",
        B_DRILL => "drill",
        _ => "unknown",
    }
}

fn sos_field_name(k: u8) -> &'static str {
    match k {
        S_FORMAT_VERSION => "format_version",
        S_LATITUDE => "latitude",
        S_LONGITUDE => "longitude",
        S_ACCURACY_M => "accuracy_m",
        S_LOCATION_SOURCE => "location_source",
        S_TIMESTAMP => "timestamp",
        S_KIND => "kind",
        S_ORIGINAL_MESSAGE_ID => "original_message_id",
        S_REASON => "reason",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_broadcast() -> EmergencyBroadcast {
        EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [0xab; 16],
            republish_id: Some([0xcd; 16]),
            issued_at: 1_723_334_400,
            expires_at: 1_723_334_400 + 3600,
            severity: Severity::Critical,
            certainty: Certainty::Observed,
            message_type: AlertMessageType::Alert,
            area_code: "IN-GJ".into(),
            language: "en".into(),
            headline: "Dam failure imminent".into(),
            instructions: Some("Move to high ground".into()),
            authority: AuthorityMeta {
                issuer_name: Some("GSDMA".into()),
                geo_scope: Some("IN-GJ".into()),
                functional_scope: Some("disaster".into()),
                max_severity: 4,
            },
            authority_peer_short: Some([0xef; 16]),
            drill: false,
        }
    }

    fn sample_sos() -> SosMessage {
        SosMessage {
            format_version: 1,
            latitude: 23_022_500,
            longitude: 72_571_250,
            accuracy_m: 12,
            location_source: LocationSource::Gps,
            timestamp: 1_723_334_400,
            kind: SosKind::Sos,
            original_message_id: None,
            reason: Some(SosReason::Trapped),
        }
    }

    #[test]
    fn broadcast_round_trip() {
        let b = sample_broadcast();
        let enc = encode_broadcast(&b).unwrap();
        assert_eq!(decode_broadcast(&enc).unwrap(), b);
    }

    #[test]
    fn broadcast_minimal_round_trip() {
        let b = EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [1u8; 16],
            republish_id: None,
            issued_at: 1_000,
            expires_at: 1_000 + 3600,
            severity: Severity::Minor,
            certainty: Certainty::Unlikely,
            message_type: AlertMessageType::Cancel,
            area_code: "IN".into(),
            language: "en".into(),
            headline: "C".into(),
            instructions: None,
            authority: AuthorityMeta {
                issuer_name: None,
                geo_scope: None,
                functional_scope: None,
                max_severity: 0,
            },
            authority_peer_short: None,
            drill: false,
        };
        let enc = encode_broadcast(&b).unwrap();
        assert_eq!(decode_broadcast(&enc).unwrap(), b);
    }

    #[test]
    fn sos_round_trip() {
        let s = sample_sos();
        let enc = encode_sos(&s).unwrap();
        assert_eq!(decode_sos(&enc).unwrap(), s);
    }

    #[test]
    fn sos_cancel_round_trip() {
        let s = SosMessage {
            format_version: 1,
            latitude: 0,
            longitude: 0,
            accuracy_m: 0,
            location_source: LocationSource::None,
            timestamp: 1_723_334_400,
            kind: SosKind::Cancel,
            original_message_id: Some([9u8; 16]),
            reason: None,
        };
        let enc = encode_sos(&s).unwrap();
        assert_eq!(decode_sos(&enc).unwrap(), s);
    }

    #[test]
    fn sos_fits_p0_budget() {
        // P0 payload ceiling is 84 bytes (255 - 171 fixed overhead).
        let worst = SosMessage {
            format_version: 1,
            latitude: 23_022_500,
            longitude: 72_571_250,
            accuracy_m: 12,
            location_source: LocationSource::Gps,
            timestamp: 1_723_334_400,
            kind: SosKind::Cancel,
            original_message_id: Some([0xff; 16]),
            reason: Some(SosReason::Other),
        };
        let enc = encode_sos(&worst).unwrap();
        assert!(
            enc.len() <= 84,
            "SOS payload must fit P0 84-byte ceiling, got {}",
            enc.len()
        );
    }

    #[test]
    fn out_of_range_enum_rejected() {
        let b = sample_broadcast();
        // Tamper severity to 99 via re-encode trick.
        let value: Value = ciborium::de::from_reader(&encode_broadcast(&b).unwrap()[..]).unwrap();
        let mut m = match value {
            Value::Map(m) => m,
            _ => unreachable!(),
        };
        for (k, v) in m.iter_mut() {
            if *k == Value::Integer(Integer::from(u64::from(B_SEVERITY))) {
                *v = Value::Integer(Integer::from(99u64));
            }
        }
        let mut out = Vec::new();
        ciborium::ser::into_writer(&Value::Map(m), &mut out).unwrap();
        assert!(matches!(
            decode_broadcast(&out),
            Err(EmergencyCodecError::InvalidEnumCode(99, "severity"))
        ));
    }

    #[test]
    fn unknown_keys_tolerated() {
        let b = sample_broadcast();
        let mut value: Value =
            ciborium::de::from_reader(&encode_broadcast(&b).unwrap()[..]).unwrap();
        if let Value::Map(m) = &mut value {
            m.push((
                Value::Integer(Integer::from(99u64)),
                Value::Text("future-ext".into()),
            ));
        }
        let mut out = Vec::new();
        ciborium::ser::into_writer(&value, &mut out).unwrap();
        assert_eq!(decode_broadcast(&out).unwrap(), b);
    }

    #[test]
    fn known_duplicate_key_rejected() {
        // RT-011: two values for one CDE key must be rejected (parser
        // disagreement defense; RFC 8949 §4.2 unique-key requirement).
        let b = sample_broadcast();
        let mut value: Value =
            ciborium::de::from_reader(&encode_broadcast(&b).unwrap()[..]).unwrap();
        if let Value::Map(m) = &mut value {
            let key = Value::Integer(Integer::from(u64::from(B_HEADLINE)));
            // Find the existing value and push a second entry for the same key.
            let existing_v = m
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| v.clone())
                .unwrap();
            m.push((key, existing_v));
        }
        let mut out = Vec::new();
        ciborium::ser::into_writer(&value, &mut out).unwrap();
        let err = decode_broadcast(&out).unwrap_err();
        assert!(
            err.to_string().contains("duplicate key"),
            "duplicate CDE key must be rejected loudly; got {err}"
        );
    }

    #[test]
    fn oversized_headline_rejected() {
        let mut b = sample_broadcast();
        b.headline = "x".repeat(97);
        assert!(matches!(
            encode_broadcast(&b),
            Err(EmergencyCodecError::Invalid(ModelError::FieldTooLong(_)))
        ));
    }

    #[test]
    fn missing_required_field_rejected() {
        let b = sample_broadcast();
        let mut value: Value =
            ciborium::de::from_reader(&encode_broadcast(&b).unwrap()[..]).unwrap();
        if let Value::Map(m) = &mut value {
            m.retain(|(k, _)| *k != Value::Integer(Integer::from(u64::from(B_HEADLINE))));
        }
        let mut out = Vec::new();
        ciborium::ser::into_writer(&value, &mut out).unwrap();
        assert!(matches!(
            decode_broadcast(&out),
            Err(EmergencyCodecError::MissingField("headline"))
        ));
    }

    #[test]
    fn truncated_data_fails_cleanly() {
        let enc = encode_broadcast(&sample_broadcast()).unwrap();
        for cut in [1usize, enc.len() / 2, enc.len() - 1] {
            assert!(decode_broadcast(&enc[..cut]).is_err(), "cut={cut}");
        }
    }

    #[test]
    fn canonical_deterministic_encoding() {
        // Same logical message must encode to byte-identical output.
        let a = sample_broadcast();
        let b = sample_broadcast();
        assert_eq!(encode_broadcast(&a).unwrap(), encode_broadcast(&b).unwrap());
    }
}
