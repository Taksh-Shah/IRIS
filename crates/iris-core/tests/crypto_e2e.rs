//! M7 — CRYPTO-001 engine integration (AC-8): a private message sent from
//! Alice's engine is **encrypted (ChaCha20-Poly1305), signed (Ed25519)** and
//! carried as the sealed envelope over a SimulatedTransport; Bob's engine
//! verifies (RFC 8032 `verify_strict`) and decrypts to the original plaintext.
//!
//! This is the first test where both engines use the real
//! [`IrisCryptoProvider`] + a [`KeyDirectory`], exercising the full
//! send-seal → wire → receive-verify-decrypt path added by CRYPTO-001.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use iris_core::crypto::key_directory::MemoryKeyDirectory;
use iris_core::message::MessagePriority;
use iris_core::message_engine::crypto::{IrisCryptoProvider, NodeIdentity};
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{InboundOutcome, MessageEngine, MessageEngineConfig};
use iris_core::protocol::{Envelope, MessageId};
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::{Transport, TransportManager};

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sample_envelope(sender: [u8; 32], recipient: [u8; 32], body: &[u8]) -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: sender.to_vec(),
        recipient_id: recipient.to_vec(),
        priority: MessagePriority::P3,
        ttl_seconds: 3600,
        timestamp: unix_now(),
        hop_count: 0,
        max_hops: None,
        payload_type: iris_core::protocol::ContentType::Text,
        payload_size: body.len() as u64,
        payload_hash: Envelope::compute_payload_hash(body),
        payload: body.to_vec(),
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

/// Build an engine with the real crypto provider (authenticates, encrypts).
/// `transport` is SHARED across the peers so the simulated transport's
/// outbound frames are visible to the test (M3 pattern). Returns the engine.
async fn engine(
    node_id: [u8; 32],
    identity: Arc<NodeIdentity>,
    transport: Arc<SimulatedTransport>,
) -> Arc<MessageEngine> {
    let manager = Arc::new(TransportManager::new());
    let engine = MessageEngine::new(
        MessageEngineConfig {
            node_id,
            gc_interval_secs: 3600,
            ..Default::default()
        },
        Arc::new(MemoryStorage::new()),
        Arc::new(IrisCryptoProvider::new(identity)),
        manager.clone(),
    );
    manager.register(transport).await.expect("register transport");
    engine
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_encrypted_signed_message_round_trips_e2e() {
    // Both participants generate real Ed25519 identity + X25519 static keys.
    let alice_ident = Arc::new(NodeIdentity::generate());
    let bob_ident = Arc::new(NodeIdentity::generate());
    let alice_id = alice_ident.identity.verifying_bytes();
    let bob_id = bob_ident.identity.verifying_bytes();
    assert_ne!(alice_id, bob_id);

    // Alice knows Bob's X25519 key (key directory / contact provisioning).
    let mut dir = MemoryKeyDirectory::new();
    dir.insert(bob_id, bob_ident.static_x25519.public_bytes());

    let transport = Arc::new(SimulatedTransport::new(
        "sim",
        "Sim",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let alice = engine(alice_id, alice_ident, transport.clone()).await;
    alice.set_key_directory(Arc::new(dir));
    let bob = engine(bob_id, bob_ident, transport.clone()).await;
    let mut bob_delivered = bob.delivered_messages();

    let body = b"secret: the grid eval is at 20:00";
    let env = sample_envelope(alice_id, bob_id, body);
    let id = env.message_id;
    alice.send_message(env).await.expect("send");

    // Alice's engine emits ONE sealed (signed + encrypted) wire frame.
    let mut stream = transport.incoming_messages();
    let msg = tokio::time::timeout(Duration::from_millis(1000), stream.next())
        .await
        .expect("transport must deliver")
        .expect("stream item");

    // Bob: verify_strict + decrypt (the CRYPTO-001 deliver path).
    let outcome = bob.process_incoming(msg).await.expect("process");
    assert_eq!(outcome, InboundOutcome::Delivered);

    let delivered = tokio::time::timeout(Duration::from_millis(1000), bob_delivered.recv())
        .await
        .expect("delivered event")
        .expect("event");
    assert_eq!(delivered.message_id, id);
    assert_eq!(delivered.payload, body, "recipient must receive the plaintext, not ciphertext");
    assert_eq!(alice.metrics().sent, 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_tampered_wire_frame_never_surfaces_plaintext() {
    let alice_ident = Arc::new(NodeIdentity::generate());
    let bob_ident = Arc::new(NodeIdentity::generate());
    let alice_id = alice_ident.identity.verifying_bytes();
    let bob_id = bob_ident.identity.verifying_bytes();

    let mut dir = MemoryKeyDirectory::new();
    dir.insert(bob_id, bob_ident.static_x25519.public_bytes());

    // Alice knows Bob's X25519 key (key directory / contact provisioning).
    let mut dir = MemoryKeyDirectory::new();
    dir.insert(bob_id, bob_ident.static_x25519.public_bytes());

    let transport = Arc::new(SimulatedTransport::new(
        "sim",
        "Sim",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let alice = engine(alice_id, alice_ident, transport.clone()).await;
    alice.set_key_directory(Arc::new(dir));
    let bob = engine(bob_id, bob_ident, transport.clone()).await;

    let env = sample_envelope(alice_id, bob_id, b"integrity check");
    alice.send_message(env).await.expect("send");

    let mut stream = transport.incoming_messages();
    let mut msg = tokio::time::timeout(Duration::from_millis(1000), stream.next())
        .await
        .expect("transport must deliver")
        .expect("stream item");

    // Flip one payload byte on the wire (ciphertext tamper) → engine must
    // reject with an error; no plaintext ever reaches the delivered stream.
    msg.payload[0] ^= 0x01;

    let outcome = bob.process_incoming(msg).await;
    assert!(
        outcome.is_err(),
        "tampered sealed frame must be rejected (AEAD tag + verify_strict)"
    );
}

// RED-0004 engine-level E2E proof: a >MTU P5 unicast (encrypted + signed) is
// fragmented on the wire, reassembled by the recipient, whole-ADU signature
// verified, and decrypted to the original plaintext. Before the fix, dedup
// swallowed fragment 1 and the AAD fields were not restored — multi-fragment
// ADUs never delivered. All three fixes (distinct wire ids, AAD-field restore,
// whole-ADU re-verify) are exercised through the REAL engine delivery path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn m7_fragmented_encrypted_message_reassembles_and_verifies() {
    let alice_ident = Arc::new(NodeIdentity::generate());
    let bob_ident = Arc::new(NodeIdentity::generate());
    let alice_id = alice_ident.identity.verifying_bytes();
    let bob_id = bob_ident.identity.verifying_bytes();

    let mut dir = MemoryKeyDirectory::new();
    dir.insert(bob_id, bob_ident.static_x25519.public_bytes());

    let transport = Arc::new(SimulatedTransport::new(
        "sim",
        "Sim",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let alice = engine(alice_id, alice_ident, transport.clone()).await;
    alice.set_key_directory(Arc::new(dir));
    let bob = engine(bob_id, bob_ident, transport.clone()).await;
    let mut bob_delivered = bob.delivered_messages();

    // P5 unicast larger than the simulated transport MTU (64 KiB) →
    // two wire fragments carrying the sealed plaintext chunks.
    let body: Vec<u8> = (0..70_000u32).map(|i| (i % 251) as u8).collect();
    let mut env = sample_envelope(alice_id, bob_id, &body);
    env.priority = MessagePriority::P5;
    env.payload_size = body.len() as u64;
    let id = env.message_id;
    alice.send_message(env).await.expect("send");

    // The delivery loop emits two fragment frames on the shared transport.
    let mut stream = transport.incoming_messages();
    let f0 = tokio::time::timeout(Duration::from_millis(2000), stream.next())
        .await
        .expect("fragment 0 on the wire")
        .expect("stream item");
    let f1 = tokio::time::timeout(Duration::from_millis(2000), stream.next())
        .await
        .expect("fragment 1 on the wire")
        .expect("stream item");

    // Fragment payload is header + chunk of ciphertext, NOT the plaintext.
    let dec0 = iris_core::protocol::codec::decode(&f0.payload).expect("decode frag 0");
    assert_eq!(dec0.payload_type, iris_core::protocol::ContentType::Fragment);
    assert!(
        !dec0.payload.windows(body.len()).any(|w| w == &body[..]),
        "wire fragment must not carry plaintext"
    );

    // Bob processes both fragments in order: first buffers, second reassembles
    // → whole-ADU signature verified → decrypted and delivered.
    let first = bob.process_incoming(f0).await.expect("process frag 0");
    assert_eq!(first, InboundOutcome::FragmentBuffered);
    let second = bob.process_incoming(f1).await.expect("process frag 1");
    assert_eq!(second, InboundOutcome::Delivered);

    let delivered = tokio::time::timeout(Duration::from_millis(2000), bob_delivered.recv())
        .await
        .expect("delivered event")
        .expect("event");
    assert_eq!(delivered.message_id, id, "original id restored after reassembly");
    assert_eq!(delivered.payload, body, "plaintext recovered from fragments");
    assert_eq!(alice.metrics().sent, 1, "one logical message sent");
}