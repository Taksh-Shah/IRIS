# IRIS Testing Strategy

## Testing Pyramid

```
                    ┌─────────────────────┐
                    │   Physical Tests    │  ← Slowest, most realistic
                    │  (real devices,     │    Run: weekly / release candidate
                    │   real transports)  │    Count: ~20 scenarios
                    └──────────┬──────────┘
                 ┌─────────────┴──────────────┐
                 │     Simulation Tests        │  ← Slow, large-scale
                 │   (100s of virtual nodes,   │    Run: nightly
                 │    simulated transports)    │    Count: ~50 scenarios
                 └──────────┬─────────────────┘
          ┌──────────────────┴──────────────────┐
          │         Integration Tests            │  ← Moderate speed
          │   (cross-component, in-process       │    Run: every commit
          │    simulated transports, no BLE)     │    Count: ~200 tests
          └──────────────────┬──────────────────┘
     ┌──────────────────────────────────────────────┐
     │                  Unit Tests                  │  ← Fastest, most numerous
     │   (pure Rust, single module, no I/O)         │    Run: every commit
     │                                              │    Count: ~1000 tests
     └──────────────────────────────────────────────┘
```

## Coverage Targets

| Component | Line Coverage Target | Reasoning |
|-----------|---------------------|-----------|
| `iris-crypto` | 100% | Every crypto operation must be tested against known vectors |
| `iris-proto` (parser) | 100% | Parser handles all external input — must handle all code paths |
| `iris-core` routing | 90% | Core routing logic, property tests supplement coverage |
| `iris-storage` | 85% | SQLite operations, some paths require specific SQLite behavior |
| `iris-transport` traits | 80% | Trait definitions, implementation-specific tests elsewhere |
| Platform adapters (Kotlin/Swift) | 70% | Some paths require real hardware |
| Overall `iris-core` workspace | 80% | Minimum acceptable threshold |

Coverage is measured with `cargo-llvm-cov` on every CI run. PRs that drop coverage more than 2% are flagged for review.

## Unit Tests

### Framework
- **Rust**: Built-in `#[test]` + `tokio::test` for async
- **Property tests**: `proptest` crate for invariant testing
- **Kotlin**: JUnit 5 + MockK
- **Swift**: XCTest with async support

### What to Unit Test (Rust Core)

**Serialization** (`iris-proto`):
- Round-trip: `encode(decode(encode(msg))) == encode(msg)`
- Known test vectors: specific inputs produce specific byte outputs
- Invalid input: malformed CBOR returns `Err`, never panics

**Cryptography** (`iris-crypto`):
- Ed25519 sign + verify with RFC 8032 test vectors
- Ed25519 signature mismatch detection (wrong key, tampered data)
- X25519 key agreement with RFC 7748 test vectors
- ChaCha20-Poly1305 encrypt + decrypt correctness
- ChaCha20-Poly1305 authentication tag failure on tampered ciphertext
- BLAKE3 known hash values
- Key zeroization (verify memory is zeroed after drop — limited by test tooling)

**Message Engine** (`iris-core`):
- TTL countdown: TTL decrements on relay, never increments
- Hop count: hop_count increments on relay, never exceeds max_hops
- Priority ordering: lower priority number dequeued first
- Expiry: expired messages not relayed

**Routing** (`iris-core`):
- PRoPHET: delivery probability increases on contact, decreases with aging
- Spray-and-Wait: copy count decrements correctly, never goes below 0
- Deduplication: seen message not relayed, unseen message accepted

**Storage** (`iris-storage`):
- Insert + retrieve round-trip
- Priority eviction order: P7 evicted before P0
- Expiry eviction: expired messages removed first
- Schema migration: v1→v2 migration produces correct schema

### Test Organization in Rust

```rust
// src/routing_engine/prophet.rs
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn delivery_probability_increases_on_contact() {
        let mut router = ProphetRouter::new(Default::default());
        let peer = NodeId::test_peer_1();

        let p_before = router.delivery_probability(&peer);
        assert_eq!(p_before, 0.0);

        router.on_contact(peer, Instant::now());
        let p_after = router.delivery_probability(&peer);
        assert!(p_after > p_before);
        assert!(p_after <= 1.0);
    }

    #[test]
    fn delivery_probability_ages_toward_zero() {
        let mut router = ProphetRouter::new(Default::default());
        let peer = NodeId::test_peer_1();
        router.on_contact(peer, Instant::now());
        let p_initial = router.delivery_probability(&peer);

        router.age_predictabilities(1000.0); // large time delta
        let p_aged = router.delivery_probability(&peer);
        assert!(p_aged < p_initial);
    }

    proptest! {
        #[test]
        fn probability_always_in_range(n_contacts in 0usize..100) {
            let mut router = ProphetRouter::new(Default::default());
            let peer = NodeId::test_peer_1();
            for _ in 0..n_contacts {
                router.on_contact(peer, Instant::now());
                router.age_predictabilities(0.1);
            }
            let p = router.delivery_probability(&peer);
            prop_assert!(p >= 0.0 && p <= 1.0);
        }
    }
}
```

## Integration Tests

Integration tests exercise multiple components together using in-process simulated transports. No real BLE, Wi-Fi, or network required.

### Test Setup Pattern

```rust
// tests/integration/message_relay.rs
use iris_sim::{SimulatedNetwork, SimulatedTransport, NodeBuilder};
use iris_core::IrisCore;

async fn create_test_node(network: &SimulatedNetwork) -> IrisCore {
    let config = IrisCoreConfig {
        storage_path: ":memory:".to_string(),   // in-memory SQLite
        routing_algorithm: "prophet".to_string(),
        emergency_enabled: true,
        ..Default::default()
    };
    let core = IrisCore::new(config).await.unwrap();
    let transport = SimulatedTransport::new(network.clone());
    core.register_transport(Box::new(transport)).await.unwrap();
    core.start().await.unwrap();
    core
}

#[tokio::test]
async fn message_delivered_single_hop() {
    let network = SimulatedNetwork::new();
    let alice = create_test_node(&network).await;
    let bob = create_test_node(&network).await;
    network.connect(alice.node_id(), bob.node_id()).await;

    let msg_id = alice.send_message(
        bob.node_id(),
        b"Hello Bob".to_vec(),
        Priority::P4Normal,
    ).await.unwrap();

    let delivery = tokio::time::timeout(
        Duration::from_secs(5),
        bob.wait_for_message(msg_id)
    ).await.expect("timeout").expect("error");

    assert_eq!(delivery.payload, b"Hello Bob");
}

#[tokio::test]
async fn sos_delivered_through_relay() {
    let network = SimulatedNetwork::new();
    let alice = create_test_node(&network).await;
    let relay = create_test_node(&network).await;
    let emergency = create_test_node(&network).await;

    // Alice → relay → emergency (alice and emergency not directly connected)
    network.connect(alice.node_id(), relay.node_id()).await;
    network.connect(relay.node_id(), emergency.node_id()).await;

    let sos_id = alice.send_sos().await.unwrap();

    let received = tokio::time::timeout(
        Duration::from_secs(10),
        emergency.wait_for_sos()
    ).await.expect("SOS delivery timeout");

    assert_eq!(received.origin, alice.node_id());
}
```

## Simulation Tests

Simulation tests run hundreds of virtual nodes with simulated mobility patterns. See `docs/experiments/EXPERIMENT_INDEX.md` for planned simulation experiments.

Simulation tests run nightly in CI (too slow for per-commit). Success criteria and expected outcomes are defined per-scenario before running.

## Physical Tests

Physical tests require real devices. Cannot be fully automated. Run on weekly release candidate builds.

Minimum physical test matrix before release:
1. Android↔Android message delivery (2 Pixels)
2. Android↔iOS message delivery (Pixel + iPhone)
3. 3-hop relay (Android→Android→iOS)
4. iOS background receive (iOS in background, Android sends)
5. SOS delivery under simulated poor conditions (nodes moved in/out of range)
6. OEM battery kill test (Redmi + Samsung with standard settings)

## Test Environment Isolation

Each test creates:
- Fresh in-memory SQLite (`:memory:`)
- Fresh Ed25519 keypair (not a fixed test key)
- Fresh `SimulatedNetwork` instance
- No shared state between tests

Tests must not depend on:
- Real time (use `tokio::time::pause()` + `advance()` for time-dependent tests)
- Filesystem (use `:memory:` for SQLite)
- Network (use `SimulatedNetwork`)
- Fixed node IDs (generate fresh per test)

## CI Integration

```yaml
# .github/workflows/ci.yml (excerpt)
test:
  name: Test
  runs-on: ubuntu-22.04
  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Run unit tests
      run: cargo test --workspace --lib
    - name: Run integration tests
      run: cargo test --workspace --test '*'
    - name: Coverage
      run: |
        cargo install cargo-llvm-cov
        cargo llvm-cov --workspace --lcov --output-path coverage.lcov
    - name: Upload coverage
      uses: codecov/codecov-action@v3
      with:
        files: coverage.lcov
        fail_ci_if_error: true
        threshold: 80
```
