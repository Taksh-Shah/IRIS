//! Storage seam — MSG-001 ↔ STORE-001 (WP-2 implements this trait).
//!
//! The MessageEngine depends only on this trait; the concrete SQLite/SQLCipher
//! backend lives in `crates/iris-storage` (STORAGE.md: no SQL in iris-core).
//! Engine-side methods are async; the WP-2 backend wraps `rusqlite` in
//! `spawn_blocking`.

use crate::message_engine::lifecycle::MessageStatus;
use crate::protocol::Envelope;
use crate::protocol::MessageId;

/// Error surface exposed to the engine by a storage backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// The underlying store failed (I/O, SQL, encryption key, disk full…).
    Backend(String),
    /// The store is out of capacity and could not honor the write.
    StorageFull,
    /// A required key/decryption step failed (SQLCipher wrong key etc.).
    DecryptionFailed,
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageError::Backend(m) => write!(f, "storage backend: {m}"),
            StorageError::StorageFull => write!(f, "storage: full"),
            StorageError::DecryptionFailed => write!(f, "storage: decryption failed"),
        }
    }
}

impl std::error::Error for StorageError {}

/// Message persistence interface (MSG_DESIGN.md §Interfaces → Storage).
#[async_trait::async_trait]
pub trait MessageStorage: Send + Sync + 'static {
    /// Persist an envelope. Deduplication at the storage layer uses
    /// INSERT OR IGNORE (STORAGE.md) — a duplicate insert is a no-op.
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError>;

    /// Load a single envelope by id.
    async fn load(&self, id: &MessageId) -> Result<Option<Envelope>, StorageError>;

    /// Update the lifecycle status of a stored message.
    async fn update_status(
        &self,
        id: &MessageId,
        status: MessageStatus,
    ) -> Result<(), StorageError>;

    /// Delete (or tombstone) a message.
    async fn delete(&self, id: &MessageId) -> Result<(), StorageError>;

    /// Evict all messages expired before `before_unix`. Returns count evicted.
    /// P0 messages are NEVER evicted here (INV-ROUTE-003; enforced in WP-2).
    async fn evict_expired(&self, before_unix: u64) -> Result<usize, StorageError>;

    /// Evict lowest-priority messages to free `target_bytes`. P0 exempt.
    async fn evict_by_priority(&self, target_bytes: u64) -> Result<usize, StorageError>;

    /// Current disk usage in bytes (quota enforcement input).
    async fn usage_bytes(&self) -> Result<u64, StorageError>;

    /// Next messages to send, ordered for the engine's queue (P0-first).
    async fn get_queue(&self, limit: usize) -> Result<Vec<Envelope>, StorageError>;
}

/// A no-op storage backend for engine-only tests (no SQL dependency).
#[derive(Default, Debug)]
pub struct MemoryStorage {
    inner: std::sync::Mutex<Vec<Envelope>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl MessageStorage for MemoryStorage {
    async fn persist(&self, envelope: &Envelope) -> Result<(), StorageError> {
        let mut v = self.inner.lock().unwrap();
        if v.iter().any(|e| e.message_id == envelope.message_id) {
            return Ok(()); // INSERT OR IGNORE semantics
        }
        v.push(envelope.clone());
        Ok(())
    }

    async fn load(&self, id: &MessageId) -> Result<Option<Envelope>, StorageError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .iter()
            .find(|e| &e.message_id == id)
            .cloned())
    }

    async fn update_status(
        &self,
        _id: &MessageId,
        _status: MessageStatus,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    async fn delete(&self, id: &MessageId) -> Result<(), StorageError> {
        self.inner.lock().unwrap().retain(|e| &e.message_id != id);
        Ok(())
    }

    async fn evict_expired(&self, _before_unix: u64) -> Result<usize, StorageError> {
        Ok(0)
    }

    async fn evict_by_priority(&self, _target_bytes: u64) -> Result<usize, StorageError> {
        Ok(0)
    }

    async fn usage_bytes(&self) -> Result<u64, StorageError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .iter()
            .map(|e| e.payload.len() as u64)
            .sum())
    }

    async fn get_queue(&self, limit: usize) -> Result<Vec<Envelope>, StorageError> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .iter()
            .take(limit)
            .cloned()
            .collect())
    }
}
