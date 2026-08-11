# IRIS Authorization

**Document ID:** IRIS-ARCH-IDENT-001  
**Version:** 1.0  
**Status:** Active  

---

## 1. Overview

Authorization in IRIS controls which operations a node is permitted to perform, beyond simple message authentication. Authentication answers "who is this node?"; authorization answers "what is this node allowed to do?"

IRIS uses a capability-based authorization model. Capabilities are scoped, signed tokens that grant specific permissions to specific nodes. Capabilities are designed for DTN operation: they are self-contained (no lookup required) and time-bounded (short-lived to limit exposure).

---

## 2. Operations Requiring Authorization

Not all IRIS operations require explicit authorization. The following table distinguishes free operations (any authenticated node) from capability-gated operations:

| Operation | Who Can Perform | Authorization Required |
|-----------|----------------|----------------------|
| Send P3–P7 message | Any node | No (authenticated identity sufficient) |
| Receive any message | Any node | No |
| Act as relay (forward bundles) | Any node | No |
| Send P0 SOS | Any node | No — SOS is a fundamental right |
| Send P1 Medical alert | Any node | No — medical data is self-asserted |
| Send P2 Location update | Any node | No |
| **Broadcast emergency instructions** | Authority only | Yes: `BROADCAST_AUTHORITY` capability |
| **Claim NDRF/Red Cross identity** | Authority only | Yes: `AUTHORITY_IDENTITY` capability |
| **Act as gateway node** (priority queue override) | Designated nodes | Yes: `GATEWAY_PRIORITY` capability |
| **Issue capability tokens** | Root CA or delegated CA | Yes: `CA_DELEGATE` capability |
| **Publish to public emergency channel** | Authority or verified user | Yes: `CHANNEL_PUBLISH` capability |
| **Revoke node credentials** | Root CA only | Yes: root CA capability |
| **Modify network-wide config** | IRIS operator | Yes: `NETWORK_ADMIN` capability |

**Design principle:** IRIS does NOT require authorization to send SOS or receive emergency broadcasts. Authorization gates authority actions — telling others what to do — not self-reporting distress.

---

## 3. Capability Tokens

### 3.1 Token Structure (CBOR)

```
CapabilityToken = {
  1: bstr,       ; token_id: 16 random bytes
  2: bstr,       ; holder_node_id: 32-byte BLAKE3 NodeId
  3: [uint],     ; capabilities: list of CapabilityCode
  4: uint,       ; issued_at: unix timestamp
  5: uint,       ; expires_at: unix timestamp (max issued_at + 30 days)
  6: bstr,       ; issuer_node_id: 32-byte NodeId of issuing authority
  7: bstr,       ; issuer_signature: Ed25519 signature over fields 1-6
  ? 8: bstr,     ; delegation_chain: CBOR array of parent CapabilityTokens
  ? 9: {text => any}  ; constraints: scope-limiting key-value pairs
}
```

### 3.2 Capability Codes

```rust
pub enum CapabilityCode {
    BroadcastAuthority = 1,    // Send broadcast messages to all nodes
    AuthorityIdentity  = 2,    // Claim an authority name (NDRF, Red Cross, etc.)
    GatewayPriority    = 3,    // Override queue scheduling as gateway node
    CaDelegate         = 4,    // Issue capability tokens (bounded by issuer's own capabilities)
    ChannelPublish     = 5,    // Post to public emergency channels
    NetworkAdmin       = 6,    // Modify network-wide configuration bundles
    RevokeNode         = 7,    // Issue node revocation bundles
    RelayPriority      = 8,    // Reserve transport bandwidth as designated relay
}
```

### 3.3 Constraints Field

The constraints field limits capability scope:

```
constraints = {
  "geographic_region": "India/Gujarat",         ; limit to geographic region
  "channel_ids": ["emergency-ahmedabad-2024"],  ; limit to specific channels
  "max_broadcast_rate": 10,                      ; max broadcasts per hour
  "valid_authority_names": ["NDRF-GJ-01"],       ; specific authority names
  "delegation_depth": 2,                          ; max further delegation hops
}
```

### 3.4 Token Validity Rules

- `expires_at - issued_at` MUST NOT exceed 30 days (2,592,000 seconds)
- Tokens issued by delegated CAs (depth > 0) MUST NOT exceed their parent token's `expires_at`
- The delegation chain MUST be validated to trace back to a trusted root CA public key
- Tokens are checked against the revocation Bloom filter on every use

---

## 4. Emergency Authority Certificates

Emergency authority certificates are long-lived (90-day max) X.509-equivalent certificates in CBOR format, issued out-of-band by IRIS root CAs to recognized emergency organizations.

### 4.1 Root CA Hierarchy

```
IRIS Root CA (pre-installed in app binary)
├── NDRF Root CA
│   ├── NDRF Regional CA (Gujarat)
│   │   └── NDRF-GJ-01 Authority Certificate
│   └── NDRF Regional CA (Maharashtra)
│       └── NDRF-MH-01 Authority Certificate
├── Red Cross India CA
│   └── Red Cross Gujarat Chapter
│       └── RedCross-GJ-01 Authority Certificate
└── SDRF CA (State Disaster Response Force)
    ├── SDRF Gujarat
    └── SDRF Maharashtra
```

Root CA public keys are bundled with the IRIS application binary at build time. The IRIS build process cryptographically pins these keys using Rust's `include_bytes!` macro at compile time.

### 4.2 Authority Certificate Format

```
AuthorityCertificate = {
  1: bstr,           ; cert_id: 16 random bytes
  2: bstr,           ; subject_node_id: 32-byte NodeId
  3: text,           ; authority_name: human-readable (e.g., "NDRF-GJ-01")
  4: uint,           ; authority_type: NDRF=0, RedCross=1, SDRF=2, Hospital=3, NGO=4
  5: bstr,           ; subject_public_key: Ed25519 public key (32 bytes)
  6: uint,           ; issued_at: unix timestamp
  7: uint,           ; expires_at: unix timestamp (max 90 days)
  8: bstr,           ; issuer_cert_id: parent certificate ID
  9: bstr,           ; issuer_signature: Ed25519 signature over fields 1-8
  ? 10: text,        ; geographic_scope: e.g., "India/Gujarat"
  ? 11: [uint],      ; granted_capabilities: CapabilityCodes
}
```

### 4.3 Certificate Chain Validation

```rust
pub fn validate_authority_certificate(
    cert: &AuthorityCertificate,
    cert_chain: &[AuthorityCertificate],
    root_keys: &RootCaKeySet,
    now: SystemTime,
) -> Result<ValidatedAuthority, AuthError> {
    // 1. Check cert is not expired
    if now > cert.expires_at {
        return Err(AuthError::CertificateExpired);
    }
    
    // 2. Verify signature using parent cert's public key
    let parent = find_parent_cert(cert, cert_chain)?;
    parent.subject_public_key.verify(
        &cert.signing_input(),
        &cert.issuer_signature,
    )?;
    
    // 3. Recursively validate parent up to root
    if root_keys.contains(&parent.subject_public_key) {
        // Parent is a root CA — chain is complete
        Ok(ValidatedAuthority { cert: cert.clone(), depth: cert_chain.len() })
    } else {
        // Recurse: validate parent chain
        validate_authority_certificate(&parent, cert_chain, root_keys, now)
            .map(|_| ValidatedAuthority { cert: cert.clone(), depth: cert_chain.len() })
    }
}
```

---

## 5. Permission Checking at Message Receipt

Every incoming message bundle goes through the authorization gate before being made available to the application layer.

### 5.1 Authorization Flow

```
Bundle received and cryptographically validated
        │
        ▼
 Does operation require capability?
  (check bundle.operation_type against AuthorizationPolicy)
        │
    ┌───┴───┐
   Yes      No
    │        └─► Accept bundle
    ▼
 Extract capability token from bundle extension block
        │
        ▼
 Validate token:
   - Ed25519 signature valid?
   - Token not expired?
   - Token not in revocation filter?
   - holder_node_id matches bundle source NodeId?
   - Capability code present in token.capabilities?
   - Constraints satisfied for this operation?
        │
    ┌───┴───┐
  Valid   Invalid
    │        └─► Reject bundle, emit AuthFailure metric, log
    ▼
 Accept bundle with ValidatedAuthority annotation
```

### 5.2 Authorization Policy (Default)

```rust
pub struct AuthorizationPolicy {
    rules: Vec<AuthRule>,
}

pub struct AuthRule {
    operation: OperationType,
    required_capability: CapabilityCode,
    allow_without_capability: bool,  // for graceful degradation
}

impl Default for AuthorizationPolicy {
    fn default() -> Self {
        Self {
            rules: vec![
                AuthRule {
                    operation: OperationType::BroadcastEmergencyInstruction,
                    required_capability: CapabilityCode::BroadcastAuthority,
                    allow_without_capability: false,
                },
                AuthRule {
                    operation: OperationType::ClaimAuthorityIdentity,
                    required_capability: CapabilityCode::AuthorityIdentity,
                    allow_without_capability: false,
                },
                AuthRule {
                    operation: OperationType::PublishToPublicChannel,
                    required_capability: CapabilityCode::ChannelPublish,
                    allow_without_capability: false,
                },
                // P0-P7 message sending: no capability required
            ],
        }
    }
}
```

---

## 6. Rust Authorizer Trait

```rust
/// The Authorizer trait abstracts authorization checking for testability.
#[async_trait]
pub trait Authorizer: Send + Sync {
    /// Check if the given node is authorized for the operation.
    /// Returns Ok(AuthContext) with validated authority info if authorized.
    async fn authorize(
        &self,
        node_id: NodeId,
        operation: OperationType,
        capability_token: Option<&CapabilityToken>,
        cert_chain: Option<&[AuthorityCertificate]>,
    ) -> Result<AuthContext, AuthError>;
    
    /// Check if a node holds a specific capability (lighter check, no full validation).
    async fn has_capability(
        &self,
        node_id: NodeId,
        capability: CapabilityCode,
    ) -> bool;
}

pub struct AuthContext {
    pub node_id: NodeId,
    pub authority_name: Option<String>,
    pub authority_type: Option<AuthorityType>,
    pub granted_capabilities: Vec<CapabilityCode>,
    pub cert_chain_depth: usize,
}

/// Production implementation using local cert store and revocation filter.
pub struct IrisAuthorizer {
    root_keys: Arc<RootCaKeySet>,
    revocation_filter: Arc<RwLock<RevocationFilter>>,
    policy: AuthorizationPolicy,
    metrics: AuthMetrics,
}

#[async_trait]
impl Authorizer for IrisAuthorizer {
    async fn authorize(
        &self,
        node_id: NodeId,
        operation: OperationType,
        capability_token: Option<&CapabilityToken>,
        cert_chain: Option<&[AuthorityCertificate]>,
    ) -> Result<AuthContext, AuthError> {
        let rule = self.policy.find_rule(operation);
        
        if rule.allow_without_capability {
            return Ok(AuthContext::unauthenticated(node_id));
        }
        
        let token = capability_token.ok_or(AuthError::MissingCapability)?;
        
        // Check revocation
        if self.revocation_filter.read().await.is_revoked(&token.token_id) {
            return Err(AuthError::TokenRevoked);
        }
        
        // Validate capability
        self.validate_token(token, node_id, &rule.required_capability)?;
        
        // Build auth context
        let authority = if let Some(chain) = cert_chain {
            validate_authority_certificate(
                &chain[0], chain, &self.root_keys, SystemTime::now()
            ).ok()
        } else {
            None
        };
        
        Ok(AuthContext::from_token(token, authority))
    }
}
```

---

## 7. Offline Operation Considerations

Authorization checking in IRIS must work with no network connectivity. This shapes all design decisions:

- **Self-contained tokens:** Capability tokens include all necessary information to validate them locally. No CA lookup.
- **Pre-installed root keys:** Root CA public keys are in the app binary. No download required.
- **Short-lived tokens:** 30-day max validity limits the exposure window for compromised tokens without requiring online revocation.
- **Revocation Bloom filter:** Propagated via peer sync like any bundle, so revocations propagate even in disconnected networks (slowly).

The consequence is that a token revoked after issuance will continue to be accepted by nodes that have not yet received the revocation bundle. In a well-connected network, revocation propagates within minutes. In a 72-hour partition, revocation may not propagate to isolated nodes for up to 72 hours. This is an accepted limitation documented in `docs/identity/REVOCATION.md`.
