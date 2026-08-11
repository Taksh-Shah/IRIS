# Trust Model

## Overview

Trust in IRIS answers: "Should I believe this message is from who it claims to be from?"
Trust is separate from routing — IRIS relays P0 emergency messages from unknown nodes
(routing does not require trust), but trust determines how the user experience is presented
and how much weight to give content.

## Trust-on-First-Use (TOFU)

When a message arrives from a previously-unknown NodeId:

```rust
pub fn handle_first_contact(
    sender: &NodeId,
    sender_public_key: &ed25519_dalek::PublicKey,
    contact_store: &mut ContactStore,
) -> TrustLevel {
    // Verify: NodeId derives from public key (anti-spoofing)
    let expected_node_id = NodeId::from_public_key(sender_public_key);
    if &expected_node_id != sender {
        // NodeId doesn't match public key — reject
        return TrustLevel::Rejected;
    }

    // Add as UNVERIFIED contact
    contact_store.add_contact(Contact {
        node_id: sender.clone(),
        public_key: sender_public_key.clone(),
        trust_level: TrustLevel::Unverified,
        first_seen: SystemTime::now(),
        last_seen: SystemTime::now(),
        display_name: None,
        verification_records: vec![],
    });

    TrustLevel::Unverified
}
```

The user is shown new contacts with a "New contact" indicator. No automatic trust.
No automatic rejection. TOFU means: "I'll talk to you, but I don't know who you are."

## Trust Levels

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustLevel {
    /// NodeId seen but we have no public key confirmation (should not persist)
    Unknown = 0,

    /// Public key verified (NodeId matches), no other verification
    Unverified = 1,

    /// Mutual message exchange — they replied to us, or we've had conversation
    Contact = 2,

    /// Out-of-band verification completed (QR code, SAS)
    Verified = 3,

    /// User explicitly marked as trusted (friend, colleague)
    TrustedContact = 4,

    /// Certificate chain from known authority (NDRF, Red Cross)
    Authority = 5,
}
```

Trust level determines UI presentation:
- `Unknown/Unverified`: grey avatar, "Unknown sender"
- `Contact`: standard avatar, display name shown (but marked unverified)
- `Verified`: checkmark indicator, display name trusted
- `TrustedContact`: highlighted, user's chosen label shown
- `Authority`: badge with organization logo, role shown

## Trust Transitions

```rust
pub fn transition_trust(
    contact: &mut Contact,
    new_evidence: TrustEvidence,
) -> Result<TrustLevel> {
    let new_level = match (&contact.trust_level, &new_evidence) {

        // Receiving a reply = mutual exchange → Contact
        (TrustLevel::Unverified, TrustEvidence::MutualExchange) => TrustLevel::Contact,

        // QR code scan → Verified (from any lower level)
        (_, TrustEvidence::QrCodeVerification { verifier_sig, .. }) => {
            verify_qr_code_verification(verifier_sig, contact)?;
            TrustLevel::Verified
        }

        // SAS (Short Authentication String) → Verified
        (_, TrustEvidence::SasVerification { sas_code }) => {
            verify_sas(sas_code, &contact.public_key, &self_public_key())?;
            TrustLevel::Verified
        }

        // User action → TrustedContact
        (_, TrustEvidence::UserTrusted) => TrustLevel::TrustedContact,

        // Valid authority certificate
        (_, TrustEvidence::AuthorityCertificate { cert }) => {
            verify_certificate_chain(cert, &TRUSTED_ROOT_CERTS)?;
            TrustLevel::Authority
        }

        // Invalid transitions
        _ => return Err(TrustError::InvalidTransition),
    };

    contact.trust_level = new_level.clone();
    contact.last_updated = SystemTime::now();
    Ok(new_level)
}
```

## Out-of-Band Verification

### QR Code Verification

Two users physically present to each other:

```
User A opens IRIS → "Verify Contact" → displays QR code
    QR encodes: { node_id, public_key, display_name, timestamp, signature }

User B scans QR with IRIS camera:
    1. Decode QR payload
    2. Verify signature (payload signed by A's private key)
    3. Verify NodeId matches public_key in payload
    4. Verify timestamp is recent (< 5 minutes, anti-replay)
    5. If all pass: create VerificationRecord signed by B's key
    6. B's status for A: Verified

Simultaneously A scans B's QR → mutual verification
Both end up with Verified status for each other
```

```rust
pub struct QrCodeVerificationPayload {
    pub node_id: NodeId,
    pub public_key: ed25519_dalek::PublicKey,
    pub display_name: Option<String>,
    pub timestamp: SystemTime,
    /// Signs all above fields — proves key ownership
    pub signature: Signature,
}
```

### Short Authentication String (SAS)

For voice verification over radio/phone when camera is unavailable:

```rust
pub fn compute_sas(
    my_public_key: &ed25519_dalek::PublicKey,
    their_public_key: &ed25519_dalek::PublicKey,
) -> String {
    // Combine both public keys (sorted, to be order-independent)
    let mut keys = [my_public_key.as_bytes(), their_public_key.as_bytes()];
    keys.sort();

    let combined = [keys[0], keys[1]].concat();
    let hash = sha2::Sha256::digest(&combined);

    // Encode first 3 bytes as 5-char alphanumeric (base32-like)
    encode_sas(&hash[..3])
}

fn encode_sas(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ"; // 32 chars, no ambiguous chars
    let value = u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]]);
    let mut result = String::with_capacity(5);
    let mut v = value;
    for _ in 0..5 {
        result.push(ALPHABET[(v % 32) as usize] as char);
        v /= 32;
    }
    result.chars().rev().collect()
}
```

Example SAS: "MANGO" or "7R4KX". Both users compute independently.
If codes match (verified verbally): mutual verification complete.
Probability of collision: 1/32^5 = 1/33,554,432 — negligible.

## Web of Trust

Contacts can vouch for others:

```rust
pub struct TrustVouching {
    pub voucher: NodeId,           // who is vouching
    pub vouched_for: NodeId,       // who is being vouched for
    pub voucher_trust_level: TrustLevel, // voucher's level with recipient
    pub timestamp: SystemTime,
    pub signature: Signature,      // voucher signs this vouching record
}

pub fn apply_web_of_trust(
    unknown: &NodeId,
    my_contacts: &ContactStore,
) -> Option<TrustLevel> {
    // Find if any of my TrustedContacts have vouched for `unknown`
    for contact in my_contacts.trusted_contacts() {
        if let Some(vouching) = contact.vouchings.get(unknown) {
            if verify_vouching_signature(vouching, &contact.public_key).is_ok() {
                // One hop of transitivity: my trusted contact trusts them
                return Some(TrustLevel::Contact); // bump to Contact, not Verified
            }
        }
    }
    None
}
```

Web of trust grants at most `Contact` level transitively. Full verification
still requires out-of-band verification.

## Authority Certificates

### Certificate Structure

```rust
pub struct AuthorityCertificate {
    pub subject_node_id: NodeId,
    pub subject_public_key: ed25519_dalek::PublicKey,
    pub subject_display_name: String,
    pub organization: String,
    pub role: String,
    pub permissions: Vec<AuthorityPermission>,
    pub issued_by: NodeId,
    pub issuer_public_key: ed25519_dalek::PublicKey,
    pub issuer_certificate: Option<Box<AuthorityCertificate>>,
    pub valid_from: SystemTime,
    pub valid_until: SystemTime,
    pub certificate_id: Uuid,
    pub issuer_signature: Signature,  // signs all above fields
}

pub enum AuthorityPermission {
    IssueEvacuationOrders,
    IssueMedicalDirectives,
    AccessRestrictedChannels,
    RelayUnlimitedMessages,
    ViewAllMessages,  // for command coordination
}
```

### Certificate Chain Verification

```rust
pub fn verify_certificate_chain(
    cert: &AuthorityCertificate,
    trusted_roots: &[TrustedRoot],
) -> Result<VerificationResult> {
    // Step 1: Verify leaf certificate signature
    let issuer_key = &cert.issuer_public_key;
    let payload = cert.signing_payload();
    issuer_key.verify(&payload, &cert.issuer_signature)?;

    // Step 2: Verify issuer certificate (recursively)
    if let Some(issuer_cert) = &cert.issuer_certificate {
        verify_certificate_chain(issuer_cert, trusted_roots)?;
    } else {
        // No issuer certificate → must be self-signed root
        // Check against trusted roots list
        let is_trusted_root = trusted_roots.iter().any(|root| {
            root.public_key == cert.issuer_public_key
        });
        if !is_trusted_root {
            return Err(CertError::UntrustedRoot);
        }
    }

    // Step 3: Check validity period
    let now = SystemTime::now();
    if now < cert.valid_from || now > cert.valid_until {
        return Err(CertError::Expired);
    }

    Ok(VerificationResult::Valid {
        trust_level: TrustLevel::Authority,
        organization: cert.organization.clone(),
        role: cert.role.clone(),
    })
}
```

### Preloaded Root Certificates

IRIS ships with root certificates for recognized Indian disaster response organizations:

```rust
pub fn load_trusted_roots() -> Vec<TrustedRoot> {
    vec![
        TrustedRoot {
            organization: "NDRF India",
            public_key: include_bytes!("../certs/ndrf_root.pub"),
            embedded_since: "2024-01-01",
        },
        TrustedRoot {
            organization: "Red Cross Society of India",
            public_key: include_bytes!("../certs/rcsi_root.pub"),
            embedded_since: "2024-01-01",
        },
        TrustedRoot {
            organization: "SDMA (placeholder — each state issues own cert)",
            public_key: include_bytes!("../certs/sdma_root.pub"),
            embedded_since: "2024-06-01",
        },
    ]
}
```

Verification works offline — root certs are embedded in the app binary.

## Emergency Trust Override

For P0 SOS messages: trust verification is NOT a prerequisite for relay.

```rust
pub fn should_relay(msg: &IrisMessage, sender_trust: TrustLevel) -> bool {
    match msg.priority {
        // P0 SOS: relay regardless of trust — life safety
        Priority::P0 => true,

        // P1 Evacuation: relay if at least Unverified (public key verified)
        Priority::P1 => sender_trust >= TrustLevel::Unverified,

        // P2–P3: relay if Contact or higher
        Priority::P2 | Priority::P3 => sender_trust >= TrustLevel::Contact,

        // P4+: relay only if Verified or higher
        _ => sender_trust >= TrustLevel::Verified,
    }
}
```

Rationale: an unknown person may be dying. We cannot let bureaucratic trust
requirements prevent relaying a genuine SOS. The E2EE and digital signature still
protect against forgery — but we do not require pre-established trust for relaying.
