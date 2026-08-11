# IRIS Credential Revocation

**Document ID:** IRIS-ARCH-IDENT-002  
**Version:** 1.0  
**Status:** Active  

---

## 1. The Revocation Problem in DTN

Traditional credential revocation relies on online infrastructure:

- **Certificate Revocation Lists (CRL):** The verifier downloads a CRL from a CA server and checks if the certificate serial number appears. Requires HTTP access to the CA.
- **OCSP (Online Certificate Status Protocol):** Real-time query to an OCSP responder. Requires TLS connection to the CA's OCSP endpoint.
- **OCSP Stapling:** Server pre-fetches OCSP response. Still requires the server to have internet access.

None of these mechanisms work in IRIS. In a disaster scenario:
- Internet connectivity may be unavailable for hours or days.
- There is no central IRIS server.
- Nodes cannot reliably reach the issuing CA.
- Connectivity is intermittent and partitioned.

This is the fundamental tension: revocation requires "negative information" (this credential is no longer valid) to propagate to all verifiers, but DTN networks have no guaranteed propagation channel.

---

## 2. IRIS Revocation Strategy

IRIS uses a multi-layered approach, accepting that perfect instant revocation is impossible and designing for graceful degradation:

| Layer | Mechanism | Coverage | Latency |
|-------|-----------|----------|---------|
| Primary | Short-lived expiring credentials | All tokens | At issuance |
| Emergency | Emergency revocation bundles (P0) | Specific tokens | Minutes–hours |
| Propagated | Revocation Bloom filter (peer sync) | All revoked tokens | Eventual consistency |
| Last resort | Root CA key rotation | All certs from old root | Requires app update |

---

## 3. Short-Lived Credentials (Primary Mechanism)

The most effective revocation mechanism is not revoking credentials at all — it is ensuring they expire quickly enough that manual revocation is rarely needed.

### 3.1 TTL Limits by Credential Type

| Credential Type | Maximum Validity | Default Validity | Renewal Policy |
|----------------|-----------------|-----------------|----------------|
| Capability token (user-facing) | 30 days | 7 days | Manual re-issuance |
| Emergency capability token | 72 hours | 24 hours | Authority re-issues |
| Authority certificate (org-level) | 90 days | 30 days | CA re-issues |
| Root CA certificate | Indefinite | N/A | App update required |

**Rationale for 30-day max:** In a typical disaster response operation, 30 days covers the full operational period. After 30 days, the credential expires naturally. A compromised credential that cannot be revoked is valid for at most 30 days.

**Rationale for 72-hour emergency tokens:** Emergency operations are time-bounded. A field responder's emergency capability token expires automatically after 72 hours, requiring renewal from the issuing authority. This limits the damage from a stolen field device.

### 3.2 Short-Lived Token Renewal

Before a token expires, the holder must request renewal from the issuing authority node. Renewal is an out-of-band process (direct device-to-device, or via the authority's Satellite/Cellular link). IRIS does not automate renewal — it alerts the user when a token is within 24 hours of expiry.

### 3.3 Clock Skew Handling

Token expiry depends on consistent clocks. IRIS applies a ±30 minute tolerance to expiry checks, using the IRIS distributed time sync protocol. If local time is uncertain (no GPS, no peer sync in 6+ hours), the tolerance is expanded to ±2 hours. Tokens more than 2 hours past their stated expiry are always rejected regardless of clock uncertainty.

---

## 4. Emergency Revocation Bundles

When a credential must be revoked before its natural expiry (device lost, operator compromised, authority revoked), IRIS uses emergency revocation bundles.

### 4.1 Revocation Bundle Format

```
RevocationBundle = {
  1: bstr,       ; revocation_id: 16 random bytes
  2: uint,       ; revocation_type: TOKEN=0, CERT=1, NODE=2
  3: bstr,       ; target_id: token_id or cert_id or node_id being revoked
  4: text,       ; reason: human-readable reason (max 256 chars)
  5: uint,       ; issued_at: unix timestamp
  6: uint,       ; valid_until: timestamp (revocation itself expires — bundle TTL)
  7: bstr,       ; issuer_node_id: NodeId of revoking authority
  8: bstr,       ; issuer_cert_id: the authority cert used to sign this revocation
  9: bstr,       ; issuer_signature: Ed25519 signature over fields 1-8
}
```

### 4.2 Revocation Bundle Routing

Emergency revocation bundles are transmitted at **P0 priority**. This means:
- They are transmitted immediately on all available transports (multipath)
- They preempt all other queued transmissions except other P0 bundles
- They are never evicted from the store
- They use Epidemic routing to maximize propagation

A node receiving a revocation bundle:
1. Validates the issuer's authority certificate (must have `RevokeNode` capability or be a root CA)
2. Adds the revoked `target_id` to the local revocation Bloom filter
3. Persists the revocation entry to disk
4. Forwards the revocation bundle via Epidemic routing (regardless of duplicate status — the bundle's delivery is too important to skip)

### 4.3 Revocation Bundle Retention

Revocation bundles are retained in the bundle store for the duration of the revoked credential's original validity period. If a token was valid until 2024-03-01, the revocation bundle is retained until 2024-03-01, ensuring that any node receiving a stale copy of the token also eventually receives the revocation.

After the original credential has expired, the revocation is no longer needed and the bundle is deleted.

---

## 5. Revocation Bloom Filter

The revocation Bloom filter is a compact representation of all known revoked token IDs, propagated via peer synchronization.

### 5.1 Filter Parameters

```
Filter size (m):  65,536 bits (8 KB)
Hash functions (k): 5
Target false-positive rate: < 0.5% at 10,000 revoked tokens
Expected revocation rate: < 100 per month in normal operations
Bloom filter capacity before 1% FP: ~15,000 entries
```

At 8 KB, the revocation filter fits comfortably in a single BLE advertisement extension block, enabling passive propagation during any peer contact.

### 5.2 Filter Propagation

The revocation Bloom filter is exchanged during the bundle exchange handshake that occurs at the start of every peer contact:

```rust
pub struct PeerSyncHandshake {
    pub node_id: NodeId,
    pub bundle_id_bloom: Bloom,          // bundles I have (for dedup)
    pub revocation_filter: RevocationBloom, // tokens I know are revoked
    pub protocol_version: u8,
    pub signature: Ed25519Signature,
}
```

On receiving a peer's revocation filter, the local node computes the union (OR of bit arrays) with its own filter. This ensures revocations propagate as fast as peer contacts occur.

### 5.3 Filter Aging

Bloom filters cannot have entries removed. IRIS uses a time-partitioned filter to handle filter aging:

```rust
pub struct RevocationFilter {
    active: BloomFilter,     // revocations added in last 30 days
    archive: BloomFilter,    // revocations from 30-60 days ago
    generation: u32,
    last_rotation: SystemTime,
}

impl RevocationFilter {
    pub fn is_revoked(&self, token_id: &[u8]) -> bool {
        self.active.contains(token_id) || self.archive.contains(token_id)
    }
    
    pub fn rotate_if_needed(&mut self) {
        if self.last_rotation.elapsed().unwrap() > Duration::from_secs(30 * 86400) {
            self.archive = std::mem::take(&mut self.active);
            self.active = BloomFilter::new();
            self.generation += 1;
            self.last_rotation = SystemTime::now();
        }
    }
}
```

After 60 days, revocations fall out of the filter. This is acceptable because credentials have a maximum 30-day validity — by 60 days, all credentials that were revoked have also naturally expired.

---

## 6. Node-Level Revocation

In extreme cases (a device is confirmed stolen and being used maliciously), a root CA can issue a node-level revocation. This revokes all tokens associated with a NodeId, not just a specific token.

```rust
pub enum RevocationType {
    Token(TokenId),         // single token revoked
    Certificate(CertId),   // authority cert revoked (all capabilities from this cert)
    Node(NodeId),           // all capabilities from this node revoked
}
```

Node-level revocation is the nuclear option: it adds the NodeId to the revocation filter. Nodes receiving subsequent messages from the revoked NodeId will reject their capability tokens (though non-capability messages like P0 SOS are still accepted — even a revoked node can send an SOS).

---

## 7. Known Limitations

### 7.1 Propagation Delay in Partitioned Networks

A revocation issued while a network partition exists will not reach nodes in the partitioned segment until the partition heals. During a 72-hour partition, a revoked credential may remain valid for up to 72 hours in the isolated segment.

**Accepted risk:** This is a fundamental DTN limitation. Mitigated by short-lived credentials (max 30 days) ensuring the damage window is bounded.

### 7.2 Bloom Filter False Positives

The revocation Bloom filter has a < 0.5% false positive rate. This means 0.5% of valid tokens may be incorrectly flagged as revoked and rejected. For P3–P7 operations, this is mildly disruptive. For P0 SOS (which does not require capability tokens), there is no impact.

**Accepted risk:** 0.5% erroneous rejections are preferable to zero revocation capability.

### 7.3 Cannot Revoke Root CA Keys Without App Update

Root CA public keys are compiled into the application binary. If a root CA key is compromised, revocation requires publishing an updated IRIS application version. This is equivalent to the standard PKI practice of key ceremony and root CA rotation, but in IRIS requires end-user app updates.

**Mitigation:** Root CA key material is stored offline (air-gapped HSM). Root CA key ceremonies require 3-of-5 quorum of key custodians.

### 7.4 Revocation After Credential Use

If a node uses a capability token to send an emergency broadcast, and the token is later revoked, the broadcast has already been sent and may have already propagated. IRIS cannot un-send a message or recall propagated content. Revocation prevents future use of the token, not retroactive invalidation of past actions.

---

## 8. Revocation API

```rust
/// Check if a token is revoked. Used by the Authorizer.
pub async fn is_revoked(
    filter: &RevocationFilter,
    token_id: &TokenId,
) -> bool;

/// Issue a revocation (requires CA capability).
pub async fn issue_revocation(
    ca_key: &Ed25519PrivateKey,
    ca_cert: &AuthorityCertificate,
    revocation: RevocationTarget,
    reason: &str,
) -> Result<RevocationBundle, RevocationError>;

/// Apply a received revocation bundle to local filter.
pub async fn apply_revocation(
    filter: &mut RevocationFilter,
    bundle: &RevocationBundle,
    root_keys: &RootCaKeySet,
) -> Result<(), RevocationError>;

/// Get revocation filter for peer sync handshake.
pub fn export_filter(filter: &RevocationFilter) -> RevocationBloom;

/// Merge peer's revocation filter into ours.
pub fn merge_filter(local: &mut RevocationFilter, peer: &RevocationBloom);
```
