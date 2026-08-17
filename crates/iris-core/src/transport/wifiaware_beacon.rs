//! WIFIAWARE-001 — NAN discovery beacon: build + strict parse (AC-2/AC-6).
//!
//! v1 discovery (RES-0020 R1, WIFI_AWARE_TRANSPORT_DESIGN.md §2.2) uses
//! publish/subscribe NAN semantics with the IRIS **discovery beacon** carried
//! in `service_specific_info` (+ `match_filter`). A match produces a *candidate
//! peer* only — identity is resolved later from the verified advertisement /
//! envelope (IDENT-001); this beacon is a hint, never a trust boundary
//! (DEC-WA-0007).
//!
//! Format is deliberately tiny (fits `characteristics.maxServiceSpecificInfo
//! Length` — bounded, usually ~255 B on Android) and parsed **defensively**:
//! malformed/truncated/oversized payloads are rejected without panic or
//! allocation blowup (RES-0020 R8: the Android Wi-Fi-stack CVE pattern is
//! parser/state bugs).
//!
//! Format (fixed-width, versioned, no length ambiguity — mirrors the BLE-001
//! discovery beacon shape for uniformity across transports):
//! ```text
//! [0]     version     u8      = 1
//! [1..3]  capability  u16 BE  (bitmask, see CapabilityBits)
//! [3]     beacon kind u8      = 0 (discovery-only)
//! [4..20] peer_short  u8x16   (SHA-256(pubkey)[..16], IDENT-001 peer_short)
//! [20..22] freshness  u16 BE  (minutes since epoch mod 2^16, anti-stale hint)
//! ```
//! Total = 22 bytes. Signing/integrity lives in the IRIS envelope / verified
//! advertisement layer; the NAN MAC is never an identity (platform randomizes
//! it ~30 min) — DEC-WA-0007.

use crate::message::PeerId;

/// Beacon wire length in bytes.
pub const WIFI_AWARE_BEACON_LEN: usize = 22;
/// Current beacon version.
pub const WIFI_AWARE_BEACON_VERSION: u8 = 1;
/// Discovery-only beacon kind.
pub const KIND_DISCOVERY: u8 = 0;

/// NAN/NDP-related capability bit flags (beacon subset of the transport caps).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityBits(u16);

impl CapabilityBits {
    pub const NDP_UNICAST: u16 = 1 << 0;
    pub const PUBLISH_BROADCAST: u16 = 1 << 1;
    pub const RANGING: u16 = 1 << 2;
    pub const GEOFENCE: u16 = 1 << 3;
    pub const SUSPEND_RESUME: u16 = 1 << 4;

    /// Known mask — bits above this are reserved and rejected on parse.
    pub const fn all() -> Self {
        CapabilityBits(0b11111)
    }
    /// No capability bits set.
    pub const fn empty() -> Self {
        CapabilityBits(0)
    }
    pub const fn from_bits(bits: u16) -> Self {
        CapabilityBits(bits & Self::all().bits())
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

/// Parse/validation error for a NAN discovery beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeaconError {
    /// Payload shorter than `WIFI_AWARE_BEACON_LEN`.
    TooShort,
    /// Payload longer than `MAX_SERVICE_SPECIFIC_INFO_BYTES` (impossible for
    /// our own `build`, defensive against a hostile adapter return).
    TooLong,
    /// Unsupported version.
    UnsupportedVersion(u8),
    /// Unknown beacon kind.
    UnknownKind(u8),
    /// Reserved/capability bits set outside the known mask.
    ReservedBits,
}

impl std::fmt::Display for BeaconError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "wifi-aware beacon: {self:?}")
    }
}

impl std::error::Error for BeaconError {}

/// A parsed NAN discovery beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WifiAwareBeacon {
    pub capabilities: CapabilityBits,
    pub peer_short: [u8; 16],
    /// Minutes since epoch mod 2^16 (freshness hint; never a trust boundary).
    pub freshness_minutes: u16,
}

impl WifiAwareBeacon {
    /// Build the `service_specific_info` bytes for a peer's publish beacon.
    pub fn build(
        capabilities: CapabilityBits,
        peer_short: [u8; 16],
        freshness_minutes: u16,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIFI_AWARE_BEACON_LEN);
        out.push(WIFI_AWARE_BEACON_VERSION);
        out.extend_from_slice(&capabilities.bits().to_be_bytes());
        out.push(KIND_DISCOVERY);
        out.extend_from_slice(&peer_short);
        out.extend_from_slice(&freshness_minutes.to_be_bytes());
        debug_assert_eq!(out.len(), WIFI_AWARE_BEACON_LEN);
        out
    }

    /// Strictly parse `service_specific_info`. Rejects undersized,
    /// oversized-past-cap, version-mismatched, unknown-kind, and reserved-bit
    /// payloads.
    pub fn parse(bytes: &[u8]) -> Result<Self, BeaconError> {
        if bytes.len() < WIFI_AWARE_BEACON_LEN {
            return Err(BeaconError::TooShort);
        }
        if bytes.len() > MAX_SERVICE_SPECIFIC_INFO_BYTES {
            return Err(BeaconError::TooLong);
        }
        let version = bytes[0];
        if version != WIFI_AWARE_BEACON_VERSION {
            return Err(BeaconError::UnsupportedVersion(version));
        }
        let caps = u16::from_be_bytes([bytes[1], bytes[2]]);
        if caps & !CapabilityBits::all().bits() != 0 {
            return Err(BeaconError::ReservedBits);
        }
        let kind = bytes[3];
        if kind != KIND_DISCOVERY {
            return Err(BeaconError::UnknownKind(kind));
        }
        let mut peer_short = [0u8; 16];
        peer_short.copy_from_slice(&bytes[4..20]);
        let freshness = u16::from_be_bytes([bytes[20], bytes[21]]);
        Ok(WifiAwareBeacon {
            capabilities: CapabilityBits(caps),
            peer_short,
            freshness_minutes: freshness,
        })
    }

    /// Surface the short id as a zero-padded `PeerId` for the discovery stream
    /// (candidate hint only — the full identity is resolved on the verified
    /// advertisement / envelope path, DEC-WA-0007).
    pub fn candidate_peer_id(&self) -> PeerId {
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&self.peer_short);
        PeerId(id)
    }
}

/// Upper bound on `service_specific_info` bytes the transport will produce or
/// accept (Android `characteristics.maxServiceSpecificInfoLength` is typically
/// 255 bytes; we never build anything this large).
pub const MAX_SERVICE_SPECIFIC_INFO_BYTES: usize = 255;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parse_round_trip() {
        let b = WifiAwareBeacon::build(CapabilityBits::all(), [0xAB; 16], 42);
        assert_eq!(b.len(), WIFI_AWARE_BEACON_LEN);
        let parsed = WifiAwareBeacon::parse(&b).unwrap();
        assert_eq!(parsed.peer_short, [0xAB; 16]);
        assert_eq!(parsed.freshness_minutes, 42);
        assert!(parsed.capabilities.contains(CapabilityBits::NDP_UNICAST));
        assert!(parsed
            .capabilities
            .contains(CapabilityBits::PUBLISH_BROADCAST));
        assert!(parsed.capabilities.contains(CapabilityBits::RANGING));
    }

    #[test]
    fn too_short_rejected_no_panic() {
        assert_eq!(
            WifiAwareBeacon::parse(&[1u8; 5]).unwrap_err(),
            BeaconError::TooShort
        );
        assert_eq!(
            WifiAwareBeacon::parse(&[]).unwrap_err(),
            BeaconError::TooShort
        );
        // Exhaustive short-length sweep (parser robustness, RES-0020 R8).
        for len in 0..WIFI_AWARE_BEACON_LEN {
            let bytes = vec![0x11u8; len];
            assert_eq!(
                WifiAwareBeacon::parse(&bytes).unwrap_err(),
                BeaconError::TooShort,
                "len {len} must reject"
            );
        }
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut b = WifiAwareBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        b[0] = 99;
        assert_eq!(
            WifiAwareBeacon::parse(&b).unwrap_err(),
            BeaconError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn unknown_kind_rejected() {
        let mut b = WifiAwareBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        b[3] = 0x7F;
        assert_eq!(
            WifiAwareBeacon::parse(&b).unwrap_err(),
            BeaconError::UnknownKind(0x7F)
        );
    }

    #[test]
    fn reserved_bits_rejected() {
        let mut b = WifiAwareBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        // Flip a bit above the known mask (bit 5..15).
        b[1] |= 0x80;
        assert_eq!(
            WifiAwareBeacon::parse(&b).unwrap_err(),
            BeaconError::ReservedBits
        );
    }

    #[test]
    fn oversized_padded_service_info_tolerated_without_panic() {
        // Android may pad/embed extra service info beyond the 22-byte prefix;
        // the prefix parses fine and extra bytes are ignored (defensive).
        let b = WifiAwareBeacon::build(
            CapabilityBits::from_bits(CapabilityBits::NDP_UNICAST),
            [7u8; 16],
            1,
        );
        let mut padded = b.clone();
        padded.extend_from_slice(&[0u8; 230]);
        assert_eq!(padded.len(), 252);
        let parsed = WifiAwareBeacon::parse(&padded).unwrap();
        assert_eq!(parsed.peer_short, [7u8; 16]);
    }

    #[test]
    fn oversized_past_cap_rejected_no_panic() {
        // Above the documented platform cap (255 B) → TooLong, even though
        // the 22-byte prefix is well-formed (defensive upper bound).
        let base = WifiAwareBeacon::build(
            CapabilityBits::from_bits(CapabilityBits::NDP_UNICAST),
            [7u8; 16],
            1,
        );
        let mut padded = base.clone();
        padded.extend_from_slice(&[0u8; MAX_SERVICE_SPECIFIC_INFO_BYTES]); // 22+255=277
        assert_eq!(
            WifiAwareBeacon::parse(&padded).unwrap_err(),
            BeaconError::TooLong
        );
        // Boundary: exactly the cap is still accepted.
        let mut at_cap = base.clone();
        at_cap.extend_from_slice(&[0u8; MAX_SERVICE_SPECIFIC_INFO_BYTES - WIFI_AWARE_BEACON_LEN]);
        assert_eq!(at_cap.len(), MAX_SERVICE_SPECIFIC_INFO_BYTES);
        assert!(WifiAwareBeacon::parse(&at_cap).is_ok());
    }

    #[test]
    fn candidate_peer_id_pads_short_id() {
        let mut short = [0u8; 16];
        short[0] = 0xDE;
        let b = WifiAwareBeacon::build(CapabilityBits::empty(), short, 0);
        let parsed = WifiAwareBeacon::parse(&b).unwrap();
        let id = parsed.candidate_peer_id();
        assert_eq!(&id.0[..16], &short);
        assert_eq!(&id.0[16..], &[0u8; 16]);
    }
}
