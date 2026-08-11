# Security Testing Strategy

## Overview

Security testing for IRIS must cover cryptographic correctness, protocol robustness, adversarial
node behavior, and physical transport properties. This document defines the complete security
testing methodology — from automated fuzzing to manual penetration testing to third-party audit.

## 1. Fuzzing

### Philosophy

Fuzzing is the highest-leverage security testing activity for a network protocol implementation.
Parsers, deserializers, and cryptographic input handlers are the most common source of
memory corruption, crash, and logic vulnerabilities. All of these are fuzz targets in IRIS.

### Fuzz Targets

Located in `crates/iris-fuzz/src/`:

```
bin/
  fuzz_cbor_parser.rs         - CBOR deserialization of message envelopes
  fuzz_message_envelope.rs    - Full message envelope parsing and validation
  fuzz_routing_input.rs       - Routing decision engine input
  fuzz_ble_advertisement.rs   - BLE advertisement packet parsing
  fuzz_lora_frame.rs          - LoRa frame parsing
  fuzz_auth_verify.rs         - Ed25519 signature verification input handling
  fuzz_cert_chain.rs          - Authority certificate chain parsing
  fuzz_replay_cache.rs        - Replay cache input (message IDs, timestamps)
```

### Fuzzing Infrastructure

**cargo-fuzz** (libFuzzer backend): used for all Rust targets.

```bash
# Install
cargo install cargo-fuzz

# Run individual target
cargo fuzz run fuzz_cbor_parser -- -max_total_time=3600

# Run all targets in sequence
scripts/run_all_fuzz.sh --duration=3600

# Minimize corpus
cargo fuzz cmin fuzz_cbor_parser
```

**Corpus management:**
```
fuzz/corpus/fuzz_cbor_parser/
  - seed_valid_message.bin    # A valid message
  - seed_empty.bin            # Empty input
  - seed_max_size.bin         # Maximum allowed size
  - seed_all_zeros.bin        # All-zero bytes
  - seed_all_ff.bin           # All 0xFF bytes
  - seed_malformed_cbor.bin   # Known-bad CBOR examples
  [discovered corpus entries added automatically by fuzzer]
```

**OSS-Fuzz integration**: IRIS targets submission to OSS-Fuzz for continuous 24/7 fuzzing
on Google's infrastructure. Integration files in `oss-fuzz/`. Target: active by IRIS v0.3.

**ClusterFuzz**: for internal testing on larger compute budgets before releases.

### Coverage Requirements

Before each release, fuzz target coverage must be reported:
```
cargo fuzz coverage fuzz_cbor_parser
```
Required: >85% line coverage of the parsing code path for each target.

## 2. Penetration Testing

### Adversarial Node Simulation

A test harness in `crates/iris-test-harness/src/adversarial/` implements malicious node
behaviors for integration testing:

```rust
/// Malicious relay that silently drops a configurable fraction of messages
pub struct BlackholeRelay { drop_fraction: f64 }

/// Relay that replays old messages
pub struct ReplayRelay { stored_messages: VecDeque<RawMessage> }

/// Relay that modifies message priority before forwarding
pub struct PriorityEscalationRelay { target_priority: u8 }

/// Relay that sends messages with invalid signatures
pub struct SignatureForgerRelay {}

/// Relay that floods the network with duplicate messages
pub struct FloodRelay { flood_rate: u32 }

/// Sybil node cluster: creates N fake identities all controlled by one adversary
pub struct SybilCluster { identity_count: usize }
```

**Test scenarios:**
```bash
# Run blackhole detection test
cargo test --test adversarial blackhole_detected_via_ack_timeout

# Run signature forgery test
cargo test --test adversarial forged_signature_dropped_at_relay

# Run priority escalation test
cargo test --test adversarial priority_escalation_fails_signature

# Run Sybil resistance test (100 fake identities)
cargo test --test adversarial sybil_100_identities_limited_by_rate

# Run flood resistance test
cargo test --test adversarial flood_10k_msgs_p4_does_not_block_p0
```

### Network Topology Tests

- **2-node**: basic relay, direct link
- **5-node chain**: store-carry-forward across 5 hops
- **10-node mesh**: epidemic routing
- **50-node simulation**: partition and reconnect
- **100-node Sybil**: 50 real + 50 adversarial nodes

## 3. Cryptographic Known-Answer Tests

### Ed25519 (RFC 8032 Test Vectors)

```rust
#[test]
fn ed25519_rfc8032_test_vectors() {
    // Test vector 1 from RFC 8032
    let private_key = hex!("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae3d55");
    let public_key  = hex!("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    let message     = hex!("");
    let expected_sig = hex!("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b");
    
    let sig = Ed25519::sign(&private_key, &message);
    assert_eq!(sig, expected_sig);
    assert!(Ed25519::verify(&public_key, &message, &sig));
}
// Additional vectors: all 5 from RFC 8032 tested
```

### X25519 (RFC 7748 Test Vectors)

```rust
#[test]
fn x25519_rfc7748_test_vectors() {
    // Test vector from RFC 7748 Section 6.1
    let alice_private = hex!("77076d0a7318a57d3c16c17251b26645c6c2f6ca74d43dde0beab693d2f3e4e");
    let bob_public    = hex!("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
    let expected_shared = hex!("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");
    
    let shared = X25519::diffie_hellman(&alice_private, &bob_public);
    assert_eq!(shared, expected_shared);
}
```

### ChaCha20-Poly1305 (RFC 8439 Test Vectors)

```rust
#[test]
fn chacha20poly1305_rfc8439_test_vector() {
    let key   = hex!("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
    let nonce = hex!("070000004041424344454647");
    let plaintext = b"Ladies and Gentlemen of the class of '99...";
    let aad   = hex!("50515253c0c1c2c3c4c5c6c7");
    let expected_ct = hex!("d31a8d34648e60db7b86afbc53ef7ec2...");  // truncated for doc
    
    let ct = ChaCha20Poly1305::encrypt(&key, &nonce, plaintext, &aad);
    assert_eq!(ct, expected_ct);
}
```

## 4. Timing Attack Resistance

### Constant-Time Comparison

All cryptographic equality comparisons use constant-time operations (Rust `subtle` crate):

```rust
use subtle::ConstantTimeEq;

// CORRECT — timing-safe
fn verify_mac(computed: &[u8], expected: &[u8]) -> bool {
    computed.ct_eq(expected).into()
}

// WRONG — timing-unsafe (do not use)
fn verify_mac_bad(computed: &[u8], expected: &[u8]) -> bool {
    computed == expected  // early exit on first mismatch — leaks info
}
```

**CI check**: `scripts/check_timing_safety.sh` runs `cargo audit` and searches for direct
`==` comparisons on types that should use `ct_eq`. CI fails if any are found.

### Timing Test Methodology

For signature verification: measure time for valid vs. invalid signatures across 10,000 trials.
Acceptable: <5% coefficient of variation between distributions. Tested in CI with a dedicated
timing test that runs on a fixed-frequency isolated core.

## 5. Memory Safety

Rust eliminates the largest category of security vulnerabilities:
- **Buffer overflows**: bounds checking at compile time + runtime
- **Use-after-free**: borrow checker prevents
- **Double-free**: ownership system prevents
- **Null pointer dereference**: `Option<T>` forces explicit handling

**FFI boundaries**: unsafe code at FFI boundaries (C crypto libraries called from Rust)
requires manual review. All `unsafe` blocks are documented with:
```rust
// SAFETY: [explain why this is safe despite being unsafe Rust]
```

**FFI audit policy**: every `unsafe` block must be approved in code review by a second
engineer. CI runs `cargo geiger` to count unsafe blocks; increases require explicit justification.

## 6. Third-Party Audit

**Pre-production audit plan:**

Phase 1 (Protocol design review): engage security firm to review:
- Cryptographic protocol design (key exchange, signing, encryption)
- Authentication architecture
- Routing security properties

Phase 2 (Code audit): focused review of:
- `crates/iris-protocol/` (all cryptographic code)
- `crates/iris-core/` (message processing, routing)
- All `unsafe` blocks
- FFI boundaries

Phase 3 (Penetration test): adversarial testing against:
- Running IRIS mesh deployment (physical hardware)
- Android and iOS applications
- Edge node software

Target: complete audit before first production deployment (post v1.0).

## 7. Bug Bounty and Responsible Disclosure

**Disclosure policy:**
- Report to: security@iris.app (PGP key published on website)
- Fix window: 90 days for coordinated disclosure
- Researchers credited in security advisories unless anonymity requested

**Severity and reward tiers:**
| Severity | Description | Reward |
|----------|-------------|--------|
| Critical | Remote code execution, crypto break | ₹5,00,000+ |
| High | Privacy breach, auth bypass, fake emergency | ₹2,00,000 |
| Medium | DoS, significant metadata leak | ₹50,000 |
| Low | Minor information disclosure | ₹10,000 |

## 8. Security Regression Testing

Every discovered vulnerability becomes a permanent regression test:
```
tests/security_regressions/
  CVE-IRIS-2024-001_replay_without_persistence.rs
  CVE-IRIS-2024-002_cbor_panic_on_null_map_key.rs
  [...]
```

Regression tests run in CI. A regression test must fail on the unfixed code to be accepted.
