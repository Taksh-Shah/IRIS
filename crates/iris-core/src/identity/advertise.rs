//! Key advertisement — RED-0005 (IDENT_DESIGN §7, SPKI-style / RFC 9804).
//!
//! A [`KeyAdvertisementV1`] is the "keyholder certifies the key" primitive:
//! the identity Ed25519 key **certifies** the static X25519 encryption key by
//! signing the SPKI-style subject block. Peers verify with `verify_strict`
//! (RFC 8032), reject small-order certified keys (RED-0011), and on success
//! bind `identity_pubkey → static_x25519_pubkey` in the key directory.
//!
//! Wire: CBOR (`ciborium`) over the fixed field set; signature is appended in
//! its own bytes. Transport = carried as an application blob inside a
//! `ContentType::KeyRotation` envelope and/or piggybacked in
//! `auth_cert_chain` — same bytes, wire unchanged.

use crate::crypto::ed25519 as ed;
use crate::crypto::keygen::IdentityKeypair;
use crate::identity::small_order;
use serde_bytes::ByteArray;

/// Format version for the advertisement wire form.
pub const ADVERTISE_FORMAT_VERSION: u8 = 1;

/// A signed key advertisement (SPKI-style / RFC 9804). Wire form is the CBOR
/// subject block (via `signable_bytes`) concatenated with the 64-byte sig;
/// see [`KeyAdvertisementV1::to_bytes`] / [`from_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct KeyAdvertisementV1 {
    pub format_version: u8,
    /// Ed25519 verifying key — subject / keyholder (PeerId).
    pub identity_pubkey: [u8; 32],
    /// Certified X25519 static public key — what peers encrypt to.
    pub static_x25519_pubkey: [u8; 32],
    /// Monotonic rotation counter (D4).
    pub key_gen_counter: u64,
    /// Unix expiry (0 = no expiry).
    pub valid_until: u64,
    /// `Ed25519(identity)` over the fields above.
    #[serde(with = "serde_bytes")]
    pub sig: ByteArray<64>,
}

/// Errors during build/verify of a key advertisement.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdvertiseError {
    #[error("advertisement: bad signature")]
    BadSignature,
    #[error("advertisement: signer ≠ subject (cross-identity certificate rejected)")]
    SignerMismatch,
    #[error("advertisement: certified X25519 key is a small-order key (RED-0011)")]
    SmallOrderKey,
    #[error("advertisement: unsupported format version {0}")]
    BadVersion(u8),
    #[error("advertisement: wire decode {0}")]
    Codec(String),
}

impl KeyAdvertisementV1 {
    /// Build an advertisement: identity signs its static X25519 key.
    ///
    /// The signing scope is the *exact* canonical bytes of
    /// `(format_version ‖ identity_pubkey ‖ static_x25519_pubkey ‖
    /// key_gen_counter ‖ valid_until)` in CBOR, so verification is
    /// byte-for-byte deterministic.
    pub fn build(
        identity: &IdentityKeypair,
        static_x25519_pubkey: [u8; 32],
        key_gen_counter: u64,
        valid_until: u64,
    ) -> Result<Self, AdvertiseError> {
        if small_order::is_small_order(&static_x25519_pubkey) {
            return Err(AdvertiseError::SmallOrderKey);
        }
        let mut ad = Self {
            format_version: ADVERTISE_FORMAT_VERSION,
            identity_pubkey: identity.verifying_bytes(),
            static_x25519_pubkey,
            key_gen_counter,
            valid_until,
            sig: ByteArray::from([0u8; 64]),
        };
        let signable = ad.signable_bytes().map_err(AdvertiseError::Codec)?;
        ad.sig = ed::sign(identity, &signable)
            .map_err(|_| AdvertiseError::BadSignature)?
            .into();
        Ok(ad)
    }

    /// The exact bytes the signature covers (SPKI subject block).
    pub fn signable_bytes(&self) -> Result<Vec<u8>, String> {
        let mut buf = Vec::new();
        ciborium::into_writer(
            &(
                self.format_version,
                self.identity_pubkey,
                self.static_x25519_pubkey,
                self.key_gen_counter,
                self.valid_until,
            ),
            &mut buf,
        )
        .map_err(|e| e.to_string())?;
        Ok(buf)
    }

    /// Serialize the full advertisement (subject block ‖ 64-byte sig).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = self.signable_bytes().expect("encode");
        buf.extend_from_slice(self.sig.as_ref());
        buf
    }

    /// Deserialize a full advertisement (subject block ‖ 64-byte sig).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AdvertiseError> {
        if bytes.len() < 64 {
            return Err(AdvertiseError::Codec("too short".into()));
        }
        let (obj, consumed) = decode_subject_with_len(bytes).map_err(AdvertiseError::Codec)?;
        // Trailing 64 bytes are the signature; everything else must have been
        // consumed by the CBOR subject block (no trailing garbage accepted).
        if consumed != bytes.len() - 64 {
            return Err(AdvertiseError::Codec(format!(
                "trailing bytes after subject (consumed {consumed}, total {})",
                bytes.len()
            )));
        }
        let mut sig = ByteArray::from([0u8; 64]);
        sig.as_mut().copy_from_slice(&bytes[consumed..]);
        Ok(Self {
            format_version: obj.0,
            identity_pubkey: obj.1,
            static_x25519_pubkey: obj.2,
            key_gen_counter: obj.3,
            valid_until: obj.4,
            sig,
        })
    }

    /// Verify `sig` under `identity_pubkey` over the canonical subject block,
    /// plus RED-0011 small-order rejection. `Ok(true)` only when all pass.
    pub fn verify(&self) -> Result<bool, AdvertiseError> {
        if self.format_version != ADVERTISE_FORMAT_VERSION {
            return Err(AdvertiseError::BadVersion(self.format_version));
        }
        if small_order::is_small_order(&self.static_x25519_pubkey) {
            return Err(AdvertiseError::SmallOrderKey);
        }
        let vk = ed25519_dalek::VerifyingKey::from_bytes(&self.identity_pubkey)
            .map_err(|_| AdvertiseError::BadSignature)?;
        let signable = self.signable_bytes().map_err(AdvertiseError::Codec)?;
        let ok = ed::verify_strict(&vk, &signable, &self.sig)
            .map_err(|_| AdvertiseError::BadSignature)?;
        if ok {
            Ok(true)
        } else {
            Err(AdvertiseError::BadSignature)
        }
    }
}

/// The canonical (un-signed) subject block of an advertisement.
pub type SubjectBlock = (u8, [u8; 32], [u8; 32], u64, u64);

/// Decode the subject block, reporting how many bytes it consumed.
#[allow(clippy::type_complexity)]
fn decode_subject_with_len(bytes: &[u8]) -> Result<(SubjectBlock, usize), String> {
    struct Counting<'a>(&'a [u8]);
    impl<'a> std::io::Read for Counting<'a> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = buf.len().min(self.0.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let mut reader = Counting(bytes);
    let t: SubjectBlock = ciborium::from_reader(&mut reader).map_err(|e| e.to_string())?;
    let consumed = bytes.len() - reader.0.len();
    Ok((t, consumed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};

    fn pair() -> (IdentityKeypair, X25519Keypair) {
        (IdentityKeypair::generate(), X25519Keypair::generate())
    }

    #[test]
    fn valid_ad_builds_and_verifies() {
        let (id, x) = pair();
        let ad = KeyAdvertisementV1::build(&id, x.public_bytes(), 0, 0).unwrap();
        assert_eq!(ad.identity_pubkey, id.verifying_bytes());
        assert!(ad.verify().unwrap());
    }

    #[test]
    fn tampered_static_key_fails_verify() {
        let (id, x) = pair();
        let mut ad = KeyAdvertisementV1::build(&id, x.public_bytes(), 0, 0).unwrap();
        ad.static_x25519_pubkey[0] ^= 0x01;
        assert!(matches!(ad.verify(), Err(AdvertiseError::BadSignature)));
    }

    #[test]
    fn tampered_counter_fails_verify() {
        let (id, x) = pair();
        let mut ad = KeyAdvertisementV1::build(&id, x.public_bytes(), 0, 0).unwrap();
        ad.key_gen_counter = 1;
        assert!(matches!(ad.verify(), Err(AdvertiseError::BadSignature)));
    }

    #[test]
    fn serialization_round_trip() {
        let (id, x) = pair();
        let ad = KeyAdvertisementV1::build(&id, x.public_bytes(), 3, 0).unwrap();
        let bytes = ad.to_bytes();
        let parsed = KeyAdvertisementV1::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, ad);
    }

    #[test]
    fn small_order_certified_key_rejected_at_build_and_verify() {
        let (id, _x) = pair();
        let small = [0u8; 32]; // u = 0 is the canonical low-order key
        assert!(matches!(
            KeyAdvertisementV1::build(&id, small, 0, 0),
            Err(AdvertiseError::SmallOrderKey)
        ));
    }

    #[test]
    fn forged_ad_wrong_signer_rejected() {
        // Cross-identity: one identity cannot forge another's advertisement.
        let (id_a, x_a) = pair();
        let (id_b, _x_b) = pair();
        let mut ad = KeyAdvertisementV1::build(&id_a, x_a.public_bytes(), 0, 0).unwrap();
        ad.identity_pubkey = id_b.verifying_bytes(); // spoof the keyholder
        assert!(matches!(ad.verify(), Err(AdvertiseError::BadSignature)));
    }
}
