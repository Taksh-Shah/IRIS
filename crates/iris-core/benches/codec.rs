//! AC-10 (DEC-TEST-0011) codec CPU-path benchmarks.
//!
//! Wire-code + `MessageId` parse + envelope CBOR encode/decode. No regression
//! gate v1 (BLK-0005: battery/physical perf deferred); criterion runs
//! host-side on CI without failing the build.

use criterion::{criterion_group, criterion_main, Criterion};
use iris_core::message::MessagePriority;
use iris_core::protocol::codec;
use iris_core::protocol::content_type::ContentType;
use iris_core::protocol::message_id::MessageId;
use iris_core::protocol::Envelope;
use std::hint::black_box;

fn sample_envelope() -> Envelope {
    let body: Vec<u8> = (0..180u32).map(|i| (i % 251) as u8).collect();
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: vec![1u8; 32],
        recipient_id: vec![2u8; 32],
        priority: MessagePriority::P3,
        ttl_seconds: 3600,
        timestamp: 1_700_000_000,
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Text,
        payload_size: body.len() as u64,
        payload_hash: Envelope::compute_payload_hash(&body),
        payload: body,
        payload_ref: None,
        signature: Some([7u8; 64]),
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

fn bench_codec(c: &mut Criterion) {
    let id = MessageId::from_bytes([0xAA; 16]);
    let env = sample_envelope();
    let wire = codec::encode(&env).expect("encode");

    c.bench_function("codec/message_id_parse_16B", |b| {
        b.iter(|| MessageId::from_slice(black_box(&[0xBB; 16])).is_some())
    });
    c.bench_function("codec/content_type_wire_roundtrip", |b| {
        b.iter(|| {
            for v in 1..=16u8 {
                black_box(ContentType::from_u8(v).unwrap().as_u8());
            }
        })
    });
    c.bench_function("codec/priority_wire_roundtrip", |b| {
        b.iter(|| {
            for v in 0..=7u8 {
                black_box(MessagePriority::from_u8(v).unwrap().as_u8());
            }
        })
    });
    c.bench_function("codec/envelope_encode_222B", |b| {
        b.iter(|| codec::encode(black_box(&env)).expect("encode"))
    });
    c.bench_function("codec/envelope_decode_222B", |b| {
        b.iter(|| codec::decode(black_box(&wire)).expect("decode"))
    });
    black_box(id);
}

criterion_group!(benches, bench_codec);
criterion_main!(benches);
