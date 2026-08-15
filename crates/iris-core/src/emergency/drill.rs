//! TEST_MODE drill helpers — EMERG-001 (AC-10, DEC-EMERG-0008).
//!
//! Drills are real broadcasts carrying the drill marker (`drill == true` and/or
//! `message_type == Drill` and/or `severity == Test`). They exercise the full
//! verification + relay path but **must never surface** on an OS alert layer.
//! The `surface_decision` helper gives the engine the single gate.

use crate::emergency::model::{AlertMessageType, EmergencyBroadcast, Severity};

/// Whether a decoded broadcast is a drill (TEST_MODE).
pub fn is_drill(b: &EmergencyBroadcast) -> bool {
    b.drill || b.message_type == AlertMessageType::Drill || b.severity == Severity::Test
}

/// What the app layer should do with a verified broadcast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceDecision {
    /// Real alert — render on the OS alert layer.
    Surface,
    /// Drill — verify + relay but never surface.
    DoNotSurface,
}

/// Singular gate for the OS-surface path (DEC-EMERG-0008).
pub fn surface_decision(b: &EmergencyBroadcast) -> SurfaceDecision {
    if is_drill(b) {
        SurfaceDecision::DoNotSurface
    } else {
        SurfaceDecision::Surface
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emergency::model::{AuthorityMeta, Certainty};

    fn base() -> EmergencyBroadcast {
        EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [1u8; 16],
            republish_id: None,
            issued_at: 1_000,
            expires_at: 1_000 + 3600,
            severity: Severity::Severe,
            certainty: Certainty::Likely,
            message_type: AlertMessageType::Alert,
            area_code: "IN-GJ".into(),
            language: "en".into(),
            headline: "h".into(),
            instructions: None,
            authority: AuthorityMeta {
                issuer_name: None,
                geo_scope: None,
                functional_scope: None,
                max_severity: 4,
            },
            authority_peer_short: None,
            drill: false,
        }
    }

    #[test]
    fn real_alert_surfaces() {
        let b = base();
        assert!(!is_drill(&b));
        assert_eq!(surface_decision(&b), SurfaceDecision::Surface);
    }

    #[test]
    fn drill_flag_suppresses() {
        let mut b = base();
        b.drill = true;
        assert!(is_drill(&b));
        assert_eq!(surface_decision(&b), SurfaceDecision::DoNotSurface);
    }

    #[test]
    fn drill_message_type_suppresses() {
        let mut b = base();
        b.message_type = AlertMessageType::Drill;
        assert!(is_drill(&b));
        assert_eq!(surface_decision(&b), SurfaceDecision::DoNotSurface);
    }

    #[test]
    fn test_severity_suppresses() {
        let mut b = base();
        b.severity = Severity::Test;
        assert!(is_drill(&b));
        assert_eq!(surface_decision(&b), SurfaceDecision::DoNotSurface);
    }
}