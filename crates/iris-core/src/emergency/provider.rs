//! EmergencyProvider seam — EMERG-001 (AC-11).
//!
//! The message engine depends on [`EmergencyProvider`] by trait, never on a
//! concrete type. The engine's default is [`NoopEmergencyProvider`] — a
//! mechanical no-op that keeps behavior identical to pre-EMERG-001 until a
//! real provider (`EmergencyGateway`) is installed via
//! `MessageEngine::set_emergency_provider`.
//!
//! Seams the engine calls:
//! - `classify`/`verify_envelope` on the incoming emergency path,
//! - `sos_rate` / `sos_record` / `sos_reset` for the AC-5/AC-6 gates,
//! - `audit` for the AC-9 metadata ledger.

use crate::emergency::audit::AuditEvent;
use crate::emergency::sos::{classify_sos, OriginalSos, SosOutcome};
use crate::protocol::envelope::Envelope;

use super::rate_limit::RateLimitDecision;
use super::{AuditLog, EmergencyAuditRecord};

/// Errors surfaced by the emergency provider seam.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmergencyError {
    #[error("emergency: provider not armed")]
    NotArmed,
    #[error("emergency: {0}")]
    Other(String),
}

/// The engine-facing contract for the emergency subsystem.
pub trait EmergencyProvider: Send + Sync {
    /// Whether a real provider is installed (drives engine behavior switch).
    fn armed(&self) -> bool {
        true
    }

    /// Classify an incoming SOS payload (field 13) for a sender.
    ///
    /// `original` — the ledger record for the referenced original SOS (PM-2):
    ///   resolved by the engine via `sos_original`; `None` → cancel rejected.
    /// `cancel_sender_short` — 16-byte short ID of the cancel envelope's sender
    ///   (PM-15 same-signer check); ignored for `Sos`/`Test` payloads.
    fn classify_sos(
        &self,
        payload: &[u8],
        now_unix: u64,
        original: Option<OriginalSos>,
        cancel_sender_short: [u8; 16],
    ) -> Result<SosOutcome, crate::emergency::SosError>;

    /// Rate-limit decision for a sender's SOS (AC-5).
    fn sos_rate(&self, sender_id: &[u8], now_unix: u64) -> RateLimitDecision;

    /// Record an accepted SOS for a sender (AC-5 bookkeeping).
    fn sos_record(&self, sender_id: &[u8], now_unix: u64);

    /// Atomically check the rate limit and record in a single lock acquisition
    /// (PM-18). Default delegates to `sos_rate` + `sos_record` (two locks, safe
    /// for implementations without a shared mutex).
    fn sos_check_and_record(&self, sender_id: &[u8], now_unix: u64) -> RateLimitDecision {
        let decision = self.sos_rate(sender_id, now_unix);
        self.sos_record(sender_id, now_unix);
        decision
    }

    /// Register an accepted SOS in the cancel ledger (PM-2).
    /// Called by the engine after `classify_sos` returns `AcceptedSos`.
    /// Default: no-op (unarmed provider stores nothing).
    fn sos_register(&self, _message_id: [u8; 16], _timestamp: u64, _sender_id: &[u8]) {}

    /// Look up a previously accepted SOS by its message_id (PM-2 cancel path).
    /// Returns `None` when the id is unknown or the cancel window has expired.
    /// Default: returns `None` (unarmed provider has no ledger).
    fn sos_original(&self, _message_id: [u8; 16]) -> Option<OriginalSos> {
        None
    }

    /// Reset a sender's SOS window (verified CANCEL, AC-6).
    fn sos_reset(&self, sender_id: &[u8]);

    /// Verify + classify an incoming EmergencyAlert envelope (AC-2/3/8/10).
    fn verify_envelope(
        &self,
        envelope: &Envelope,
        now_unix: u64,
    ) -> crate::emergency::VerifyOutcome;

    /// Write a metadata-only audit row (AC-9).
    fn audit(&self, record: EmergencyAuditRecord);
}

/// Default unarmed provider: mechanical no-ops that preserve pre-emergency
/// engine behavior (AC-11). Every hard guarantee is off until armed.
#[derive(Debug, Clone, Default)]
pub struct NoopEmergencyProvider;

impl NoopEmergencyProvider {
    pub fn new() -> Self {
        Self
    }
}

impl EmergencyProvider for NoopEmergencyProvider {
    fn armed(&self) -> bool {
        false
    }

    fn classify_sos(
        &self,
        _payload: &[u8],
        _now_unix: u64,
        _original: Option<OriginalSos>,
        _cancel_sender_short: [u8; 16],
    ) -> Result<SosOutcome, crate::emergency::SosError> {
        Ok(SosOutcome::AcceptedSos { cancel_deadline: 0 })
    }

    fn sos_rate(&self, _sender_id: &[u8], _now_unix: u64) -> RateLimitDecision {
        RateLimitDecision::Allowed { remaining: 3 }
    }

    fn sos_record(&self, _sender_id: &[u8], _now_unix: u64) {}

    fn sos_reset(&self, _sender_id: &[u8]) {}

    fn verify_envelope(
        &self,
        _envelope: &Envelope,
        _now_unix: u64,
    ) -> crate::emergency::VerifyOutcome {
        // Unarmed: drop with a clear, auditable reason.
        crate::emergency::VerifyOutcome::Drop("emergency provider not armed".into())
    }

    fn audit(&self, _record: EmergencyAuditRecord) {}
}

/// Composed v1 provider: chains + SOS gates + rate limit + audit + mode, all
/// backed by a `TrustStore`. Installed by the engine operator when EMERG-001
/// is active (`MessageEngine::set_emergency_provider`).
///
/// This is the thin coordination layer; the heavy logic lives in
/// `authority` / `sos` / `rate_limit` / `mode` / `broadcast`.
#[derive(Debug, Clone)]
pub struct EmergencyGateway {
    trust: crate::identity::trust_store::TrustStore,
    limiter: std::sync::Arc<std::sync::Mutex<super::SosRateLimiter>>,
    audit: std::sync::Arc<std::sync::Mutex<AuditLog>>,
    replay_guard: std::sync::Arc<std::sync::Mutex<BroadcastReplayGuard>>,
    /// PM-2/PM-15: ledger of accepted SOS messages for cancel verification.
    /// Keyed by message_id bytes; value is (timestamp, sender_short_id).
    /// Entries are evicted when the 60-minute cancel window elapses.
    sos_ledger: std::sync::Arc<std::sync::Mutex<SosLedger>>,
}

impl EmergencyGateway {
    pub fn new(trust: crate::identity::trust_store::TrustStore, audit_capacity: usize) -> Self {
        Self {
            trust,
            limiter: std::sync::Arc::new(std::sync::Mutex::new(super::SosRateLimiter::new())),
            audit: std::sync::Arc::new(std::sync::Mutex::new(AuditLog::new(audit_capacity))),
            replay_guard: std::sync::Arc::new(std::sync::Mutex::new(BroadcastReplayGuard::new())),
            sos_ledger: std::sync::Arc::new(std::sync::Mutex::new(SosLedger::new())),
        }
    }

    /// Snapshot of the audit ring (UI/diagnostics).
    pub fn audit_snapshot(&self) -> Vec<EmergencyAuditRecord> {
        self.audit
            .lock()
            .map(|mut a| a.snapshot().to_vec())
            .unwrap_or_default()
    }
}

impl EmergencyProvider for EmergencyGateway {
    fn armed(&self) -> bool {
        true
    }

    fn classify_sos(
        &self,
        payload: &[u8],
        now_unix: u64,
        original: Option<OriginalSos>,
        cancel_sender_short: [u8; 16],
    ) -> Result<SosOutcome, crate::emergency::SosError> {
        classify_sos(payload, now_unix, original, cancel_sender_short)
    }

    fn sos_register(&self, message_id: [u8; 16], timestamp: u64, sender_id: &[u8]) {
        let sender_short = crate::identity::peer_short_from_sender(sender_id);
        let now = crate::message_engine::expiry::unix_now();
        if let Ok(mut ledger) = self.sos_ledger.lock() {
            ledger.insert(message_id, timestamp, sender_short, now);
        }
    }

    fn sos_original(&self, message_id: [u8; 16]) -> Option<OriginalSos> {
        let now = crate::message_engine::expiry::unix_now();
        self.sos_ledger
            .lock()
            .ok()
            .and_then(|mut ledger| ledger.lookup(message_id, now))
    }

    fn sos_rate(&self, sender_id: &[u8], now_unix: u64) -> RateLimitDecision {
        let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
        limiter.check(&short_of(sender_id, 16), now_unix)
    }

    fn sos_record(&self, sender_id: &[u8], now_unix: u64) {
        let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
        limiter.record(&short_of(sender_id, 16), now_unix);
    }

    /// PM-18: single lock acquisition for check+record to eliminate the
    /// TOCTOU race where two concurrent SOS from the same sender could both
    /// see `Allowed` before either records.
    fn sos_check_and_record(&self, sender_id: &[u8], now_unix: u64) -> RateLimitDecision {
        let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
        let decision = limiter.check(&short_of(sender_id, 16), now_unix);
        limiter.record(&short_of(sender_id, 16), now_unix);
        decision
    }

    fn sos_reset(&self, sender_id: &[u8]) {
        let mut limiter = self.limiter.lock().unwrap_or_else(|e| e.into_inner());
        limiter.reset(&short_of(sender_id, 16));
    }

    fn verify_envelope(
        &self,
        envelope: &Envelope,
        now_unix: u64,
    ) -> crate::emergency::VerifyOutcome {
        let outcome = crate::emergency::verify_and_classify(envelope, &self.trust, now_unix);
        let replay_id = match &outcome {
            crate::emergency::VerifyOutcome::Relay { .. }
            | crate::emergency::VerifyOutcome::Suppressed { .. } => {
                crate::emergency::codec::decode_broadcast(&envelope.payload)
                    .ok()
                    .map(|b| b.republish_id.unwrap_or(b.broadcast_id))
            }
            _ => None,
        };
        if let Some(id) = replay_id {
            // EMERG-RT-006: suppress verified-broadcast replays (same
            // broadcast_id within retention) so a fresh-message_id re-encode
            // cannot flood relay/surface. One extra decode on the
            // already-verified path is acceptable here.
            let mut guard = self.replay_guard.lock().unwrap_or_else(|e| e.into_inner());
            if !guard.check_and_record(id, now_unix) {
                drop(guard);
                self.audit_entry(AuditEvent::BroadcastReplayDropped, envelope, None);
                return crate::emergency::VerifyOutcome::Drop(
                    "verified broadcast replay suppressed (RT-006)".into(),
                );
            }
        }
        match &outcome {
            crate::emergency::VerifyOutcome::Relay { .. } => {
                self.audit_entry(AuditEvent::BroadcastRelayed, envelope, None);
            }
            crate::emergency::VerifyOutcome::Suppressed { .. } => {
                self.audit_entry(AuditEvent::DrillProcessed, envelope, None);
            }
            crate::emergency::VerifyOutcome::Drop(reason) => {
                self.audit_entry(
                    AuditEvent::BroadcastAuthDropped,
                    envelope,
                    Some(reason.clone()),
                );
            }
        }
        outcome
    }

    fn audit(&self, record: EmergencyAuditRecord) {
        if let Ok(mut a) = self.audit.lock() {
            a.append(record);
        }
    }
}

impl EmergencyGateway {
    fn audit_entry(&self, event: AuditEvent, envelope: &Envelope, note: Option<String>) {
        let subject = crate::emergency::authority::authority_short_id(&envelope.sender_id);
        let timestamp = crate::message_engine::expiry::unix_now();
        self.audit(EmergencyAuditRecord {
            timestamp,
            event,
            subject,
            note,
        });
    }
}

/// First `n` bytes of an id (rate-limit key: 16-byte prefix is a strong per
/// sender fingerprint for the window bucketing; full identity stays off the
/// audit trail).
fn short_of(id: &[u8], n: usize) -> [u8; 16] {
    let mut out = [0u8; 16];
    let take = n.min(id.len()).min(out.len());
    out[..take].copy_from_slice(&id[..take]);
    out
}

/// PM-2/PM-15: bounded ledger of accepted SOS messages for cancel verification.
///
/// Keyed by the SOS envelope's `message_id` bytes (`[u8; 16]`).
/// Value: `(timestamp, sender_short)` — the original SOS's UNIX timestamp and
/// the 16-byte short sender derived via `peer_short_from_sender`.
///
/// Entries are evicted when the 60-minute cancel window elapses, bounded at
/// `MAX_ENTRIES` for defence against a flood of unique SOS ids.
#[derive(Debug, Default)]
struct SosLedger {
    entries: std::collections::HashMap<[u8; 16], (u64, [u8; 16])>,
}

impl SosLedger {
    /// Hard cap — at 1 SOS/sec that is 1 hour of distinct senders.
    const MAX_ENTRIES: usize = 3_600;

    fn new() -> Self {
        Self::default()
    }

    fn insert(&mut self, message_id: [u8; 16], timestamp: u64, sender_short: [u8; 16], now: u64) {
        // Evict stale entries on every insert to keep the map bounded.
        let cutoff = now.saturating_sub(super::sos::SOS_CANCEL_WINDOW_SECS);
        self.entries.retain(|_, (ts, _)| *ts >= cutoff);
        // Drop oldest entry if still at capacity (edge case: many messages in one second).
        if self.entries.len() >= Self::MAX_ENTRIES {
            // Remove an arbitrary entry — the map is already bounded; fairness
            // is not a priority here.
            if let Some(key) = self.entries.keys().next().copied() {
                self.entries.remove(&key);
            }
        }
        self.entries.insert(message_id, (timestamp, sender_short));
    }

    fn lookup(&mut self, message_id: [u8; 16], now: u64) -> Option<OriginalSos> {
        let cutoff = now.saturating_sub(super::sos::SOS_CANCEL_WINDOW_SECS);
        match self.entries.get(&message_id) {
            Some(&(ts, sender_id)) if ts >= cutoff => Some(OriginalSos { timestamp: ts, sender_id }),
            Some(_) => {
                // Entry exists but is outside the cancel window — evict it.
                self.entries.remove(&message_id);
                None
            }
            None => None,
        }
    }
}

/// Bounded broadcast replay guard (EMERG-RT-006, MEDIUM): the engine's dedup
/// drops same-message_id replays, but a hostile peer could re-encode a valid
/// alert with a fresh message_id (same `broadcast_id` + chain) and flood the
/// relay/surface path. This guard drops a verified EmergencyAlert whose
/// `broadcast_id` was relayed within the retention window, bounded at
/// [`BroadcastReplayGuard::MAX_IDS`] (1024) with plain-id eviction. A
/// republish (`republish_id` differing from `broadcast_id`) is a distinct ID
/// and is allowed.
#[derive(Debug, Default)]
pub struct BroadcastReplayGuard {
    /// Most-recently-seen broadcast ids (evicted from the front).
    ids: Vec<[u8; 16]>,
    /// Unix second each id was last seen (window expiration).
    seen_at: Vec<u64>,
    /// Retention in seconds (5 min — alerts republish on a slower cadence).
    retention_secs: u64,
}

impl BroadcastReplayGuard {
    /// Bounded set size (id namespace 2^128 makes collisions negligible for
    /// memory; this caps hostile unique-id floods at a few KB).
    pub const MAX_IDS: usize = 1024;
    const DEFAULT_RETENTION_SECS: u64 = 300;

    pub fn new() -> Self {
        Self {
            ids: Vec::with_capacity(Self::MAX_IDS),
            seen_at: Vec::with_capacity(Self::MAX_IDS),
            retention_secs: Self::DEFAULT_RETENTION_SECS,
        }
    }

    /// Register a relayed broadcast_id. Returns `true` the FIRST time (within
    /// retention) — the alert is allowed; `false` on a replay within the
    /// window → suppress.
    pub fn check_and_record(&mut self, broadcast_id: [u8; 16], now_unix: u64) -> bool {
        let cutoff = now_unix.saturating_sub(self.retention_secs);
        // Drop expired entries in place (amortized O(n); n ≤ MAX_IDS).
        let mut i = 0;
        while i < self.ids.len() {
            if self.seen_at[i] < cutoff {
                self.ids.swap_remove(i);
                self.seen_at.swap_remove(i);
            } else {
                i += 1;
            }
        }
        if let Some(pos) = self.ids.iter().position(|id| *id == broadcast_id) {
            self.seen_at[pos] = now_unix;
            return false;
        }
        if self.ids.len() >= Self::MAX_IDS {
            self.ids.remove(0);
            self.seen_at.remove(0);
        }
        self.ids.push(broadcast_id);
        self.seen_at.push(now_unix);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_provider_is_unarmed_and_permissive() {
        let p = NoopEmergencyProvider::new();
        assert!(!p.armed());
        assert_eq!(
            p.sos_rate(&[1u8; 32], 1_000),
            RateLimitDecision::Allowed { remaining: 3 }
        );
        assert!(matches!(
            p.classify_sos(&[0xde, 0xad], 1_000, None, [0u8; 16]),
            Ok(SosOutcome::AcceptedSos { .. })
        ));
    }

    #[test]
    fn gateway_rate_limit_gates_after_third() {
        let trust = crate::identity::trust_store::TrustStore::new();
        let gw = EmergencyGateway::new(trust, 64);
        let sender = [1u8; 32];
        for i in 0usize..3 {
            assert_eq!(
                gw.sos_rate(&sender, 1_000 + i as u64),
                RateLimitDecision::Allowed {
                    remaining: 3usize - i
                }
            );
            gw.sos_record(&sender, 1_000 + i as u64);
        }
        assert!(matches!(
            gw.sos_rate(&sender, 1_003),
            RateLimitDecision::Downgraded { .. }
        ));
        gw.sos_reset(&sender);
        assert_eq!(
            gw.sos_rate(&sender, 1_004),
            RateLimitDecision::Allowed { remaining: 3 }
        );
    }

    #[test]
    fn replay_guard_suppresses_same_broadcast_id_until_retention_elapses() {
        // EMERG-RT-006: a verified alert with the same broadcast_id must not
        // be relayed twice within the retention window; a republish using a
        // different id is a distinct alert.
        let mut g = BroadcastReplayGuard::new();
        assert!(g.check_and_record([1u8; 16], 1_000), "first sight allowed");
        assert!(
            !g.check_and_record([1u8; 16], 1_005),
            "replay within window suppressed"
        );
        // Republish id differs → distinct.
        assert!(
            g.check_and_record([2u8; 16], 1_010),
            "republish id is distinct"
        );
        // A different broadcast_id is allowed.
        assert!(g.check_and_record([3u8; 16], 1_011), "fresh id allowed");
        // After retention elapses, a new sight of [1;16] is allowed again.
        assert!(
            g.check_and_record([1u8; 16], 1_000 + 400),
            "allowed after retention"
        );
    }

    #[test]
    fn replay_guard_bounded_at_max() {
        // RT-006: hostile unique-id floods must not grow the set without bound.
        let mut g = BroadcastReplayGuard::new();
        for i in 0..(BroadcastReplayGuard::MAX_IDS + 64) {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&(i as u64).to_le_bytes());
            assert!(g.check_and_record(id, 1_000), "fresh id i={i} allowed");
            assert!(g.ids.len() <= BroadcastReplayGuard::MAX_IDS);
        }
        assert_eq!(g.ids.len(), BroadcastReplayGuard::MAX_IDS);
    }

    #[test]
    fn gateway_audit_ring_captures_drops() {
        let trust = crate::identity::trust_store::TrustStore::new();
        let gw = EmergencyGateway::new(trust, 8);
        // Unauthenticated emergency envelope → drop + audit.
        let env = Envelope {
            version: 1,
            message_id: crate::protocol::message_id::MessageId::new_v7(),
            sender_id: vec![0x42u8; 32],
            recipient_id: crate::protocol::EMERGENCY_BROADCAST.to_vec(),
            priority: crate::message::MessagePriority::P3,
            ttl_seconds: 3600,
            timestamp: 1_000,
            hop_count: 0,
            max_hops: Some(5),
            payload_type: crate::protocol::ContentType::EmergencyAlert,
            payload_size: 0,
            payload_hash: [0u8; 32],
            payload: vec![0xde, 0xad],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        let out = gw.verify_envelope(&env, 1_000);
        assert!(matches!(out, crate::emergency::VerifyOutcome::Drop(_)));
        let snap = gw.audit_snapshot();
        assert!(!snap.is_empty());
        assert!(snap
            .iter()
            .any(|r| r.event == AuditEvent::BroadcastAuthDropped));
    }
}
