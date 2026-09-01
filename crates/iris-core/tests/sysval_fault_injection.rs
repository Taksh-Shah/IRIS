//! HV-4 — the injectable-RF-failure sim, exercised end to end.
//!
//! `SimulatedTransport` historically modelled only success (plus loss/latency
//! and a static MTU). Every hardware defect in the prior bug-hunt (HW-1..21,
//! FFI-1..19) was invisible to it by construction. HV-4 adds four failure modes
//! that later Tier-1..3 findings need to reproduce in CI:
//!
//! - `disconnect_after_sends` — an unsolicited mid-stream link death (HV-7 /
//!   HV-27): the transport degrades and the send fails `NotConnected`.
//! - `mtu_shrink_after_sends` / `mtu_after_shrink` — the Android peripheral-role
//!   link that never renegotiates its MTU up (HV-8).
//! - `scan_fail_after_calls` — `onScanFailed` / SCANNING_TOO_FREQUENTLY, the
//!   AOSP 5-per-30 s throttle (HV-11 / HV-14).
//! - `busy_first_n_connects` — `WifiP2pManager` `reason=BUSY` during the
//!   platform P2P bring-up window (HV-23).
//!
//! The GO/GO election tie (also named in HV-4's sketch) is Wi-Fi Direct group
//! formation and belongs with HV-19 — the Wi-Fi Direct sim does not model group
//! election yet, so it is deferred to that finding.

use std::time::Duration;

use iris_core::message::{DiscoveryConfig, MessagePriority, PeerId, PeerInfo, SerializedMessage};
use iris_core::protocol::MessageId;
use iris_core::transport::simulated::{SimConfig, SimulatedTransport};
use iris_core::transport::{Transport, TransportState};
use iris_core::TransportError;

fn msg(n: u8, len: usize) -> SerializedMessage {
    SerializedMessage {
        message_id: MessageId::from([n; 16]),
        priority: MessagePriority::P3,
        payload: vec![b'x'; len],
    }
}

fn peer_info() -> PeerInfo {
    PeerInfo {
        peer_id: PeerId([2u8; 32]),
        addresses: Vec::new(),
        transport_addresses: Vec::new(),
        last_seen: None,
    }
}

/// HV-23: the P2P BUSY window is transient — a caller that retries `connect()`
/// (which is exactly what `DiscoveryManager`'s scan loop does) gets through once
/// the platform state machine is ready, and IRIS is not permanently wedged.
#[tokio::test]
async fn busy_connect_window_admits_on_retry() {
    let t = SimulatedTransport::new(
        "sim-wd",
        "Wi-Fi Direct (sim)",
        SimConfig {
            busy_first_n_connects: 4,
            ..Default::default()
        },
    );
    let peer = peer_info();

    let mut admitted_on = None;
    for attempt in 1..=10 {
        match t.connect(&peer).await {
            Ok(_) => {
                admitted_on = Some(attempt);
                break;
            }
            Err(TransportError::Busy) => continue,
            Err(e) => panic!("unexpected connect error: {e}"),
        }
    }
    assert_eq!(
        admitted_on,
        Some(5),
        "connect must be refused for the first 4 attempts, then succeed"
    );
    assert_eq!(t.state(), TransportState::Connected);
}

/// HV-8: once the link's MTU has shrunk, an oversized send must fail with a
/// *visible, actionable* `MessageTooLarge` (the signal the upstream fragmenter
/// keys on) — never a silent `Ok` that drops the payload.
#[tokio::test]
async fn mtu_shrink_surfaces_too_large_not_silent_loss() {
    let t = SimulatedTransport::new(
        "sim-ble",
        "BLE (sim)",
        SimConfig {
            max_message_size: Some(512),
            mtu_shrink_after_sends: 1,
            mtu_after_shrink: 20,
            latency_base_ms: 0,
            latency_spread_ms: 0,
            ..Default::default()
        },
    );
    let peer = PeerId([9u8; 32]);

    // The reply direction (2nd send onward) is pinned at the tiny MTU.
    t.send(&peer, &msg(0, 200)).await.expect("first send fits");
    match t.send(&peer, &msg(1, 200)).await {
        Err(TransportError::MessageTooLarge { limit, actual }) => {
            assert_eq!((limit, actual), (20, 200));
        }
        other => panic!("expected MessageTooLarge after MTU shrink, got {other:?}"),
    }
}

/// HV-7 / HV-27: a link can die mid-stream with no disconnect callback. The sim
/// now models that as an *observable* `Degraded` state (plus a `NotConnected`
/// send error) rather than a transport that keeps reporting `Available` while
/// every send silently fails.
#[tokio::test]
async fn injected_disconnect_is_observable() {
    let t = SimulatedTransport::new(
        "sim-ble",
        "BLE (sim)",
        SimConfig {
            disconnect_after_sends: 3,
            latency_base_ms: 0,
            latency_spread_ms: 0,
            ..Default::default()
        },
    );
    let peer = PeerId([9u8; 32]);

    for i in 0..3 {
        t.send(&peer, &msg(i, 10)).await.expect("sends before the drop");
    }
    assert_eq!(t.state(), TransportState::Available, "healthy until the drop");

    match t.send(&peer, &msg(3, 10)).await {
        Err(TransportError::NotConnected) => {}
        other => panic!("expected NotConnected on the injected drop, got {other:?}"),
    }
    assert_eq!(
        t.state(),
        TransportState::Degraded,
        "the drop must be visible to transport selection, not hidden"
    );
}

/// HV-14: reconnect churn drives repeated `discover_peers` calls; after the
/// throttle ceiling the platform refuses further scans. The sim reproduces the
/// lockout so HV-11/HV-14's fix can prove it recovers.
#[tokio::test]
async fn scan_throttle_lockout_reproduces() {
    let t = SimulatedTransport::new(
        "sim-ble",
        "BLE (sim)",
        SimConfig {
            scan_fail_after_calls: 5,
            ..Default::default()
        },
    );
    for _ in 0..5 {
        assert!(t.discover_peers(DiscoveryConfig::default()).await.is_ok());
    }
    for _ in 0..3 {
        match t.discover_peers(DiscoveryConfig::default()).await {
            Err(TransportError::Busy) => {}
            _ => panic!("scan past the ceiling must be refused"),
        }
    }
}

/// SIM-001 determinism carries over to the new modes: same config (incl. seed)
/// → identical fault timing, so a bench-derived scenario is reproducible.
#[tokio::test]
async fn injected_faults_are_deterministic() {
    let cfg = SimConfig {
        disconnect_after_sends: 4,
        busy_first_n_connects: 2,
        seed: 99,
        latency_base_ms: 0,
        latency_spread_ms: 0,
        ..Default::default()
    };
    let a = SimulatedTransport::new("a", "A", cfg.clone());
    let b = SimulatedTransport::new("b", "B", cfg);
    let peer = peer_info();

    for _ in 0..2 {
        assert!(matches!(a.connect(&peer).await, Err(TransportError::Busy)));
        assert!(matches!(b.connect(&peer).await, Err(TransportError::Busy)));
    }
    assert!(a.connect(&peer).await.is_ok() && b.connect(&peer).await.is_ok());

    let pid = PeerId([9u8; 32]);
    for i in 0..4u8 {
        let ra = a.send(&pid, &msg(i, 8)).await;
        let rb = b.send(&pid, &msg(i, 8)).await;
        assert_eq!(ra.is_ok(), rb.is_ok(), "send {i} outcome must match");
    }
    assert!(a.send(&pid, &msg(4, 8)).await.is_err());
    assert!(b.send(&pid, &msg(4, 8)).await.is_err());
    assert_eq!(a.state(), TransportState::Degraded);
    assert_eq!(b.state(), TransportState::Degraded);
}
