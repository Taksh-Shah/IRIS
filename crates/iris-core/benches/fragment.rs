//! AC-10 (DEC-TEST-0011) fragmentation benchmarks.
//!
//! The RED-0004 fragmentation path: `split_payload` over a >MTU (70 KB) sealed
//! payload into ≤64 KiB chunks, fragment-header encoding, and two-fragment
//! reassembly through `FragmentAssembler::feed`.

use criterion::{criterion_group, criterion_main, Criterion};
use iris_core::message::MessagePriority;
use iris_core::message_engine::fragment::{encode_fragment, split_payload, FragmentAssembler};
use iris_core::protocol::content_type::ContentType;
use iris_core::protocol::message_id::MessageId;
use iris_core::protocol::Envelope;
use std::hint::black_box;

fn bench_fragment(c: &mut Criterion) {
    // >MTU P5 payload (70 KB) → two ≤64 KiB wire chunks.
    let payload: Vec<u8> = (0..70_000u32).map(|i| (i % 251) as u8).collect();
    let original_id = MessageId::from_bytes([0x11; 16]);
    let sig = Some([0x55; 64]);

    let chunks = split_payload(&payload, 64 * 1024);
    assert_eq!(chunks.len(), 2);

    let f0 = encode_fragment(0, 2, original_id, ContentType::Fragment, sig, &chunks[0]);
    let f1 = encode_fragment(1, 2, original_id, ContentType::Fragment, sig, &chunks[1]);

    let frag0 = fragment_envelope(&f0, original_id);
    let frag1 = fragment_envelope(&f1, original_id);

    c.bench_function("fragment/split_70kb_64k_chunks", |b| {
        b.iter(|| split_payload(black_box(&payload), black_box(64 * 1024)))
    });
    c.bench_function("fragment/encode_header", |b| {
        b.iter(|| {
            encode_fragment(
                black_box(0),
                black_box(2),
                black_box(original_id),
                black_box(ContentType::Fragment),
                black_box(sig),
                black_box(&chunks[0]),
            )
        })
    });
    c.bench_function("fragment/feed_2_fragments_reassemble", |b| {
        b.iter(|| {
            let mut assembler = FragmentAssembler::new(std::time::Duration::from_secs(120));
            black_box(assembler.feed(black_box(frag0.clone())));
            black_box(assembler.feed(black_box(frag1.clone())));
        })
    });
}

fn fragment_envelope(fragment_payload: &[u8], original_id: MessageId) -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: original_id,
        sender_id: vec![1u8; 32],
        recipient_id: vec![2u8; 32],
        priority: MessagePriority::P5,
        ttl_seconds: 3600,
        timestamp: 1_700_000_000,
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Fragment,
        payload_size: fragment_payload.len() as u64,
        payload_hash: Envelope::compute_payload_hash(fragment_payload),
        payload: fragment_payload.to_vec(),
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

criterion_group!(benches, bench_fragment);
criterion_main!(benches);
