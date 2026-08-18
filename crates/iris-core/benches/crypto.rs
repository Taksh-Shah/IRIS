//! AC-10 (DEC-TEST-0011) crypto CPU-path benchmarks.
//!
//! The real `IrisCryptoProvider` production path — Ed25519 sign/verify and
//! ChaCha20-Poly1305 (X25519 KDF) encrypt/decrypt — the cost of every sealed
//! wire frame. Async trait methods are driven through a current-thread tokio
//! runtime inside each iteration (no wall-clock waits: all CPU-bound).

use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use iris_core::message::MessagePriority;
use iris_core::message_engine::crypto::message_kdf_info;
use iris_core::message_engine::crypto::{CryptoProvider, IrisCryptoProvider, NodeIdentity};
use iris_core::protocol::codec::encode_for_aead;
use iris_core::protocol::content_type::ContentType;
use iris_core::protocol::message_id::MessageId;
use iris_core::protocol::Envelope;
use std::hint::black_box;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
}

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

fn bench_crypto(c: &mut Criterion) {
    let rt = runtime();
    let alice = Arc::new(NodeIdentity::generate());
    let bob = Arc::new(NodeIdentity::generate());
    let alice_provider = IrisCryptoProvider::new(alice.clone());
    let bob_provider = IrisCryptoProvider::new(bob.clone());
    let bob_x = bob.static_x25519.public_bytes();

    // Pre-signed envelope + pre-sealed 4 KB payload (the message engine's
    // send path: sign then encrypt; AAD = encode_for_aead at encryption time).
    let mut env = sample_envelope();
    rt.block_on(alice_provider.sign(&mut env)).expect("sign");
    let aad = encode_for_aead(&env).expect("aad");
    let info = message_kdf_info(env.message_id);
    let plaintext: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
    let (ciphertext, hdr) = rt
        .block_on(alice_provider.encrypt(&info, &aad, &plaintext, &bob_x))
        .expect("encrypt");

    c.bench_function("crypto/ed25519_sign", |b| {
        b.iter(|| {
            let mut e = sample_envelope();
            rt.block_on(alice_provider.sign(black_box(&mut e)))
                .expect("sign");
        })
    });
    c.bench_function("crypto/ed25519_verify", |b| {
        b.iter(|| {
            rt.block_on(alice_provider.verify(black_box(&env)))
                .expect("verify")
        })
    });
    c.bench_function("crypto/chacha20_encrypt_4kb", |b| {
        b.iter(|| {
            rt.block_on(alice_provider.encrypt(
                black_box(&info),
                black_box(&aad),
                black_box(&plaintext),
                black_box(&bob_x),
            ))
            .expect("encrypt");
        })
    });
    c.bench_function("crypto/chacha20_decrypt_4kb", |b| {
        b.iter(|| {
            rt.block_on(bob_provider.decrypt(
                black_box(&info),
                black_box(&aad),
                black_box(&ciphertext),
                black_box(&hdr),
            ))
            .expect("decrypt");
        })
    });
}

criterion_group!(benches, bench_crypto);
criterion_main!(benches);
