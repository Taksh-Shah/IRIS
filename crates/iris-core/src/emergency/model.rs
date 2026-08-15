//! Emergency payload models — EMERG-001 (EMERG_DESIGN.md §4-§5).
//!
//! Wire models for `ContentType::Sos` (P0) and `ContentType::EmergencyAlert`
//! (P3) payloads. Pure data types: no crypto, no engine dependencies.
//! Enum wire codes are fixed protocol constants (never derived from
//! discriminants).
//!
//! ## Authority profile is payload-level (design reality, iter 67+)
//!
//! `KeyAdvertisementV1` carries only the identity→X25519 keyholder binding — no
//! role/scope/severity/drill fields. The complete authority profile therefore
//! lives in [`AuthorityMeta`] inside the signed `EmergencyBroadcast` (the
//! payload is covered by the envelope Ed25519 signature), and is bound to the
//! sender by the chain-verification step plus the `authority_peer_short`
//! leaf-binding check (RED-0008).

use serde::{Deserialize, Serialize};

/// Alert severity (CAP v1.2 semantics subset; wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Severity {
    Minor = 1,
    Moderate = 2,
    Severe = 3,
    Critical = 4,
    /// TEST_MODE drills — never rendered as a real alert (DEC-EMERG-0008).
    Test = 5,
}

impl Severity {
    /// Convert a raw wire code to severity; out of range → `None`.
    pub fn from_u8(v: u8) -> Option<Severity> {
        Some(match v {
            1 => Severity::Minor,
            2 => Severity::Moderate,
            3 => Severity::Severe,
            4 => Severity::Critical,
            5 => Severity::Test,
            _ => return None,
        })
    }

    /// Raw wire code.
    pub fn as_u8(self) -> u8 {
        match self {
            Severity::Minor => 1,
            Severity::Moderate => 2,
            Severity::Severe => 3,
            Severity::Critical => 4,
            Severity::Test => 5,
        }
    }
}

/// Alert certainty (CAP v1.2 semantics subset; wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Certainty {
    Observed = 1,
    Likely = 2,
    Possible = 3,
    Unlikely = 4,
}

impl Certainty {
    pub fn from_u8(v: u8) -> Option<Certainty> {
        Some(match v {
            1 => Certainty::Observed,
            2 => Certainty::Likely,
            3 => Certainty::Possible,
            4 => Certainty::Unlikely,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Certainty::Observed => 1,
            Certainty::Likely => 2,
            Certainty::Possible => 3,
            Certainty::Unlikely => 4,
        }
    }
}

/// Authority alert message types (wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AlertMessageType {
    Alert = 1,
    Update = 2,
    Cancel = 3,
    Accept = 4,
    /// TEST_MODE drill (DEC-EMERG-0008).
    Drill = 5,
    /// Signed revocation-list broadcast (P3, propagates offline).
    RevocationList = 6,
    /// Disaster-mode activations / deactivations (mode + area + reason).
    DisasterActivate = 7,
    DisasterDeactivate = 8,
}

impl AlertMessageType {
    pub fn from_u8(v: u8) -> Option<AlertMessageType> {
        Some(match v {
            1 => AlertMessageType::Alert,
            2 => AlertMessageType::Update,
            3 => AlertMessageType::Cancel,
            4 => AlertMessageType::Accept,
            5 => AlertMessageType::Drill,
            6 => AlertMessageType::RevocationList,
            7 => AlertMessageType::DisasterActivate,
            8 => AlertMessageType::DisasterDeactivate,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        match self {
            AlertMessageType::Alert => 1,
            AlertMessageType::Update => 2,
            AlertMessageType::Cancel => 3,
            AlertMessageType::Accept => 4,
            AlertMessageType::Drill => 5,
            AlertMessageType::RevocationList => 6,
            AlertMessageType::DisasterActivate => 7,
            AlertMessageType::DisasterDeactivate => 8,
        }
    }
}

/// GPS location source for SOS (wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum LocationSource {
    Gps = 1,
    Network = 2,
    UserManual = 3,
    None = 4,
}

impl LocationSource {
    pub fn from_u8(v: u8) -> Option<LocationSource> {
        Some(match v {
            1 => LocationSource::Gps,
            2 => LocationSource::Network,
            3 => LocationSource::UserManual,
            4 => LocationSource::None,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        match self {
            LocationSource::Gps => 1,
            LocationSource::Network => 2,
            LocationSource::UserManual => 3,
            LocationSource::None => 4,
        }
    }
}

/// SOS kind (wire codes fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum SosKind {
    Sos = 1,
    Cancel = 2,
    /// TEST_MODE drill (DEC-EMERG-0008).
    Test = 3,
}

impl SosKind {
    pub fn from_u8(v: u8) -> Option<SosKind> {
        Some(match v {
            1 => SosKind::Sos,
            2 => SosKind::Cancel,
            3 => SosKind::Test,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        match self {
            SosKind::Sos => 1,
            SosKind::Cancel => 2,
            SosKind::Test => 3,
        }
    }
}

/// Compact SOS reason code (EMERG_DESIGN.md §5 — fixed integers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum SosReason {
    Injury = 0,
    Trapped = 1,
    Medical = 2,
    Fire = 3,
    SearchAndRescue = 4,
    Other = 5,
}

impl SosReason {
    pub fn from_u8(v: u8) -> Option<SosReason> {
        Some(match v {
            0 => SosReason::Injury,
            1 => SosReason::Trapped,
            2 => SosReason::Medical,
            3 => SosReason::Fire,
            4 => SosReason::SearchAndRescue,
            5 => SosReason::Other,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        match self {
            SosReason::Injury => 0,
            SosReason::Trapped => 1,
            SosReason::Medical => 2,
            SosReason::Fire => 3,
            SosReason::SearchAndRescue => 4,
            SosReason::Other => 5,
        }
    }
}

/// Authority profile for an emergency broadcast — **payload-level**
/// (EMERG_DESIGN.md §3 top line; `KeyAdvertisementV1` has no profile fields).
///
/// Bound to the sender by the envelope Ed25519 signature (the payload is field
/// 13 of the signed scope) + the chain-verification anchor (RED-0008). The
/// `authority_peer_short` field on the broadcast binds the profile to the chain
/// leaf identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AuthorityMeta {
    /// Informational issuer label ("GSDMA"); enforcement uses the chain profile.
    pub issuer_name: Option<String>,
    /// Geo scope prefix (ISO 3166-2, e.g. "IN-GJ"); area_code must start with it.
    pub geo_scope: Option<String>,
    /// Functional scope tags (comma-separated: public,disaster,drill,revocation).
    pub functional_scope: Option<String>,
    /// Severity cap (1..4); 0 = unrestricted.
    pub max_severity: u8,
}

impl AuthorityMeta {
    pub const MAX_ISSUER_LEN: usize = 64;
    pub const MAX_SCOPE_LEN: usize = 32;

    pub fn validate(&self) -> Result<(), ModelError> {
        if self
            .issuer_name
            .as_ref()
            .is_some_and(|s| s.len() > Self::MAX_ISSUER_LEN)
        {
            return Err(ModelError::FieldTooLong("issuer_name"));
        }
        if self
            .geo_scope
            .as_ref()
            .is_some_and(|s| s.len() > Self::MAX_SCOPE_LEN)
        {
            return Err(ModelError::FieldTooLong("geo_scope"));
        }
        if self
            .functional_scope
            .as_ref()
            .is_some_and(|s| s.len() > Self::MAX_SCOPE_LEN)
        {
            return Err(ModelError::FieldTooLong("functional_scope"));
        }
        if self.max_severity > 4 {
            return Err(ModelError::BadMaxSeverity(self.max_severity));
        }
        Ok(())
    }
}

/// Authority broadcast payload — minimal CAP-inspired CBOR subset
/// (EMERG_DESIGN.md §4, DEC-EMERG-0001). The envelope's Ed25519 signature
/// covers the encoded payload (field 13); the authority chain rides envelope
/// field 18 (`auth_cert_chain`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmergencyBroadcast {
    pub format_version: u8,
    /// UUID v4 broadcast identity (16 bytes).
    pub broadcast_id: [u8; 16],
    /// Parent `broadcast_id` for UPDATE / CANCEL linkage.
    pub republish_id: Option<[u8; 16]>,
    /// Unix seconds.
    pub issued_at: u64,
    /// Unix seconds; must be `issued_at + ≤ 72 h`.
    pub expires_at: u64,
    pub severity: Severity,
    pub certainty: Certainty,
    pub message_type: AlertMessageType,
    /// ISO 3166-2 area code ("IN-GJ", "IN-GJ-19"); ≤ 8 chars.
    pub area_code: String,
    /// BCP 47 language ("en", "hi", "gu"); ≤ 5 chars.
    pub language: String,
    /// ≤ 96 chars (C1: compact; CAP allows 2000).
    pub headline: String,
    /// ≤ 96 chars.
    pub instructions: Option<String>,
    /// Payload-level authority profile (signed; enforcement reads this).
    pub authority: AuthorityMeta,
    /// Leaf authority short id (SHA-256(sender)[..16]); binds the profile to
    /// the chain leaf (RED-0008).
    pub authority_peer_short: Option<[u8; 16]>,
    /// TEST_MODE marker (DEC-EMERG-0008).
    pub drill: bool,
}

impl EmergencyBroadcast {
    /// Headline + instructions cap (C1: compact 96/96 instead of 2000).
    pub const MAX_HEADLINE_LEN: usize = 96;
    pub const MAX_INSTRUCTIONS_LEN: usize = 96;
    pub const MAX_AREA_CODE_LEN: usize = 8;
    pub const MAX_LANGUAGE_LEN: usize = 5;
    /// Authority broadcasts may not be valid for more than 72 h.
    pub const MAX_VALIDITY_SECS: u64 = 72 * 3600;

    /// Validate the broadcast's structural + size constraints. Called by the
    /// codec before emitting and after decoding (fail loud, never truncate).
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.format_version != 1 {
            return Err(ModelError::BadFormatVersion(self.format_version));
        }
        if self.expires_at < self.issued_at {
            return Err(ModelError::ExpiryBeforeIssue);
        }
        if self.expires_at.saturating_sub(self.issued_at) > Self::MAX_VALIDITY_SECS {
            return Err(ModelError::ValidityTooLong);
        }
        if self.area_code.len() > Self::MAX_AREA_CODE_LEN {
            return Err(ModelError::FieldTooLong("area_code"));
        }
        if self.language.len() > Self::MAX_LANGUAGE_LEN {
            return Err(ModelError::FieldTooLong("language"));
        }
        if self.headline.len() > Self::MAX_HEADLINE_LEN {
            return Err(ModelError::FieldTooLong("headline"));
        }
        if self
            .instructions
            .as_ref()
            .is_some_and(|s| s.len() > Self::MAX_INSTRUCTIONS_LEN)
        {
            return Err(ModelError::FieldTooLong("instructions"));
        }
        self.authority.validate()?;
        Ok(())
    }
}

/// SOS payload — P0, never encrypted, ≤ 84 B (EMERG_DESIGN.md §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SosMessage {
    pub format_version: u8,
    /// Degrees × 1_000_000 (i32).
    pub latitude: i32,
    /// Degrees × 1_000_000 (i32).
    pub longitude: i32,
    /// Meters; 0 = unknown.
    pub accuracy_m: u16,
    pub location_source: LocationSource,
    /// Unix seconds (envelope carrier clock).
    pub timestamp: u32,
    pub kind: SosKind,
    /// Present when `kind == Cancel`; must reference the original SOS.
    pub original_message_id: Option<[u8; 16]>,
    pub reason: Option<SosReason>,
}

impl SosMessage {
    /// P0 payload ceiling (DISC-0008).
    pub const MAX_ENCODED_BYTES: usize = 84;

    /// Validate structural constraints. Called by the codec.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.format_version != 1 {
            return Err(ModelError::BadFormatVersion(self.format_version));
        }
        if self.kind == SosKind::Cancel && self.original_message_id.is_none() {
            return Err(ModelError::CancelMissingOriginal);
        }
        if self.kind != SosKind::Cancel && self.original_message_id.is_some() {
            return Err(ModelError::OriginalOnNonCancel);
        }
        Ok(())
    }
}

/// Validation errors for emergency models.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("unsupported emergency format version {0}")]
    BadFormatVersion(u8),
    #[error("expires_at before issued_at")]
    ExpiryBeforeIssue,
    #[error("authority broadcast validity exceeds 72 h")]
    ValidityTooLong,
    #[error("field exceeds max length: {0}")]
    FieldTooLong(&'static str),
    #[error("authority max_severity out of range: {0}")]
    BadMaxSeverity(u8),
    #[error("SOS cancel missing original_message_id")]
    CancelMissingOriginal,
    #[error("original_message_id present on non-cancel SOS")]
    OriginalOnNonCancel,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_broadcast() -> EmergencyBroadcast {
        EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [7u8; 16],
            republish_id: None,
            issued_at: 1_000,
            expires_at: 1_000 + 3600,
            severity: Severity::Severe,
            certainty: Certainty::Likely,
            message_type: AlertMessageType::Alert,
            area_code: "IN-GJ".into(),
            language: "en".into(),
            headline: "Cyclone approaching".into(),
            instructions: Some("Move to shelter".into()),
            authority: AuthorityMeta {
                issuer_name: Some("GSDMA".into()),
                geo_scope: Some("IN-GJ".into()),
                functional_scope: Some("public".into()),
                max_severity: 4,
            },
            authority_peer_short: None,
            drill: false,
        }
    }

    #[test]
    fn severities_round_trip_wire_codes() {
        for s in [
            Severity::Minor,
            Severity::Moderate,
            Severity::Severe,
            Severity::Critical,
            Severity::Test,
        ] {
            assert_eq!(Severity::from_u8(s.as_u8()), Some(s));
        }
        assert_eq!(Severity::from_u8(0), None);
        assert_eq!(Severity::from_u8(6), None);
        assert_eq!(Severity::Critical.as_u8(), 4);
    }

    #[test]
    fn sos_kind_reason_round_trip() {
        assert_eq!(
            SosKind::from_u8(SosKind::Cancel.as_u8()),
            Some(SosKind::Cancel)
        );
        assert_eq!(SosKind::from_u8(0), None);
        assert_eq!(SosReason::from_u8(4), Some(SosReason::SearchAndRescue));
        assert_eq!(SosReason::from_u8(6), None);
    }

    #[test]
    fn broadcast_validate_ok() {
        assert!(base_broadcast().validate().is_ok());
    }

    #[test]
    fn broadcast_validate_rejects_bad_shape() {
        let base = base_broadcast();
        // Expiry before issue.
        let mut bad = base.clone();
        bad.expires_at = 500;
        assert!(matches!(bad.validate(), Err(ModelError::ExpiryBeforeIssue)));
        // Validity > 72 h.
        let mut bad = base.clone();
        bad.expires_at = 1_000 + 73 * 3600;
        assert!(matches!(bad.validate(), Err(ModelError::ValidityTooLong)));
        // Overflowing headline.
        let mut bad = base.clone();
        bad.headline = "x".repeat(97);
        assert!(matches!(bad.validate(), Err(ModelError::FieldTooLong("headline"))));
        // Authority profile constraints.
        let mut bad = base.clone();
        bad.authority.max_severity = 7;
        assert!(matches!(bad.validate(), Err(ModelError::BadMaxSeverity(7))));
        let mut bad = base.clone();
        bad.authority.geo_scope = Some("X".repeat(33));
        assert!(matches!(bad.validate(), Err(ModelError::FieldTooLong("geo_scope"))));
    }

    #[test]
    fn sos_cancel_requires_original_id() {
        let ok = SosMessage {
            format_version: 1,
            latitude: 0,
            longitude: 0,
            accuracy_m: 0,
            location_source: LocationSource::None,
            timestamp: 1_000,
            kind: SosKind::Cancel,
            original_message_id: Some([9u8; 16]),
            reason: None,
        };
        assert!(ok.validate().is_ok());
        let mut bad = ok.clone();
        bad.original_message_id = None;
        assert!(matches!(bad.validate(), Err(ModelError::CancelMissingOriginal)));
        // Non-cancel with original present.
        let mut bad = ok;
        bad.kind = SosKind::Sos;
        assert!(matches!(bad.validate(), Err(ModelError::OriginalOnNonCancel)));
    }
}