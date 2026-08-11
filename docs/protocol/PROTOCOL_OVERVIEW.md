# Protocol Overview

## Purpose
IRIS protocol defines how messages are structured, addressed, routed, and delivered across a heterogeneous, disruption-tolerant network. The protocol is transport-agnostic: the same binary message format is used over BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite, or Internet.

The protocol is designed for the hardest case: two phones with BLE, no Internet, no infrastructure, in a disaster zone. Everything that works in that scenario also works when better connectivity is available.

## Design Goals

1. **Minimal overhead for emergency messages** — P0 SOS must fit in 255 bytes (LoRa maximum payload)
2. **Transport independence** — same binary format over all transports; transport adapter handles delivery
3. **Forward compatibility** — new message types and fields can be added without breaking old nodes
4. **Security by default** — all messages are signed; private messages are encrypted; no opt-out
5. **Self-describing** — CBOR encoding allows partial parsing without full schema; survives version mismatch
6. **DTN-friendly** — supports store-carry-forward, multi-hop relay, contact-based routing
7. **Minimal state** — message contains everything needed to route and deliver it; no external state required

## Wire Format

**CBOR (Concise Binary Object Representation), RFC 8949.**

### Why CBOR

| Format | Binary | Self-describing | Schema required | Established standard | DTN usage |
|--------|--------|----------------|----------------|--------------------|-----------| 
| CBOR | Yes | Yes | No | RFC 8949 | BPv7 uses CBOR |
| Protocol Buffers | Yes | No | Yes (compiled) | Yes (Google) | Rare |
| MessagePack | Yes | Partial | No | Community spec | Rare |
| JSON | No (text) | Yes | No | RFC 8259 | No |
| ASN.1/DER | Yes | Partial | Yes | ISO/ITU | Rare |
| Custom binary | Yes | No | Yes (manual) | No | N/A |

CBOR chosen for:
- **Standard**: IETF RFC, implemented in every major language
- **No schema compilation**: new nodes can parse old messages without schema files
- **Self-describing**: survives partial schema mismatch between versions
- **BPv7 alignment**: IETF DTN Bundle Protocol uses CBOR; IRIS is conceptually aligned
- **Rust support**: `ciborium` crate is well-maintained, `serde` compatible
- **Size**: similar to MessagePack, significantly smaller than JSON

### CBOR Integer Map Keys
All IRIS protocol CBOR structures use integer keys (not string keys) for minimum wire size.
```
Key 1 = version
Key 2 = message_id
Key 3 = sender_id
Key 4 = recipient_id
(etc. — see MESSAGE_ENVELOPE.md for full key map)
```

Integer key (1 byte for 0-23) vs string key ("version" = 7 bytes) saves significant space in the 255-byte P0 budget.

## Relationship to Bundle Protocol (BPv7)

BPv7 (RFC 9171) is the IETF standard for DTN bundle protocol. IRIS is conceptually aligned with BPv7 but does not fully implement it.

### BPv7 Concepts Adopted
- Bundle = IRIS Message (self-contained, routable unit)
- Bundle lifetime = IRIS TTL
- Endpoint ID = IRIS NodeId (cryptographic address)
- Custody = IRIS store-and-forward (node takes responsibility for delivery)
- Contact graphs = IRIS temporal graph and contact schedule

### Why IRIS Does Not Implement BPv7 Directly

| Concern | BPv7 approach | IRIS approach | Reason |
|---------|-------------|-------------|-------|
| Endpoint ID format | URI (e.g., `dtn://host/endpoint`) | 32-byte hash | URI is too large for LoRa; cryptographic address is self-certifying |
| Bundle size overhead | ~100-200 bytes minimum | ~226 bytes (P0), flexible | Similar; BPv7 overhead is compatible but IRIS P0 format is more compact |
| Security (BPSec) | Separate BPSec extension (RFC 9172) | Integrated Ed25519 + ChaCha20 | BPSec adds complexity; IRIS integrates security as mandatory, not optional |
| Complexity | Full BPv7 is complex | Simplified subset | IRIS is mobile-first; full BPv7 requires significant implementation effort |
| Admin records | Administrative record types | Simplified ACK | BPv7 admin records not needed for IRIS use case |

### Future BPv7 Interoperability
IRIS may implement a BPv7 gateway adapter in future for interoperability with DTN infrastructure:
- NASA ION software
- Serval Rhizome networks
- Academic DTN research networks
- IETF DTNWG reference implementations

This would allow IRIS mesh to connect to established DTN infrastructure during large-scale disasters. Tracked in roadmap as GATEWAY_BPV7 feature.

## Protocol Versioning

### Version Field
Every IRIS envelope carries `protocol_version: uint`.

Current version: **1** (initial production version).

### Version Handling Rules

| Condition | Behavior |
|---------|---------|
| Received version == local version | Normal processing |
| Received minor version > local minor | Parse known fields, ignore unknown fields (forward compatible) |
| Received major version > local major | Log UNKNOWN_VERSION, DROP message |
| Received version < local version | Process with backward compatibility mode (if implemented) or DROP |

Major version increments: breaking changes (field renames, type changes, incompatible semantics).
Minor version increments: additive changes (new optional fields, new message types).

### Version Negotiation
On peer contact, capability bundles include `protocol_version: uint`. If major versions differ, nodes log the mismatch and limit communication to mutually supported features. No automatic version negotiation protocol — simplicity is preferred.

### Upgrade Path
- New minor version: deploy new nodes; old nodes continue to work, ignoring new fields
- New major version: coordinated upgrade via OTA; transition period where both versions coexist; old nodes drop new-format messages gracefully

## Message Classes and Priorities

Priority is not just queue ordering — it determines routing algorithm, transport selection, storage policy, and relay aggressiveness.

| Priority | Class | Max Payload | Wire Limit | TTL | Routing | ACK | Storage |
|---------|-------|------------|-----------|-----|---------|-----|---------|
| P0 | SOS | ~40 bytes | 255 bytes | 72h | Epidemic | Required | Always |
| P1 | Medical | ~300 bytes | 512 bytes | 24h | Multi-path | Required | Always |
| P2 | Location | ~100 bytes | 256 bytes | 12h | PRoPHET | Optional | Always |
| P3 | Emergency text | ~850 bytes | 1 KB | 12h | PRoPHET | Recommended | Always |
| P4 | Normal text | ~63 KB | 64 KB | 8h | Standard | Optional | Default |
| P5 | Image | ~2 MB | 2 MB | 4h | Direct preferred | No | If space |
| P6 | Voice | ~5 MB | 5 MB | 2h | Wi-Fi/Internet only | No | If space |
| P7 | Video/bulk | ~50 MB | 50 MB | 1h | Internet only | No | No |

### P0 Wire Size Budget Detail
LoRa limits payloads to 255 bytes at SF7/BW125. P0 messages must fit this limit.

```
P0 Envelope byte budget:
  CBOR map header:          3 bytes
  version (key+val):        2 bytes
  message_id (key+16B):    18 bytes
  sender_id (key+16B):     18 bytes  ← truncated to 16B for P0 (vs 32B standard)
  recipient_id (key+4B):    6 bytes  ← EMERGENCY_BROADCAST is 4B constant for P0
  priority (key+val):       2 bytes
  ttl_seconds (key+4B):     6 bytes
  timestamp (key+4B):       6 bytes
  hop_count (key+val):      2 bytes
  max_hops (key+val):       2 bytes
  payload_type (key+val):   2 bytes
  payload_size (key+1B):    3 bytes
  payload (key+?B):         variable
  signature (key+64B):     66 bytes  ← Ed25519 signature
  CBOR overhead padding:   10 bytes  ← conservative estimate
  ─────────────────────────────────
  Fixed fields total:     ~146 bytes
  Available for payload:   109 bytes

P0 payload: GPS coordinates (16 bytes) + short text (up to 90 bytes) = 106 bytes
Target: "HELP. 5 injured. Need rescue." + coordinates = fits comfortably.
```

Note: Standard messages use full 32-byte sender_id and full 32-byte recipient_id. P0 uses abbreviated format to maximize payload space.

## Security Model

### Mandatory Signatures
Every IRIS message is signed by the originating node using Ed25519.
- Signature covers: all envelope fields except the signature field itself
- Verification: every relay node verifies before relaying
- Effect: modification by any relay invalidates signature and causes DROP

### Optional Encryption
- Broadcast messages (EMERGENCY_BROADCAST, group messages): signed but not encrypted
- Private messages (P0 with known recipient, P3-P4): signed + encrypted
- Encryption scheme: X25519 key agreement + HKDF + ChaCha20-Poly1305

### Emergency Broadcast Authority
Emergency broadcasts use a separate authority certificate chain:
- Root CA: IRIS Emergency Authority (key managed by IRIS project + government partners)
- Intermediate: State disaster management authorities (SDMA)
- Leaf: Individual authorized operator devices

Any relay node validates the certificate chain before propagating an emergency broadcast. Invalid authority → DROP + log UNAUTHORIZED_BROADCAST.

### Group Message Security
- Group key: symmetric key shared among group members
- Group key encrypted with each member's public key (stored in group key package)
- Group messages: encrypted with group key, signed by sender
- Group key rotation: when member leaves, new key distributed to remaining members

## Protocol Document Index

| Document | Content |
|---------|---------|
| MESSAGE_ENVELOPE.md | Wire format: complete CBOR field definitions, size analysis |
| ADDRESSING.md | Node addressing, cryptographic addresses, privacy-preserving advertisement |
| MESSAGE_MODEL.md | Message types, lifecycle, storage, per-priority specifications |
| DELIVERY_POLICIES.md | Delivery semantics: best-effort, confirmed, priority, broadcast |
| PRIORITY_MODEL.md | Priority system design, queue ordering, interaction with routing |
| TTL.md | Time-to-live design: temporal TTL, hop TTL, clock skew handling |
| ACKNOWLEDGEMENTS.md | ACK design, correlation, timeout behavior |
| RETRIES.md | Retry logic, exponential backoff, multi-path retry |
| DEDUPLICATION.md | Duplicate detection: Bloom filters, exact cache, cross-transport dedup |
| FRAGMENTATION.md | Message fragmentation: per-transport MTU, fragment envelope format |
| REASSEMBLY.md | Fragment reassembly: buffering, timeout, partial delivery |
| SYNCHRONIZATION.md | State sync: Bloom filter exchange, message transfer, routing gossip |
| VERSIONING.md | Version management: major/minor, upgrade path, negotiation |
| COMPATIBILITY.md | Interoperability: old/new node compatibility, BPv7 gateway |

## Implemented vs Planned

| Component | Status | Notes |
|-----------|--------|-------|
| CBOR envelope format | Designed | Implementation pending |
| Ed25519 signatures | Designed | Primitives selected |
| X25519 + ChaCha20-Poly1305 | Designed | Primitives selected |
| BLE transport adapter | Planned | EXP-0001, EXP-0002 pending |
| LoRa transport adapter | Planned | EXP-0007 pending |
| P0 SOS message type | Designed | Fits LoRa budget confirmed |
| PRoPHET routing | Designed | Algorithm documented |
| Store-carry-forward | Designed | SQLite store documented |
| BPv7 gateway | Roadmap | Post-v1 |
| Satellite transport | Roadmap | Hardware procurement pending |
