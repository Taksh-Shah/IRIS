//! Wire message identifier — 16-byte UUIDv7.

use std::fmt;

use uuid::Uuid;

/// 16-byte UUIDv7 message identifier (MESSAGE_ENVELOPE.md field 2, RFC 9562).
///
/// Normalized across IRIS per RES-0008 Finding C1: `MESSAGE_ENVELOPE.md` is
/// authoritative for the wire form (16 bytes). The deduplication exact-set
/// (RES-0008 R5) and the storage key (STORAGE.md) both use these same bytes.
/// This replaces the earlier 24-byte (DEDUPLICATION.md) and 32-byte
/// (`message.rs` `SerializedMessage`) declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessageId(pub [u8; 16]);

impl MessageId {
    /// Byte length of a wire message id.
    pub const LEN: usize = 16;

    /// Generate a fresh UUIDv7 identifier (RFC 9562 time-ordered).
    pub fn new_v7() -> Self {
        MessageId(*Uuid::now_v7().as_bytes())
    }

    /// Build from a 16-byte array.
    pub fn from_bytes(b: [u8; 16]) -> Self {
        MessageId(b)
    }

    /// Raw bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Consume and return the raw bytes.
    pub fn to_bytes(self) -> [u8; 16] {
        self.0
    }

    /// Parse from a slice; returns `None` unless exactly 16 bytes.
    pub fn from_slice(s: &[u8]) -> Option<Self> {
        if s.len() != Self::LEN {
            return None;
        }
        let mut b = [0u8; 16];
        b.copy_from_slice(s);
        Some(MessageId(b))
    }

    /// Truncated 8-byte hex prefix — the only form allowed in logs, metrics,
    /// or exported telemetry (privacy rule P2 / OBS_DESIGN.md). Full ID
    /// remains available in memory via `Display`. Allocation-free.
    pub fn short(&self) -> crate::observability::ShortId {
        crate::observability::ShortId::from(self)
    }
}

impl From<[u8; 16]> for MessageId {
    fn from(b: [u8; 16]) -> Self {
        MessageId(b)
    }
}

impl From<MessageId> for [u8; 16] {
    fn from(id: MessageId) -> Self {
        id.0
    }
}

impl From<Uuid> for MessageId {
    fn from(u: Uuid) -> Self {
        MessageId(*u.as_bytes())
    }
}

impl From<MessageId> for Uuid {
    fn from(id: MessageId) -> Self {
        Uuid::from_bytes(id.0)
    }
}

impl AsRef<[u8]> for MessageId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_v7_produces_16_bytes() {
        let id = MessageId::new_v7();
        assert_eq!(id.as_bytes().len(), 16);
        // UUIDv7 version nibble is 7 (RFC 9562): bytes[6] >> 4 == 7.
        assert_eq!(id.0[6] >> 4, 7, "UUIDv7 version nibble must be 7");
    }

    #[test]
    fn round_trips_through_uuid() {
        let id = MessageId::new_v7();
        let u: Uuid = id.into();
        let back: MessageId = u.into();
        assert_eq!(back, id);
    }

    #[test]
    fn from_slice_rejects_wrong_length() {
        assert!(MessageId::from_slice(&[0u8; 15]).is_none());
        assert!(MessageId::from_slice(&[0u8; 17]).is_none());
        assert!(MessageId::from_slice(&[0u8; 16]).is_some());
    }

    #[test]
    fn display_is_lowercase_hex() {
        let id = MessageId([0x01u8; 16]);
        assert_eq!(id.to_string(), "01010101010101010101010101010101");
    }
}