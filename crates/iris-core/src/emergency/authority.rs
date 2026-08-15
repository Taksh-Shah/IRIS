//! Emergency authority enforcement — EMERG-001 (AC-2/3, RED-0008).
//!
//! ## Design reality (verified iter 67+)
//!
//! `KeyAdvertisementV1` carries only the identity→X25519 keyholder binding —
//! **no** role/scope/severity/drill fields. The authority profile is therefore
//! **payload-level**: [`AuthorityMeta`] inside the signed `EmergencyBroadcast`,
//! bound to the sender by the envelope Ed25519 signature + the `verify_chain`
//! anchor (the chain's final element must equal the envelope `sender_id`), plus
//! an explicit `authority_peer_short == SHA-256(sender)[..16]` binding to defeat
//! profile forgery by a chain member.
//!
//! Pipeline (EMERG_DESIGN.md §3, 9 steps; any failure ⇒ DROP + audit, no reply):
//!   1. field 18 present,
//!   2. chain depth ≤ [`MAX_AUTHORITY_CHAIN_LEN`] (4: N→S→D→L) *before*
//!      `verify_chain`'s global cap of 8,
//!   3. `verify_chain(chain, trust, sender_id)` — root trusted, links valid,
//!      final identity == sender (RED-0008),
//!   4. leaf binding: `authority_peer_short` matches `SHA-256(sender)[..16]`,
//!   5. geo scope: `area_code` starts with `geo_scope` prefix (ISO 3166-2),
//!   6. functional scope: broadcast `message_type` ∈ `functional_scope`,
//!   7. severity ≤ `max_severity` cap (0 = unrestricted),
//!   8. drill discipline: `drill` flag ⇔ Drill/Test marker (AC-10),
//!   9. validity: `now ∈ [issued_at, expires_at)`.

use crate::identity::advertise::KeyAdvertisementV1;
use crate::identity::chain::verify_chain;
use crate::identity::trust_store::TrustStore;

use super::model::{AlertMessageType, AuthorityMeta, EmergencyBroadcast, Severity};

/// Max authority chain depth (National→State→District→Local).
/// Identity's global chain cap is 8; emergency enforces the stricter ≤ 4.
pub const MAX_AUTHORITY_CHAIN_LEN: usize = 4;

/// Successful verification result (audit-safe subset + profile for the app
/// layer). No payload content rides here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAuthority {
    /// Envelope sender/leaf identity (== chain final element).
    pub sender_id: [u8; 32],
    /// Verified chain depth.
    pub chain_depth: usize,
    /// Leaf short id bound to the payload (`authority_peer_short`).
    pub authority_peer_short: [u8; 16],
    /// The signed payload-level authority profile.
    pub profile: AuthorityMeta,
}

/// Authority verification failures — each maps to "DROP + audit, no reply".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthorityError {
    #[error("authority: no chain attached (field 18 empty)")]
    NoChain,
    #[error("authority: chain depth {0} exceeds cap {MAX_AUTHORITY_CHAIN_LEN}")]
    ChainTooLong(usize),
    #[error("authority: chain verification failed: {0}")]
    Chain(String),
    #[error("authority: chain root is not a provisioned authority root")]
    UntrustedAuthorityRoot,
    #[error("authority: authority_peer_short does not bind to the chain leaf")]
    LeafBindingMismatch,
    #[error("authority: area_code outside the authority geo scope")]
    AreaOutOfScope,
    #[error("authority: message_type {0} outside functional scope")]
    MessageTypeOutOfScope(u8),
    #[error("authority: severity {0} exceeds max_severity cap {1}")]
    SeverityExceedsCap(u8, u8),
    #[error("authority: drill flag inconsistent with message_type/severity")]
    DrillMismatch,
    #[error("authority: broadcast not currently valid (now {now}, issued {issued}, expires {expires})")]
    NotCurrentlyValid { now: u64, issued: u64, expires: u64 },
    #[error("authority: leaf advertisement decode failed: {0}")]
    LeafDecode(String),
}

/// The functional-scope tag for the message type used in scope checks
/// (EMERG_DESIGN.md §3 functional-scope taxonomy).
pub fn functional_tag(t: AlertMessageType) -> &'static str {
    match t {
        AlertMessageType::Alert
        | AlertMessageType::Update
        | AlertMessageType::Cancel
        | AlertMessageType::Accept => "public",
        AlertMessageType::Drill => "drill",
        AlertMessageType::RevocationList => "revocation",
        AlertMessageType::DisasterActivate | AlertMessageType::DisasterDeactivate => "disaster",
    }
}

/// Verify an authority emergency broadcast in one step.
///
/// `chain_blobs` = envelope field 18; `trust` = the local trust store;
/// `sender_id` = envelope field 3; `broadcast` = decoded payload;
/// `now_unix` = the engine's clock (injected for pure testability; the
/// underlying [`verify_chain`] reads its own clock for link expiry).
pub fn verify_authoritative(
    chain_blobs: &[Vec<u8>],
    trust: &TrustStore,
    sender_id: &[u8],
    broadcast: &EmergencyBroadcast,
    now_unix: u64,
) -> Result<VerifiedAuthority, AuthorityError> {
    // Step 1: chain present.
    if chain_blobs.is_empty() {
        return Err(AuthorityError::NoChain);
    }
    // Step 2: depth guard (before verify_chain's global cap of 8).
    if chain_blobs.len() > MAX_AUTHORITY_CHAIN_LEN {
        return Err(AuthorityError::ChainTooLong(chain_blobs.len()));
    }
    // Step 3: RED-0008 chain verification (root trusted, links valid, final
    // identity == sender_id).
    verify_chain(chain_blobs, trust, sender_id)
        .map_err(|e| AuthorityError::Chain(format!("{e}")))?;

    // Step 3b (EMERG-RT-001): the chain ROOT must be a *provisioned authority
    // root*, never merely a TOFU-adopted mesh peer. `verify_chain` accepts any
    // trusted root (Unverified/Verified/AuthorityRoot); the emergency contract
    // requires the explicit authority anchor (EMERG_DESIGN §3 "preloaded NDMA
    // root"). A peer that simply advertised itself cannot anchor an emergency
    // chain — otherwise any mesh member could self-issue a "verified" CRITICAL
    // alert (the RT-001 collapse).
    let root_bytes = chain_blobs.first().expect("non-empty after NoChain");
    let root_ad =
        KeyAdvertisementV1::from_bytes(root_bytes).map_err(|e| AuthorityError::LeafDecode(e.to_string()))?;
    if !trust.is_authority_root(&root_ad.identity_pubkey) {
        return Err(AuthorityError::UntrustedAuthorityRoot);
    }

    // Step 4: leaf binding — decode the leaf advertisement and bind its
    // identity to the payload's authority_peer_short.
    let leaf_bytes = chain_blobs.last().expect("non-empty after NoChain");
    let leaf = KeyAdvertisementV1::from_bytes(leaf_bytes)
        .map_err(|e| AuthorityError::LeafDecode(e.to_string()))?;
    let expected_short = authority_short_id(sender_id);
    let Some(peer_short) = broadcast.authority_peer_short else {
        return Err(AuthorityError::LeafBindingMismatch);
    };
    if peer_short != expected_short {
        return Err(AuthorityError::LeafBindingMismatch);
    }

    let p = &broadcast.authority;

    // Step 5: geo scope (area_code must start with the scope prefix).
    // Error carries NO payload bytes (RT-007: audit notes must stay
    // content-free; use a static discriminant instead).
    if let Some(scope) = &p.geo_scope {
        if !scope.trim().is_empty() && !broadcast.area_code.starts_with(scope.trim()) {
            return Err(AuthorityError::AreaOutOfScope);
        }
    }
    // Step 6: functional scope (message_type tag ∈ scope list).
    if let Some(scope) = &p.functional_scope {
        if !scope.trim().is_empty() {
            let tag = functional_tag(broadcast.message_type);
            let allowed = scope.split(',').map(str::trim).collect::<Vec<_>>();
            if !allowed.contains(&tag) {
                return Err(AuthorityError::MessageTypeOutOfScope(
                    broadcast.message_type.as_u8(),
                ));
            }
        }
    }
    // Step 7: severity cap (0 = unrestricted). Drill markers (Severity::Test)
    // are exempt — DEC-EMERG-0008 drills must not be blocked by the real-alert
    // cap (Test = 5 > Critical = 4 by design).
    if p.max_severity != 0
        && broadcast.severity != Severity::Test
        && broadcast.severity.as_u8() > p.max_severity
    {
        return Err(AuthorityError::SeverityExceedsCap(
            broadcast.severity.as_u8(),
            p.max_severity,
        ));
    }
    // Step 8: drill discipline (AC-10) — a Drill message/Test severity implies
    // the drill flag and vice versa.
    let drill_by_marker =
        broadcast.message_type == AlertMessageType::Drill || broadcast.severity == Severity::Test;
    if broadcast.drill != drill_by_marker {
        return Err(AuthorityError::DrillMismatch);
    }
    // Step 9: validity window.
    if now_unix < broadcast.issued_at || now_unix >= broadcast.expires_at {
        return Err(AuthorityError::NotCurrentlyValid {
            now: now_unix,
            issued: broadcast.issued_at,
            expires: broadcast.expires_at,
        });
    }

    Ok(VerifiedAuthority {
        sender_id: leaf.identity_pubkey,
        chain_depth: chain_blobs.len(),
        authority_peer_short: peer_short,
        profile: p.clone(),
    })
}

/// SHA-256(sender_id)[..16] — the pseudonymous, non-reversible short binding,
/// matching IDENT-001 `identity::peer_id::peer_short` (not blake3 — docs conformance).
pub fn authority_short_id(sender_id: &[u8]) -> [u8; 16] {
    use sha2::{Digest, Sha256};
    let mut out = [0u8; 16];
    out.copy_from_slice(&Sha256::digest(sender_id)[..16]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};
    use crate::emergency::model::Certainty;
    use crate::identity::chain::authorizing_cert;

    fn now() -> u64 {
        crate::message_engine::expiry::unix_now()
    }

    fn sample_broadcast() -> EmergencyBroadcast {
        EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [1u8; 16],
            republish_id: None,
            issued_at: now() - 60,
            expires_at: now() + 3600,
            severity: Severity::Severe,
            certainty: Certainty::Observed,
            message_type: AlertMessageType::Alert,
            area_code: "IN-GJ".into(),
            language: "en".into(),
            headline: "Flood warning".into(),
            instructions: None,
            authority: AuthorityMeta {
                issuer_name: Some("SDR".into()),
                geo_scope: Some("IN-GJ".into()),
                functional_scope: Some("public".into()),
                max_severity: 4,
            },
            authority_peer_short: None,
            drill: false,
        }
    }

    /// Build a verified chain of given depth (root registered as authority
    /// root in the store — EMERG-RT-001: only provisioned authority roots
    /// anchor emergency chains). Returns (chain_blobs, trust, leaf_sender_id).
    fn build_chain(depth: usize) -> (Vec<Vec<u8>>, TrustStore, [u8; 32]) {
        let trust = TrustStore::new();
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        let root_ad = KeyAdvertisementV1::build(&root, root_x.public_bytes(), 0, 0).unwrap();
        trust.register_authority_root(&root_ad, now()).unwrap();

        let mut blobs = vec![root_ad.to_bytes()];
        let mut sender = root.verifying_bytes();
        let mut parent = root;
        for counter in 1u64..depth as u64 {
            let child = IdentityKeypair::generate();
            let child_x = X25519Keypair::generate();
            let child_ad = KeyAdvertisementV1::build(&child, child_x.public_bytes(), counter, 0)
                .unwrap();
            let cert = authorizing_cert(&parent, &child_ad).to_bytes();
            blobs.push(cert);
            sender = child.verifying_bytes();
            parent = child;
        }
        (blobs, trust, sender)
    }

    fn with_short(b: &mut EmergencyBroadcast, sender: &[u8]) {
        b.authority_peer_short = Some(authority_short_id(sender));
    }

    #[test]
    fn verified_broadcast_passes_all_steps() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        let v = verify_authoritative(&chain, &trust, &sender, &b, now()).unwrap();
        assert_eq!(v.sender_id, sender);
        assert_eq!(v.chain_depth, 2);
        assert_eq!(v.authority_peer_short, authority_short_id(&sender));
    }

    #[test]
    fn single_trusted_root_passes() {
        let (chain, trust, sender) = build_chain(1);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        assert!(verify_authoritative(&chain, &trust, &sender, &b, now()).is_ok());
    }

    #[test]
    fn no_chain_rejected() {
        let b = sample_broadcast();
        let trust = TrustStore::new();
        assert_eq!(
            verify_authoritative(&[], &trust, &[0u8; 32], &b, now()),
            Err(AuthorityError::NoChain)
        );
    }

    #[test]
    fn chain_too_long_rejected() {
        let (chain, trust, sender) = build_chain(5);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::ChainTooLong(5))
        );
    }

    #[test]
    fn untrusted_root_chain_rejected() {
        // Chain built against a store that never saw the root → Chain error.
        let store = TrustStore::new();
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        let root_ad = KeyAdvertisementV1::build(&root, root_x.public_bytes(), 0, 0).unwrap();
        let chain = vec![root_ad.to_bytes()];
        let mut b = sample_broadcast();
        with_short(&mut b, &root.verifying_bytes());
        assert!(matches!(
            verify_authoritative(&chain, &store, &root.verifying_bytes(), &b, now()),
            Err(AuthorityError::Chain(_))
        ));
    }

    #[test]
    fn tofu_adopted_peer_cannot_be_authority_root() {
        // EMERG-RT-001 CRITICAL regression: a peer that merely advertised
        // itself (TOFU `adopt_advertisement` → Unverified) must NOT anchor an
        // emergency chain, even though verify_chain would accept it as a
        // trusted root. Without this, any mesh member could self-issue a
        // "verified" CRITICAL alert.
        let trust = TrustStore::new();
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        let root_ad = KeyAdvertisementV1::build(&root, root_x.public_bytes(), 0, 0).unwrap();
        // TOFU adoption succeeds → level Unverified, NOT AuthorityRoot.
        assert_eq!(trust.adopt_advertisement(&root_ad, now()), crate::identity::AdoptionOutcome::BoundUnverified);
        assert!(!trust.is_authority_root(&root.verifying_bytes()));

        let chain = vec![root_ad.to_bytes()];
        let mut b = sample_broadcast();
        b.authority_peer_short = Some(authority_short_id(&root.verifying_bytes()));
        assert_eq!(
            verify_authoritative(&chain, &trust, &root.verifying_bytes(), &b, now()),
            Err(AuthorityError::UntrustedAuthorityRoot)
        );
    }

    #[test]
    fn provisioned_authority_root_anchors_self_chain() {
        // A provisioned authority root may broadcast directly (single-element
        // self-signed chain) — NDMA root issuing its own alert.
        let trust = TrustStore::new();
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        let root_ad = KeyAdvertisementV1::build(&root, root_x.public_bytes(), 0, 0).unwrap();
        trust.register_authority_root(&root_ad, now()).unwrap();
        assert!(trust.is_authority_root(&root.verifying_bytes()));

        let chain = vec![root_ad.to_bytes()];
        let mut b = sample_broadcast();
        b.authority_peer_short = Some(authority_short_id(&root.verifying_bytes()));
        assert!(verify_authoritative(&chain, &trust, &root.verifying_bytes(), &b, now()).is_ok());
    }

    #[test]
    fn sender_mismatch_rejected() {
        let (chain, trust, _sender) = build_chain(2);
        let wrong = IdentityKeypair::generate().verifying_bytes();
        let mut b = sample_broadcast();
        with_short(&mut b, &wrong);
        assert!(matches!(
            verify_authoritative(&chain, &trust, &wrong, &b, now()),
            Err(AuthorityError::Chain(_))
        ));
    }

    #[test]
    fn leaf_binding_mismatch_rejected() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        b.authority_peer_short = Some([0xAA; 16]); // does not hash to sender
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::LeafBindingMismatch)
        );
    }

    #[test]
    fn geo_scope_denied() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.area_code = "MH-01".into(); // outside IN-GJ
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::AreaOutOfScope)
        );
    }

    #[test]
    fn geo_scope_prefix_allows_subregions() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.area_code = "IN-GJ-19".into();
        assert!(verify_authoritative(&chain, &trust, &sender, &b, now()).is_ok());
    }

    #[test]
    fn functional_scope_denied_for_revocation() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.message_type = AlertMessageType::RevocationList;
        b.authority.functional_scope = Some("public".into());
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::MessageTypeOutOfScope(
                AlertMessageType::RevocationList.as_u8()
            ))
        );
    }

    #[test]
    fn severity_cap_enforced() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.authority.max_severity = 2; // Moderate
        b.severity = Severity::Critical;
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::SeverityExceedsCap(
                Severity::Critical.as_u8(),
                2
            ))
        );
    }

    #[test]
    fn drill_mismatch_rejected() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.drill = true; // flag set on a real Public alert
        assert_eq!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::DrillMismatch)
        );
    }

    #[test]
    fn valid_drill_passes() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.drill = true;
        b.message_type = AlertMessageType::Drill;
        b.severity = Severity::Test;
        b.authority.functional_scope = Some("public,drill".into());
        assert!(verify_authoritative(&chain, &trust, &sender, &b, now()).is_ok());
    }

    #[test]
    fn validity_window_rejected() {
        let (chain, trust, sender) = build_chain(2);
        let mut b = sample_broadcast();
        with_short(&mut b, &sender);
        b.expires_at = now() - 5; // expired
        assert!(matches!(
            verify_authoritative(&chain, &trust, &sender, &b, now()),
            Err(AuthorityError::NotCurrentlyValid { .. })
        ));
    }

    #[test]
    fn short_id_is_stable_and_distinct() {
        let s1 = authority_short_id(&[1u8; 32]);
        let s2 = authority_short_id(&[2u8; 32]);
        assert_ne!(s1, s2);
        assert_eq!(s1, authority_short_id(&[1u8; 32]));
    }
}