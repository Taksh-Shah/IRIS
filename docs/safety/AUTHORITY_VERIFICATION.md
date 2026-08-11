# IRIS Authority Verification in Offline Environments

**Document ID:** IRIS-SAFETY-001  
**Version:** 1.0  
**Status:** Active  

---

## 1. Problem Statement

Emergency authorities (NDRF, Red Cross, SDRF) must be able to prove their identity to IRIS users in environments with no internet connectivity. A user receiving an emergency broadcast needs confidence that the instruction comes from a legitimate authority, not a bad actor impersonating one.

Traditional identity verification relies on online certificate validation. IRIS must provide equivalent assurance entirely offline, using only pre-loaded trust anchors and locally verifiable cryptographic proofs.

This document describes how IRIS achieves offline authority verification, what QR-based credential distribution looks like in the field, and what the system does when verification is not possible.

---

## 2. Root of Trust Architecture

### 2.1 Pre-Installed Authority Public Keys

IRIS ships with a set of authority public keys compiled into the application binary. These keys are the root of trust for all authority verification.

```
IRIS App Binary
└── iris-core/src/trust/root_cas.rs
    ├── NDRF Root CA public key (Ed25519, 32 bytes)
    ├── Red Cross India Root CA public key
    ├── SDRF Root CA public key
    └── Root key set hash (BLAKE3 of all keys concatenated)
```

The root key set is included using Rust's `include_bytes!` macro, meaning the public keys are embedded at compile time. The build process signs the release binary with the IRIS signing key, and the app store's signature verification ensures the binary has not been tampered with.

**What "pre-installed" means for security:**
- A user cannot add or remove root CAs without installing a modified IRIS binary
- A modified IRIS binary would fail app store signature verification
- The root CAs are the same across all IRIS installations from a given release
- Root CA key changes require a new app release (by design — provides user visibility)

### 2.2 Trust Chain

```
IRIS Root CA Key (in binary)
        │ signs
        ▼
NDRF Regional CA Certificate (issued offline, stored on authority device)
        │ signs
        ▼
NDRF-GJ-01 Authority Certificate (on field commander's device)
        │ used to sign
        ▼
Emergency Broadcast Bundle (received by end user)
```

Verification at the user's device:
1. Extract NDRF-GJ-01 authority certificate from the bundle
2. Extract NDRF Regional CA certificate from the bundle (authority includes chain)
3. Verify Regional CA cert is signed by NDRF Root CA (pre-installed key)
4. Verify Authority Certificate is signed by Regional CA key
5. Verify Bundle is signed by Authority Certificate's public key
6. Check none of the certificates are expired or revoked

All steps use local computation only — no network calls.

---

## 3. QR Code-Based Authority Credential Distribution

In disaster scenarios, authority devices may need to issue short-lived credentials to field responders who arrive with standard IRIS installations (no pre-loaded authority certificates).

### 3.1 QR Code Credential Format

An authority can generate a QR code that encodes a short-lived capability token:

```
QR Payload (Base64URL-encoded CBOR):
{
  type: "iris-authority-credential",
  version: 1,
  token: <CapabilityToken CBOR>,
  cert_chain: [<AuthorityCertificate CBOR>, ...],  ; up to 3 certs in chain
  display_name: "NDRF Gujarat Field Team",
  qr_expiry: 1704153600,  ; when the QR code itself expires (not the token)
}
```

The QR code is displayed on the authority's device screen. A field responder scans it with their IRIS app. The app:
1. Decodes and validates the certificate chain (offline, using pre-installed root keys)
2. Validates the capability token (signed by the authority cert's key)
3. Prompts the user: "Accept credential from NDRF Gujarat? Valid 24 hours. [Accept / Reject]"
4. On acceptance: stores the credential locally; the device now has elevated capabilities

### 3.2 QR Code Security Properties

- **No replay:** QR codes include a `qr_expiry` field; old QR codes cannot be used after this timestamp
- **Offline verification:** The entire chain is embedded in the QR code; no network needed
- **User consent:** Credential acceptance always requires explicit user tap
- **Bounded validity:** Tokens issued via QR code are limited to 72 hours maximum
- **Screen-only:** QR codes are displayed on screen, not stored in a file, making capture harder (not impossible)

### 3.3 QR Code UX Flow

```
Authority Device Screen:
┌────────────────────────────────┐
│  NDRF AUTHORITY CREDENTIAL     │
│  ┌──────────────────────────┐  │
│  │  [QR Code]               │  │
│  │                          │  │
│  └──────────────────────────┘  │
│  Valid for: 72 hours           │
│  Capabilities:                 │
│    ✓ Emergency broadcast       │
│    ✓ Channel publish           │
│  Geographic scope: Gujarat     │
│                                │
│  Scan with IRIS app to accept  │
└────────────────────────────────┘

Recipient App After Scan:
┌────────────────────────────────┐
│  Accept Authority Credential?  │
│                                │
│  From: NDRF-GJ-01              │
│  ✓ Verified by NDRF Root CA    │
│                                │
│  Grants:                       │
│  • Emergency broadcast rights  │
│  • Channel publish (Gujarat)   │
│                                │
│  Valid until: 24h from now     │
│                                │
│  [ACCEPT]        [REJECT]      │
└────────────────────────────────┘
```

### 3.4 Physical Security of QR Codes

QR codes should be treated as physical credentials. IRIS guidance for authorities:
- Do not display QR codes in public settings (they can be photographed and scanned by anyone)
- Use QR codes only for direct transfer to verified field team members
- Generate short-validity QR codes (6-hour validity for field distribution)
- After issuing credentials, revoke unused QR codes by revoking the underlying token via emergency revocation bundle

---

## 4. Certificate Chain Validation (Offline)

The full offline validation procedure, implemented in `crates/identity/src/validator.rs`:

```rust
pub struct OfflineValidator {
    root_keys: &'static [RootCaEntry],
    revocation_filter: Arc<RwLock<RevocationFilter>>,
    clock: Arc<dyn Clock>,
}

impl OfflineValidator {
    pub fn validate_authority_chain(
        &self,
        leaf_cert: &AuthorityCertificate,
        chain: &[AuthorityCertificate],
    ) -> Result<ValidationResult, ValidationError> {
        let now = self.clock.now();
        
        // 1. Validate leaf certificate time bounds
        if now < leaf_cert.issued_at {
            return Err(ValidationError::NotYetValid { cert: leaf_cert.cert_id });
        }
        if now > leaf_cert.expires_at + Duration::from_secs(1800) { // 30min tolerance
            return Err(ValidationError::Expired { cert: leaf_cert.cert_id });
        }
        
        // 2. Check revocation filter
        if self.revocation_filter.read().unwrap().is_revoked(&leaf_cert.cert_id) {
            return Err(ValidationError::Revoked { cert: leaf_cert.cert_id });
        }
        
        // 3. Find and validate parent
        let parent = self.find_parent(leaf_cert, chain)?;
        let parent_key = Ed25519PublicKey::from_bytes(&parent.subject_public_key)?;
        parent_key.verify(&leaf_cert.signing_input(), &leaf_cert.issuer_signature)?;
        
        // 4. Check if parent is a root CA
        if let Some(root_entry) = self.root_keys.iter()
            .find(|r| r.public_key == parent.subject_public_key.as_slice())
        {
            // Chain terminates at trusted root
            return Ok(ValidationResult {
                authority_name: leaf_cert.authority_name.clone(),
                authority_type: leaf_cert.authority_type,
                root_ca_name: root_entry.name,
                chain_depth: chain.len(),
                expires_at: leaf_cert.expires_at,
                geographic_scope: leaf_cert.geographic_scope.clone(),
            });
        }
        
        // 5. Recursively validate parent
        self.validate_authority_chain(&parent, chain)
    }
}
```

### 4.1 Clock Uncertainty Handling

When local time is uncertain (no GPS, no peer sync, device clock potentially wrong):

```rust
fn check_cert_validity(
    cert: &AuthorityCertificate,
    now: SystemTime,
    clock_confidence: ClockConfidence,
) -> ValidityStatus {
    let tolerance = match clock_confidence {
        ClockConfidence::High => Duration::from_secs(1800),   // ±30 min
        ClockConfidence::Medium => Duration::from_secs(7200), // ±2 hours
        ClockConfidence::Low => Duration::from_secs(86400),   // ±24 hours (rarely used)
        ClockConfidence::Unknown => {
            // Cannot validate timing; treat as potentially valid but warn
            return ValidityStatus::UncertainClockWarning;
        }
    };
    
    if now + tolerance < cert.issued_at {
        ValidityStatus::NotYetValid
    } else if now - tolerance > cert.expires_at {
        ValidityStatus::Expired
    } else {
        ValidityStatus::Valid
    }
}
```

---

## 5. Fallback: Unverifiable Authority

When authority credentials cannot be verified (chain broken, cert expired, unknown root CA, clock too uncertain), IRIS follows a defined fallback policy.

### 5.1 Failure Modes and User Experience

| Failure Mode | What Happened | UI Treatment |
|-------------|---------------|-------------|
| Expired certificate | Authority cert expired before renewal | Warning: "Authority certificate expired [date]. May be genuine but cannot verify." |
| Unknown root CA | Cert chain doesn't reach any pre-installed root | Error: "Unknown certificate authority. Do NOT follow instructions without independent verification." |
| Signature invalid | Cert signature fails verification | Error: "VERIFICATION FAILED. This message was tampered with or is fraudulent." |
| Chain broken | Parent cert not included in message | Warning: "Certificate chain incomplete. Cannot verify authority." |
| Revoked certificate | Certificate in revocation filter | Error: "This authority credential has been REVOKED. Do not follow instructions." |
| Clock uncertain | Cannot validate cert timing | Warning: "Cannot verify credential timing. Device clock unsynchronized." |

### 5.2 Graceful Degradation Policy

The IRIS position on unverifiable authority messages:

1. **Never silently accept:** All verification failures are shown to the user. There is no "auto-trust" mode.
2. **Never block communication:** Unverified messages are delivered to the user with warnings. Users may choose to follow instructions from unverified sources at their own judgment — in a disaster, partial information may be better than none.
3. **P0 SOS always delivered:** Regardless of sender verification status, P0 SOS messages are always delivered and displayed. The risk of suppressing a real SOS outweighs the risk of displaying a fake one.
4. **Authority broadcasts without valid cert are displayed with full warning, not displayed as authority broadcasts.** They appear as standard messages with a prominent warning banner.

### 5.3 What Users Should Do with Unverified Authority Messages

IRIS includes a help screen explaining:

> "If you receive an emergency instruction from an unverified source:
> 1. Cross-check with other sources (radio, neighbors, visible official presence)
> 2. Consider the specific instruction — is it consistent with your situation?
> 3. When in doubt, move to safety first. Do not wait for verification.
> 4. Report suspicious authority claims using the flag function."

---

## 6. Security Boundaries

| Guarantee | Provided By |
|-----------|-------------|
| Cannot forge NDRF identity without NDRF private key | Ed25519 signature security |
| Cannot forge cert chain without root CA private key | Ed25519 signature security |
| Cannot use expired credentials silently | Expiry check with clock tolerance |
| Cannot use revoked credentials silently | Revocation Bloom filter check |
| Cannot intercept and modify authority message | ChaCha20-Poly1305 AEAD on payload |
| Cannot replay old authority message after TTL | Bundle TTL + dedup Bloom filter |

| NOT Guaranteed |
|----------------|
| Instant revocation in partitioned network |
| Protection against authority device theft with valid key |
| Verification when device clock is completely unknown |
| Preventing screenshot/copy of QR-delivered credentials |
