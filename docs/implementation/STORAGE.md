# IRIS Storage Design

## Storage Engine

IRIS uses SQLite as the primary storage engine, accessed via the `rusqlite` crate in Rust core. All storage logic is contained in the `iris-storage` crate — no SQL appears in `iris-core` or platform layers.

## Encryption

### Approach: SQLCipher

SQLCipher is an SQLite extension that encrypts the entire database file using AES-256 in CBC mode with HMAC-SHA512 page authentication. Every page is individually encrypted.

**Advantages**:
- Entire database file is opaque without the key — no metadata leakage
- Key rotation is a single `PRAGMA rekey` operation
- Transparent to all query code — normal SQLite API

**Key derivation**: SQLCipher accepts a passphrase (string) or raw key bytes. IRIS derives the storage key from the node's master key using HKDF:

```rust
use hkdf::Hkdf;
use sha2::Sha256;

fn derive_storage_key(master_key: &[u8; 32]) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut storage_key = [0u8; 32];
    hk.expand(b"iris-storage-key-v1", &mut storage_key)
        .expect("HKDF expand: valid length");
    storage_key
}
```

The master key is stored in the platform keystore (Android Keystore / Secure Enclave / macOS Keychain). The storage key is derived at startup and never persisted.

**SQLCipher setup in rusqlite**:

```rust
use rusqlite::{Connection, OpenFlags};

fn open_encrypted_db(path: &str, key: &[u8; 32]) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )?;

    // Set encryption key (must be first pragma after open)
    let key_hex = hex::encode(key);
    conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", key_hex))?;

    // Verify key is correct (will error if wrong key)
    conn.execute_batch("SELECT count(*) FROM sqlite_master;")?;

    Ok(conn)
}
```

## Schema Version Management

`rusqlite_migration` provides safe schema upgrades:

```rust
use rusqlite_migration::{Migrations, M};

fn get_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!("migrations/001_initial.sql")),
        M::up(include_str!("migrations/002_add_routing_events.sql")),
        M::up(include_str!("migrations/003_add_capabilities.sql")),
    ])
}

pub fn open_and_migrate(path: &str, key: &[u8; 32]) -> Result<Connection, StorageError> {
    let mut conn = open_encrypted_db(path, key)?;
    get_migrations().to_latest(&mut conn)?;
    configure_pragmas(&conn)?;
    Ok(conn)
}

fn configure_pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        PRAGMA cache_size = -8000;  -- 8MB cache
    ")
}
```

WAL (Write-Ahead Logging) mode: concurrent reads do not block writes. `synchronous = NORMAL` in WAL mode provides durability with better performance than `FULL`.

## Complete Schema

### `messages` Table

Primary message store. Payload is encrypted at the application layer (ChaCha20-Poly1305) before storage — SQLCipher encrypts the page, but payload is double-encrypted.

```sql
-- migrations/001_initial.sql
CREATE TABLE messages (
    message_id      TEXT        NOT NULL PRIMARY KEY,  -- UUIDv7 hex
    sender_id       BLOB        NOT NULL,               -- 32-byte NodeId
    recipient_id    BLOB        NOT NULL,               -- 32-byte NodeId (or broadcast marker)
    channel_id      BLOB,                               -- NULL for direct message
    priority        INTEGER     NOT NULL CHECK (priority BETWEEN 0 AND 7),
    status          TEXT        NOT NULL DEFAULT 'PENDING_SEND'
                    CHECK (status IN ('PENDING_SEND', 'RELAYED', 'DELIVERED', 'EXPIRED', 'FAILED')),
    created_at      INTEGER     NOT NULL,               -- Unix timestamp ms
    expires_at      INTEGER     NOT NULL,               -- Unix timestamp ms
    hop_count       INTEGER     NOT NULL DEFAULT 0,
    max_hops        INTEGER     NOT NULL DEFAULT 7,
    payload         BLOB        NOT NULL,               -- ChaCha20-Poly1305 encrypted
    payload_size    INTEGER     NOT NULL,               -- Unencrypted payload size (bytes)
    signature       BLOB        NOT NULL,               -- Ed25519 signature (64 bytes)
    ack_received    INTEGER     NOT NULL DEFAULT 0,     -- Boolean: 0/1
    ack_received_at INTEGER,                            -- Unix timestamp ms, NULL if not acked
    is_own_message  INTEGER     NOT NULL DEFAULT 0      -- Boolean: sent by this node
);

CREATE INDEX idx_messages_recipient ON messages (recipient_id, status, priority);
CREATE INDEX idx_messages_expires ON messages (expires_at) WHERE status NOT IN ('DELIVERED', 'EXPIRED');
CREATE INDEX idx_messages_priority ON messages (priority, created_at) WHERE status = 'PENDING_SEND';
CREATE INDEX idx_messages_channel ON messages (channel_id) WHERE channel_id IS NOT NULL;
```

### `routing_events` Table

Contact history for PRoPHET routing algorithm. Every contact with a peer is recorded.

```sql
CREATE TABLE routing_events (
    event_id        TEXT        NOT NULL PRIMARY KEY,   -- UUIDv7 hex
    node_id         BLOB        NOT NULL,               -- Peer NodeId (32 bytes)
    contact_time    INTEGER     NOT NULL,               -- Unix timestamp ms
    separation_time INTEGER,                            -- When contact ended (NULL if ongoing)
    direction       TEXT        NOT NULL
                    CHECK (direction IN ('INCOMING', 'OUTGOING', 'BIDIRECTIONAL')),
    transport       TEXT        NOT NULL,               -- 'ble', 'wifi_aware', 'tcp', 'lora'
    messages_exchanged INTEGER  NOT NULL DEFAULT 0
);

CREATE INDEX idx_routing_node_time ON routing_events (node_id, contact_time DESC);
```

### `delivery_probabilities` Table

Cached PRoPHET delivery probability values. Recomputed on contact; stored for fast routing decisions.

```sql
CREATE TABLE delivery_probabilities (
    destination_id  BLOB        NOT NULL,               -- Destination NodeId
    probability     REAL        NOT NULL,               -- 0.0 to 1.0
    last_updated    INTEGER     NOT NULL,               -- Unix timestamp ms
    PRIMARY KEY (destination_id)
);
```

### `seen_messages` Table

Deduplication table. Tracks message IDs this node has processed to prevent relay loops.

```sql
CREATE TABLE seen_messages (
    message_id      TEXT        NOT NULL PRIMARY KEY,   -- UUIDv7 hex
    seen_at         INTEGER     NOT NULL                -- Unix timestamp ms
);

CREATE INDEX idx_seen_at ON seen_messages (seen_at);
```

Seen messages older than `max_ttl * 2` are pruned during maintenance cycles.

### `neighbors` Table

Currently active and recently seen neighbors.

```sql
CREATE TABLE neighbors (
    node_id         BLOB        NOT NULL PRIMARY KEY,   -- 32-byte NodeId
    transport       TEXT        NOT NULL,               -- Primary transport type
    last_seen       INTEGER     NOT NULL,               -- Unix timestamp ms
    first_seen      INTEGER     NOT NULL,               -- Unix timestamp ms
    capabilities    BLOB,                               -- CBOR-encoded CapabilitySet
    rssi_last       INTEGER,                            -- Last RSSI reading (dBm)
    contact_count   INTEGER     NOT NULL DEFAULT 1
);

CREATE INDEX idx_neighbors_last_seen ON neighbors (last_seen DESC);
```

### `contacts` Table

Known identity records — persistent across app restarts.

```sql
CREATE TABLE contacts (
    node_id         BLOB        NOT NULL PRIMARY KEY,   -- 32-byte NodeId (= public key hash)
    display_name    TEXT,                               -- User-assigned name
    public_key      BLOB        NOT NULL,               -- Ed25519 public key (32 bytes)
    trust_level     INTEGER     NOT NULL DEFAULT 0
                    CHECK (trust_level BETWEEN 0 AND 3), -- 0=unknown, 1=trusted, 2=verified, 3=admin
    first_contact   INTEGER     NOT NULL,               -- Unix timestamp ms
    last_contact    INTEGER     NOT NULL,               -- Unix timestamp ms
    is_emergency_contact INTEGER NOT NULL DEFAULT 0     -- Boolean
);
```

### `metrics` Table

Local metrics storage (offline Prometheus alternative).

```sql
CREATE TABLE metrics (
    metric_name     TEXT        NOT NULL,
    value           REAL        NOT NULL,
    labels          TEXT,                               -- JSON key-value pairs
    recorded_at     INTEGER     NOT NULL
);

CREATE INDEX idx_metrics_name_time ON metrics (metric_name, recorded_at DESC);
```

## Storage Quota and Eviction

### Quota

Default storage budget: 500MB for the message store. Configurable via `IrisCoreConfig.max_storage_bytes`.

### Eviction Policy

Eviction is triggered when storage usage exceeds 80% of quota. Messages are removed in priority order:

```rust
async fn evict_messages(&self, target_bytes: u64) -> Result<u64, StorageError> {
    let conn = self.conn.lock().await;
    // Remove expired messages first (regardless of priority)
    let expired = conn.execute(
        "DELETE FROM messages WHERE expires_at < ?1 AND status NOT IN ('DELIVERED')",
        [unix_now_ms()]
    )?;

    // If still over budget, evict by (priority DESC, expires_at ASC)
    // Lowest priority, soonest-to-expire removed first
    if self.current_usage_bytes().await? > target_bytes {
        conn.execute("
            DELETE FROM messages
            WHERE message_id IN (
                SELECT message_id FROM messages
                WHERE status NOT IN ('DELIVERED')
                  AND is_own_message = 0
                ORDER BY priority DESC, expires_at ASC
                LIMIT ?1
            )",
            [estimated_eviction_count(target_bytes)]
        )?;
    }

    Ok(self.current_usage_bytes().await?)
}
```

**Priority-based rules**:
- P0 messages: never evicted (unless expired)
- P1-P2 messages: evicted only after all P3-P7 are removed
- P3-P7 messages: evicted in order of soonest expiry first
- Own messages (sent by this node): lower eviction priority than relayed messages

## Message Persistence Guarantees

All priority levels P0-P7 are persisted immediately on receipt using SQLite's ACID transaction guarantees. WAL mode ensures the write is durable before the function returns.

```rust
pub async fn persist_message(&self, envelope: &Envelope) -> Result<(), StorageError> {
    let conn = self.conn.lock().await;
    conn.execute(
        "INSERT OR IGNORE INTO messages
         (message_id, sender_id, recipient_id, priority, status, created_at,
          expires_at, hop_count, max_hops, payload, payload_size, signature)
         VALUES (?1, ?2, ?3, ?4, 'PENDING_SEND', ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            envelope.id.to_hex(),
            envelope.sender.as_bytes(),
            envelope.recipient.as_bytes(),
            envelope.priority as u8,
            envelope.created_at_ms,
            envelope.expires_at_ms,
            envelope.hop_count,
            envelope.max_hops,
            envelope.encrypted_payload.as_slice(),
            envelope.payload_size,
            envelope.signature.as_slice(),
        ]
    )?;
    Ok(())
}
```

The `INSERT OR IGNORE` ensures deduplication at the storage layer (secondary to the Bloom filter check).
