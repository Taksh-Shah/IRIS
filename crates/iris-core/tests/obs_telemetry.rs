//! OBS-001 integration tests (WP-9 acceptance): telemetry counters flow through
//! real seams (message_engine, routing, SCF), a shared registry accumulates
//! across engines, and privacy helpers truncate identifiers.

use std::sync::Arc;

use iris_core::discovery::neighbor_table::NeighborTable;
use iris_core::message::{LinkQuality, MessagePriority, PeerId, PeerInfo};
use iris_core::message_engine::crypto::DevCryptoProvider;
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{MessageEngine, MessageEngineConfig};
use iris_core::observability::metric;
use iris_core::observability::MetricsRegistry;
use iris_core::protocol::{ContentType, Envelope, MessageId};
use iris_core::routing::scf::{ScfConfig, ScfEngine};
use iris_core::routing::{ForwardingDecision, RoutingEngine};
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::TransportId;

const ALICE: [u8; 32] = [0xAA; 32];
const BOB: [u8; 32] = [0xBB; 32];
const CAROL: [u8; 32] = [0xCC; 32];

fn pid(id: u8) -> PeerId {
    let mut b = [0u8; 32];
    b[0] = id;
    PeerId::from_bytes(b)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn envelope_for(recipient: [u8; 32], priority: MessagePriority, payload: &[u8]) -> Envelope {
    Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: ALICE.to_vec(),
        recipient_id: recipient.to_vec(),
        priority,
        ttl_seconds: 3600,
        timestamp: now_unix(),
        hop_count: 0,
        max_hops: None,
        payload_type: ContentType::Text,
        payload_size: payload.len() as u64,
        payload_hash: Envelope::compute_payload_hash(payload),
        payload: payload.to_vec(),
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    }
}

async fn alice_engine() -> Arc<MessageEngine> {
    let transport = Arc::new(SimulatedTransport::new(
        "sim-alice",
        "Sim A",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let manager = Arc::new(iris_core::transport::TransportManager::new());
    manager.register(transport.clone()).await.unwrap();
    MessageEngine::new(
        MessageEngineConfig {
            node_id: ALICE,
            gc_interval_secs: 3600,
            ..Default::default()
        },
        Arc::new(MemoryStorage::new()),
        Arc::new(DevCryptoProvider::new()),
        manager,
    )
}

// --- AC3: shared registry accumulates across routing + SCF engines ---

#[tokio::test(flavor = "multi_thread")]
async fn shared_registry_accumulates_across_routing_and_scf() {
    let reg = MetricsRegistry::new();

    // Routing: a direct delivery decision (neighbor present) and a Store.
    let table = NeighborTable::new(std::time::Duration::from_secs(60));
    let mut engine = RoutingEngine::new().with_telemetry(reg.clone());
    let table2 = NeighborTable::new(std::time::Duration::from_secs(60));
    // Populate a neighbor so Algorithm 1 fires (Direct).
    table2
        .upsert(
            &PeerInfo {
                peer_id: pid(7),
                addresses: vec![],
                transport_addresses: vec![],
                last_seen: None,
            },
            &TransportId::from("sim"),
            LinkQuality::Excellent,
        )
        .await;
    let direct = engine
        .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(7), 0, MessagePriority::P4, vec![], &table2)
        .await;
    assert!(matches!(direct, ForwardingDecision::Forward { .. }));
    let _ = table;

    // SCF: buffer then evict under pressure -> evictions counter.
    let mut scf = ScfEngine::new(
        MemoryStorage::new(),
        ScfConfig {
            max_storage_bytes: 0,
            ..Default::default()
        },
    )
    .with_telemetry(reg.clone());
    // A P1 (rank 1) message; P0 is exempt, P1 evictable.
    let env = envelope_for(CAROL, MessagePriority::P1, b"evict me");
    let _ = scf.buffer_message(env, None);
    let evicted = scf.evict_to_fit(1_000_000);
    assert!(!evicted.is_empty(), "P1 must be evicted under pressure");

    let snap = reg.snapshot();
    assert_eq!(
        snap[metric::ROUTING_DECISIONS_TOTAL],
        1,
        "one routing decision"
    );
    assert_eq!(snap[metric::SCF_EVICTIONS_TOTAL], 1, "one SCF eviction");
}

// --- AC3 (message_engine): telemetry counters increment through the engine ---

#[tokio::test(flavor = "multi_thread")]
async fn engine_telemetry_counts_expired_and_delivered() {
    let engine = alice_engine().await;
    let reg = MetricsRegistry::new();

    // Force an expired inbound: TTL long since elapsed -> expired counter.
    let mut expired_env = envelope_for(BOB, MessagePriority::P4, b"too late");
    expired_env.timestamp = 1; // 1970; expires instantly
    let _ = engine
        .process_incoming(iris_core::message::IncomingMessage {
            peer_id: pid(9),
            transport_id: "sim".to_string(),
            payload: iris_core::protocol::codec::encode(&expired_env).unwrap(),
            received_at: std::time::Instant::now(),
        })
        .await;

    // A live inbound addressed to Alice -> delivered counter.
    let live = envelope_for(ALICE, MessagePriority::P4, b"for alice");
    let _ = engine
        .process_incoming(iris_core::message::IncomingMessage {
            peer_id: pid(9),
            transport_id: "sim".to_string(),
            payload: iris_core::protocol::codec::encode(&live).unwrap(),
            received_at: std::time::Instant::now(),
        })
        .await;

    let snap = engine.telemetry().snapshot();
    assert_eq!(
        snap[metric::MESSAGES_EXPIRED_TOTAL],
        1,
        "expired inbound counted"
    );
    assert_eq!(
        snap[metric::MESSAGES_DELIVERED_TOTAL],
        1,
        "delivered inbound counted"
    );
    let _ = reg;
}

// --- AC3 (OBS-RT-06): injected registry accumulates engine + routing ---

#[tokio::test(flavor = "multi_thread")]
async fn engine_accepts_injected_shared_registry() {
    let reg = MetricsRegistry::new();

    let transport = Arc::new(SimulatedTransport::new(
        "sim-alice",
        "Sim A",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let manager = Arc::new(iris_core::transport::TransportManager::new());
    manager.register(transport.clone()).await.unwrap();
    let engine = MessageEngine::new_with_telemetry(
        MessageEngineConfig {
            node_id: ALICE,
            gc_interval_secs: 3600,
            ..Default::default()
        },
        Arc::new(MemoryStorage::new()),
        Arc::new(DevCryptoProvider::new()),
        manager,
        reg.clone(),
    );

    // Expired inbound through the shared engine registry.
    let mut expired_env = envelope_for(BOB, MessagePriority::P4, b"too late");
    expired_env.timestamp = 1;
    let _ = engine
        .process_incoming(iris_core::message::IncomingMessage {
            peer_id: pid(9),
            transport_id: "sim".to_string(),
            payload: iris_core::protocol::codec::encode(&expired_env).unwrap(),
            received_at: std::time::Instant::now(),
        })
        .await;

    // A routing decision into the same registry.
    let mut routing = RoutingEngine::new().with_telemetry(reg.clone());
    let table = NeighborTable::new(std::time::Duration::from_secs(60));
    table
        .upsert(
            &PeerInfo {
                peer_id: pid(7),
                addresses: vec![],
                transport_addresses: vec![],
                last_seen: None,
            },
            &TransportId::from("sim"),
            LinkQuality::Excellent,
        )
        .await;
    let _ = routing
        .decide(MessageId::new_v7(), 0, 0, u64::MAX, pid(1), pid(7), 0, MessagePriority::P4, vec![], &table)
        .await;

    let snap = reg.snapshot();
    assert_eq!(
        snap[metric::MESSAGES_EXPIRED_TOTAL],
        1,
        "engine counter in shared registry"
    );
    assert_eq!(
        snap[metric::ROUTING_DECISIONS_TOTAL],
        1,
        "routing counter in shared registry"
    );
}

// --- AC4: privacy — short() truncates identifiers to 8 bytes (16 hex) ---

#[test]
fn privacy_short_truncates_identifiers() {
    let id = MessageId([0xAB; 16]);
    let short = id.short();
    let short_str = short.to_string();
    assert_eq!(short_str.len(), 16, "8-byte prefix in hex = 16 chars");
    assert!(
        id.to_string().starts_with(&short_str),
        "short is a prefix of full id"
    );
    assert_ne!(
        short_str,
        id.to_string(),
        "short must not equal the full 32-char id"
    );

    let peer = PeerId([0xCD; 32]);
    let short_p = peer.short().to_string();
    assert_eq!(short_p.len(), 16);
    assert!(peer.to_string().starts_with(&short_p));
}
