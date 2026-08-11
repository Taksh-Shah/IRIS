# Message Authentication Design

## Overview

Every IRIS message is cryptographically signed by the sender. Authentication is not optional —
unauthenticated messages are rejected at every relay. This document describes what is signed,
the algorithm used, how signatures are verified, and how public keys are distributed across
the network without centralized infrastructure.

## Core Principle

In a decentralized, infrastructure-free network, cryptographic signing replaces the role of
a central authority. A relay node that cannot reach the IRIS server can still verify that a
message was genuinely sent by its claimed author. This is the foundation of trust in IRIS.

## What Is Signed

The signature covers a canonical CBOR serialization of the message header fields. Payload
content is covered indirectly via `payload_hash`.

### Signed Data Structure (Rust definition)

```rust
/// The data structure that is serialized to CBOR and then signed.
/// This must be byte-for-byte reproducible across all platforms.
#[derive(Serialize)]
struct SignedFields<'a> {
    version: u8,              // Protocol version (currently 1)
    message_id: &'a [u8; 16], // UUID v4, random bytes
    sender_id: &'a NodeId,    // Sender's public key hash (32 bytes)
    recipient_id: &'a RecipientId, // Recipient NodeId or GroupId or broadcast
    priority: u8,             // 0-7
    ttl_seconds: u32,         // Message time-to-live
    timestamp: i64,           // Unix timestamp, seconds
    hop_count: u8,            // Current hop count (0 at origin)
    max_hops: u8,             // Maximum allowed hops
    payload_type: u16,        // Registered payload type code
    payload_size: u32,        // Byte count of payload
    payload_hash: &'a [u8; 32], // SHA-256 of payload bytes
}
```

### Why These Fields?

- `version`: prevents signature reuse across protocol versions
- `message_id`: ensures each message has a unique signature (no two messages share an ID)
- `sender_id` + `recipient_id`: authenticates the communication relationship
- `priority`: prevents priority escalation attack (relay cannot upgrade P4 → P0)
- `ttl_seconds`: prevents TTL extension attack (relay cannot extend message lifetime)
- `timestamp`: provides freshness, prevents infinite-lifetime replay
- `hop_count`: included at 0 (origin value) — relays verify this field was 0 at signing
- `max_hops`: prevents hop limit manipulation
- `payload_type` + `payload_size` + `payload_hash`: ties signature to specific payload

### What Is NOT in the Signed Fields

- Routing metadata added by relays (e.g., `received_via`, `relay_path`)
- Encryption headers (these are added after signing)
- Transport-layer framing

## Signature Algorithm

**Algorithm**: Ed25519 as defined in RFC 8032.

**Why Ed25519:**
- **Deterministic**: same message + same key always produces same signature.
  No randomness required at signing time, eliminating "weak RNG" class of vulnerabilities.
- **Fast**: verification in ~50 microseconds on Cortex-M4 class hardware.
  Critical for relay nodes that must verify every message before forwarding.
- **Small**: 64-byte signature, 32-byte public key. Essential for LoRa transport
  (250-byte payload limit).
- **Secure**: no known practical attacks. Well-studied since 2011.
- **Constant-time**: reference implementations are constant-time, resisting timing attacks.

**Signature format:**
```
Ed25519 signature = 64 bytes
  [0..31]  = R (EC point, compressed)
  [32..63] = S (scalar)
```

**Signing process:**
```
1. Serialize SignedFields to canonical CBOR (deterministic encoding — no float, sorted keys)
2. signature = Ed25519_sign(sender_private_key, cbor_bytes)
3. Embed signature in MessageEnvelope.signature field (64 bytes)
```

## Signature Verification

Every relay node performs the following before forwarding any message:

```
VERIFY(message):
  1. Reconstruct SignedFields from message header
     (set hop_count = 0, use original signed values for all other fields)
  2. Re-serialize to canonical CBOR (same algorithm as signer used)
  3. Retrieve sender's public key (see Key Distribution below)
  4. result = Ed25519_verify(public_key, cbor_bytes, message.signature)
  5. If result == INVALID → DROP message, log rejection
  6. Also verify: SHA256(payload) == message.payload_hash
  7. If hash mismatch → DROP (payload was tampered after signing)
```

**Verification is mandatory.** There is no "trust mode" or "skip verification" flag.
A relay that forwards unverified messages is violating the protocol.

## Key Distribution

### The Key Distribution Problem

Verification requires the sender's full Ed25519 public key (32 bytes). But `sender_id` is
a 32-byte hash of the public key (SHA-256), not the key itself. How does a relay that has
never seen the sender obtain the public key?

### Solution: Inline Key on First Contact

When a node sends its first message to a recipient or through a relay that has not seen it
before, the public key is included inline in the MessageEnvelope:

```
MessageEnvelope {
  header: { ... signed fields ... },
  signature: [64 bytes],
  sender_public_key: Option<[u8; 32]>,  // Present on first contact
  payload: EncryptedPayload,
}
```

The relay caches `sender_id → sender_public_key` after first successful verification.
Subsequent messages from same sender do not need to include the public key.

### Key Verification

When `sender_public_key` is present, relay verifies:
```
SHA256(sender_public_key) == message.header.sender_id
```

If this fails, the public key is forged. DROP immediately.

### Key Cache

```
struct KeyCache {
  entries: LruCache<NodeId, PublicKey>,  // Max 100,000 entries
  persistent_store: SqliteStore,          // Persisted for known contacts
}
```

Known contacts have their keys persisted to storage. Unknown relayed keys are cached
in LRU (evicted when cache is full).

### Key Gossip

When a relay needs a public key it does not have and cannot get from inline:
1. Check local cache and storage — not found
2. Send `KEY_REQUEST { node_id: target_id }` to neighboring peers
3. Peers respond with `KEY_RESPONSE { node_id, public_key }` if they have it
4. Validate response: SHA256(public_key) == target_id
5. Cache and verify message

Key gossip is rate-limited to prevent gossip flooding.

## Group Message Authentication

Group messages are signed with the individual sender's Ed25519 key (same as direct messages).
This means:
- Recipients know exactly WHO in the group sent each message
- Group membership does not affect individual accountability
- A member cannot repudiate a group message they sent

**Optional group HMAC**: for closed groups where message authenticity matters to all members:
```
group_hmac = HMAC-SHA256(group_shared_key, message_id || sender_id || payload_hash)
```

Group HMAC is an additional layer — it authenticates group membership. HMAC key is
distributed to group members via secure direct message.

## Emergency Broadcast Authentication

Emergency broadcasts require TWO layers of authentication:

**Layer 1**: Ed25519 signature of sender (authority's key) — same as all messages.

**Layer 2**: Authority certificate chain (see EMERGENCY_ABUSE.md):
```
Certificate chain: Authority_Root → State_Auth → District_Auth → Sender
Each link: X.509-like certificate signed by parent authority's key
```

Every relay verifies the full certificate chain. A malicious relay cannot fabricate
authority signatures without the authority's private key.

## Rust Implementation Notes

Core types in `crates/iris-protocol/src/auth.rs`:

```rust
pub struct MessageSignature([u8; 64]);
pub struct PublicKey([u8; 32]);
pub struct NodeId([u8; 32]); // SHA-256 of public key

impl NodeId {
    pub fn from_public_key(key: &PublicKey) -> Self {
        NodeId(sha256(key.0))
    }
    pub fn verify_key(&self, key: &PublicKey) -> bool {
        self.0 == sha256(key.0)
    }
}
```

Signing uses `ed25519-dalek` crate (audited, constant-time).
CBOR serialization uses `ciborium` crate with deterministic encoding mode.

## Security Considerations

- **Non-repudiation**: Ed25519 signatures provide non-repudiation. A sender cannot later
  deny sending a message (assuming their private key was not compromised).
- **Forward security**: IRIS does not currently provide forward security for signatures.
  If a private key is compromised, past messages can be attributed to that key.
  (Encryption keys use ephemeral X25519, providing forward secrecy for payload content.)
- **Replay vs. authentication**: signature verifies authenticity, not freshness.
  Replay protection is separate — see REPLAY_PROTECTION.md.
