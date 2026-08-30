//! Serde DTOs crossing the Tauri IPC boundary (DESKTOP_DESIGN.md §5, §8).
//!
//! Privacy P2: every identifier crossing to the webview is truncated by
//! construction via `ShortId::short()` — payload bytes only ever travel as the
//! explicit user-visible chat text, never as message metadata.

use serde::Serialize;

use iris_core::message::PeerId;
use iris_core::message_engine::MetricsSnapshot;
use iris_core::observability::ShortId;
use iris_core::protocol::{ContentType, Envelope};

/// Result of a successful `send_message` invocation (truncated id only).
#[derive(Debug, Clone, Serialize)]
pub struct MessageIdView {
    pub id_hex: String,
}

impl MessageIdView {
    pub fn from_envelope(env: &Envelope) -> Self {
        Self {
            id_hex: env.message_id.short().to_string(),
        }
    }
}

/// One delivered message streamed to the UI (DESKTOP_DESIGN.md §5 D5).
#[derive(Debug, Clone, Serialize)]
pub struct IncomingMessageView {
    /// 8-byte / 16-hex truncated sender id.
    pub from_short: String,
    /// 8-byte / 16-hex truncated message id.
    pub message_id_short: String,
    pub priority: u8,
    pub received_at_unix: u64,
    /// User-visible text, present only for `Text`-type payloads that parse as
    /// UTF-8. Everything else carries `None` (payload-free metadata).
    pub text: Option<String>,
    pub content_type: String,
}

impl IncomingMessageView {
    pub fn from_envelope(env: &Envelope) -> Self {
        let text = if matches!(env.payload_type, ContentType::Text | ContentType::Sos) {
            std::str::from_utf8(&env.payload).ok().map(str::to_owned)
        } else {
            None
        };
        Self {
            from_short: if env.sender_id.len() == 32 {
                let mut a = [0u8; 32];
                a.copy_from_slice(&env.sender_id);
                PeerId::from_bytes(a).short().to_string()
            } else {
                ShortId::from_prefix([0u8; 8]).to_string()
            },
            message_id_short: env.message_id.short().to_string(),
            priority: env.priority.as_u8(),
            received_at_unix: env.timestamp,
            text,
            content_type: format!("{:?}", env.payload_type),
        }
    }
}

/// Per-transport row for the status panel.
#[derive(Debug, Clone, Serialize)]
pub struct TransportStatus {
    pub id: String,
    pub display: String,
    pub state: String,
}

/// Engine counters (MSG-001 `MetricsSnapshot`), no P3 metric names leaked.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct EngineMetricsView {
    pub sent: u64,
    pub delivered: u64,
    pub relayed: u64,
    pub dropped_duplicates: u64,
    pub expired: u64,
    pub delivery_failed: u64,
}

impl From<MetricsSnapshot> for EngineMetricsView {
    fn from(m: MetricsSnapshot) -> Self {
        Self {
            sent: m.sent,
            delivered: m.delivered,
            relayed: m.relayed,
            dropped_duplicates: m.dropped_duplicates,
            expired: m.expired,
            delivery_failed: m.delivery_failed,
        }
    }
}

/// Aggregate status for the shell (AC5: offline state is representable).
#[derive(Debug, Clone, Serialize)]
pub struct MeshStatus {
    pub node_id_short: String,
    pub transports: Vec<TransportStatus>,
    pub online: bool,
    pub metrics: EngineMetricsView,
}

/// One non-zero `iris.*_total` counter (OBS-001 snapshot).
#[derive(Debug, Clone, Serialize)]
pub struct MetricView {
    pub name: String,
    pub value: u64,
}

/// Parse a 64-hex-char node id from UI input.
pub fn parse_peer_id(s: &str) -> Result<PeerId, String> {
    let s = s.trim();
    if s.len() != 64 {
        return Err("recipient must be 64 hex chars (32-byte node id)".to_string());
    }
    let mut out = [0u8; 32];
    for (i, pair) in s.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_val(pair[0]).ok_or_else(|| format!("invalid hex at char {}", i * 2))?;
        let lo = hex_val(pair[1]).ok_or_else(|| format!("invalid hex at char {}", i * 2 + 1))?;
        out[i] = (hi << 4) | lo;
    }
    Ok(PeerId(out))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_id_hex_round_trip() {
        let hex = "aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899";
        let peer = parse_peer_id(hex).expect("valid hex");
        assert_eq!(
            peer.as_bytes()[..8],
            [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x11]
        );
    }

    #[test]
    fn peer_id_rejects_bad_length_and_chars() {
        assert!(parse_peer_id("abcd").is_err(), "too short");
        assert!(parse_peer_id(&"zz".repeat(32)).is_err(), "bad chars");
    }
}
