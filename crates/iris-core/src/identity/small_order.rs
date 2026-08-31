//! RED-0011 — small-order X25519 public-key rejection.
//!
//! Rejects remote X25519 public keys that lie in the small-order (torsion)
//! subgroup before any Diffie–Hellman is attempted with them. A peer that
//! substitutes a low-order point forces the shared secret into a tiny set,
//! letting the attacker brute-force the key and/or inject a zero secret.
//!
//! Complements — not replaces — the RFC 7748 §6.1 all-zero shared-secret
//! reject already enforced in `crypto::x25519::diffie_hellman`: RED-0011
//! catches the *point substitution itself* at the trust/chain boundary
//! (advertisement adoption, chain validation), BEFORE a DH is attempted.
//!
//! Detection uses the curve's own torsion test (curve25519-dalek
//! `EdwardsPoint::is_small_order`, i.e. membership in E[8]) after mapping the
//! Montgomery u-coordinate back to the main curve. A u that maps to the twist
//! (`to_edwards` → `None`) is not a main-curve low-order point and is left to
//! the §6.1 all-zero check (RFC 7748 §6 and §6.1).

use curve25519_dalek::montgomery::MontgomeryPoint;

/// The classic low-order u-coordinates carried in the literature and in
/// curve25519-dalek's own torsion table (E[8]). Kept as explicit fast-path
/// constants so `is_small_order` never depends on decompression succeeding:
/// `MontgomeryPoint::to_edwards` returns `None` for the twist, but the four
/// canonical low-order points (order 2 / 4 / 8, plus identity u = 0) all
/// decompress — the constants are a second, checkable safety net.
pub const SMALL_ORDER_U: [[u8; 32]; 4] = [
    // u = 0 (identity under the birationally mapped DH — RFC 7748 §6.1)
    [0u8; 32],
    // u = 1
    [
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0,
    ],
    // u = 325606250916557431795983626356110631294008115727848805560023387167927233504
    // PRY-31: this entry was mistyped (divergent from byte 6, with an internal
    // byte-run repeat). Value below is `EIGHT_TORSION[1].to_montgomery()` from
    // curve25519-dalek — cross-checked by `small_order_constants_are_genuine`.
    [
        0xe0, 0xeb, 0x7a, 0x7c, 0x3b, 0x41, 0xb8, 0xae, 0x16, 0x56, 0xe3, 0xfa, 0xf1, 0x9f, 0xc4,
        0x6a, 0xda, 0x09, 0x8d, 0xeb, 0x9c, 0x32, 0xb1, 0xfd, 0x86, 0x62, 0x05, 0x16, 0x5f, 0x49,
        0xb8, 0x00,
    ],
    // u = 39382357235489614581723060781553021112529911719440698176882885853963445705823
    [
        0x5f, 0x9c, 0x95, 0xbc, 0xa3, 0x50, 0x8c, 0x24, 0xb1, 0xd0, 0xb1, 0x55, 0x9c, 0x83, 0xef,
        0x5b, 0x04, 0x44, 0x5c, 0xc4, 0x58, 0x1c, 0x8e, 0x86, 0xd8, 0x22, 0x4e, 0xdd, 0xd0, 0x9f,
        0x11, 0x57,
    ],
];

/// Whether `public_key` is a small-order X25519 public key (RED-0011).
///
/// Checks the explicit classic constants first (cheap, deterministic), then
/// falls back to the E[8] membership test for any other low-order point.
pub fn is_small_order(public_key: &[u8; 32]) -> bool {
    if SMALL_ORDER_U.iter().any(|u| u == public_key) {
        return true;
    }
    // PRY-31: reject non-canonical field-element encodings (`u >= p`, ignoring
    // the always-masked high bit). `x25519-dalek`'s `MontgomeryPoint` does not
    // reduce its input, so `p` (≡ 0) and `p+1` (≡ 1) would otherwise agree to
    // the same weak secret as the identity / u = 1 without matching any entry
    // in `SMALL_ORDER_U`.
    if !is_canonical_u(public_key) {
        return true;
    }
    let mp = MontgomeryPoint(*public_key);
    // `to_edwards` fails for twist points; a twist point is not a main-curve
    // low-order key and is handled by the all-zero shared-secret reject.
    [0u8, 1u8]
        .iter()
        .filter_map(|&sign| mp.to_edwards(sign))
        .any(|p| p.is_small_order())
}

/// Whether `u` is `< p = 2^255 - 19` once the (always ignored) high bit is
/// masked — i.e. a canonical X25519 field-element encoding.
fn is_canonical_u(u: &[u8; 32]) -> bool {
    // p in little-endian: 0xed, then 0xff * 30, then 0x7f.
    const P_LE: [u8; 32] = [
        0xed, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x7f,
    ];
    let mut masked = *u;
    masked[31] &= 0x7f;
    // Compare big-endian (most-significant byte first).
    for i in (0..32).rev() {
        match masked[i].cmp(&P_LE[i]) {
            std::cmp::Ordering::Less => return true,
            std::cmp::Ordering::Greater => return false,
            std::cmp::Ordering::Equal => {}
        }
    }
    false // masked == p, not canonical
}

#[cfg(test)]
mod tests {
    use super::*;
    use curve25519_dalek::constants::{ED25519_BASEPOINT_POINT, EIGHT_TORSION};

    /// PRY-31: every `SMALL_ORDER_U` entry must be a genuine low-order point —
    /// decompress it to Edwards and confirm the curve agrees. A mistyped
    /// constant (the original `[2]`) fails this; it cannot pass by tautology
    /// the way `classic_small_order_values_are_rejected` did.
    #[test]
    fn small_order_constants_are_genuine() {
        for (i, u) in SMALL_ORDER_U.iter().enumerate() {
            let mp = MontgomeryPoint(*u);
            let decompressed_low_order = [0u8, 1u8]
                .iter()
                .filter_map(|&s| mp.to_edwards(s))
                .any(|p| p.is_small_order());
            assert!(
                decompressed_low_order,
                "SMALL_ORDER_U[{i}] = {u:02x?} is not a real low-order point"
            );
        }
    }

    #[test]
    fn non_canonical_identity_encodings_are_rejected() {
        // p (≡ 0) and p+1 (≡ 1): non-canonical, unreduced by x25519-dalek.
        let mut p = [0xffu8; 32];
        p[0] = 0xed;
        p[31] = 0x7f;
        let mut p_plus_1 = p;
        p_plus_1[0] = 0xee;
        assert!(is_small_order(&p), "u = p must be rejected");
        assert!(is_small_order(&p_plus_1), "u = p+1 must be rejected");
        // p-1 is canonical (< p); leave its classification to the curve test.
        let mut p_minus_1 = p;
        p_minus_1[0] = 0xec;
        assert!(is_canonical_u(&p_minus_1), "p-1 is a canonical encoding");
    }

    #[test]
    fn classic_small_order_values_are_rejected() {
        for u in SMALL_ORDER_U {
            assert!(is_small_order(&u), "low-order u must be rejected");
        }
    }

    #[test]
    fn whole_torsion_subgroup_is_rejected() {
        // Every element of E[8] (8 points) must be flagged. Converting the
        // curve's own torsion table to Montgomery covers all 8 elements,
        // generated independently of our hand-written constants.
        for tp in EIGHT_TORSION {
            let u = tp.to_montgomery().to_bytes();
            assert!(
                is_small_order(&u),
                "torsion point u={u:02x?} must be rejected"
            );
        }
    }

    #[test]
    fn valid_remote_key_accepted() {
        let kp = crate::crypto::keygen::X25519Keypair::generate();
        assert!(!is_small_order(&kp.public_bytes()));
    }

    #[test]
    fn basepoint_is_valid() {
        // The Ed25519 basepoint maps to a large prime-order point — never a
        // small-order rejection.
        let u = ED25519_BASEPOINT_POINT.to_montgomery().to_bytes();
        assert!(!is_small_order(&u));
    }
}
