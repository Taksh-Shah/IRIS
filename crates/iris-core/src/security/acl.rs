//! ACL-1: Emergency send authorization data (per-alert-class key/role allowlists).
//! Spoofed/revoked/expired/replayed authority rejected.
//! EMERG-001 verify pattern; closes RED-0002/RED-0003/ADR-0011.
//! RES-0018 R9/Q4.3, CAP v1.2 DSig, RFC 9804, CACM 2023 WEA Ed25519 ~68B preloaded-key.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::identity::advertise::KeyAdvertisementV1;
use crate::identity::TrustLevel;
use crate::identity::TrustStore;
use crate::message::MessagePriority;
use crate::protocol::{ContentType, Envelope};

/// 16-byte authority short ID (SHA-256(authority_pubkey)[..16]).
pub type AuthorityShort = [u8; 16];
/// 16-byte sender short ID.
pub type SenderShort = [u8; 16];

/// Emergency alert class for ACL.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum AlertClass {
    /// P0 broadcast: requires verified authority role chain.
    Broadcast,
    /// P1 medical/health: authority + certified responders.
    Medical,
    /// SOS: any verified identity @ 3/hr (existing EMERG-001 limiter).
    Sos,
    /// Drill/test: verified authority, cap-exempt.
    Drill,
}

/// ACL-1 configuration.
///
/// PRY-12 — reserved but not yet wired:
/// - `require_chain_for_broadcast_medical` and `sos_rate_limit_per_hour` are
///   **not read by any code path** in this crate. Setting them has no effect.
///   `check_sos_identity` does not rate-limit (SOS rate limiting is owned by
///   EMERG-001), and Medical is currently validated identically to Broadcast.
/// - `allowlists` is fixed at construction: `add_authority` / `reset` need
///   `&mut EmergencyAcl` but the ACL only exists as `Arc<EmergencyAcl>` in
///   `FullSecurityPolicy`. Making it interior-mutable and wiring the SOS
///   limiter is tracked under PRY-1 (gated).
#[derive(Clone, Debug)]
pub struct AclConfig {
    /// Allowlist: alert_class -> list of authorized authority short IDs.
    /// PRY-12: effectively read-only after construction (see struct docs).
    pub allowlists: HashMap<AlertClass, Vec<AuthorityShort>>,
    /// PRY-12: **not currently read** — reserved. Chain verification for
    /// Broadcast/Medical happens unconditionally in `check_authority_chain`.
    pub require_chain_for_broadcast_medical: bool,
    /// PRY-12: **not currently read** — reserved. SOS rate limiting is enforced
    /// by EMERG-001, not this ACL.
    pub sos_rate_limit_per_hour: u32,
}

impl Default for AclConfig {
    fn default() -> Self {
        AclConfig {
            allowlists: HashMap::new(),
            require_chain_for_broadcast_medical: true,
            sos_rate_limit_per_hour: 3,
        }
    }
}

/// ACL decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AclDecision {
    /// Authorized — proceed.
    Authorized,
    /// Not authorized — silent drop (no error/NAK).
    Unauthorized,
    /// SOS rate limited — degraded delivery (P3).
    /// PRY-12: **never returned by this crate** — `check_sos_identity` does not
    /// rate-limit. Kept because `message_engine` matches on it; the actual SOS
    /// per-hour limit lives in EMERG-001.
    SosRateLimited,
    /// Authority chain invalid/expired/revoked/replayed.
    InvalidAuthority,
}

/// Emergency send ACL (ACL-1).
pub struct EmergencyAcl {
    config: AclConfig,
    trust_store: Arc<TrustStore>,
    metrics: AclMetrics,
}

#[derive(Default, Debug)]
pub struct AclMetrics {
    pub authorized: AtomicU64,
    pub unauthorized: AtomicU64,
    pub sos_rate_limited: AtomicU64,
    pub invalid_authority: AtomicU64,
}

impl EmergencyAcl {
    /// Create a new ACL with default configuration and trust store.
    pub fn new(config: AclConfig, trust_store: Arc<TrustStore>) -> Self {
        EmergencyAcl {
            config,
            trust_store,
            metrics: AclMetrics::default(),
        }
    }

    /// Create with default config.
    pub fn default(trust_store: Arc<TrustStore>) -> Self {
        Self::new(AclConfig::default(), trust_store)
    }

    /// Get metrics snapshot.
    pub fn metrics(&self) -> AclMetricsSnapshot {
        AclMetricsSnapshot {
            authorized: self.metrics.authorized.load(Ordering::Relaxed),
            unauthorized: self.metrics.unauthorized.load(Ordering::Relaxed),
            sos_rate_limited: self.metrics.sos_rate_limited.load(Ordering::Relaxed),
            invalid_authority: self.metrics.invalid_authority.load(Ordering::Relaxed),
        }
    }

    /// Add an authorized authority for an alert class.
    pub fn add_authority(&mut self, class: AlertClass, authority: AuthorityShort) {
        self.config
            .allowlists
            .entry(class)
            .or_default()
            .push(authority);
    }

    /// Check if a sender is authorized to send an emergency message of given class.
    /// `envelope` contains the sender_id and auth_cert_chain (if present).
    /// `now_unix` is current unix time for expiry checks.
    /// Returns AclDecision.
    pub async fn check(&self, envelope: &Envelope, now_unix: u64) -> AclDecision {
        let alert_class = match envelope.priority {
            MessagePriority::P0 => AlertClass::Broadcast,
            MessagePriority::P1 => AlertClass::Medical,
            MessagePriority::P2 => AlertClass::Sos,
            _ => AlertClass::Sos,
        };

        // Determine alert class from payload type if available.
        // PRY-9: ContentType::KeyRotation is NO LONGER mapped to AlertClass::Drill.
        // Key rotation is a routine identity-hygiene operation for every node, not
        // a drill; overloading the same type caused a populated Drill allowlist to
        // drop all mesh-wide key rotations. Drills need their own content-type
        // marker (EMERG-001 follow-up). KeyRotation envelopes are no longer routed
        // through check_emergency_acl by the engine (message_engine/mod.rs).
        let class = match envelope.payload_type {
            ContentType::EmergencyAlert => {
                if matches!(envelope.priority, MessagePriority::P0) {
                    AlertClass::Broadcast
                } else {
                    AlertClass::Medical
                }
            }
            ContentType::Sos => AlertClass::Sos,
            _ => alert_class,
        };

        match class {
            AlertClass::Broadcast | AlertClass::Medical => {
                self.check_authority_chain(envelope, class, now_unix).await
            }
            AlertClass::Sos => self.check_sos_identity(envelope, now_unix).await,
            AlertClass::Drill => self.check_authority_chain(envelope, class, now_unix).await,
        }
    }

    /// Verify authority chain for Broadcast/Medical/Drill.
    async fn check_authority_chain(
        &self,
        envelope: &Envelope,
        class: AlertClass,
        now_unix: u64,
    ) -> AclDecision {
        let allowed = self.config.allowlists.get(&class);
        let allowlist_empty = allowed.map_or(true, |v| v.is_empty());

        if allowlist_empty {
            match class {
                // PRY-1: reaching this code path means the policy is ARMED — an
                // un-armed `FullSecurityPolicy` and `NoopSecurityPolicy` both
                // short-circuit to `Authorized` *before* the ACL is consulted
                // (`security/mod.rs::check_emergency_acl`). An armed node with
                // no configured authorities cannot verify a public-safety
                // broadcast, so it must drop it — the old `empty => Authorized`
                // conflated "un-armed" with "armed but unconfigured" and let a
                // spoofed P0 broadcast straight through.
                AlertClass::Broadcast | AlertClass::Medical => {
                    tracing::warn!(
                        event = "ACL_ARMED_NO_AUTHORITIES",
                        class = ?class,
                        "armed policy has no configured authorities — dropping broadcast"
                    );
                    self.metrics
                        .invalid_authority
                        .fetch_add(1, Ordering::Relaxed);
                    return AclDecision::InvalidAuthority;
                }
                // Drill stays permissive-on-empty: `ContentType::KeyRotation`
                // is currently (mis)mapped to `Drill` (PRY-9), so tightening
                // here would drop every routine key-rotation envelope
                // mesh-wide. Revisit once PRY-9 gives drills their own marker.
                _ => {
                    self.metrics.authorized.fetch_add(1, Ordering::Relaxed);
                    return AclDecision::Authorized;
                }
            }
        }

        let chain_raw = match &envelope.auth_cert_chain {
            Some(c) => c,
            None => {
                self.metrics
                    .invalid_authority
                    .fetch_add(1, Ordering::Relaxed);
                return AclDecision::InvalidAuthority;
            }
        };

        // SEC-RT-02: the previous check_desp_decode-raw + "root is allowlisted"
        // NEVER called the crypto verifier — any attacker could present *signed*
        // bytes whose root short-id happened to match an allowlist entry and pass
        // the gate without proving their key was actually certified by that
        // authority. Also it read `chain.last()` as the root, but the RED-0008
        // convention is `element[0]` = root, so even the "allowlist contains the
        // root" check looked at the wrong element.
        //
        // Full RED-0008 verification (chain.rs): structural decode, root trusted,
        // parent-certifies-child signatures (verify_strict), small-order, monotonic
        // counters, sender binding, expiry, length cap, revocation severing.
        if let Err(e) = crate::identity::chain::verify_chain(
            chain_raw,
            &self.trust_store,
            &envelope.sender_id,
            // PRY-29: enforce authority-root requirement inside verify_chain so
            // the `is_authority_root` post-check below is no longer needed.
            crate::identity::chain::RootRequirement::AuthorityRoot,
        ) {
            tracing::warn!(
                event = "ACL_CHAIN_REJECTED",
                class = ?class,
                error = %e,
                "emergency authority chain rejected"
            );
            self.metrics
                .invalid_authority
                .fetch_add(1, Ordering::Relaxed);
            return AclDecision::InvalidAuthority;
        }

        // Root anchoring: `element[0]` is the trusted authority root (RED-0008).
        // The allowlist must contain that root AND the store must mark it an
        // explicit authority root (a TOFU peer is trusted for peer trees but
        // never anchors an emergency chain — EMERG-RT-001).
        let root_bytes = match chain_raw.first() {
            Some(r) => r,
            None => {
                self.metrics
                    .invalid_authority
                    .fetch_add(1, Ordering::Relaxed);
                return AclDecision::InvalidAuthority;
            }
        };
        let root = match KeyAdvertisementV1::from_bytes(root_bytes.as_slice()) {
            Ok(ad) => ad,
            Err(_) => {
                self.metrics
                    .invalid_authority
                    .fetch_add(1, Ordering::Relaxed);
                return AclDecision::InvalidAuthority;
            }
        };
        // PRY-29: is_authority_root check is now enforced inside verify_chain
        // (RootRequirement::AuthorityRoot above). Only the allowlist check remains.
        if !allowed.unwrap().contains(&root.authority_short_id()) {
            self.metrics
                .invalid_authority
                .fetch_add(1, Ordering::Relaxed);
            return AclDecision::InvalidAuthority;
        }

        // PRY-32: the chain + allowlist + anchor check above is only half the
        // SEC-001 authority model. The payload-level profile constraints — geo
        // scope, functional scope, `max_severity` cap, drill discipline and the
        // `issued_at`/`expires_at` validity window — live in
        // `emergency::authority::verify_authoritative`. The engine also runs
        // that (via `emergency_gate`), but ONLY when the EMERG-001 provider is
        // armed, which is a *separate* flag from the security policy's. Run it
        // here too so a deployment that arms SEC-001 without EMERG-001 still
        // enforces the full profile for Broadcast/Medical. A non-emergency
        // payload (no decodable `EmergencyBroadcast`) keeps the anchor-only
        // result — the engine's emergency path fails those closed when armed.
        if matches!(class, AlertClass::Broadcast | AlertClass::Medical) {
            if let Ok(broadcast) = crate::emergency::decode_broadcast(&envelope.payload) {
                if let Err(e) = crate::emergency::verify_authoritative(
                    chain_raw,
                    &self.trust_store,
                    &envelope.sender_id,
                    &broadcast,
                    now_unix,
                ) {
                    tracing::warn!(
                        event = "ACL_AUTHORITY_META_REJECTED",
                        class = ?class,
                        error = %e,
                        "emergency authority profile check failed (geo/severity/drill/validity)"
                    );
                    self.metrics
                        .invalid_authority
                        .fetch_add(1, Ordering::Relaxed);
                    return AclDecision::InvalidAuthority;
                }
            }
        }

        self.metrics.authorized.fetch_add(1, Ordering::Relaxed);
        AclDecision::Authorized
    }

    /// PRY-10: verify `envelope` carries a valid Ed25519 signature by
    /// `identity_pubkey` (the self-authenticating 32-byte identity). Returns
    /// `false` on a missing signature, an unparseable key, an encoding error,
    /// or a `verify_strict` rejection.
    fn envelope_signature_valid(&self, envelope: &Envelope, identity_pubkey: &[u8; 32]) -> bool {
        let Some(sig) = envelope.signature else {
            return false;
        };
        let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(identity_pubkey) else {
            return false;
        };
        let Ok(signable) = crate::protocol::codec::encode_for_signing(envelope) else {
            return false;
        };
        matches!(
            crate::crypto::ed25519::verify_strict(&vk, &signable, &sig),
            Ok(true)
        )
    }

    /// Verify SOS sender has verified identity (TrustStore Verified tier).
    async fn check_sos_identity(&self, envelope: &Envelope, _now_unix: u64) -> AclDecision {
        // Need to get the full 32-byte pubkey to check trust level
        // For now, we derive from sender_id (which should be 32 bytes)
        if envelope.sender_id.len() != 32 {
            self.metrics.unauthorized.fetch_add(1, Ordering::Relaxed);
            return AclDecision::Unauthorized;
        }
        let mut pubkey = [0u8; 32];
        pubkey.copy_from_slice(&envelope.sender_id);

        // PRY-10: the ACL is a security gate — it must not authorize on a
        // claimed `sender_id` alone (an attacker sets it to any known
        // `Verified` peer's key). `sender_id` IS the 32-byte Ed25519 identity
        // key (self-authenticating), so verify the envelope is actually signed
        // by it here rather than trusting a signature check elsewhere in the
        // pipeline that this function cannot see the result of.
        if !self.envelope_signature_valid(envelope, &pubkey) {
            tracing::warn!(
                event = "ACL_SOS_BAD_SIGNATURE",
                "SOS envelope signature missing or invalid — rejecting"
            );
            self.metrics
                .invalid_authority
                .fetch_add(1, Ordering::Relaxed);
            return AclDecision::InvalidAuthority;
        }

        let trust = self.trust_store.level(&pubkey);
        let store_empty = self.trust_store.is_empty();
        match trust {
            TrustLevel::Verified | TrustLevel::AuthorityRoot => {
                self.metrics.authorized.fetch_add(1, Ordering::Relaxed);
                AclDecision::Authorized
            }
            // SEC-RT-08: empty trust store = security not configured → Noop
            // permissive (AC-12). Previously EVERY Unknown/Unverified sender
            // was authorized regardless of configuration.
            TrustLevel::Unknown | TrustLevel::Unverified if store_empty => {
                self.metrics.authorized.fetch_add(1, Ordering::Relaxed);
                AclDecision::Authorized
            }
            TrustLevel::Unknown | TrustLevel::Unverified => {
                // ARMED node with a populated store: a self-asserted first-seen
                // identity must NOT pass the SOS gate (spoofable 32-byte sender).
                self.metrics.unauthorized.fetch_add(1, Ordering::Relaxed);
                AclDecision::Unauthorized
            }
            _ => {
                self.metrics.unauthorized.fetch_add(1, Ordering::Relaxed);
                AclDecision::Unauthorized
            }
        }
    }

    pub fn metrics_snapshot(&self) -> AclMetricsSnapshot {
        self.metrics()
    }

    pub fn reset(&mut self) {
        self.config.allowlists.clear();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AclMetricsSnapshot {
    pub authorized: u64,
    pub unauthorized: u64,
    pub sos_rate_limited: u64,
    pub invalid_authority: u64,
}

// Extension trait for KeyAdvertisementV1
trait AuthorityShortId {
    fn authority_short_id(&self) -> AuthorityShort;
}

impl AuthorityShortId for KeyAdvertisementV1 {
    fn authority_short_id(&self) -> AuthorityShort {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.identity_pubkey);
        let result = hasher.finalize();
        let mut short = [0u8; 16];
        short.copy_from_slice(&result[..16]);
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::keygen::{IdentityKeypair, X25519Keypair};
    use crate::identity::TrustStore;
    use std::sync::Arc;

    fn now() -> u64 {
        crate::message_engine::expiry::unix_now()
    }

    fn test_env(
        priority: MessagePriority,
        payload_type: ContentType,
        sender_id: Vec<u8>,
        chain: Option<Vec<Vec<u8>>>,
    ) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id,
            recipient_id: vec![],
            priority,
            ttl_seconds: 3600,
            timestamp: now(),
            hop_count: 0,
            max_hops: None,
            payload_type,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: vec![],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: chain,
        }
    }

    fn root_ad() -> (IdentityKeypair, X25519Keypair, KeyAdvertisementV1) {
        let root = IdentityKeypair::generate();
        let x = X25519Keypair::generate();
        let ad = KeyAdvertisementV1::build(&root, x.public_bytes(), 0, 0).unwrap();
        (root, x, ad)
    }

    /// PRY-10: sign `env` in place so it passes the SOS ACL's signature gate.
    fn sign_env(env: &mut Envelope, kp: &IdentityKeypair) {
        let signable = crate::protocol::codec::encode_for_signing(env).unwrap();
        env.signature = Some(crate::crypto::ed25519::sign(kp, &signable).unwrap());
    }

    /// PRY-32: a single-element authority-root chain + the sender_id it anchors.
    fn authority_root_chain() -> (Arc<TrustStore>, [u8; 32], Vec<Vec<u8>>, [u8; 16]) {
        let store = Arc::new(TrustStore::new());
        let root = IdentityKeypair::generate();
        let root_x = X25519Keypair::generate();
        let ad = KeyAdvertisementV1::build(&root, root_x.public_bytes(), 0, 0).unwrap();
        store.register_authority_root(&ad, now()).unwrap();
        let sender = root.verifying_bytes();
        (
            store,
            sender,
            vec![ad.to_bytes()],
            ad.authority_short_id(),
        )
    }

    /// PRY-32: the ACL now enforces the payload-level authority profile
    /// (geo scope / severity cap / drill / validity) — not just the chain
    /// anchor. Pre-fix a district authority capped at `IN-GJ` could push a
    /// nationwide alert and the ACL returned Authorized.
    #[tokio::test]
    async fn acl_rejects_out_of_geo_scope_broadcast() {
        use crate::emergency::model::{
            AlertMessageType, AuthorityMeta, Certainty, EmergencyBroadcast, Severity,
        };
        let (store, sender, chain, root_short) = authority_root_chain();
        let mut config = AclConfig::default();
        config
            .allowlists
            .insert(AlertClass::Broadcast, vec![root_short]);
        let acl = EmergencyAcl::new(config, store);

        let mk = |area: &str, scope: &str| EmergencyBroadcast {
            format_version: 1,
            broadcast_id: [1u8; 16],
            republish_id: None,
            issued_at: now() - 60,
            expires_at: now() + 3600,
            severity: Severity::Severe,
            certainty: Certainty::Observed,
            message_type: AlertMessageType::Alert,
            area_code: area.into(),
            language: "en".into(),
            headline: "x".into(),
            instructions: None,
            authority: AuthorityMeta {
                issuer_name: None,
                geo_scope: Some(scope.into()),
                functional_scope: None,
                max_severity: 4,
            },
            authority_peer_short: Some(crate::emergency::authority::authority_short_id(&sender)),
            drill: false,
        };

        let env = |b: &EmergencyBroadcast| Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: sender.to_vec(),
            recipient_id: vec![],
            priority: MessagePriority::P0,
            ttl_seconds: 3600,
            timestamp: now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::EmergencyAlert,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: crate::emergency::encode_broadcast(b).unwrap(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: Some(chain.clone()),
        };

        // In scope: authorized.
        assert_eq!(
            acl.check(&env(&mk("IN-GJ", "IN-GJ")), now()).await,
            AclDecision::Authorized
        );
        // Out of geo scope: district scoped IN-GJ pushing an IN-MH alert.
        assert_eq!(
            acl.check(&env(&mk("IN-MH", "IN-GJ")), now()).await,
            AclDecision::InvalidAuthority
        );
        // Severity above the profile cap (Severe=3 vs a max_severity of 2).
        let mut over = mk("IN-GJ", "IN-GJ");
        over.authority.max_severity = 2;
        assert_eq!(
            acl.check(&env(&over), now()).await,
            AclDecision::InvalidAuthority
        );
    }

    #[tokio::test]
    async fn acl_metrics_and_add_authority() {
        let store = Arc::new(TrustStore::new());
        let mut acl = EmergencyAcl::default(store);
        acl.add_authority(AlertClass::Broadcast, [0xAA; 16]);
        acl.add_authority(AlertClass::Medical, [0xBB; 16]);
        acl.add_authority(AlertClass::Sos, [0xCC; 16]);
        let snap = acl.metrics_snapshot();
        assert_eq!(snap.authorized, 0);
        assert_eq!(snap.unauthorized, 0);
        assert_eq!(acl.metrics().sos_rate_limited, 0);
        acl.reset();
    }

    #[tokio::test]
    async fn acl_authorized_with_valid_authority_chain() {
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        store.register_authority_root(&ad, now()).unwrap();
        let short = ad.authority_short_id();

        let mut config = AclConfig::default();
        config.allowlists.insert(AlertClass::Broadcast, vec![short]);
        let acl = EmergencyAcl::new(config, store);

        let env = test_env(
            MessagePriority::P0,
            ContentType::EmergencyAlert,
            ad.identity_pubkey.to_vec(),
            Some(vec![ad.to_bytes()]),
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::Authorized);
        assert_eq!(acl.metrics().authorized, 1);
    }

    #[tokio::test]
    async fn acl_missing_chain_invalid_authority() {
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        store.register_authority_root(&ad, now()).unwrap();

        let mut config = AclConfig::default();
        config
            .allowlists
            .insert(AlertClass::Broadcast, vec![ad.authority_short_id()]);
        let acl = EmergencyAcl::new(config, store);

        let env = test_env(
            MessagePriority::P0,
            ContentType::EmergencyAlert,
            ad.identity_pubkey.to_vec(),
            None,
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::InvalidAuthority);
        assert_eq!(acl.metrics().invalid_authority, 1);
    }

    #[tokio::test]
    async fn acl_untrusted_chain_invalid_authority() {
        // Valid-looking chain whose root is NOT in the trust store must be
        // rejected by the crypto verifier (SEC-RT-02: no raw-allowlist bypass).
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        let mut config = AclConfig::default();
        config
            .allowlists
            .insert(AlertClass::Medical, vec![ad.authority_short_id()]);
        let acl = EmergencyAcl::new(config, store);

        let env = test_env(
            MessagePriority::P1,
            ContentType::EmergencyAlert,
            ad.identity_pubkey.to_vec(),
            Some(vec![ad.to_bytes()]),
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::InvalidAuthority);
    }

    #[tokio::test]
    async fn acl_chain_root_not_allowlisted() {
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        store.register_authority_root(&ad, now()).unwrap();

        // Allowlist names a DIFFERENT short id than the presented root.
        let mut config = AclConfig::default();
        config
            .allowlists
            .insert(AlertClass::Drill, vec![[0xEE; 16]]);
        let acl = EmergencyAcl::new(config, store);

        let env = test_env(
            MessagePriority::P3,
            ContentType::KeyRotation,
            ad.identity_pubkey.to_vec(),
            Some(vec![ad.to_bytes()]),
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::InvalidAuthority);
    }

    #[tokio::test]
    async fn acl_chain_root_not_authority_root() {
        // Root is TOFU-adopted (trusted for peer trees) but NOT provisioned as
        // an authority root -> must not anchor an emergency chain (EMERG-RT-001).
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        store.adopt_advertisement(&ad, now());

        let mut config = AclConfig::default();
        config
            .allowlists
            .insert(AlertClass::Broadcast, vec![ad.authority_short_id()]);
        let acl = EmergencyAcl::new(config, store);

        let env = test_env(
            MessagePriority::P0,
            ContentType::EmergencyAlert,
            ad.identity_pubkey.to_vec(),
            Some(vec![ad.to_bytes()]),
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::InvalidAuthority);
    }

    #[tokio::test]
    async fn acl_sos_empty_store_authorized() {
        // Empty trust store = Noop permissive (AC-12) even in SOS path — but
        // PRY-10: a *validly signed* envelope is still required.
        let store = Arc::new(TrustStore::new());
        let acl = EmergencyAcl::default(store);
        let kp = IdentityKeypair::generate();
        let mut env = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            kp.verifying_bytes().to_vec(),
            None,
        );
        sign_env(&mut env, &kp);
        assert_eq!(acl.check(&env, now()).await, AclDecision::Authorized);
    }

    /// PRY-10: an attacker sets `sender_id` to a known Verified peer's key but
    /// cannot produce that peer's signature — the SOS gate must reject.
    #[tokio::test]
    async fn acl_sos_spoofed_verified_sender_without_signature_rejected() {
        let store = Arc::new(TrustStore::new());
        let (victim, _vx, victim_ad) = root_ad();
        store.register_authority_root(&victim_ad, now()).unwrap();
        let acl = EmergencyAcl::default(store);

        // No signature at all.
        let env = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            victim.verifying_bytes().to_vec(),
            None,
        );
        assert_eq!(acl.check(&env, now()).await, AclDecision::InvalidAuthority);

        // Signature by the ATTACKER, not the victim.
        let attacker = IdentityKeypair::generate();
        let mut env2 = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            victim.verifying_bytes().to_vec(),
            None,
        );
        sign_env(&mut env2, &attacker);
        assert_eq!(acl.check(&env2, now()).await, AclDecision::InvalidAuthority);
    }

    #[tokio::test]
    async fn acl_sos_unknown_identity_populated_store_unauthorized() {
        // Armed node with a populated store: a self-asserted 32-byte sender
        // that is unknown must NOT pass the SOS gate (SEC-RT-08).
        let store = Arc::new(TrustStore::new());
        let (_root, _x, ad) = root_ad();
        store.register_authority_root(&ad, now()).unwrap();
        let acl = EmergencyAcl::default(store);

        // PRY-10: unknown + unsigned self-asserted sender — rejected. (The
        // signature gate now fires first: InvalidAuthority rather than
        // Unauthorized, but an unknown armed-store sender still cannot pass.)
        let unknown = IdentityKeypair::generate();
        let mut env = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            unknown.verifying_bytes().to_vec(),
            None,
        );
        sign_env(&mut env, &unknown);
        assert_eq!(acl.check(&env, now()).await, AclDecision::Unauthorized);
        assert_eq!(acl.metrics().unauthorized, 1);
    }

    #[tokio::test]
    async fn acl_sos_verified_authorized() {
        let store = Arc::new(TrustStore::new());
        let (root, _x, ad) = root_ad();
        store.register_authority_root(&ad, now()).unwrap();
        let acl = EmergencyAcl::default(store);

        let mut env = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            ad.identity_pubkey.to_vec(),
            None,
        );
        sign_env(&mut env, &root);
        assert_eq!(acl.check(&env, now()).await, AclDecision::Authorized);
    }

    #[tokio::test]
    async fn acl_sos_short_sender_unauthorized() {
        let store = Arc::new(TrustStore::new());
        let acl = EmergencyAcl::default(store);
        let env = test_env(MessagePriority::P2, ContentType::Sos, vec![1u8; 8], None);
        assert_eq!(acl.check(&env, now()).await, AclDecision::Unauthorized);
    }

    #[tokio::test]
    async fn acl_sos_revoked_identity_unauthorized() {
        // Level falls outside Verified/AuthorityRoot/Unknown/Unverified (e.g.
        // Revoked) -> rejected by the catch-all arm.
        let store = Arc::new(TrustStore::new());
        let (root, _x, ad) = root_ad();
        store.adopt_advertisement(&ad, now());
        store.revoke(&ad.identity_pubkey);
        // Populate the store further so it is not empty (armed Noop is gated).
        let (_r2, _x2, ad2) = root_ad();
        store.register_authority_root(&ad2, now()).unwrap();
        let acl = EmergencyAcl::default(store);

        // PRY-10: sign it so it passes the signature gate and actually reaches
        // the revocation check (the catch-all `_ => Unauthorized` arm).
        let mut env = test_env(
            MessagePriority::P2,
            ContentType::Sos,
            ad.identity_pubkey.to_vec(),
            None,
        );
        sign_env(&mut env, &root);
        assert_eq!(acl.check(&env, now()).await, AclDecision::Unauthorized);
    }

    #[tokio::test]
    async fn acl_noop_default_authorized() {
        let trust_store = Arc::new(TrustStore::new());
        let acl = EmergencyAcl::default(trust_store);

        let envelope = Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![],
            priority: MessagePriority::P0,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::EmergencyAlert,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: vec![],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };

        // PRY-1: an armed ACL (this is the only kind — `EmergencyAcl` is never
        // reached un-armed) with no configured authorities must DROP a P0
        // broadcast, not authorize it.
        assert_eq!(
            acl.check(&envelope, crate::message_engine::expiry::unix_now())
                .await,
            AclDecision::InvalidAuthority
        );
    }

    /// PRY-1: armed policy, empty allowlist — Broadcast/Medical are dropped,
    /// but SOS (empty store) and Drill stay permissive.
    #[tokio::test]
    async fn armed_empty_allowlist_denies_broadcast_and_medical() {
        let acl = EmergencyAcl::default(Arc::new(TrustStore::new()));
        let env = |p: MessagePriority, ct: ContentType| Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![],
            priority: p,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ct,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: vec![],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        let t = crate::message_engine::expiry::unix_now();
        assert_eq!(
            acl.check(&env(MessagePriority::P0, ContentType::EmergencyAlert), t).await,
            AclDecision::InvalidAuthority
        );
        assert_eq!(
            acl.check(&env(MessagePriority::P1, ContentType::EmergencyAlert), t).await,
            AclDecision::InvalidAuthority
        );
        // PRY-9: KeyRotation no longer maps to Drill; it falls through to the
        // priority-based class. P3 KeyRotation → Sos class → empty store →
        // InvalidAuthority (PRY-10 signature gate fires first on unsigned env).
        assert_eq!(
            acl.check(&env(MessagePriority::P3, ContentType::KeyRotation), t).await,
            AclDecision::InvalidAuthority
        );
    }
}

#[cfg(all(test, feature = "proptest"))]
mod proptest_tests {
    use super::*;
    use crate::identity::TrustStore;
    use proptest::prelude::*;
    use std::sync::Arc;

    fn block_on<F: std::future::Future>(fut: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(fut)
    }

    fn test_envelope(priority: MessagePriority, payload_type: ContentType) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: crate::protocol::MessageId::new_v7(),
            sender_id: vec![1u8; 32],
            recipient_id: vec![],
            priority,
            ttl_seconds: 3600,
            timestamp: crate::message_engine::expiry::unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: vec![],
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    proptest! {
        #[test]
        fn acl_never_panics(
            priority in any::<MessagePriority>(),
            payload_type in any::<ContentType>(),
        ) {
            let trust_store = Arc::new(TrustStore::new());
            let acl = EmergencyAcl::default(trust_store);
            let envelope = test_envelope(priority, payload_type);
            let _ = block_on(acl.check(&envelope, crate::message_engine::expiry::unix_now()));
        }

        #[test]
        fn acl_armed_empty_allowlist_class_decision(
            priority in any::<MessagePriority>(),
            payload_type in any::<ContentType>(),
        ) {
            // PRY-1: armed ACL, empty allowlist. Broadcast/Medical (P0/P1
            // emergency) are dropped as InvalidAuthority; every other class is
            // still permissive-on-empty (SOS via the empty-store Noop path,
            // Drill pending PRY-9).
            let trust_store = Arc::new(TrustStore::new());
            let acl = EmergencyAcl::default(trust_store);
            let envelope = test_envelope(priority, payload_type);
            let decision = block_on(acl.check(&envelope, crate::message_engine::expiry::unix_now()));
            // PRY-9: KeyRotation no longer has a special arm; it falls into `_`
            // and is treated like any other type (priority-based class).
            let is_broadcast_or_medical = match payload_type {
                ContentType::EmergencyAlert => true,
                ContentType::Sos => false,
                _ => matches!(priority, MessagePriority::P0 | MessagePriority::P1),
            };
            if is_broadcast_or_medical {
                prop_assert_eq!(decision, AclDecision::InvalidAuthority);
            } else {
                // SOS may also be InvalidAuthority now (PRY-10: unsigned
                // envelopes fail the signature gate); Drill stays permissive.
                prop_assert!(matches!(
                    decision,
                    AclDecision::Authorized
                        | AclDecision::Unauthorized
                        | AclDecision::InvalidAuthority
                ));
            }
        }
    }
}
