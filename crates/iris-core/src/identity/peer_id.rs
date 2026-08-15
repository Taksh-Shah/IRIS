//! PeerId derivation — IDENT-001 D1 (DEC-P0005).
//!
//! A node's identity IS its Ed25519 public key (self-authenticating: the
//! verifier reconstructs `VerifyingKey::from_bytes` straight from the wire,
//! CRYPTO-001 `message_engine/crypto.rs`). No MAC / serial / hardware-derived
//! identity — CVE-2025-53627 (Meshtastic NodeNum, CVSS 8.2).
//!
//! Derived views:
//! - `peer_id`    = the full 32-byte `VerifyingKey` (wire identity).
//! - `peer_short` = `SHA-256(pubkey)[..16]` — truncated for the P0 abbreviated
//!   envelope (`sender_id` 16 B) / display. Deterministic across processes.
//! - `human_uid`  = uppercase base32 over `SHA-256(pubkey)[..10]` per the
//!   ADDRESSING.md human-address convention (display only, non-authoritative).
//!
//! The `IRIS_NODE_ID` env override remains a *UID convenience label* only —
//! never an identity or an input to authentication.

use sha2::{Digest, Sha256};

/// The Ed25519 verifying key bytes = the node's PeerId (32 bytes).
pub fn peer_id(verifying_key: &ed25519_dalek::VerifyingKey) -> [u8; 32] {
    verifying_key.to_bytes()
}

/// `SHA-256(pubkey)[..16]` — the 16-byte truncated form (P0 abbreviated
/// envelopes / display). Deterministic across calls and processes; distinct
/// keys → distinct shorts (collision resistance of a 128-bit truncation).
pub fn peer_short(pubkey: &[u8; 32]) -> [u8; 16] {
    let digest = Sha256::digest(pubkey);
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

/// Uppercase RFC 4648 base32 of `SHA-256(pubkey)[..10]` — ADDRESSING.md
/// human-address convention (display only).
pub fn human_uid(pubkey: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let digest = Sha256::digest(pubkey);
    let bytes = &digest[..10];
    let mut out = String::with_capacity(16);
    // 10 bytes → 16 base32 chars (RFC 4648, no padding).
    let mut acc: u64 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        acc = (acc << 8) | u64::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((acc >> bits) & 0x1F) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((acc << (5 - bits)) & 0x1F) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Convenience: `peer_short` as lowercase hex (matches P0 abbreviated-envelope
/// display used in the desktop shell / logs).
pub fn peer_short_hex(pubkey: &[u8; 32]) -> String {
    let s = peer_short(pubkey);
    let mut out = String::with_capacity(32);
    for b in s {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::IdentityKeypair;

    #[test]
    fn peer_id_is_verifying_key_bytes() {
        let kp = IdentityKeypair::generate();
        assert_eq!(peer_id(&kp.verifying), kp.verifying_bytes());
    }

    #[test]
    fn peer_short_same_key_same_short_id() {
        let kp = IdentityKeypair::generate();
        let pubkey = kp.verifying_bytes();
        assert_eq!(peer_short(&pubkey), peer_short(&pubkey));
        let hex_a = peer_short_hex(&pubkey);
        let hex_b = peer_short_hex(&pubkey);
        assert_eq!(hex_a, hex_b);
    }

    #[test]
    fn distinct_keys_produce_distinct_short_ids() {
        let a = IdentityKeypair::generate().verifying_bytes();
        let b = IdentityKeypair::generate().verifying_bytes();
        assert_ne!(a, b);
        assert_ne!(peer_short(&a), peer_short(&b));
    }

    #[test]
    fn peer_short_is_16_bytes() {
        let kp = IdentityKeypair::generate();
        assert_eq!(peer_short(&kp.verifying_bytes()).len(), 16);
    }

    #[test]
    fn no_hardware_derived_identity() {
        // The PeerId / short id derive exclusively from the key, never from a
        // MAC/serial: randomizing any "hardware" they might be confused with
        // (here simulated by a process-random key) changes the id entirely,
        // and there is no addressable hardware input in the derivation path.
        let a = IdentityKeypair::generate();
        let b = IdentityKeypair::generate();
        let short_a = peer_short_hex(&a.verifying_bytes());
        let short_b = peer_short_hex(&b.verifying_bytes());
        assert_ne!(short_a, short_b, "id must be key-derived, not static");
    }

    #[test]
    fn human_uid_is_16_uppercase_base32_chars() {
        let kp = IdentityKeypair::generate();
        let uid = human_uid(&kp.verifying_bytes());
        assert_eq!(uid.len(), 16);
        assert!(uid.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        // Deterministic across calls.
        assert_eq!(uid, human_uid(&kp.verifying_bytes()));
    }

    #[test]
    fn matches_hex_in_p0_abbreviated_envelopes() {
        // The truncated short id equals what the P0 abbreviated envelope
        // stores for `sender_id` (SHA-256(pubkey)[..16]).
        let kp = IdentityKeypair::generate();
        let pubkey = kp.verifying_bytes();
        let short = peer_short(&pubkey);
        let mut expected = [0u8; 16];
        expected.copy_from_slice(&Sha256::digest(pubkey)[..16]);
        assert_eq!(short, expected);
    }
}