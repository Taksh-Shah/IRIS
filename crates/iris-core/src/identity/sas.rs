//! Short Authentication String (SAS) — TRUST_MODEL.md out-of-band
//! verification, for the QR-scan / manual-entry pairing flow (AC: trusted
//! peers feature).
//!
//! Both peers compute the same 5-character code from their two Ed25519
//! identity public keys (order-independent — sorted before hashing, so it
//! does not matter which side calls itself "mine"). Comparing the two
//! displayed strings (visually after a QR scan, or verbally over voice) is
//! the out-of-band confirmation that promotes a peer from `Unverified` to
//! `Verified` in the [`crate::identity::TrustStore`] — [`compute_sas`] itself
//! does not touch the store.

use sha2::{Digest, Sha256};

const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"; // 32 chars, no ambiguous glyphs

/// Order-independent 5-character SAS from two Ed25519 identity public keys.
/// Probability of collision: 1/32^5 ≈ 1/33,554,432.
pub fn compute_sas(a_identity_pubkey: &[u8; 32], b_identity_pubkey: &[u8; 32]) -> String {
    let mut keys = [a_identity_pubkey.as_slice(), b_identity_pubkey.as_slice()];
    keys.sort_unstable();

    let mut hasher = Sha256::new();
    hasher.update(keys[0]);
    hasher.update(keys[1]);
    let hash = hasher.finalize();

    encode_sas(&hash[..3])
}

fn encode_sas(bytes: &[u8]) -> String {
    let value = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    let mut result = String::with_capacity(5);
    let mut v = value;
    for _ in 0..5 {
        result.push(ALPHABET[(v % 32) as usize] as char);
        v /= 32;
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_five_alphabet_chars() {
        let sas = compute_sas(&[1u8; 32], &[2u8; 32]);
        assert_eq!(sas.len(), 5);
        assert!(sas.chars().all(|c| ALPHABET.contains(&(c as u8))));
    }

    #[test]
    fn order_independent() {
        let a = [3u8; 32];
        let b = [9u8; 32];
        assert_eq!(compute_sas(&a, &b), compute_sas(&b, &a));
    }

    #[test]
    fn deterministic() {
        let a = [5u8; 32];
        let b = [7u8; 32];
        assert_eq!(compute_sas(&a, &b), compute_sas(&a, &b));
    }

    #[test]
    fn distinct_inputs_differ() {
        let a = [1u8; 32];
        assert_ne!(compute_sas(&a, &[2u8; 32]), compute_sas(&a, &[3u8; 32]));
    }

    #[test]
    fn same_key_twice_is_stable() {
        // Degenerate but must not panic (e.g. verifying your own advertisement).
        let a = [4u8; 32];
        assert_eq!(compute_sas(&a, &a).len(), 5);
    }
}
