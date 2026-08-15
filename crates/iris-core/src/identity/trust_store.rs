//! Trust store — IDENT-001 D3 (IDENT_DESIGN §10, TOFU + verified tier).
//!
//! Trust-to-usage (TOFU) table keyed by PeerId (Ed25519 identity key):
//!   - `Unverified`  first-seen via a verified advertisement (TOFU bind),
//!   - `Verified`    promoted by an out-of-band QR/pairing confirmation
//!     (`verify_peer` — the v1 API hook; UI distinguishes the tiers),
//!   - `KeyChanged`  same identity, different static key with a *stale*
//!     counter — warning state, requires re-verify,
//!   - `Revoked`     null-rotation received — blocks all paths.
//!
//! Semantics: TOFU binding never auto-promotes; revocation never auto-heals;
//! earliest-seen-wins on conflicting advertisements (Ceramic lesson, §7 step 3
//! and §9.6 step 4). Routing reputation is explicitly NOT an identity-trust
//! input (ROUTE-002/SEC-001).
//!
//! The store also implements the encryption key directory role: after
//! `adopt_advertisement` succeeds, `resolve_x25519` answers the engine's reads.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::identity::advertise::{AdvertiseError, KeyAdvertisementV1};
use crate::identity::small_order;

/// Errors from [`TrustStore::verify_peer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VerifyError {
    #[error("trust: peer unknown — a first advertisement is required before out-of-band verification")]
    UnknownPeer,
    #[error("trust: peer is revoked and cannot be verified")]
    Revoked,
    #[error("trust: confirmed static key differs from the store's certified key — re-pair with the current key")]
    KeyMismatch,
}

/// Per-peer trust level (IDENT_DESIGN §10 table).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrustLevel {
    /// Never seen (no entry).
    Unknown,
    /// TOFU first-seen (ad verified, not out-of-band).
    Unverified,
    /// Out-of-band (QR/pairing) confirmation.
    Verified,
    /// **Authority root** — provisioned out-of-band at provisioning time
    /// (EMERG-001 AC-2, EMERG_DESIGN §3 "preloaded NDMA root"). The ONLY level
    /// that may anchor an emergency authority chain. Not reachable via
    /// `adopt_advertisement` (TOFU) — a peer that simply advertises itself can
    /// never become a root (EMERG-RT-001).
    AuthorityRoot,
    /// Same identity, different static key, stale counter — warning.
    KeyChanged,
    /// Null-rotation received — blocked.
    Revoked,
}

/// Outcome of adopting an advertisement into the trust store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionOutcome {
    /// First-ever bind (TOFU).
    BoundUnverified,
    /// Repeated identical ad — idempotent no-op.
    Duplicate,
    /// Same identity, same key, newer counter — benign refresh.
    Refreshed,
    /// Monotonic rotation adopted (D4 §7 step 3).
    RotationAdopted,
    /// Key changed with a stale/equal counter — warn + require re-verify.
    KeyChangeWarn,
    /// Revoked identity — advertisement refused.
    Revoked,
    /// Rejected outright (small-order key, bad signature, bad version, expired).
    Rejected(AdvertiseError),
}

/// A single peer's trust entry.
#[derive(Debug, Clone)]
pub struct TrustEntry {
    /// PeerId = Ed25519 verifying key (subject).
    pub identity_pubkey: [u8; 32],
    /// X25519 static public key peers encrypt to (the certified key).
    pub static_x25519_pubkey: [u8; 32],
    pub level: TrustLevel,
    /// Earliest-seen unix timestamp (wins on conflict).
    pub first_seen_unix: u64,
    pub key_gen_counter: u64,
    /// `valid_until` from the winning advertisement (0 = no expiry).
    pub valid_until: u64,
}

/// TOFU trust store + encryption key directory (thread-safe).
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    inner: Arc<Mutex<HashMap<[u8; 32], TrustEntry>>>,
}

impl TrustStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Current trust level for `identity_pubkey` (`Unknown` when unseen).
    pub fn level(&self, identity_pubkey: &[u8; 32]) -> TrustLevel {
        self.inner
            .lock()
            .map(|m| m.get(identity_pubkey).map(|e| e.level).unwrap_or(TrustLevel::Unknown))
            .unwrap_or(TrustLevel::Unknown)
    }

    /// The certified X25519 key for a PeerId (the engine's encrypt target).
    /// RED-0011 is enforced at the read boundary too: a small-order key is never
    /// resolved, even from a corrupt/stale entry.
    pub fn resolve_x25519(&self, identity_pubkey: &[u8; 32]) -> Option<[u8; 32]> {
        let m = self.inner.lock().ok()?;
        let e = m.get(identity_pubkey)?;
        match e.level {
            TrustLevel::Revoked | TrustLevel::KeyChanged => None,
            _ if small_order::is_small_order(&e.static_x25519_pubkey) => None,
            _ => Some(e.static_x25519_pubkey),
        }
    }
    /// Adopt a verified advertisement (§7). Verifies signature strictness and
    /// RED-0011 before touching state; enforces earliest-seen-wins, rotation
    /// monotonicity, and revocation blocking.
    pub fn adopt_advertisement(&self, ad: &KeyAdvertisementV1, now: u64) -> AdoptionOutcome {
        if let Err(e) = ad.verify() {
            return AdoptionOutcome::Rejected(e);
        }
        // valid_until is a wall-clock instant set by the *remote*; tolerate the
        // same skew budget the rest of the stack applies to peer clocks
        // (RT-009: a fast local clock must not permanently reject healthy ads,
        // RFC 9171 §4.4.2 skew handling).
        const SKEW_BUDGET: u64 = crate::message_engine::expiry::DEFAULT_SKEW_BUDGET_SECS;
        if ad.valid_until != 0 && ad.valid_until.saturating_add(SKEW_BUDGET) < now {
            return AdoptionOutcome::Rejected(AdvertiseError::Codec("expired advertisement".into()));
        }
        let mut m = self.inner.lock().expect("trust store lock");
        let id = ad.identity_pubkey;

        let existing = m.get(&id).cloned();

        match existing {
            None => {
                // TOFU first-seen bind (never auto-promotes).
                m.insert(
                    id,
                    TrustEntry {
                        identity_pubkey: id,
                        static_x25519_pubkey: ad.static_x25519_pubkey,
                        level: TrustLevel::Unverified,
                        first_seen_unix: now,
                        key_gen_counter: ad.key_gen_counter,
                        valid_until: ad.valid_until,
                    },
                );
                AdoptionOutcome::BoundUnverified
            }
            Some(old) => {
                if old.level == TrustLevel::Revoked {
                    return AdoptionOutcome::Revoked;
                }
                let e = m.entry(id).or_insert(old);
                if e.static_x25519_pubkey == ad.static_x25519_pubkey {
                    if ad.key_gen_counter > e.key_gen_counter {
                        e.key_gen_counter = ad.key_gen_counter;
                        e.valid_until = ad.valid_until;
                        // Legitimate same-key refresh clears a prior warning
                        // (RT-001 recovery path).
                        if e.level == TrustLevel::KeyChanged {
                            e.level = TrustLevel::Unverified;
                        }
                        AdoptionOutcome::Refreshed
                    } else {
                        // Same key; equal or stale counter → idempotent no-op
                        // (earliest-seen-wins semantics).
                        AdoptionOutcome::Duplicate
                    }
                } else if ad.key_gen_counter > e.key_gen_counter {
                    // Different static key with a STRICTLY higher counter and a
                    // valid signature (verify() already ran): legitimate
                    // monotonic rotation (D4) — adopt even from a prior
                    // KeyChanged warning (recovery, RT-001).
                    e.key_gen_counter = ad.key_gen_counter;
                    e.valid_until = ad.valid_until;
                    e.static_x25519_pubkey = ad.static_x25519_pubkey;
                    if e.level == TrustLevel::KeyChanged {
                        e.level = TrustLevel::Unverified;
                    }
                    AdoptionOutcome::RotationAdopted
                } else if ad.key_gen_counter == e.key_gen_counter {
                    // Equal counter with a DIFFERENT key → genuine conflict:
                    // warn and require out-of-band re-verification (D3).
                    e.level = TrustLevel::KeyChanged;
                    AdoptionOutcome::KeyChangeWarn
                } else {
                    // STALE replay (counter < stored): the existing
                    // higher-generation entry wins. MUST NOT downgrade a
                    // healthy peer to KeyChanged (RT-001) — that would
                    // permanently disable E2EE via an attacker-relayed old
                    // advertisement.
                    AdoptionOutcome::Duplicate
                }
            }
        }
    }

    /// Out-of-band (QR/pairing) confirmation → `Verified` (D3).
    ///
    /// RT-010: the operator confirms *both* the identity AND the exact static
    /// key shown in the QR/pairing payload. If the store's certified key
    /// differs from what was confirmed out-of-band (e.g. a same-counter
    /// conflict swapped the key), verification fails loudly — the user must
    /// re-pair with the current key.
    pub fn verify_peer(
        &self,
        identity_pubkey: &[u8; 32],
        expected_static_x25519_pubkey: &[u8; 32],
    ) -> Result<(), VerifyError> {
        let mut m = self.inner.lock().expect("trust store lock");
        let Some(e) = m.get_mut(identity_pubkey) else {
            return Err(VerifyError::UnknownPeer); // require a first advertisement
        };
        if e.level == TrustLevel::Revoked {
            return Err(VerifyError::Revoked);
        }
        // RT-010: confirm the certified key matches what was seen out-of-band.
        if &e.static_x25519_pubkey != expected_static_x25519_pubkey {
            return Err(VerifyError::KeyMismatch);
        }
        e.level = TrustLevel::Verified;
        Ok(())
    }

    /// Heal a revoked identity back to `Unverified` (RT-010). A revocation is
    /// otherwise permanent — this is the operator-initiated recovery path when
    /// the revocation was a mistake (e.g. wrong node null-rotated). The entry
    /// stays non-promoted and must be re-verified out-of-band before use.
    pub fn un_revoke(&self, identity_pubkey: &[u8; 32]) {
        let mut m = self.inner.lock().expect("trust store lock");
        if let Some(e) = m.get_mut(identity_pubkey) {
            if e.level == TrustLevel::Revoked {
                e.level = TrustLevel::Unverified;
                // The placeholder key was wiped by revoke(); restore resolve
                // eligibility by leaving the (zeroed) key — but zero is
                // small-order, so resolution stays blocked until a fresh,
                // verified advertisement (or rotation) installs a real key.
            }
        }
    }

    /// Apply a revocation (null-rotation): mark `Revoked`, remove the key.
    pub fn revoke(&self, identity_pubkey: &[u8; 32]) {
        let mut m = self.inner.lock().expect("trust store lock");
        if let Some(e) = m.get_mut(identity_pubkey) {
            e.level = TrustLevel::Revoked;
            e.static_x25519_pubkey = [0u8; 32];
        }
    }

    /// Adopt a **rotation** already verified by the caller (D4 §9.6): replace
    /// the static key, bump the counter, keep the trust level. Applied only
    /// after the rotation event's signature + monotonicity have been checked.
    ///
    /// RT-013: re-checks RED-0011 small-order inside (defense-in-depth: this is
    /// a public API and must never install a low-order enc key, regardless of
    /// which caller gate validated the event).
    pub fn adopt_rotation(
        &self,
        identity_pubkey: &[u8; 32],
        new_static_x25519_pubkey: [u8; 32],
        key_gen_counter: u64,
        valid_until: u64,
    ) -> Result<bool, &'static str> {
        if small_order::is_small_order(&new_static_x25519_pubkey) {
            return Err("small-order static key (RED-0011)");
        }
        let mut m = self.inner.lock().expect("trust store lock");
        if let Some(e) = m.get_mut(identity_pubkey) {
            if e.level != TrustLevel::Revoked {
                e.static_x25519_pubkey = new_static_x25519_pubkey;
                e.key_gen_counter = key_gen_counter;
                e.valid_until = valid_until;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Snapshot of all entries (UI / diagnostics; key bytes never logged).
    pub fn entries(&self) -> Vec<TrustEntry> {
        self.inner
            .lock()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Whether `identity_pubkey` is trusted enough to accept signed traffic
    /// from. An authority root (EMERG-001) is a trusted root; TOFU-adopted or
    /// out-of-band-verified peers are trusted for normal RED-0008 chains
    /// (EMERG-RT-001 fix: a TOFU peer is trusted *for the peer tree*, but only
    /// an explicit authority root anchors an emergency chain — see
    /// [`Self::is_authority_root`]).
    pub fn is_trusted_root(&self, identity_pubkey: &[u8; 32]) -> bool {
        matches!(
            self.level(identity_pubkey),
            TrustLevel::AuthorityRoot | TrustLevel::Unverified | TrustLevel::Verified
        )
    }

    /// Provision an authority root out-of-band (EMERG_DESIGN §3 "preloaded NDMA
    /// root"). This is the ONLY way a peer becomes an emergency chain anchor;
    /// `adopt_advertisement` (TOFU/mesh-advertisement traffic) can never reach
    /// `AuthorityRoot`. Re-adopting an already-known identity promotes it
    /// (idempotent). Failed verification / small-order / revoked inputs are
    /// rejected.
    pub fn register_authority_root(
        &self,
        ad: &KeyAdvertisementV1,
        now: u64,
    ) -> Result<(), &'static str> {
        if let Err(_e) = ad.verify() {
            return Err("authority root advertisement failed verification");
        }
        if small_order::is_small_order(&ad.static_x25519_pubkey) {
            return Err("authority root uses a small-order static key (RED-0011)");
        }
        const SKEW_BUDGET: u64 = crate::message_engine::expiry::DEFAULT_SKEW_BUDGET_SECS;
        if ad.valid_until != 0 && ad.valid_until.saturating_add(SKEW_BUDGET) < now {
            return Err("authority root advertisement expired");
        }
        let mut m = self.inner.lock().expect("trust store lock");
        let id = ad.identity_pubkey;
        if m.get(&id).map(|e| e.level) == Some(TrustLevel::Revoked) {
            return Err("authority root is revoked");
        }
        m.insert(
            id,
            TrustEntry {
                identity_pubkey: id,
                static_x25519_pubkey: ad.static_x25519_pubkey,
                level: TrustLevel::AuthorityRoot,
                first_seen_unix: now,
                key_gen_counter: ad.key_gen_counter,
                valid_until: ad.valid_until,
            },
        );
        Ok(())
    }

    /// Whether `identity_pubkey` is a provisioned authority root (EMERG-001
    /// AC-2). Emergency chains must anchor on an authority root, never on a
    /// TOFU-adopted peer (EMERG-RT-001).
    pub fn is_authority_root(&self, identity_pubkey: &[u8; 32]) -> bool {
        self.level(identity_pubkey) == TrustLevel::AuthorityRoot
    }
}

/// Adapter: route encryption-key reads through the trust store, rejecting
/// small-order keys eagerly (RED-0011 at the read boundary too).
pub struct TrustKeyDirectory {
    store: TrustStore,
}

impl TrustKeyDirectory {
    pub fn new(store: TrustStore) -> Self {
        Self { store }
    }
}

impl crate::crypto::key_directory::KeyDirectory for TrustKeyDirectory {
    fn x25519_pubkey(&self, node_id: &[u8; 32]) -> Option<[u8; 32]> {
        self.store.resolve_x25519(node_id).filter(|k| !small_order::is_small_order(k))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::key_directory::KeyDirectory;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};

    fn now() -> u64 {
        crate::message_engine::expiry::unix_now()
    }

    fn ad_for(id: &IdentityKeypair, x: &X25519Keypair, counter: u64, until: u64) -> KeyAdvertisementV1 {
        KeyAdvertisementV1::build(id, x.public_bytes(), counter, until).unwrap()
    }

    #[test]
    fn tofu_bind_then_repeated_ad_is_idempotent() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unknown);
        let ad = ad_for(&id, &x, 0, 0);
        assert_eq!(store.adopt_advertisement(&ad, now()), AdoptionOutcome::BoundUnverified);
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
        assert_eq!(store.adopt_advertisement(&ad, now()), AdoptionOutcome::Duplicate);
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
    }

    #[test]
    fn resolution_feeds_key_directory() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        let ad = ad_for(&id, &x, 0, 0);
        store.adopt_advertisement(&ad, now());
        let dir = TrustKeyDirectory::new(store);
        assert_eq!(
            dir.x25519_pubkey(&id.verifying_bytes()),
            Some(x.public_bytes())
        );
    }

    #[test]
    fn verify_peer_promotes_to_verified() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        let ad = ad_for(&id, &x, 0, 0);
        store.adopt_advertisement(&ad, now());
        assert_eq!(
            store.verify_peer(&id.verifying_bytes(), &x.public_bytes()),
            Ok(())
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Verified);
    }

    #[test]
    fn verify_peer_rejects_key_mismatch() {
        // RT-010: out-of-band confirmation of the WRONG key must fail loudly.
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let other = X25519Keypair::generate();
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x, 0, 0), now());
        assert_eq!(
            store.verify_peer(&id.verifying_bytes(), &other.public_bytes()),
            Err(VerifyError::KeyMismatch)
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
    }

    #[test]
    fn key_change_warns_and_requires_reverify() {
        let id = IdentityKeypair::generate();
        let x1 = X25519Keypair::generate();
        let x2 = X25519Keypair::generate();
        assert_ne!(x1.public_bytes(), x2.public_bytes());
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x1, 0, 0), now());
        // Same counter, different key → warning (earliest-seen-wins).
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x2, 0, 0), now()),
            AdoptionOutcome::KeyChangeWarn
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::KeyChanged);
        // Resolution blocked while KeyChanged.
        assert_eq!(store.resolve_x25519(&id.verifying_bytes()), None);
    }

    #[test]
    fn rotation_adopts_when_counter_strictly_increases() {
        let id = IdentityKeypair::generate();
        let x1 = X25519Keypair::generate();
        let x2 = X25519Keypair::generate();
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x1, 0, 0), now());
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x2, 5, 0), now()),
            AdoptionOutcome::RotationAdopted
        );
        assert_eq!(
            store.resolve_x25519(&id.verifying_bytes()),
            Some(x2.public_bytes())
        );
    }

    #[test]
    fn stale_counter_earliest_seen_wins() {
        let id = IdentityKeypair::generate();
        let x1 = X25519Keypair::generate();
        let x2 = X25519Keypair::generate();
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x1, 5, 0), now());
        // RT-001: a stale (counter < stored) advertisement with a DIFFERENT key
        // is a replay — the existing higher-generation entry wins and the
        // healthy peer is NOT downgraded to KeyChanged (that would permanently
        // disable E2EE). Only an *equal*-counter conflict warns.
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x2, 3, 0), now()),
            AdoptionOutcome::Duplicate
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
        assert_eq!(
            store.resolve_x25519(&id.verifying_bytes()),
            Some(x1.public_bytes())
        );
    }

    #[test]
    fn equal_counter_different_key_warns_but_recoverable() {
        let id = IdentityKeypair::generate();
        let x1 = X25519Keypair::generate();
        let x2 = X25519Keypair::generate();
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x1, 5, 0), now());
        // Genuine same-counter conflict (both signed by the same identity):
        // warn + block resolution until re-verify.
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x2, 5, 0), now()),
            AdoptionOutcome::KeyChangeWarn
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::KeyChanged);
        assert_eq!(store.resolve_x25519(&id.verifying_bytes()), None);
        // RT-001 recovery: a STRICTLY higher counter with a valid signature is
        // the identity holder advancing the key — adopt it and clear the warn.
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x2, 6, 0), now()),
            AdoptionOutcome::RotationAdopted
        );
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
        assert_eq!(
            store.resolve_x25519(&id.verifying_bytes()),
            Some(x2.public_bytes())
        );
    }

    #[test]
    fn revocation_blocks_all_paths_and_never_auto_heals() {
        let id = IdentityKeypair::generate();
        let x = X25519Keypair::generate();
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x, 0, 0), now());
        store.revoke(&id.verifying_bytes());
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Revoked);
        assert_eq!(store.resolve_x25519(&id.verifying_bytes()), None);
        // Even a valid future advertisement is refused post-revocation.
        assert!(matches!(
            store.adopt_advertisement(&ad_for(&id, &x, 9, 0), now()),
            AdoptionOutcome::Revoked
        ));
        // verify_peer cannot heal a revocation.
        assert_eq!(
            store.verify_peer(&id.verifying_bytes(), &x.public_bytes()),
            Err(VerifyError::Revoked)
        );
        // un_revoke is the operator recovery path: back to Unverified, and a
        // stale advertisement's key must NOT be resurrected (a fresh
        // advertisement/rotation installs the real key again).
        store.un_revoke(&id.verifying_bytes());
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unverified);
        assert_eq!(store.resolve_x25519(&id.verifying_bytes()), None);
    }

    #[test]
    fn un_revoke_allows_re_adoption_after_heal() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        store.adopt_advertisement(&ad_for(&id, &x, 0, 0), now());
        store.revoke(&id.verifying_bytes());
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Revoked);
        // Revoked blocks re-adoption even of a fresh ad.
        assert!(matches!(
            store.adopt_advertisement(&ad_for(&id, &x, 9, 0), now()),
            AdoptionOutcome::Revoked
        ));
        // Operator heals; now a fresh advertisement is adoptable again. The
        // revoked entry's zeroed key + higher counter makes this a rotation
        // adoption (the certified key is reinstalled monotonic).
        store.un_revoke(&id.verifying_bytes());
        assert_eq!(
            store.adopt_advertisement(&ad_for(&id, &x, 10, 0), now()),
            AdoptionOutcome::RotationAdopted
        );
        assert_eq!(
            store.resolve_x25519(&id.verifying_bytes()),
            Some(x.public_bytes())
        );
    }

    #[test]
    fn adversarial_ad_rejected() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        let mut ad = ad_for(&id, &x, 0, 0);
        ad.sig[0] ^= 0xFF;
        assert!(matches!(
            store.adopt_advertisement(&ad, now()),
            AdoptionOutcome::Rejected(AdvertiseError::BadSignature)
        ));
        assert_eq!(store.level(&id.verifying_bytes()), TrustLevel::Unknown);
    }

    #[test]
    fn small_order_ad_rejected_red_0011() {
        let id = IdentityKeypair::generate();
        let small = small_order::SMALL_ORDER_U[0];
        let res = KeyAdvertisementV1::build(&id, small, 0, 0);
        assert!(matches!(res, Err(AdvertiseError::SmallOrderKey)));
    }

    #[test]
    fn expired_ad_rejected() {
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        // Expired beyond the DEFAULT_SKEW_BUDGET_SECS tolerance → rejected.
        let ad = ad_for(&id, &x, 0, now() - crate::message_engine::expiry::DEFAULT_SKEW_BUDGET_SECS - 10);
        assert!(matches!(
            store.adopt_advertisement(&ad, now()),
            AdoptionOutcome::Rejected(_)
        ));
    }

    #[test]
    fn expired_within_skew_budget_accepted() {
        // RT-009: a remote key that only *looks* expired because the local
        // clock runs ahead within the skew budget is still accepted.
        let (id, x) = (IdentityKeypair::generate(), X25519Keypair::generate());
        let store = TrustStore::new();
        let ad = ad_for(&id, &x, 0, now() - 60); // 60 s < 300 s budget
        assert_eq!(
            store.adopt_advertisement(&ad, now()),
            AdoptionOutcome::BoundUnverified,
            "valid_until within the skew budget must not be rejected"
        );
    }
}