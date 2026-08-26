//! PostgreSQL schema for STORE-001 (STORAGE.md §Complete Schema).
//!
//! Applied idempotently at connect time (`CREATE TABLE IF NOT EXISTS`). The
//! scalar columns mirror the envelope for querying/indexing (priority, status,
//! expiry, recipient); the canonical CBOR envelope is stored in `envelope_cbor`
//! so `load` returns the byte-exact original.

pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS messages (
    message_id      TEXT        NOT NULL PRIMARY KEY,       -- 16-byte UUIDv7 hex
    priority        SMALLINT    NOT NULL CHECK (priority BETWEEN 0 AND 7),
    status          TEXT        NOT NULL DEFAULT 'PENDING_SEND'
                    CHECK (status IN ('PENDING_SEND', 'IN_TRANSIT', 'DELIVERED',
                                      'ACKNOWLEDGED', 'EXPIRED', 'DELIVERY_FAILED')),
    created_at      BIGINT      NOT NULL,                   -- Unix timestamp s
    expires_at      BIGINT      NOT NULL,                   -- Unix timestamp s
    hop_count       SMALLINT    NOT NULL DEFAULT 0,
    max_hops        SMALLINT,
    payload_size    BIGINT      NOT NULL,                   -- Sealed envelope payload size (ciphertext bytes) for at-rest-sealed rows (RED-0012); unsealed rows store plaintext size
    envelope_cbor   BYTEA       NOT NULL,                   -- canonical CBOR (codec::encode)
    is_own_message  BOOLEAN     NOT NULL DEFAULT FALSE      -- sent by this node
);

CREATE INDEX IF NOT EXISTS idx_messages_expires ON messages (expires_at);
CREATE INDEX IF NOT EXISTS idx_messages_priority ON messages (status, priority, created_at);
"#;

/// TAK-11: the original partial indexes carried WHERE predicates their
/// consumers never satisfy (`delete_expired` has no status predicate;
/// `get_queue` matches two statuses, wider than `= 'PENDING_SEND'`), so the
/// planner provably could not use either and every GC tick and queue read
/// fell back to a sequential scan. Existing databases get the swap via this
/// idempotent migration; fresh databases get the corrected definitions in
/// DDL directly.
pub const MIGRATE_FIX_PARTIAL_INDEXES: &str = r#"
DROP INDEX IF EXISTS idx_messages_expires;
DROP INDEX IF EXISTS idx_messages_priority;
CREATE INDEX IF NOT EXISTS idx_messages_expires ON messages (expires_at);
CREATE INDEX IF NOT EXISTS idx_messages_priority ON messages (status, priority, created_at);
"#;

/// TAK-9 defence-in-depth: reject negative BIGINT scalars at the database
/// layer too (peer-chosen u64 timestamps/sizes must never land as negatives
/// even if an application-side check is bypassed). Idempotent via named
/// constraint + pg_constraint probe (PG has no ADD CONSTRAINT IF NOT EXISTS).
pub const MIGRATE_NONNEGATIVE_CONSTRAINT: &str = r#"
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'messages_scalars_nonnegative'
    ) THEN
        ALTER TABLE messages ADD CONSTRAINT messages_scalars_nonnegative
            CHECK (created_at >= 0 AND expires_at >= 0 AND payload_size >= 0);
    END IF;
END
$$;
"#;

/// CRYPTO-001 at-rest hardening for databases created before the gate
/// (v1 schema carried plaintext `sender_id`/`recipient_id` + a recipient
/// index). Idempotent: drops the plaintext identity columns/index when
/// present so recipient/sender metadata is never inferable at rest, without
/// touching sealed rows.
pub const MIGRATE_DROP_PLAINTEXT_IDENTITY: &str = r#"
ALTER TABLE messages DROP COLUMN IF EXISTS sender_id;
ALTER TABLE messages DROP COLUMN IF EXISTS recipient_id;
DROP INDEX IF EXISTS idx_messages_recipient;
"#;
