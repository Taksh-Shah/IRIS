//! # IRIS Protocol Wire Types + Canonical CBOR Codec
//!
//! Implementation arm of PROTO-001 (WP-A per ORCH-0001). This module makes the
//! envelope specification in `docs/protocol/MESSAGE_ENVELOPE.md` executable.
//!
//! ## Wire format
//!
//! A flat CBOR map (RFC 8949) with integer keys 1–18, encoded with **Core
//! Deterministic Encoding (CDE)** per RFC 8949 §4.2:
//! - map keys sorted ascending (integer keys encode as ascending values),
//! - definite-length byte strings, no indefinite-length items,
//! - minimal integer encoding.
//!
//! ## Signature scope (ADR-0011)
//!
//! `encode_for_signing()` covers fields 1–14 **excluding field 8 (`hop_count`)**
//! and field 15 (`signature`):
//! - relays must be able to modify `hop_count` without the sender's key
//!   (ADR-0011 Open Issue 1 => signature scope by design),
//! - field 15 is the signature itself,
//! - fields 16+ (`encryption_hdr`, `routing_hints`, `auth_cert_chain`) are
//!   unsigned extensions appended after the signature (Vector 4 in
//!   `PROTOCOL_TEST_VECTORS.md`).
//!
//! ## Extension tolerance (ADR-0011)
//!
//! Decoding any unknown map key **> 18 is ignored** (append-only extension
//! mechanism, ADR-0011 §Extension). Nodes never fail on newer fields.

pub mod codec;
pub mod content_type;
pub mod envelope;
pub mod message_id;

pub use codec::{
    decode, encode, encode_for_aead, encode_for_signing, verify_signing_bytes, EnvelopeError,
};
pub use content_type::ContentType;
pub use envelope::{EncryptionHdr, Envelope, RoutingHints};
pub use message_id::MessageId;

/// Current wire protocol version (MESSAGE_ENVELOPE.md field 1).
pub const PROTOCOL_VERSION: u8 = 1;

/// EMERGENCY_BROADCAST recipient constant (4-byte form, P0 abbreviated).
/// Value 1 as defined in `MESSAGE_ENVELOPE.md` and `PROTOCOL_TEST_VECTORS.md`.
pub const EMERGENCY_BROADCAST: [u8; 4] = [0x00, 0x00, 0x00, 0x01];