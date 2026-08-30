//! Disaster mode state machine — EMERG-001 (AC-7, DISASTER_MODES.md).
//!
//! Node-local mode ladder: `Normal → Emergency → Crisis`, with `Degraded`
//! reached when infrastructure loss has passed and queues empty (de-escalation
//! path). Transitions are **guarded**: escalations need a 15-minute hold to
//! avoid flapping (DEC-EMERG-0006), and authority deactivation bypasses the
//! hold (verified CANCEL/deactivate). Modes change message priority buckets
//! and LoRa duty behavior in the engine via the provider seam.

/// Disaster mode (wire codes fixed; DM_MODES.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum DisasterMode {
    /// Normal operations.
    Normal = 0,
    /// Local emergency active (SOS density elevated).
    Emergency = 1,
    /// Full crisis (broadcast storm / infrastructure loss).
    Crisis = 2,
    /// Post-crisis recovery (degraded but stable).
    Degraded = 3,
}

impl DisasterMode {
    pub fn from_u8(v: u8) -> Option<DisasterMode> {
        Some(match v {
            0 => DisasterMode::Normal,
            1 => DisasterMode::Emergency,
            2 => DisasterMode::Crisis,
            3 => DisasterMode::Degraded,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Human label for diagnostics/audit.
    pub fn label(self) -> &'static str {
        match self {
            DisasterMode::Normal => "NORMAL",
            DisasterMode::Emergency => "EMERGENCY",
            DisasterMode::Crisis => "CRISIS",
            DisasterMode::Degraded => "DEGRADED",
        }
    }
}

/// Minimum time a mode must hold before an *escalation* is permitted (anti-flap).
pub const ESCALATION_HOLD_SECS: u64 = 900; // 15 min

/// Trigger thresholds (EMERG_DESIGN.md §8 activation inputs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Triggers {
    /// SOS density per km² per 10 min.
    pub sos_density_per_km2: f64,
    /// Gateway capacity share (0..1) vs 7-day average.
    pub gateway_capacity_share: f64,
    /// Inbound rate multiplier vs 30-min baseline.
    pub inbound_rate_multiplier: f64,
}

impl Triggers {
    pub const SOS_DENSITY_THRESHOLD: f64 = 10.0; // 10 / km² / 10 min
    pub const GATEWAY_CAPACITY_MIN_SHARE: f64 = 0.10; // 10% of 7d avg
    pub const INBOUND_RATE_THRESHOLD: f64 = 10.0; // 10× baseline / 30 min
}

/// A decided (guarded) mode transition for the engine to apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeTransition {
    pub from: DisasterMode,
    pub to: DisasterMode,
    pub at_unix: u64,
    /// Reason tag for audit (e.g. "sos-density", "gateway-capacity",
    /// "inbound-rate", "authority-deactivate").
    pub reason: &'static str,
}

fn any_trigger_active(t: &Triggers) -> bool {
    t.sos_density_per_km2 >= Triggers::SOS_DENSITY_THRESHOLD
        || t.gateway_capacity_share <= Triggers::GATEWAY_CAPACITY_MIN_SHARE
        || t.inbound_rate_multiplier >= Triggers::INBOUND_RATE_THRESHOLD
}

fn escalation_reason(t: &Triggers) -> &'static str {
    if t.sos_density_per_km2 >= Triggers::SOS_DENSITY_THRESHOLD {
        "sos-density"
    } else if t.gateway_capacity_share <= Triggers::GATEWAY_CAPACITY_MIN_SHARE {
        "gateway-capacity"
    } else {
        "inbound-rate"
    }
}

/// Guarded transition decision.
///
/// * Authority deactivation wins immediately (bypasses hold).
/// * Escalation (Normal → Emergency, Emergency → Crisis) requires the hold to
///   have elapsed AND at least one trigger above its threshold (anti-flap).
/// * De-escalation (Crisis → Degraded → Normal) follows the ladder only after
///   the hold; Crisis → Degraded requires triggers to be inactive.
pub fn guarded_transition(
    current: DisasterMode,
    last_transition_at_unix: Option<u64>,
    now_unix: u64,
    authority_deactivate: bool,
    triggers: &Triggers,
) -> Option<ModeTransition> {
    // Authority deactivation wins immediately (no hold required).
    if authority_deactivate && current != DisasterMode::Normal {
        return Some(ModeTransition {
            from: current,
            to: DisasterMode::Normal,
            at_unix: now_unix,
            reason: "authority-deactivate",
        });
    }

    let hold_elapsed =
        now_unix.saturating_sub(last_transition_at_unix.unwrap_or(0)) >= ESCALATION_HOLD_SECS;
    let triggered = any_trigger_active(triggers);

    match current {
        // Escalation path (PM-7: was missing).
        DisasterMode::Normal if hold_elapsed && triggered => Some(ModeTransition {
            from: current,
            to: DisasterMode::Emergency,
            at_unix: now_unix,
            reason: escalation_reason(triggers),
        }),
        DisasterMode::Emergency if hold_elapsed && triggered => Some(ModeTransition {
            from: current,
            to: DisasterMode::Crisis,
            at_unix: now_unix,
            reason: escalation_reason(triggers),
        }),
        // De-escalation path.
        DisasterMode::Crisis if hold_elapsed && !triggered => Some(ModeTransition {
            from: current,
            to: DisasterMode::Degraded,
            at_unix: now_unix,
            reason: "de-escalation",
        }),
        DisasterMode::Degraded if hold_elapsed => Some(ModeTransition {
            from: current,
            to: DisasterMode::Normal,
            at_unix: now_unix,
            reason: "recovery",
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_TRIGGERS: Triggers = Triggers {
        sos_density_per_km2: 0.0,
        gateway_capacity_share: 1.0,
        inbound_rate_multiplier: 1.0,
    };

    const HIGH_TRIGGERS: Triggers = Triggers {
        sos_density_per_km2: Triggers::SOS_DENSITY_THRESHOLD,
        gateway_capacity_share: Triggers::GATEWAY_CAPACITY_MIN_SHARE,
        inbound_rate_multiplier: Triggers::INBOUND_RATE_THRESHOLD,
    };

    #[test]
    fn wire_codes_round_trip() {
        for m in [
            DisasterMode::Normal,
            DisasterMode::Emergency,
            DisasterMode::Crisis,
            DisasterMode::Degraded,
        ] {
            assert_eq!(DisasterMode::from_u8(m.as_u8()), Some(m));
        }
        assert_eq!(DisasterMode::from_u8(9), None);
    }

    #[test]
    fn deactivate_bypasses_hold() {
        let t = guarded_transition(DisasterMode::Crisis, Some(1_000), 1_005, true, &NO_TRIGGERS);
        assert_eq!(
            t,
            Some(ModeTransition {
                from: DisasterMode::Crisis,
                to: DisasterMode::Normal,
                at_unix: 1_005,
                reason: "authority-deactivate",
            })
        );
    }

    #[test]
    fn deactivate_only_when_not_normal() {
        assert_eq!(
            guarded_transition(DisasterMode::Normal, Some(1_000), 1_005, true, &NO_TRIGGERS),
            None
        );
    }

    #[test]
    fn degraded_recovers_after_hold() {
        let t = guarded_transition(
            DisasterMode::Degraded,
            Some(1_000),
            1_000 + 900,
            false,
            &NO_TRIGGERS,
        );
        assert!(matches!(
            t,
            Some(ModeTransition {
                to: DisasterMode::Normal,
                ..
            })
        ));
        // Too early.
        let t = guarded_transition(
            DisasterMode::Degraded,
            Some(1_000),
            1_000 + 899,
            false,
            &NO_TRIGGERS,
        );
        assert!(t.is_none());
    }

    // PM-7: escalation paths were missing.
    #[test]
    fn escalation_normal_to_emergency_after_hold() {
        let t = guarded_transition(
            DisasterMode::Normal,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &HIGH_TRIGGERS,
        );
        assert_eq!(
            t,
            Some(ModeTransition {
                from: DisasterMode::Normal,
                to: DisasterMode::Emergency,
                at_unix: ESCALATION_HOLD_SECS,
                reason: "sos-density",
            })
        );
        // Before hold: no transition even with active triggers.
        let t = guarded_transition(
            DisasterMode::Normal,
            Some(0),
            ESCALATION_HOLD_SECS - 1,
            false,
            &HIGH_TRIGGERS,
        );
        assert!(t.is_none(), "must not escalate before hold");
    }

    #[test]
    fn escalation_emergency_to_crisis_after_hold() {
        let t = guarded_transition(
            DisasterMode::Emergency,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &HIGH_TRIGGERS,
        );
        assert_eq!(
            t,
            Some(ModeTransition {
                from: DisasterMode::Emergency,
                to: DisasterMode::Crisis,
                at_unix: ESCALATION_HOLD_SECS,
                reason: "sos-density",
            })
        );
    }

    #[test]
    fn de_escalation_crisis_to_degraded_after_hold() {
        let t = guarded_transition(
            DisasterMode::Crisis,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &NO_TRIGGERS,
        );
        assert_eq!(
            t,
            Some(ModeTransition {
                from: DisasterMode::Crisis,
                to: DisasterMode::Degraded,
                at_unix: ESCALATION_HOLD_SECS,
                reason: "de-escalation",
            })
        );
        // Active triggers block Crisis → Degraded.
        let t = guarded_transition(
            DisasterMode::Crisis,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &HIGH_TRIGGERS,
        );
        assert!(t.is_none(), "Crisis must not de-escalate while triggers are active");
    }

    #[test]
    fn no_escalation_without_trigger() {
        // Quiescent triggers must not escalate from any state.
        let t = guarded_transition(
            DisasterMode::Normal,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &NO_TRIGGERS,
        );
        assert!(t.is_none(), "Normal must not escalate without triggers");
        let t = guarded_transition(
            DisasterMode::Emergency,
            Some(0),
            ESCALATION_HOLD_SECS,
            false,
            &NO_TRIGGERS,
        );
        assert!(t.is_none(), "Emergency must not escalate without triggers");
    }
}
