//! Golden-vector regression harness (TEST-001 AC-8 / DEC-TEST-0008).
//!
//! Reads the committed byte-exact corpus in `tests/golden_vectors/` and verifies
//! BOTH directions: encoding the structural envelope yields the exact corpus
//! bytes, and decoding the corpus bytes yields the structural envelope. Any
//! codec change that alters wire bytes (version, field order, map arity, key
//! encoding, id/hash/timestamp arithmetic) fails here before any interop test.
//!
//! The corpus is the IRIS-flat-format authority. V001/V002 deliberately pin the
//! P0 SOS + signing-scope vectors whose PROTOCOL_TEST_VECTORS.md prose contained
//! an arithmetic text error (0x66B5C000 vs the correct 0x66B7FF00 for the 2024
//! timestamp) — this harness re-catches that DISC-0009-class bug permanently
//! (AC-8: "at least one regression test re-catches a DISC-0009-class error").

use std::fs;
use std::path::PathBuf;

use iris_core::message::MessagePriority;
use iris_core::protocol::codec::{decode, encode, encode_for_signing};
use iris_core::protocol::{ContentType, EncryptionHdr, Envelope, MessageId, RoutingHints};

/// Corpus directory relative to the crate root (`CARGO_MANIFEST_DIR`).
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden_vectors")
}

fn unhex(s: &str) -> Vec<u8> {
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..compact.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&compact[i..i + 2], 16).expect("corpus hex must be valid"))
        .collect()
}

/// P0 SOS structural envelope matching the V001/V002 corpus fields.
fn v1_p0_sos() -> Envelope {
    let payload: Vec<u8> = (0..66u8).collect();
    Envelope {
        version: 1,
        message_id: MessageId::from_bytes([
            0x01, 0x8f, 0x1a, 0x2b, 0x3c, 0x4d, 0x5e, 0x6f, 0x70, 0x81, 0x92, 0xa3, 0xb4, 0xc5,
            0xd6, 0xe7,
        ]),
        sender_id: vec![
            0xa3, 0xbc, 0x7e, 0x2f, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa,
            0xbb, 0xcc,
        ],
        recipient_id: vec![0x00, 0x00, 0x00, 0x01],
        priority: MessagePriority::P0,
        ttl_seconds: 259_200,
        timestamp: 1_723_334_400,
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Sos,
        payload_size: 66,
        payload_hash: [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
            0xdd, 0xee, 0xff, 0x01,
        ],
        payload,
        payload_ref: None,
        signature: Some([0xAA; 64]),
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn v001_p0_sos_cde_matches_corpus_byte_exact() {
    let corpus =
        fs::read_to_string(corpus_dir().join("V001_p0_sos.md")).expect("V001 corpus present");
    let expected = unhex(&fold_hex_block(&corpus));
    assert_eq!(encode(&v1_p0_sos()).unwrap(), expected);
}

fn fold_hex_block(md: &str) -> String {
    md.lines()
        .skip_while(|l| !l.contains("hex: >-"))
        .skip(1)
        .take_while(|l| !l.trim_start().starts_with("fields:"))
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect()
}

#[test]
fn v002_p0_sos_signing_scope_matches_corpus() {
    let corpus = fs::read_to_string(corpus_dir().join("V002_p0_sos_signing.md"))
        .expect("V002 corpus present");
    let expected = unhex(&fold_hex_block(&corpus));
    assert_eq!(encode_for_signing(&v1_p0_sos()).unwrap(), expected);
}

#[test]
fn v003_full_envelope_matches_corpus() {
    let corpus = fs::read_to_string(corpus_dir().join("V003_full_envelope.md"))
        .expect("V003 corpus present");
    let expected = unhex(&fold_hex_block(&corpus));
    let payload = b"hello iris".to_vec();
    let env = Envelope {
        version: 1,
        message_id: MessageId::from_bytes([
            0x01, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0,
            0xe0, 0xf0,
        ]),
        sender_id: vec![7u8; 32],
        recipient_id: vec![8u8; 32],
        priority: MessagePriority::P4,
        ttl_seconds: 28_800,
        timestamp: 1_723_334_400,
        hop_count: 2,
        max_hops: Some(20),
        payload_type: ContentType::Text,
        payload_size: payload.len() as u64,
        payload_hash: Envelope::compute_payload_hash(&payload),
        payload,
        payload_ref: None,
        signature: Some([0xBB; 64]),
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
    };
    assert_eq!(encode(&env).unwrap(), expected);
}

#[test]
fn corpus_decodes_both_ways_for_all_vectors() {
    for entry in fs::read_dir(corpus_dir()).expect("corpus directory readable") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let md = fs::read_to_string(&path).unwrap();
        let kind = md
            .lines()
            .find_map(|l| l.trim().strip_prefix("kind: "))
            .unwrap_or("envelope_cde");
        if kind != "envelope_cde" {
            continue;
        }
        let wire = unhex(&fold_hex_block(&md));
        // Decode the corpus bytes and re-encode: byte-exact round-trip.
        let decoded = decode(&wire).expect("corpus vector must decode");
        let reencoded = encode(&decoded).expect("must re-encode");
        assert_eq!(
            reencoded,
            wire,
            "byte-exact round-trip failed for {:?}",
            path.file_name().unwrap()
        );
    }
}

#[test]
fn p0_sos_fits_lora_budget_from_corpus() {
    // The V001 golden byte-length must stay within the 255-byte LoRa P0 budget;
    // a regression that grows the wire format fails here before field budget
    // checks in codec tests.
    let corpus = fs::read_to_string(corpus_dir().join("V001_p0_sos.md")).unwrap();
    let wire = unhex(&fold_hex_block(&corpus));
    assert!(
        wire.len() <= iris_core::protocol::envelope::P0_MAX_ENVELOPE_BYTES,
        "P0 envelope grew past LoRa budget: {} bytes",
        wire.len()
    );
    assert_eq!(wire.len(), 237, "V001 length pinned in corpus");
}

/// Explicit DISC-0009-class regression: the timestamp field hex must encode
/// 0x66B7FF00 (1723334400 == 0x66B7FF00), NOT the arithmetically wrong
/// 0x66B5C000 that appeared in PROTOCOL_TEST_VECTORS.md prose. This is the
/// byte-level guard: assert the exact substring at field-7 in the wire bytes.
#[test]
fn timestamp_arithmetic_reveget_66b7ff00_not_doc_error_66b5c000() {
    let wire = encode(&v1_p0_sos()).unwrap();
    let wire_hex = hex(&wire);
    assert!(
        wire_hex.contains("071a66b7ff00"),
        "field 7 timestamp must encode 0x66B7FF00 (correct arithmetic), got substring not found in {wire_hex}"
    );
    assert!(
        !wire_hex.contains("071a66b5c000"),
        "DISC-0009 class: found the documented-wrong timestamp bytes 0x66B5C000"
    );
    // And the value-level check independent of hex representation:
    assert_eq!(v1_p0_sos().timestamp, 1_723_334_400);
}
