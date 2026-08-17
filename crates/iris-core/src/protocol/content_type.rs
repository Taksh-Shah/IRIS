//! Payload content types (MESSAGE_ENVELOPE.md §Payload Types, codes 1–16).

use crate::message::MessagePriority;

/// Content/payload type codes carried in envelope field 10.
///
/// Codes are fixed by the wire spec (`MESSAGE_ENVELOPE.md` §Payload Types);
/// numeric values are part of the protocol and must never change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum ContentType {
    /// Plain text message (64 KB).
    Text = 1,
    /// Emergency SOS signal — 255 byte total envelope on LoRa.
    Sos = 2,
    /// GPS coordinates (256 bytes).
    Location = 3,
    /// Compressed image (2 MB).
    Image = 4,
    /// Compressed audio (5 MB).
    Voice = 5,
    /// Compressed video (50 MB).
    Video = 6,
    /// Generic file (50 MB).
    File = 7,
    /// Medical emergency (512 bytes).
    Medical = 8,
    /// Delivery acknowledgement (128 bytes).
    Ack = 9,
    /// Routing table update (1 KB).
    RoutingGossip = 10,
    /// Node capability bundle (256 bytes).
    Capability = 11,
    /// Message sync request (4 KB).
    SyncRequest = 12,
    /// Message fragment (MTU).
    Fragment = 13,
    /// Authority emergency broadcast (2 KB).
    EmergencyAlert = 14,
    /// Group key package (4 KB).
    GroupKey = 15,
    /// Identity key rotation notice (1 KB).
    KeyRotation = 16,
}

/// Manual Arbitrary implementation for proptest (enabled via proptest feature)
#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for ContentType {
    type Parameters = ();
    type Strategy = proptest::strategy::Just<ContentType>;

    fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
        proptest::strategy::Just(ContentType::Text) // Default to Text for simplicity
    }
}

impl ContentType {
    /// Convert a raw code to a content type; out of range returns `None`.
    pub fn from_u8(v: u8) -> Option<ContentType> {
        Some(match v {
            1 => ContentType::Text,
            2 => ContentType::Sos,
            3 => ContentType::Location,
            4 => ContentType::Image,
            5 => ContentType::Voice,
            6 => ContentType::Video,
            7 => ContentType::File,
            8 => ContentType::Medical,
            9 => ContentType::Ack,
            10 => ContentType::RoutingGossip,
            11 => ContentType::Capability,
            12 => ContentType::SyncRequest,
            13 => ContentType::Fragment,
            14 => ContentType::EmergencyAlert,
            15 => ContentType::GroupKey,
            16 => ContentType::KeyRotation,
            _ => return None,
        })
    }

    /// Raw wire code.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Assign transmission priority by message type per `PRIORITY_MODEL.md`
    /// §Priority Determination at Origination.
    ///
    /// The wire-spec content types map to the eight priority classes:
    /// SOS→P0, MEDICAL→P1, LOCATION→P2, EMERGENCY_ALERT→P3, TEXT→P4,
    /// IMAGE→P5, VOICE→P6, VIDEO→P7. Control/overhead types (ACK, gossip,
    /// capability, sync, fragment, group key, key rotation) default to P4
    /// (Normal); FILE is bulk data and gets P7. Priority is fixed at
    /// origination and never changes in transit (PRIORITY_MODEL non-goal).
    pub fn assign_priority(self) -> MessagePriority {
        match self {
            ContentType::Sos => MessagePriority::P0,
            ContentType::Medical => MessagePriority::P1,
            ContentType::Location => MessagePriority::P2,
            ContentType::EmergencyAlert => MessagePriority::P3,
            ContentType::Text => MessagePriority::P4,
            ContentType::Ack
            | ContentType::RoutingGossip
            | ContentType::Capability
            | ContentType::SyncRequest
            | ContentType::Fragment
            | ContentType::GroupKey
            | ContentType::KeyRotation => MessagePriority::P4,
            ContentType::Image => MessagePriority::P5,
            ContentType::Voice => MessagePriority::P6,
            ContentType::Video | ContentType::File => MessagePriority::P7,
        }
    }

    /// True for payload codes that carry encrypted user data end-to-end
    /// (everything except control-plane and acknowledgment types).
    pub fn is_user_payload(self) -> bool {
        !matches!(
            self,
            ContentType::Ack
                | ContentType::RoutingGossip
                | ContentType::Capability
                | ContentType::SyncRequest
                | ContentType::Fragment
                | ContentType::GroupKey
                | ContentType::KeyRotation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_codes_round_trip() {
        for code in 1..=16u8 {
            let ct = ContentType::from_u8(code).expect("codes 1..=16 valid");
            assert_eq!(ct.as_u8(), code);
            assert_eq!(ContentType::from_u8(ct.as_u8()), Some(ct));
        }
    }

    #[test]
    fn out_of_range_rejected() {
        for code in [0u8, 17, 100, 255] {
            assert_eq!(ContentType::from_u8(code), None, "{code} must be rejected");
        }
    }

    #[test]
    fn priority_mapping_matches_priority_model() {
        assert_eq!(ContentType::Sos.assign_priority(), MessagePriority::P0);
        assert_eq!(ContentType::Medical.assign_priority(), MessagePriority::P1);
        assert_eq!(ContentType::Location.assign_priority(), MessagePriority::P2);
        assert_eq!(
            ContentType::EmergencyAlert.assign_priority(),
            MessagePriority::P3
        );
        assert_eq!(ContentType::Text.assign_priority(), MessagePriority::P4);
        assert_eq!(ContentType::Image.assign_priority(), MessagePriority::P5);
        assert_eq!(ContentType::Voice.assign_priority(), MessagePriority::P6);
        assert_eq!(ContentType::Video.assign_priority(), MessagePriority::P7);
    }

    #[test]
    fn user_payload_classification() {
        assert!(ContentType::Text.is_user_payload());
        assert!(ContentType::Sos.is_user_payload());
        assert!(!ContentType::Ack.is_user_payload());
        assert!(!ContentType::Capability.is_user_payload());
        assert!(!ContentType::Fragment.is_user_payload());
    }
}
