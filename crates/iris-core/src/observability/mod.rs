//! Observability layer — OBS-001 (WP-9).
//!
//! Structured telemetry for the IRIS core: typed `tracing` events with a
//! centralized taxonomy + privacy helpers, and an injected lock-free
//! `MetricsRegistry` (no global state) read on demand at flush time.
//!
//! Design: `docs/implementation/OBS_DESIGN.md`. Research: `RES-0012`.
//!
//! Privacy contract (constraints):
//! - P1: payload bytes never in any event/log/metric.
//! - P2: identifiers in local/exported text are truncated 8-byte prefixes
//!   (`MessageId::short()` / `PeerId::short()`); full IDs in memory only.
//! - P3 (convention, reviewer-enforced): event fields follow the taxonomy in
//!   `mod event`; no compile-time or runtime allow-list exists yet. A future
//!   `macro_rules! iris_event!` or CI grep will mechanise this. Until then
//!   reviewers must ensure no `?.payload`, `?peer`, or full identity appears.
//! - P4: telemetry opt-in; production default `INFO`, TRACE never enabled.
//!   Peer identity (stable pseudonyms) is only emitted at DEBUG/TRACE level
//!   in this module; do not promote to INFO/WARN without a privacy review.
//! - P5 (host responsibility): the embedder MUST call `MetricsRegistry::reset()`
//!   at least once per 7-day window (e.g. in the periodic metric flush).
//!   v1 ships no in-crate rotation timer; see `reset()` doc for the contract.
//! - P6: no per-event metric writes on the hot path (metrics read at flush).
//!
//! **v1 scope note (SIM-21):** `MetricsRegistry` ships counters only. The
//! design document (`docs/implementation/OBSERVABILITY.md`) references gauges
//! (`iris_messages_in_queue`, `iris_storage_usage_bytes`, etc.) and a latency
//! histogram (`iris_message_delivery_latency_ms`) that are deferred to v2.
//! The naming grammar also diverges: the doc uses `iris_x_y`, the code uses
//! `iris.x.y`. Operators should not expect those series to appear in v1.

use std::collections::HashMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Privacy-truncated identifier (P2 / OBS_DESIGN.md).
///
/// Holds the **first 8 bytes** of a `MessageId`/`PeerId` as a fixed array and
/// prints them as 16 hex chars. Because this is the only type the event
/// registry accepts for identifiers, a full 16/32-byte ID cannot appear in any
/// log/event/metric field by construction (OBS-RT-01). No heap allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShortId([u8; 8]);

impl ShortId {
    /// Build from a fixed 8-byte prefix.
    pub fn from_prefix(b: [u8; 8]) -> Self {
        ShortId(b)
    }

    /// The raw 8-byte prefix.
    pub fn as_bytes(&self) -> &[u8; 8] {
        &self.0
    }
}

impl fmt::Display for ShortId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<&crate::protocol::MessageId> for ShortId {
    fn from(id: &crate::protocol::MessageId) -> Self {
        let mut b = [0u8; 8];
        b.copy_from_slice(&id.as_bytes()[..8]);
        ShortId(b)
    }
}

impl From<&crate::message::PeerId> for ShortId {
    fn from(id: &crate::message::PeerId) -> Self {
        let mut b = [0u8; 8];
        b.copy_from_slice(&id.as_bytes()[..8]);
        ShortId(b)
    }
}

/// Event names — the taxonomy (OBS_DESIGN.md). Centralized so the allow-list
/// of attributes per event lives in one place (privacy P3).
pub mod event {
    // MSG-001 message_engine — 9 events
    pub const MSG_CREATED: &str = "msg.created";
    pub const MSG_QUEUED: &str = "msg.queued";
    pub const MSG_SENT: &str = "msg.sent";
    pub const MSG_DELIVERED: &str = "msg.delivered";
    pub const MSG_ACKNOWLEDGED: &str = "msg.acknowledged";
    pub const MSG_EXPIRED: &str = "msg.expired";
    pub const MSG_DELIVERY_FAILED: &str = "msg.delivery_failed";
    pub const MSG_DROPPED_DUPLICATE: &str = "msg.dropped_duplicate";
    pub const MSG_FRAGMENTS_REASSEMBLED: &str = "msg.fragments_reassembled";
    pub const MSG_SENT_UNENCRYPTED: &str = "msg.sent_unencrypted";

    // EMERG-001 — 4 events
    pub const MSG_EMERGENCY_AUTH_DROPPED: &str = "emergency.auth_dropped";
    pub const MSG_EMERGENCY_SOS_RATE_LIMITED: &str = "emergency.sos_rate_limited";
    pub const MSG_EMERGENCY_BROADCAST_RELAYED: &str = "emergency.broadcast_relayed";
    pub const MSG_EMERGENCY_DRILL_SUPPRESSED: &str = "emergency.drill_suppressed";

    // ROUTE-001/002 — 5 events
    pub const ROUTE_DECISION: &str = "route.decision";
    pub const ROUTE_FLOOD: &str = "route.flood";
    pub const ROUTE_STORED: &str = "route.stored";
    pub const ROUTE_DROPPED: &str = "route.dropped";
    pub const ROUTE_PRUNED: &str = "route.pruned";

    // SCF-001 — 5 events
    pub const SCF_BUFFERED: &str = "scf.buffered";
    pub const SCF_AWAITING_CONTACT: &str = "scf.awaiting_contact";
    pub const SCF_FORWARD_ATTEMPT: &str = "scf.forward_attempt";
    pub const SCF_EVICTED: &str = "scf.evicted";
    pub const SCF_REAPED_EXPIRED: &str = "scf.reaped_expired";

    // GW-001 — 4 events
    pub const GW_ADOPTED: &str = "gw.adopted";
    pub const GW_WITHDRAWN: &str = "gw.withdrawn";
    pub const GW_HEALTH_TRANSITION: &str = "gw.health_transition";
    pub const GW_SELECTED: &str = "gw.selected";

    // Topology bus — 1 event
    pub const TOPO_EVENT: &str = "topo.event";

    // SEC-001 — 7 events
    pub const MSG_RATE_LIMITED: &str = "security.rate_limited";
    pub const MSG_QUOTA_EXCEEDED: &str = "security.quota_exceeded";
    pub const MSG_REPLAY_TOO_OLD: &str = "security.replay_too_old";
    pub const MSG_REPLAY_TOO_FUTURE: &str = "security.replay_too_future";
    pub const MSG_REPLAY_DETECTED: &str = "security.replay_detected";
    pub const MSG_LIKELY_SPAM: &str = "security.likely_spam";
    pub const MSG_EMERGENCY_ACL_DENIED: &str = "security.emergency_acl_denied";
}

/// Metric names — `iris.<domain>.<name>` grammar with unit suffix (RES-0012 §C).
pub mod metric {
    // SIM-22: ALL drives both MetricsRegistry::new() and the count assertion in
    // tests — add any new constant here AND nowhere else to register it.
    pub const ALL: &[&str] = &[
        MESSAGES_SENT_TOTAL,
        MESSAGES_DELIVERED_TOTAL,
        MESSAGES_RELAYED_TOTAL,
        MESSAGES_DROPPED_DUPLICATES_TOTAL,
        MESSAGES_EXPIRED_TOTAL,
        MESSAGES_DELIVERY_FAILED_TOTAL,
        MESSAGES_SENT_UNENCRYPTED_TOTAL,
        MESSAGES_EMERGENCY_AUTH_DROPPED_TOTAL,
        MESSAGES_EMERGENCY_SOS_RATE_LIMITED_TOTAL,
        MESSAGES_RATE_LIMITED_TOTAL,
        MESSAGES_QUOTA_EXCEEDED_TOTAL,
        MESSAGES_REPLAY_TOO_OLD_TOTAL,
        MESSAGES_REPLAY_TOO_FUTURE_TOTAL,
        MESSAGES_REPLAY_DETECTED_TOTAL,
        MESSAGES_LIKELY_SPAM_TOTAL,
        MESSAGES_EMERGENCY_ACL_DENIED_TOTAL,
        ROUTING_DECISIONS_TOTAL,
        ROUTING_FLOODS_TOTAL,
        SCF_EVICTIONS_TOTAL,
    ];

    pub const MESSAGES_SENT_TOTAL: &str = "iris.messages.sent_total";
    pub const MESSAGES_DELIVERED_TOTAL: &str = "iris.messages.delivered_total";
    pub const MESSAGES_RELAYED_TOTAL: &str = "iris.messages.relayed_total";
    pub const MESSAGES_DROPPED_DUPLICATES_TOTAL: &str = "iris.messages.dropped_duplicates_total";
    pub const MESSAGES_EXPIRED_TOTAL: &str = "iris.messages.expired_total";
    pub const MESSAGES_DELIVERY_FAILED_TOTAL: &str = "iris.messages.delivery_failed_total";
    /// EMERG-001: unverifiable EmergencyAlert dropped (anti-probing, AC-10).
    pub const MESSAGES_EMERGENCY_AUTH_DROPPED_TOTAL: &str =
        "iris.messages.emergency_auth_dropped_total";
    /// EMERG-001: SOS downgraded to P3 after the 3/hr allowance (AC-5).
    pub const MESSAGES_EMERGENCY_SOS_RATE_LIMITED_TOTAL: &str =
        "iris.messages.emergency_sos_rate_limited_total";
    /// Unicast sent WITHOUT encryption because the recipient X25519 key was
    /// absent from the key directory (RED-0002 fail-open — see threat model).
    /// P0 SOS broadcast is intentionally unencrypted and NOT counted here.
    pub const MESSAGES_SENT_UNENCRYPTED_TOTAL: &str = "iris.messages.sent_unencrypted_total";
    /// SEC-001: message silently dropped by rate limiter (per sender/class).
    pub const MESSAGES_RATE_LIMITED_TOTAL: &str = "iris.messages.rate_limited_total";
    /// SEC-001: message rejected by storage quota (per sender).
    pub const MESSAGES_QUOTA_EXCEEDED_TOTAL: &str = "iris.messages.quota_exceeded_total";
    /// SEC-001: replay protection — timestamp too old.
    pub const MESSAGES_REPLAY_TOO_OLD_TOTAL: &str = "iris.messages.replay_too_old_total";
    /// SEC-001: replay protection — timestamp too far in future.
    pub const MESSAGES_REPLAY_TOO_FUTURE_TOTAL: &str = "iris.messages.replay_too_future_total";
    /// SEC-001: replay protection — high-water mark replay detected.
    pub const MESSAGES_REPLAY_DETECTED_TOTAL: &str = "iris.messages.replay_detected_total";
    /// SEC-001: receiver-side likely_spam annotation.
    pub const MESSAGES_LIKELY_SPAM_TOTAL: &str = "iris.messages.likely_spam_total";
    /// SEC-001: emergency message denied by ACL-1.
    pub const MESSAGES_EMERGENCY_ACL_DENIED_TOTAL: &str =
        "iris.messages.emergency_acl_denied_total";
    pub const ROUTING_DECISIONS_TOTAL: &str = "iris.routing.decisions_total";
    pub const ROUTING_FLOODS_TOTAL: &str = "iris.routing.floods_total";
    pub const SCF_EVICTIONS_TOTAL: &str = "iris.scf.evictions_total";
    /// SYS-3: inbound messages dropped because a broadcast channel overran.
    /// Includes both state-stream and message-stream lag (MG-22 split pending).
    pub const TRANSPORT_INBOUND_LAGGED: &str = "iris.transport.inbound_lagged_total";
}

/// Lock-free counter-backed metrics registry (v1: counters only).
///
/// No global state: a registry is created and injected into the engines
/// (same pattern as `EngineMetrics`). Counters are `AtomicU64`, read on
/// demand at flush time (privacy/overhead P6).
///
/// `Clone` is cheap and shares the same counters — inject one registry into
/// several engines to accumulate a single mesh-wide telemetry view.
///
/// All registered counter names live in `metric::ALL`; that slice is the
/// single source of truth for both registration and the count assertion in
/// tests (SIM-22). v1 ships no gauge or histogram — see module-level note.
#[derive(Debug, Clone)]
pub struct MetricsRegistry {
    inner: Arc<RegistryInner>,
}

#[derive(Debug)]
struct RegistryInner {
    counters: HashMap<&'static str, AtomicU64>,
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsRegistry {
    /// A fresh registry with all counters pre-registered (zeroed).
    /// The set of counters is driven by `metric::ALL` — the single source.
    pub fn new() -> Self {
        let mut counters = HashMap::with_capacity(metric::ALL.len() + 4);
        for &name in metric::ALL {
            counters.insert(name, AtomicU64::new(0));
        }
        Self {
            inner: Arc::new(RegistryInner { counters }),
        }
    }

    /// Increment a counter by one (relaxed, lock-free, no allocation).
    ///
    /// # Panics (debug only)
    /// Panics in debug builds if `name` was not pre-registered in
    /// `metric::ALL` — a new `pub const` in `mod metric` must also appear
    /// in `ALL` (SIM-22).
    pub fn increment(&self, name: &'static str) {
        debug_assert!(
            self.inner.counters.contains_key(name),
            "unregistered metric name '{name}' — add it to metric::ALL"
        );
        if let Some(c) = self.inner.counters.get(name) {
            c.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Add a delta (used for bytes, batches, recipient counts).
    ///
    /// # Panics (debug only)
    /// Same contract as `increment` (SIM-22).
    pub fn add(&self, name: &'static str, delta: u64) {
        debug_assert!(
            self.inner.counters.contains_key(name),
            "unregistered metric name '{name}' — add it to metric::ALL"
        );
        if let Some(c) = self.inner.counters.get(name) {
            c.fetch_add(delta, Ordering::Relaxed);
        }
    }

    /// Point-in-time snapshot of all counters.
    ///
    /// Includes the global transport inbound-lag counter (SYS-3) so callers
    /// see a complete picture without needing a separate read path.
    pub fn snapshot(&self) -> HashMap<&'static str, u64> {
        let mut snap: HashMap<&'static str, u64> = self.inner
            .counters
            .iter()
            .map(|(k, v)| (*k, v.load(Ordering::Relaxed)))
            .collect();
        snap.insert(
            metric::TRANSPORT_INBOUND_LAGGED,
            crate::transport::inbound_lagged_total(),
        );
        snap
    }

    /// Reset all counters to zero (call at flush/rotation time).
    ///
    /// P5 (host responsibility): the embedder MUST call this at least once per
    /// 7-day window (e.g. in a periodic metric flush) so counters do not
    /// accumulate without bound. v1 ships no in-crate rotation timer.
    /// `iris-android/engine.rs`, `iris-ios/engine.rs`, and
    /// `iris-desktop/engine_handle.rs` each hold a registry and must call
    /// this method in their flush / maintenance tick (SIM-24).
    pub fn reset(&self) {
        for c in self.inner.counters.values() {
            c.store(0, Ordering::Relaxed);
        }
    }

    /// Derive a [`DeliveryWindow`] covering the interval between `prev`
    /// (a snapshot taken earlier) and now (SIM-23).
    ///
    /// `window_ms` is the elapsed wall-clock time in milliseconds between the
    /// two snapshots — callers supply this because the registry carries no
    /// clock. Returns zeroed counts if no messages were sent in the interval.
    pub fn window_since(
        &self,
        prev: &HashMap<&'static str, u64>,
        window_ms: u64,
    ) -> DeliveryWindow {
        let now = self.snapshot();
        let sent = now
            .get(metric::MESSAGES_SENT_TOTAL)
            .copied()
            .unwrap_or(0)
            .saturating_sub(*prev.get(metric::MESSAGES_SENT_TOTAL).unwrap_or(&0));
        let delivered = now
            .get(metric::MESSAGES_DELIVERED_TOTAL)
            .copied()
            .unwrap_or(0)
            .saturating_sub(
                *prev
                    .get(metric::MESSAGES_DELIVERED_TOTAL)
                    .unwrap_or(&0),
            );
        DeliveryWindow {
            window_ms,
            sent,
            delivered,
        }
    }
}

/// Derived 1-hour delivery window metrics (RES-0012 §C).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DeliveryWindow {
    pub window_ms: u64,
    pub sent: u64,
    pub delivered: u64,
}

impl DeliveryWindow {
    /// Delivery ratio in `0..=1`. Returns `0.0` when nothing was sent.
    pub fn ratio(&self) -> f64 {
        if self.sent == 0 {
            0.0
        } else {
            self.delivered as f64 / self.sent as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::MessagePriority;

    #[test]
    fn registry_increment_and_snapshot() {
        let reg = MetricsRegistry::new();
        reg.increment(metric::MESSAGES_SENT_TOTAL);
        reg.increment(metric::MESSAGES_SENT_TOTAL);
        reg.add(metric::ROUTING_DECISIONS_TOTAL, 3);
        let snap = reg.snapshot();
        assert_eq!(snap[metric::MESSAGES_SENT_TOTAL], 2);
        assert_eq!(snap[metric::ROUTING_DECISIONS_TOTAL], 3);
        // SIM-22: metric::ALL is the single registration source; snapshot len
        // must equal ALL.len() + 1 (the SYS-3 transport-lag global).
        assert_eq!(reg.snapshot().len(), metric::ALL.len() + 1);
    }

    #[test]
    fn registry_window_since_derives_delivery_window() {
        let reg = MetricsRegistry::new();
        let prev = reg.snapshot();
        reg.increment(metric::MESSAGES_SENT_TOTAL);
        reg.increment(metric::MESSAGES_SENT_TOTAL);
        reg.increment(metric::MESSAGES_DELIVERED_TOTAL);
        let w = reg.window_since(&prev, 3_600_000);
        assert_eq!(w.sent, 2);
        assert_eq!(w.delivered, 1);
        assert_eq!(w.window_ms, 3_600_000);
        assert!((w.ratio() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn registry_reset_zeroes() {
        let reg = MetricsRegistry::new();
        reg.increment(metric::MESSAGES_SENT_TOTAL);
        reg.reset();
        let snap = reg.snapshot();
        assert_eq!(snap[metric::MESSAGES_SENT_TOTAL], 0);
    }

    #[test]
    fn delivery_window_ratio() {
        let w = DeliveryWindow {
            window_ms: 3_600_000,
            sent: 4,
            delivered: 3,
        };
        assert!((w.ratio() - 0.75).abs() < 1e-9);
        let empty = DeliveryWindow::default();
        assert_eq!(empty.ratio(), 0.0);
    }

    #[test]
    fn short_id_is_truncated_and_prefix_of_full() {
        use crate::message::PeerId;
        use crate::protocol::MessageId;

        let id = MessageId([0xAB; 16]);
        let short = ShortId::from(&id);
        let s = short.to_string();
        assert_eq!(s.len(), 16, "8-byte prefix = 16 hex chars");
        assert!(id.to_string().starts_with(&s));
        assert_ne!(s, id.to_string());

        let peer = PeerId([0xCD; 32]);
        let sp = ShortId::from(&peer).to_string();
        assert_eq!(sp.len(), 16);
        assert!(peer.to_string().starts_with(&sp));

        // Display never exceeds the truncated form.
        assert_eq!(format!("{short}"), s);
        assert!(format!("{:?}", ShortId::from(&id)).len() >= 16);
    }

    #[test]
    fn priority_label_is_stable_u8() {
        assert_eq!(MessagePriority::P0.as_u8(), 0);
        assert_eq!(MessagePriority::P4.as_u8(), 4);
        assert_eq!(MessagePriority::P7.as_u8(), 7);
    }
}
