# Protocol Versioning

## Purpose
Defines how IRIS protocol versions are identified, negotiated between nodes, and how backward compatibility is managed across software updates.

## Scope
Wire protocol versioning, negotiation handshake, backward compatibility policy.

---

## Version Scheme

IRIS protocol uses a two-part version:

```
protocol_version = (major: u8, minor: u8)
```

- **Major version change**: incompatible wire format change — nodes on different major versions cannot communicate
- **Minor version change**: additive/backward-compatible change — new optional fields, new message types

Current version: `(1, 0)`

---

## Version Field in Messages

Every message envelope includes the protocol version:

```
{
  0: msg_id,
  1: protocol_version,   ; [major, minor] as CBOR array
  ...
}
```

Relay nodes check `major` only. Recipient devices check both.

---

## Version Negotiation Handshake

During transport handshake, both nodes announce their supported version range:

```
Hello {
    node_id: bytes,
    min_version: [major, minor],
    max_version: [major, minor],
    capabilities: [u8],      ; bitmask of optional feature flags
}
```

Negotiated version = `min(max_version_A.major, max_version_B.major)` for major, and `min(max_minor)` for minor within that major.

If no overlapping major version, connection is refused:
```
HelloReject { reason: VersionMismatch, our_range: [...], their_range: [...] }
```

---

## Backward Compatibility Rules

### Minor version compatibility (v1.0 ↔ v1.1)

- v1.1 MUST be able to read v1.0 messages
- v1.1 may add new CBOR fields with new keys
- Unknown CBOR keys from a newer version MUST be ignored by older nodes
- v1.0 may drop unknown keys when forwarding (stripping)

### Major version migration (v1.x → v2.x)

- Both versions run simultaneously during transition period (minimum 6 months)
- Nodes advertise support for both in Hello
- Gateways act as translators between major versions during transition

---

## Protocol Feature Flags

Optional capabilities advertised in Hello:

| Bit | Feature              | Description                         |
|-----|----------------------|-------------------------------------|
| 0   | FRAGMENTATION        | Supports fragment protocol          |
| 1   | CUSTODY_ACK          | Supports custody acknowledgements   |
| 2   | BLOOM_SYNC           | Supports Bloom filter sync exchange |
| 3   | MULTIPATH            | Supports simultaneous multi-transport|
| 4   | ML_ROUTING           | Has ML routing model loaded         |
| 5   | AUTHORITY_CERT       | Has verified authority certificate  |

Nodes MUST NOT use a feature the peer hasn't advertised support for.

---

## Deprecation Policy

1. Feature deprecation is announced as minor version bump
2. Feature is marked deprecated for 2 minor versions
3. Feature is removed in next major version

No features have been deprecated as of v1.0.

---

## Version Compatibility Matrix

| v1.0 ↔ v1.0 | Full compatibility |
| v1.0 ↔ v1.1 | v1.0 ignores new fields |
| v1.0 ↔ v2.0 | Incompatible — gateway translation required |
| v1.x ↔ v2.x | Incompatible — only gateway-mediated |

---

## Known Limitations

- In a fully partitioned network with mixed versions, nodes cannot negotiate. Each partition operates at its own version and merges data when connectivity restores — version translation must happen at gateways.
- CBOR field key stripping (v1.1 message forwarded by v1.0 relay) may lose information. Forward-compatible design must ensure no v1.1 field is required for correct delivery.

---

## References

- `docs/protocol/MESSAGE_ENVELOPE.md` — `protocol_version` field
- `docs/protocol/COMPATIBILITY.md` — Compatibility testing procedures
