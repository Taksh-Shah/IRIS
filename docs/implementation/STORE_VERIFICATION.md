# STORE-001 Verification Report

**Document ID**: IRIS-STORE-VER-001
**Version**: 1.1
**Node**: STORE-001
**Date**: 2026-08-11 (v1.0 design) / 2026-08-13 (v1.1 implementation, PostgreSQL)

---

## Acceptance Criteria Verification

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | Database schema migrations applied idempotently | `src/schema.rs` — `CREATE TABLE IF NOT EXISTS` + partial indexes; applied at connect (`batch_execute`) | ✅ PASS |
| 2 | At-rest encryption integrated | Deferred to CRYPTO-001 gate (BLK-0001); DEC-0002 documents the PostgreSQL pivot and debt | ⚠️ DEFERRED (BLK-0001) |
| 3 | Priority-based eviction (P0 never evicted) | `src/eviction.rs` — `priority > 0` guard, one-row-at-a-time until target; test `evict_by_priority_never_touches_p0`, `evict_by_priority_removes_lowest_priority_first` | ✅ PASS |
| 4 | TTL garbage collection (every 60s) | `src/gc.rs` — 60 s background task; `evict_expired` reclaims expired rows (incl. expired P0) | ✅ PASS |
| 5 | Deduplication at storage layer | `INSERT ... ON CONFLICT (message_id) DO NOTHING`; test `insert_or_ignore_dedups_at_storage_layer` | ✅ PASS |
| 6 | Quota enforcement (500 MB default) | `PgStorageConfig.max_storage_bytes` (env `IRIS_MAX_STORAGE_BYTES`); StorageFull for non-P0 over quota, P0 always accepted; test `quota_rejects_non_p0_but_accepts_p0` | ✅ PASS |
| 7 | Byte-exact envelope persistence | `envelope_cbor BYTEA` via canonical CBOR codec; test `persist_load_roundtrip` | ✅ PASS |
| 8 | Queue ordering P0-first | `get_queue` ORDER BY priority ASC; test `get_queue_orders_p0_first_and_filters_terminal` | ✅ PASS |
| 9 | MessageEngine integration (M3) | `tests/m3_engine.rs` — MessageEngine over PgStorage across SimulatedTransport round-trip delivers | ✅ PASS |

## Implementation Test Evidence (2026-08-13)

- **`cargo test -p iris-storage`**: 10/10 store tests + 1/1 M3 integration (DB-backed, `IRIS_PG_PASSWORD` from env)
- **`cargo test --workspace`**: green (iris-core 94/94 + iris-storage)
- **`cargo clippy --workspace --all-targets`**: 0 warnings

## Known Limitations (recorded in PROJECT_GRAPH.yaml + DEC-0002)

- In-transit TLS: dev uses `NoTls` on loopback; per-deployment TLS required
- At-rest encryption + storage key derivation deferred to CRYPTO-001 (BLK-0001)
- Single shared connection; pooling/reconnect planned for deployment harness
- Additional tables (routing_events, neighbors, contacts, metrics) land with
  their owning nodes

## Verdict: IMPLEMENTED

All core criteria pass with implementation evidence. Encryption is gated behind
BLK-0001 (CRYPTO-001 human gate) and recorded as a known limitation, not
silently dropped.
