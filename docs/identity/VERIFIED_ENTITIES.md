# IRIS Verified Entities

**Document ID:** IRIS-ARCH-IDENT-003  
**Version:** 1.0  
**Status:** Active  

---

## 1. Overview

Verified entities are organizations and individuals in IRIS that have been granted elevated trust through an out-of-band verification process. Verification allows receivers to distinguish between:

- **Unverified node:** Any IRIS user; self-asserted identity; can send SOS, location, personal messages
- **Verified entity:** Organization whose identity has been confirmed by IRIS root CA process; can issue broadcasts, claim authority roles, publish to emergency channels

Verification is not a prerequisite for using IRIS. The system is designed so that even an unverified node's P0 SOS is treated as equally urgent. Verification grants additional broadcast and coordination capabilities, not basic communication rights.

---

## 2. Verified Entity Types

| Type Code | Entity Type | Examples | Authority Level |
|-----------|------------|----------|----------------|
| 0 | National Emergency Agency | NDRF (National Disaster Response Force), NDMA | Highest: can issue authority certs |
| 1 | State Emergency Agency | SDRF Gujarat, SDRF Maharashtra | High: can broadcast within state scope |
| 2 | Red Cross / Red Crescent | Indian Red Cross Society (chapter) | High: medical coordination authority |
| 3 | Government Hospital | AIIMS, Civil Hospital Ahmedabad | Medium: medical resource coordination |
| 4 | Municipal Emergency Service | Municipal fire brigade, civic body | Medium: local coordination |
| 5 | Registered NGO | Goonj, HelpNow, registered disaster NGOs | Limited: can post in NGO channels |
| 6 | Military (defense coordination) | Indian Army disaster cell (when authorized) | Scoped: specific operation |
| 7 | UN Agency | UNICEF, WFP disaster cell | High: when operating in India |
| 8 | Verified Journalist | Registered press with PCI credential | Limited: verified news source marking |

### 2.1 What Verification Grants

| Capability | Type 0 | Type 1–2 | Type 3–5 | Type 6–8 |
|-----------|--------|----------|----------|----------|
| Emergency broadcast to all nodes | Yes | Regional | No | No |
| Authority identity badge in UI | Yes | Yes | Yes | Yes |
| Publish to public emergency channels | Yes | Yes | Yes | Limited |
| Issue capability tokens to subordinates | Yes | Type ≤ own | No | No |
| Coordinate medical resource requests | Yes | Yes | Type 3 only | No |
| Claim verified source in news posts | No | No | No | Type 8 only |

---

## 3. Certificate Issuance Process

### 3.1 Pre-Disaster (Normal Operations)

The standard process for issuing authority certificates to emergency organizations:

```
Step 1: Organization submits application to IRIS Root CA team
   - Official government registration documents
   - Proof of disaster response mandate (e.g., NDRF charter, state notification)
   - Designated device list (IMEI, device model, intended operator)
   - Point of contact: named officer with government ID

Step 2: IRIS Root CA team manual verification (3–5 business days)
   - Verify registration against government databases
   - Confirm organization is legitimately disaster-response related
   - Validate designated operator identity
   - Legal review: DPDPA 2023 data handling compliance

Step 3: Root CA key ceremony
   - Performed offline on air-gapped hardware
   - Requires 2-of-3 root CA key custodians present
   - Certificate signed and stored on hardware token (YubiKey 5)

Step 4: Certificate delivery
   - Authority certificate and private key installed on designated device via USB
   - QR code with certificate fingerprint provided for second-factor verification
   - Certificate pinned to specific device NodeId (cannot be transferred)
```

### 3.2 During Disaster (Emergency Fast-Track)

Recognizing that disasters may require rapid onboarding of new responders, IRIS supports an emergency fast-track process:

```
Step 1: Field responder contacts NDRF or regional authority with existing certificate
Step 2: NDRF issues a 72-hour delegated capability token (CaDelegate capability)
Step 3: Delegated token allows NDRF field commander to issue 24-hour tokens to team members
Step 4: All delegated tokens trace back to NDRF root authority
```

Emergency fast-track tokens have constraints:
- Maximum validity: 72 hours
- Geographic constraint: must match declared disaster zone
- No further delegation: `delegation_depth = 0`
- Limited capabilities: typically `CHANNEL_PUBLISH` and `BROADCAST_AUTHORITY` only

### 3.3 Certificate Renewal

Certificates are renewed by the issuing CA at the same verification standard. Renewal is proactive: IRIS alerts the device 14 days before certificate expiry. Renewal requires the original verification evidence to still be valid (organization still registered, same operator named).

---

## 4. Root CA Infrastructure

### 4.1 Root CA Identities

IRIS ships with the following root CA public keys pre-installed:

```rust
// Compiled into iris-core/src/trust/root_cas.rs
pub const ROOT_CA_KEYS: &[RootCaEntry] = &[
    RootCaEntry {
        name: "IRIS-NDRF-Root-CA-v1",
        entity_type: EntityType::NationalEmergencyAgency,
        public_key: include_bytes!("../keys/ndrf_root_ca_v1.pub"),
        valid_from: 1704067200,  // 2024-01-01
        valid_until: 1893456000, // 2030-01-01
        scope: CertScope::National,
    },
    RootCaEntry {
        name: "IRIS-RedCross-India-Root-CA-v1",
        entity_type: EntityType::RedCross,
        public_key: include_bytes!("../keys/redcross_india_root_ca_v1.pub"),
        valid_from: 1704067200,
        valid_until: 1893456000,
        scope: CertScope::National,
    },
    RootCaEntry {
        name: "IRIS-SDRF-Root-CA-v1",
        entity_type: EntityType::StateEmergencyAgency,
        public_key: include_bytes!("../keys/sdrf_root_ca_v1.pub"),
        valid_from: 1704067200,
        valid_until: 1893456000,
        scope: CertScope::State,
    },
];
```

### 4.2 Root CA Key Storage

Root CA private keys are:
- Stored on HSM (Hardware Security Module), never extracted
- Accessible only in physically secured location with 2-person integrity control
- Backup: 5 Shamir Secret Sharing shards, stored in 5 separate geographic locations
- Air-gapped from all networks; signing done on dedicated offline workstation

### 4.3 Root CA Key Rotation

Root CA keys are valid for 6 years (as shown above). Rotation requires:
1. New root CA key pair generated in key ceremony
2. New root CA cert signed by IRIS governance authority
3. IRIS app updated to include new root CA public key
4. 1-year overlap period: both old and new root keys valid
5. All authority certs re-issued under new root CA during overlap period

---

## 5. Certificate Format (CBOR)

Full CBOR structure of an authority certificate (matching the structure in `AUTHORIZATION.md`):

```cbor
; Example: NDRF Gujarat Regional Authority Certificate
{
  1: h'a3b2c1d4e5f6a7b8c9d0e1f2a3b4c5d6',  ; cert_id
  2: h'<32-byte BLAKE3 NodeId of NDRF-GJ-01 device>',
  3: "NDRF-GJ-01 Gujarat Regional Authority",
  4: 0,                                       ; authority_type: NDRF
  5: h'<32-byte Ed25519 public key>',
  6: 1704067200,                              ; issued_at: 2024-01-01
  7: 1711843200,                              ; expires_at: 2024-03-31 (90 days)
  8: h'<NDRF Regional CA cert_id>',           ; parent cert
  9: h'<64-byte Ed25519 signature>',
  10: "India/Gujarat",                        ; geographic_scope
  11: [1, 2, 5]                               ; capabilities: BroadcastAuthority, AuthorityIdentity, ChannelPublish
}
```

Certificates are embedded in the P2 handshake block whenever an authority node initiates contact. Receivers cache the certificate for 24 hours to avoid re-transmission overhead.

---

## 6. Verification UI

### 6.1 What Users See

When a user receives a message from a verified entity, the IRIS UI displays:

**For unverified node:**
```
[Message from Unknown User]
Node: 7f3a...c2b1 (last 4 bytes of NodeId)
⚠ Identity unverified
```

**For verified authority (NDRF):**
```
[OFFICIAL EMERGENCY BROADCAST]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✓ NDRF - Gujarat Regional Authority
  Verified by: IRIS Root CA (NDRF)
  Certificate valid until: 31 March 2024
  Geographic scope: Gujarat, India

MESSAGE: All residents in Kutch district 
proceed to designated assembly points...
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
[Tap to view full certificate]
```

**For unverifiable authority claim (certificate chain broken or expired):**
```
[Message claims to be from NDRF]
⚠ WARNING: AUTHORITY CANNOT BE VERIFIED
  Certificate chain validation failed.
  This message claims authority status but
  cannot be cryptographically verified.
  
  Treat with caution. Contact local 
  emergency services via other means.
```

### 6.2 UI Implementation Notes

The verification badge is computed by the IRIS core library, not the UI layer, to prevent spoofing by custom UI implementations. The core library returns a `MessageVerificationStatus` enum:

```rust
pub enum MessageVerificationStatus {
    Unverified,
    SelfAsserted { node_id: NodeId },
    VerifiedAuthority {
        authority_name: String,
        authority_type: AuthorityType,
        cert_expires_at: SystemTime,
        geographic_scope: Option<String>,
        root_ca_name: String,
    },
    VerificationFailed {
        reason: VerificationFailureReason,
        claimed_name: Option<String>,
    },
}
```

The UI renders different visual treatments for each status. The `VerificationFailed` status always shows a prominent warning, never silently degrades to `Unverified`.

### 6.3 Accessibility

Authority verification indicators use both color AND text/icon (not color alone), complying with WCAG 2.1 AA contrast requirements. In disaster conditions, screens may be viewed in direct sunlight; icons are high-contrast (black on white or white on dark red for warnings).

---

## 7. Privacy Considerations (DPDPA 2023)

Under the Digital Personal Data Protection Act 2023:

- **Authority certificates are NOT personal data** for DPDPA purposes: they belong to organizations, not natural persons.
- **NodeIds are pseudonymous** personal data. Verified entity NodeIds are organizational identifiers.
- **Certificate propagation:** Authority certificates are propagated to all nodes within radio range. This is necessary for offline verification and is analogous to a responder showing their ID badge — accepted under the necessity principle.
- **Revocation bundles** contain the revoked entity's NodeId, which may be pseudonymously traceable. Revocation bundles are P0-priority and must propagate; this is accepted as necessary for safety.

IRIS does not log which nodes have received which authority certificates. The propagation is fire-and-forget at the routing layer.
