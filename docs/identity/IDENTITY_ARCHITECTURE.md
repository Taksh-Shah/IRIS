# Identity Architecture

## Overview

IRIS uses a self-sovereign identity model: no central authority creates or controls
identities. Every user generates their own cryptographic identity on their device.
The public key is the address; the private key is the unforgeable proof of identity.

This design works offline, survives server outages, and cannot be revoked by any
third party — including IRIS itself. In a disaster where central systems may be down
for weeks, this property is essential.

## Core Identity Primitive

An IRIS identity is an Ed25519 key pair:

```rust
pub struct IrisIdentity {
    /// Public key — this IS the identity. Shared with everyone.
    pub public_key: ed25519_dalek::PublicKey,

    /// Private key — never leaves this device.
    pub private_key: ed25519_dalek::SecretKey,

    /// NodeId: compact addressing form of the public key
    pub node_id: NodeId,

    /// Human-readable label chosen by the user
    pub display_name: Option<String>,

    /// When this identity was created
    pub created_at: SystemTime,

    /// Identity level
    pub level: IdentityLevel,
}

/// NodeId = first 32 bytes of SHA-256(public_key_bytes)
/// 32 bytes = 256-bit security, globally unique in practice
pub struct NodeId([u8; 32]);

impl NodeId {
    pub fn from_public_key(pk: &ed25519_dalek::PublicKey) -> Self {
        let hash = sha2::Sha256::digest(pk.as_bytes());
        NodeId(hash.into())
    }

    /// Human-readable short form: first 8 hex chars
    /// e.g., "A3B7F2C9" — used in UI
    pub fn short_form(&self) -> String {
        hex::encode(&self.0[..4]).to_uppercase()
    }
}
```

## Identity Levels

IRIS supports five identity levels with increasing assurance:

### Level 1: Anonymous

Ephemeral key pair, new per session. Use cases:
- Temporary emergency use without prior setup
- Privacy-sensitive browsing of public channels
- First 60 seconds of app use before identity creation

```rust
pub struct AnonymousIdentity {
    ephemeral_key: IrisIdentity,
    session_start: SystemTime,
    // Automatically discarded when session ends
}
```

### Level 2: Pseudonymous

Stable key pair, not linked to real name. Persistent across sessions. The user
has a consistent NodeId that contacts recognize, but no name is attached.

This is the minimum level for P4–P7 message exchange.

### Level 3: Named

Stable key pair + user-chosen display name. Name is not verified by IRIS — it is
self-asserted. Two users could claim the same name. The NodeId is still the ground
truth; the name is a hint.

```rust
pub struct NamedIdentity {
    pub key_pair: IrisIdentity,
    pub display_name: String,
    pub name_set_at: SystemTime,
    // Name is included in signed capability bundle
    // Signature proves name was set by key owner, not that it's the real name
}
```

### Level 4: Verified

Named identity + out-of-band verification. Another trusted user has confirmed
that this NodeId belongs to this real person. Verification methods:
- **QR code scan**: physically scan each other's IRIS QR codes → mutual verification
- **Short authentication string**: 5-character code computed from both public keys,
  compared verbally ("does your screen show MANGO-7?")

```rust
pub struct VerifiedIdentity {
    pub key_pair: IrisIdentity,
    pub display_name: String,
    pub verifications: Vec<VerificationRecord>,
}

pub struct VerificationRecord {
    pub verified_by: NodeId,          // who verified this identity
    pub method: VerificationMethod,
    pub verified_at: SystemTime,
    pub verifier_signature: Signature, // verifier signs: "I verified NodeId X is DisplayName Y"
}
```

### Level 5: Authority

Certificate chain from a trusted organization. Used by:
- NDRF (National Disaster Response Force)
- State disaster management authorities
- Red Cross / Red Crescent
- Police, fire, medical organizations

```rust
pub struct AuthorityIdentity {
    pub key_pair: IrisIdentity,
    pub display_name: String,
    pub certificate: AuthorityCertificate,
}

pub struct AuthorityCertificate {
    pub subject_node_id: NodeId,
    pub subject_name: String,
    pub organization: String,        // "NDRF", "Red Cross India"
    pub role: String,                // "Team Leader", "Rescue Coordinator"
    pub issued_by: NodeId,           // issuing authority's NodeId
    pub issuer_certificate: Option<Box<AuthorityCertificate>>, // chain
    pub valid_from: SystemTime,
    pub valid_until: SystemTime,
    pub issuer_signature: Signature,
}
```

## Identity Bootstrap

### First Run

```rust
pub async fn initialize_identity(secure_storage: &SecureStorage) -> Result<IrisIdentity> {
    // Check if identity already exists
    if let Some(existing) = secure_storage.load_identity().await? {
        return Ok(existing);
    }

    // Generate new Ed25519 key pair using OS secure random
    let mut csprng = OsRng;
    let signing_key = ed25519_dalek::SigningKey::generate(&mut csprng);

    let identity = IrisIdentity {
        public_key: signing_key.verifying_key(),
        private_key: signing_key.to_bytes().into(),
        node_id: NodeId::from_public_key(&signing_key.verifying_key()),
        display_name: None,
        created_at: SystemTime::now(),
        level: IdentityLevel::Pseudonymous,
    };

    // Store securely (platform-specific — see KEY_MANAGEMENT.md)
    secure_storage.save_identity(&identity).await?;

    Ok(identity)
}
```

### Platform-Specific Storage

| Platform | Storage Location | Hardware Backing |
|----------|----------------|-----------------|
| Android (API 23+) | Android Keystore System | Hardware TEE on supported devices |
| iOS (A7+) | Secure Enclave | Hardware enclave |
| iOS (older) | iOS Keychain with kSecAttrAccessibleAfterFirstUnlock | Software |
| Linux | PKCS#8 file, AES-256-GCM encrypted | None (user password as KEK) |
| macOS | macOS Keychain | Secure Enclave on T2/M-series Macs |
| Windows | DPAPI-encrypted file | TPM on supported hardware |

## Identity Export

Users can back up their identity to restore on a new device:

```rust
pub struct IdentityBackup {
    pub version: u8,
    pub node_id: NodeId,           // for user to verify they have the right backup
    pub display_name: Option<String>,
    pub created_at: SystemTime,
    /// Private key encrypted with Argon2id-derived key from user password
    pub encrypted_private_key: EncryptedBlob,
    pub salt: [u8; 32],
    pub backup_at: SystemTime,
}

pub struct EncryptedBlob {
    pub nonce: [u8; 12],           // AES-256-GCM nonce
    pub ciphertext: Vec<u8>,       // encrypted private key bytes
    pub tag: [u8; 16],             // GCM authentication tag
}

pub fn export_identity(identity: &IrisIdentity, password: &str) -> Result<IdentityBackup> {
    let salt: [u8; 32] = OsRng.gen();

    // Argon2id key derivation (memory-hard, prevents brute force)
    let kek = argon2id_derive(password, &salt, Argon2Params {
        memory_kib: 65536,   // 64 MB
        iterations: 3,
        parallelism: 4,
        output_len: 32,
    })?;

    let encrypted = aes_256_gcm_encrypt(&identity.private_key_bytes(), &kek)?;

    Ok(IdentityBackup {
        version: 1,
        node_id: identity.node_id.clone(),
        display_name: identity.display_name.clone(),
        created_at: identity.created_at,
        encrypted_private_key: encrypted,
        salt,
        backup_at: SystemTime::now(),
    })
}
```

## Multiple Device Support

One user may have multiple IRIS devices (phone + tablet + laptop). Options:

**Option A: Independent identities** (default). Each device has its own NodeId.
Contacts must add each device separately. Simple, no key sharing.

**Option B: Linked identities** (opt-in). User explicitly links devices:
- Device A generates "link token" (signed by A's private key)
- User physically transfers token to Device B (QR code or NFC)
- Device B verifies and registers as "linked to Device A's identity"
- Other users see both devices as the same person

Linking is user-controlled and optional. IRIS does not automatically discover
or link a user's own devices.

## Emergency Identities for First Responders

Pre-generated authority identities for disaster response organizations:

```rust
pub struct PreGeneratedEmergencyIdentity {
    pub organization: String,
    pub role: String,
    pub node_id: NodeId,
    pub certificate: AuthorityCertificate,
    pub private_key_encrypted: EncryptedBlob,  // with organization-issued PIN
}
```

NDRF or state DMA issues these to responders before deployment. The responder's
phone imports the pre-generated identity, which immediately carries Authority-level
trust in the IRIS network. Other nodes automatically recognize NDRF-certified
identities and display appropriate trust indicators.
