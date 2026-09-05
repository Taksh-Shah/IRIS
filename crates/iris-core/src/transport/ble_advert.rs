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
//! v1 (legacy, 22 bytes):
//! [0]    version     u8      = 1
//! [1..3] capability  u16 BE  (bitmask, see CapabilityBits)
//! [3]    beacon kind u8      = 0 (discovery-only)
//! [4..20] peer_short u8x16   (SHA-256(pubkey)[..16], IDENT-001 peer_short)
//! [20..22] freshness u16 BE  (minutes since epoch mod 2^16, anti-stale hint)
//!
//! v2 (current, 28 bytes — DEC-BLE-0008/HV-21):
//! [0..22] identical to v1
//! [22..28] wifi_direct_mac  u8x6  (this node's Wi-Fi P2P device address, or
//!          all-zero if not yet known to the sender — see below)
//! ```
//! Total = 22 bytes (v1) or 28 bytes (v2). Signing/integrity lives in the
//! IRIS envelope / verified advertisement layer (IDENT-001
//! KeyAdvertisementV1); this beacon is a *candidate* hint, never a trust
//! boundary (DEC-BLE-0006).
//!
//! **DEC-BLE-0008 (HV-21, 2026-09-05): why the beacon carries a Wi-Fi P2P
//! MAC.** Wi-Fi Direct's own DNS-SD service discovery (`discoverServices`/
//! `addServiceRequest`/TXT records) was found, on real hardware, to never
//! resolve at all — zero PTR/TXT listener callbacks fired across an
//! extended bench session, while the platform's plain `discoverPeers()` API
//! (no app-level filtering) *does* find nearby devices reliably. Plain peer
//! discovery alone is not enough on its own, though: it returns every
//! Wi-Fi-Direct-capable device in range with no way to know which one is
//! running IRIS or what its `PeerId` is — connecting blindly to an unknown
//! device is both wasteful and can surface an OS "Invitation" prompt on a
//! random stranger's phone. The fix: reuse the BLE control-plane channel
//! IRIS already has for exactly this purpose ("Wi-Fi Direct is the data
//! plane on the BLE control plane" — module doc). Once BLE tells us a
//! peer's `PeerId` *and* its Wi-Fi P2P MAC, Wi-Fi Direct's own
//! `discoverPeers()`/`requestPeers()` results are filtered against that
//! known-MAC set — only a device already identified over BLE is ever
//! connected to, and the DNS-SD path (and its OEM-fragile TXT listeners,
//! HV-21) is no longer needed at all.
//!
//! **`build` emits whichever wire size fits the caller's `wifi_direct_mac`
//! argument** — `None` produces the plain 22-byte v1 form, `Some` produces
//! the 28-byte v2 form. This is deliberate, not just a size optimisation:
//! `ble.rs::start_advertising` (the only production caller) was confirmed
//! on real hardware to hit `ADVERTISE_FAILED_DATA_TOO_LARGE` on **every**
//! attempt once a v2-sized payload was broadcast — legacy BLE advertising
//! has a hard ~31-byte total ceiling (flags + service-UUID + this
//! service-data AD structure) that the original "fits the legacy 251 B"
//! comment above understated. So `start_advertising` always passes `None`
//! today; the MAC is stored (`BleTransport::wifi_direct_mac`,
//! `set_local_wifi_direct_mac`) but **not yet actually carried over the
//! air** — doing so needs the GATT identify-characteristic fallback path
//! instead (already used when the beacon itself can't fit the advert, see
//! `discover_peers`'s `gatt_read` call), which has no such size ceiling.
//! That wiring is not yet implemented; this module only has the wire
//! format and the codec ready for it. `parse` accepts both v1 and v2 so
//! whichever caller eventually emits v2 does not break the other side.

use crate::message::PeerId;

/// Legacy (v1) beacon wire length in bytes — no Wi-Fi Direct MAC.
pub const BEACON_LEN: usize = 22;
/// Current (v2) beacon wire length in bytes — adds the 6-byte P2P MAC.
pub const BEACON_LEN_V2: usize = 28;
/// Legacy beacon version (still parsed for backward compatibility).
pub const BEACON_VERSION_V1: u8 = 1;
/// Current beacon version — `build` always emits this.
pub const BEACON_VERSION: u8 = 2;
/// Discovery-only beacon kind.
pub const KIND_DISCOVERY: u8 = 0;
/// Byte width of the Wi-Fi Direct MAC field.
const WIFI_DIRECT_MAC_LEN: usize = 6;

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
    /// BLE-20: mask to the known set so `build`→`parse` always round-trips.
    pub fn set(&mut self, bit: u16) {
        self.0 |= bit & Self::all().bits();
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
    /// Beacon longer than `MAX_ADVERT_BYTES` (BLE-21).
    TooLong,
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
    /// This peer's Wi-Fi Direct (P2P) device address, if the sender knew it
    /// at build time (HV-21/DEC-BLE-0008) — `None` for a v1 beacon or a v2
    /// beacon built before the platform told the sender its own address
    /// (all-zero MAC is treated as "unknown", never a real address).
    pub wifi_direct_mac: Option<[u8; 6]>,
}

impl DiscoveryBeacon {
    /// Build a beacon for a peer. **Emits the legacy 22-byte v1 wire form
    /// when `wifi_direct_mac` is `None`** — legacy BLE advertising has a
    /// hard payload ceiling (31 bytes total including the flags and
    /// service-UUID AD structures) that this hardware was confirmed to hit
    /// with a v2, always-28-byte beacon (`ADVERTISE_FAILED_DATA_TOO_LARGE`
    /// on every attempt — see `ble.rs::start_advertising`'s own comment).
    /// Only builds the v2 (28-byte) form — with the MAC appended — when
    /// `Some` is actually passed; a caller advertising over the air must
    /// pass `None` (see `ble.rs`) until the MAC is carried some other way
    /// (the GATT identify-characteristic path, not yet implemented).
    pub fn build(
        capabilities: CapabilityBits,
        peer_short: [u8; 16],
        freshness_minutes: u16,
        wifi_direct_mac: Option<[u8; 6]>,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(BEACON_LEN_V2);
        out.push(if wifi_direct_mac.is_some() { BEACON_VERSION } else { BEACON_VERSION_V1 });
        out.extend_from_slice(&capabilities.bits().to_be_bytes());
        out.push(KIND_DISCOVERY);
        out.extend_from_slice(&peer_short);
        out.extend_from_slice(&freshness_minutes.to_be_bytes());
        if let Some(mac) = wifi_direct_mac {
            out.extend_from_slice(&mac);
            debug_assert_eq!(out.len(), BEACON_LEN_V2);
        } else {
            debug_assert_eq!(out.len(), BEACON_LEN);
        }
        out
    }

    /// Strictly parse a beacon. Accepts v1 (22 B) and v2 (28 B, HV-21)
    /// payloads; rejects undersized, version-mismatched, unknown-kind, and
    /// reserved-bit payloads.
    pub fn parse(bytes: &[u8]) -> Result<Self, AdvertError> {
        if bytes.len() < BEACON_LEN {
            return Err(AdvertError::TooShort);
        }
        // BLE-21: reject oversized adverts before any allocation.
        if bytes.len() > MAX_ADVERT_BYTES {
            return Err(AdvertError::TooLong);
        }
        let version = bytes[0];
        if version != BEACON_VERSION && version != BEACON_VERSION_V1 {
            return Err(AdvertError::UnsupportedVersion(version));
        }
        // A v2 sender's payload must actually carry the MAC field; a v1
        // beacon padded to 28 bytes (e.g. by an intermediate stack) would
        // otherwise silently misparse its own version byte as v2-shaped.
        if version == BEACON_VERSION && bytes.len() < BEACON_LEN_V2 {
            return Err(AdvertError::TooShort);
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
        let wifi_direct_mac = if version == BEACON_VERSION {
            let mut mac = [0u8; WIFI_DIRECT_MAC_LEN];
            mac.copy_from_slice(&bytes[22..28]);
            if mac == [0u8; WIFI_DIRECT_MAC_LEN] {
                None
            } else {
                Some(mac)
            }
        } else {
            None
        };
        Ok(DiscoveryBeacon {
            capabilities: CapabilityBits(caps),
            peer_short,
            freshness_minutes: freshness,
            wifi_direct_mac,
        })
    }

    /// Surface the short id as a sentinel-padded `PeerId` for the discovery stream.
    /// The full 32-byte identity is resolved only after the GATT handshake /
    /// verified advertisement (IDENT-001); here it is a candidate hint.
    ///
    /// BLE-8: upper 16 bytes are set to `0xFF` so a candidate id can never
    /// collide with a verified `PeerId` (derived from a 32-byte public-key hash
    /// with cryptographically negligible probability of all-ones upper half).
    pub fn candidate_peer_id(&self) -> PeerId {
        let mut id = [0xFFu8; 32];
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
        let b = DiscoveryBeacon::build(
            CapabilityBits::all(),
            [0xAB; 16],
            42,
            Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]),
        );
        assert_eq!(b.len(), BEACON_LEN_V2);
        let parsed = DiscoveryBeacon::parse(&b).unwrap();
        assert_eq!(parsed.peer_short, [0xAB; 16]);
        assert_eq!(parsed.freshness_minutes, 42);
        assert!(parsed.capabilities.contains(CapabilityBits::GATT_UNICAST));
        assert!(parsed.capabilities.contains(CapabilityBits::PERIODIC_ADV));
        assert_eq!(
            parsed.wifi_direct_mac,
            Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66])
        );
    }

    #[test]
    fn build_with_no_known_p2p_mac_parses_as_none() {
        // HV-21: the sender may not know its own Wi-Fi Direct address yet
        // (the platform broadcast hasn't landed) — all-zero on the wire,
        // `None` once parsed, never mistaken for a real address.
        let b = DiscoveryBeacon::build(CapabilityBits::empty(), [1u8; 16], 0, None);
        let parsed = DiscoveryBeacon::parse(&b).unwrap();
        assert_eq!(parsed.wifi_direct_mac, None);
    }

    #[test]
    fn legacy_v1_beacon_still_parses_without_a_mac() {
        // DEC-BLE-0008: a peer still running the old build() (22 B, version
        // 1, no MAC field) must remain fully readable by the upgraded
        // parser — this is the one-way compatibility guarantee the doc
        // comment promises.
        let mut v1 = Vec::with_capacity(BEACON_LEN);
        v1.push(BEACON_VERSION_V1);
        v1.extend_from_slice(&CapabilityBits::all().bits().to_be_bytes());
        v1.push(KIND_DISCOVERY);
        v1.extend_from_slice(&[0x9Au8; 16]);
        v1.extend_from_slice(&7u16.to_be_bytes());
        assert_eq!(v1.len(), BEACON_LEN);

        let parsed = DiscoveryBeacon::parse(&v1).unwrap();
        assert_eq!(parsed.peer_short, [0x9Au8; 16]);
        assert_eq!(parsed.freshness_minutes, 7);
        assert_eq!(parsed.wifi_direct_mac, None);
    }

    #[test]
    fn v2_version_byte_with_v1_length_is_rejected_not_misparsed() {
        // A payload claiming version 2 but truncated to the v1 length must
        // not silently read garbage/zero bytes as a MAC — reject it as
        // TooShort rather than fabricating a "None" MAC that could hide a
        // real, truncated one.
        let mut truncated = Vec::with_capacity(BEACON_LEN);
        truncated.push(BEACON_VERSION);
        truncated.extend_from_slice(&CapabilityBits::empty().bits().to_be_bytes());
        truncated.push(KIND_DISCOVERY);
        truncated.extend_from_slice(&[0u8; 16]);
        truncated.extend_from_slice(&0u16.to_be_bytes());
        assert_eq!(truncated.len(), BEACON_LEN);
        assert_eq!(
            DiscoveryBeacon::parse(&truncated).unwrap_err(),
            AdvertError::TooShort
        );
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
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0, None);
        b[0] = 99;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::UnsupportedVersion(99)
        );
    }

    #[test]
    fn unknown_kind_rejected() {
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0, None);
        b[3] = 0x7F;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::UnknownKind(0x7F)
        );
    }

    #[test]
    fn reserved_bits_rejected() {
        let mut b = DiscoveryBeacon::build(CapabilityBits::empty(), [0u8; 16], 0, None);
        // Flip a bit above the known mask (bit 5..15).
        b[1] |= 0x80;
        assert_eq!(
            DiscoveryBeacon::parse(&b).unwrap_err(),
            AdvertError::ReservedBits
        );
    }

    #[test]
    fn advert_size_bounds_enforced() {
        // Payloads up to MAX_ADVERT_BYTES are accepted (BLE 5 extended adverts
        // may be padded); anything strictly over must be rejected without panic
        // (BLE-21 / RES-0019 R5).
        let b = DiscoveryBeacon::build(
            CapabilityBits::from_bits(CapabilityBits::GATT_UNICAST),
            [7u8; 16],
            1,
            None,
        );
        let mut padded = b.clone();
        padded.extend_from_slice(&vec![0u8; MAX_ADVERT_BYTES - BEACON_LEN]);
        assert_eq!(padded.len(), MAX_ADVERT_BYTES);
        let parsed = DiscoveryBeacon::parse(&padded).unwrap();
        assert_eq!(parsed.peer_short, [7u8; 16]);
        // One byte over the cap must be rejected.
        let mut oversize = padded.clone();
        oversize.push(0u8);
        assert_eq!(
            DiscoveryBeacon::parse(&oversize).unwrap_err(),
            AdvertError::TooLong,
            "payloads > MAX_ADVERT_BYTES must be rejected"
        );
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
    fn candidate_peer_id_uses_sentinel_upper_half() {
        let mut short = [0u8; 16];
        short[0] = 0xDE;
        let b = DiscoveryBeacon::build(CapabilityBits::empty(), short, 0, None);
        let parsed = DiscoveryBeacon::parse(&b).unwrap();
        let id = parsed.candidate_peer_id();
        // lower 16 bytes = peer_short; upper 16 bytes = 0xFF sentinel (BLE-8)
        assert_eq!(&id.0[..16], &short);
        assert_eq!(&id.0[16..], &[0xFFu8; 16]);
    }
}
