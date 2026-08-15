# BLOCKERS.md — Active Blockers

**Schema version**: 1.0
**Last updated**: 2026-08-11T19:30:00Z

---

## Active Blockers

### BLK-0001: Cryptographic Architecture Unresolved
**Severity**: CRITICAL
**Blocks**: CRYPTO-001, IDENT-001, SEC-001, EMERG-001
**Type**: HUMAN_APPROVAL_REQUIRED
**Description**: Three open crypto architecture issues from ADR-0006:
1. Per-session vs per-message ephemeral key inconsistency
2. HKDF-BLAKE3 is a non-standard construction
3. Forward secrecy properties need clarification
**Resolution Path**: Human cryptographic architecture review
**Escalation**: ESCALATION-CRYPTO-001-20260811.md

### BLK-0002: Graph Size Insufficient
**Severity**: MEDIUM
**Blocks**: Full system coverage
**Type**: SCOPE_GAP
**Description**: 32 nodes insufficient for full system coverage; need 50+ additional nodes for protocol conformance, platform details, security properties, CI/CD, supply chain, product features
**Resolution Path**: Add nodes incrementally as work progresses
**Impact**: Not blocking current work on PROTO-001

### BLK-0003: Supply Chain Security Missing
**Severity**: MEDIUM
**Blocks**: Production deployment
**Type**: SCOPE_GAP
**Description**: No graph branch for SBOM, dependency pinning, artifact signing
**Resolution Path**: Add supply chain security nodes in future phase
**Impact**: Not blocking current work

### BLK-0004: Protocol Conformance Testing Missing
**Severity**: LOW
**Blocks**: v1.0 release
**Type**: SCOPE_GAP
**Description**: No PROTOCOL_CONFORMANCE node for interoperability testing
**Resolution Path**: Add node when multiple implementations exist
**Impact**: Not blocking current work

### BLK-0005: BLE-001 Device Tests + Battery Measurement Pending Hardware
**Severity**: MEDIUM (downgraded from HIGH — phase transition resolved)
**Blocks**: BLE-001 ACCEPT, BLE-002, WIFIAWARE-001, WIFIDIRECT-001 (device-tier acceptance)
**Type**: RESOURCE (hardware)
**Description**: Phase transition to CORE_PROTOCOL_IMPLEMENTATION was AUTHORIZED 2026-08-12 (Option A). Rust workspace scaffolded; Transport trait + TransportManager + INTERNET-001 implemented (27/27 tests). BLE-001 adapter scaffold (`crates/iris-core/src/transport/ble.rs`) in place. What remains: physical Android device connect/send/receive test (2+ devices), battery measurement (EXP-003/GAP-003), and Android FFI adapter (JNI) — all require physical hardware.
**Resolution Path**: Physical Android devices provided; Android platform crate implemented
**Impact**: Blocks first MVP demo device path (BLE-001, ANDROID-001)

---

## Resolved Blockers

None yet.

---

## Blocker Record Format

```
BLK-XXXX: [Title]
Severity: CRITICAL | HIGH | MEDIUM | LOW
Blocks: [Node IDs]
Type: HUMAN_APPROVAL_REQUIRED | DEPENDENCY | SCOPE_GAP | RESOURCE | EXTERNAL
Description: [What is blocking]
Resolution Path: [How to unblock]
Escalation: [Link to escalation doc if any]
```
