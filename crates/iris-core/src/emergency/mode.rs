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

/// Guarded transition decision.
///
/// * Authority deactivation wins immediately (bypasses hold).
/// * Escalation requires `last_transition_at` ≥ `ESCALATION_HOLD_SECS` ago
///   (or no prior transition).
/// * De-escalation (Crisis → …) follows the ladder only after the hold.
pub fn guarded_transition(
    current: DisasterMode,
    last_transition_at_unix: Option<u64>,
    now_unix: u64,
    authority_deactivate: bool,
) -> Option<ModeTransition> {
    // Crises only clear via verified authority deactivate (or manual).
    if authority_deactivate && current != DisasterMode::Normal {
        return Some(ModeTransition {
            from: current,
            to: DisasterMode::Normal,
            at_unix: now_unix,
            reason: "authority-deactivate",
        });
    }
    // De-escalation path from a settled Crisis/Degraded also needs the hold.
    if current == DisasterMode::Degraded && now_unix.saturating_sub(last_transition_at_unix.unwrap_or(0)) >= ESCALATION_HOLD_SECS {
        // Degraded → Normal once service is restored (engine re-sets triggers).
        return Some(ModeTransition {
            from: current,
            to: DisasterMode::Normal,
            at_unix: now_unix,
            reason: "recovery",
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let t = guarded_transition(DisasterMode::Crisis, Some(1_000), 1_005, true);
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
            guarded_transition(DisasterMode::Normal, Some(1_000), 1_005, true),
            None
        );
    }

    #[test]
    fn degraded_recovers_after_hold() {
        let t = guarded_transition(DisasterMode::Degraded, Some(1_000), 1_000 + 900, false);
        assert!(matches!(t, Some(ModeTransition { to: DisasterMode::Normal, .. })));
        // Too early.
        let t = guarded_transition(DisasterMode::Degraded, Some(1_000), 1_000 + 899, false);
        assert!(t.is_none());
    }
}