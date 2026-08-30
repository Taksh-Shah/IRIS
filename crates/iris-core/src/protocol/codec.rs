//! Canonical CBOR (CDE) codec for the IRIS envelope.
//!
//! Implements RFC 8949 §4.2 "Core Deterministic Encoding":
//! - sorted integer map keys (ascending),
//! - definite-length byte strings and arrays,
//! - minimal integer encoding (no leading zeros),
//! - no indefinite-length items.
//!
//! Maps are built as `Vec<(u8, Value)>` (key, value) with keys in ascending
//! wire order and explicitly sorted at emission, because `ciborium::Value`
//! does not implement `Ord` (so a plain `BTreeMap` is unavailable) and its
//! `Value::Map` holds `Vec<(Value, Value)>` preserving insertion order.
//!
//! ## Signature scope (ADR-0011)
//!
//! `encode_for_signing()` serializes fields **1–7, 9–14, 16, and 18**:
//! - field 8 (`hop_count`) is **excluded** — relays increment it without the
//!   sender's key (ADR-0011 Open Issue 1; MESSAGE_ENVELOPE.md relay section),
//! - field 15 (`signature`) is the signature itself,
//! - field 17 (`routing_hints`) is **excluded** — a mutable relay-scheduling
//!   hint that may be updated in transit without re-signing,
//! - fields 16 (`encryption_hdr`) and 18 (`auth_cert_chain`) are **included**:
//!   excluding them was exploitable — a relay could flip the ephemeral public
//!   key (field 16) and the message would still verify but fail to decrypt on
//!   every subsequent path, silently destroying multi-path redundancy; similarly,
//!   stripping field 18 let any relay suppress authority emergency alerts by
//!   deleting the ACL chain while the signature still verified. See the inline
//!   comments in `build_map` for full details.
//!
//! ## AEAD associated-data scope (CRYPTO-001)
//!
//! `encode_for_aead()` serializes fields **1–7 and 9–11 and 14** — the
//! immutable message-identity fields. It is the signature scope **minus**
//! field 12 (`payload_hash`, ciphertext-dependent) and field 13 (`payload`,
//! the ciphertext itself). It is computed BEFORE encryption (no circular
//! dependency) and binds the ciphertext to the message identity so a
//! ciphertext cannot be swapped between messages.
//!
//! ## Extension tolerance (ADR-0011)
//!
//! `decode()` ignores any map key not in 1–18 (append-only extension
//! mechanism); a node never fails on newer fields.

use ciborium::value::{Integer, Value};
use thiserror::Error;

use crate::message::MessagePriority;

use super::content_type::ContentType;
use super::envelope::{EncryptionHdr, Envelope, RoutingHints, P0_MAX_ENVELOPE_BYTES};
use super::message_id::MessageId;
use super::PROTOCOL_VERSION;

/// Codec errors. All are variants a relay treats as "DROP + log": a message
/// that cannot be decoded must never be forwarded.
#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("CBOR encode failed: {0}")]
    Encode(#[from] ciborium::ser::Error<std::io::Error>),
    #[error("CBOR decode failed: {0}")]
    Decode(#[from] ciborium::de::Error<std::io::Error>),
    #[error("root value is not a CBOR map")]
    NotAMap,
    #[error("missing required field {0}")]
    MissingField(&'static str),
    #[error("field {0} has invalid type")]
    InvalidField(&'static str),
    #[error("unsupported protocol version {0}")]
    UnsupportedVersion(u64),
    #[error("invalid priority code {0}")]
    InvalidPriority(u64),
    #[error("invalid content type code {0}")]
    InvalidContentType(u64),
    #[error("invalid sender_id length {0} (expected 16 or 32)")]
    InvalidSenderIdLen(usize),
    #[error("invalid recipient_id length {0} (expected 4 or 32)")]
    InvalidRecipientIdLen(usize),
    #[error("payload_size ({0}) does not match payload length ({1})")]
    PayloadSizeMismatch(u64, usize),
    #[error("signature must be exactly 64 bytes, got {0}")]
    InvalidSignatureLen(usize),
}

// ---------------------------------------------------------------------------
// Map building (wire key u8 -> ciborium Value), sorted at emission.
// ---------------------------------------------------------------------------

/// In-progress map: `(wire_key, value)` pairs, built in ascending key order.
type Map = Vec<(u8, Value)>;

fn insert_u64(map: &mut Map, k: u8, v: u64) {
    map.push((k, Value::Integer(Integer::from(v))));
}

fn insert_bytes(map: &mut Map, k: u8, v: &[u8]) {
    map.push((k, Value::Bytes(v.to_vec())));
}

fn insert_value(map: &mut Map, k: u8, v: Value) {
    map.push((k, v));
}

/// Convert an in-progress map to a CBOR map, keys sorted ascending (CDE).
fn map_to_value(mut map: Map) -> Value {
    map.sort_by_key(|(k, _)| *k);
    let pairs = map
        .into_iter()
        .map(|(k, v)| (Value::Integer(Integer::from(u64::from(k))), v))
        .collect();
    Value::Map(pairs)
}

fn write_map(map: Map) -> Result<Vec<u8>, EnvelopeError> {
    let value = map_to_value(map);
    let mut out = Vec::new();
    ciborium::ser::into_writer(&value, &mut out)?;
    Ok(out)
}

fn encode_encryption_hdr(hdr: &EncryptionHdr) -> Value {
    let mut m = Map::new();
    insert_bytes(&mut m, 1, &hdr.ephemeral_pubkey);
    insert_bytes(&mut m, 2, &hdr.nonce);
    if let Some(kid) = hdr.key_id {
        insert_bytes(&mut m, 3, &kid);
    }
    map_to_value(m)
}

fn encode_routing_hints(rh: &RoutingHints) -> Value {
    let mut m = Map::new();
    if let Some(region) = &rh.last_known_region {
        insert_value(&mut m, 1, Value::Text(region.clone()));
    }
    if let Some(gw) = rh.gateway_seen_via {
        insert_bytes(&mut m, 2, &gw);
    }
    if let Some(p) = rh.delivery_prob {
        insert_value(&mut m, 3, Value::Float(p));
    }
    if let Some(req) = rh.requires_gateway {
        insert_value(&mut m, 4, Value::Bool(req));
    }
    if let Some(relays) = &rh.preferred_relays {
        let arr = relays.iter().map(|r| Value::Bytes(r.to_vec())).collect();
        insert_value(&mut m, 5, Value::Array(arr));
    }
    map_to_value(m)
}

/// Build the (sorted-at-emission) map for an envelope.
///
/// `scope` controls which fields are included:
/// - `Scope::Full`: the full wire envelope (fields 1–18).
/// - `Scope::Signing`: fields 1–7, 9–14, 16, 18 (signature scope), excluding
///   8 (`hop_count`, mutated by relays), 15 (`signature` itself) and
///   17 (`routing_hints`, a mutable scheduling hint).
/// - `Scope::Aead`: fields 1–7, 9–11, 14 (AEAD associated data, CRYPTO-001),
///   excluding 12 (`payload_hash`) and 13 (`payload`) from the signing scope,
///   and excluding 16 (produced by the encryption it would authenticate).
fn build_map(env: &Envelope, scope: Scope) -> Map {
    let mut m = Map::new();
    insert_u64(&mut m, 1, u64::from(env.version));
    insert_bytes(&mut m, 2, env.message_id.as_bytes());
    insert_bytes(&mut m, 3, &env.sender_id);
    insert_bytes(&mut m, 4, &env.recipient_id);
    insert_u64(&mut m, 5, u64::from(env.priority.as_u8()));
    insert_u64(&mut m, 6, env.ttl_seconds);
    insert_u64(&mut m, 7, env.timestamp);
    if matches!(scope, Scope::Full) {
        // Field 8 (hop_count): relays increment it in transit, so it is
        // excluded from both the signature scope (ADR-0011) and the AEAD AAD
        // (CRYPTO-001). Only the full wire envelope carries it.
        insert_u64(&mut m, 8, u64::from(env.hop_count));
    }
    if let Some(max_hops) = env.max_hops {
        insert_u64(&mut m, 9, u64::from(max_hops));
    }
    insert_u64(&mut m, 10, u64::from(env.payload_type.as_u8()));
    insert_u64(&mut m, 11, env.payload_size);
    if !matches!(scope, Scope::Aead) {
        // Payload hash (12) and payload (13) are excluded from AEAD AAD: the
        // hash depends on the ciphertext and the AAD must be computable BEFORE
        // encryption (no circular dependency).
        insert_bytes(&mut m, 12, &env.payload_hash);
        insert_bytes(&mut m, 13, &env.payload);
    }
    if let Some(ref p) = env.payload_ref {
        insert_bytes(&mut m, 14, p.as_bytes());
    }
    if matches!(scope, Scope::Full) {
        if let Some(sig) = env.signature {
            insert_bytes(&mut m, 15, &sig);
        }
        if let Some(rh) = &env.routing_hints {
            insert_value(&mut m, 17, encode_routing_hints(rh));
        }
    }

    // Fields 16 and 18 are covered by the SIGNATURE as well as the full wire
    // form. They are still excluded from the AEAD AAD: the encryption header is
    // produced *by* encryption, so binding it into that encryption's own AAD
    // would be circular.
    //
    // Leaving them signature-exempt was exploitable by any relay:
    //
    //  - field 16 (`encryption_hdr`) carries the ephemeral public key and the
    //    nonce. Flipping one byte still verified, so the message was recorded
    //    in dedup and advanced the replay high-water mark, and only then failed
    //    to decrypt. Every later copy arriving over a redundant mesh path was
    //    then dropped as a duplicate — silently destroying the message in a
    //    system whose entire availability model is multipath flooding.
    //
    //  - field 18 (`auth_cert_chain`) is what the emergency ACL checks. Simply
    //    deleting it still verified, and the receiver then dropped the alert as
    //    `InvalidAuthority` — a silent, untraceable suppression of emergency
    //    broadcasts for the whole downstream subtree.
    if !matches!(scope, Scope::Aead) {
        if let Some(hdr) = &env.encryption_hdr {
            insert_value(&mut m, 16, encode_encryption_hdr(hdr));
        }
        if let Some(chain) = &env.auth_cert_chain {
            let arr = chain.iter().map(|c| Value::Bytes(c.clone())).collect();
            insert_value(&mut m, 18, Value::Array(arr));
        }
    }
    m
}

/// Which field scope to emit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Scope {
    Full,
    Signing,
    Aead,
}

/// Encode a full envelope (fields 1–18 present) as canonical CBOR.
pub fn encode(env: &Envelope) -> Result<Vec<u8>, EnvelopeError> {
    if env.payload_size != env.payload.len() as u64 {
        return Err(EnvelopeError::PayloadSizeMismatch(
            env.payload_size,
            env.payload.len(),
        ));
    }
    if let Some(sig) = env.signature {
        if sig.len() != 64 {
            return Err(EnvelopeError::InvalidSignatureLen(sig.len()));
        }
    }
    write_map(build_map(env, Scope::Full))
}

/// Encode the signature-scope bytes: fields 1–7, 9–14, 16, and 18.
///
/// These are the exact bytes an Ed25519 signature must cover
/// (ADR-0011 / MESSAGE_ENVELOPE.md signing procedure). Field 8
/// (hop_count) and field 17 (routing_hints) are excluded; fields 16
/// (encryption_hdr) and 18 (auth_cert_chain) are included — see the
/// module-level "Signature scope" section for the security rationale.
pub fn encode_for_signing(env: &Envelope) -> Result<Vec<u8>, EnvelopeError> {
    write_map(build_map(env, Scope::Signing))
}

/// Encode the AEAD associated-data bytes: fields 1–7, 9–11, 14.
///
/// These bind a ChaCha20-Poly1305 ciphertext to the immutable message identity
/// (CRYPTO-001). They are computable BEFORE encryption (no circular dependency
/// on the ciphertext-derived `payload_hash`/`payload`). Does not change
/// `encode_for_signing` (additive seam).
pub fn encode_for_aead(env: &Envelope) -> Result<Vec<u8>, EnvelopeError> {
    write_map(build_map(env, Scope::Aead))
}

/// Structurally verify an envelope against the wire contract.
///
/// This is **not** cryptographic signature verification (that is the
/// `CryptoProvider` seam, BLK-0001). It recomputes the signing-scope bytes,
/// checks the signature field is present and 64 bytes, verifies the payload
/// hash matches the payload, and checks content size consistency. Returns the
/// signing-scope bytes for later Ed25519 verification.
pub fn verify_signing_bytes(env: &Envelope) -> Result<Vec<u8>, EnvelopeError> {
    if env.signature.is_none() {
        return Err(EnvelopeError::MissingField("signature (field 15)"));
    }
    if env.payload_size != env.payload.len() as u64 {
        return Err(EnvelopeError::PayloadSizeMismatch(
            env.payload_size,
            env.payload.len(),
        ));
    }
    if env.payload_hash != Envelope::compute_payload_hash(&env.payload) {
        return Err(EnvelopeError::InvalidField("payload_hash (field 12)"));
    }
    encode_for_signing(env)
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// Wire field names for error messages (1–18).
fn field_name(k: u8) -> &'static str {
    match k {
        1 => "version",
        2 => "message_id",
        3 => "sender_id",
        4 => "recipient_id",
        5 => "priority",
        6 => "ttl_seconds",
        7 => "timestamp",
        8 => "hop_count",
        9 => "max_hops",
        10 => "payload_type",
        11 => "payload_size",
        12 => "payload_hash",
        13 => "payload",
        14 => "payload_ref",
        15 => "signature",
        16 => "encryption_hdr",
        17 => "routing_hints",
        18 => "auth_cert_chain",
        _ => "unknown",
    }
}

/// Look up a field by its wire key in a decoded `Value::Map`.
fn get(entries: &[(Value, Value)], k: u8) -> Option<&Value> {
    let want = Value::Integer(Integer::from(u64::from(k)));
    entries.iter().find(|(key, _)| *key == want).map(|(_, v)| v)
}

fn expect_map(value: &Value) -> Result<&[(Value, Value)], EnvelopeError> {
    match value {
        Value::Map(m) => Ok(m),
        _ => Err(EnvelopeError::NotAMap),
    }
}

fn as_u64(value: &Value) -> Result<u64, EnvelopeError> {
    match value {
        Value::Integer(i) => {
            let n: i128 = (*i).into();
            u64::try_from(n).map_err(|_| EnvelopeError::InvalidField("unsigned integer"))
        }
        _ => Err(EnvelopeError::InvalidField("unsigned integer")),
    }
}

fn as_bytes(value: &Value) -> Result<&[u8], EnvelopeError> {
    match value {
        Value::Bytes(b) => Ok(b),
        _ => Err(EnvelopeError::InvalidField("byte string")),
    }
}

fn get_u64(map: &[(Value, Value)], k: u8) -> Result<u64, EnvelopeError> {
    let v = get(map, k).ok_or(EnvelopeError::MissingField(field_name(k)))?;
    as_u64(v)
}

fn get_bytes(map: &[(Value, Value)], k: u8) -> Result<Vec<u8>, EnvelopeError> {
    let v = get(map, k).ok_or(EnvelopeError::MissingField(field_name(k)))?;
    Ok(as_bytes(v)?.to_vec())
}

fn get_opt_u64(map: &[(Value, Value)], k: u8) -> Result<Option<u64>, EnvelopeError> {
    match get(map, k) {
        Some(v) => as_u64(v).map(Some),
        None => Ok(None),
    }
}

fn get_opt_bytes(map: &[(Value, Value)], k: u8) -> Result<Option<Vec<u8>>, EnvelopeError> {
    match get(map, k) {
        Some(v) => Ok(Some(as_bytes(v)?.to_vec())),
        None => Ok(None),
    }
}

fn decode_encryption_hdr(m: &[(Value, Value)]) -> Result<EncryptionHdr, EnvelopeError> {
    let epub = get_bytes(m, 1)?;
    let nonce = get_bytes(m, 2)?;
    let key_id = get_opt_bytes(m, 3)?;
    let ephemeral_pubkey: [u8; 32] = epub
        .try_into()
        .map_err(|_| EnvelopeError::InvalidField("encryption_hdr.ephemeral_pubkey (32 bytes)"))?;
    let nonce: [u8; 12] = nonce
        .try_into()
        .map_err(|_| EnvelopeError::InvalidField("encryption_hdr.nonce (12 bytes)"))?;
    let key_id = match key_id {
        Some(k) => Some(
            k.try_into()
                .map_err(|_| EnvelopeError::InvalidField("encryption_hdr.key_id (4 bytes)"))?,
        ),
        None => None,
    };
    Ok(EncryptionHdr {
        ephemeral_pubkey,
        nonce,
        key_id,
    })
}

fn decode_routing_hints(m: &[(Value, Value)]) -> Result<RoutingHints, EnvelopeError> {
    let mut rh = RoutingHints::default();
    if let Some(v) = get(m, 1) {
        match v {
            Value::Text(s) => rh.last_known_region = Some(s.clone()),
            _ => {
                return Err(EnvelopeError::InvalidField(
                    "routing_hints.last_known_region",
                ))
            }
        }
    }
    if let Some(gw) = get_opt_bytes(m, 2)? {
        let arr: [u8; 32] = gw.try_into().map_err(|_| {
            EnvelopeError::InvalidField("routing_hints.gateway_seen_via (32 bytes)")
        })?;
        rh.gateway_seen_via = Some(arr);
    }
    if let Some(v) = get(m, 3) {
        match v {
            Value::Float(f) => rh.delivery_prob = Some(*f),
            _ => return Err(EnvelopeError::InvalidField("routing_hints.delivery_prob")),
        }
    }
    if let Some(v) = get(m, 4) {
        match v {
            Value::Bool(b) => rh.requires_gateway = Some(*b),
            _ => {
                return Err(EnvelopeError::InvalidField(
                    "routing_hints.requires_gateway",
                ))
            }
        }
    }
    if let Some(v) = get(m, 5) {
        match v {
            Value::Array(items) => {
                let mut relays = Vec::with_capacity(items.len());
                for item in items {
                    let b = as_bytes(item)?;
                    let arr: [u8; 32] = b.try_into().map_err(|_| {
                        EnvelopeError::InvalidField("routing_hints.preferred_relays (32-byte ids)")
                    })?;
                    relays.push(arr);
                }
                rh.preferred_relays = Some(relays);
            }
            _ => {
                return Err(EnvelopeError::InvalidField(
                    "routing_hints.preferred_relays",
                ))
            }
        }
    }
    Ok(rh)
}

/// Decode a canonical CBOR envelope.
///
/// Unknown map keys (>18) are ignored per the ADR-0011 extension mechanism.
/// Structural validation: version matches current major version; priority and
/// content-type codes are in range; sender/recipient id lengths match the
/// wire contract; payload_size matches the payload bytes.
pub fn decode(bytes: &[u8]) -> Result<Envelope, EnvelopeError> {
    let value: Value = ciborium::de::from_reader(bytes)?;
    let map = expect_map(&value)?;

    let version = get_u64(map, 1)?;
    if version != u64::from(PROTOCOL_VERSION) {
        return Err(EnvelopeError::UnsupportedVersion(version));
    }

    let message_id = MessageId::from_slice(&get_bytes(map, 2)?)
        .ok_or(EnvelopeError::InvalidField("message_id (16 bytes)"))?;

    let sender_id = get_bytes(map, 3)?;
    if !matches!(sender_id.len(), 16 | 32) {
        return Err(EnvelopeError::InvalidSenderIdLen(sender_id.len()));
    }

    let recipient_id = get_bytes(map, 4)?;
    if !matches!(recipient_id.len(), 4 | 32) {
        return Err(EnvelopeError::InvalidRecipientIdLen(recipient_id.len()));
    }

    let priority_code = get_u64(map, 5)?;
    let priority = MessagePriority::from_u8(
        u8::try_from(priority_code).map_err(|_| EnvelopeError::InvalidPriority(priority_code))?,
    )
    .ok_or(EnvelopeError::InvalidPriority(priority_code))?;

    // PM-12: enforce P0 envelope size budget at decode.  An oversized P0 is
    // un-evictable (INV-ROUTE-003), rate-limit exempt, and structurally unable
    // to traverse LoRa — the transport it is designed for.  Reject before it
    // enters storage or the ACK tracker.
    if priority == MessagePriority::P0 && bytes.len() > P0_MAX_ENVELOPE_BYTES {
        return Err(EnvelopeError::InvalidField(
            "P0 SOS envelope exceeds P0_MAX_ENVELOPE_BYTES (255 B)",
        ));
    }

    let ttl_seconds = get_u64(map, 6)?;
    let timestamp = get_u64(map, 7)?;
    let hop_count = u8::try_from(get_u64(map, 8)?)
        .map_err(|_| EnvelopeError::InvalidField("hop_count (0-255)"))?;
    let max_hops = match get_opt_u64(map, 9)? {
        Some(v) => {
            Some(u8::try_from(v).map_err(|_| EnvelopeError::InvalidField("max_hops (0-255)"))?)
        }
        None => None,
    };

    let ct_code = get_u64(map, 10)?;
    let payload_type = ContentType::from_u8(
        u8::try_from(ct_code).map_err(|_| EnvelopeError::InvalidContentType(ct_code))?,
    )
    .ok_or(EnvelopeError::InvalidContentType(ct_code))?;

    let payload_size = get_u64(map, 11)?;
    let payload_hash: [u8; 32] = get_bytes(map, 12)?
        .try_into()
        .map_err(|_| EnvelopeError::InvalidField("payload_hash (32 bytes)"))?;
    let payload = get_bytes(map, 13)?;
    if payload_size != payload.len() as u64 {
        return Err(EnvelopeError::PayloadSizeMismatch(
            payload_size,
            payload.len(),
        ));
    }

    let payload_ref = match get_opt_bytes(map, 14)? {
        Some(b) => Some(
            MessageId::from_slice(&b)
                .ok_or(EnvelopeError::InvalidField("payload_ref (16 bytes)"))?,
        ),
        None => None,
    };

    let signature = match get_opt_bytes(map, 15)? {
        Some(b) => {
            let len = b.len();
            let arr: [u8; 64] = b
                .try_into()
                .map_err(|_| EnvelopeError::InvalidSignatureLen(len))?;
            Some(arr)
        }
        None => None,
    };

    let encryption_hdr = match get(map, 16) {
        Some(Value::Map(m)) => Some(decode_encryption_hdr(m)?),
        Some(_) => return Err(EnvelopeError::InvalidField("encryption_hdr (map)")),
        None => None,
    };

    let routing_hints = match get(map, 17) {
        Some(Value::Map(m)) => Some(decode_routing_hints(m)?),
        Some(_) => return Err(EnvelopeError::InvalidField("routing_hints (map)")),
        None => None,
    };

    let auth_cert_chain = match get(map, 18) {
        Some(Value::Array(items)) => {
            let mut chain = Vec::with_capacity(items.len());
            for item in items {
                chain.push(as_bytes(item)?.to_vec());
            }
            Some(chain)
        }
        Some(_) => return Err(EnvelopeError::InvalidField("auth_cert_chain (array)")),
        None => None,
    };

    Ok(Envelope {
        version: PROTOCOL_VERSION,
        message_id,
        sender_id,
        recipient_id,
        priority,
        ttl_seconds,
        timestamp,
        hop_count,
        max_hops,
        payload_type,
        payload_size,
        payload_hash,
        payload,
        payload_ref,
        signature,
        encryption_hdr,
        routing_hints,
        auth_cert_chain,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::envelope::{EncryptionHdr, RoutingHints};
    use crate::protocol::Envelope;

    const MID: [u8; 16] = hex_literal::hex!("018f1a2b3c4d5e6f708192a3b4c5d6e7");
    const SID: [u8; 16] = hex_literal::hex!("a3bc7e2f112233445566778899aabbcc");
    const HASH: [u8; 32] = [
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
        0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
        0xff, 0x01,
    ];

    fn p0_sos_payload() -> Vec<u8> {
        // 66 bytes of "ciphertext" (0x00..=0x41).
        (0..66u8).collect()
    }

    fn p0_sos_envelope() -> Envelope {
        let payload = p0_sos_payload();
        Envelope {
            version: 1,
            message_id: MID.into(),
            sender_id: SID.to_vec(),
            recipient_id: crate::protocol::EMERGENCY_BROADCAST.to_vec(),
            priority: MessagePriority::P0,
            ttl_seconds: 259_200,
            timestamp: 1_723_334_400,
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Sos,
            payload_size: payload.len() as u64,
            payload_hash: HASH,
            payload,
            payload_ref: None,
            signature: Some([0xAA; 64]),
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    #[test]
    fn p0_sos_encodes_byte_exact_cde() {
        let env = p0_sos_envelope();
        let bytes = encode(&env).unwrap();

        // Build the expected canonical CDE byte stream by hand from the
        // MESSAGE_ENVELOPE.md field layout (map of 13 entries => header 0xad).
        let mut expected: Vec<u8> = Vec::new();
        expected.push(0xad); // map(13)
        expected.extend_from_slice(&[0x01, 0x01]); // version = 1
        expected.extend_from_slice(&[0x02, 0x50]); // message_id bytes(16)
        expected.extend_from_slice(&MID);
        expected.extend_from_slice(&[0x03, 0x50]); // sender_id bytes(16)
        expected.extend_from_slice(&SID);
        expected.extend_from_slice(&[0x04, 0x44, 0x00, 0x00, 0x00, 0x01]); // EMERGENCY_BROADCAST
        expected.extend_from_slice(&[0x05, 0x00]); // P0
        expected.extend_from_slice(&[0x06, 0x1a, 0x00, 0x03, 0xf4, 0x80]); // ttl 259200
        expected.extend_from_slice(&[0x07, 0x1a, 0x66, 0xb7, 0xff, 0x00]); // ts 1723334400 = 0x66B7FF00 (doc hex 0x66B5C000 in PROTOCOL_TEST_VECTORS.md is arithmetically wrong; value column is authoritative)
        expected.extend_from_slice(&[0x08, 0x00]); // hop_count 0
        expected.extend_from_slice(&[0x0a, 0x02]); // payload_type SOS
        expected.extend_from_slice(&[0x0b, 0x18, 0x42]); // payload_size 66
        expected.extend_from_slice(&[0x0c, 0x58, 0x20]); // payload_hash bytes(32)
        expected.extend_from_slice(&HASH);
        expected.extend_from_slice(&[0x0d, 0x58, 0x42]); // payload bytes(66)
        expected.extend_from_slice(&p0_sos_payload());
        expected.extend_from_slice(&[0x0f, 0x58, 0x40]); // signature bytes(64)
        expected.extend_from_slice(&[0xAA; 64]);

        assert_eq!(
            bytes.len(),
            237,
            "P0 SOS byte-exact size (13-entry map, 66 B payload, 64 B sig)"
        );
        assert_eq!(bytes, expected, "canonical CDE bytes must match byte-exact");
        assert!(bytes.len() <= crate::protocol::envelope::P0_MAX_ENVELOPE_BYTES);
    }

    #[test]
    fn p0_sos_round_trips() {
        let env = p0_sos_envelope();
        let bytes = encode(&env).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded, env);
        assert!(decoded.is_p0_abbreviated());
        assert!(decoded.is_emergency_broadcast());
    }

    #[test]
    fn signing_scope_excludes_hop_count_and_signature_and_extensions() {
        // Use an envelope with every optional field populated so scope
        // assertions cover field 9 (max_hops) and 14 (payload_ref).
        let env = Envelope {
            max_hops: Some(20),
            payload_ref: Some(MessageId::new_v7()),
            encryption_hdr: Some(EncryptionHdr {
                ephemeral_pubkey: [1u8; 32],
                nonce: [2u8; 12],
                key_id: None,
            }),
            routing_hints: Some(RoutingHints {
                last_known_region: Some("IN-GJ-19".into()),
                ..Default::default()
            }),
            ..p0_sos_envelope()
        };
        let signing = encode_for_signing(&env).unwrap();
        let value: Value = ciborium::de::from_reader(&signing[..]).unwrap();
        let map = expect_map(&value).unwrap();

        // Signature scope = fields 1..=7, 9..=14.
        for k in [1u8, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 13, 14] {
            assert!(
                get(map, k).is_some(),
                "field {k} must be in signature scope"
            );
        }
        // Field 16 (encryption_hdr) is INSIDE the signature scope: it carries
        // the ephemeral key and nonce, and leaving it unsigned let any relay
        // corrupt a message irrecoverably while the signature still verified.
        assert!(
            get(map, 16).is_some(),
            "encryption_hdr must be covered by the signature"
        );

        // Excluded: 8 (hop_count, relays mutate it), 15 (the signature itself),
        // 17 (routing_hints, a mutable scheduling hint).
        for k in [8u8, 15, 17] {
            assert!(
                get(map, k).is_none(),
                "field {k} must be excluded from signature scope"
            );
        }
        // Encoding for signing must be self-consistent.
        assert_eq!(encode_for_signing(&env).unwrap(), signing);
    }

    #[test]
    fn full_envelope_with_extensions_round_trips() {
        let mut env = Envelope {
            message_id: MessageId::new_v7(),
            sender_id: vec![7u8; 32],
            recipient_id: vec![8u8; 32],
            priority: MessagePriority::P4,
            ttl_seconds: 28_800,
            timestamp: 1_723_334_400,
            hop_count: 2,
            max_hops: Some(20),
            payload_type: ContentType::Text,
            payload: b"hello iris".to_vec(),
            encryption_hdr: Some(EncryptionHdr {
                ephemeral_pubkey: [9u8; 32],
                nonce: [10u8; 12],
                key_id: Some([11u8; 4]),
            }),
            routing_hints: Some(RoutingHints {
                last_known_region: Some("IN-GJ-19".into()),
                gateway_seen_via: Some([12u8; 32]),
                delivery_prob: Some(0.875),
                requires_gateway: Some(true),
                preferred_relays: Some(vec![[13u8; 32], [14u8; 32]]),
            }),
            auth_cert_chain: Some(vec![vec![1, 2, 3], vec![4, 5, 6]]),
            payload_ref: Some(MessageId::new_v7()),
            ..p0_sos_envelope()
        };
        // Keep the remainder of the P0 defaults; fix payload-consistency.
        env.payload_size = env.payload.len() as u64;
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);

        let bytes = encode(&env).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded, env, "full envelope must round-trip");

        let signing = encode_for_signing(&env).unwrap();
        let decoded_signing: Value = ciborium::de::from_reader(&signing[..]).unwrap();
        let smap = expect_map(&decoded_signing).unwrap();
        assert!(get(smap, 16).is_some(), "encryption_hdr must be signed");
        assert!(get(smap, 18).is_some(), "auth_cert_chain must be signed");
        assert!(
            get(smap, 17).is_none(),
            "routing_hints stays unsigned (mutable scheduling hint)"
        );

        // The AEAD AAD must NOT contain field 16: the encryption header is
        // produced by the very encryption whose AAD this is.
        let aad = encode_for_aead(&env).unwrap();
        let decoded_aad: Value = ciborium::de::from_reader(&aad[..]).unwrap();
        let amap = expect_map(&decoded_aad).unwrap();
        assert!(
            get(amap, 16).is_none(),
            "encryption_hdr must stay out of its own AAD"
        );

        // verify_signing_bytes passes when fields are consistent.
        assert!(verify_signing_bytes(&env).is_ok());
    }

    #[test]
    fn relay_cannot_tamper_with_encryption_header_or_auth_chain() {
        // The concrete attacks these fields were open to. A relay mutating
        // either used to leave the signature bytes unchanged, so the tamper was
        // undetectable; now the signing bytes must differ, which is what makes
        // `verify_strict` reject it downstream.
        let env = Envelope {
            encryption_hdr: Some(EncryptionHdr {
                ephemeral_pubkey: [1u8; 32],
                nonce: [2u8; 12],
                key_id: None,
            }),
            auth_cert_chain: Some(vec![vec![0xAA; 8]]),
            ..p0_sos_envelope()
        };
        let baseline = encode_for_signing(&env).unwrap();

        // Attack 1: flip a nonce byte — previously caused permanent message
        // loss via a dedup tombstone plus a decrypt failure.
        let mut nonce_tampered = env.clone();
        nonce_tampered.encryption_hdr = Some(EncryptionHdr {
            ephemeral_pubkey: [1u8; 32],
            nonce: [3u8; 12],
            key_id: None,
        });
        assert_ne!(
            encode_for_signing(&nonce_tampered).unwrap(),
            baseline,
            "a mutated nonce must change the signed bytes"
        );

        // Attack 2: strip the authority chain — previously a silent suppression
        // of emergency broadcasts for the whole downstream subtree.
        let mut chain_stripped = env.clone();
        chain_stripped.auth_cert_chain = None;
        assert_ne!(
            encode_for_signing(&chain_stripped).unwrap(),
            baseline,
            "removing the authority chain must change the signed bytes"
        );

        // Control: hop_count stays outside the signature so relays can still
        // increment it.
        let mut hopped = env.clone();
        hopped.hop_count = env.hop_count.saturating_add(1);
        assert_eq!(
            encode_for_signing(&hopped).unwrap(),
            baseline,
            "hop_count must remain relay-mutable"
        );
    }

    #[test]
    fn decode_rejects_unsupported_version() {
        let valid = encode(&p0_sos_envelope()).unwrap();
        let tampered = tamper_field_value(&valid, 1, 0x02, valid.clone());
        let err = decode(&tampered).unwrap_err();
        assert!(matches!(err, EnvelopeError::UnsupportedVersion(2)));
    }

    #[test]
    fn decode_rejects_invalid_priority() {
        let mut env = p0_sos_envelope();
        env.priority = MessagePriority::P7;
        let valid = encode(&env).unwrap();
        let tampered = tamper_field_value(&valid, 5, 0x08, valid.clone());
        let err = decode(&tampered).unwrap_err();
        assert!(matches!(err, EnvelopeError::InvalidPriority(_)));
    }

    #[test]
    fn decode_rejects_invalid_content_type() {
        let mut env = p0_sos_envelope();
        env.payload_type = ContentType::KeyRotation;
        let valid = encode(&env).unwrap();
        let tampered = tamper_field_value(&valid, 10, 0xFF, valid.clone());
        let err = decode(&tampered).unwrap_err();
        assert!(matches!(
            err,
            EnvelopeError::InvalidContentType(_) | EnvelopeError::InvalidField(_)
        ));
    }

    #[test]
    fn encode_rejects_payload_size_mismatch() {
        let mut env = p0_sos_envelope();
        env.payload_size += 1; // corrupt
        let err = encode(&env).unwrap_err();
        assert!(matches!(err, EnvelopeError::PayloadSizeMismatch(_, _)));
    }

    #[test]
    fn extension_keys_are_ignored_on_decode() {
        let env = p0_sos_envelope();
        let valid = encode(&env).unwrap();

        // Append fictional field 19 to a valid envelope; decode must succeed
        // and produce the same envelope (ADR-0011 extension tolerance).
        let mut value: Value = ciborium::de::from_reader(&valid[..]).unwrap();
        if let Value::Map(m) = &mut value {
            m.push((Value::Integer(Integer::from(19)), Value::Bool(true)));
        }
        let mut out = Vec::new();
        ciborium::ser::into_writer(&value, &mut out).unwrap();
        let decoded = decode(&out).unwrap();
        assert_eq!(decoded, env);
    }

    /// Re-encode a decoded map with one integer field replaced.
    fn tamper_field_value(bytes: &[u8], field_key: u8, new_value: u8, _orig: Vec<u8>) -> Vec<u8> {
        let value: Value = ciborium::de::from_reader(bytes).unwrap();
        if let Value::Map(m) = &value {
            let mut m = m.clone();
            let want = Value::Integer(Integer::from(u64::from(field_key)));
            for (k, v) in m.iter_mut() {
                if *k == want {
                    *v = Value::Integer(Integer::from(new_value));
                }
            }
            let mut out = Vec::new();
            ciborium::ser::into_writer(&Value::Map(m), &mut out).unwrap();
            out
        } else {
            unreachable!()
        }
    }

    #[test]
    fn payload_hash_mismatch_fails_verification() {
        let mut env = p0_sos_envelope();
        env.payload_hash = [0x00; 32];
        assert!(verify_signing_bytes(&env).is_err());
    }

    #[test]
    fn aead_scope_excludes_8_12_13_15_16_plus_and_includes_11() {
        // CRYPTO-001 AAD contract: fields 1–7, 9–11, 14. Field 11
        // (payload_size) IS included (it is part of the immutable identity in
        // the AAD), while 8 (hop_count), 12 (payload_hash), 13 (payload),
        // 15 (signature) and 16+ extensions are excluded.
        let mut env = p0_sos_envelope();
        env.max_hops = Some(5);
        env.payload_ref = Some(MessageId::new_v7());
        let aead = encode_for_aead(&env).unwrap();
        let signing = encode_for_signing(&env).unwrap();
        assert_ne!(aead, signing, "AAD must be distinct from signing scope");

        let aead_map: Value = ciborium::de::from_reader(&aead[..]).unwrap();
        let smap = expect_map(&aead_map).unwrap();
        for key in [8u64, 12, 13, 15, 16, 17, 18] {
            assert!(
                get(smap, u8::try_from(key).unwrap()).is_none(),
                "field {key} must be excluded from AAD"
            );
        }
        // Fields 1–7, 9–11, 14 present.
        for key in [1u64, 2, 3, 4, 5, 6, 7, 9, 10, 11, 14] {
            assert!(
                get(smap, u8::try_from(key).unwrap()).is_some(),
                "field {key} must be present in AAD"
            );
        }
    }

    #[test]
    fn aead_scope_tracks_payload_size_change() {
        // payload_size is in the AAD (field 11). When encryption changes the
        // wire payload length, the AAD changes accordingly — the engine sets
        // payload_size to the sealed length before deriving the AAD.
        let mut env = p0_sos_envelope();
        let aad_plain = encode_for_aead(&env).unwrap();
        env.payload_size += 16; // seal adds a 16-byte tag
        let aad_sealed = encode_for_aead(&env).unwrap();
        assert_ne!(aad_plain, aad_sealed, "payload_size change must change AAD");

        // But a pure payload-content change (field 13, excluded) does NOT.
        let mut env2 = p0_sos_envelope();
        env2.payload = vec![0x09; 16];
        let aad_ignores_payload = encode_for_aead(&env2).unwrap();
        assert_eq!(
            aad_ignores_payload, aad_plain,
            "payload bytes excluded from AAD"
        );
    }

    #[test]
    fn missing_signature_fails_verification() {
        let mut env = p0_sos_envelope();
        env.signature = None;
        assert!(verify_signing_bytes(&env).is_err());
    }

    #[test]
    fn p0_fits_lora_budget_with_real_payload() {
        // P0 abbreviated form fixed overhead is 171 B (13 map entries with
        // 16-byte sender + 4-byte recipient); max inline payload = 255-171 =
        // 84 bytes — better than the 69 B estimate in MESSAGE_ENVELOPE.md
        // §Size Budget Analysis (which used looser overhead math).
        let fits: [usize; 7] = [0, 16, 30, 58, 66, 69, 84];
        for payload_len in fits {
            let payload = vec![0x42u8; payload_len];
            let mut env = p0_sos_envelope();
            env.payload = payload;
            env.payload_size = env.payload.len() as u64;
            env.payload_hash = Envelope::compute_payload_hash(&env.payload);
            let bytes = encode(&env).unwrap();
            assert!(
                bytes.len() <= crate::protocol::envelope::P0_MAX_ENVELOPE_BYTES,
                "P0 with {payload_len}-byte payload must fit 255, got {}",
                bytes.len()
            );
        }
        // 85-byte payload is the hard limit for the abbreviated form.
        let mut env = p0_sos_envelope();
        env.payload = vec![0x42u8; 85];
        env.payload_size = env.payload.len() as u64;
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);
        assert!(encode(&env).unwrap().len() > crate::protocol::envelope::P0_MAX_ENVELOPE_BYTES);
    }

    // PM-12: oversized P0 must be rejected by decode, not just by the encode test.
    #[test]
    fn p0_oversized_envelope_rejected_on_decode() {
        // Build a P0 envelope whose encoded form exceeds 255 bytes.
        let mut env = p0_sos_envelope();
        env.payload = vec![0x42u8; 85]; // encode gives > 255 B
        env.payload_size = env.payload.len() as u64;
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);
        let bytes = encode(&env).unwrap();
        assert!(bytes.len() > P0_MAX_ENVELOPE_BYTES, "test precondition");
        let err = decode(&bytes).unwrap_err();
        assert!(
            matches!(err, EnvelopeError::InvalidField(_)),
            "expected InvalidField for oversized P0, got {err:?}"
        );
    }

    // PM-14: signing scope must cover field 16 and field 18, but not field 17.
    #[test]
    fn signing_scope_covers_fields_16_and_18_not_17() {
        let env = p0_sos_envelope();

        // Field 16 (encryption_hdr): changing it must change the signing bytes.
        let mut with_hdr = env.clone();
        with_hdr.encryption_hdr = Some(EncryptionHdr {
            ephemeral_pubkey: [1u8; 32],
            nonce: [2u8; 12],
            key_id: None,
        });
        assert_ne!(
            encode_for_signing(&env).unwrap(),
            encode_for_signing(&with_hdr).unwrap(),
            "field 16 (encryption_hdr) must be in the signing scope"
        );

        // Field 18 (auth_cert_chain): changing it must change the signing bytes.
        let mut with_chain = env.clone();
        with_chain.auth_cert_chain = Some(vec![vec![3u8; 32]]);
        assert_ne!(
            encode_for_signing(&env).unwrap(),
            encode_for_signing(&with_chain).unwrap(),
            "field 18 (auth_cert_chain) must be in the signing scope"
        );

        // Field 17 (routing_hints): changing it must NOT change the signing bytes.
        let mut with_hints = env.clone();
        with_hints.routing_hints = Some(RoutingHints {
            last_known_region: Some("EU".into()),
            ..Default::default()
        });
        assert_eq!(
            encode_for_signing(&env).unwrap(),
            encode_for_signing(&with_hints).unwrap(),
            "field 17 (routing_hints) must NOT be in the signing scope"
        );
    }
}
