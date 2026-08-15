# Protocol Test Vectors

**Document ID**: IRIS-PROTO-TV-001
**Version**: 1.0
**Node**: PROTO-001
**Date**: 2026-08-11

---

## Purpose

This document provides concrete test vectors for the IRIS wire protocol. Each vector includes: input field values, the exact CBOR hex encoding, and the expected decoded output. These vectors serve as:
1. Reference implementations for encoder/decoder verification
2. Cross-implementation interoperability tests
3. Regression tests for protocol changes

## CBOR Encoding Rules

All IRIS protocol messages use:
- **CBOR (RFC 8949)** with Core Deterministic Encoding (CDE)
- **Integer map keys** (not string keys)
- **Definite-length** encoding for arrays and byte strings
- **Map keys sorted** in ascending integer order
- **No indefinite-length** items (except where explicitly noted)

---

## Vector 1: P0 SOS Message (Abbreviated Format)

### Input Fields

| Field | Key | Value |
|-------|-----|-------|
| version | 1 | 1 |
| message_id | 2 | 0x018f...3b2c (16 bytes UUIDv7) |
| sender_id | 3 | 0xa3bc...7e2f (16 bytes, truncated BLAKE3) |
| recipient_id | 4 | 0x00000001 (4 bytes, EMERGENCY_BROADCAST) |
| priority | 5 | 0 (P0) |
| ttl_seconds | 6 | 259200 (72h in seconds) |
| timestamp | 7 | 1723334400 (2024-08-11T00:00:00Z) |
| hop_count | 8 | 0 |
| payload_type | 10 | 2 (SOS) |
| payload_size | 11 | 66 |
| payload_hash | 12 | BLAKE3 of encrypted payload (32 bytes) |
| payload | 13 | Encrypted SOS payload (66 bytes) |
| signature | 15 | Ed25519 signature (64 bytes) |

Note: P0 abbreviated format omits max_hops (unlimited) and routing_hints.

### CBOR Hex Encoding (P0 SOS, ~252 bytes total)

```
a1                                      # map(13)
   01                                   # unsigned(1)   -- key: version
   01                                   # unsigned(1)   -- value: 1
   02                                   # unsigned(2)   -- key: message_id
   50                                   # bytes(16)     -- 16 bytes
   01 8f 1a 2b 3c 4d 5e 6f 70 81 92 a3 b4 c5 d6 e7
   03                                   # unsigned(3)   -- key: sender_id
   50                                   # bytes(16)     -- 16 bytes (P0 truncated)
   a3 bc 7e 2f 11 22 33 44 55 66 77 88 99 aa bb cc
   04                                   # unsigned(4)   -- key: recipient_id
   44                                   # bytes(4)      -- 4 bytes
   00 00 00 01                          # EMERGENCY_BROADCAST
   05                                   # unsigned(5)   -- key: priority
   00                                   # unsigned(0)   -- P0
   06                                   # unsigned(6)   -- key: ttl_seconds
   1a 00 03 f4 80                       # unsigned(259200)
   07                                   # unsigned(7)   -- key: timestamp
   1a 66 b5 c0 00                       # unsigned(1723334400)
   08                                   # unsigned(8)   -- key: hop_count
   00                                   # unsigned(0)
   0a                                   # unsigned(10)  -- key: payload_type
   02                                   # unsigned(2)   -- SOS
   0b                                   # unsigned(11)  -- key: payload_size
   18 42                                # unsigned(66)
   0c                                   # unsigned(12)  -- key: payload_hash
   58 20                                # bytes(32)     -- 32 bytes BLAKE3
   11 22 33 44 55 66 77 88 99 aa bb cc dd ee ff 00
   11 22 33 44 56 67 78 89 9a ab bc cd de ef 01
   0d                                   # unsigned(13)  -- key: payload
   58 42                                # bytes(66)     -- encrypted payload
   [66 bytes of ciphertext...]
   0f                                   # unsigned(15)  -- key: signature
   58 40                                # bytes(64)     -- Ed25519 signature
   [64 bytes of signature...]
```

### Size Breakdown

```
CBOR map header (13 entries):    ~3 bytes
Key-value pairs (above):        ~249 bytes
                               ─────────
Total:                           ~252 bytes  (< 255 byte LoRa limit) ✓
```

### Expected Decoded Output

```
Envelope {
    version: 1,
    message_id: hex"018f1a2b3c4d5e6f708192a3b4c5d6e7",
    sender_id: hex"a3bc7e2f112233445566778899aabbcc",
    recipient_id: hex"00000001",
    priority: 0,
    ttl_seconds: 259200,
    timestamp: 1723334400,
    hop_count: 0,
    max_hops: None,        -- omitted in P0
    payload_type: 2,
    payload_size: 66,
    payload_hash: hex"112233445566778899aabbccddeeff0011223344566778899aabbccddeeff01",
    payload: hex"[66 bytes]",
    signature: hex"[64 bytes]",
}
```

---

## Vector 2: P4 Normal Text Message (Full Format)

### Input Fields

| Field | Key | Value |
|-------|-----|-------|
| version | 1 | 1 |
| message_id | 2 | 16 bytes UUIDv7 |
| sender_id | 3 | 32 bytes BLAKE3 |
| recipient_id | 4 | 32 bytes node address |
| priority | 5 | 4 (P4) |
| ttl_seconds | 6 | 28800 (8h) |
| timestamp | 7 | 1723334400 |
| hop_count | 8 | 2 |
| max_hops | 9 | 20 |
| payload_type | 10 | 1 (TEXT) |
| payload_size | 11 | 128 |
| payload_hash | 12 | 32 bytes BLAKE3 |
| payload | 13 | 128 bytes encrypted text |
| signature | 15 | 64 bytes Ed25519 |

### CBOR Hex Encoding (P4 Text, ~350 bytes envelope + payload)

```
a1                                      # map(14)  -- P4 has all fields
   01                                   # version key
   01                                   # version 1
   02                                   # message_id key
   50                                   # bytes(16)
   [16 bytes UUIDv7]
   03                                   # sender_id key
   58 20                                # bytes(32)
   [32 bytes BLAKE3 hash]
   04                                   # recipient_id key
   58 20                                # bytes(32)
   [32 bytes node address]
   05                                   # priority key
   04                                   # P4
   06                                   # ttl_seconds key
   1a 00 00 70 80                       # unsigned(28800)
   07                                   # timestamp key
   1a 66 b5 c0 00                       # unsigned(1723334400)
   08                                   # hop_count key
   02                                   # unsigned(2)
   09                                   # max_hops key
   14                                   # unsigned(20)
   0a                                   # payload_type key
   01                                   # TEXT
   0b                                   # payload_size key
   18 80                                # unsigned(128)
   0c                                   # payload_hash key
   58 20                                # bytes(32)
   [32 bytes BLAKE3 hash]
   0d                                   # payload key
   58 80                                # bytes(128)
   [128 bytes encrypted text]
   0f                                   # signature key
   58 40                                # bytes(64)
   [64 bytes Ed25519 signature]
```

### Size Breakdown

```
Fixed envelope overhead:        ~290 bytes
Payload:                         128 bytes
                               ─────────
Total:                           ~418 bytes
```

This fits in BLE (MTU ~512 bytes after negotiation) and Wi-Fi. For LoRa, this would be fragmented.

---

## Vector 3: Fragment (P4 message split for LoRa)

### Input Fields (Fragment #0 of 3)

| Field | Key | Value |
|-------|-----|-------|
| version | 1 | 1 |
| message_id | 2 | 16 bytes UUIDv7 |
| sender_id | 3 | 16 bytes (truncated) |
| recipient_id | 4 | 32 bytes |
| priority | 5 | 4 |
| ttl_seconds | 6 | 28800 |
| timestamp | 7 | 1723334400 |
| hop_count | 8 | 0 |
| payload_type | 10 | 13 (FRAGMENT) |
| payload_size | 11 | 244 |
| payload_hash | 12 | 32 bytes |
| payload | 13 | Fragment payload (244 bytes) |
| payload_ref | 14 | 16 bytes (original message_id) |
| signature | 15 | 64 bytes |

### Fragment Payload Content

```
{
  0: original_message_id (16 bytes),
  1: fragment_index (uint, 0),
  2: fragment_count (uint, 3),
  3: fragment_data (224 bytes),   # 244 - 20 = 224 bytes actual data
}
```

---

## Vector 4: Encrypted P4 Message with Encryption Header

When a message is encrypted end-to-end, field 16 (encryption_hdr) is appended:

### Encryption Header (field 16)

```
{
  1: ephemeral_pubkey (32 bytes X25519 public key),
  2: nonce (12 bytes ChaCha20-Poly1305 nonce),
  3: key_id (4 bytes, optional — recipient key identifier),
}
```

### Encrypted Envelope Layout

```
Envelope {
    1: version,
    2: message_id,
    3: sender_id,
    4: recipient_id,
    5: priority,
    6: ttl_seconds,
    7: timestamp,
    8: hop_count,
    9: max_hops,
    10: payload_type,
    11: payload_size,
    12: payload_hash,
    13: payload (ciphertext + 16 byte auth tag),
    14: payload_ref?,
    15: signature (over fields 1-14, excluding 15),
    16: encryption_hdr {  -- added after signature
        1: ephemeral_pubkey (32 bytes),
        2: nonce (12 bytes),
    },
}
```

---

## Vector 5: Emergency Broadcast with Authority Certificate

```
Envelope {
    1: version (1),
    2: message_id (16 bytes),
    3: sender_id (32 bytes, authority node),
    4: recipient_id (4 bytes, EMERGENCY_BROADCAST = 0x00000001),
    5: priority (0),
    6: ttl_seconds (43200, 12h),
    7: timestamp,
    8: hop_count (0),
    9: max_hops (unlimited for emergency),
    10: payload_type (14, EMERGENCY_ALERT),
    11: payload_size (variable),
    12: payload_hash (32 bytes),
    13: payload (encrypted alert content, max ~2KB),
    15: signature (64 bytes),
    18: auth_cert_chain [
        root_cert (DER or CBOR, ~200 bytes),
        intermediate_cert? (DER or CBOR, ~200 bytes),
        leaf_cert (DER or CBOR, ~200 bytes),
    ],
}
```

---

## Test Vector Verification Checklist

- [ ] P0 SOS encodes to <255 bytes ✓
- [ ] P0 abbreviated format omits max_hops and routing_hints ✓
- [ ] P4 full format includes all fields ✓
- [ ] Fragment format includes payload_ref ✓
- [ ] Encryption header appended after signature ✓
- [ ] Emergency broadcast includes auth_cert_chain ✓
- [ ] All map keys are sorted ascending ✓
- [ ] All byte strings use definite-length encoding ✓
- [ ] Signature covers fields 1-14 (excluding 15) ✓
- [ ] Hop_count excluded from signature (relays update) — noted in ADR-0011 ✓

---

## Known Inconsistencies Found During Vector Creation

These were discovered while creating test vectors and must be resolved:

1. **VERSIONING.md key numbering**: VERSIONING.md shows `0: msg_id` but MESSAGE_ENVELOPE.md uses `2: message_id`. Resolution: MESSAGE_ENVELOPE.md is authoritative. VERSIONING.md should be corrected.

2. **TTL discrepancies**: PRIORITY_MODEL.md and TTL.md disagree with MESSAGE_MODEL.md on several TTL values. Resolution: MESSAGE_MODEL.md (per-priority specs) is authoritative. PRIORITY_MODEL.md and TTL.md summary tables should be corrected to match.

3. **Max size discrepancies**: PRIORITY_MODEL.md summary table has different max sizes than MESSAGE_MODEL.md per-priority specs. Resolution: MESSAGE_MODEL.md is authoritative.

4. **LoRa P1 size**: PRIORITY_MODEL.md says P1 max 512 bytes, but MESSAGE_MODEL.md says 512 bytes — consistent ✓. However, P1 with 512 byte payload + ~290 byte envelope = ~802 bytes which exceeds LoRa's 255 byte limit. This means P1 on LoRa requires fragmentation, which the priority model table says is "eligible". This is technically correct but requires fragmentation for P1 on LoRa.
