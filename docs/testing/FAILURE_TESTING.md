# IRIS Failure Injection Testing

## Overview

IRIS is a distributed system that must function under real-world failure conditions: nodes dropping off, transports failing, storage filling up, clocks drifting. Failure injection testing (chaos engineering) verifies that IRIS degrades gracefully rather than catastrophically.

## Compile-Time Feature Flag

Fault injection is gated behind the `fault_injection` Cargo feature. This ensures zero overhead in production builds and no accidental fault injection in release.

```toml
# crates/iris-core/Cargo.toml
[features]
fault_injection = []
```

All fault injection code uses:
```rust
#[cfg(feature = "fault_injection")]
```

Run fault injection tests:
```bash
cargo test --features fault_injection -- fault_
```

## Fault Injection Points

### 1. Transport Failure

```rust
// src/transport_manager/fault_injection.rs
#[cfg(feature = "fault_injection")]
use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(feature = "fault_injection")]
static TRANSPORT_FAILURE_RATE: AtomicU32 = AtomicU32::new(0); // 0-100%

#[cfg(feature = "fault_injection")]
pub fn set_transport_failure_rate(percent: u32) {
    TRANSPORT_FAILURE_RATE.store(percent.min(100), Ordering::Relaxed);
}

// In TransportAdapter::send():
#[cfg(feature = "fault_injection")]
fn maybe_inject_transport_failure(&self) -> Result<(), TransportError> {
    let rate = TRANSPORT_FAILURE_RATE.load(Ordering::Relaxed);
    if rate > 0 {
        let roll: u32 = rand::random::<u32>() % 100;
        if roll < rate {
            return Err(TransportError::Unavailable("fault_injection: transport failure".into()));
        }
    }
    Ok(())
}
```

### 2. Storage Failure

```rust
#[cfg(feature = "fault_injection")]
static STORAGE_FAILURE_RATE: AtomicU32 = AtomicU32::new(0);

// In StorageEngine::persist_message():
#[cfg(feature = "fault_injection")]
fn maybe_inject_storage_failure() -> Result<(), StorageError> {
    // ... same pattern
}
```

### 3. Message Drop

```rust
#[cfg(feature = "fault_injection")]
static MESSAGE_DROP_RATE: AtomicU32 = AtomicU32::new(0);

// In MessageEngine::relay_message():
// Drop the message silently (simulate bit-flip, buffer corruption, etc.)
```

### 4. Packet Corruption

```rust
#[cfg(feature = "fault_injection")]
fn maybe_corrupt_packet(data: &mut Vec<u8>) {
    let rate = CORRUPTION_RATE.load(Ordering::Relaxed);
    if rate > 0 && (rand::random::<u32>() % 100) < rate {
        let idx = rand::random::<usize>() % data.len();
        data[idx] ^= 0xFF; // flip all bits in one byte
    }
}
```

### 5. Clock Skew

```rust
#[cfg(feature = "fault_injection")]
static CLOCK_SKEW_MS: AtomicI64 = AtomicI64::new(0);

// Replace all now() calls with:
pub fn now_with_skew() -> u64 {
    let base = unix_now_ms() as i64;
    #[cfg(feature = "fault_injection")]
    let skew = CLOCK_SKEW_MS.load(Ordering::Relaxed);
    #[cfg(not(feature = "fault_injection"))]
    let skew = 0i64;
    (base + skew).max(0) as u64
}
```

### 6. Node Crash Simulation

```rust
#[cfg(feature = "fault_injection")]
pub async fn simulate_crash_and_restart(core: &mut IrisCore) -> Result<(), IrisError> {
    // Stop all transports (simulate crash — in-memory state lost)
    core.stop().await?;
    // Restart (state reloads from SQLite — durable state survives)
    core.start().await?;
    Ok(())
}
```

## Test Scenarios

### Scenario 1: Node Disappears Mid-Relay

```rust
#[tokio::test]
#[cfg(feature = "fault_injection")]
async fn message_survives_relay_node_crash() {
    let network = SimulatedNetwork::new();
    let alice = create_test_node(&network).await;
    let relay = create_test_node(&network).await;
    let bob = create_test_node(&network).await;

    network.connect(alice.node_id(), relay.node_id()).await;
    network.connect(relay.node_id(), bob.node_id()).await;

    // Alice sends to Bob via relay
    let msg_id = alice.send_message(bob.node_id(), b"hello".to_vec(), Priority::P2).await.unwrap();

    // Wait until relay has the message
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(relay.has_message(&msg_id).await);

    // Simulate relay crash
    relay.simulate_crash().await;

    // Bob should still eventually get message via relay's stored copy (if relay restarts)
    // Or message should be stored at Alice and retried
    // Either way: message is NOT lost
    relay.restart().await;
    network.connect(relay.node_id(), bob.node_id()).await;

    let received = tokio::time::timeout(
        Duration::from_secs(30),
        bob.wait_for_message(msg_id)
    ).await.expect("message should eventually deliver after relay restarts");

    assert_eq!(received.payload, b"hello");
}
```

### Scenario 2: Gateway Failure During Forwarding

```rust
#[tokio::test]
#[cfg(feature = "fault_injection")]
async fn sos_delivered_when_gateway_fails() {
    let network = SimulatedNetwork::new();
    let alice = create_test_node(&network).await;
    let gateway = create_test_node_with_internet(&network).await;
    let local_responder = create_test_node(&network).await;

    network.connect(alice.node_id(), gateway.node_id()).await;
    network.connect(alice.node_id(), local_responder.node_id()).await;

    // Kill gateway
    gateway.simulate_crash().await;
    network.disconnect(alice.node_id(), gateway.node_id()).await;

    // Alice sends SOS
    let sos_id = alice.send_sos().await.unwrap();

    // Local responder should receive SOS via direct BLE
    let received = tokio::time::timeout(
        Duration::from_secs(10),
        local_responder.wait_for_sos()
    ).await.expect("SOS should reach local responder even without gateway");

    assert_eq!(received.origin, alice.node_id());
}
```

### Scenario 3: Storage Full During Message Receipt

```rust
#[tokio::test]
#[cfg(feature = "fault_injection")]
async fn storage_full_triggers_priority_eviction() {
    let config = IrisCoreConfig {
        max_storage_bytes: 1024 * 1024, // 1MB — tiny for testing
        ..Default::default()
    };
    let core = IrisCore::new(config).await.unwrap();

    // Fill storage with low-priority messages
    for i in 0..100 {
        let _ = fill_message_p7(&core, 10_000).await; // 10KB each = 1MB total
    }

    // Now receive a high-priority message
    let p1_msg = test_message_p1();
    let result = core.receive_incoming(p1_msg.clone()).await;

    // Must succeed — should have evicted P7 messages
    assert!(result.is_ok(), "P1 message must be accepted even when storage full");
    assert!(core.has_message(&p1_msg.id).await);

    // Verify some P7 messages were evicted
    let storage_usage = core.storage_usage_bytes().await;
    assert!(storage_usage <= 1024 * 1024);
}
```

### Scenario 4: Battery Critical Mid-SOS

```rust
#[tokio::test]
#[cfg(feature = "fault_injection")]
async fn sos_completes_at_critical_battery() {
    let core = create_test_node(&SimulatedNetwork::new()).await;

    // Set battery to 2%
    core.set_simulated_battery_level(2).await;

    // This should trigger LOW_BATTERY mode, but SOS must still work
    let sos_id = core.send_sos().await;
    assert!(sos_id.is_ok(), "SOS must be accepted at 2% battery");

    // Verify SOS is queued with P0 priority
    let queue_status = core.get_queue_status().await;
    assert!(queue_status.p0_count > 0, "SOS must be queued as P0");
}
```

### Scenario 5: Duplicate Message Delivery

```rust
#[tokio::test]
async fn duplicate_message_delivered_exactly_once() {
    let core = create_test_node(&SimulatedNetwork::new()).await;
    let mut received_count = 0u32;

    let msg = test_message_p4();

    // Inject the same message multiple times (simulates duplicate relay)
    for _ in 0..5 {
        let _ = core.receive_incoming(msg.clone()).await;
    }

    // Should only be delivered to app layer once
    let deliveries = core.get_delivered_messages().await;
    let matches: Vec<_> = deliveries.iter()
        .filter(|m| m.id == msg.id)
        .collect();
    assert_eq!(matches.len(), 1, "Message must be delivered exactly once");
}
```

## Recovery Tests

### After Crash: Reload and Resume

```rust
#[tokio::test]
#[cfg(feature = "fault_injection")]
async fn recovery_resumes_pending_messages() {
    let storage_path = tempfile::tempdir().unwrap();
    let config = IrisCoreConfig {
        storage_path: storage_path.path().to_str().unwrap().to_string(),
        ..Default::default()
    };

    // Session 1: queue messages
    let core = IrisCore::new(config.clone()).await.unwrap();
    let msg_id = core.send_message(
        test_recipient_id(),
        b"surviving message".to_vec(),
        Priority::P2
    ).await.unwrap();

    // Crash (drop core without graceful shutdown)
    drop(core);

    // Session 2: restart
    let core2 = IrisCore::new(config).await.unwrap();
    let queue = core2.get_queue_status().await;

    // Message must be in queue after recovery
    assert!(queue.contains_message(&msg_id), "P2 message must survive restart");
}
```

### Convergence After Network Partition

```rust
#[tokio::test]
async fn messages_delivered_after_partition_heals() {
    let network = SimulatedNetwork::new();
    let alice = create_test_node(&network).await;
    let bob = create_test_node(&network).await;

    // Initial connection
    network.connect(alice.node_id(), bob.node_id()).await;

    // Send a message, then partition
    let msg_id = alice.send_message(
        bob.node_id(), b"delayed message".to_vec(), Priority::P4
    ).await.unwrap();
    network.disconnect(alice.node_id(), bob.node_id()).await;

    // Message is stuck (stored at Alice, can't reach Bob)
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!bob.has_message(&msg_id).await, "Bob should not have message during partition");

    // Heal partition
    network.connect(alice.node_id(), bob.node_id()).await;

    // Message should eventually deliver
    let received = tokio::time::timeout(
        Duration::from_secs(30),
        bob.wait_for_message(msg_id)
    ).await.expect("Message must deliver after partition heals");

    assert_eq!(received.payload, b"delayed message");
}
```
