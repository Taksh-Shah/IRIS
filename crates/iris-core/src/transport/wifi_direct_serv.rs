//! WIFIDIRECT-001 — Wi-Fi Direct DNS-SD (Bonjour) TXT-record beacon:
//! build + strict parse (AC-2/AC-6).
//!
//! v1 discovery (RES-0021 Q4, WIFI_DIRECT_TRANSPORT_DESIGN.md §2.3) uses Wi-Fi
//! Direct Service Discovery (DNS-SD / Bonjour): the p2p service name is the
//! constant [`WIFI_DIRECT_SERVICE_NAME`], and the IRIS **TXT record** carries a
//! 22-byte discovery beacon mirroring the uniform BLE-001 / WIFIAWARE-001
//! beacon shape. A DNS-SD match produces a *candidate peer* only — identity is
//! resolved later from the verified advertisement / envelope (IDENT-001); the
//! record is a hint and is NEVER a trust boundary (DEC-WD-0007: identity by
//! public-key fingerprint, never the P2P device MAC, which is randomized /
//! persistent-group-bound on Android).
//!
//! Format is tiny (fits a DNS-SD TXT record value, bounded ~255 B) and parsed
//! **defensively**: malformed/truncated/oversized payloads are rejected without
//! panic or allocation blowup (RES-0021 Q8: the Wi-Fi stack CVE pattern
//! — CVE-2021-0326 adjacent unauthenticated P2P RCE — is parser/state class bug).
//!
//! Format (fixed-width, versioned, no length ambiguity):
//! ```text
//! [0]     version      u8      = 1
//! [1..3]  capability   u16 BE  (bitmask, see WifiDirectCapBits)
//! [3]     record kind  u8      = 0 (discovery-only)
//! [4..20] peer_short   u8x16   (SHA-256(pubkey)[..16], IDENT-001 peer_short)
//! [20..22] freshness   u16 BE  (minutes since epoch mod 2^16, anti-stale hint)
//! ```
//! Total = 22 bytes. Signing/integrity lives in the IRIS envelope / verified
//! advertisement layer; WPA2 protects the L2 link only (DEC-WD-0002/0007).

use crate::message::PeerId;

/// Wi-Fi Direct DNS-SD (Bonjour) p2p service name IRIS advertises/matches.
/// Android DNS-SD filters on this service type; TXT record carries the beacon.
pub const WIFI_DIRECT_SERVICE_NAME: &str = "com.iris.mesh.v1";
/// TXT-record wire length in bytes.
pub const WIFI_DIRECT_TXT_LEN: usize = 22;
/// Current TXT-record version.
pub const WIFI_DIRECT_TXT_VERSION: u8 = 1;
/// Discovery-only record kind.
pub const KIND_DISCOVERY: u8 = 0;

/// Wi-Fi Direct capability bit flags carried in the DNS-SD TXT record. The
/// beacon subset of the transport caps; SAE is reported only on R2-capable
/// devices and consumed by a later capability-gated phase (DEC-WD-0002) — never
/// a v1 default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiDirectCapBits(u16);

impl WifiDirectCapBits {
    pub const GROUP_OWNER_CAPABLE: u16 = 1 << 0;
    pub const CLIENT_CAPABLE: u16 = 1 << 1;
    pub const PERSISTENT_GO: u16 = 1 << 2;
    pub const BAND_5GHZ: u16 = 1 << 3;
    pub const WPA3_SAE_CAPABLE: u16 = 1 << 4;

    /// Known mask — bits above this are reserved and rejected on parse.
    pub const fn all() -> Self {
        WifiDirectCapBits(0b11111)
    }
    /// No capability bits set.
    pub const fn empty() -> Self {
        WifiDirectCapBits(0)
    }
    pub const fn from_bits(bits: u16) -> Self {
        WifiDirectCapBits(bits & Self::all().bits())
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
    pub fn set(&mut self, bit: u16) {
        self.0 |= bit & Self::all().bits();
    }
    pub fn contains(self, bit: u16) -> bool {
        self.0 & bit != 0
    }
}

/// Parse/validation error for a Wi-Fi Direct DNS-SD TXT record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TxtRecordError {
    /// Payload shorter than `WIFI_DIRECT_TXT_LEN`.
    TooShort,
    /// Payload longer than `MAX_DNS_SD_TXT_BYTES` (defensive upper bound).
    TooLong,
    /// Unsupported version.
    UnsupportedVersion(u8),
    /// Unknown record kind.
    UnknownKind(u8),
    /// Reserved/capability bits set outside the known mask.
    ReservedBits,
    /// All-zero `peer_short`: an identity hint of 0x00..00 would collide with
    /// the "unknown sender" sentinel `PeerId([0u8; 32])` downstream, so the
    /// record is rejected outright (WIFIDIRECT-001_RT-001).
    ZeroShortId,
}

impl std::fmt::Display for TxtRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wifi-direct txt record: {self:?}")
    }
}

impl std::error::Error for TxtRecordError {}

/// A parsed Wi-Fi Direct DNS-SD TXT-record beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiDirectTxtRecord {
    pub capabilities: WifiDirectCapBits,
    pub peer_short: [u8; 16],
    /// Minutes since epoch mod 2^16 (freshness hint; never a trust boundary).
    pub freshness_minutes: u16,
}

impl WifiDirectTxtRecord {
    /// Build the TXT-record value bytes for a peer's DNS-SD advertisement.
    pub fn build(
        capabilities: WifiDirectCapBits,
        peer_short: [u8; 16],
        freshness_minutes: u16,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIFI_DIRECT_TXT_LEN);
        out.push(WIFI_DIRECT_TXT_VERSION);
        out.extend_from_slice(&capabilities.bits().to_be_bytes());
        out.push(KIND_DISCOVERY);
        out.extend_from_slice(&peer_short);
        out.extend_from_slice(&freshness_minutes.to_be_bytes());
        debug_assert_eq!(out.len(), WIFI_DIRECT_TXT_LEN);
        out
    }

    /// Strictly parse a DNS-SD TXT-record value. Rejects undersized,
    /// oversized-past-cap, version-mismatched, unknown-kind, reserved-bit, and
    /// all-zero-`peer_short` payloads.
    pub fn parse(bytes: &[u8]) -> Result<Self, TxtRecordError> {
        if bytes.len() < WIFI_DIRECT_TXT_LEN {
            return Err(TxtRecordError::TooShort);
        }
        if bytes.len() > MAX_DNS_SD_TXT_BYTES {
            return Err(TxtRecordError::TooLong);
        }
        let version = bytes[0];
        if version != WIFI_DIRECT_TXT_VERSION {
            return Err(TxtRecordError::UnsupportedVersion(version));
        }
        let caps = u16::from_be_bytes([bytes[1], bytes[2]]);
        if caps & !WifiDirectCapBits::all().bits() != 0 {
            return Err(TxtRecordError::ReservedBits);
        }
        let kind = bytes[3];
        if kind != KIND_DISCOVERY {
            return Err(TxtRecordError::UnknownKind(kind));
        }
        if bytes[4..20].iter().all(|b| *b == 0) {
            return Err(TxtRecordError::ZeroShortId);
        }
        let mut peer_short = [0u8; 16];
        peer_short.copy_from_slice(&bytes[4..20]);
        let freshness = u16::from_be_bytes([bytes[20], bytes[21]]);
        Ok(WifiDirectTxtRecord {
            capabilities: WifiDirectCapBits(caps),
            peer_short,
            freshness_minutes: freshness,
        })
    }

    /// Surface the short id as a zero-padded `PeerId` for the discovery stream
    /// (candidate hint only — full identity is resolved on the verified
    /// advertisement / envelope path, DEC-WD-0007).
    pub fn candidate_peer_id(&self) -> PeerId {
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&self.peer_short);
        PeerId(id)
    }
}

/// Upper bound on a DNS-SD TXT-record value the transport will produce or
/// accept. Android TXT records are bounded (typically ~255 B per key); we never
/// build anything this large.
pub const MAX_DNS_SD_TXT_BYTES: usize = 255;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parse_round_trip() {
        let b = WifiDirectTxtRecord::build(WifiDirectCapBits::all(), [0xAB; 16], 42);
        assert_eq!(b.len(), WIFI_DIRECT_TXT_LEN);
        let parsed = WifiDirectTxtRecord::parse(&b).unwrap();
        assert_eq!(parsed.peer_short, [0xAB; 16]);
        assert_eq!(parsed.freshness_minutes, 42);
        assert!(parsed
            .capabilities
            .contains(WifiDirectCapBits::GROUP_OWNER_CAPABLE));
        assert!(parsed
            .capabilities
            .contains(WifiDirectCapBits::CLIENT_CAPABLE));
        assert!(parsed
            .capabilities
            .contains(WifiDirectCapBits::PERSISTENT_GO));
        assert!(parsed.capabilities.contains(WifiDirectCapBits::BAND_5GHZ));
        assert!(parsed
            .capabilities
            .contains(WifiDirectCapBits::WPA3_SAE_CAPABLE));
    }

    #[test]
    fn service_name_is_fixed() {
        // AC-2: the DNS-SD service name is constant (used as the p2p service
        // filter); TXT record rides the fixed service.
        assert_eq!(WIFI_DIRECT_SERVICE_NAME, "com.iris.mesh.v1");
    }

    #[test]
    fn too_short_rejected_no_panic() {
        assert_eq!(
            WifiDirectTxtRecord::parse(&[1u8; 5]).unwrap_err(),
            TxtRecordError::TooShort
        );
        assert_eq!(
            WifiDirectTxtRecord::parse(&[]).unwrap_err(),
            TxtRecordError::TooShort
        );
        // Exhaustive short-length sweep (parser robustness, RES-0021 Q8).
        for len in 0..WIFI_DIRECT_TXT_LEN {
            let bytes = vec![0x11u8; len];
            assert_eq!(
                WifiDirectTxtRecord::parse(&bytes).unwrap_err(),
                TxtRecordError::TooShort,
                "len {len} must reject"
            );
        }
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut b = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), [0u8; 16], 0);
        b[0] = 99;
        assert_eq!(
            WifiDirectTxtRecord::parse(&b).unwrap_err(),
            TxtRecordError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn unknown_kind_rejected() {
        let mut b = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), [0u8; 16], 0);
        b[3] = 0x7F;
        assert_eq!(
            WifiDirectTxtRecord::parse(&b).unwrap_err(),
            TxtRecordError::UnknownKind(0x7F)
        );
    }

    #[test]
    fn reserved_bits_rejected() {
        let mut b = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), [0u8; 16], 0);
        // Flip a bit above the known mask (bit 5..15).
        b[1] |= 0x80;
        assert_eq!(
            WifiDirectTxtRecord::parse(&b).unwrap_err(),
            TxtRecordError::ReservedBits
        );
    }

    #[test]
    fn oversized_padded_txt_tolerated_without_panic() {
        // DNS-SD TXT records may carry extra key=value bytes beyond our
        // 22-byte prefix; the prefix parses fine and extra bytes are ignored
        // (defensive).
        let b = WifiDirectTxtRecord::build(
            WifiDirectCapBits::from_bits(WifiDirectCapBits::GROUP_OWNER_CAPABLE),
            [7u8; 16],
            1,
        );
        let mut padded = b.clone();
        padded.extend_from_slice(&[0u8; 230]);
        assert_eq!(padded.len(), 252);
        let parsed = WifiDirectTxtRecord::parse(&padded).unwrap();
        assert_eq!(parsed.peer_short, [7u8; 16]);
    }

    #[test]
    fn oversized_past_cap_rejected_no_panic() {
        // Above the documented platform cap (255 B) → TooLong, even though the
        // 22-byte prefix is well-formed (defensive upper bound).
        let base = WifiDirectTxtRecord::build(
            WifiDirectCapBits::from_bits(WifiDirectCapBits::CLIENT_CAPABLE),
            [7u8; 16],
            1,
        );
        let mut padded = base.clone();
        padded.extend_from_slice(&[0u8; MAX_DNS_SD_TXT_BYTES]); // 22+255=277
        assert_eq!(
            WifiDirectTxtRecord::parse(&padded).unwrap_err(),
            TxtRecordError::TooLong
        );
        // Boundary: exactly the cap is still accepted.
        let mut at_cap = base.clone();
        at_cap.extend_from_slice(&[0u8; MAX_DNS_SD_TXT_BYTES - WIFI_DIRECT_TXT_LEN]);
        assert_eq!(at_cap.len(), MAX_DNS_SD_TXT_BYTES);
        assert!(WifiDirectTxtRecord::parse(&at_cap).is_ok());
    }

    #[test]
    fn candidate_peer_id_pads_short_id() {
        let mut short = [0u8; 16];
        short[0] = 0xDE;
        let b = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), short, 0);
        let parsed = WifiDirectTxtRecord::parse(&b).unwrap();
        let id = parsed.candidate_peer_id();
        assert_eq!(&id.0[..16], &short);
        assert_eq!(&id.0[16..], &[0u8; 16]);
    }

    #[test]
    fn zero_peer_short_rejected() {
        // RT-001: an all-zero `peer_short` would map to the "unknown sender"
        // sentinel PeerId([0u8; 32]) on the candidate path. The record must be
        // rejected so no inbound frame can ever be attributed to the zero
        // sentinel via a well-formed TXT record (DEC-WD-0007 candidate seam).
        let b = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), [0u8; 16], 0);
        assert_eq!(
            WifiDirectTxtRecord::parse(&b).unwrap_err(),
            TxtRecordError::ZeroShortId
        );
        // Any non-zero byte is accepted (boundary).
        let mut short = [0u8; 16];
        short[15] = 0x01;
        let b2 = WifiDirectTxtRecord::build(WifiDirectCapBits::empty(), short, 0);
        assert!(WifiDirectTxtRecord::parse(&b2).is_ok());
    }
}
