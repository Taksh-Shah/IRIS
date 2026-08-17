//! M3 integration: MessageEngine (MSG-001) over the PostgreSQL store (STORE-001)
//! across a SimulatedTransport (TRANSPORT-001).

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use futures_util::StreamExt;
use iris_core::message::MessagePriority;
use iris_core::message_engine::crypto::DevCryptoProvider;
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{InboundOutcome, MessageEngine, MessageEngineConfig};
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::{Transport, TransportManager};

const ALICE: [u8; 32] = [0xAA; 32];
const BOB: [u8; 32] = [0xBB; 32];

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn engine_roundtrip_over_postgres_store() {
    if !pg_available() {
        eprintln!("SKIP: IRIS_PG_PASSWORD unset");
        return;
    }
    let _g = LOCK.lock().await;

    let store = fresh_store().await;
    let transport = Arc::new(SimulatedTransport::new(
        "sim-alice",
        "Sim A",
        SimConfig {
            packet_loss_rate: 0.0,
            ..Default::default()
        },
    ));
    let manager = Arc::new(TransportManager::new());
    manager.register(transport.clone()).await.unwrap();

    let alice = MessageEngine::new(
        MessageEngineConfig {
            node_id: ALICE,
            gc_interval_secs: 3600,
            ..Default::default()
        },
        Arc::new(store),
        Arc::new(DevCryptoProvider::new()),
        manager,
    );

    let bob_manager = Arc::new(TransportManager::new());
    let bob = MessageEngine::new(
        MessageEngineConfig {
            node_id: BOB,
            gc_interval_secs: 3600,
            ..Default::default()
        },
        Arc::new(MemoryStorage::new()),
        Arc::new(DevCryptoProvider::new()),
        bob_manager,
    );
    let mut bob_delivered = bob.delivered_messages();

    let env = env_for(
        BOB,
        MessagePriority::P4,
        unix_now(),
        3600,
        b"hello pg engine",
    );
    let id = env.message_id;
    alice.send_message(env).await.expect("send");

    // Drive the one frame that crossed the transport into Bob's engine.
    let mut stream = transport.incoming_messages();
    let msg = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .expect("transport must deliver")
        .expect("stream item");
    let outcome = bob.process_incoming(msg).await.expect("process");
    assert_eq!(outcome, InboundOutcome::Delivered);

    let delivered = tokio::time::timeout(Duration::from_millis(500), bob_delivered.recv())
        .await
        .expect("delivered event")
        .expect("event");
    assert_eq!(delivered.message_id, id);
    assert_eq!(delivered.payload, b"hello pg engine");
    assert_eq!(alice.metrics().sent, 1);
}
