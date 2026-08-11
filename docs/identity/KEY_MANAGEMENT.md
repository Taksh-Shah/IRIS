# Key Management Lifecycle

## Overview

Key management covers the complete lifecycle of cryptographic keys in IRIS:
generation, secure storage, use, rotation, backup, recovery, and revocation.
Each phase has different requirements on each platform.

## Key Generation

### Algorithm Choice

IRIS uses **Ed25519** for identity signing and **X25519** for key exchange:

| Purpose | Algorithm | Key Size | Security Level |
|---------|-----------|----------|----------------|
| Identity / Signing | Ed25519 | 32 bytes private, 32 bytes public | ~128-bit |
| Key Exchange (E2EE) | X25519 ECDH | 32 bytes private, 32 bytes public | ~128-bit |
| Symmetric Encryption | AES-256-GCM | 32 bytes | 256-bit |
| Key Derivation | HKDF-SHA256 | — | — |
| Password KDF | Argon2id | — | Memory-hard |

Ed25519 advantages: fast (100k signs/second), compact (64-byte signatures),
no risk of nonce reuse vulnerability (unlike ECDSA), deterministic.

### Android Key Generation

```kotlin
// Option A: Android Keystore (hardware-backed on modern devices)
// Private key never leaves the secure element
fun generateIdentityKeyAndroid(): KeyPair {
    val keyPairGenerator = KeyPairGenerator.getInstance(
        KeyProperties.KEY_ALGORITHM_EC,
        "AndroidKeyStore"
    )
    keyPairGenerator.initialize(
        KeyGenParameterSpec.Builder(
            "iris_identity_key",
            KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY
        )
        .setDigests(KeyProperties.DIGEST_SHA256)
        .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
        // Note: AndroidKeyStore supports secp256r1 (ECDSA P-256) not Ed25519
        // Ed25519 available via BouncyCastle for software keys,
        // or accept ECDSA P-256 for hardware-backed keys
        .setUserAuthenticationRequired(false) // Don't require biometrics for mesh relay
        .setInvalidatedByBiometricEnrollment(false)
        .build()
    )
    return keyPairGenerator.generateKeyPair()
}

// Option B: Rust ring crate (software, Ed25519)
// Use when Ed25519 required and hardware backing not critical
// Private key stored encrypted in Keystore-protected encrypted file
```

**Android Keystore limitation**: Ed25519 not directly supported as of Android 14.
IRIS uses ECDSA P-256 for Android Keystore (hardware-backed), and Ed25519 for the
higher-level IRIS protocol (keys derived from or converted from the P-256 key).
For devices without hardware keystore, Ed25519 via `ring` crate in a Keystore-
encrypted file.

### iOS Key Generation

```swift
// Secure Enclave: hardware-backed on iPhone 5s (A7) and later
// Note: Secure Enclave supports ECDSA P-256 (secp256r1), not Ed25519
// Same strategy as Android: P-256 for hardware backing, Ed25519 at protocol level

import CryptoKit

func generateIdentityKeyIOS() throws -> SecureEnclave.P256.Signing.PrivateKey {
    // This key never leaves the Secure Enclave — cannot be exported
    let privateKey = try SecureEnclave.P256.Signing.PrivateKey(
        accessControl: SecAccessControl.create(
            allocator: nil,
            protection: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            flags: .privateKeyUsage,
            error: nil
        )!
    )
    return privateKey
}

// For Ed25519 (when Secure Enclave not required):
func generateEd25519KeyIOS() -> Curve25519.Signing.PrivateKey {
    // Stored in iOS Keychain
    return Curve25519.Signing.PrivateKey()
}
```

### Desktop Key Generation

```rust
// Linux/macOS/Windows: generate Ed25519 key, encrypt and save to disk
use ring::signature::{Ed25519KeyPair, KeyPair};
use ring::rand::SystemRandom;

pub fn generate_identity_key_desktop() -> Result<Ed25519KeyPair> {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| KeyError::GenerationFailed)?;

    Ok(Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| KeyError::ParseFailed)?)
}
```

## Key Storage

### Android Keystore System

```
Key material → Android Keystore
                  ↓
        (Hardware TEE on API 23+ devices)
        OR
        (Software keystore on older/emulator)
```

Advantages:
- Private key never extractable (hardware-backed)
- Key usage requires device unlock (configurable)
- Key invalidated if device factory reset

Verification:
```kotlin
fun isHardwareBacked(keyAlias: String): Boolean {
    val keyInfo = KeyFactory.getInstance("EC")
        .getKeySpec(
            (KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
                .getKey(keyAlias, null)),
            KeyInfo::class.java
        )
    return keyInfo.isInsideSecureHardware
}
```

### iOS Secure Enclave

Available on all iPhones since iPhone 5s (2013). The private key:
- Is generated inside the Secure Enclave
- Never exists in application memory
- Cannot be exported in any form
- Signing operations happen inside the Secure Enclave

```swift
// Sign with Secure Enclave key — signing happens in hardware
func sign(data: Data, with key: SecureEnclave.P256.Signing.PrivateKey) throws -> P256.Signing.ECDSASignature {
    return try key.signature(for: data)
}
```

### Desktop Encrypted Key File

```rust
pub struct EncryptedKeyFile {
    pub version: u8,           // 1
    pub algorithm: String,     // "Ed25519"
    pub kdf: String,           // "argon2id"
    pub kdf_params: Argon2Params,
    pub salt: [u8; 32],
    pub nonce: [u8; 12],       // AES-256-GCM nonce
    pub ciphertext: Vec<u8>,   // encrypted PKCS#8 key bytes
    pub tag: [u8; 16],         // GCM auth tag
}

impl EncryptedKeyFile {
    pub fn create(key_pair: &Ed25519KeyPair, password: &str) -> Result<Self> {
        let salt: [u8; 32] = OsRng.gen();
        let kdf_params = Argon2Params {
            memory_kib: 65536,
            iterations: 3,
            parallelism: 4,
        };

        // Derive 32-byte encryption key from password
        let mut key = [0u8; 32];
        argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(
                kdf_params.memory_kib,
                kdf_params.iterations,
                kdf_params.parallelism,
                Some(32)
            )?
        ).hash_password_into(password.as_bytes(), &salt, &mut key)?;

        // Encrypt the PKCS#8 key bytes
        let nonce: [u8; 12] = OsRng.gen();
        let cipher = Aes256Gcm::new_from_slice(&key).unwrap();
        let mut ciphertext = key_pair.to_pkcs8_der()?.to_vec();
        let tag = cipher.encrypt_in_place_detached(
            Nonce::from_slice(&nonce),
            b"", // no additional data
            &mut ciphertext
        )?;

        Ok(EncryptedKeyFile {
            version: 1,
            algorithm: "Ed25519".to_string(),
            kdf: "argon2id".to_string(),
            kdf_params,
            salt,
            nonce,
            ciphertext,
            tag: tag.into(),
        })
    }
}
```

File location:
- Linux: `~/.local/share/iris/identity.key`
- macOS: `~/Library/Application Support/io.iris.node/identity.key`
- Windows: `%APPDATA%\IRIS\identity.key`

## Forward Secrecy

Every message uses an ephemeral X25519 key pair for key exchange:

```rust
pub fn create_message_keys(
    sender_static: &IrisIdentity,
    recipient_public: &ed25519_dalek::PublicKey,
) -> MessageKeys {
    // Convert recipient Ed25519 public key to X25519 for Diffie-Hellman
    let recipient_x25519 = convert_ed25519_to_x25519(recipient_public);

    // Generate ephemeral X25519 key pair for this message
    let ephemeral_private = x25519_dalek::EphemeralSecret::new(OsRng);
    let ephemeral_public = x25519_dalek::PublicKey::from(&ephemeral_private);

    // ECDH: shared secret
    let shared_secret = ephemeral_private.diffie_hellman(&recipient_x25519);

    // HKDF to derive message key and MAC key
    let (msg_key, mac_key) = hkdf_expand(
        shared_secret.as_bytes(),
        b"IRIS-message-key-v1",
        b"",
        64  // 32 bytes for AES key + 32 bytes for HMAC key
    );

    MessageKeys {
        ephemeral_public,      // included in message header
        encrypt_key: msg_key,
        mac_key,
    }
}
```

Recipient derives the same keys using their static private key + sender's ephemeral public key.
The ephemeral key is discarded after use. Compromise of static keys does not reveal past messages.

## Key Rotation

```rust
pub async fn rotate_identity_key(
    current: &IrisIdentity,
    secure_storage: &SecureStorage,
    network: &Network,
) -> Result<IrisIdentity> {
    // Generate new identity
    let new_identity = initialize_identity(secure_storage).await?;

    // Create signed rotation announcement
    let announcement = KeyRotationAnnouncement {
        old_node_id: current.node_id.clone(),
        new_node_id: new_identity.node_id.clone(),
        rotation_at: SystemTime::now(),
        reason: RotationReason::UserRequested,
    };

    // Sign with OLD key (proves the old key owner authorized the rotation)
    let signed = current.sign(&announcement.encode())?;

    // Broadcast to all known contacts
    network.broadcast(SignedKeyRotationAnnouncement {
        announcement,
        signature: signed,
        old_public_key: current.public_key.clone(),
    }).await?;

    // Archive old identity (keep for verifying old messages, not for new ones)
    secure_storage.archive_identity(current).await?;

    Ok(new_identity)
}
```

## Group Keys

For group channels:

```rust
pub struct GroupKey {
    pub group_id: GroupId,
    pub symmetric_key: [u8; 32],   // AES-256-GCM key
    pub issued_by: NodeId,         // group admin
    pub issued_at: SystemTime,
    pub version: u32,              // increment on rotation
}

// Admin distributes group key encrypted individually to each member
pub fn distribute_group_key(
    group_key: &GroupKey,
    members: &[(NodeId, ed25519_dalek::PublicKey)],
    admin_identity: &IrisIdentity,
) -> Vec<EncryptedGroupKey> {
    members.iter().map(|(member_id, member_public)| {
        let message_keys = create_message_keys(admin_identity, member_public);
        let encrypted_group_key = aes_256_gcm_encrypt(
            &group_key.encode(),
            &message_keys.encrypt_key,
        );
        EncryptedGroupKey {
            recipient: member_id.clone(),
            group_id: group_key.group_id.clone(),
            key_version: group_key.version,
            ephemeral_sender_public: message_keys.ephemeral_public,
            encrypted_payload: encrypted_group_key,
        }
    }).collect()
}
```

## Key Revocation

```rust
pub struct KeyRevocationMessage {
    pub revoked_node_id: NodeId,
    pub revocation_at: SystemTime,
    pub reason: RevocationReason,
    /// Signed with the key being revoked — proves key owner authorized revocation
    pub signature: Signature,
}

pub enum RevocationReason {
    DeviceLost,
    DeviceCompromised,
    IdentityChange,
    OrganizationDeparture, // for authority certificates
}
```

**Offline revocation limitation**: if a node is not online when a revocation is
broadcast, it will not receive the revocation until it rejoins the network. In the
interim, it may still accept messages from a revoked key. This is an inherent
limitation of offline-capable systems. Mitigation: short message TTLs, re-broadcast
of revocations when nodes reconnect.
