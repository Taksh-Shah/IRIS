# Protocol Compatibility

## Purpose
Defines compatibility testing procedures and policies to ensure IRIS nodes of different versions can interoperate during a rolling upgrade.

## Scope
Wire protocol compatibility between IRIS versions. Does NOT cover OS/platform compatibility (see platform docs).

---

## Compatibility Matrix

| From \ To | v1.0 | v1.1 | v2.0 |
|-----------|------|------|------|
| v1.0      | Full | Full (v1.0 ignores new fields) | Gateway translation only |
| v1.1      | Full | Full | Gateway translation only |
| v2.0      | Gateway translation only | Gateway translation only | Full |

"Full" means two nodes can exchange messages without any translation layer.
"Gateway translation only" means a gateway acting as protocol bridge is required.

---

## Compatibility Test Suite

### Suite 1: Same-Version Baseline

```
TC-COMPAT-001: v1.0 ↔ v1.0 full message exchange
TC-COMPAT-002: v1.0 message round-trip encode/decode
TC-COMPAT-003: All 8 priority levels exchanged successfully
TC-COMPAT-004: All delivery policies exercised
TC-COMPAT-005: Fragment + reassembly cross-version
```

### Suite 2: Minor Version Compatibility (v1.0 ↔ v1.1)

```
TC-COMPAT-101: v1.1 node sends message with new optional field to v1.0 node
               Expected: v1.0 node accepts message, ignores unknown field
TC-COMPAT-102: v1.0 node sends message to v1.1 node
               Expected: v1.1 accepts, treats missing optional field as absent
TC-COMPAT-103: v1.0 relay forwards v1.1 message (may strip unknown fields)
               Expected: core fields preserved, v1.1 optional fields may be lost
TC-COMPAT-104: Capability negotiation with version mismatch
               Expected: nodes agree on intersection of capabilities
TC-COMPAT-105: v1.1 feature flag not advertised by v1.0 peer
               Expected: v1.1 node does not use unavailable feature
```

### Suite 3: Major Version Incompatibility (v1.x ↔ v2.x)

```
TC-COMPAT-201: v1.0 node receives v2.0 Hello
               Expected: HelloReject with VersionMismatch reason
TC-COMPAT-202: Gateway translates v1.0 → v2.0 message
               Expected: all core fields preserved, v2.0-only fields synthesized
TC-COMPAT-203: Gateway translates v2.0 → v1.0 message
               Expected: v2.0-only fields stripped, v1.0 receives valid message
TC-COMPAT-204: E2E signature preserved through gateway translation
               Expected: recipient can still verify originator signature
```

---

## Field Forward Compatibility Rules

When a v1.0 node receives a CBOR map with unknown integer keys:
1. **MUST NOT reject the message** due to unknown keys
2. **MUST preserve unknown keys when forwarding** (relay passthrough)
3. **MUST NOT attempt to interpret** unknown keys

When a v1.0 node generates a CBOR map:
1. **MUST NOT generate keys not in v1.0 spec**
2. Keys 100-199 are **reserved for future minor versions** — never use experimentally

```rust
fn decode_message(cbor: &[u8]) -> Result<Message, DecodeError> {
    let map: BTreeMap<u8, CborValue> = cbor::decode(cbor)?;

    let msg = Message {
        msg_id: map.get_bytes(0)?,
        protocol_version: map.get_array(1)?,
        // ... other known fields ...
    };

    // Preserve unknown keys for passthrough
    msg.unknown_fields = map.into_iter()
        .filter(|(k, _)| !KNOWN_KEYS.contains(k))
        .collect();

    Ok(msg)
}
```

---

## Upgrade Path

### Rolling Upgrade Procedure

When upgrading a deployed network from v1.0 to v1.1:

1. **Do not force-upgrade all nodes simultaneously.** Upgrade gateways first.
2. Gateways run both versions in parallel during transition period.
3. Mobile nodes upgrade via app store at their own pace.
4. Edge servers upgrade manually (field teams).
5. After >95% nodes on v1.1: deprecate v1.0 support (minimum 6 months).

### Major Version Transition (v1.x → v2.x)

1. Deploy gateway nodes with translation capability first.
2. Announce v2.0 transition in-app: "Network upgrade available."
3. Both versions coexist for minimum 12 months.
4. v1.x nodes that cannot upgrade remain operational (translation handles it).

---

## Compatibility Testing in CI

Compatibility tests run in CI on every protocol change:

```yaml
# .github/workflows/compat.yml (example)
matrix:
  from_version: [v1.0, v1.1]
  to_version: [v1.0, v1.1]
  message_type: [SOS, Medical, Location, Text]
  delivery_policy: [BEST_EFFORT, CONFIRMED, EMERGENCY_BROADCAST]
```

Tests use a simulated two-node network with version pinning via Docker images.

---

## Known Compatibility Risks

- **CBOR strict mode**: some CBOR parsers reject maps with unknown keys. IRIS parsers MUST use lenient mode.
- **Field stripping at v1.0 relay**: if v1.1 adds a required-for-v1.1 field, v1.0 relay strips it, and v1.1 recipient sees missing field. Design rule: v1.1 fields must be optional or have v1.0-compatible defaults.
- **Signature coverage**: if v1.1 adds a new signed field and v1.0 relay strips it, the v1.1 recipient's signature verification fails. Design rule: new fields in minor versions must NOT be included in the signed payload.

---

## References

- `docs/protocol/VERSIONING.md` — Version numbering and negotiation
- `docs/protocol/MESSAGE_ENVELOPE.md` — CBOR field map and known keys
- RFC 8949 §5 — CBOR well-formedness and decoding rules
