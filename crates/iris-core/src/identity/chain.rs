//! `auth_cert_chain` validation — RED-0008 (IDENT_DESIGN §9, key-anchored).
//!
//! Field 18 = ordered array of certificate elements that demonstrate an
//! authority path from a **known trusted identity root** to the message signer.
//! No PKIX/X.509, no `rustls-webpki` — every cryptographic check is
//! `ed25519::verify_strict` (RFC 8032) plus the RED-0011 small-order guard on
//! each certified key (SPKI-style authorization, RFC 2693 / RFC 9804 mindset).
//!
//! Chain rules (§9):
//!   1. structural decode — any failure rejects,
//!   2. root (element[0]) must already be trusted in the `TrustStore`
//!      (TOFU/verified); ad-hoc self-signed chains with no trusted anchor are
//!      rejected — the whole point vs PKI,
//!   3. consecutive links: `verify_strict(prev.identity_pubkey,
//!      next.identity_pubkey, next.sig)` — the parent certifies the child's
//!      keyholder ("authorization"), matching the §9 rule-3 formula,
//!   4. small-order static keys anywhere in the chain → reject (RED-0011),
//!   5. monotonic counter policy: each element's counter strictly increases
//!      from the previous element's,
//!   6. the final element's identity must equal the envelope's `sender_id`,
//!   7. no expired links (`valid_until == 0 || valid_until >= now`),
//!   8. chain length capped (≤ [`MAX_CHAIN_LEN`]) to bound adversarial work,
//!   9. a revoked identity anywhere severs the chain.

use crate::identity::advertise::KeyAdvertisementV1;
use crate::identity::small_order;
use crate::identity::trust_store::{TrustLevel, TrustStore};

/// Cap on chain length (deep-chain DoS guard, §9).
pub const MAX_CHAIN_LEN: usize = 8;

/// Why a chain failed validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChainError {
    #[error("chain: empty")]
    Empty,
    #[error("chain: too long (len {0}, cap {MAX_CHAIN_LEN})")]
    TooLong(usize),
    #[error("chain: root not trusted — ad-hoc unanchored chain rejected")]
    UntrustedRoot,
    #[error("chain: link {0} decode failed: {1}")]
    Undecodable(usize, String),
    #[error("chain: link {0}: {1}")]
    Verdict(usize, String),
    #[error("chain: broken link {0} (parent does not certify child keyholder)")]
    BrokenLink(usize),
    #[error("chain: link {0} has a non-monotonic counter")]
    StaleCounter(usize),
    #[error("chain: link {0} expired")]
    Expired(usize),
    #[error("chain: final identity does not match envelope sender_id")]
    SenderMismatch,
    #[error("chain: envelope sender_id is {0} bytes (expected 16 abbreviated or 32 full)")]
    BadSenderIdLength(usize),
    #[error("chain: element {0} is revoked")]
    RevokedElement(usize),
    #[error("chain: root element's certified key differs from the trust store's entry for the root — stale certificate")]
    RootKeyMismatch,
}

/// Validate a key-anchored chain terminating in a trusted root (§9).
///
/// `sender_id` is the envelope field 3.
///
/// **`verify_chain` is stateless** — it only *reads* `trust` and never mutates
/// it. On `Ok(())` the chain is cryptographically valid and anchored, but the
/// derived peer's certified X25519 key is **not** yet resolvable for
/// encryption: the caller must explicitly call
/// [`TrustStore::adopt_advertisement`] with the final chain element to bind it
/// (TOFU for derived peers). A caller that skips that step and then encrypts to
/// the peer will get `KeyUnavailable` / a `None` from the key directory.
///
/// PRY-26: an earlier revision *did* register on success; the doc's first
/// sentence was left describing that behaviour after it was removed.
pub fn verify_chain(
    chain: &[Vec<u8>],
    trust: &TrustStore,
    sender_id: &[u8],
) -> Result<(), ChainError> {
    if chain.is_empty() {
        return Err(ChainError::Empty);
    }
    if chain.len() > MAX_CHAIN_LEN {
        return Err(ChainError::TooLong(chain.len()));
    }

    let now = crate::message_engine::expiry::unix_now();
    let ads = decode_elements(chain)?;

    // Rule 2: root must already be trusted. Pure ad-hoc self-signed chains
    // with no trusted anchor have no authority — this is the v1 anti-PKI rule.
    let root = &ads[0];
    if !trust.is_trusted_root(&root.identity_pubkey) {
        return Err(ChainError::UntrustedRoot);
    }
    if trust.level(&root.identity_pubkey) == TrustLevel::Revoked {
        return Err(ChainError::RevokedElement(0));
    }
    // Rule 2b (RT-011): the chain's root element must be *consistent* with the
    // store's certified key for that root. A stale certificate that still names
    // an old (pre-rotation) static key must not re-authorize that key — the
    // store's entry is the source of truth for what the root currently uses.
    // RED-0011 (small-order) is evaluated first so a low-order root key is
    // rejected on crypto grounds, not reported as a consistency mismatch.
    if small_order::is_small_order(&root.static_x25519_pubkey) {
        return Err(ChainError::Verdict(
            0,
            "small-order static key (RED-0011)".into(),
        ));
    }
    if let Some(entry) = trust
        .entries()
        .iter()
        .find(|e| e.identity_pubkey == root.identity_pubkey)
    {
        if entry.static_x25519_pubkey != root.static_x25519_pubkey {
            return Err(ChainError::RootKeyMismatch);
        }
    }

    for (i, ad) in ads.iter().enumerate() {
        // Rule 1b (RT-011): every element must carry the current advertisement
        // format version. `from_bytes` decodes any version but only v1 is
        // meaningful for the certifying hop (prevents a version-forged element
        // from being accepted into a chain).
        if ad.format_version != crate::identity::advertise::ADVERTISE_FORMAT_VERSION {
            return Err(ChainError::Verdict(
                i,
                format!("bad format version {}", ad.format_version),
            ));
        }
        // Rule 4: RED-0011 at every link (before any crypto touch of the key).
        if small_order::is_small_order(&ad.static_x25519_pubkey) {
            return Err(ChainError::Verdict(
                i,
                "small-order static key (RED-0011)".into(),
            ));
        }
        // Rule 7: no expired links.
        if ad.valid_until != 0 && ad.valid_until < now {
            return Err(ChainError::Expired(i));
        }
        // Rule 9: revocation severs the chain.
        if trust.level(&ad.identity_pubkey) == TrustLevel::Revoked {
            return Err(ChainError::RevokedElement(i));
        }
        // Rule 5 (PRY-17): anti-replay on each element's OWN rotation
        // generation. The old check required `ad.key_gen_counter` to strictly
        // increase between *adjacent* elements — but that field counts how many
        // times *that identity* has rotated its X25519 key; two different
        // identities' rotation counts have no ordering relationship. A
        // freshly-provisioned district authority (counter 0) certified by a
        // twice-rotated NDMA root (counter 2) failed `0 <= 2` → every
        // legitimate multi-level chain was rejected. The real property is:
        // an element must not name an OLDER generation than the trust store has
        // already recorded for that same identity. Whole-chain freshness is
        // already covered by per-element `valid_until` (rule 7) and by each
        // link's signature being over the child's current advertisement.
        if let Some(seen) = trust.recorded_counter(&ad.identity_pubkey) {
            if ad.key_gen_counter < seen {
                return Err(ChainError::StaleCounter(i));
            }
        }

        // Rule 3: parent certifies child (`verify_strict(prev.key, ad.key,
        // ad.sig)`). Root itself is trusted by the store, so its edge is the
        // first certifying hop (i == 1 against root).
        if i > 0 {
            let parent = &ads[i - 1];
            let parent_vk = ed25519_dalek::VerifyingKey::from_bytes(&parent.identity_pubkey)
                .map_err(|_| ChainError::BrokenLink(i))?;
            let signable = ad.signable_bytes().map_err(|e| ChainError::Verdict(i, e))?;
            let ok = crate::crypto::ed25519::verify_strict(&parent_vk, &signable, &ad.sig)
                .map_err(|_| ChainError::BrokenLink(i))?;
            if !ok {
                return Err(ChainError::BrokenLink(i));
            }
        }
    }

    // Rule 6: final element anchors the message signer.
    // PRY-35: `identity_pubkey` is always 32 bytes; P0 abbreviated envelopes
    // carry a 16-byte `sender_id` (= SHA-256(pubkey)[..16]). A raw slice
    // compare against a 16-byte id is *always* unequal, so every abbreviated
    // emergency broadcast that carries a chain was rejected before any real
    // check ran. Compare against the correct representation for each length.
    let final_ad = ads.last().expect("non-empty by the Empty check");
    let matches = match sender_id.len() {
        32 => final_ad.identity_pubkey.as_slice() == sender_id,
        16 => crate::identity::peer_id::peer_short(&final_ad.identity_pubkey).as_slice() == sender_id,
        other => return Err(ChainError::BadSenderIdLength(other)),
    };
    if !matches {
        return Err(ChainError::SenderMismatch);
    }

    Ok(())
}

/// Decode every element with its position for diagnostics.
fn decode_elements(chain: &[Vec<u8>]) -> Result<Vec<KeyAdvertisementV1>, ChainError> {
    chain
        .iter()
        .enumerate()
        .map(|(i, bytes)| {
            KeyAdvertisementV1::from_bytes(bytes)
                .map_err(|e| ChainError::Undecodable(i, e.to_string()))
        })
        .collect()
}

/// Build an authorization certificate: an element whose `sig` is produced by
/// `parent` over `child`'s advertisement subject block — the parent certifies
/// the child keyholder (RED-0008, SPKI-style). The child keypair is *not*
/// needed to construct the certificate (only the parent signs).
///
/// This is the v1 helper for constructing a valid chain for tests and for the
/// desktop authority path (future work); peers pack these `to_bytes()` into
/// `Envelope.auth_cert_chain`.
pub fn authorizing_cert(
    parent: &crate::crypto::keygen::IdentityKeypair,
    child_ad: &KeyAdvertisementV1,
) -> KeyAdvertisementV1 {
    let sig = crate::crypto::ed25519::sign(parent, &child_ad.signable_bytes().expect("encode"))
        .expect("sign");
    KeyAdvertisementV1 {
        format_version: child_ad.format_version,
        identity_pubkey: child_ad.identity_pubkey,
        static_x25519_pubkey: child_ad.static_x25519_pubkey,
        key_gen_counter: child_ad.key_gen_counter,
        valid_until: child_ad.valid_until,
        sig: sig.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};

    fn now() -> u64 {
        crate::message_engine::expiry::unix_now()
    }

    fn ad(id: &IdentityKeypair, x: &X25519Keypair, counter: u64, until: u64) -> KeyAdvertisementV1 {
        KeyAdvertisementV1::build(id, x.public_bytes(), counter, until).unwrap()
    }

    fn ser(a: &KeyAdvertisementV1) -> Vec<u8> {
        a.to_bytes()
    }

    fn trusted_root() -> (TrustStore, IdentityKeypair, X25519Keypair) {
        let trust = TrustStore::new();
        let root = IdentityKeypair::generate();
        let x = X25519Keypair::generate();
        trust.adopt_advertisement(&ad(&root, &x, 0, 0), now());
        (trust, root, x)
    }

    #[test]
    fn trusted_root_self_signed_single_element_passes() {
        // A single-element chain is just the trusted root advertising (or
        // sending) on its own behalf: root ∈ store, final identity == sender.
        let (trust, root, x) = trusted_root();
        let chain = vec![ser(&ad(&root, &x, 0, 0))];
        assert_eq!(
            verify_chain(&chain, &trust, &root.verifying_bytes()),
            Ok(()),
            "trusted root on its own chain must pass"
        );
    }

    #[test]
    fn authorized_child_chain_passes() {
        // Root certifies child; child sends. Chain: [root_ad, child_cert].
        let (trust, root, root_x) = trusted_root();
        let child = IdentityKeypair::generate();
        let child_x = X25519Keypair::generate();

        let root_ad = ad(&root, &root_x, 0, 0);
        let child_ad = ad(&child, &child_x, 1, 0);
        let child_cert = authorizing_cert(&root, &child_ad);

        let chain = vec![ser(&root_ad), ser(&child_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &child.verifying_bytes()),
            Ok(()),
            "root-authorized child chain must pass"
        );
    }

    #[test]
    fn untrusted_root_chain_rejected() {
        // Nothing in the store, self-signed ad → no anchor.
        let trust = TrustStore::new();
        let leaf = IdentityKeypair::generate();
        let x = X25519Keypair::generate();
        let chain = vec![ser(&ad(&leaf, &x, 0, 0))];
        assert_eq!(
            verify_chain(&chain, &trust, &leaf.verifying_bytes()),
            Err(ChainError::UntrustedRoot)
        );
    }

    #[test]
    fn broken_link_rejected() {
        let (trust, root, root_x) = trusted_root();
        let child = IdentityKeypair::generate();
        let child_x = X25519Keypair::generate();
        let evil = IdentityKeypair::generate();

        let root_ad = ad(&root, &root_x, 0, 0);
        let child_ad = ad(&child, &child_x, 1, 0);
        // Sign with the WRONG parent (evil, not root) → link must break.
        let child_cert = authorizing_cert(&evil, &child_ad);

        let chain = vec![ser(&root_ad), ser(&child_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &child.verifying_bytes()),
            Err(ChainError::BrokenLink(1))
        );
    }

    #[test]
    fn stale_counter_link_rejected() {
        // PRY-17: rule 5 now rejects an element naming an OLDER rotation
        // generation than the store has already recorded for that identity
        // (a stale-chain replay) — not a cross-identity adjacent comparison.
        let (trust, root, root_x) = trusted_root();
        let child = IdentityKeypair::generate();
        let child_x = X25519Keypair::generate();

        // Store has already seen `child` at generation 5.
        trust.adopt_advertisement(&ad(&child, &child_x, 5, 0), now());

        // A replayed chain carries a stale generation-3 element for `child`.
        let root_ad = ad(&root, &root_x, 0, 0);
        let stale_child = ad(&child, &child_x, 3, 0);
        let child_cert = authorizing_cert(&root, &stale_child);
        let chain = vec![ser(&root_ad), ser(&child_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &child.verifying_bytes()),
            Err(ChainError::StaleCounter(1))
        );
    }

    #[test]
    fn multilevel_chain_with_low_child_counter_passes() {
        // PRY-17 regression: a freshly-provisioned authority (counter 0)
        // certified by a root that has already rotated twice (counter 2) is a
        // legitimate chain. The old adjacent-counter rule rejected it because
        // `0 <= 2` → StaleCounter(1).
        let trust = TrustStore::new();
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        trust.adopt_advertisement(&ad(&root, &root_x, 2, 0), now());

        let child = IdentityKeypair::generate();
        let child_x = X25519Keypair::generate();
        let root_ad = ad(&root, &root_x, 2, 0);
        let child_ad = ad(&child, &child_x, 0, 0);
        let child_cert = authorizing_cert(&root, &child_ad);
        let chain = vec![ser(&root_ad), ser(&child_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &child.verifying_bytes()),
            Ok(())
        );
    }

    #[test]
    fn empty_chain_rejected() {
        let trust = TrustStore::new();
        assert_eq!(
            verify_chain(&[], &trust, &[0u8; 32]),
            Err(ChainError::Empty)
        );
    }

    #[test]
    fn overlong_chain_rejected() {
        let trust = TrustStore::new();
        let chain: Vec<Vec<u8>> = vec![vec![1u8; 90]; MAX_CHAIN_LEN + 1];
        assert!(matches!(
            verify_chain(&chain, &trust, &[0u8; 32]),
            Err(ChainError::TooLong(_))
        ));
    }

    #[test]
    fn undecodable_link_rejected() {
        let trust = TrustStore::new();
        let chain = vec![vec![0xAAu8; 16]];
        assert!(matches!(
            verify_chain(&chain, &trust, &[0u8; 32]),
            Err(ChainError::Undecodable(0, _))
        ));
    }

    #[test]
    fn expired_link_rejected() {
        let (trust, root, root_x) = trusted_root();
        let leaf = IdentityKeypair::generate();
        let leaf_x = X25519Keypair::generate();
        // Root certifies an already-expired leaf.
        let leaf_ad = ad(&leaf, &leaf_x, 1, now() - 10); // expired
        let leaf_cert = authorizing_cert(&root, &leaf_ad);
        let root_ad = ad(&root, &root_x, 0, 0);
        let chain = vec![ser(&root_ad), ser(&leaf_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &leaf.verifying_bytes()),
            Err(ChainError::Expired(1))
        );
    }

    #[test]
    fn root_certified_key_must_match_store() {
        // A stale certificate naming a DIFFERENT static key for the trusted
        // root must be rejected — the store's entry is the source of truth
        // (RT-011 root-vs-store consistency).
        let (trust, root, _root_x) = trusted_root();
        let child = IdentityKeypair::generate();
        // Present a root element with a brand-new (never-adopted) static key.
        let forged_root_ad = ad(&root, &X25519Keypair::generate(), 0, 0);
        let child_ad = ad(&child, &X25519Keypair::generate(), 1, 0);
        let child_cert = authorizing_cert(&root, &child_ad);
        let chain = vec![ser(&forged_root_ad), ser(&child_cert)];
        assert_eq!(
            verify_chain(&chain, &trust, &child.verifying_bytes()),
            Err(ChainError::RootKeyMismatch)
        );
    }

    #[test]
    fn wrong_format_version_link_rejected() {
        // A chain element carrying a foreign format version must be rejected
        // before any crypto touch (RT-011 version gate).
        let (trust, root, root_x) = trusted_root();
        let leaf = IdentityKeypair::generate();
        let leaf_x = X25519Keypair::generate();
        let root_ad = ad(&root, &root_x, 0, 0);
        let mut leaf_ad = ad(&leaf, &leaf_x, 1, 0);
        leaf_ad.format_version = 0xFF;
        let leaf_cert = authorizing_cert(&root, &leaf_ad);
        let chain = vec![ser(&root_ad), ser(&leaf_cert)];
        assert!(matches!(
            verify_chain(&chain, &trust, &leaf.verifying_bytes()),
            Err(ChainError::Verdict(1, _))
        ));
    }

    #[test]
    fn small_order_pubkey_in_chain_rejected() {
        let (trust, root, _) = trusted_root();
        // Craft a chain element carrying the canonical low-order u = 0.
        let small = small_order::SMALL_ORDER_U[0];
        let mut crafted = ad(&root, &X25519Keypair::generate(), 0, 0);
        crafted.static_x25519_pubkey = small;
        let chain = vec![ser(&crafted)];
        assert!(matches!(
            verify_chain(&chain, &trust, &root.verifying_bytes()),
            Err(ChainError::Verdict(0, _))
        ));
    }

    #[test]
    fn sender_mismatch_rejected() {
        let (trust, root, x) = trusted_root();
        let chain = vec![ser(&ad(&root, &x, 0, 0))];
        let wrong = IdentityKeypair::generate().verifying_bytes();
        assert_eq!(
            verify_chain(&chain, &trust, &wrong),
            Err(ChainError::SenderMismatch)
        );
    }

    /// PRY-35: an abbreviated (16-byte) `sender_id` must be compared against
    /// `peer_short(identity_pubkey)`, not raw-sliced (always unequal). Pre-fix
    /// every abbreviated emergency broadcast carrying a chain was `SenderMismatch`.
    #[test]
    fn abbreviated_sender_id_matches_via_peer_short() {
        let (trust, root, x) = trusted_root();
        let chain = vec![ser(&ad(&root, &x, 0, 0))];

        let short = crate::identity::peer_id::peer_short(&root.verifying_bytes());
        assert_eq!(verify_chain(&chain, &trust, &short), Ok(()));

        // A wrong 16-byte id still mismatches.
        assert_eq!(
            verify_chain(&chain, &trust, &[0x00u8; 16]),
            Err(ChainError::SenderMismatch)
        );
        // Any other length is an explicit error, not an incidental mismatch.
        assert_eq!(
            verify_chain(&chain, &trust, &[0u8; 20]),
            Err(ChainError::BadSenderIdLength(20))
        );
    }
}
