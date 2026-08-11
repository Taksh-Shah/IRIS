# IRIS Unit Testing

## Framework

Rust unit tests use the built-in `#[test]` attribute. Async tests use `#[tokio::test]`. Property-based tests use the `proptest` crate.

```toml
[dev-dependencies]
proptest = "1"
tokio-test = "0.4"
tempfile = "3"
hex-literal = "0.4"
criterion = { version = "0.5", features = ["async_tokio"] }
```

## Test Organization

Unit tests live adjacent to the source they test, in a `mod tests` block:

```rust
// src/crypto/signing.rs

pub fn sign_message(key: &SigningKey, message: &[u8]) -> Signature { ... }
pub fn verify_signature(key: &VerifyingKey, message: &[u8], sig: &Signature) -> bool { ... }

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn test_keypair() -> (SigningKey, VerifyingKey) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        (signing_key, verifying_key)
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let (sk, vk) = test_keypair();
        let message = b"Hello, IRIS!";
        let sig = sign_message(&sk, message);
        assert!(verify_signature(&vk, message, &sig));
    }

    #[test]
    fn tampered_message_fails_verification() {
        let (sk, vk) = test_keypair();
        let message = b"Hello, IRIS!";
        let sig = sign_message(&sk, message);
        let tampered = b"Hello, EVIL!";
        assert!(!verify_signature(&vk, tampered, &sig));
    }

    #[test]
    fn wrong_key_fails_verification() {
        let (sk1, _) = test_keypair();
        let (_, vk2) = test_keypair();
        let message = b"Hello, IRIS!";
        let sig = sign_message(&sk1, message);
        assert!(!verify_signature(&vk2, message, &sig));
    }
}
```

## Crypto: Known-Answer Tests (KAT)

Known-answer tests verify that the cryptographic implementation produces the exact expected output for known inputs. These catch implementation bugs that property tests might miss.

### Ed25519 KAT (RFC 8032 Test Vectors)

```rust
#[test]
fn ed25519_rfc8032_test_vector_1() {
    // From RFC 8032 Section 6, Test Vector 1
    let sk_bytes = hex_literal::hex!(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae3d55"
    );
    let msg = b"";
    let expected_sig = hex_literal::hex!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );

    let sk = SigningKey::from_bytes(&sk_bytes);
    let sig = sign_message(&sk, msg);
    assert_eq!(sig.to_bytes(), expected_sig);
}

#[test]
fn ed25519_rfc8032_test_vector_3() {
    // From RFC 8032 Section 6, Test Vector 3 (non-empty message)
    let sk_bytes = hex_literal::hex!(
        "c5aa8df43f9f837bedb7442f31dcb7b166d38535076f094b85ce3a2e0b4458f7"
    );
    let msg = hex_literal::hex!("af82");
    let expected_sig = hex_literal::hex!(
        "6291d657deec24024827e69c3abe01a30ce548a284743a445e3680d7db5ac3ac18ff9b538d16f290ae67f760984dc6594a7c15e9716ed28dc027beceea1ec40a"
    );

    let sk = SigningKey::from_bytes(&sk_bytes);
    let sig = sign_message(&sk, &msg);
    assert_eq!(sig.to_bytes(), expected_sig);
}
```

### X25519 KAT (RFC 7748 Test Vectors)

```rust
#[test]
fn x25519_rfc7748_test_vector_1() {
    // RFC 7748 Section 6.1
    let alice_sk = hex_literal::hex!(
        "77076d0a7318a57d3c16c17251b26645df1fb6b4a2c0c9e9c6b3af7b60c6b3a"
    );
    let bob_pk = hex_literal::hex!(
        "de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f"
    );
    let expected_shared = hex_literal::hex!(
        "4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742"
    );

    let alice_secret = StaticSecret::from(alice_sk);
    let bob_public = PublicKey::from(bob_pk);
    let shared = alice_secret.diffie_hellman(&bob_public);
    assert_eq!(shared.as_bytes(), &expected_shared);
}
```

### ChaCha20-Poly1305 KAT

```rust
#[test]
fn chacha20poly1305_encrypt_decrypt_roundtrip() {
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
    use chacha20poly1305::aead::{Aead, KeyInit};

    let key = Key::from([0u8; 32]);
    let nonce = Nonce::from([0u8; 12]);
    let plaintext = b"IRIS message payload";

    let cipher = ChaCha20Poly1305::new(&key);
    let ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).unwrap();
    let decrypted = cipher.decrypt(&nonce, ciphertext.as_ref()).unwrap();
    assert_eq!(decrypted, plaintext);
}

#[test]
fn chacha20poly1305_tampered_ciphertext_rejected() {
    use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
    use chacha20poly1305::aead::{Aead, KeyInit};

    let key = Key::from([0u8; 32]);
    let nonce = Nonce::from([0u8; 12]);
    let plaintext = b"IRIS message payload";

    let cipher = ChaCha20Poly1305::new(&key);
    let mut ciphertext = cipher.encrypt(&nonce, plaintext.as_ref()).unwrap();
    ciphertext[0] ^= 0xFF; // flip a bit

    let result = cipher.decrypt(&nonce, ciphertext.as_ref());
    assert!(result.is_err(), "Tampered ciphertext must be rejected");
}
```

### BLAKE3 KAT

```rust
#[test]
fn blake3_known_hash() {
    let hash = blake3::hash(b"");
    // Known BLAKE3 hash of empty input
    let expected = hex_literal::hex!(
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc5a68b2b3a3c54c"
    );
    assert_eq!(hash.as_bytes(), &expected);
}

#[test]
fn blake3_message_id_derivation() {
    // Verify message ID is deterministic from content
    let content = b"sender|recipient|timestamp|payload_hash";
    let id1 = blake3::hash(content);
    let id2 = blake3::hash(content);
    assert_eq!(id1, id2, "Hash must be deterministic");
}
```

## Property Tests

Property tests verify invariants hold for all valid inputs, not just specific test cases.

```rust
use proptest::prelude::*;

// Message encoding round-trip
proptest! {
    #[test]
    fn cbor_roundtrip_any_valid_message(
        priority in 0u8..=7u8,
        ttl in 1u32..=86400u32,
        payload_size in 0usize..=512usize,
        payload in proptest::collection::vec(any::<u8>(), 0..512),
    ) {
        let msg = IrisMessage::new_test(priority, ttl, payload);
        let encoded = msg.to_cbor().expect("encode should not fail");
        let decoded = IrisMessage::from_cbor(&encoded).expect("decode should not fail");
        prop_assert_eq!(msg, decoded);
    }
}

// Hop count never increases
proptest! {
    #[test]
    fn hop_count_never_increases_on_relay(
        initial_hop_count in 0u8..=6u8
    ) {
        let mut envelope = test_envelope_with_hop_count(initial_hop_count);
        relay_process(&mut envelope);
        prop_assert!(envelope.hop_count >= initial_hop_count);
    }
}

// TTL never increases
proptest! {
    #[test]
    fn ttl_never_increases_on_relay(
        ttl_remaining in 1u32..=86400u32
    ) {
        let mut envelope = test_envelope_with_ttl(ttl_remaining);
        let ttl_before = envelope.ttl_remaining();
        tokio_test::block_on(simulate_relay_delay(&mut envelope));
        prop_assert!(envelope.ttl_remaining() <= ttl_before);
    }
}

// Priority queue ordering preserved
proptest! {
    #[test]
    fn priority_queue_always_dequeues_highest_priority(
        priorities in proptest::collection::vec(0u8..=7u8, 1..=100)
    ) {
        let mut queue = PriorityQueue::new();
        for (i, &p) in priorities.iter().enumerate() {
            queue.push(test_message(p, i));
        }
        let mut last_priority = 0u8;
        while let Some(msg) = queue.pop() {
            prop_assert!(msg.priority >= last_priority,
                "Dequeued priority {} after {}", msg.priority, last_priority);
            last_priority = msg.priority;
        }
    }
}
```

## Async Unit Tests

```rust
#[tokio::test]
async fn message_expires_after_ttl() {
    tokio::time::pause(); // Control time

    let storage = InMemoryStorage::new();
    let engine = MessageEngine::new(storage.clone());

    let msg = test_message_with_ttl(Duration::from_secs(60));
    engine.enqueue(msg.clone()).await.unwrap();

    // Advance time past TTL
    tokio::time::advance(Duration::from_secs(61)).await;
    engine.run_expiry_pass().await.unwrap();

    let status = storage.get_status(&msg.id).await.unwrap();
    assert_eq!(status, MessageStatus::Expired);
}

#[tokio::test]
async fn duplicate_message_rejected() {
    let dedup = DeduplicationEngine::new_in_memory().await.unwrap();
    let msg_id = MessageId::new();

    assert!(!dedup.is_seen(&msg_id).await.unwrap());
    dedup.mark_seen(&msg_id).await.unwrap();
    assert!(dedup.is_seen(&msg_id).await.unwrap());

    // Second mark is idempotent, does not error
    dedup.mark_seen(&msg_id).await.unwrap();
    assert!(dedup.is_seen(&msg_id).await.unwrap());
}
```

## Running Tests

```bash
# All unit tests
cargo test --workspace --lib

# Specific crate
cargo test -p iris-crypto

# Specific test
cargo test -p iris-crypto -- ed25519_rfc8032

# With output (show println! in passing tests)
cargo test -- --nocapture

# Property tests with more iterations
PROPTEST_CASES=10000 cargo test --lib

# Coverage
cargo llvm-cov --workspace --lib --html
open target/llvm-cov/html/index.html
```
