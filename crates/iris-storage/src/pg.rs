//! PostgreSQL implementation of the [`MessageStorage`] seam — STORE-001.
//!
//! A single multiplexed `tokio_postgres::Client` (wrapped in an `Arc`) serves
//! concurrent async queries; the engine talks to this backend through the
//! seam trait. Connection parameters come from [`PgStorageConfig`], with the
//! password read from `IRIS_PG_PASSWORD` (never hardcoded / committed).

use std::sync::Arc;

use iris_core::message_engine::lifecycle::MessageStatus;
use iris_core::message_engine::storage::{MessageStorage, StorageError};
use iris_core::message::MessagePriority;
use iris_core::protocol::{codec, Envelope, MessageId};

use crate::eviction::{delete_expired, evict_lowest_priority, usage_bytes};
use crate::schema::{DDL, MIGRATE_DROP_PLAINTEXT_IDENTITY};
use crate::seal::{NoSealer, RowSealer};

/// Connection + quota configuration for [`PgStorage`].
#[derive(Debug, Clone)]
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
            eviction_threshold: 0.8,
        }
    }
}

/// PostgreSQL message store.
pub struct PgStorage {
    client: Arc<tokio_postgres::Client>,
    config: PgStorageConfig,
    /// At-rest row sealing (CRYPTO-001 gate). `NoSealer` by default.
    sealer: Arc<dyn RowSealer>,
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
        let _expired = delete_expired(&self.client, now_unix).await.map_err(backend)?;
        if self.config.max_storage_bytes > 0 {
            let target = (self.config.max_storage_bytes as f64 * self.config.eviction_threshold) as u64;
            if usage_bytes(&self.client).await.map_err(backend)? > target {
                let _ = evict_lowest_priority(&self.client, target).await.map_err(backend)?;
            }
        }
        Ok(())
    }

    /// Access the raw client for maintenance/administration.
    pub fn client(&self) -> &tokio_postgres::Client {
        &self.client
    }

    fn projected_row_bytes(cbor_len: u64) -> u64 {
        cbor_len + crate::eviction::ROW_OVERHEAD_BYTES
    }
}

#[async_trait::async_trait]
impl MessageStorage for PgStorage {
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError> {
        let cbor = codec::encode(envelope).map_err(|e| StorageError::Backend(e.to_string()))?;
        let expires_at = envelope.timestamp.saturating_add(envelope.ttl_seconds);
        let aad = crate::seal::row_aad(&envelope.message_id.to_string(), envelope.priority as u8, expires_at);
        let stored = self
            .sealer
            .seal(&aad, &cbor)
            .map_err(|e| StorageError::Backend(format!("seal: {e}")))?;
        let is_p0 = envelope.priority == MessagePriority::P0;

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
                    &(envelope.timestamp as i64),
                    &(expires_at as i64),
                    &(envelope.hop_count as i16),
                    &envelope.max_hops.map(|h| h as i16),
                    &(envelope.payload_size as i64),
                    &stored,
                    &false,
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

    async fn update_status(&self, id: &MessageId, status: MessageStatus) -> Result<(), StorageError> {
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
            .execute("DELETE FROM messages WHERE message_id = $1", &[&id.to_string()])
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
            let cbor = self.sealer.unseal(&aad, &bytes)?;
            match codec::decode(&cbor) {
                Ok(env) => out.push(env),
                Err(e) => return Err(StorageError::Backend(format!("get_queue decode: {e}"))),
            }
        }
        Ok(out)
    }
}
