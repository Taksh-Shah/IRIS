//! HW-3: bridges Rust's `tracing` events to Android logcat.
//!
//! Before this, the core (`iris-core`) had real instrumentation —
//! `tracing::debug!`/`info!`/`warn!` calls throughout `message_engine`,
//! `discovery`, and the transports — but nothing was ever subscribed to it.
//! Every one of those events was silently dropped, so debugging anything past
//! the Kotlin/FFI boundary meant reading source and guessing. This session's
//! hardware-verification pass burned significant time on exactly that: a
//! message that was accepted by the engine but never actually sent produced
//! zero signal anywhere, because the only place that would have said WHY
//! (`discovery::scan_once`'s connect attempts, `message_engine::
//! deliver_outbound`'s transport-selection decisions) was never wired to
//! anything visible.
//!
//! [`init`] installs a [`paranoid_android`] layer (writes to logcat via the
//! NDK's `liblog`, tagged `iriscore`) filtered by [`EnvFilter`]. Idempotent —
//! safe to call from every `IrisEngine::new()`, even if more than one engine
//! is constructed in-process (tests, a future multi-engine host) — only the
//! first call actually installs a subscriber; the rest are no-ops.

use std::sync::Once;

static INIT: Once = Once::new();

/// Default filter when no override is supplied. `RUST_LOG` isn't settable at
/// runtime on Android (no shell environment reaches the app process), so this
/// is the real default, not just a fallback: `debug` for this project's own
/// crates (where the useful events live) and `info` for everything else
/// (dependency noise at `debug` is not worth the log volume on a phone).
// HV-21: the cdylib is `[lib] name = "iriscode"`, so this crate's tracing
// events carry the module path `iriscode::…`, NOT `iris_android::…` — the old
// `iris_android=debug` directive matched nothing and every `tracing::debug!`
// in engine.rs (including `android: inbound rejected: {e}`, the one line that
// says WHY an inbound message was dropped) was silently filtered out. Keep
// both names so the intent survives a future rename.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
const DEFAULT_FILTER: &str = "info,iris_core=debug,iris_android=debug,iriscode=debug";

/// Install the logcat bridge. Call once at engine construction, before
/// anything that might emit a `tracing` event worth seeing (i.e. first thing
/// in `IrisEngine::new`). Safe to call multiple times or from a non-Android
/// host build — a no-op in both cases after the first successful install.
pub fn init() {
    INIT.call_once(|| {
        install();
    });
}

#[cfg(target_os = "android")]
fn install() {
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_new(DEFAULT_FILTER).unwrap_or_else(|_| EnvFilter::new("info"));
    let android_layer = paranoid_android::layer("iriscore");
    // HV-86: capture taxonomy events into the in-memory ring `/diag` reads.
    let ring_layer = iris_core::observability::ring::RingLayer;

    // `try_init` (not `init`) because a second `IrisEngine` in the same
    // process — a real possibility across config changes / test harnesses —
    // would otherwise panic on the global-subscriber-already-set case that
    // `Once` alone doesn't fully cover (a subscriber set by something else
    // entirely, e.g. a future test harness, is a legitimate "already done").
    let _ = tracing_subscriber::registry()
        .with(ring_layer)
        .with(android_layer)
        .with(filter)
        .try_init();
}

/// Non-Android hosts (the Windows/Linux/macOS dev-loop build of
/// `cargo build --workspace`, distinct from the `cargo ndk` cross-build that
/// produces the shipped `.so`) have no logcat to write to and no NDK liblog
/// to link against. Nothing to install.
#[cfg(not(target_os = "android"))]
fn install() {}
