# IRIS Storage Design

> **2026-08-13 (DEC-0002)**: Storage engine is **PostgreSQL** (operator-directed;
> supersedes the earlier SQLite/SQLCipher design below). All storage logic is
> contained in the `iris-storage` crate — no SQL appears in `iris-core`.

## Storage Engine

IRIS uses **PostgreSQL** as the primary storage engine, accessed via the
`tokio-postgres` crate in Rust core (workspace dependency). The
`MessageStorage` seam (defined in `iris-core` / MSG-001) is implemented by
`PgStorage` in `crates/iris-storage`.

Connection parameters (host, port, user, dbname) default to
`127.0.0.1:5432` / `postgres`; the password is read from `IRIS_PG_PASSWORD`
(never committed). See `PgStorageConfig::from_env`.

```rust
let store = PgStorage::connect_from_env().await?;
```

### In-Transit Encryption

Dev/loopback connections use `NoTls`. Any non-loopback deployment MUST configure
TLS (`MakeTlsConnector`) — tracked as a follow-up in DEC-0002.

### At-Rest Encryption (CRYPTO-001 — implemented)

At-rest row sealing landed with the CRYPTO-001 gate (2026-08-15). Rows are
sealed with **ChaCha20-Poly1305** under a key derived from the node master key:

```
storage_key = HKDF-SHA256(salt = b"iris-at-rest-v1", IKM = node_master_key).expand(&[], 32)
```

Sealing is opt-in via `PgStorage::with_sealer(Arc<dyn RowSealer>)`:

- `NoSealer` (default) — stores plaintext canonical CBOR (pre-gate behavior).
- `StorageKeySealer::from_master_key(&master)` — AEAD seal of `envelope_cbor`.

**Sealed blob format**: `nonce(12) ‖ ciphertext ‖ tag(16)` with a fresh random
nonce per row (RFC 8439 — equal rows never collide). The AEAD tag binds
**AAD = `message_id ‖ priority ‖ expires_at`** (`row_aad` in `src/seal.rs`),
so out-of-band metadata tamper (row swap, priority/expiry rewrite, wrong key)
is detected on read. Read paths that see an opaque blob fail loudly
(`StorageError::DecryptionFailed`) rather than returning garbage.

**Schema (v2)**: plaintext identity columns (`sender_id`, `recipient_id`) and
`idx_messages_recipient` are **dropped** — recipient/sender metadata is not
inferable at rest. `MIGRATE_DROP_PLAINTEXT_IDENTITY` is applied idempotently
at connect time for databases created before the gate. Index columns retained
for lifecycle/queue queries: `message_id`, `priority`, `status`, `created_at`,
`expires_at`, `hop_count`, `max_hops`, `payload_size`, `is_own_message`.

The node master key material is provisioned by IDENT-001 (TEE/keystore);
v1 tests/dev generate in-memory master keys.

## Schema Management

Schema DDL is idempotent (`CREATE TABLE IF NOT EXISTS`) and applied at connect
time (`src/schema.rs`). Future schema changes ship as additional statements in
the same module (or a migration table when the schema stabilizes).

## Complete Schema

### `messages` Table

Primary message store. Full-fidelity persistence keeps the canonical CBOR
envelope (`codec::encode`) in `envelope_cbor` (sealed per CRYPTO-001 when a
`RowSealer` is configured); scalar columns mirror the envelope for querying/
indexing (priority, status, expiry). Plaintext identity columns were dropped
(see At-Rest Encryption above).

```sql
CREATE TABLE IF NOT EXISTS messages (
    message_id      TEXT        NOT NULL PRIMARY KEY,   -- 16-byte UUIDv7 hex
    priority        SMALLINT    NOT NULL CHECK (priority BETWEEN 0 AND 7),
    status          TEXT        NOT NULL DEFAULT 'PENDING_SEND'
                    CHECK (status IN ('PENDING_SEND', 'IN_TRANSIT', 'DELIVERED',
                                      'ACKNOWLEDGED', 'EXPIRED', 'DELIVERY_FAILED')),
    created_at      BIGINT      NOT NULL,                -- Unix timestamp s
    expires_at      BIGINT      NOT NULL,                -- Unix timestamp s
    hop_count       SMALLINT    NOT NULL DEFAULT 0,
    max_hops        SMALLINT,
    payload_size    BIGINT      NOT NULL,                -- Unencrypted payload size (bytes)
    envelope_cbor   BYTEA       NOT NULL,                -- sealed or canonical CBOR envelope
    is_own_message  BOOLEAN     NOT NULL DEFAULT FALSE   -- sent by this node
);

CREATE INDEX IF NOT EXISTS idx_messages_expires ON messages (expires_at)
    WHERE status NOT IN ('DELIVERED', 'ACKNOWLEDGED', 'EXPIRED');
CREATE INDEX IF NOT EXISTS idx_messages_priority ON messages (priority, created_at)
    WHERE status = 'PENDING_SEND';
```

Other tables designed for the system (`routing_events`, `delivery_probabilities`,
`seen_messages`, `neighbors`, `contacts`, `metrics`) land with their owning
nodes (ROUTE-001, DISCO-001, IDENT-001, OBSERVABILITY).

## Storage Quota and Eviction

### Quota

Default storage budget: **500 MB** (`IRIS_MAX_STORAGE_BYTES`). A write that
would exceed quota returns `StorageError::StorageFull` for non-P0 messages;
**P0 messages are always accepted** (INV-EMERG-001 spirit, INV-ROUTE-003).

The eviction trigger fraction is tunable via `IRIS_EVICTION_THRESHOLD`
(default **0.8**, clamped to `0.1..=0.95` — out-of-range or unparseable values
fall back to the default so eviction can never be disabled by misconfiguration).

### Eviction Policy

Eviction is triggered when usage exceeds `quota * eviction_threshold` (default
0.8). `gc_once`:
1. Reclaims all expired rows regardless of priority (`expires_at < now`).
2. If still over threshold, evicts one row at a time — lowest priority first,
   then own-message preference, then soonest expiry — until usage is at/below
   target.

**Priority-based rules** (enforced by `priority > 0` guards):
- P0 messages: **never evicted** unless expired (INV-ROUTE-003)
- P1–P2: evicted only after all lower priorities are removed
- P3–P7: evicted soonest-expiry first
- Own messages: lower eviction priority than relayed messages

## Message Persistence Guarantees

All priority levels P0–P7 are persisted immediately on receipt via the engine's
`persist` call. Deduplication at the storage layer uses
`INSERT ... ON CONFLICT (message_id) DO NOTHING` (secondary to the Bloom-filter
check in MSG-001).
