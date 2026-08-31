//! Identity provisioning — IDENT-001 D2 (§5.1/§5.2/§5.4).
//!
//! [`NodeIdentityV1`] is the durable per-device identity: the RFC 8032 Ed25519
//! seed (signing) + static X25519 secret (encryption) + monotonic
//! key-generation counter. Stored as **seeds**, never expanded private
//! scalars, so runtime keypairs are re-derived deterministically through the
//! existing `crypto::keygen` providers.
//!
//! Provisioning flow (§5.2):
//!   1. store empty → generate `OsRng` seeds + master key, persist both,
//!   2. store present → load + decode (loud failure on corrupt/version-mismatch;
//!      regeneration would change PeerId and orphan storage — §5.4),
//!   3. derive PeerId from the Ed25519 verifying key (§6).

use std::sync::Arc;

use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};
use crate::identity::peer_id;
use crate::identity::store::{KeyStore, KeyStoreError};
use zeroize::Zeroizing;

/// v1 serialization format version (byte 0 of the blob; store header keeps a
/// separate file-format byte, IDENT_DESIGN §5.3).
pub const IDENTITY_FORMAT_VERSION: u8 = 1;

/// Wire layout (fixed, deterministic — versioned at byte 0):
///   [0]   format_version = 1
///   [1-4] reserved (0)
///   [5-12]   created_unix       u64 BE
///   [13-44]  identity_ed25519_seed   32 B
///   [45-76]  static_x25519_secret    32 B
///   [77-84]  key_gen_counter         u64 BE
const BODY_LEN: usize = 1 + 4 + 8 + 32 + 32 + 8;

/// Identity management errors.
#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("identity: {0}")]
    Store(#[from] KeyStoreError),
    #[error("identity: corrupt or unsupported blob (len {0})")]
    Malformed(usize),
    #[error("identity: unknown format version {0} — operator must re-provision")]
    BadVersion(u8),
    #[error("identity: store did not contain expected identity after provisioning")]
    Missing,
}

/// The durable per-device identity (IDENT_DESIGN §5.1).
///
/// Secret fields ride `Zeroizing` so the seeds are wiped from process memory on
/// drop. Neither seed is ever logged (OBS-001 seam excludes key bytes).
#[derive(Debug, Clone)]
pub struct NodeIdentityV1 {
    pub format_version: u8,
    pub created_unix: u64,
    /// RFC 8032 Ed25519 signing-key seed (32 B).
    pub identity_ed25519_seed: Zeroizing<[u8; 32]>,
    /// Static X25519 encryption-key secret (32 B).
    pub static_x25519_secret: Zeroizing<[u8; 32]>,
    /// Monotonic rotation counter (D4; 0 at provision).
    pub key_gen_counter: u64,
}

impl NodeIdentityV1 {
    /// Serialize to the versioned binary form (§5.1 layout).
    ///
    /// RT-008: the serialized buffer contains the seeds, so it rides
    /// [`Zeroizing`] — wiped when the caller drops it. `save_identity` /
    /// `from_bytes` consumers get a `&[u8]` view via deref coercion.
    pub fn to_bytes(&self) -> Zeroizing<Vec<u8>> {
        let mut out = Zeroizing::new(Vec::with_capacity(BODY_LEN));
        out.push(self.format_version);
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&self.created_unix.to_be_bytes());
        out.extend_from_slice(&*self.identity_ed25519_seed);
        out.extend_from_slice(&*self.static_x25519_secret);
        out.extend_from_slice(&self.key_gen_counter.to_be_bytes());
        out
    }

    /// Decode a blob, **fail loudly** on any corruption / version mismatch.
    pub fn from_bytes(blob: &[u8]) -> Result<Self, IdentityError> {
        if blob.len() != BODY_LEN {
            return Err(IdentityError::Malformed(blob.len()));
        }
        let version = blob[0];
        if version != IDENTITY_FORMAT_VERSION {
            return Err(IdentityError::BadVersion(version));
        }
        let read_u64 = |i: usize| -> u64 {
            let mut b = [0u8; 8];
            b.copy_from_slice(&blob[i..i + 8]);
            u64::from_be_bytes(b)
        };
        // PRY-23: decode secret material straight into `Zeroizing` so the
        // transient stack copy is wiped when the temporary is dropped — the
        // old closure returned a bare `[u8; 32]` that lingered unzeroed.
        let read_secret_32 = |i: usize| -> Zeroizing<[u8; 32]> {
            let mut b = Zeroizing::new([0u8; 32]);
            b.copy_from_slice(&blob[i..i + 32]);
            b
        };
        Ok(Self {
            format_version: version,
            created_unix: read_u64(5),
            identity_ed25519_seed: read_secret_32(13),
            static_x25519_secret: read_secret_32(45),
            key_gen_counter: read_u64(77),
        })
    }

    /// Re-derive the runtime Ed25519 identity keypair (deterministic).
    pub fn identity_keypair(&self) -> IdentityKeypair {
        IdentityKeypair::from_seed(*self.identity_ed25519_seed)
    }

    /// Re-derive the runtime static X25519 keypair (deterministic).
    pub fn static_x25519_keypair(&self) -> X25519Keypair {
        X25519Keypair::from_secret_bytes(*self.static_x25519_secret)
    }

    /// The PeerId = the Ed25519 verifying key bytes (self-authenticating).
    pub fn peer_id(&self) -> [u8; 32] {
        peer_id::peer_id(&self.identity_keypair().verifying)
    }
}

/// Provision-on-first-run / load-on-subsequent-run facade (§5.2/§5.4).
pub struct IdentityManager {
    identity: NodeIdentityV1,
    _master: Zeroizing<[u8; 32]>,
    _store: Arc<dyn KeyStore>,
}

impl IdentityManager {
    /// Provision a fresh identity and persist it, or load the persisted one.
    ///
    /// **Loud-failure contract**: a present-but-corrupt blob returns
    /// [`IdentityError`] — it is never silently regenerated (that would change
    /// the PeerId and orphan storage; operator must re-provision + re-pair,
    /// §5.4).
    pub fn provision_or_load(store: Arc<dyn KeyStore>) -> Result<Self, IdentityError> {
        if let Some(blob) = store.load_identity()? {
            let identity = NodeIdentityV1::from_bytes(&blob)?;
            let master = store.load_master_key()?.ok_or(IdentityError::Missing)?;
            return Ok(Self {
                identity,
                _master: master,
                _store: store,
            });
        }
        Self::provision(store)
    }

    fn provision(store: Arc<dyn KeyStore>) -> Result<Self, IdentityError> {
        use rand::RngCore;
        let now = crate::message_engine::expiry::unix_now();

        let mut seed_a = [0u8; 32];
        let mut seed_b = [0u8; 32];
        let mut master = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut seed_a);
        rand::rngs::OsRng.fill_bytes(&mut seed_b);
        rand::rngs::OsRng.fill_bytes(&mut master);

        let identity = NodeIdentityV1 {
            format_version: IDENTITY_FORMAT_VERSION,
            created_unix: now,
            identity_ed25519_seed: Zeroizing::new(seed_a),
            static_x25519_secret: Zeroizing::new(seed_b),
            key_gen_counter: 0,
        };
        store.save_identity(&identity.to_bytes())?;
        store.save_master_key(&master)?;
        tracing::info!("identity: provisioned device identity (v{IDENTITY_FORMAT_VERSION})");
        Ok(Self {
            identity,
            _master: Zeroizing::new(master),
            _store: store,
        })
    }

    /// The node's PeerId (32 B Ed25519 verifying key).
    pub fn peer_id(&self) -> [u8; 32] {
        self.identity.peer_id()
    }

    /// 16-byte truncated short id (P0 envelopes / display).
    pub fn peer_short(&self) -> [u8; 16] {
        peer_id::peer_short(&self.peer_id())
    }

    /// Uppercase base32 human UID (display only, §6).
    pub fn human_uid(&self) -> String {
        peer_id::human_uid(&self.peer_id())
    }

    /// Monotonic key-generation counter (for rotation advertisements).
    pub fn key_gen_counter(&self) -> u64 {
        self.identity.key_gen_counter
    }

    /// The static X25519 public key others encrypt to.
    pub fn static_x25519_pubkey(&self) -> [u8; 32] {
        self.identity.static_x25519_keypair().public_bytes()
    }

    /// The encrypted-at-rest master storage key (wraps identity + rows).
    pub fn master_key(&self) -> &[u8; 32] {
        &self._master
    }

    /// Build the runtime identity bundle the crypto seam consumes (CRYPTO-001
    /// `NodeIdentity`), cloning the seed material into the provider.
    pub fn runtime_node_identity(
        &self,
    ) -> std::sync::Arc<crate::message_engine::crypto::NodeIdentity> {
        std::sync::Arc::new(crate::message_engine::crypto::NodeIdentity {
            identity: self.identity.identity_keypair(),
            static_x25519: self.identity.static_x25519_keypair(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::store::FileKeyStore;

    fn temp_store() -> (Arc<dyn KeyStore>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        (Arc::new(FileKeyStore::at(dir.path())), dir)
    }

    #[test]
    fn provision_then_reload_returns_identical_peer_id() {
        let (store, _d) = temp_store();
        let m1 = IdentityManager::provision_or_load(store.clone()).unwrap();
        let peer1 = m1.peer_id();
        let short1 = m1.peer_short();
        let static1 = m1.static_x25519_pubkey();
        let ctr1 = m1.key_gen_counter();

        // Simulated restart: a brand-new manager over the same store.
        let m2 = IdentityManager::provision_or_load(store).unwrap();
        assert_eq!(m2.peer_id(), peer1, "PeerId must survive restart");
        assert_eq!(m2.peer_short(), short1);
        assert_eq!(m2.static_x25519_pubkey(), static1);
        assert_eq!(m2.key_gen_counter(), ctr1);
        assert_eq!(ctr1, 0, "counter starts at 0");
    }

    #[test]
    fn provision_is_idempotent_within_session() {
        let (store, _d) = temp_store();
        let a = IdentityManager::provision_or_load(store.clone()).unwrap();
        let b = IdentityManager::provision_or_load(store).unwrap();
        assert_eq!(a.peer_id(), b.peer_id(), "load must not regenerate");
    }

    #[test]
    fn corrupt_blob_fails_loudly_not_silent_regen() {
        let (store, dir) = temp_store();
        let path = dir.path().join("keys").join("identity.v1");
        std::fs::create_dir_all(dir.path().join("keys")).unwrap();
        std::fs::write(&path, [0x01, 0, 0, 0, 0xAA, 0xBB]).unwrap(); // too short + wrong

        let res = IdentityManager::provision_or_load(store);
        assert!(
            matches!(res, Err(IdentityError::Malformed(_))),
            "corrupt blob must fail loudly, never silently regenerate"
        );
    }

    #[test]
    fn wrong_version_blob_fails_loudly() {
        let (store, dir) = temp_store();
        let blob = NodeIdentityV1 {
            format_version: 2,
            created_unix: 1,
            identity_ed25519_seed: Zeroizing::new([1u8; 32]),
            static_x25519_secret: Zeroizing::new([2u8; 32]),
            key_gen_counter: 0,
        };
        // Write through the store so the file header + payload are both
        // present as production would persist them.
        store.save_identity(&blob.to_bytes()).unwrap();
        let _ = dir;
        assert!(matches!(
            IdentityManager::provision_or_load(store),
            Err(IdentityError::BadVersion(2))
        ));
    }

    #[test]
    fn serialization_round_trip_matches_layout() {
        let id = NodeIdentityV1 {
            format_version: IDENTITY_FORMAT_VERSION,
            created_unix: 0x0102030405060708,
            identity_ed25519_seed: Zeroizing::new([0xAA; 32]),
            static_x25519_secret: Zeroizing::new([0xBB; 32]),
            key_gen_counter: 0x1122334455667788,
        };
        let bytes = id.to_bytes();
        assert_eq!(bytes.len(), BODY_LEN);
        let parsed = NodeIdentityV1::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.created_unix, 0x0102030405060708);
        assert_eq!(*parsed.identity_ed25519_seed, [0xAA; 32]);
        assert_eq!(*parsed.static_x25519_secret, [0xBB; 32]);
        assert_eq!(parsed.key_gen_counter, 0x1122334455667788);
    }

    #[test]
    fn seeds_are_zeroized_on_drop() {
        // AC-2: seed buffers must be wiped by the Drop path. Proof reads the
        // heap allocation AFTER drop_in_place ran the zeroize path but BEFORE
        // the memory is returned to the allocator — the only sound way to
        // observe the wipe.
        use std::alloc::{self, Layout};
        let layout = Layout::new::<Zeroizing<[u8; 32]>>();
        let ptr = unsafe { alloc::alloc(layout) } as *mut Zeroizing<[u8; 32]>;
        assert!(!ptr.is_null(), "alloc failed");
        unsafe {
            ptr.write(Zeroizing::new([0x5Au8; 32]));
        }
        let view = ptr as *const u8;
        // Run the Drop path (clears the seed) — no dealloc yet.
        unsafe {
            std::ptr::drop_in_place(ptr);
        }
        let erased = unsafe { std::slice::from_raw_parts(view, 32) };
        let all_zero = erased.iter().all(|&b| b == 0);
        unsafe {
            alloc::dealloc(ptr as *mut u8, layout);
        }
        assert!(all_zero, "seed must be zeroed by the Drop path");
    }

    use tempfile;
}
