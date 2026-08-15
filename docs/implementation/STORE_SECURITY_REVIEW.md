# STORE-001 Security Review

**Document ID**: IRIS-STORE-SEC-001
**Version**: 1.0
**Node**: STORE-001
**Date**: 2026-08-11

---

## Scope

Security analysis of the IRIS storage layer design (STORAGE.md + STORE_REQUIREMENTS.md).

## Threat Model

### Assets
1. Message content (encrypted at application layer)
2. Routing metadata (contact history, delivery probabilities)
3. Identity data (contacts, public keys)
4. Storage encryption key

### Threats

#### T-001: Database File Theft
**Scenario**: Attacker gains physical access to device storage and copies the database file.
**Mitigation**: SQLCipher AES-256-CBC encryption with HMAC-SHA512 page authentication. Without the key, the file is opaque.
**Status**: ✅ Mitigated

#### T-002: Key Extraction from Memory
**Scenario**: Attacker with running device extracts storage key from process memory.
**Mitigation**: Storage key derived from master key at startup, master key in hardware-backed keystore (Android Keystore / Secure Enclave). Key exists in memory only during database operations.
**Status**: ✅ Partially mitigated (hardware-backed when available)

#### T-003: SQL Injection
**Scenario**: Malformed message data triggers SQL injection.
**Mitigation**: All queries use parameterized statements (rusqlite params![] macro). No string concatenation in SQL.
**Status**: ✅ Mitigated

#### T-004: Storage Exhaustion (DoS)
**Scenario**: Attacker floods network with messages, exhausting storage quota.
**Mitigation**: 500MB quota with priority-based eviction. P0-P1 never evicted, but P2-P7 are evicted under pressure. Rate limiting at transport layer.
**Status**: ✅ Mitigated

#### T-005: WAL File Tampering
**Scenario**: Attacker modifies WAL file to corrupt database.
**Mitigation**: SQLCipher HMAC-SHA512 authenticates each page. Tampered pages fail authentication.
**Status**: ✅ Mitigated

#### T-006: Metadata Leakage via Index Patterns
**Scenario**: Attractor infers message patterns from index access patterns.
**Mitigation**: Indexes are internal to SQLite. No external index query interface. Application queries are parameterized.
**Status**: ⚠️ Acceptable risk (requires device compromise)

## Security Requirements Verification

| Requirement | Status |
|-------------|--------|
| Encryption at rest (AES-256) | ✅ SQLCipher |
| Page-level integrity (HMAC-SHA512) | ✅ SQLCipher |
| Key derivation (HKDF-SHA256) | ✅ Documented |
| Master key in hardware keystore | ✅ Documented |
| Parameterized queries | ✅ rusqlite params![] |
| No plaintext key persistence | ✅ Documented |
| Integrity check on startup | ⚠️ Needs explicit PRAGMA |

## Finding

The storage design is **secure for v1**. The only gap is the lack of an explicit startup integrity check (PRAGMA integrity_check) after opening the database. This should be added to the MSG-001 implementation.

## Verdict: PASS

No blocking security issues. Design is sound.
