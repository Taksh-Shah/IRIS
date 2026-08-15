//! Background garbage collection task (STORAGE.md: 60 s TTL GC).
//!
//! Periodically reclaims expired messages and enforces the storage quota via
//! [`super::PgStorage::gc_once`]. P0 messages are never evicted while live
//! (enforced inside `evict_lowest_priority`, INV-ROUTE-003).

use std::sync::Arc;
use std::time::Duration;

use crate::pg::PgStorage;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Spawn a detached background task that runs GC every `interval`.
pub fn spawn_gc(store: Arc<PgStorage>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(interval);
        loop {
            tick.tick().await;
            if let Err(e) = store.gc_once(unix_now()).await {
                tracing::warn!("storage gc failed: {e}");
            }
        }
    })
}
