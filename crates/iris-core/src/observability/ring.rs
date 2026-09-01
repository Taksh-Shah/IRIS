//! HV-86 — a bounded in-memory ring of recent transport-level events.
//!
//! The live [`crate::observability::MetricsRegistry`] answers "how many" but
//! not "what just happened, in what order". The intermittent-failure debugging
//! this whole hardware-verification pass exists for needs the latter: at the
//! moment a send fails, *which* transport was selected, did a link exist, did a
//! scan just get throttled, did a peer drop.
//!
//! [`RingLayer`] is a `tracing` layer that captures every event carrying an
//! `event = "..."` field (the OBS-001 taxonomy — `discovery.*`, `msg.*`,
//! `engine.*`, `wifiaware.*`, …) into a process-global 128-entry ring. It adds
//! **no new privacy surface**: it records exactly the field values the event
//! already renders to the log line (payload bytes and full identities are
//! forbidden there by P1/P2/P3), and drops everything else.
//!
//! `IrisEngine::snapshot()` (FFI) drains [`recent`] into the `/diag` output so a
//! failure can be inspected after the fact from a plain `adb` capture.

use std::collections::VecDeque;
use std::sync::Mutex;

use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

/// Ring capacity. ~128 lifecycle events covers several scan/connect cycles —
/// long enough to see the run-up to a failure, short enough to stay cheap.
const CAPACITY: usize = 128;

/// One captured event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RingEvent {
    /// Unix epoch milliseconds at capture (local wall clock).
    pub unix_ms: u64,
    /// `"ERROR"` | `"WARN"` | `"INFO"` | `"DEBUG"` | `"TRACE"`.
    pub level: &'static str,
    /// The `event = "..."` taxonomy value, e.g. `"discovery.connect_ok"`.
    pub event: String,
    /// A compact `key=value key=value` rendering of the other captured fields
    /// plus the event's message. Never contains payload bytes or full ids.
    pub detail: String,
}

static RING: Mutex<VecDeque<RingEvent>> = Mutex::new(VecDeque::new());

/// Snapshot the ring, oldest first. Cheap clone; does not clear.
pub fn recent() -> Vec<RingEvent> {
    RING.lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default()
}

/// Drop everything (tests; a `/diag` "clear" affordance later).
pub fn clear() {
    if let Ok(mut r) = RING.lock() {
        r.clear();
    }
}

/// Push one event, evicting the oldest when full.
pub fn record(ev: RingEvent) {
    if let Ok(mut r) = RING.lock() {
        if r.len() == CAPACITY {
            r.pop_front();
        }
        r.push_back(ev);
    }
}

fn level_str(l: &Level) -> &'static str {
    match *l {
        Level::ERROR => "ERROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DEBUG",
        Level::TRACE => "TRACE",
    }
}

/// Collects the `event` field, the message, and the remaining fields into a
/// compact line. Fields named `event` are pulled out; the giant `iris.diag` /
/// `iris.kpi` self-dumps are skipped so the ring never records its own reads.
#[derive(Default)]
struct RingVisitor {
    event: Option<String>,
    parts: Vec<String>,
}

impl Visit for RingVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "event" {
            self.event = Some(value.to_string());
        } else {
            self.parts.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        if field.name() == "event" {
            self.event = Some(rendered.trim_matches('"').to_string());
        } else if field.name() == "message" {
            self.parts.insert(0, rendered.trim_matches('"').to_string());
        } else {
            self.parts.push(format!("{}={}", field.name(), rendered));
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.parts.push(format!("{}={}", field.name(), value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.parts.push(format!("{}={}", field.name(), value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.parts.push(format!("{}={}", field.name(), value));
    }
}

/// `tracing` layer that feeds [`RING`]. Install alongside the logcat layer.
#[derive(Debug, Default, Clone, Copy)]
pub struct RingLayer;

impl<S: Subscriber> Layer<S> for RingLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut v = RingVisitor::default();
        event.record(&mut v);
        // Only events that opted into the taxonomy — keeps the ring to
        // lifecycle events, not every debug line.
        let Some(name) = v.event else { return };
        // The snapshot's own summary lines would otherwise recurse into here.
        if name == "iris.diag" || name == "iris.kpi" {
            return;
        }
        record(RingEvent {
            unix_ms: crate::message_engine::expiry::unix_now().saturating_mul(1000),
            level: level_str(event.metadata().level()),
            event: name,
            detail: v.parts.join(" "),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing_subscriber::prelude::*;

    #[test]
    fn captures_taxonomy_events_and_bounds_length() {
        clear();
        let _guard = tracing_subscriber::registry().with(RingLayer).set_default();

        tracing::info!(event = "discovery.connect_ok", transport = "ble-android", "linked");
        tracing::warn!(event = "msg.all_sends_failed", attempts = 3u64);
        tracing::debug!("no event field — must be ignored");
        tracing::info!(event = "iris.diag", "self-dump — must be ignored");

        let got = recent();
        assert_eq!(got.len(), 2, "only the two taxonomy events: {got:?}");
        assert_eq!(got[0].event, "discovery.connect_ok");
        assert!(got[0].detail.contains("transport=ble-android"));
        assert!(got[0].detail.contains("linked"));
        assert_eq!(got[1].event, "msg.all_sends_failed");
        assert!(got[1].detail.contains("attempts=3"));

        for i in 0..CAPACITY + 20 {
            tracing::info!(event = "discovery.scan_pass_complete", n = i as u64);
        }
        assert_eq!(recent().len(), CAPACITY, "ring must stay bounded");
    }
}
