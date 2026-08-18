//! AC-10 (DEC-TEST-0011) envelope framing benchmarks.
//!
//! The per-send framing paths: `encode_for_signing` (Ed25519 AAD),
//! `encode_for_aead` (ChaCha20-Poly1305 AAD) and the payload hash — the three
//! CPU paths executed for every sealed message regardless of transport.

use criterion::{criterion_group, criterion_main, Criterion};
use iris_core::message::MessagePriority;
use iris_core::protocol::codec::{encode_for_aead, encode_for_signing};
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
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

fn bench_envelope(c: &mut Criterion) {
    let env = sample_envelope();
    c.bench_function("envelope/encode_for_aead_222B", |b| {
        b.iter(|| encode_for_aead(black_box(&env)).expect("aad"))
    });
    c.bench_function("envelope/encode_for_signing_222B", |b| {
        b.iter(|| encode_for_signing(black_box(&env)).expect("sig-aad"))
    });
    c.bench_function("envelope/payload_hash_180B", |b| {
        b.iter(|| Envelope::compute_payload_hash(black_box(&env.payload)))
    });
}

criterion_group!(benches, bench_envelope);
criterion_main!(benches);
