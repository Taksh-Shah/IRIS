//! AC-4 (DEC-TEST-0004) tokio-test / paused-time suites.
//!
//! These suites verify **async behavior** on the real engine and
//! `TransportManager` under deterministic (paused) virtual time — the pieces
//! Kani cannot model (Kani proves pure functions only; loom models the leaf
//! structures in `loom_models.rs`). No tool models the full manager;
//! `tokio::time::*` lets these tests run fast and deterministically on every
//! host (no wall-clock sleeps, no flaky timing).
//!
//! The helper [`advance_driver`] runs virtual time forward continuously so
//! transport latency sleeps and engine poll timers fire without ANY wall-clock
//! waiting; assertions are made at the *virtual* time horizon they describe.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use iris_core::message::MessagePriority;
use iris_core::message_engine::crypto::DevCryptoProvider;
use iris_core::message_engine::storage::MemoryStorage;
use iris_core::message_engine::{InboundOutcome, MessageEngine, MessageEngineConfig};
use iris_core::protocol::{Envelope, MessageId};
use iris_core::transport::manager::TransportSelectionRequest;
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::{Transport, TransportId, TransportManager};

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

fn dev_engine(node_id: [u8; 32], manager: Arc<TransportManager>) -> Arc<MessageEngine> {
    MessageEngine::new(
        MessageEngineConfig {
            node_id,
            gc_interval_secs: 3600,
            poll_interval: Duration::from_millis(5),
            ..Default::default()
        },
        Arc::new(MemoryStorage::new()),
        Arc::new(DevCryptoProvider::new()),
        manager,
    )
}

/// Background virtual-clock driver: steps virtual time forward by 5 ms per
/// iteration so transport sleep tasks, engine poll loops and ack timers all
/// fire deterministically. Real time per iteration is microseconds; the loop
/// is aborted at test end. MUST be `.abort()`ed before the test body returns
/// so no paused task leaks across tests.
fn advance_driver() -> (tokio::task::JoinHandle<()>, Arc<AtomicBool>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop2 = stop.clone();
    let handle = tokio::spawn(async move {
        while !stop2.load(Ordering::Relaxed) {
            tokio::time::advance(Duration::from_millis(5)).await;
            tokio::task::yield_now().await;
        }
    });
    (handle, stop)
}

fn stop_driver(handle: tokio::task::JoinHandle<()>, stop: Arc<AtomicBool>) {
    stop.store(true, Ordering::Relaxed);
    handle.abort();
}

#[tokio::test(start_paused = true)]
async fn message_engine_round_trip_delivers_under_paused_time() {
    let alice_id = [1u8; 32];
    let bob_id = [2u8; 32];

    let transport = Arc::new(SimulatedTransport::new("sim", "Sim", SimConfig::default()));
    let manager = Arc::new(TransportManager::new());
    manager.register(transport.clone()).await.expect("register");

    let alice = dev_engine(alice_id, manager.clone());
    let bob = dev_engine(bob_id, manager.clone());
    let mut bob_delivered = bob.delivered_messages();

    // Send must complete without any virtual time flowing (no timer awaited).
    let body = b"paused-time lifecycle body";
    let env = sample_envelope(alice_id, bob_id, body);
    let id = env.message_id;
    alice.send_message(env).await.expect("send");

    // From here on, drive virtual time so the SimulatedTransport latency sleep
    // (~10 ms) fires and the frame reaches Bob's inbound broadcast.
    let (driver, stop) = advance_driver();

    let mut stream = transport.incoming_messages();
    let frame = tokio::time::timeout(
        Duration::from_secs(2),
        futures_util::StreamExt::next(&mut stream),
    )
    .await
    .expect("frame must arrive after virtual-time advance")
    .expect("stream item");
    let outcome = bob.process_incoming(frame).await.expect("process");
    assert_eq!(outcome, InboundOutcome::Delivered);

    let delivered = tokio::time::timeout(Duration::from_secs(2), bob_delivered.recv())
        .await
        .expect("delivery event after advance")
        .expect("event");
    assert_eq!(delivered.message_id, id);
    assert_eq!(delivered.payload, body);

    // Clean shutdown must not hang when tasks are fully virtual-time driven.
    alice.shutdown().await;
    bob.shutdown().await;
    stop_driver(driver, stop);
}

#[tokio::test(start_paused = true)]
async fn transport_manager_register_select_deregister_under_paused_time() {
    let manager = Arc::new(TransportManager::new());
    let sim = Arc::new(SimulatedTransport::new("sim", "Sim", SimConfig::default()));

    manager.register(sim.clone()).await.expect("register");
    assert_eq!(manager.list().await.len(), 1);

    let (driver, stop) = advance_driver();

    // Repeated selection across virtual time must never change ranking or
    // panic (RED-0003-01 total_cmp path; no NaN partial_cmp).
    for _ in 0..10 {
        let req = TransportSelectionRequest {
            target_peer: Some(iris_core::message::PeerId([9u8; 32])),
            message_size: 100,
            priority: MessagePriority::P4,
            max_latency_ms: Some(1000),
            prefer_low_cost: false,
            multipath: false,
            fragmentable: false,
        };
        let selection = manager.select_transports(&req).await;
        assert_eq!(selection.len(), 1);
        assert_eq!(selection[0].transport_id.as_str(), "sim");
    }

    // Long virtual horizon exercises the register-spawned state-forwarder;
    // deregister must not hang on any pending timer.
    tokio::time::advance(Duration::from_secs(5)).await;
    manager
        .deregister(&TransportId::from("sim"))
        .await
        .expect("deregister");
    assert!(manager.get(&TransportId::from("sim")).await.is_none());
    assert!(manager.list().await.is_empty());
    stop_driver(driver, stop);
}

// TAK-20: `engine_background_tasks_advance_with_virtual_time_only` was
// DELETED per its finding — it spawned a bare `tokio::time::sleep` and
// asserted tokio's own timer semantics, observing no IRIS code at all while
// its name claimed engine coverage. It duplicated tokio's test suite.

#[tokio::test(start_paused = true)]
async fn engine_send_abort_clean_when_dropped_under_paused_time() {
    let transport = Arc::new(SimulatedTransport::new("sim", "Sim", SimConfig::default()));
    let manager = Arc::new(TransportManager::new());
    manager.register(transport.clone()).await.expect("register");

    let engine = dev_engine([7u8; 32], manager.clone());
    let env = sample_envelope([7u8; 32], [8u8; 32], b"drop me");
    engine.send_message(env).await.expect("send");

    // Drive through several delivery/ack horizons.
    let (driver, stop) = advance_driver();
    tokio::time::advance(Duration::from_secs(120)).await;
    tokio::time::advance(Duration::from_secs(3600)).await;

    // TAK-20: the old version asserted nothing — it would pass identically if
    // drop leaked every background task. Cancellation is now OBSERVED via the
    // runtime's public task metrics: alive-task count must return to its
    // pre-drop baseline once the engine is dropped and the executor drains.
    yield_and_settle().await;
    let before_drop = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();

    drop(engine);
    for _ in 0..50 {
        tokio::task::yield_now().await;
        if tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks()
            <= before_drop
        {
            break;
        }
        // Nudge virtual time so any timer-bound tasks reach their abort points.
        tokio::time::advance(Duration::from_millis(10)).await;
    }
    let after_drop = tokio::runtime::Handle::current()
        .metrics()
        .num_alive_tasks();
    assert!(
        after_drop <= before_drop,
        "engine drop leaked background tasks: before={before_drop} after={after_drop}"
    );

    stop_driver(driver, stop);
}

/// Let the executor run pending tasks until quiescent (bounded spins).
async fn yield_and_settle() {
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn transport_selection_survives_nan_cost_and_ranks_deterministically() {
    let manager = Arc::new(TransportManager::new());
    // Healthy transport: normal cost inputs.
    let healthy = Arc::new(SimulatedTransport::new(
        "sim-healthy",
        "Healthy",
        SimConfig::default(),
    ));
    // Adversarial transport: NaN battery cost poisons its score (P4 request
    // applies the battery penalty). Under the pre-RED-0003-01 comparator this
    // panicked inside sort_by; under total_cmp it must be eliminated
    // deterministically, never reorder the survivor, and never hang.
    let mut poisoned_cfg = SimConfig::default();
    poisoned_cfg.battery_ma_override = Some(f32::NAN);
    let poisoned = Arc::new(SimulatedTransport::new("sim-nan", "NaN", poisoned_cfg));

    manager
        .register(healthy.clone())
        .await
        .expect("register healthy");
    manager
        .register(poisoned.clone())
        .await
        .expect("register nan");

    let req = TransportSelectionRequest {
        target_peer: Some(iris_core::message::PeerId([9u8; 32])),
        message_size: 100,
        priority: MessagePriority::P4,
        max_latency_ms: Some(1000),
        prefer_low_cost: false,
        multipath: false,
        fragmentable: false,
    };

    let first = manager.select_transports(&req).await;
    assert_eq!(
        first.len(),
        1,
        "NaN-scored candidate must be eliminated: {first:?}"
    );
    assert_eq!(first[0].transport_id.as_str(), "sim-healthy");

    for _ in 0..10 {
        let again = manager.select_transports(&req).await;
        assert_eq!(again.len(), 1);
        assert_eq!(
            first[0].transport_id, again[0].transport_id,
            "selection must be deterministic across repeated calls"
        );
    }
}
