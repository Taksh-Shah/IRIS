//! Emergency audit trail — EMERG-001 (AC-9).
//!
//! Pseudonymous metadata-only ledger for emergency events. **No payload
//! content** (no text, no coordinates, no identities) is ever written:
//! subjects are SHA-256 prefixes. Rows carry a compact reason tag. The v1
//! store is an in-memory ring; production adapters swap in disk logging
//! through the same record type.

/// Emergency audit event kinds (wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum AuditEvent {
    /// SOS downgraded by the rate limiter (AC-5).
    SosRateLimited = 1,
    /// Authority broadcast dropped (auth/scope/validity failure) (AC-2/3).
    BroadcastAuthDropped = 2,
    /// SOS cancel rejected (unknown/window/missing) (AC-6).
    SosCancelRejected = 3,
    /// Disaster-mode transition applied (AC-7).
    ModeTransition = 4,
    /// Drill (TEST_MODE) processed — never surfaced (AC-10).
    DrillProcessed = 5,
    /// Verified authority broadcast relayed (AC-8).
    BroadcastRelayed = 6,
    /// Verified broadcast seen before (same broadcast_id) — replay suppressed
    /// by the bounded flood guard (RT-006).
    BroadcastReplayDropped = 7,
}

impl AuditEvent {
    pub fn from_u8(v: u8) -> Option<AuditEvent> {
        Some(match v {
            1 => AuditEvent::SosRateLimited,
            2 => AuditEvent::BroadcastAuthDropped,
            3 => AuditEvent::SosCancelRejected,
            4 => AuditEvent::ModeTransition,
            5 => AuditEvent::DrillProcessed,
            6 => AuditEvent::BroadcastRelayed,
            7 => AuditEvent::BroadcastReplayDropped,
            _ => return None,
        })
    }
}

/// One audit row — metadata only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyAuditRecord {
    /// Unix seconds.
    pub timestamp: u64,
    pub event: AuditEvent,
    /// Pseudonymous subject (16-byte SHA-256 prefix).
    pub subject: [u8; 16],
    /// Compact reason tag (≤ 64 chars; never message content).
    pub note: Option<String>,
}

/// In-memory bounded audit log (ring capacity).
#[derive(Debug, Clone)]
pub struct AuditLog {
    capacity: usize,
    records: Vec<EmergencyAuditRecord>,
}

impl AuditLog {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            records: Vec::with_capacity(capacity.max(1)),
        }
    }

    pub fn append(&mut self, record: EmergencyAuditRecord) {
        if self.records.len() >= self.capacity {
            let overflow = 1 + self.records.len() - self.capacity;
            self.records.drain(..overflow);
        }
        self.records.push(record);
    }

    /// Snapshot for UI/diagnostics (audit rows are metadata-safe to display).
    pub fn snapshot(&self) -> &[EmergencyAuditRecord] {
        &self.records
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

impl Default for AuditLog {
    fn default() -> Self {
        Self::new(1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(i: u8) -> EmergencyAuditRecord {
        EmergencyAuditRecord {
            timestamp: u64::from(i),
            event: AuditEvent::BroadcastRelayed,
            subject: [i; 16],
            note: None,
        }
    }

    #[test]
    fn ring_bounds_capacity() {
        let mut log = AuditLog::new(3);
        for i in 0..6 {
            log.append(rec(i));
        }
        assert_eq!(log.len(), 3);
        let snap = log.snapshot();
        assert_eq!(snap[0].timestamp, 3);
        assert_eq!(snap[2].timestamp, 5);
    }

    #[test]
    fn event_codes_round_trip() {
        for e in [
            AuditEvent::SosRateLimited,
            AuditEvent::BroadcastAuthDropped,
            AuditEvent::SosCancelRejected,
            AuditEvent::ModeTransition,
            AuditEvent::DrillProcessed,
            AuditEvent::BroadcastRelayed,
            AuditEvent::BroadcastReplayDropped,
        ] {
            assert_eq!(AuditEvent::from_u8(e as u8), Some(e));
        }
        assert_eq!(AuditEvent::from_u8(0), None);
    }
}
