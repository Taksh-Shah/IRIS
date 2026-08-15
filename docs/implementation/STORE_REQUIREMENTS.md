# STORE-001 Requirements Specification

**Document ID**: IRIS-STORE-REQ-001
**Version**: 1.0
**Node**: STORE-001
**Date**: 2026-08-11

---

## Functional Requirements

### FR-001: Message Persistence
The storage layer MUST persist all P0-P4 messages immediately on creation or receipt using ACID transactions.

### FR-002: Priority-Based Eviction
When storage exceeds 80% quota, evict in order: expired → P7 → P6 → P5 → P4 → P3 → P2. P0 and P1 are NEVER evicted.

### FR-003: TTL Enforcement
Expired messages MUST be removed within 60 seconds of expiry. TTL enforcement MUST account for 300-second clock skew budget.

### FR-004: Deduplication
The storage layer MUST prevent duplicate message storage via INSERT OR IGNORE on message_id.

### FR-005: Encrypted Storage
The database file MUST be encrypted at rest using SQLCipher (AES-256-CBC with HMAC-SHA512).

### FR-006: WAL Mode
SQLite MUST operate in WAL (Write-Ahead Logging) mode to enable concurrent reads during writes.

### FR-007: Schema Migration
Database schema MUST be upgradable without data loss using rusqlite_migration.

### FR-008: Routing State Persistence
Contact history and delivery probability estimates MUST persist across app restarts.

### FR-009: Seen Message Tracking
Processed message IDs MUST be tracked for deduplication with automatic pruning after 144 hours.

### FR-010: Quota Enforcement
Default storage quota is 500MB. When exceeded, eviction MUST reduce usage below 80%.

## Non-Functional Requirements

### NFR-001: Performance
- Message insert: <10ms (P0-P4)
- Message lookup by ID: <5ms
- Eviction cycle: <100ms for 1000 messages
- TTL cleanup: <500ms per cycle

### NFR-002: Reliability
- WAL mode provides crash recovery
- SQLCipher provides page-level integrity
- Startup integrity check via PRAGMA integrity_check

### NFR-003: Concurrency
- Multiple concurrent readers (relay + UI + routing)
- Single writer (relay engine) with busy_timeout=5000ms
- Connection pool for async access

### NFR-004: Storage Efficiency
- BLOB payloads are application-encrypted before storage
- Indexes optimized for priority queue and TTL cleanup
- Partial indexes reduce index size

### NFR-005: Security
- Storage key derived from master key via HKDF-SHA256
- Master key in platform keystore (Android Keystore / Secure Enclave)
- Storage key never persisted to disk

## Acceptance Criteria Mapping

| Criterion | Requirement | Status |
|-----------|-------------|--------|
| SQLite WAL mode | FR-006 | ✅ Designed |
| SQLCipher encryption | FR-005 | ✅ Designed |
| Schema migrations | FR-007 | ✅ Designed |
| Priority eviction | FR-002 | ✅ Designed |
| TTL GC | FR-003 | ✅ Designed |
| Deduplication | FR-004 | ✅ Designed |
| Thread safety | NFR-003 | ⚠️ Needs pool design |
| Quota enforcement | FR-010 | ✅ Designed |
| Integrity check | NFR-002 | ⚠️ Needs explicit startup check |
| Batch insert | NFR-001 | ⚠️ Needs implementation |

## Gaps Requiring Design Attention

1. **Connection pooling strategy** — needs async pool (deadpool/bb8)
2. **Startup integrity check** — needs explicit PRAGMA integrity_check on open
3. **Batch insert API** — needed for high-throughput relay scenarios
4. **Storage compaction** — VACUUM after mass eviction
5. **Metrics hooks** — storage-specific observability
