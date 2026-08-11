# Message Envelope

## Overview
The message envelope is the outermost container for all IRIS messages. It is what every node (relay or recipient) sees and handles. The payload inside may be encrypted, but the envelope fields are visible to relays for routing and authentication.

The envelope is the atomic unit of the IRIS protocol: a message is always routed, stored, and forwarded as a complete envelope. Relays cannot read the payload but can read and validate all envelope fields.

## Wire Format (CBOR)

All fields use integer map keys (not string keys) for minimum wire overhead.

```
Envelope {
    1: uint,         version           -- Protocol version (currently 1)
    2: bytes(16),    message_id        -- UUIDv7: 16 bytes
    3: bytes(32),    sender_id         -- BLAKE3(Ed25519_pubkey), 32 bytes
    4: bytes(32),    recipient_id      -- Node/Group address, or broadcast constant
    5: uint,         priority          -- 0=P0_SOS through 7=P7_VIDEO
    6: uint,         ttl_seconds       -- Remaining TTL at origination
    7: uint,         timestamp         -- Unix epoch seconds at origination
    8: uint,         hop_count         -- Current relay hop count (starts 0)
    9: uint,         max_hops          -- Maximum hops permitted
   10: uint,         payload_type      -- Content type (see Payload Types table)
   11: uint,         payload_size      -- Size in bytes of encrypted payload
   12: bytes(32),    payload_hash      -- BLAKE3 hash of encrypted payload
   13: bytes(?),     payload           -- Encrypted payload (inline for small messages)
   14: bytes(16)?,   payload_ref       -- Fragment reference ID (if payload is fragmented)
   15: bytes(64),    signature         -- Ed25519 signature over fields 1-14
   16: bytes(?)?     encryption_hdr    -- X25519 key exchange header (if encrypted)
   17: map?          routing_hints     -- Optional routing metadata (untrusted hints)
   18: bytes(?)?     auth_cert_chain   -- Authority certificate chain (emergency broadcasts only)
}
```

### CBOR Integer Key Rationale
String keys add significant overhead:
- `"version"` = 8 bytes in CBOR (1-byte type + 7-byte string)
- Key `1` = 1 byte in CBOR

For the P0 255-byte budget, integer keys save ~100 bytes of overhead vs string keys.

### Payload Types

| Code | Type | Description | Max Size |
|------|------|-------------|----------|
| 1 | TEXT | Plain text message | 64KB |
| 2 | SOS | Emergency SOS signal | 255 bytes total envelope |
| 3 | LOCATION | GPS coordinates | 256 bytes |
| 4 | IMAGE | Compressed image | 2MB |
| 5 | VOICE | Compressed audio | 5MB |
| 6 | VIDEO | Compressed video | 50MB |
| 7 | FILE | Generic file | 50MB |
| 8 | MEDICAL | Medical emergency | 512 bytes |
| 9 | ACK | Delivery acknowledgement | 128 bytes |
| 10 | ROUTING_GOSSIP | Routing table update | 1KB |
| 11 | CAPABILITY | Node capability bundle | 256 bytes |
| 12 | SYNC_REQUEST | Message sync request | 4KB |
| 13 | FRAGMENT | Message fragment | MTU |
| 14 | EMERGENCY_ALERT | Authority emergency broadcast | 2KB |
| 15 | GROUP_KEY | Group key package | 4KB |
| 16 | KEY_ROTATION | Identity key rotation notice | 1KB |

## Size Budget Analysis

### P0 SOS Envelope (255-byte LoRa Hard Limit)

P0 uses an abbreviated envelope format to maximize payload space. Differences from standard:
- `sender_id`: 16 bytes (truncated BLAKE3 output) instead of 32 bytes
- `recipient_id`: 4 bytes (constant for EMERGENCY_BROADCAST) instead of 32 bytes
- `max_hops`: omitted (effectively unlimited for P0)
- `routing_hints`: omitted

```
CBOR overhead (map header, key/value tags): ~20 bytes
version (key 1B + val 1B):                   2 bytes
message_id (key 1B + len 1B + 16B):         18 bytes
sender_id (key 1B + len 1B + 16B):          18 bytes  ← 16B for P0
recipient_id (key 1B + len 1B + 4B):         6 bytes  ← 4B for P0 EMERGENCY_BROADCAST
priority (key 1B + val 1B):                  2 bytes
ttl_seconds (key 1B + len 2B + 4B):         7 bytes
timestamp (key 1B + len 2B + 4B):           7 bytes
hop_count (key 1B + val 1B):                 2 bytes
payload_type (key 1B + val 1B):              2 bytes
payload_size (key 1B + val 1B):              2 bytes
payload_hash (key 1B + len 1B + 32B):       34 bytes
signature (key 1B + len 1B + 64B):          66 bytes
─────────────────────────────────────────────────────
Fixed total:                               ~186 bytes
Available for payload (inline):             69 bytes

P0 payload: GPS 16B + short text 50B = 66 bytes → fits.
Example: "HELP. Trapped. Injuries. 23.2217,69.6620" = 42 chars + 16B GPS = 58B → fits.
```

### P4 Normal Text Envelope (No Size Limit — BLE/Wi-Fi/Internet)

Standard format, full fields:
```
CBOR overhead:                              ~25 bytes
version:                                     2 bytes
message_id:                                 18 bytes
sender_id (32B):                            34 bytes
recipient_id (32B):                         34 bytes
priority:                                    2 bytes
ttl_seconds:                                 7 bytes
timestamp:                                   7 bytes
hop_count:                                   2 bytes
max_hops:                                    2 bytes
payload_type:                                2 bytes
payload_size (up to 64KB → 3 bytes):         5 bytes
payload_hash:                               34 bytes
signature:                                  66 bytes
encryption_hdr (see below):                ~50 bytes
─────────────────────────────────────────────────────
Fixed envelope overhead:                  ~290 bytes
Payload: up to 64KB (fragmented for BLE/LoRa)
```

### Fragmented Message Envelope
For P4-P7 messages larger than transport MTU:
- Each fragment is a separate envelope with `payload_type = FRAGMENT`
- Fragment payload includes: `{message_id, fragment_seq, total_fragments, data}`
- Original message `payload_ref` = UUIDv7 reference to fragment set
- Fragments independently routed (may arrive out of order)

## Signature Computation

The Ed25519 signature covers all envelope fields except the `signature` field itself (field 15).

### Signing Procedure
```rust
fn sign_envelope(envelope_without_sig: &Envelope, privkey: &Ed25519PrivateKey) -> [u8; 64] {
    // Serialize envelope to CBOR (without signature field)
    let cbor_bytes = cbor_serialize_without_field(envelope_without_sig, field_key: 15);
    // Sign the serialized bytes
    Ed25519::sign(privkey, &cbor_bytes)
}
```

### Verification Procedure
```rust
fn verify_envelope(envelope: &Envelope, pubkey: &Ed25519PublicKey) -> bool {
    // Serialize same envelope without signature field (must match signing procedure exactly)
    let cbor_bytes = cbor_serialize_without_field(envelope, field_key: 15);
    // Verify signature
    Ed25519::verify(pubkey, &cbor_bytes, &envelope.signature)
}
```

### Canonical Serialization
For signature verification to work across different implementations, CBOR serialization must be canonical:
- Map keys must be sorted by key value (ascending integer order)
- Definite-length encoding for all arrays and byte strings
- No optional fields omitted after their position (use null/absent map entry consistently)

This is CBOR "Core Deterministic Encoding" (CDE), RFC 8949 §4.2.

## Encryption Header Format

For private messages encrypted with X25519 + ChaCha20-Poly1305:

```
encryption_hdr = {
    1: bytes(32),    ephemeral_pubkey  -- X25519 ephemeral public key
    2: bytes(12),    nonce             -- ChaCha20-Poly1305 nonce (random)
    3: bytes(4)?     key_id            -- Recipient key ID (if recipient has multiple keys)
}
```

### Encryption Procedure
```rust
fn encrypt_payload(plaintext: &[u8], recipient_pubkey: &X25519PublicKey) -> (Vec<u8>, EncryptionHdr) {
    // Generate ephemeral X25519 key pair
    let eph_privkey = X25519PrivateKey::generate();
    let eph_pubkey = eph_privkey.public_key();
    
    // ECDH: compute shared secret
    let shared_secret = eph_privkey.diffie_hellman(recipient_pubkey);
    
    // Key derivation: HKDF-SHA256
    let key = HKDF::new(shared_secret.as_bytes(), b"iris-message-key-v1");
    
    // Encrypt: ChaCha20-Poly1305
    let nonce = random_bytes(12);
    let ciphertext = ChaCha20Poly1305::encrypt(&key, &nonce, plaintext);
    
    let hdr = EncryptionHdr { ephemeral_pubkey: eph_pubkey, nonce };
    (ciphertext, hdr)
}
```

### Decryption Procedure
```rust
fn decrypt_payload(ciphertext: &[u8], hdr: &EncryptionHdr, local_privkey: &X25519PrivateKey) -> Option<Vec<u8>> {
    // ECDH with ephemeral public key from header
    let shared_secret = local_privkey.diffie_hellman(&hdr.ephemeral_pubkey);
    
    // Key derivation (same as encryption)
    let key = HKDF::new(shared_secret.as_bytes(), b"iris-message-key-v1");
    
    // Decrypt and verify authentication tag
    ChaCha20Poly1305::decrypt(&key, &hdr.nonce, ciphertext)
    // Returns None if authentication tag invalid (tampered ciphertext)
}
```

## Routing Hints

Optional field `routing_hints` (map, field key 17). Contents are **not authenticated** — relays use as suggestions only, never as authoritative routing decisions.

```
routing_hints = {
    1: string?,    last_known_region  -- e.g. "IN-GJ-19" (ISO 3166-2 district)
    2: bytes(32)?, gateway_seen_via   -- NodeId of gateway that forwarded this
    3: float?,     delivery_prob      -- Sender's delivery probability estimate
    4: bool?,      requires_gateway   -- This message requires Internet gateway to deliver
    5: [bytes(32)]? preferred_relays  -- Suggested relay nodes (sender's routing knowledge)
}
```

Routing hints are gossip-quality data: useful when available, not required for correctness.

## Relay Node Behavior

When a relay node processes an envelope:

```
1. Parse CBOR envelope
   → Parse error: DROP + log PARSE_ERROR
   
2. Check protocol_version (field 1)
   → Major version mismatch: DROP + log VERSION_MISMATCH
   
3. Verify Ed25519 signature (field 15)
   → Invalid: DROP + log SIGNATURE_INVALID (security boundary, never relay)
   
4. Check deduplication (message_id, field 2)
   → Already seen: DROP silently
   
5. Check TTL (ttl_seconds, field 6, and timestamp, field 7)
   → Expired: DROP + log TTL_EXPIRED
   
6. Check hop_count (field 8) vs max_hops (field 9)
   → hop_count >= max_hops: DROP + log HOP_LIMIT
   
7. Increment hop_count
   
8. Check if I am the recipient
   → Yes: decrypt payload, deliver to application
   → No: continue relay
   
9. Select next hop(s) via routing engine
   
10. Update relay_count and last_relay_node in local message record
    (these are local tracking fields, not in the wire format)
    
11. Forward via Transport Manager
```

### What Relays MUST NOT Modify
- `message_id` (field 2): immutable, used for deduplication
- `sender_id` (field 3): must remain original sender
- `recipient_id` (field 4): must remain intended recipient
- `payload` (field 13): modification breaks signature + auth tag
- `payload_hash` (field 12): must match actual payload
- `signature` (field 15): any modification invalidates it

Violation of these rules is detectable: signature verification fails at next node → message dropped.

### What Relays MAY Do
- Increment `hop_count` — wait, this is caught by the previous rule!

Actually: relays do NOT modify the signed envelope at all. The hop_count in the relay's own copy is incremented for local tracking, but the wire format sent to the next hop must have a valid signature. The signature was computed by the originator. If a relay modified hop_count and re-signed... they can't, because they don't have the sender's private key.

**Resolution**: hop_count is NOT covered by the signature for relay purposes. The field is decrement-tracked at each hop: each receiving node decrements its own expectation, not the wire value. Alternatively: max_hops is signed, hop_count in the wire format is unsigned but can be freely updated by each relay (and this part is specifically excluded from the signature scope by design).

This is documented as a protocol design decision in ADR-0011.

## Authority Certificate Chain (Emergency Broadcasts)

Field 18: `auth_cert_chain`, present only when `recipient_id == EMERGENCY_BROADCAST`.

```
auth_cert_chain = [
    bytes(?)   root_cert,         -- Self-signed root CA certificate (DER or CBOR)
    bytes(?)?  intermediate_cert, -- Optional intermediate (e.g. state authority)
    bytes(?)   leaf_cert,         -- Leaf certificate for this specific device
]
```

Each certificate contains: public key, subject (authority name), validity period, issuer signature.

Relay behavior on EMERGENCY_BROADCAST:
1. Validate certificate chain (leaf signed by intermediate or root, root is trusted)
2. Verify message signature using leaf certificate public key
3. If valid: relay immediately to all neighbors (epidemic)
4. If invalid: DROP + log UNAUTHORIZED_BROADCAST

Root CA public keys are embedded in IRIS software build (like browser CA roots). They cannot be changed without a software update.
