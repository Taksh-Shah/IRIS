//! PostgreSQL implementation of the [`MessageStorage`] seam — STORE-001.
//!
//! A single multiplexed `tokio_postgres::Client` (wrapped in an `Arc`) serves
//! concurrent async queries; the engine talks to this backend through the
//! seam trait. Connection parameters come from [`PgStorageConfig`], with the
//! password read from `IRIS_PG_PASSWORD` (never hardcoded / committed).

use std::sync::Arc;

use iris_core::message::MessagePriority;
use iris_core::message_engine::lifecycle::MessageStatus;
use iris_core::message_engine::storage::{MessageStorage, StorageError};
use iris_core::protocol::{codec, Envelope, MessageId};

use crate::eviction::{delete_expired, evict_lowest_priority, usage_bytes};
use crate::schema::{
    DDL, MIGRATE_DROP_PLAINTEXT_IDENTITY, MIGRATE_FIX_PARTIAL_INDEXES,
    MIGRATE_NONNEGATIVE_CONSTRAINT,
};
use crate::seal::{NoSealer, RowSealer};

/// Connection + quota configuration for [`PgStorage`].
///
/// `Debug` is hand-implemented to redact `password` (TAK-16): the derived form
/// printed the literal secret, so any `tracing::debug!(?config, ...)` or
/// panic payload containing the config wrote `IRIS_PG_PASSWORD` to the logs.
#[derive(Clone)]
pub struct PgStorageConfig {
    pub host: String,
    pub port: u16,
    pub dbname: String,
    pub user: String,
    /// The password. `from_env()` fills this from `IRIS_PG_PASSWORD`.
    pub password: String,
    /// Storage quota in bytes (0 disables quota enforcement).
    pub max_storage_bytes: u64,
    /// Fraction of quota that triggers eviction (STORAGE.md: 0.8).
    pub eviction_threshold: f64,
    /// This node's identity, when known (TAK-10). When set, `persist` derives
    /// the `is_own_message` column by comparing the envelope sender to it, so
    /// the documented eviction policy ("relayed before own") actually has a
    /// live key. `None` keeps every row marked relayed — the pre-TAK-10
    /// behaviour — and is what `from_env` produces until `IRIS_NODE_ID` is
    /// provided. Deliberately carried in config rather than widening the
    /// Section-2-owned `MessageStorage::persist` signature.
    pub node_id: Option<[u8; 32]>,
}

impl std::fmt::Debug for PgStorageConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PgStorageConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("dbname", &self.dbname)
            .field("user", &self.user)
            .field("password", &"[redacted]")
            .field("max_storage_bytes", &self.max_storage_bytes)
            .field("eviction_threshold", &self.eviction_threshold)
            .finish()
    }
}

impl PgStorageConfig {
    pub fn from_env() -> Self {
        Self {
            host: std::env::var("IRIS_PG_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("IRIS_PG_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5432),
            dbname: std::env::var("IRIS_PG_DB").unwrap_or_else(|_| "iris".into()),
            user: std::env::var("IRIS_PG_USER").unwrap_or_else(|_| "postgres".into()),
            password: std::env::var("IRIS_PG_PASSWORD").unwrap_or_default(),
            max_storage_bytes: std::env::var("IRIS_MAX_STORAGE_BYTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(500 * 1024 * 1024),
            eviction_threshold: parse_eviction_threshold(
                std::env::var("IRIS_EVICTION_THRESHOLD").ok(),
            ),
            node_id: std::env::var("IRIS_NODE_ID")
                .ok()
                .as_deref()
                .and_then(parse_node_id),
        }
    }
}

/// Parse a 64-hex-char `IRIS_NODE_ID` into the 32-byte peer identity
/// (TAK-10). Absent or malformed values yield `None` (rows stay marked
/// relayed) rather than a wrong identity.
fn parse_node_id(raw: &str) -> Option<[u8; 32]> {
    let hex = raw.trim();
    if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        let hi = (chunk[0] as char).to_digit(16)? as u8;
        let lo = (chunk[1] as char).to_digit(16)? as u8;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Parse and range-validate `IRIS_EVICTION_THRESHOLD` (TAK-25). Anything
/// absent, unparseable, or outside `0.1..=0.95` falls back to the documented
/// default of `0.8`, so a misconfiguration can never disable eviction
/// (`>= 1.0`) or trigger it on every write (`<= 0.0`).
fn parse_eviction_threshold(raw: Option<String>) -> f64 {
    raw.and_then(|v| v.parse::<f64>().ok())
        .filter(|v| (0.1..=0.95).contains(v))
        .unwrap_or(0.8)
}

/// PostgreSQL message store.
pub struct PgStorage {
    client: Arc<tokio_postgres::Client>,
    config: PgStorageConfig,
    /// At-rest row sealing (CRYPTO-001 gate). `NoSealer` by default.
    sealer: Arc<dyn RowSealer>,
    /// Rows quarantined by [`PgStorage::get_queue`] because they could not be
    /// unsealed or decoded (TAK-3). Observable via [`PgStorage::quarantined_rows`].
    quarantined: std::sync::atomic::AtomicU64,
}

fn status_to_text(s: MessageStatus) -> &'static str {
    match s {
        MessageStatus::Created | MessageStatus::PendingSend => "PENDING_SEND",
        MessageStatus::InTransit => "IN_TRANSIT",
        MessageStatus::Delivered => "DELIVERED",
        MessageStatus::Acknowledged => "ACKNOWLEDGED",
        MessageStatus::Expired => "EXPIRED",
        MessageStatus::DeliveryFailed => "DELIVERY_FAILED",
    }
}

fn backend(e: tokio_postgres::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

impl PgStorage {
    /// Connect, spawn the connection driver task, and apply the schema.
    pub async fn connect(config: PgStorageConfig) -> Result<Self, StorageError> {
        let mut pg = tokio_postgres::Config::new();
        pg.host(&config.host)
            .port(config.port)
            .dbname(&config.dbname)
            .user(&config.user)
            .password(&config.password);

        let (client, connection) = pg
            .connect(tokio_postgres::NoTls)
            .await
            .map_err(|e| StorageError::Backend(format!("pg connect: {e}")))?;
        // Drive the connection in the background.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                tracing::warn!("pg connection closed: {e}");
            }
        });

        let store = Self {
            client: Arc::new(client),
            config,
            sealer: Arc::new(NoSealer),
            quarantined: std::sync::atomic::AtomicU64::new(0),
        };
        store
            .client
            .batch_execute(DDL)
            .await
            .map_err(|e| StorageError::Backend(format!("pg migrate: {e}")))?;
        // CRYPTO-001: harden pre-gate databases (drop plaintext identity cols).
        store
            .client
            .batch_execute(MIGRATE_DROP_PLAINTEXT_IDENTITY)
            .await
            .map_err(|e| StorageError::Backend(format!("pg migrate v2: {e}")))?;
        // TAK-9: non-negative scalars enforced at the DB layer as well.
        store
            .client
            .batch_execute(MIGRATE_NONNEGATIVE_CONSTRAINT)
            .await
            .map_err(|e| StorageError::Backend(format!("pg migrate v3: {e}")))?;
        // TAK-11: replace the unusable partial indexes on existing DBs
        // (fresh databases already get the corrected DDL).
        store
            .client
            .batch_execute(MIGRATE_FIX_PARTIAL_INDEXES)
            .await
            .map_err(|e| StorageError::Backend(format!("pg migrate v4: {e}")))?;
        Ok(store)
    }

    /// Connect using environment configuration.
    pub async fn connect_from_env() -> Result<Self, StorageError> {
        Self::connect(PgStorageConfig::from_env()).await
    }

    /// Replace the at-rest row sealer (CRYPTO-001). Call after `connect`.
    pub fn with_sealer(mut self, sealer: Arc<dyn RowSealer>) -> Self {
        self.sealer = sealer;
        self
    }

    /// Run one GC tick: reclaim expired rows, then evict by priority if usage
    /// exceeds the quota threshold (STORAGE.md). P0 never evicted while live.
    pub async fn gc_once(&self, now_unix: u64) -> Result<(), StorageError> {
        let _expired = delete_expired(&self.client, now_unix)
            .await
            .map_err(backend)?;
        if self.config.max_storage_bytes > 0 {
            let target =
                (self.config.max_storage_bytes as f64 * self.config.eviction_threshold) as u64;
            if usage_bytes(&self.client).await.map_err(backend)? > target {
                let _ = evict_lowest_priority(&self.client, target)
                    .await
                    .map_err(backend)?;
            }
        }
        Ok(())
    }

    /// Access the raw client for maintenance/administration.
    pub fn client(&self) -> &tokio_postgres::Client {
        &self.client
    }

    /// Rows quarantined by `get_queue` so far (TAK-3): unreadable rows are
    /// skipped, marked `DELIVERY_FAILED` and counted instead of bricking the
    /// send queue. A sudden rise equal to the whole queue indicates a
    /// systemic cause — most likely a sealing-key mismatch — and should page
    /// an operator before more traffic is accepted.
    pub fn quarantined_rows(&self) -> u64 {
        self.quarantined.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Move a row out of the send queue after an unrecoverable read failure:
    /// `status -> DELIVERY_FAILED` leaves the `get_queue` predicate and makes
    /// the row evictable. Failures to quarantine itself are logged and do not
    /// propagate into the drain path.
    async fn quarantine_row(&self, message_id: &str) {
        if let Err(e) = self
            .client
            .execute(
                "UPDATE messages SET status = 'DELIVERY_FAILED' WHERE message_id = $1",
                &[&message_id],
            )
            .await
        {
            tracing::error!(message_id = %message_id, error = %e, "quarantine update failed");
        }
    }

    fn projected_row_bytes(cbor_len: u64) -> u64 {
        cbor_len + crate::eviction::ROW_OVERHEAD_BYTES
    }
}

#[async_trait::async_trait]
impl MessageStorage for PgStorage {
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError> {
        // TAK-9: same-width `as` casts silently reinterpret bits, letting a
        // peer-chosen u64 timestamp/expiry/payload_size land as a NEGATIVE
        // BIGINT (instant-expiring or immortal rows, corrupted ordering).
        // Reject out-of-domain values at the boundary instead.
        let created_at = i64::try_from(envelope.timestamp)
            .map_err(|_| StorageError::Backend("timestamp out of i64 range".into()))?;
        let expires_at = envelope.timestamp.saturating_add(envelope.ttl_seconds);
        let expires_at = i64::try_from(expires_at)
            .map_err(|_| StorageError::Backend("expiry out of i64 range".into()))?;
        let payload_size = i64::try_from(envelope.payload_size)
            .map_err(|_| StorageError::Backend("payload_size out of i64 range".into()))?;

        let cbor = codec::encode(envelope).map_err(|e| StorageError::Backend(e.to_string()))?;
        let aad = crate::seal::row_aad(
            &envelope.message_id.to_string(),
            envelope.priority as u8,
            envelope.timestamp.saturating_add(envelope.ttl_seconds),
        );
        let stored = self
            .sealer
            .seal(&aad, &cbor)
            .map_err(|e| StorageError::Backend(format!("seal: {e}")))?;
        let is_p0 = envelope.priority == MessagePriority::P0;
        // TAK-10: real ownership when the node identity is configured — the
        // eviction policy's "relayed before own" tie-break needs a live key.
        // Malformed sender vectors simply stay marked relayed.
        let is_own = match (
            &self.config.node_id,
            <[u8; 32]>::try_from(envelope.sender_id.as_slice()),
        ) {
            (Some(node_id), Ok(sender)) => *node_id == sender,
            _ => false,
        };

        if self.config.max_storage_bytes > 0 {
            let current = usage_bytes(&self.client).await.map_err(backend)?;
            let grow = Self::projected_row_bytes(stored.len() as u64);
            if current.saturating_add(grow) > self.config.max_storage_bytes && !is_p0 {
                // Non-P0 write would exceed quota → refuse; P0 always accepted.
                return Err(StorageError::StorageFull);
            }
        }

        self.client
            .execute(
                "INSERT INTO messages
                   (message_id, priority, status, created_at,
                    expires_at, hop_count, max_hops, payload_size, envelope_cbor, is_own_message)
                 VALUES ($1,$2,'PENDING_SEND',$3,$4,$5,$6,$7,$8,$9)
                 ON CONFLICT (message_id) DO NOTHING",
                &[
                    &envelope.message_id.to_string(),
                    &(envelope.priority as i16),
                    &created_at,
                    &expires_at,
                    &(envelope.hop_count as i16),
                    &envelope.max_hops.map(|h| h as i16),
                    &payload_size,
                    &stored,
                    &is_own,
                ],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("persist: {e}")))?;
        Ok(())
    }

    async fn load(&self, id: &MessageId) -> Result<Option<Envelope>, StorageError> {
        let row = self
            .client
            .query_opt(
                "SELECT envelope_cbor, priority, expires_at FROM messages WHERE message_id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("load: {e}")))?;
        match row {
            Some(r) => {
                let bytes: Vec<u8> = r.get(0);
                let priority: i16 = r.get(1);
                let expires_at: i64 = r.get(2);
                let aad = crate::seal::row_aad(&id.to_string(), priority as u8, expires_at as u64);
                let restored = self.sealer.unseal(&aad, &bytes)?;
                if bytes == restored {
                    // Plaintext row (legacy / NoSealer) — decode directly.
                    return codec::decode(&bytes)
                        .map(Some)
                        .map_err(|e| StorageError::Backend(format!("load decode: {e}")));
                }
                codec::decode(&restored)
                    .map(Some)
                    .map_err(|e| StorageError::Backend(format!("load unseal decode: {e}")))
            }
            None => Ok(None),
        }
    }

    async fn update_status(
        &self,
        id: &MessageId,
        status: MessageStatus,
    ) -> Result<(), StorageError> {
        self.client
            .execute(
                "UPDATE messages SET status = $2 WHERE message_id = $1",
                &[&id.to_string(), &status_to_text(status)],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("update_status: {e}")))?;
        Ok(())
    }

    async fn delete(&self, id: &MessageId) -> Result<(), StorageError> {
        self.client
            .execute(
                "DELETE FROM messages WHERE message_id = $1",
                &[&id.to_string()],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("delete: {e}")))?;
        Ok(())
    }

    async fn evict_expired(&self, before_unix: u64) -> Result<usize, StorageError> {
        delete_expired(&self.client, before_unix)
            .await
            .map(|n| n as usize)
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn evict_by_priority(&self, target_bytes: u64) -> Result<usize, StorageError> {
        evict_lowest_priority(&self.client, target_bytes)
            .await
            .map(|n| n as usize)
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn usage_bytes(&self) -> Result<u64, StorageError> {
        usage_bytes(&self.client)
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))
    }

    async fn get_queue(&self, limit: usize) -> Result<Vec<Envelope>, StorageError> {
        let rows = self
            .client
            .query(
                "SELECT envelope_cbor, message_id, priority, expires_at FROM messages
                 WHERE status IN ('PENDING_SEND', 'IN_TRANSIT')
                 ORDER BY priority ASC, created_at ASC, message_id ASC
                 LIMIT $1",
                &[&(limit as i64)],
            )
            .await
            .map_err(|e| StorageError::Backend(format!("get_queue: {e}")))?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let bytes: Vec<u8> = r.get(0);
            let id: String = r.get(1);
            let priority: i16 = r.get(2);
            let expires_at: i64 = r.get(3);
            let aad = crate::seal::row_aad(&id, priority as u8, expires_at as u64);
            // TAK-3: one unreadable row must not brick the whole queue. The
            // row is quarantined (status -> DELIVERY_FAILED), which removes it
            // from this predicate and makes it evictable; the failure is
            // logged at ERROR and counted (see `quarantined_rows`). A
            // systemic cause (e.g. wrong sealing key) shows up as the counter
            // rising once per queued row — visible, not silent.
            let cbor = match self.sealer.unseal(&aad, &bytes) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(message_id = %id, error = %e, "quarantining unsealable row");
                    self.quarantined
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.quarantine_row(&id).await;
                    continue;
                }
            };
            match codec::decode(&cbor) {
                Ok(env) => out.push(env),
                Err(e) => {
                    tracing::error!(message_id = %id, error = %e, "quarantining undecodable row");
                    self.quarantined
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    self.quarantine_row(&id).await;
                    continue;
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_parser_accepts_well_formed_hex() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let parsed = parse_node_id(hex).expect("valid id");
        assert_eq!(parsed[0], 0x01);
        assert_eq!(parsed[15], 0xef);
        assert_eq!(parsed[31], 0xef);
    }

    #[test]
    fn node_id_parser_rejects_malformed_input() {
        assert_eq!(parse_node_id(""), None);
        assert_eq!(parse_node_id("zz"), None);
        assert_eq!(parse_node_id("0123"), None); // too short
        let long_odd = format!("0{}", "a".repeat(64));
        assert_eq!(parse_node_id(&long_odd), None);
        let not_hex = "g".repeat(64);
        assert_eq!(parse_node_id(&not_hex), None);
    }

    #[test]
    fn threshold_defaults_when_absent_or_invalid() {
        assert_eq!(parse_eviction_threshold(None), 0.8);
        assert_eq!(parse_eviction_threshold(Some("not-a-number".into())), 0.8);
        // Out-of-range values must fall back, never disable or hyper-trigger.
        assert_eq!(parse_eviction_threshold(Some("1.5".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("0.0".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("-0.2".into())), 0.8);
        assert_eq!(parse_eviction_threshold(Some("0.96".into())), 0.8);
    }

    #[test]
    fn threshold_accepts_in_range_values() {
        assert_eq!(parse_eviction_threshold(Some("0.5".into())), 0.5);
        assert_eq!(parse_eviction_threshold(Some("0.1".into())), 0.1);
        assert_eq!(parse_eviction_threshold(Some("0.95".into())), 0.95);
    }
}
