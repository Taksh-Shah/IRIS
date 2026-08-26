//! Priority-aware eviction queries (STORAGE.md §Storage Quota and Eviction).
//!
//! Invariants enforced here:
//! - **P0 is never evicted while unexpired** (INV-ROUTE-003): `priority > 0`
//!   in every `evict_lowest_priority` statement.
//! - Expired rows are always reclaimed first regardless of priority.
//! - Lower priority evicted before higher; among equal priority, soonest expiry
//!   first, relayed (not own) messages before own.

/// Approximate per-row overhead (index + WAL/visibility overhead) beyond the
/// stored CBOR blob, used for quota accounting.
pub(crate) const ROW_OVERHEAD_BYTES: u64 = 192;

pub(crate) async fn usage_bytes(
    client: &tokio_postgres::Client,
) -> Result<u64, tokio_postgres::Error> {
    let row = client
        .query_one(
            "SELECT SUM(octet_length(envelope_cbor) + $1::bigint)::bigint FROM messages",
            &[&(ROW_OVERHEAD_BYTES as i64)],
        )
        .await?;
    let v: Option<i64> = row.get(0);
    Ok(v.unwrap_or(0).max(0) as u64)
}

/// Delete every message whose absolute expiry has passed. P0 expired messages
/// ARE reclaimed here (STORAGE.md: "unless expired").
pub(crate) async fn delete_expired(
    client: &tokio_postgres::Client,
    before_unix: u64,
) -> Result<u64, tokio_postgres::Error> {
    let n = client
        .execute(
            "DELETE FROM messages WHERE expires_at < $1",
            &[&(before_unix as i64)],
        )
        .await?;
    Ok(n as u64)
}

/// Reclaim the lowest-priority rows until usage falls at/below `target_bytes`.
/// Returns the number of rows deleted. P0 is exempt (INV-ROUTE-003).
pub(crate) async fn evict_lowest_priority(
    client: &tokio_postgres::Client,
    target_bytes: u64,
) -> Result<u64, tokio_postgres::Error> {
    let mut deleted = 0u64;
    loop {
        if usage_bytes(client).await? <= target_bytes {
            break;
        }
        let n = client
            .execute(
                "DELETE FROM messages
                 WHERE message_id IN (
                    SELECT message_id FROM messages
                    WHERE priority > 0
                    ORDER BY priority DESC, is_own_message ASC, expires_at ASC, created_at ASC
                    LIMIT 1
                 )",
                &[],
            )
            .await?;
        if n == 0 {
            break; // nothing evictable remains
        }
        deleted += n as u64;
    }
    Ok(deleted)
}
