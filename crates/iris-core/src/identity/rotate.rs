//! Rotation & revocation — IDENT-001 D4 (IDENT_DESIGN §9.6).
//!
//! Rotation/revocation rides the existing `ContentType::KeyRotation` (16)
//! envelope path. A [`RotationEventV1`] is signed by the **current** trusted
//! identity and carries the replacement static X25519 key plus a strictly
//! monotonic counter.
//!
//! Receiving-node policy (§9.6):
//!   1. signature verifies under the current trusted identity,
//!   2. **Rotation** (type 0): counter must be > stored counter; adopt the new
//!      static key, bump the counter (`encryption_hdr.key_id = counter` for
//!      future TX); older envelopes still decrypt until `valid_until`,
//!   3. **Revocation / null-rotation** (type 1): static key replaced with a
//!      placeholder `[0u8; 32]`, identity marked `Revoked`, key-directory
//!      entry removed, all incoming traffic from that identity rejected —
//!      including inside chains (chain.rs severs on `Revoked`),
//!   4. equal-counter conflict → **earliest-seen-wins**; the loser is dropped.
//!
//! v1 does not build a full KERI-style signed event log (equivocation
//! detection = v2, BLK-0002).

use crate::identity::small_order;
use crate::identity::trust_store::TrustStore;

/// Rotation payload format version.
pub const ROTATION_FORMAT_VERSION: u8 = 1;

/// Kind of rotation event.
pub const KIND_ROTATION: u8 = 0;
/// Null-rotation (revocation).
pub const KIND_REVOCATION: u8 = 1;

/// Signed rotation/revocation payload (IDENT_DESIGN §9.6). Wire form = CBOR
/// subject block (via `signable_bytes`) ‖ 64-byte sig.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationEventV1 {
    pub format_version: u8,
    /// 0 = rotation, 1 = revocation (null-rotation).
    pub rotation_type: u8,
    /// Replacement static X25519 public key; placeholder `[0u8; 32]` for
    /// revocation.
    pub new_static_x25519_pubkey: [u8; 32],
    /// Strictly greater than the current stored counter.
    pub key_gen_counter: u64,
    /// Unix expiry (0 = no expiry).
    pub valid_until: u64,
    /// `Ed25519(current identity)` over the fields above.
    pub sig_identity: [u8; 64],
}

/// Validation outcome of a rotation/revocation event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationOutcome {
    /// Monotonic rotation adopted; `key_id = counter` for future TX.
    RotationAdopted,
    /// Null-rotation applied: identity revoked, key removed.
    Revoked,
}

/// Why a rotation/revocation event was rejected (§9.6).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RotationError {
    #[error("rotation: unsupported format version {0}")]
    BadVersion(u8),
    #[error("rotation: unknown rotation type {0}")]
    BadType(u8),
    #[error("rotation: signature verification failed")]
    BadSignature,
    #[error("rotation: new static key is small-order (RED-0011)")]
    SmallOrderKey,
    #[error("rotation: event expired")]
    Expired,
    #[error("rotation: identity already revoked")]
    Revoked,
    #[error("rotation: stale or equal counter (earliest-seen-wins)")]
    StaleCounter,
    #[error("rotation: identity unknown — an advertisement must be adopted (TOFU) before a rotation is accepted")]
    UnknownPeer,
}

impl RotationEventV1 {
    /// Subject block the identity signs (canonical, CBOR).
    pub fn signable_bytes(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        ciborium::into_writer(
            &(
                self.format_version,
                self.rotation_type,
                self.new_static_x25519_pubkey,
                self.key_gen_counter,
                self.valid_until,
            ),
            &mut buf,
        )
        .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    /// Serialize subject block ‖ signature.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = self.signable_bytes().expect("encode");
        buf.extend_from_slice(&self.sig_identity);
        buf
    }

    /// Deserialize a rotation event.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 64 {
            return Err("too short".into());
        }
        let mut ctx = ConfinedReader::new(bytes);
        let t: (u8, u8, [u8; 32], u64, u64) =
            ciborium::from_reader(&mut ctx).map_err(|e| e.to_string())?;
        let consumed = bytes.len() - ctx.remaining();
        if consumed != bytes.len() - 64 {
            return Err(format!("trailing bytes ({consumed}/{})", bytes.len()));
        }
        let mut sig = [0u8; 64];
        sig.copy_from_slice(&bytes[consumed..]);
        Ok(Self {
            format_version: t.0,
            rotation_type: t.1,
            new_static_x25519_pubkey: t.2,
            key_gen_counter: t.3,
            valid_until: t.4,
            sig_identity: sig,
        })
    }

    /// Verify `sig_identity` under `current_identity_pubkey` (strict).
    fn verify_sig(&self, current_identity_pubkey: &[u8; 32]) -> Result<bool, &'static str> {
        let vk = ed25519_dalek::VerifyingKey::from_bytes(current_identity_pubkey)
            .map_err(|_| "bad identity pubkey")?;
        let signable = self.signable_bytes().map_err(|_| "codec")?;
        match crate::crypto::ed25519::verify_strict(&vk, &signable, &self.sig_identity) {
            Ok(true) => Ok(true),
            Ok(false) => Err("bad signature"),
            Err(_) => Err("bad signature"),
        }
    }
}

/// Reader that tracks remaining bytes (for strict trailing-byte rejection).
struct ConfinedReader<'a> {
    data: &'a [u8],
}

impl<'a> ConfinedReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data }
    }
    fn remaining(&self) -> usize {
        self.data.len()
    }
}

impl std::io::Read for ConfinedReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = buf.len().min(self.data.len());
        buf[..n].copy_from_slice(&self.data[..n]);
        self.data = &self.data[n..];
        Ok(n)
    }
}

/// Apply a rotation/revocation event (§9.6 receiving-node policy).
///
/// `current_identity_pubkey` = the currently trusted identity (PeerId). Uses
/// the trust store's stored counter for monotonicity and earliest-seen-wins.
pub fn apply(
    trust: &TrustStore,
    event: &RotationEventV1,
    current_identity_pubkey: &[u8; 32],
) -> Result<RotationOutcome, RotationError> {
    if event.format_version != ROTATION_FORMAT_VERSION {
        return Err(RotationError::BadVersion(event.format_version));
    }
    if event.rotation_type != KIND_ROTATION && event.rotation_type != KIND_REVOCATION {
        return Err(RotationError::BadType(event.rotation_type));
    }
    if event.rotation_type == KIND_ROTATION
        && small_order::is_small_order(&event.new_static_x25519_pubkey)
    {
        return Err(RotationError::SmallOrderKey);
    }
    if !event.verify_sig(current_identity_pubkey).unwrap_or(false) {
        return Err(RotationError::BadSignature);
    }
    // RT-009: `valid_until` is a wall-clock instant set by the *remote*;
    // tolerate the same skew budget as `TrustStore::adopt_advertisement` so a
    // fast local clock does not reject healthy rotation events (RFC 9171 §4.4.2).
    const SKEW_BUDGET: u64 = crate::message_engine::expiry::DEFAULT_SKEW_BUDGET_SECS;
    if event.valid_until != 0
        && event.valid_until.saturating_add(SKEW_BUDGET) < crate::message_engine::expiry::unix_now()
    {
        return Err(RotationError::Expired);
    }
    // A revoked identity cannot rotate (nothing trusted to rotate).
    if trust.level(current_identity_pubkey) == crate::identity::trust_store::TrustLevel::Revoked {
        return Err(RotationError::Revoked);
    }

    let stored_counter = {
        let entries = trust.entries();
        let entry = entries
            .iter()
            .find(|e| e.identity_pubkey == *current_identity_pubkey);
        // RT-012: an identity with no prior advertisement (no TOFU bound entry)
        // must NOT silently accept a rotation. Earliest-seen-wins requires a
        // baseline to enforce monotonicity against.
        let Some(entry) = entry else {
            return Err(RotationError::UnknownPeer);
        };
        entry.key_gen_counter
    };

    // Earliest-seen-wins: a non-strictly-greater counter is a replay/stale
    // event → dropped.
    if event.key_gen_counter <= stored_counter {
        return Err(RotationError::StaleCounter);
    }

    match event.rotation_type {
        KIND_ROTATION => {
            // RT-013: adopt_rotation re-validates small-order; a defense
            // failure here is a bug (RED-0011), not a runtime-path error.
            trust
                .adopt_rotation(
                    current_identity_pubkey,
                    event.new_static_x25519_pubkey,
                    event.key_gen_counter,
                    event.valid_until,
                )
                .expect("rotation small-order re-check must pass (RED-0011)");
            // Entry now carries the new key; `encryption_hdr.key_id =
            // counter` selects it for future TX.
            Ok(RotationOutcome::RotationAdopted)
        }
        KIND_REVOCATION => {
            trust.revoke(current_identity_pubkey);
            Ok(RotationOutcome::Revoked)
        }
        _ => Err(RotationError::BadType(event.rotation_type)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};
    use crate::identity::advertise::KeyAdvertisementV1;
    use crate::identity::trust_store::TrustStore;

    fn now() -> u64 {
        crate::message_engine::expiry::unix_now()
    }

    fn build_rotation(
        id: &IdentityKeypair,
        rtype: u8,
        new_key: [u8; 32],
        counter: u64,
        until: u64,
    ) -> RotationEventV1 {
        let mut ev = RotationEventV1 {
            format_version: ROTATION_FORMAT_VERSION,
            rotation_type: rtype,
            new_static_x25519_pubkey: new_key,
            key_gen_counter: counter,
            valid_until: until,
            sig_identity: [0u8; 64],
        };
        ev.sig_identity = crate::crypto::ed25519::sign(id, &ev.signable_bytes().unwrap()).unwrap();
        ev
    }

    fn trusted(id: &IdentityKeypair, x: &X25519Keypair) -> TrustStore {
        let trust = TrustStore::new();
        trust.adopt_advertisement(
            &KeyAdvertisementV1::build(id, x.public_bytes(), 0, 0).unwrap(),
            now(),
        );
        trust
    }

    #[test]
    fn rotation_adopts_with_monotonic_counter() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);
        let new_x = X25519Keypair::generate();

        let ev = build_rotation(&id, KIND_ROTATION, new_x.public_bytes(), 1, 0);
        assert_eq!(
            apply(&trust, &ev, &id.verifying_bytes()),
            Ok(RotationOutcome::RotationAdopted)
        );
        assert_eq!(
            trust.resolve_x25519(&id.verifying_bytes()),
            Some(new_x.public_bytes())
        );
    }

    #[test]
    fn stale_counter_rejected_earliest_seen_wins() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);
        let new_x = X25519Keypair::generate();

        let stale = build_rotation(&id, KIND_ROTATION, new_x.public_bytes(), 0, 0); // == stored
        assert_eq!(
            apply(&trust, &stale, &id.verifying_bytes()),
            Err(RotationError::StaleCounter)
        );
        assert_eq!(
            trust.resolve_x25519(&id.verifying_bytes()),
            Some(old_x.public_bytes())
        );
    }

    #[test]
    fn equal_counter_conflict_earliest_seen_wins() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);
        // First rotation to key A at counter 1.
        let a = X25519Keypair::generate();
        let ev_a = build_rotation(&id, KIND_ROTATION, a.public_bytes(), 1, 0);
        assert_eq!(
            apply(&trust, &ev_a, &id.verifying_bytes()),
            Ok(RotationOutcome::RotationAdopted)
        );
        // Conflicting rotation to key B at the same counter → dropped.
        let b = X25519Keypair::generate();
        let ev_b = build_rotation(&id, KIND_ROTATION, b.public_bytes(), 1, 0);
        assert_eq!(
            apply(&trust, &ev_b, &id.verifying_bytes()),
            Err(RotationError::StaleCounter)
        );
        assert_eq!(
            trust.resolve_x25519(&id.verifying_bytes()),
            Some(a.public_bytes())
        );
    }

    #[test]
    fn null_rotation_revokes_and_blocks_paths() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);

        let ev = build_rotation(&id, KIND_REVOCATION, [0u8; 32], 1, 0);
        assert_eq!(
            apply(&trust, &ev, &id.verifying_bytes()),
            Ok(RotationOutcome::Revoked)
        );
        assert_eq!(
            trust.level(&id.verifying_bytes()),
            crate::identity::trust_store::TrustLevel::Revoked
        );
        // After revocation: no key resolves, no further rotation accepted.
        assert_eq!(trust.resolve_x25519(&id.verifying_bytes()), None);
        let ev2 = build_rotation(
            &id,
            KIND_ROTATION,
            X25519Keypair::generate().public_bytes(),
            2,
            0,
        );
        assert_eq!(
            apply(&trust, &ev2, &id.verifying_bytes()),
            Err(RotationError::Revoked)
        );
    }

    #[test]
    fn forged_event_rejected() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);
        let evil = IdentityKeypair::generate();

        // Signed by evil, not the trusted identity.
        let ev = build_rotation(
            &evil,
            KIND_ROTATION,
            X25519Keypair::generate().public_bytes(),
            1,
            0,
        );
        assert_eq!(
            apply(&trust, &ev, &id.verifying_bytes()),
            Err(RotationError::BadSignature)
        );
        assert_eq!(
            trust.resolve_x25519(&id.verifying_bytes()),
            Some(old_x.public_bytes())
        );
    }

    #[test]
    fn small_order_new_key_rejected() {
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);

        let small = small_order::SMALL_ORDER_U[0];
        let ev = build_rotation(&id, KIND_ROTATION, small, 1, 0);
        assert_eq!(
            apply(&trust, &ev, &id.verifying_bytes()),
            Err(RotationError::SmallOrderKey)
        );
    }

    #[test]
    fn unknown_peer_rotation_rejected() {
        // RT-012: an identity with no TOFU-bound advertisement must not
        // silently accept a rotation (earliest-seen-wins needs a baseline).
        let trust = TrustStore::new();
        let id = IdentityKeypair::generate();
        let ev = build_rotation(
            &id,
            KIND_ROTATION,
            X25519Keypair::generate().public_bytes(),
            1,
            0,
        );
        assert_eq!(
            apply(&trust, &ev, &id.verifying_bytes()),
            Err(RotationError::UnknownPeer)
        );
    }

    #[test]
    fn adopt_rotation_defensive_small_order_recheck() {
        // RT-013: adopt_rotation itself must refuse a low-order key even if
        // every caller gate were bypassed.
        use crate::crypto::key_directory::KeyDirectory;
        let id = IdentityKeypair::generate();
        let old_x = X25519Keypair::generate();
        let trust = trusted(&id, &old_x);
        let small = small_order::SMALL_ORDER_U[0];
        assert_eq!(
            trust.adopt_rotation(&id.verifying_bytes(), small, 5, 0),
            Err("small-order static key (RED-0011)")
        );
        // The certified key is untouched.
        assert_eq!(
            crate::identity::trust_store::TrustKeyDirectory::new(trust)
                .x25519_pubkey(&id.verifying_bytes()),
            Some(old_x.public_bytes())
        );
    }

    #[test]
    fn serialization_round_trip() {
        let id = IdentityKeypair::generate();
        let ev = build_rotation(
            &id,
            KIND_ROTATION,
            X25519Keypair::generate().public_bytes(),
            7,
            0,
        );
        let parsed = RotationEventV1::from_bytes(&ev.to_bytes()).unwrap();
        assert_eq!(parsed, ev);
    }
}
