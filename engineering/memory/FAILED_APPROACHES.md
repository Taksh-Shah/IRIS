# FAILED_APPROACHES.md — Failed Approaches Log

**Schema version**: 1.0
**Last updated**: 2026-08-15T16:00:00Z

---

## Failed Approaches

### FAIL-0005: Adopting a standardized "CAP-in-CBOR" wire format
**Date**: 2026-08-15
**Node**: EMERG-001
**Attempted**: Find/use a standardized encoding of OASIS CAP alert messages in CBOR (originating research text referenced "SEP-0182")
**Reason for failure**: No such standard exists. CAP v1.2 is XML (OASIS, 2010); ITU-T X.1303/X.1303bis encode CAP in ASN.1 with XER↔PER compact binary (not CBOR, not IRIS-envelope-compatible); no RFC, IETF draft, or industry spec defines "CAP-CBOR"/"SEP-0182" (2026-08-15 search). The unresolvable reference is recorded as RES-0017 Gap G1 (same policy as RES-0016 "QEMU/PLD").
**Evidence**: OASIS CAP v1.2 (Level 1); ITU-T X.1303bis (Level 1); standards/registry search 2026-08-15 (no result)
**What was learned**: CAP is the *semantic* model for alert fields (event/severity/urgency/expires/area); the wire encoding is IRIS's own decision. Use a minimal CAP-inspired CBOR subset on the existing flat envelope (DEC-EMERG-0001). Never claim conformance to a nonexistent standard.
**Do not repeat**: Do not cite or depend on "SEP-0182" or any CAP-CBOR standard; do not import CAP-XML or ASN.1/PER into the IRIS envelope.

### FAIL-0004: HKDF-BLAKE3 key derivation
**Date**: 2026-08-14
**Node**: CRYPTO-001
**Attempted**: ADR-0006 line 98 proposed key derivation from X25519 shared secret via "HKDF-BLAKE3" (HKDF over HMAC-BLAKE3)
**Reason for failure**: Non-standard construction — HKDF (RFC 5869) is defined over HMAC, and HMAC-BLAKE3 is not an IETF-standardized MAC; no RFC vectors, no interop, no auditability. DISC-0002 flagged this; DEC-P0002 formally rejected it in the CRYPTO-001 research (RES-0015 D2).
**Evidence**: RFC 5869 (Level 1); DISC-0002; RES-0015 §3; DEC-P0002 resolution 2026-08-14
**What was learned**: Standardized HKDF-SHA256 (RFC 5869, RustCrypto `hkdf`+`sha2`, Appendix A vectors) selected. BLAKE3 remains for non-crypto hashing only (message_id, payload_hash, dedup Bloom). Future BLAKE3-derived keys, if ever needed, use BLAKE3's native `derive_key` KDF — never a homemade HMAC-BLAKE3 HKDF.
**Do not repeat**: Never invent HKDF over a non-standard HMAC; use RFC 5869 HKDF with a standardized hash or a native spec'd KDF (BLAKE3 derive_key).

### FAIL-0001: P0 SOS ignores LoRa duty cycle
**Date**: 2026-08-11
**Node**: LORA-001
**Attempted**: Allow P0 SOS messages to bypass WPC 1% duty cycle tracking
**Reason for failure**: Regulatory violation — WPC duty cycle applies to all transmissions regardless of priority
**Evidence**: WPC Gazette 2021, docs/legal/INDIA_COMPLIANCE.md
**What was learned**: Emergency priority affects queue ordering and transport selection, not regulatory compliance. When LoRa budget exhausted, use alternative transports.
**Do not repeat**: Never bypass regulatory constraints for any message priority

### FAIL-0002: Claiming IRIS engaged legal counsel
**Date**: 2026-08-11
**Node**: LEGAL-001
**Attempted**: State "IRIS has engaged qualified Indian telecommunications legal counsel" in documentation
**Reason for failure**: Fabricated fact — no legal counsel has been engaged
**Evidence**: Self-review during architecture validation
**What was learned**: Never fabricate organizational facts. State what is true: "Legal counsel review is required before deployment."
**Do not repeat**: Never fabricate evidence, qualifications, or organizational facts

### FAIL-0003: Ed25519 uses BLAKE3 internally
**Date**: 2026-08-11
**Node**: CRYPTO-001
**Attempted**: Document that Ed25519 uses BLAKE3 in IRIS's case
**Reason for failure**: Ed25519 uses SHA-512 internally per RFC 8032 — this is not configurable
**Evidence**: RFC 8032
**What was learned**: Ed25519's internal hash function is fixed. BLAKE3 is used elsewhere in IRIS (NodeId derivation) but not inside Ed25519.
**Do not repeat**: Never claim algorithm internals are configurable when they are defined by standard

---

## Failed Approach Record Format

```
FAIL-XXXX: [Title]
Date: YYYY-MM-DD
Node: XXXX-XXX
Attempted: [What was tried]
Reason for failure: [Why it failed]
Evidence: [Proof of failure]
What was learned: [Key insight]
Do not repeat: [Specific action to avoid]
```
