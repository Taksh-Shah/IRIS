//! RED-0001 desktop identity wiring — IDENT_DESIGN §11.
//!
//! Replaces the inert `DevCryptoProvider` in the production path:
//!
//! ```text
//! 1. provision_or_load_identity() -> NodeIdentity  // §5.2/5.4 (FileKeyStore)
//! 2. Arc<NodeIdentity> -> IrisCryptoProvider         // real provider
//! 3. MessageEngineConfig.node_id = PeerId (VerifyingKey bytes)
//! 4. master key -> storage sealer (MemoryStorage keeps NoSealer for now)
//! 5. KeyDirectory = TrustKeyDirectory (advertisement-fed)
//! ```
//!
//! DEC-0010: the production path no longer touches `DevCryptoProvider`, runs
//! without a persisted identity, or skips sealer wiring.
//!
//! `IRIS_NODE_ID` is display-only here: it no longer selects the node identity
//! (PeerId comes exclusively from the persisted identity; §11).

use std::sync::Arc;

use iris_core::identity::{
    FileKeyStore, IdentityManager, KeyAdvertisementV1, TrustKeyDirectory, TrustStore,
};
use iris_core::message_engine::crypto::{IrisCryptoProvider, NodeIdentity};

/// The provisioned desktop identity + trust bundle (§11).
pub struct DesktopIdentity {
    manager: IdentityManager,
    node_id: [u8; 32],
    human_uid: String,
    /// Advertisement-fed trust store (TOFU bind, rotation, revocation).
    trust: TrustStore,
}

impl DesktopIdentity {
    /// Provision on first run / load after (§5.2/§5.4, loud failure).
    pub fn provision_or_load() -> Result<Self, String> {
        let store: Arc<dyn iris_core::identity::KeyStore> =
            Arc::new(FileKeyStore::default_data_dir());
        let manager = IdentityManager::provision_or_load(store)
            .map_err(|e| format!("identity provisioning failed: {e}"))?;
        let node_id = manager.peer_id();
        let human_uid = manager.human_uid();
        if let Ok(label) = std::env::var("IRIS_NODE_ID") {
            tracing::info!("identity: IRIS_NODE_ID='{label}' is display-only; PeerId is persisted");
        }
        tracing::info!("identity: PeerId {}", hex16(&node_id));
        Ok(Self {
            manager,
            node_id,
            human_uid,
            trust: TrustStore::new(),
        })
    }

    /// The node's 32-byte PeerId (Ed25519 verifying key bytes).
    pub fn node_id(&self) -> [u8; 32] {
        self.node_id
    }

    /// Base32 human UID (display only).
    pub fn human_uid(&self) -> &str {
        &self.human_uid
    }

    /// Runtime identity bundle for the real crypto provider.
    pub fn runtime_identity(&self) -> Arc<NodeIdentity> {
        self.manager.runtime_node_identity()
    }

    /// The master storage key (future `StorageKeySealer` for PgStorage rows).
    pub fn master_key(&self) -> &[u8; 32] {
        self.manager.master_key()
    }

    /// The static X25519 public key others encrypt to.
    pub fn static_x25519_pubkey(&self) -> [u8; 32] {
        self.manager.static_x25519_pubkey()
    }

    /// Adopt a received key advertisement into the trust store (DISCO-001 feed).
    pub fn adopt_key_advertisement(
        &self,
        ad: &KeyAdvertisementV1,
        now: u64,
    ) -> iris_core::identity::AdoptionOutcome {
        self.trust.adopt_advertisement(ad, now)
    }

    /// Advertisement-fed key directory — the engine's encrypt target (§11.5).
    pub fn trust_key_directory(&self) -> TrustKeyDirectory {
        TrustKeyDirectory::new(self.trust.clone())
    }

    /// The real crypto provider over the persisted identity (§11.2).
    pub fn crypto_provider(&self) -> IrisCryptoProvider {
        IrisCryptoProvider::new(self.runtime_identity())
    }
}

/// First 16 hex chars of a node id — display helper (privacy P2).
fn hex16(id: &[u8; 32]) -> String {
    let mut s = String::with_capacity(16);
    for b in &id[..8] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Provision a real identity into a temp dir through the exact production
    /// code path (`provision_or_load` over `FileKeyStore`).
    fn temp_identity(dir: PathBuf) -> DesktopIdentity {
        let store: Arc<dyn iris_core::identity::KeyStore> =
            Arc::new(FileKeyStore::at(dir.join("iris")));
        let manager = IdentityManager::provision_or_load(store).unwrap();
        DesktopIdentity {
            node_id: manager.peer_id(),
            human_uid: manager.human_uid(),
            manager,
            trust: TrustStore::new(),
        }
    }

    #[test]
    fn provisioned_peer_id_is_persisted_and_stable() {
        let dir = tempfile::tempdir().unwrap();
        let a = temp_identity(dir.path().to_path_buf());
        let id_a = a.node_id();

        // Reload from the same dir → identical PeerId.
        let store: Arc<dyn iris_core::identity::KeyStore> =
            Arc::new(FileKeyStore::at(dir.path().join("iris")));
        let b = IdentityManager::provision_or_load(store).unwrap();
        assert_eq!(b.peer_id(), id_a, "PeerId must survive reload");
        assert_eq!(hex16(&a.node_id()), hex16(&b.peer_id()));
        assert_eq!(a.human_uid(), b.human_uid());
    }

    #[test]
    fn crypto_provider_authenticates_and_roundtrips() {
        use iris_core::message_engine::crypto::{message_kdf_info, CryptoProvider};
        let dir = tempfile::tempdir().unwrap();
        let identity = temp_identity(dir.path().to_path_buf());
        let provider = identity.crypto_provider();
        assert!(provider.authenticates(), "production path must authenticate");

        let mut env = iris_core::protocol::Envelope {
            version: iris_core::protocol::PROTOCOL_VERSION,
            message_id: iris_core::protocol::MessageId::new_v7(),
            sender_id: identity.node_id().to_vec(),
            recipient_id: identity.node_id().to_vec(),
            priority: iris_core::message::MessagePriority::P4,
            ttl_seconds: 3600,
            timestamp: iris_core::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: iris_core::protocol::ContentType::Text,
            payload_size: 0,
            payload_hash: [0u8; 32],
            payload: b"hello mesh".to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        env.payload_size = env.payload.len() as u64;
        let info = message_kdf_info(env.message_id);
        let aad = [9u8; 7];
        // Encrypt to self talks to the *static X25519* key (the key-directory
        // lookup target), which the provider's own static secret decrypts.
        let recipient = identity.static_x25519_pubkey();

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            provider.sign(&mut env).await.expect("sign");
            assert!(provider.verify(&env).await.expect("verify"));

            let (ct, hdr) = provider
                .encrypt(&info, &aad, b"payload", &recipient)
                .await
                .expect("encrypt");
            assert_ne!(ct, b"payload".to_vec(), "real provider must encrypt");
            assert_eq!(hdr.ephemeral_pubkey.len(), 32);
            let pt = provider
                .decrypt(&info, &aad, &ct, &hdr)
                .await
                .expect("decrypt");
            assert_eq!(pt, b"payload");
        });
    }

    #[test]
    fn trust_key_directory_serves_adopted_keys() {
        use iris_core::crypto::key_directory::KeyDirectory;
        use iris_core::crypto::keygen::X25519Keypair;
        use iris_core::identity::advertise::KeyAdvertisementV1;

        let dir = tempfile::tempdir().unwrap();
        let id = temp_identity(dir.path().to_path_buf());

        // Advertise a *remote* node's key so the directory resolves it.
        let remote = iris_core::message_engine::crypto::NodeIdentity::generate();
        let x = X25519Keypair::generate();
        let ad = KeyAdvertisementV1::build(&remote.identity, x.public_bytes(), 0, 0).unwrap();
        let outcome = id.adopt_key_advertisement(&ad, iris_core::message_engine::expiry::unix_now());
        use iris_core::identity::AdoptionOutcome;
        assert!(matches!(outcome, AdoptionOutcome::BoundUnverified));

        let dir = id.trust_key_directory();
        let k = dir.x25519_pubkey(&remote.identity.verifying_bytes());
        assert_eq!(k, Some(x.public_bytes()));
    }
}