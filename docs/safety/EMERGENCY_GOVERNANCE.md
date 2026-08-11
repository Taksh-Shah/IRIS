# Emergency System Governance

## Overview

The IRIS emergency system has elevated privileges: it bypasses rate limits, uses reserved
storage, and can trigger mandatory UI alerts on all receiving devices. These privileges
require a governance structure that ensures they are used correctly, by authorized parties,
for genuine emergencies. This document defines that structure.

## Emergency Authority Hierarchy

The IRIS emergency authority hierarchy mirrors India's disaster management administrative
structure, as defined by the Disaster Management Act, 2005.

```
National Emergency Authority
  (National Disaster Management Authority - NDMA)
  ├── Issues certificates to state authorities
  ├── Can activate emergency for entire country
  ├── Root certificate preloaded on all IRIS nodes
  └── Certificate validity: 2 years

  State Emergency Authority (28 states + 8 UTs)
  (State Disaster Management Authority - SDMA)
  ├── Certificate issued by National Authority
  ├── Can activate emergency for their state
  ├── Scope: state code (e.g., "IN-GJ" for Gujarat)
  └── Certificate validity: 1 year

    District Emergency Authority
    (District Disaster Management Authority - DDMA)
    ├── Certificate issued by State Authority
    ├── Can activate emergency for their district
    ├── Scope: district code (e.g., "IN-GJ-19" for Kutch)
    └── Certificate validity: 6 months

      Local Emergency Authority
      (Municipality / Tehsil / Panchayat)
      ├── Certificate issued by District Authority
      ├── Can activate emergency for local area
      ├── Scope: local area code
      └── Certificate validity: 3 months
```

Specialized authority certificates also exist for:
- **Medical Emergency**: hospitals, medical coordinators (scope: medical broadcasts)
- **Search and Rescue**: NDRF/SDRF teams (scope: SAR coordination only)
- **Fire Services**: fire brigade coordination (scope: fire emergency)

## Certificate Format

Authority certificates use an X.509-inspired format, serialized as CBOR for compact encoding.

```rust
pub struct AuthorityCertificate {
    // Identity
    pub version: u8,             // Certificate format version (currently 1)
    pub serial: [u8; 16],        // Unique serial number (UUID)
    pub issuer_id: NodeId,       // Issuing authority's NodeId
    pub subject_id: NodeId,      // Certified authority's NodeId
    pub subject_name: String,    // Human-readable name (e.g., "Gujarat SDMA")
    
    // Validity
    pub valid_from: i64,         // Unix timestamp
    pub valid_until: i64,        // Unix timestamp
    
    // Scope
    pub geographic_scope: GeographicScope,
    pub functional_scope: FunctionalScope,
    
    // Permissions
    pub permissions: AuthorityPermissions,
    
    // Chain
    pub issuer_certificate: Option<Box<AuthorityCertificate>>,  // Parent cert embedded
    
    // Signature
    pub signature: Ed25519Signature,  // Signed by issuer's private key
}

pub enum GeographicScope {
    Country { country_code: String },
    State { state_code: String },
    District { district_code: String },
    Local { area_code: String },
    Unrestricted,
}

pub enum FunctionalScope {
    Emergency,          // General emergency
    MedicalEmergency,  // Medical broadcasts only
    SearchAndRescue,   // SAR coordination
    FireEmergency,     // Fire services
}

pub struct AuthorityPermissions {
    pub can_broadcast: bool,          // Send emergency broadcast to all nodes
    pub can_activate_emergency_mode: bool,  // Trigger IRIS emergency mode
    pub can_issue_subcertificates: bool,    // Issue certs to lower authorities
    pub can_revoke_subcertificates: bool,   // Revoke certs they issued
    pub sos_relay_priority: bool,      // Additional SOS routing priority
    pub broadcast_max_severity: BroadcastSeverity,
}
```

## Certificate Issuance Process

### Offline Issuance

Certificate issuance requires NO Internet connection. The process:

1. **Authority requests certificate** from parent authority
   - Generates Ed25519 key pair on dedicated signing device
   - Exports public key to parent authority (QR code, USB, NFC)

2. **Parent authority issues certificate**
   - Verifies requestor's identity through physical/official process
   - Creates certificate with appropriate scope and permissions
   - Signs with parent's private key (on air-gapped signing device)
   - Returns signed certificate (QR code, USB, NFC)

3. **Certificate loading**
   - Authority loads certificate onto their IRIS device(s)
   - IRIS verifies certificate chain before accepting
   - Certificate becomes active from `valid_from` date

### Certificate Distribution

Authority certificates are distributed to field devices:
- **Pre-event**: distributed before disaster season (via official NDMA/SDMA channels)
- **At deployment**: loaded via QR code scan when teams deploy
- **Via mesh**: new certificates can propagate as signed P1 messages when Internet is unavailable

Root certificate (NDMA) is hardcoded in IRIS firmware. Cannot be updated without
an app/firmware update — provides strong trust anchor.

## Offline Certificate Verification

Every relay node can verify certificate chains without Internet connectivity:

```rust
pub fn verify_certificate_chain(
    cert: &AuthorityCertificate,
    root_store: &RootCertificateStore,
    current_time: i64,
    claimed_area: &AreaCode,
) -> Result<VerifiedAuthority, CertError> {
    // 1. Check expiry
    if current_time < cert.valid_from || current_time > cert.valid_until {
        return Err(CertError::Expired);
    }
    
    // 2. Check geographic scope
    if !cert.geographic_scope.contains(claimed_area) {
        return Err(CertError::OutOfScope);
    }
    
    // 3. Verify issuer signature
    let issuer_cert = cert.issuer_certificate.as_ref()
        .ok_or(CertError::MissingIssuer)?;
    
    verify_ed25519_signature(
        &issuer_cert.subject_id.to_public_key()?,
        &cert.canonical_signed_bytes(),
        &cert.signature,
    )?;
    
    // 4. Verify issuer has permission to issue sub-certificates
    if !issuer_cert.permissions.can_issue_subcertificates {
        return Err(CertError::IssuerCannotDelegate);
    }
    
    // 5. Verify chain up to root
    if root_store.contains(&issuer_cert.subject_id) {
        // Chain terminates at trusted root
        Ok(VerifiedAuthority { cert: cert.clone(), chain_depth: 1 })
    } else {
        // Recurse up the chain
        let parent = verify_certificate_chain(
            issuer_cert, root_store, current_time, claimed_area
        )?;
        Ok(VerifiedAuthority { cert: cert.clone(), chain_depth: parent.chain_depth + 1 })
    }
}
```

Maximum chain depth: 4 (National → State → District → Local). Longer chains rejected.

## Emergency Authority Activation

When an authority activates emergency mode:

1. Authority sends signed EMERGENCY_ACTIVATE message to mesh
2. Every relay verifies certificate and activates emergency mode locally
3. Emergency mode propagates via gossip (see DISASTER_MODES.md)
4. All nodes in geographic scope enter emergency mode within minutes

Activation scope is enforced: a district authority cannot activate emergency for
neighboring districts or statewide.

## Post-Incident Audit

After any emergency, metadata is logged for accountability:

```rust
pub struct EmergencyAuditRecord {
    pub event_type: EmergencyEventType,  // Broadcast, Activate, Deactivate, Cancel
    pub authority_id_hash: [u8; 16],     // Hash of authority NodeId (pseudonymous)
    pub area_code: String,
    pub severity: BroadcastSeverity,
    pub timestamp: i64,
    pub broadcast_id: Option<[u8; 16]>,
    pub message_type: Option<EmergencyMessageType>,
    pub was_cancelled: bool,
    pub cancel_timestamp: Option<i64>,
}
```

Audit records retained for 1 year. Accessible to:
- The certifying authority (who issued the certificate used)
- NDMA audit team (with proper authorization)
- NOT accessible to general IRIS users or team without authorization

## Certificate Revocation

### Revocation Process

If an authority misuses their certificate:
1. Parent authority creates signed REVOKE_CERTIFICATE message
2. Revocation message is broadcast via IRIS mesh (P1 priority) and Internet
3. Every relay caches the revocation in its local revocation list
4. Future messages signed with revoked certificate are rejected

### Revocation List

```rust
pub struct RevocationList {
    pub entries: Vec<RevocationEntry>,
    pub issued_at: i64,
    pub issuer_id: NodeId,
    pub signature: Ed25519Signature,
}

pub struct RevocationEntry {
    pub serial: [u8; 16],       // Certificate serial number
    pub revoked_at: i64,        // When revoked
    pub reason: RevocationReason,
}
```

Revocation list signed by the issuing authority. Revocation lists are versioned and
timestamped. Nodes always apply the most recent revocation list they have seen.

**Revocation distribution lag**: nodes offline at time of revocation may not receive
it immediately. A revocation message propagates through the mesh; offline nodes apply
it upon reconnection. This means a revoked certificate may still work in an isolated
partition for up to the duration of isolation. Acceptable risk — physical isolation
is also a limiting factor for the attacker.
