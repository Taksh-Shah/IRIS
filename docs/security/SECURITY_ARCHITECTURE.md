# IRIS Security Architecture

## Overview

IRIS implements a defense-in-depth security model across six distinct security layers. No single layer is trusted to be the sole line of defense. An attacker who compromises one layer faces independent controls at every other layer. This document defines the architecture, composition rules, and design principles governing security in the IRIS platform.

Security in IRIS serves three goals in order of priority:
1. **Safety** — emergency communication must reach recipients; security must never prevent a valid SOS from being delivered.
2. **Privacy** — user communication is private from all parties not explicitly included.
3. **Integrity** — messages arrive unmodified, from authenticated senders, and cannot be replayed.

---

## Security Layer Stack

```
┌─────────────────────────────────────────────────────┐
│  L6: Application Security (UI, permissions, consent) │
├─────────────────────────────────────────────────────┤
│  L5: Emergency Security (verified broadcast, SOS)    │
├─────────────────────────────────────────────────────┤
│  L4: Storage Security (encrypted at rest)            │
├─────────────────────────────────────────────────────┤
│  L3: Routing Security (authenticated routing)        │
├─────────────────────────────────────────────────────┤
│  L2: Message Security (E2EE, authentication)         │
├─────────────────────────────────────────────────────┤
│  L1: Transport Security (link layer encryption)      │
├─────────────────────────────────────────────────────┤
│  L0: Physical / Platform (OS, hardware, Rust safety) │
└─────────────────────────────────────────────────────┘
```

Each layer is defined independently; a compromise at one layer does not automatically cascade to another. A malicious relay node controls L1 (transport) but cannot break L2 (E2EE message content).

---

## L0: Physical and Platform Security

### Rust Memory Safety
The core protocol engine is written in Rust, which eliminates entire classes of memory-safety vulnerabilities:
- No buffer overflows in safe Rust
- No use-after-free
- No data races (ownership model enforced at compile time)
- Integer overflow checking in debug builds; wrapping semantics in release

Unsafe Rust blocks are minimized, documented, and reviewed. All FFI boundaries (Kotlin JNI, Swift bridging header) are treated as trust boundaries and input-validated.

### OS Platform Controls
- Android: Application sandbox, permission model, keystore-backed key storage
- iOS: App sandbox, Secure Enclave for key storage, entitlement model
- Keys never written to unprotected storage

### Hardware Security
- Cryptographic keys generated using OS CSPRNG (`getrandom` crate on Rust, `SecRandomCopyBytes` on iOS, `KeyPairGenerator` with `AndroidKeyStore` on Android)
- Biometric unlock gates key access where supported
- Secure Enclave (iOS) and StrongBox (Android) for long-term identity keys where available

---

## L1: Transport Security (Link Layer Encryption)

Every communication link is encrypted independently of message-level encryption. This provides:
- Confidentiality of message existence on each hop (a relay knows a message transited, not its content)
- Protection against passive eavesdroppers monitoring a single link

### BLE Transport Security
- Pairing-based encryption for long-term links (LE Secure Connections, AES-128-CCM)
- Ephemeral session keys derived per-connection using ECDH on P-256
- MAC addresses rotated on a randomized schedule (15–60 minutes) to prevent tracking

### Wi-Fi Direct Security
- WPA3-based session encryption
- Device credentials not reused across sessions
- Group Owner role selection does not leak persistent identity

### Wi-Fi Aware (NAN)
- NAN data path uses PMKSA-derived session keys
- Service discovery advertisements are minimal (capability flags only, no content)

### LoRa Transport Security
- AES-128-CTR encryption per packet using session key
- Session key negotiated out-of-band (BLE handshake) before LoRa data transfer
- Replay protection at packet level using counter field

### Satellite Transport
- Uses provider transport-layer encryption (e.g., Iridium SBD's link encryption)
- Additional IRIS message-level encryption applied before transmission (L2 provides content protection regardless)

### Key Exchange Protocol
```
Initiator                           Responder
    |                                    |
    |--- HELLO (ephemeral_pub_key_A) --> |
    |                                    | generate ephemeral_key_B
    |<-- HELLO_ACK (ephemeral_pub_key_B) |
    |                                    |
    shared_secret = ECDH(priv_A, pub_B)  shared_secret = ECDH(priv_B, pub_A)
    session_key = HKDF(shared_secret, "IRIS-LINK-v1", context)
    |                                    |
    |=== Encrypted link established ===>|
```

---

## L2: Message Security (End-to-End Encryption)

Message security is end-to-end: only the intended recipient can decrypt content. Relay nodes forward ciphertext without access to plaintext.

### Identity Keys
Every IRIS identity has:
- **Ed25519 signing key pair** — long-term, stored in hardware security element
- **X25519 key agreement key pair** — long-term, used for key exchange

Key fingerprints are displayed to users for manual verification (safety number / QR code comparison).

### Direct Message Encryption
Protocol: **Double Ratchet Algorithm** (same as Signal Protocol)
- Forward secrecy: compromise of current key does not expose past messages
- Break-in recovery: new message keys derived from fresh DH ratchet steps
- Message keys: AES-256-GCM with 128-bit authentication tag
- Header encryption using separate ratchet chain

```
Message Envelope:
┌────────────────────────────────────────────────────────┐
│ sender_id          (32 bytes, Ed25519 public key)       │
│ recipient_id       (32 bytes, Ed25519 public key)       │
│ message_id         (16 bytes, UUID v7)                  │
│ timestamp          (8 bytes, Unix epoch ms)             │
│ ratchet_header     (encrypted, 64 bytes)                │
│ ciphertext         (variable, AES-256-GCM)              │
│ auth_tag           (16 bytes, GCM tag)                  │
│ signature          (64 bytes, Ed25519 over all fields)  │
└────────────────────────────────────────────────────────┘
```

### Group Message Encryption
Protocol: **Sender Keys** (same as Signal Groups)
- Each member distributes a sender key to all group members
- Messages encrypted with sender's chain key (AES-256-GCM)
- New member joining: all existing members re-distribute sender keys
- Member leaving: sender key rotation by all remaining members

For groups > 100 members or disaster channels: **MLS (Messaging Layer Security, RFC 9420)** is used for efficient key management.

### Key Verification
- Users verify contacts via safety number (fingerprint of both public keys)
- QR code scanning for in-person verification
- Unverified contacts get warning indicator (not blocked — blocking unverified contacts would prevent emergency communication)

---

## L3: Routing Security (Authenticated Routing)

### Route Advertisement Authentication
- Route advertisements (node announces reachability to other nodes) are signed with the advertising node's Ed25519 key
- Unsigned route advertisements are rejected
- Route advertisements include timestamp and TTL to prevent replay

### Relay Node Authentication
- Every relay hop adds a signed forwarding record
- Recipients can verify the chain of custody
- Relay nodes cannot modify message content (L2 protects this) but can be audited for forwarding behavior

### Authenticated Delivery Receipts
- Delivery receipts are signed by the recipient's key
- Recipients cannot deny receipt (non-repudiation for delivery confirmation)
- Relay nodes accumulate signed receipts to build reputation

### Routing Table Integrity
- Local routing tables are signed with device key and stored encrypted
- Tampering with stored routing tables detectable on next validation cycle

---

## L4: Storage Security (Encrypted At Rest)

### Database Encryption
- SQLite database encrypted with SQLCipher (AES-256-CBC)
- Database key derived from device unlock credential + device key
- Key derivation: `PBKDF2-HMAC-SHA256(user_credential, device_salt, 600000 iterations)`
- Key stored in Android Keystore / iOS Secure Enclave, bound to biometric/PIN

### Message Store Encryption
- Individual messages additionally encrypted at field level for forward secrecy
- Old message keys deleted after configurable retention window (default: 30 days)
- Ephemeral messages (user-configured TTL) use additional key layer deleted on expiry

### Contact Store
- Contact identifiers (public keys) stored in plaintext for lookup
- Contact metadata (name, notes) encrypted with user key
- Contact list never transmitted without explicit consent

### Routing Cache
- Routing state (contact history, delivery probability) encrypted at rest
- This data is sensitive (reveals communication patterns)

### Key Storage
- Master identity keys: hardware security element (Secure Enclave / StrongBox)
- Session keys: in-memory only, cleared on session end
- Ratchet state: encrypted in app-private storage
- Emergency authority certificates: encrypted + integrity-protected

---

## L5: Emergency Security

Emergency messages receive additional security treatment because they are trusted at higher fidelity than ordinary messages.

### Verified Emergency Broadcasts
- Only nodes holding a valid Emergency Authority Certificate (EAC) can issue emergency broadcasts
- EAC is a certificate chain: National CA → State Authority → District Authority → Local Responder
- Every relay MUST verify the EAC signature before forwarding an emergency broadcast
- Relaying an unverified emergency broadcast is a protocol violation — the relay will be flagged

### SOS Message Authentication
- SOS messages are signed by the user's long-term Ed25519 key
- Recipients can verify the SOS came from a known identity
- SOS from unknown identities are forwarded but marked as "unverified source"

### Emergency Channel Access
- Emergency broadcast channels require verified authority credentials
- Community emergency channels (neighborhood alerts) require local admin authorization
- All emergency broadcasts logged with sender identity and timestamp (audit trail)

### Anti-Abuse in Emergency System
- Rate limit: maximum 3 SOS per device per hour (hardware-enforced via device key)
- False SOS cancel mechanism: signed cancel message must come from same identity
- Emergency authority certificates have expiry (maximum 1 year, renewable)

---

## L6: Application Security

### Permission Model
- Microphone, camera, location: explicit per-use permission (not blanket grant)
- Contacts access: optional; IRIS does not require access to system contact book
- Background operation: minimal — only routing state maintenance

### UI Security
- No cleartext display of sensitive data in notifications (show "New message from [contact]" not content)
- Secure text entry for PIN/passphrase
- Screenshot prevention on sensitive screens (optional, user-configurable)

### IPC Security
- Android intents validated before processing
- No exported activities or receivers except those required for system integration
- Deep links validated against allowlist

---

## Threat Actor Categories

| Threat Actor | Capabilities | Target | Priority |
|---|---|---|---|
| Passive eavesdropper | Captures radio signals | Message content, metadata | High |
| Active relay attacker | Runs malicious relay node | Traffic manipulation | High |
| Sybil attacker | Creates many fake identities | Routing disruption | Medium |
| Emergency abuser | Sends fake SOS/broadcasts | Emergency system integrity | High |
| Physical device attacker | Physical access to device | Stored keys and messages | High |
| Government surveillance adversary | Network-level monitoring | Communication patterns | Medium |
| Insider/relay operator | Legitimate relay access | Metadata collection | Medium |

---

## Security by Design Principles

### Principle 1: Fail Secure
When security state is uncertain, fail in the secure direction. An unverified emergency broadcast is not forwarded (may miss some legitimate broadcasts) rather than forwarding all unverified broadcasts (allows fake broadcasts).

Exception: SOS from an unknown identity IS forwarded (safety takes priority over perfect authentication in life-safety scenarios) but is marked as unverified.

### Principle 2: Minimal Privilege
Relay nodes have the minimum information needed to forward messages:
- Source node identifier (needed for routing)
- Destination node identifier (needed for routing)
- Message ID (needed for deduplication)
- Expiry time (needed for TTL enforcement)
- NOT: message content, sender-recipient relationship history, location

### Principle 3: Cryptographic Agility
Algorithms are identified by a version/suite ID in message headers. Migration path exists for post-quantum algorithms (ML-KEM, ML-DSA) when standardized and viable on mobile hardware.

```
crypto_suite field values:
  0x01 = Ed25519 signing + X25519 key agreement + AES-256-GCM (current)
  0x02 = ML-DSA-44 + ML-KEM-512 + AES-256-GCM (planned, post-quantum)
```

### Principle 4: No Security Through Obscurity
Protocol specifications are public. Security relies entirely on cryptographic primitives and key secrecy, not on keeping the protocol design hidden.

### Principle 5: Emergency Overrides Are Auditable
The system allows emergency overrides of normal security controls (e.g., receiving messages from unknown identities during disaster mode). Every override is logged, signed, and auditable post-incident.

### Principle 6: Privacy-Preserving by Default
No location in message headers unless explicitly included. No persistent identifiers in BLE advertisements. Minimum metadata in routing headers.

---

## Security Layer Composition

The layers compose such that an attacker must simultaneously defeat multiple independent controls:

| Attack Goal | Must Defeat |
|---|---|
| Read message content | L1 (link encryption) AND L2 (E2EE) |
| Inject fake message | L2 (signature verification) AND L3 (routing auth) |
| Track user location | L1 (MAC rotation) AND L6 (no location in headers) |
| Spoof emergency broadcast | L5 (EAC signature verification at every relay) |
| Read stored messages | L4 (database encryption) AND L0 (hardware key storage) |
| Replay old message | L2 (message ID + replay window) AND L3 (route auth timestamp) |

No single point of failure exists in the composition. The system degrades gracefully: if one layer has a vulnerability, the remaining layers limit the blast radius.

---

## Security Review and Maintenance

- Cryptographic library audit: annual
- Protocol security review: on any protocol change
- Third-party penetration testing: pre-major-release
- Vulnerability disclosure: security@iris.network (72-hour response SLA)
- CVE tracking for all dependencies
- Automated dependency vulnerability scanning in CI

See `SECURITY_TESTING.md` for testing methodology and `THREAT_MODEL.md` for detailed threat analysis.
