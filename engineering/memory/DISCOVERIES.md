# DISCOVERIES.md — Discovered Facts

**Schema version**: 1.0
**Last updated**: 2026-08-15T20:05:00Z

---

## Discoveries

### DISC-0013: Doc-vs-code drift — SYBIL_RESISTANCE/DOS_RESISTANCE claim implemented defenses that do not exist
**Date**: 2026-08-15
**Node**: SEC-001
**Evidence maturity**: IMPLEMENTED — verified by glob + grep of crates/iris-core
**Description**: `docs/security/SYBIL_RESISTANCE.md` marks Defense 3 (Rate Limiting), Defense 4 (Anomaly Detection, "Implemented, L2 Intelligence"), and Defense 5 (Reputation System, "Partially Implemented") as implemented, but **no general per-message-class rate limiter, reputation store, or Sybil detector exists in `crates/iris-core`** — the only rate/protection machinery is emergency-scoped (SosRateLimiter, BroadcastReplayGuard in `emergency/`). `docs/security/DOS_RESISTANCE.md:213` states "All DoS defenses implemented in `crates/iris-core/src/node/dos_protection.rs`" — **that path does not exist** (`crates/iris-core/src/node/` is absent). What IS implemented for abuse resistance: message dedup Bloom + exact LRU (`message_engine/dedup.rs`), routing `ForwardedCache` (`routing/dedup_cache.rs`), trust_store replay counters (`identity/rotate.rs` earliest-seen-wins stale/equal/higher reject), recipient-side replay suppression in EmergencyGateway. SEC-001 must either implement the claimed defenses generically or correct the docs (C-pattern doc reconciliation).
**Source**: glob `crates/iris-core/src/node/**` (empty); grep for dos_protection/sybil_detect/reputation (no matches); docs/security SYBIL_RESISTANCE.md:82-185, DOS_RESISTANCE.md:213; emergency/rate_limit.rs + provider.rs real implementations
**Impact**: SEC-001 scope = generalize DoS/replay/Sybil/spam across all message classes + reconcile claimed-vs-implemented docs; documents currently overstate the security posture.

### DISC-0011: Cell-Broadcast alert systems (EU-Alert/WEA) provide NO UE authentication
**Date**: 2026-08-15
**Node**: EMERG-001
**Evidence maturity**: RESEARCHED — ETSI TS 102 900 V1.4.1 §5.5 (Level 1) + WEA spoofing research (CACM 2023, ACM 2024, arXiv 2604.24404)
**Description**: ETSI TS 102 900 (EU-ALERT) §5.5 explicitly states Cell Broadcast provides no subscriber/UE authentication. Academic work (CACM 2023 "Securing the Wireless Emergency Alerts System": Ed25519 + SIM-provisioned key, ~68 B overhead; ACM 2024 Broadcast But Verify: 3.220 ms LTE verification; arXiv 2604.24404: 5G SA spoofing testbed) shows fake-alert attacks persist and the standard mitigation is offline-verifiable Ed25519 with a pre-provisioned trust anchor. This validates IRIS's offline preloaded-root + SPKI key-anchored authority chain (IDENT-001 RED-0008 reuse) as the correct pattern for a mesh where no SIM/identity provider exists.
**Source**: ETSI TS 102 900 V1.4.1; CACM 2023; ACM 2024; arXiv 2604.24404; RES-0017 §2
**Impact**: Authority-broadcast design (DEC-EMERG-0002) relies on offline chain verification, never transport/channel authentication; mandatory relay applies only to verified broadcasts.

### DISC-0012: BNS 2023 replaced IPC effective 2024-07-01 — EMERGENCY_ABUSE.md legal references stale
**Date**: 2026-08-15
**Node**: EMERG-001
**Evidence maturity**: RESEARCHED — BNS 2023 (Act 45 of 2023) commencement + secondary mappings (see Gap G3 for unverified §153A)
**Description**: The Bharatiya Nyaya Sanhita 2023 replaced the Indian Penal Code effective 2024-07-01. Verified mappings found: IPC §420 (cheating) → BNS §318; IPC §505 (circulating false/rumor) → BNS §353(2). docs/security/EMERGENCY_ABUSE.md cites IPC-era sections — those references are stale and must migrate to BNS before EMERG-001 VERIFY. IPC §153A → BNS mapping not yet verified (LEGAL-001 hook).
**Source**: BNS 2023 (indiacode.nic.in / mha.gov.in); secondary legal mappings; RES-0017 §7
**Impact**: DEC-EMERG-0007 — update EMERGENCY_ABUSE.md legal basis; do not ship VERIFY with IPC references.

### DISC-0010: 2026 RUSTSEC crypto advisory family is libcrux (Cryspen), NOT the RustCrypto/dalek crates
**Date**: 2026-08-14
**Node**: CRYPTO-001
**Evidence maturity**: RESEARCHED - RUSTSEC advisory database 2026-08-14 snapshot
**Description**: The 2026 advisory wave (RUSTSEC-2026-0023 libcrux-ecdh, 0024/0025 libcrux-psq, 0026 libcrux-ed25519, 0075 libcrux-ed25519, 0124 libcrux-chacha20poly1305) targets **Cryspen's libcrux formally-verified bindings**, NOT the RustCrypto `chacha20poly1305`/`hkdf` or dalek `x25519-dalek`/`ed25519-dalek` crates IRIS selects. The advisory titles reuse the same algorithm names (e.g. "X25519", "chacha20poly1305") which invites review confusion. Lessons for IRIS regardless: (1) RFC 7748 §6.1 all-zero X25519 shared-secret check is mandatory (hpke-rs patched RUSTSEC-2026-0072 for this), (2) validate X25519 input bytes/clamping, (3) curve25519-dalek must be ≥4.1.3 (RUSTSEC-2024-0344 timing in Scalar29/52::sub).
**Source**: RUSTSEC advisory database 2026-08-14; RES-0015 §1.1
**Impact**: Future security reviews must not misattribute libcrux advisories to IRIS's crates; IRIS implements the RFC 7748 §6.1 all-zero reject + input validation itself. RUSTSEC audit result: all IRIS-selected crates clean at pin. Crate pins: x25519-dalek 2.0.1, ed25519-dalek 2.x (verify_strict only), chacha20poly1305 0.10.1, hkdf 0.12, curve25519-dalek ≥4.1.3.

### DISC-0001: Ed25519 uses SHA-512 internally (not configurable)
**Date**: 2026-08-11
**Node**: CRYPTO-001
**Evidence maturity**: RESEARCHED — RFC 8032
**Description**: Ed25519's internal hash is SHA-512 per RFC 8032. This is a fixed property of the algorithm, not configurable. BLAKE3 is used elsewhere in IRIS (NodeId derivation, payload hashing) but is NOT used inside Ed25519 signing.
**Source**: RFC 8032, ADR-0002
**Impact**: Corrects a potential misunderstanding that IRIS uses BLAKE3 inside Ed25519

### DISC-0002: HKDF-BLAKE3 is non-standard construction
**Date**: 2026-08-11
**Node**: CRYPTO-001
**Evidence maturity**: RESEARCHED — RFC 5869
**Description**: HKDF is defined over HMAC, which is defined over a hash function. HMAC-BLAKE3 is not a standardized construction. IRIS documentation says "HKDF-BLAKE3" which needs resolution.
**Source**: RFC 5869, ADR-0006
**Impact**: Blocks CRYPTO-001 until resolved

### DISC-0003: Current ECIES-like construction lacks forward secrecy
**Date**: 2026-08-11
**Node**: CRYPTO-001
**Evidence maturity**: RESEARCHED — cryptographic analysis
**Description**: The ephemeral sender X25519 + static recipient X25519 construction does NOT provide forward secrecy against recipient key compromise. If recipient's long-term X25519 private key is later compromised, all past plaintext messages can be recovered.
**Source**: ADR-0006
**Impact**: Documentation must not claim forward secrecy without qualifying it

### DISC-0004: LoRa duty cycle cannot be bypassed even for P0
**Date**: 2026-08-11
**Node**: LORA-001
**Evidence maturity**: RESEARCHED — WPC regulations
**Description**: WPC 1% duty cycle limit applies to all transmissions including emergency P0. Alternative transports must be used when LoRa budget is exhausted.
**Source**: WPC Gazette 2021, docs/legal/INDIA_COMPLIANCE.md
**Impact**: Removed "P0 ignores duty cycle" claim from LORA.md; P0 prioritized in queue but doesn't bypass regulations

### DISC-0005: India LoRa spectrum is 865–868 MHz (not 865–867)
**Date**: 2026-08-11
**Node**: LORA-001
**Evidence maturity**: RESEARCHED — WPC Gazette
**Description**: WPC Gazette 2021 expanded LoRa band to 865–868 MHz. Documentation must reflect current allocation.
**Source**: WPC Gazette 2021
**Impact**: Updated all spectrum references; flagged for validation against current Gazette before production

### DISC-0006: iOS cannot relay Wi-Fi Aware or Wi-Fi Direct
**Date**: 2026-08-11
**Node**: TRANSPORT-001
**Evidence maturity**: RESEARCHED — Apple developer documentation
**Description**: iOS does not expose Wi-Fi Aware or Wi-Fi Direct APIs. iOS devices must use BLE for discovery and Multipeer Connectivity Framework for data exchange.
**Source**: Apple developer docs, docs/architecture/LAYER_MODEL.md
**Impact**: Architecture must treat iOS as lower-capability relay; iOS background BLE severely limited

### DISC-0007: P0 SOS fits in 255 bytes with room to spare
**Date**: 2026-08-11
**Node**: PROTO-001
**Evidence maturity**: DESIGNED — byte budget analysis
**Description**: P0 envelope uses ~146 bytes fixed fields, leaving 109 bytes for payload. GPS (16 bytes) + text (90 bytes) = 106 bytes. Fits within 255-byte LoRa payload.
**Source**: docs/protocol/PROTOCOL_OVERVIEW.md
**Impact**: Confirms LoRa feasibility for emergency SOS

### DISC-0008: Measured P0 envelope = 237 B; payload capacity ceiling 84 B
**Date**: 2026-08-13
**Node**: PROTO-001 (WP-A codec)
**Evidence maturity**: IMPLEMENTED — byte-exact test vector
**Description**: The implemented canonical CDE codec measures the 13-entry P0 abbreviated SOS envelope at **237 bytes** with a 66-byte payload and 64-byte signature (fixed overhead 171 B), setting the **payload capacity ceiling at 84 bytes** for the 255-byte LoRa budget (85 B payload overflows — guard test enforces). This supersedes the earlier DESIGNED-level estimates: DISC-0007's "146 B fixed / 109 B payload" and MESSAGE_ENVELOPE.md §Size Budget Analysis's "69 B payload" were rough-draft arithmetic. The 16-byte sender UUID + 4-byte recipient + 32-byte hash + 64-byte signature dominate.
**Source**: crates/iris-core/src/protocol/codec.rs byte-exact vector `p0_sos_encodes_byte_exact_cde`
**Impact**: P0 payload budget in MESSAGE_ENVELOPE.md must be revised to 84 B. GPS (16 B) + short text (≤68 B) fits. Signature presence is the main cost driver — P0 with signature cannot carry larger payloads.

### DISC-0009: PROTOCOL_TEST_VECTORS.md timestamp hex arithmetically wrong
**Date**: 2026-08-13
**Node**: PROTO-001 (WP-A codec)
**Evidence maturity**: IMPLEMENTED — byte-exact test vector
**Description**: PROTOCOL_TEST_VECTORS.md lists the P0 SOS vector's `timestamp` hex as `0x66B5C000` for value 1723334400. The correct hex is **`0x66B7FF00`** (1723334400 = 0x66B7FF00; 0x66B5C000 = 1723187200). The codec emits the correct value; the doc hex column is arithmetically wrong. The value column (1723334400) is authoritative.
**Source**: crates/iris-core/src/protocol/codec.rs byte-exact vector; arithmetic verification
**Impact**: PROTOCOL_TEST_VECTORS.md hex column needs a one-byte correction (0xB7FF00 not 0xB5C000). No wire-format change; value field is authoritative.

---

## Discovery Record Format

```
DISC-XXXX: [Title]
Date: YYYY-MM-DD
Node: XXXX-XXX
Evidence maturity: HYPOTHESIZED | RESEARCHED | DESIGNED | IMPLEMENTED | ...
Description: [What was discovered]
Source: [Evidence source]
Impact: [What this changes]
```
