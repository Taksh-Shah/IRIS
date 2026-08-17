//! BLE-001 — connectionless discovery beacon: build + strict parse (AC-3/AC-6).
//!
//! The v1 discovery model (RES-0019 R1) uses Extended/Periodic advertising as a
//! **discovery beacon only** — a small signed-capable payload identifying the
//! node; bulk payload rides on GATT once connected. IRIS peers are identified by
//! their public-key fingerprint, never MAC (MAC randomizes every ~15 min on
//! Android; RES-0007/BLE.md §MAC Randomization).
//!
//! The beacon is deliberately tiny (fits the legacy 251 B and iOS-compatible
//! service-UUID subset; RES-0019 R7) and parsed **defensively** — malformed/
//! truncated/oversized adverts must be rejected without panic or allocation
//! blowup (RES-0019 R5: the Android stack CVE pattern is parser/state bugs).
//!
//! Format (fixed-width, versioned, no length ambiguity):
//! ```text
//! [0]    version     u8      = 1
//! [1..3] capability  u16 BE  (bitmask, see CapabilityBits)
//! [3]    beacon kind u8      = 0 (discovery-only)
//! [4..20] peer_short u8x16   (SHA-256(pubkey)[..16], IDENT-001 peer_short)
//! [20..22] freshness u16 BE  (minutes since epoch mod 2^16, anti-stale hint)
//! ```
//! Total = 22 bytes. Signing/integrity lives in the IRIS envelope / verified
//! advertisement layer (IDENT-001 KeyAdvertisementV1); this beacon is a *candidate*
//! hint, never a trust boundary (DEC-BLE-0006).

use crate::message::PeerId;

/// Beacon wire length in bytes.
pub const BEACON_LEN: usize = 22;
/// Current beacon version.
pub const BEACON_VERSION: u8 = 1;
/// Discovery-only beacon kind.
pub const KIND_DISCOVERY: u8 = 0;

/// Capability bit flags mirrored from `TransportCapabilities`-relevant features.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityBits(u16);

impl CapabilityBits {
    pub const GATT_UNICAST: u16 = 1 << 0;
    pub const ADVERTISE_BROADCAST: u16 = 1 << 1;
    pub const EXTENDED_ADV: u16 = 1 << 2;
    pub const PERIODIC_ADV: u16 = 1 << 3;
    pub const CODED_PHY: u16 = 1 << 4;

    pub const fn empty() -> Self {
        CapabilityBits(0)
    }
    pub const fn all() -> Self {
        CapabilityBits(0b11111)
    }
    pub const fn from_bits(bits: u16) -> Self {
        CapabilityBits(bits & 0b11111)
    }
    pub fn bits(self) -> u16 {
        self.0
    }
    pub fn set(&mut self, bit: u16) {
        self.0 |= bit;
    }
    pub fn contains(self, bit: u16) -> bool {
        self.0 & bit != 0
    }
}

/// Parse/validation error for a beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvertError {
    /// Beacon shorter than `BEACON_LEN`.
    TooShort,
    /// Unsupported version.
    UnsupportedVersion(u8),
    /// Unknown beacon kind.
    UnknownKind(u8),
    /// Reserved/capability bits set outside the known mask.
    ReservedBits,
}

impl std::fmt::Display for AdvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ble advert: {self:?}")
    }
}

impl std::error::Error for AdvertError {}

/// A parsed discovery beacon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryBeacon {
    pub capabilities: CapabilityBits,
    pub peer_short: [u8; 16],
    /// Minutes since epoch mod 2^16 (freshness hint; never a trust boundary).
    pub freshness_minutes: u16,
}

impl DiscoveryBeacon {
    /// Build a beacon for a peer.
    pub fn build(
        capabilities: CapabilityBits,
        peer_short: [u8; 16],
        freshness_minutes: u16,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(BEACON_LEN);
        out.push(BEACON_VERSION);
        out.extend_from_slice(&capabilities.bits().to_be_bytes());
        out.push(KIND_DISCOVERY);
        out.extend_from_slice(&peer_short);
        out.extend_from_slice(&freshness_minutes.to_be_bytes());
        debug_assert_eq!(out.len(), BEACON_LEN);
        out
    }

    /// Strictly parse a beacon. Rejects undersized, version-mismatched,
    /// unknown-kind, and reserved-bit payloads.
    pub fn parse(bytes: &[u8]) -> Result<Self, AdvertError> {
        if bytes.len() < BEACON_LEN {
            return Err(AdvertError::TooShort);
        }
        let version = bytes[0];
        if version != BEACON_VERSION {
            return Err(AdvertError::UnsupportedVersion(version));
        }
        let caps = u16::from_be_bytes([bytes[1], bytes[2]]);
        if caps & !CapabilityBits::all().bits() != 0 {
            return Err(AdvertError::ReservedBits);
        }
        let kind = bytes[3];
        if kind != KIND_DISCOVERY {
            return Err(AdvertError::UnknownKind(kind));
        }
        let mut peer_short = [0u8; 16];
        peer_short.copy_from_slice(&bytes[4..20]);
        let freshness = u16::from_be_bytes([bytes[20], bytes[21]]);
        Ok(DiscoveryBeacon {
            capabilities: CapabilityBits(caps),
            peer_short,
            freshness_minutes: freshness,
        })
    }

    /// Surface the short id as a zero-padded `PeerId` for the discovery stream.
    /// The full 32-byte identity is resolved only after the GATT handshake /
    /// verified advertisement (IDENT-001); here it is a candidate hint.
    pub fn candidate_peer_id(&self) -> PeerId {
        let mut id = [0u8; 32];
        id[..16].copy_from_slice(&self.peer_short);
        PeerId(id)
    }
}

/// Maximum bytes of advertised payload IRIS will scan/accept.
pub const MAX_ADVERT_BYTES: usize = 512;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_parse_round_trip() {
        let b = DiscoveryBeacon::build(CapabilityBits::all(), [0xAB; 16], 42);
        assert_eq!(b.len(), BEACON_LEN);
        let parsed = DiscoveryBeacon::parse(&b).unwrap();
        assert_eq!(parsed.peer_short, [0xAB; 16]);
        assert_eq!(parsed.freshness_minutes, 42);
        assert!(parsed.capabilities.contains(CapabilityBits::GATT_UNICAST));
        assert!(parsed.capabilities.contains(CapabilityBits::PERIODIC_ADV));
    }

    #[test]
    fn too_short_rejected() {
        assert_eq!(
            DiscoveryBeacon::parse(&[1u8; 5]).unwrap_err(),
            AdvertError::TooShort
        );
        assert_eq!(
            DiscoveryBeacon::parse(&[]).unwrap_err(),
            AdvertError::TooShort
        );
    }

    #[test]
    fn unsupported_version_rejected() {
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        b[0] = 99;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn unknown_kind_rejected() {
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        b[3] = 0x7F;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::UnknownKind(0x7F)
        );
    }

    #[test]
    fn reserved_bits_rejected() {
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0);
        // Flip a bit above the known mask (bit 5..15).
        b[1] |= 0x80;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::ReservedBits
        );
    }

    #[test]
    fn oversized_payload_tolerated_without_panic() {
        // A 512-byte padded advert must parse the 22-byte prefix fine (BLE
        // adverts may be padded); anything shorter is rejected.
        let b = DiscoveryBeacon::build(
            CapabilityBits::from_bits(CapabilityBits::GATT_UNICAST),
            [7u8; 16],
            1,
        );
        let mut padded = b.clone();
        padded.extend_from_slice(&[0u8; 490]);
        assert_eq!(padded.len(), MAX_ADVERT_BYTES);
        let parsed = DiscoveryBeacon::parse(&padded).unwrap();
        assert_eq!(parsed.peer_short, [7u8; 16]);
    }

    #[test]
    fn all_len_2_to_22_rejected_no_panic() {
        // Exhaustive short-length sweep: every length under BEACON_LEN is
        // rejected without panicking (parser robustness regression, RES-0019 R5).
        for len in 0..BEACON_LEN {
            let bytes = vec![0x11u8; len];
            assert_eq!(
                DiscoveryBeacon::parse(&bytes).unwrap_err(),
                AdvertError::TooShort,
                "len {len} must reject"
            );
        }
    }

    #[test]
    fn candidate_peer_id_pads_short_id() {
        let mut short = [0u8; 16];
        short[0] = 0xDE;
        let b = DiscoveryBeacon::build(CapabilityBits::empty(), short, 0);
        let parsed = DiscoveryBeacon::parse(&b).unwrap();
        let id = parsed.candidate_peer_id();
        assert_eq!(&id.0[..16], &short);
        assert_eq!(&id.0[16..], &[0u8; 16]);
    }
}
