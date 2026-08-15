//! DESKTOP-001 AC2 engine-level round trip over a shared loopback wire.
//!
//! Two `DesktopEngine` hosts share one `SimulatedTransport`. Alice sends to
//! Bob; the frame crosses the wire, Bob's auto-spawned inbox forwarder feeds
//! `process_incoming`, and the envelope reaches `subscribe_delivered` (the same
//! stream `subscribe_inbox` forwards to the UI Channel). This exercises the
//! production receive path without a window.

use std::sync::Arc;
use std::time::Duration;

use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::Transport;
use iris_desktop::engine_handle::DesktopEngine;

const ALICE: [u8; 32] = [0xAA; 32];
const BOB: [u8; 32] = [0xBB; 32];

fn hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn wire(id: &str) -> Arc<SimulatedTransport> {
    Arc::new(SimulatedTransport::new(
        id,
        "Wire",
        SimConfig {
            packet_loss_rate: 0.0,
            latency_base_ms: 0,
            latency_spread_ms: 0,
            ..Default::default()
        },
    ))
}

#[tokio::test]
async fn loopback_wire_delivers_from_alice_to_bob() {
    let shared = wire("wire");
    let alice = DesktopEngine::with_transport(ALICE, shared.clone())
        .await
        .expect("alice engine");
    let bob = DesktopEngine::with_transport(BOB, shared.clone())
        .await
        .expect("bob engine");

    let mut delivered = bob.subscribe_delivered();

    let view = alice
        .send_text(&hex(BOB), "hello loopback", 4)
        .await
        .expect("send should enqueue");
    assert_eq!(view.id_hex.len(), 16, "truncated id view (privacy P2)");

    // Alice's delivery loop ticks on a 50 ms poll; give the wire time to carry.
    let env = tokio::time::timeout(Duration::from_secs(5), delivered.recv())
        .await
        .expect("message must be delivered")
        .expect("delivered event");
    assert_eq!(env.payload, b"hello loopback");
    assert_eq!(env.recipient_id, BOB.to_vec());
    assert_eq!(env.message_id.short().to_string(), view.id_hex);
    assert_eq!(alice.mesh_status().await.metrics.sent, 1);
}

#[tokio::test]
async fn multiple_messages_arrive_in_order() {
    let shared = wire("wire");
    let alice = DesktopEngine::with_transport(ALICE, shared.clone())
        .await
        .expect("alice");
    let bob = DesktopEngine::with_transport(BOB, shared.clone())
        .await
        .expect("bob");
    let mut delivered = bob.subscribe_delivered();

    for i in 0..3 {
        alice
            .send_text(&hex(BOB), &format!("m{i}"), 4)
            .await
            .expect("enqueue");
    }
    for i in 0..3 {
        let env = tokio::time::timeout(Duration::from_secs(5), delivered.recv())
            .await
            .expect("delivered")
            .expect("event");
        assert_eq!(env.payload, format!("m{i}").as_bytes());
    }
}

#[tokio::test]
async fn graceful_degradation_when_transport_down() {
    let shared = wire("wire");
    let alice = DesktopEngine::with_transport(ALICE, shared.clone())
        .await
        .expect("alice");
    assert!(alice.mesh_status().await.online, "wire up => online");

    shared.shutdown().await.expect("shutdown");

    let status = alice.mesh_status().await;
    assert!(!status.online, "wire down => offline (AC5)");
    assert!(
        status.transports.iter().any(|t| t.state == "Unavailable"),
        "transport reports Unavailable"
    );

    // Sends still enqueue (store-carry-forward, never dropped).
    let view = alice
        .send_text(&hex(BOB), "queued while offline", 4)
        .await
        .expect("send must enqueue, not fail");
    assert_eq!(view.id_hex.len(), 16);
}
