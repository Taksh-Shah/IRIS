//! Background garbage collection task (STORAGE.md: 60 s TTL GC).
//!
//! Periodically reclaims expired messages and enforces the storage quota via
//! [`super::PgStorage::gc_once`]. P0 messages are never evicted while live
//! (enforced inside `evict_lowest_priority`, INV-ROUTE-003).
//!
//! Missed ticks are *delayed*, never burst (TAK-13): GC is idempotent, so a
//! pass that overruns its interval must simply skip the missed beats and
//! resume on cadence. The default `MissedTickBehavior::Burst` repaid overdue
//! ticks back-to-back, amplifying load against a database that had just
//! demonstrably stalled — turning one outage into two.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use crate::pg::PgStorage;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The GC cadence loop, generic over its job so it is testable without a
/// database. Runs `job` once per `interval`; a job that overruns the interval
/// causes the *next* run to slip by one full interval (`Delay`) instead of
/// firing every owed tick back-to-back (`Burst`). Failures are logged at WARN
/// and never terminate the loop.
pub(crate) async fn gc_loop<F, Fut>(interval: Duration, mut job: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<(), iris_core::message_engine::storage::StorageError>>,
{
    let mut tick = tokio::time::interval(interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        if let Err(e) = job().await {
            tracing::warn!("storage gc failed: {e}");
        }
    }
}

/// Spawn a detached background task that runs GC every `interval`.
pub fn spawn_gc(store: Arc<PgStorage>, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(gc_loop(interval, move || {
        let store = store.clone();
        async move { store.gc_once(unix_now()).await }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use iris_core::message_engine::storage::StorageError;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// Advance virtual time in small slices, yielding between slices so the
    /// spawned loop gets scheduling passes (same pattern as the workspace
    /// `tokio_behavior.rs` virtual-clock driver).
    async fn pump(total_ms: u64) {
        for _ in 0..(total_ms / 5) {
            tokio::time::advance(Duration::from_millis(5)).await;
            tokio::task::yield_now().await;
        }
    }

    /// One 250 ms (virtual) GC pass overruns two 100 ms intervals. Under the
    /// production `Delay` behavior the missed beats are skipped; under the old
    /// default (`Burst`, what tokio gives without this fix) they are repaid
    /// back-to-back. A Burst control interval runs on the *same* virtual
    /// timeline so the assertion compares behaviors, not magic constants.
    #[tokio::test(start_paused = true)]
    async fn missed_gc_ticks_are_delayed_not_burst() {
        let slow_job = |runs: Arc<AtomicU32>| {
            move || {
                let r = runs.clone();
                async move {
                    r.fetch_add(1, Ordering::Relaxed);
                    // First pass overruns two intervals; later passes instant.
                    if r.load(Ordering::Relaxed) == 1 {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                    }
                    Ok::<(), StorageError>(())
                }
            }
        };

        // Production path (gc_loop pins Delay).
        let delay_runs = Arc::new(AtomicU32::new(0));
        let delay_handle = tokio::spawn(gc_loop(
            Duration::from_millis(100),
            slow_job(delay_runs.clone()),
        ));

        // Counterfactual: same cadence + job shape under Burst.
        let burst_runs = Arc::new(AtomicU32::new(0));
        let burst_runs2 = burst_runs.clone();
        let burst_handle = tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(100));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
            loop {
                tick.tick().await;
                let r = burst_runs2.clone();
                r.fetch_add(1, Ordering::Relaxed);
                if r.load(Ordering::Relaxed) == 1 {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            }
        });

        pump(360).await;

        let d = delay_runs.load(Ordering::Relaxed);
        let b = burst_runs.load(Ordering::Relaxed);
        assert!(d >= 2, "cadence resumed after the overrun (d={d})");
        assert!(
            d < b,
            "Delay must skip missed beats that Burst repays (delay={d}, burst={b})"
        );
        delay_handle.abort();
        burst_handle.abort();
    }

    /// A failing job must not kill the cadence.
    #[tokio::test(start_paused = true)]
    async fn gc_loop_survives_job_failures() {
        let runs = Arc::new(AtomicU32::new(0));
        let runs2 = runs.clone();
        let handle = tokio::spawn(gc_loop(Duration::from_millis(50), move || {
            let r = runs2.clone();
            async move {
                r.fetch_add(1, Ordering::Relaxed);
                Err::<(), _>(StorageError::Backend("boom".into()))
            }
        }));
        pump(200).await;
        assert!(
            runs.load(Ordering::Relaxed) >= 3,
            "loop keeps ticking past failures"
        );
        handle.abort();
    }
}
