//! Priority-aware eviction queries (STORAGE.md §Storage Quota and Eviction).
//!
//! Invariants enforced here:
//! - **This node's own P0 is never evicted while unexpired** (INV-ROUTE-003).
//!   TAK-2: the invariant is about *this node's own* emergency traffic, not
//!   an unauthenticated peer's unverified claim to be P0 — a *relayed* P0
//!   row is evictable (last, after every other priority), matching
//!   `pg.rs::persist`'s admission bypass, which is narrowed the same way.
//!   Without this, a remote P0 flood is both unlimited on ingest and
//!   permanently unreclaimable, exhausting the store for good.
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
/// Returns the number of rows deleted. This node's own P0 is exempt
/// (INV-ROUTE-003); a relayed P0 row is evictable, last, after every other
/// priority (TAK-2).
///
/// Cost-bounded (TAK-12): each pass deletes up to [`EVICT_BATCH`] rows and the
/// loop runs at most [`EVICT_MAX_PASSES`] times — a full reclaim costs at most
/// 64 round-trip pairs instead of one pair per row (the old LIMIT 1 loop was
/// ~200k round trips to reclaim 100 MB of 1 KB rows). If the cap is exhausted
/// while still above target, that condition is escalated at ERROR level: it is
/// either a pathological ingest rate or unevictable rows accumulating (the
/// TAK-2 class) announcing itself.
pub(crate) async fn evict_lowest_priority(
    client: &tokio_postgres::Client,
    target_bytes: u64,
) -> Result<u64, tokio_postgres::Error> {
    const EVICT_MAX_PASSES: usize = 64;
    const EVICT_BATCH: i64 = 256;

    let mut deleted = 0u64;
    let mut reached_target = false;
    for _ in 0..EVICT_MAX_PASSES {
        if usage_bytes(client).await? <= target_bytes {
            reached_target = true;
            break;
        }
        let n = client
            .execute(
                "DELETE FROM messages
                 WHERE message_id IN (
                    SELECT message_id FROM messages
                    WHERE NOT (priority = 0 AND is_own_message)
                    ORDER BY priority DESC, is_own_message ASC, expires_at ASC, created_at ASC
                    LIMIT $1
                 )",
                &[&EVICT_BATCH],
            )
            .await?;
        if n == 0 {
            // Nothing evictable remains (own-P0-only residue): stop instead of
            // spinning against an invariant target.
            break;
        }
        deleted += n as u64;
    }
    if !reached_target {
        // Distinguishable from success by callers via deleted count + this
        // error-level trace; iris-storage has no metrics registry seam yet.
        tracing::error!(
            deleted,
            target_bytes,
            "eviction hit its pass cap while still above target - store remains over quota"
        );
    }
    Ok(deleted)
}
