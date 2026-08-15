# Protocol Architecture Alternatives

**Document ID**: IRIS-PROTO-ALT-001
**Version**: 1.0
**Status**: ACTIVE
**Node**: PROTO-001
**Date**: 2026-08-11

---

## Purpose

This document presents three competing architectural alternatives for the IRIS wire protocol envelope format. Each is evaluated against the same criteria. One will be selected and codified in an ADR.

## Evaluation Criteria

| Criterion | Weight | Description |
|-----------|--------|-------------|
| P0 LoRa fit | CRITICAL | Must fit emergency SOS in ≤255 bytes |
| Extensibility | HIGH | Support future fields without breaking changes |
| Interoperability | MEDIUM | Compatibility with BPv7/DTN ecosystem |
| Implementation simplicity | HIGH | Low complexity for constrained devices |
| Wire efficiency | HIGH | Minimum overhead per message |
| Security model | CRITICAL | Clear signature/encryption boundaries |
| Fragmentation support | HIGH | Clean handling of multi-transport messages |
| Platform constraints | HIGH | Works on BLE, Wi-Fi, LoRa, Satellite |

---

## Alternative A: Flat Envelope (Current IRIS Design)

### Description

Single CBOR map with integer keys 1-18. All envelope fields in one flat structure. Optional fields omitted when not needed. P0 uses abbreviated format (truncated sender_id, constant recipient_id).

### Structure

```
Envelope {
    1: version
    2: message_id (16 bytes)
    3: sender_id (32 bytes, 16 for P0)
    4: recipient_id (32 bytes, 4 for P0)
    5: priority
    6: ttl_seconds
    7: timestamp
    8: hop_count
    9: max_hops
    10: payload_type
    11: payload_size
    12: payload_hash (32 bytes)
    13: payload (encrypted)
    14: payload_ref? (fragment reference)
    15: signature (64 bytes)
    16: encryption_hdr? (ephemeral key + nonce)
    17: routing_hints? (untrusted)
    18: auth_cert_chain? (emergency only)
}
```

### P0 Size Budget

```
CBOR overhead:           ~20 bytes
Fixed fields:           ~186 bytes (P0 abbreviated)
Available payload:       69 bytes
GPS + text:              66 bytes → FITS ✓
```

### Pros

1. **Minimal overhead**: Single map header, no block headers
2. **Simple parsing**: One CBOR map decode
3. **P0 optimized**: Abbreviated format saves ~50 bytes vs full
4. **Fast serialization**: Single pass encoding
5. **Low memory**: No block array allocation needed
6. **Proven design**: Already documented and analyzed

### Cons

1. **Limited extensibility**: New fields require schema version bump
2. **No per-field integrity**: Signature covers everything or nothing
3. **No native BPv7 interop**: Requires translation gateway
4. **Fixed field semantics**: Hard to add new block types
5. **All-or-nothing parsing**: Must parse entire envelope to read one field
6. **No block replication control**: Can't mark "this field must be in every fragment"

### Score

| Criterion | Score | Notes |
|-----------|-------|-------|
| P0 LoRa fit | 10/10 | Optimized abbreviated format |
| Extensibility | 5/10 | New fields need version bump |
| Interoperability | 3/10 | No native BPv7 compatibility |
| Implementation simplicity | 9/10 | Single map, straightforward |
| Wire efficiency | 9/10 | Minimal overhead |
| Security model | 7/10 | Clear but coarse-grained |
| Fragmentation support | 6/10 | Basic, no block control |
| Platform constraints | 9/10 | Works everywhere |

**Total: 58/80**

---

## Alternative B: BPv7 Block-Based Structure

### Description

Adopt BPv7's block-based structure natively. Each bundle is a CBOR indefinite-length array of blocks. Primary block + payload block + extension blocks. Each block has its own type, number, and CRC.

### Structure

```
Bundle = CBOR indefinite-length array [
    Primary Block (block 0):
        [version, flags, crc_type, destination_eid, source_eid,
         creation_timestamp, lifetime, payload_length, ...]

    Canonical Block (block 1 = payload):
        [block_type=1, block_num=1, flags, crc_type, payload_data]

    Extension Block (block 2 = hop count):
        [block_type=19, block_num=2, flags, crc_type, hop_count, max_hops]

    Extension Block (block 3 = previous node):
        [block_type=6, block_num=3, flags, crc_type, prev_node_eid]

    Extension Block (block 4 = signature/BIB):
        [block_type=2, block_num=4, flags, crc_type, signature]

    ...
    CBOR break stop code
]
```

### P0 Size Budget

```
CBOR array header:        ~2 bytes
Primary block:           ~80 bytes (EID-based addressing, creation timestamp)
Payload block:           ~3 bytes overhead + payload
Signature block:         ~5 bytes overhead + 64 bytes
Hop count block:         ~5 bytes overhead + 2 bytes
Previous node block:     ~5 bytes overhead + variable
CRC (per block):         0-4 bytes per block
─────────────────────────────────
Estimated fixed overhead: ~120-150 bytes (with 4-5 blocks)
Available payload:       105-135 bytes
```

### Pros

1. **Native BPv7 interoperability**: Direct compatibility with DTN ecosystem
2. **Strong extensibility**: New block types without breaking changes
3. **Per-block integrity**: Each block can have its own CRC
4. **Block replication control**: "Must replicate in every fragment" flag
5. **Forward compatibility**: Unknown block types ignored by old nodes
6. **Security targeting**: BIBs can target specific blocks
7. **Standardized**: Well-documented, widely understood

### Cons

1. **Higher overhead**: Block headers add ~3-5 bytes per block × multiple blocks
2. **Complex parsing**: Must iterate block array, handle variable order
3. **Larger P0 footprint**: Block headers consume precious LoRa budget
4. **EID addressing**: BPv7 EIDs are URIs (variable length, often 20-100+ bytes)
5. **Implementation complexity**: Block management, ordering, CRC computation
6. **Memory overhead**: Must handle variable number of blocks
7. **DTN time**: Milliseconds-since-2000 epoch adds bytes vs Unix seconds

### Score

| Criterion | Score | Notes |
|-----------|-------|-------|
| P0 LoRa fit | 6/10 | Block headers eat into 255-byte budget |
| Extensibility | 10/10 | New block types, forward compatible |
| Interoperability | 10/10 | Native BPv7 |
| Implementation simplicity | 5/10 | Complex block management |
| Wire efficiency | 5/10 | Per-block overhead adds up |
| Security model | 9/10 | Per-block security targeting |
| Fragmentation support | 10/10 | Block replication flags |
| Platform constraints | 6/10 | Complex for constrained devices |

**Total: 61/80**

---

## Alternative C: Hybrid (Flat P0, Block-Based P4+)

### Description

Use flat envelope for P0-P2 (constrained transports: LoRa, BLE) and block-based structure for P3-P7 (unconstrained: Wi-Fi, Internet). The protocol version field indicates which format is used. A translation function converts between formats at transport boundaries.

### Structure

**For P0-P2 (constrained):**
```
Same as Alternative A (flat envelope, abbreviated for P0)
```

**For P3-P7 (unconstrained):**
```
Same as Alternative B (BPv7-inspired block structure)
```

**Translation at gateway nodes:**
```
P0 flat → P3 block: Parse flat map, construct equivalent block array
P3 block → P0 flat: Extract fields from blocks, build flat map (if size permits)
```

### P0 Size Budget

Same as Alternative A: ~186 bytes fixed + 69 bytes payload → FITS ✓

### P4+ Size Budget

Same as Alternative B: ~120-150 bytes overhead + up to 64KB payload

### Pros

1. **Optimal P0 fit**: Flat format maximizes constrained transport efficiency
2. **Future extensibility**: Block format for unconstrained transports
3. **BPv7 interop at gateways**: P3+ messages can interface with BPv7
4. **Graceful degradation**: Emergency messages always use minimal format
5. **Best of both worlds**: Efficiency where needed, extensibility where possible

### Cons

1. **Two protocol variants**: More complex specification
2. **Translation overhead**: Gateway nodes must convert between formats
3. **Testing burden**: Two code paths to implement and test
4. **Potential inconsistency**: Subtle semantic differences between formats
5. **Version negotiation**: Nodes must agree on format based on priority
6. **Documentation complexity**: Two formats to document and maintain

### Score

| Criterion | Score | Notes |
|-----------|-------|-------|
| P0 LoRa fit | 10/10 | Flat format for constrained |
| Extensibility | 8/10 | Block format for unconstrained |
| Interoperability | 7/10 | Gateway translation to BPv7 |
| Implementation simplicity | 6/10 | Two code paths |
| Wire efficiency | 9/10 | Optimal per transport class |
| Security model | 8/10 | Clear boundaries per format |
| Fragmentation support | 8/10 | Block format handles complex cases |
| Platform constraints | 9/10 | Adapted to each platform |

**Total: 65/80**

---

## Comparative Summary

| Criterion (weight) | A: Flat | B: BPv7 Blocks | C: Hybrid |
|-------------------|---------|----------------|-----------|
| P0 LoRa fit (CRITICAL) | 10 | 6 | 10 |
| Extensibility (HIGH) | 5 | 10 | 8 |
| Interoperability (MEDIUM) | 3 | 10 | 7 |
| Simplicity (HIGH) | 9 | 5 | 6 |
| Wire efficiency (HIGH) | 9 | 5 | 9 |
| Security model (CRITICAL) | 7 | 9 | 8 |
| Fragmentation (HIGH) | 6 | 10 | 8 |
| Platform constraints (HIGH) | 9 | 6 | 9 |
| **TOTAL** | **58** | **61** | **65** |

---

## Self-Critique

### Alternative A (Flat) Weaknesses

1. **Extensibility ceiling**: Adding new fields requires version negotiation. In a disaster scenario with mixed-version nodes, this could cause interoperability failures.
2. **No per-block security**: If a relay needs to add routing information, it cannot do so without breaking the signature. The `routing_hints` field is explicitly untrusted.
3. **BPv7 gateway complexity**: Translating from flat to BPv7 blocks requires mapping all fields, which is error-prone.

### Alternative B (BPv7) Weaknesses

1. **P0 budget risk**: With 5+ blocks, overhead could exceed 150 bytes, leaving only ~100 bytes for payload. This is tight for GPS + text + encryption header.
2. **EID addressing overhead**: BPv7's `dtn://` or `ipn:` EIDs are variable-length and often 20-50 bytes. IRIS's 32-byte fixed addresses are more compact.
3. **Implementation complexity on smartphones**: Android/iOS BLE stacks are not designed for block-based protocols. Additional parsing layer adds latency and battery cost.

### Alternative C (Hybrid) Weaknesses

1. **Two protocols to maintain**: This is the biggest risk. Subtle differences between flat and block formats could cause bugs.
2. **Translation state**: Gateway nodes must maintain state for translation, adding memory and processing overhead.
3. **Testing matrix doubles**: Every feature must be tested in both formats.

### Disaster Scenario Analysis

**Scenario: Mixed-version network after earthquake**
- Alternative A: Old nodes reject new fields → version mismatch → messages dropped
- Alternative B: Old nodes ignore unknown blocks → graceful degradation
- Alternative C: P0 messages always use flat → old nodes can always handle emergencies

**Scenario: Maximum-range LoRa P0 (SF12, 255 bytes)**
- Alternative A: 69 bytes payload → sufficient for GPS + short text
- Alternative B: ~100 bytes payload → sufficient but tight with encryption header
- Alternative C: Same as A for P0 → sufficient

**Scenario: Future transport (e.g., Starlink Direct-to-Cell)**
- Alternative A: Must add new fields via version bump
- Alternative B: Add new block type → automatic compatibility
- Alternative C: Use block format for new transport → compatible

---

## Preliminary Recommendation

**Alternative C (Hybrid)** scores highest but carries implementation complexity risk.

**Alternative A (Flat)** is simplest and meets all critical requirements (P0 fit, security, platform constraints) but has extensibility limitations.

**Decision**: Propose **Alternative A (Flat Envelope)** as the v1 protocol, with a defined extension mechanism (new fields added at end of map, unknown fields ignored by old nodes) that provides adequate extensibility without the complexity of two format variants. BPv7 interoperability will be handled by a separate gateway adapter (tracked as a future node), not by native protocol adoption.

**Rationale**:
1. IRIS v1 targets smartphones, not deep-space probes. The extensibility requirements are modest.
2. P0 LoRa fit is CRITICAL and flat envelope is optimal.
3. Implementation simplicity reduces bugs in safety-critical emergency communication.
4. BPv7 interop can be added later via gateway without changing the core protocol.
5. The extension mechanism (append-only new fields, unknown-field tolerance) provides adequate forward compatibility for v1.

This recommendation will be formalized as ADR-0011 pending review.
