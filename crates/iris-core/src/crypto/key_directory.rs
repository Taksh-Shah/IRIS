//! Key directory seam — CRYPTO-001 (v1 in-memory).
//!
//! Maps a recipient identity to its X25519 public key so the engine can
//! encrypt. v1 ships an in-memory implementation (test/dev + desktop local
//! contacts); IDENT-001 + DISCO-001 deliver real key distribution (v2 line in
//! CRYPTO_DESIGN.md). Never logs key material.

use crate::crypto::CryptoError;

/// Resolves a recipient node identity to its X25519 public key.
pub trait KeyDirectory: Send + Sync + 'static {
    /// Look up the X25519 public key for `node_id` (32-byte identity).
    fn x25519_pubkey(&self, node_id: &[u8; 32]) -> Option<[u8; 32]>;
}

/// Simple in-memory directory — test/dev + desktop local contacts.
///
/// Not persistent, not authenticated; replaced by IDENT-001 distribution.
#[derive(Debug, Default, Clone)]
pub struct MemoryKeyDirectory {
    entries: std::collections::HashMap<[u8; 32], [u8; 32]>,
}

impl MemoryKeyDirectory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `(node_id, x25519_pubkey)`.
    pub fn insert(&mut self, node_id: [u8; 32], x25519_pubkey: [u8; 32]) {
        self.entries.insert(node_id, x25519_pubkey);
    }
}

impl KeyDirectory for MemoryKeyDirectory {
    fn x25519_pubkey(&self, node_id: &[u8; 32]) -> Option<[u8; 32]> {
        self.entries.get(node_id).copied()
    }
}

/// Convenience: resolve a recipient, returning [`CryptoError::KeyUnavailable`]
/// when the key is missing.
pub fn require_key(dir: &dyn KeyDirectory, node_id: &[u8; 32]) -> Result<[u8; 32], CryptoError> {
    dir.x25519_pubkey(node_id)
        .ok_or(CryptoError::KeyUnavailable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_directory_round_trip() {
        let mut d = MemoryKeyDirectory::new();
        let id = [1u8; 32];
        let key = [2u8; 32];
        assert_eq!(d.x25519_pubkey(&id), None);
        d.insert(id, key);
        assert_eq!(d.x25519_pubkey(&id), Some(key));
    }

    #[test]
    fn require_key_reports_missing() {
        let d = MemoryKeyDirectory::new();
        assert!(matches!(
            require_key(&d, &[9u8; 32]),
            Err(CryptoError::KeyUnavailable)
        ));
    }
}
