//! SEC6_AUDIT D1 regression: the delivered-message pump must survive a
//! broadcast-lag burst (ring overflow) instead of dying silently.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use iris_desktop::engine_handle::pump_test_hooks::make_envelope;

#[tokio::test(flavor = "multi_thread")]
async fn inbox_pump_survives_broadcast_lag_and_close() {
    // Small ring so the test can overflow it deterministically.
    let (tx, rx) =
        tokio::sync::broadcast::channel::<iris_core::protocol::Envelope>(4);
    let keep_open = tx.clone(); // hold a second sender so Close is explicit later
    drop(tx);

    let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ended = Arc::new(AtomicBool::new(false));

    let seen_t = seen.clone();
    let ended_t = ended.clone();
    let handle = tokio::spawn(iris_desktop::engine_handle::pump_inbox_for_tests(
        rx,
        move |_view| {
            seen_t.fetch_add(1, Ordering::SeqCst);
            true // keep consuming
        },
        ended_t.clone(),
    ));

    // Overflow the ring while the pump is mid-poll: > capacity sends force Lagged.
    for _ in 0..40 {
        let _ = keep_open.send(make_envelope());
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // THE assertion: after a lag burst the pump is STILL ALIVE and consuming.
    let after_burst = seen.load(Ordering::SeqCst);
    assert!(
        after_burst > 0,
        "pump consumed nothing — died on the lag burst (regression)"
    );

    // More traffic after the burst must ALSO be delivered (stream is live).
    for _ in 0..8 {
        let _ = keep_open.send(make_envelope());
    }
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(
        seen.load(Ordering::SeqCst) > after_burst,
        "pump stopped consuming after the lag burst (regression)"
    );

    // Clean shutdown path: dropping the last sender closes the channel.
    drop(keep_open);
    tokio::time::timeout(std::time::Duration::from_secs(2), handle)
        .await
        .expect("pump must end on channel close")
        .expect("pump task panicked");
    assert!(ended.load(Ordering::SeqCst));
}
