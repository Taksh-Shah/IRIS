//! # Message Engine — MSG-001 (WP-1)
//!
//! Layer-5 message lifecycle engine (see `docs/implementation/MSG_DESIGN.md` and
//! ORCH-0001 WP-1). Owns:
//! - the outbound [`PriorityQueue`] (P0 preempt + DRR fairness gate),
//! - the [`DedupEngine`] (double-hash Bloom + exact LRU, R5),
//! - the [`AckTracker`] (per-priority retry table, R2),
//! - the [`FragmentAssembler`] (R8 two-fragment model),
//! - a [`MessageStorage`] seam (WP-2 implements it) and a [`CryptoProvider`]
//!   seam (CRYPTO-001 implements it after BLK-0001),
//! - a [`TransportManager`] for selection + send.
//!
//! Three background tasks (spawned by [`MessageEngine::new`]):
//! - **delivery loop** — dequeues, TTL-checks, selects transports, fragments
//!   P4–P7, sends (multipath for P0/P1), schedules retries;
//! - **ack task** — re-queues due retries, fails exhausted attempts;
//! - **GC task** — expires queue entries, evicts expired storage, enforces quota.
//!
//! # Testing / known limitations (MVP)
//! - Unicast delivery requires a concrete 32-byte recipient; broadcast uses the
//!   padded [`EMERGENCY_BROADCAST`] pseudo-peer (true flooding lands in WP-4).
//! - [`DevCryptoProvider`] is inert by design — wire the real provider post
//!   BLK-0001. See ORCH-0001 Risk R1.
//! - Bloom-filter persistence to storage is a WP-2 follow-up (RES-0010 R5).

pub mod ack;
pub mod crypto;
pub mod dedup;
pub mod expiry;
pub mod fragment;
pub mod lifecycle;
pub mod queue;
pub mod storage;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Mutex};
use tokio::task::AbortHandle;

use crate::crypto::key_directory::KeyDirectory;
use crate::message::{IncomingMessage, MessagePriority, PeerId, SerializedMessage};
use crate::message_engine::ack::AckTracker;
use crate::message_engine::crypto::{message_kdf_info, CryptoProvider};
use crate::message_engine::dedup::DedupEngine;
use crate::message_engine::expiry::{check_ttl_before_insert, is_expired, unix_now};
use crate::message_engine::fragment::{
    encode_fragment, is_fragmentable, split_payload, FragmentAssembler,
};
use crate::message_engine::lifecycle::{LifecycleError, MessageStatus};
use crate::message_engine::queue::{PriorityQueue, QueuedMessage};
use crate::message_engine::storage::{MessageStorage, StorageError};
use crate::observability::event;
use crate::observability::metric;
use crate::observability::MetricsRegistry;
use crate::protocol::{
    codec, ContentType, Envelope, EnvelopeError, MessageId, EMERGENCY_BROADCAST,
};
use crate::security::{NoopSecurityPolicy, SecurityPolicy, SenderShort};
use crate::transport::manager::TransportSelectionRequest;
use crate::transport::TransportManager;

/// Padded broadcast pseudo-peer: `[0;28] ++ EMERGENCY_BROADCAST(4)`.
pub fn broadcast_peer() -> PeerId {
    let mut b = [0u8; 32];
    b[28..].copy_from_slice(&EMERGENCY_BROADCAST);
    PeerId(b)
}

/// Engine configuration (MSG_DESIGN.md §Configuration, defaults honored).
#[derive(Debug, Clone)]
pub struct MessageEngineConfig {
    /// Max queued envelopes (default 50 000).
    pub max_queue_depth: usize,
    /// P0 consecutive-dispatch budget before the fairness gate admits one
    /// lower-priority message (default 60).
    pub starvation_p0_budget: u64,
    /// Fragment reassembly window (default 30 s).
    pub fragment_timeout_secs: u64,
    /// GC cycle interval (default 60 s).
    pub gc_interval_secs: u64,
    /// Clock-skew budget for TTL trust (default 300 s).
    pub clock_skew_budget_secs: u64,
    /// Storage quota before priority eviction kicks in (default 500 MB).
    pub storage_quota_bytes: u64,
    /// This node's 32-byte identity (used for inbound delivery vs relay).
    pub node_id: [u8; 32],
    /// Background task poll interval (default 50 ms; tests may raise it).
    pub poll_interval: Duration,
}

impl Default for MessageEngineConfig {
    fn default() -> Self {
        MessageEngineConfig {
            max_queue_depth: 50_000,
            starvation_p0_budget: 60,
            fragment_timeout_secs: 30,
            gc_interval_secs: 60,
            clock_skew_budget_secs: 300,
            storage_quota_bytes: 500 * 1024 * 1024,
            node_id: [0u8; 32],
            poll_interval: Duration::from_millis(50),
        }
    }
}

/// Engine error surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgEngineError {
    Lifecycle(LifecycleError),
    Storage(StorageError),
    Crypto(crate::message_engine::crypto::CryptoError),
    TtlExpired,
    QueueFull,
    Duplicate,
    Codec(String),
    BadEnvelope(&'static str),
}

impl std::fmt::Display for MsgEngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MsgEngineError::Lifecycle(e) => write!(f, "{e}"),
            MsgEngineError::Storage(e) => write!(f, "{e}"),
            MsgEngineError::Crypto(e) => write!(f, "{e}"),
            MsgEngineError::TtlExpired => write!(f, "message engine: ttl expired"),
            MsgEngineError::QueueFull => write!(f, "message engine: queue full"),
            MsgEngineError::Duplicate => write!(f, "message engine: duplicate"),
            MsgEngineError::Codec(e) => write!(f, "message engine: codec: {e}"),
            MsgEngineError::BadEnvelope(m) => write!(f, "message engine: bad envelope: {m}"),
        }
    }
}

impl std::error::Error for MsgEngineError {}

impl From<LifecycleError> for MsgEngineError {
    fn from(e: LifecycleError) -> Self {
        MsgEngineError::Lifecycle(e)
    }
}
impl From<StorageError> for MsgEngineError {
    fn from(e: StorageError) -> Self {
        MsgEngineError::Storage(e)
    }
}
impl From<crate::message_engine::crypto::CryptoError> for MsgEngineError {
    fn from(e: crate::message_engine::crypto::CryptoError) -> Self {
        MsgEngineError::Crypto(e)
    }
}
impl From<EnvelopeError> for MsgEngineError {
    fn from(e: EnvelopeError) -> Self {
        MsgEngineError::Codec(e.to_string())
    }
}

/// Result of processing one inbound transport message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboundOutcome {
    /// Delivered to the local application layer.
    Delivered,
    /// A duplicate of a previously seen message — dropped (dedup).
    Duplicate,
    /// TTL expired before arrival — dropped + tombstoned.
    Expired,
    /// Signed/verified but queued for relay toward the recipient.
    Relayed,
    /// Part of a fragmented ADU; buffered pending reassembly.
    FragmentBuffered,
    /// Reassembled from fragments and delivered.
    ReassembledDelivered,
    /// Unverifiable/rejected emergency content dropped without reply (AC-10).
    EmergencyDropped,
}

/// Engine counters for observability (acceptance #10).
#[derive(Debug, Default)]
pub struct EngineMetrics {
    pub sent: AtomicU64,
    pub delivered: AtomicU64,
    pub relayed: AtomicU64,
    pub dropped_duplicates: AtomicU64,
    pub expired: AtomicU64,
    pub delivery_failed: AtomicU64,
    pub dropped_emergency_auth: AtomicU64,
    pub emergency_sos_rate_limited: AtomicU64,
}

impl EngineMetrics {
    fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            sent: self.sent.load(Ordering::Relaxed),
            delivered: self.delivered.load(Ordering::Relaxed),
            relayed: self.relayed.load(Ordering::Relaxed),
            dropped_duplicates: self.dropped_duplicates.load(Ordering::Relaxed),
            expired: self.expired.load(Ordering::Relaxed),
            delivery_failed: self.delivery_failed.load(Ordering::Relaxed),
            dropped_emergency_auth: self.dropped_emergency_auth.load(Ordering::Relaxed),
            emergency_sos_rate_limited: self.emergency_sos_rate_limited.load(Ordering::Relaxed),
        }
    }
}

/// Point-in-time copy of [`EngineMetrics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    pub sent: u64,
    pub delivered: u64,
    pub relayed: u64,
    pub dropped_duplicates: u64,
    pub expired: u64,
    pub delivery_failed: u64,
    pub dropped_emergency_auth: u64,
    pub emergency_sos_rate_limited: u64,
}

/// The message engine (layer 5).
pub struct MessageEngine {
    config: MessageEngineConfig,
    queue: Mutex<PriorityQueue>,
    dedup: Mutex<DedupEngine>,
    acks: Mutex<AckTracker>,
    fragments: Mutex<FragmentAssembler>,
    storage: Arc<dyn MessageStorage>,
    crypto: Arc<dyn CryptoProvider>,
    key_directory: std::sync::RwLock<Option<Arc<dyn KeyDirectory>>>,
    /// EMERG-001 emergency provider (AC-11). Defaults to a no-op so engine
    /// behavior is unchanged until armed by the caller.
    emergency: std::sync::RwLock<Arc<dyn crate::emergency::EmergencyProvider>>,
    /// SEC-001 security policy (Noop default = un-armed engine identical to current behavior).
    security: std::sync::RwLock<Arc<dyn SecurityPolicy>>,
    manager: Arc<TransportManager>,
    delivered_tx: broadcast::Sender<Envelope>,
    metrics: EngineMetrics,
    telemetry: MetricsRegistry,
    tasks: Mutex<Vec<AbortHandle>>,
}

impl MessageEngine {
    /// Construct the engine and spawn its background tasks.
    pub fn new(
        config: MessageEngineConfig,
        storage: Arc<dyn MessageStorage>,
        crypto: Arc<dyn CryptoProvider>,
        manager: Arc<TransportManager>,
    ) -> Arc<Self> {
        Self::build(config, storage, crypto, manager, MetricsRegistry::new())
    }

    /// Construct the engine with an injected OBS-001 registry so `msg.*`
    /// counters accumulate into the same mesh-wide registry as routing/SCF
    /// (OBS-RT-06 — `MetricsRegistry` is `Clone` and shares counters).
    pub fn new_with_telemetry(
        config: MessageEngineConfig,
        storage: Arc<dyn MessageStorage>,
        crypto: Arc<dyn CryptoProvider>,
        manager: Arc<TransportManager>,
        telemetry: MetricsRegistry,
    ) -> Arc<Self> {
        Self::build(config, storage, crypto, manager, telemetry)
    }

    fn build(
        config: MessageEngineConfig,
        storage: Arc<dyn MessageStorage>,
        crypto: Arc<dyn CryptoProvider>,
        manager: Arc<TransportManager>,
        telemetry: MetricsRegistry,
    ) -> Arc<Self> {
        let (delivered_tx, _) = broadcast::channel(1024);
        let engine = Arc::new(MessageEngine {
            config: config.clone(),
            queue: Mutex::new(PriorityQueue::new(config.starvation_p0_budget.max(1))),
            dedup: Mutex::new(DedupEngine::default()),
            acks: Mutex::new(AckTracker::new()),
            fragments: Mutex::new(FragmentAssembler::new(Duration::from_secs(
                config.fragment_timeout_secs,
            ))),
            storage,
            crypto,
            key_directory: std::sync::RwLock::new(None),
            emergency: std::sync::RwLock::new(Arc::new(
                crate::emergency::NoopEmergencyProvider::new(),
            )),
            security: std::sync::RwLock::new(Arc::new(NoopSecurityPolicy)),
            manager,
            delivered_tx,
            metrics: EngineMetrics::default(),
            telemetry,
            tasks: Mutex::new(Vec::new()),
        });

        let delivery = engine.spawn_delivery_loop();
        let ack = engine.spawn_ack_task();
        let gc = engine.spawn_gc_task();
        engine.tasks.try_lock().unwrap().extend([delivery, ack, gc]);
        engine
    }

    /// Application delivery stream (inbound messages addressed to this node).
    pub fn delivered_messages(&self) -> broadcast::Receiver<Envelope> {
        self.delivered_tx.subscribe()
    }

    /// Current metrics snapshot (acceptance #10 observability).
    pub fn metrics(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Live OBS-001 telemetry registry (counters since last reset).
    pub fn telemetry(&self) -> &MetricsRegistry {
        &self.telemetry
    }

    /// Provide the identity→X25519-key directory used to encrypt addressed
    /// messages (CRYPTO-001/IDENT-001 seam; v1 in-memory for tests/desktop).
    pub fn set_key_directory(&self, directory: Arc<dyn KeyDirectory>) {
        *self.key_directory.write().unwrap() = Some(directory);
    }

    /// Arm the EMERG-001 emergency subsystem (AC-11). Before this call the
    /// engine runs under [`crate::emergency::NoopEmergencyProvider`]: emergency
    /// payloads are not authority-verified and SOS rate limits are not applied.
    /// Installing a provider enables the chain-verification + SOS gates.
    pub fn set_emergency_provider(&self, provider: Arc<dyn crate::emergency::EmergencyProvider>) {
        *self.emergency.write().unwrap() = provider;
    }

    /// Install a SEC-001 security policy (AC-12). Before this call the engine
    /// runs under [`NoopSecurityPolicy`]: all security checks are permissive
    /// and behavior is identical to the un-armed engine.
    pub fn set_security_policy(&self, policy: Arc<dyn SecurityPolicy>) {
        *self.security.write().unwrap() = policy;
    }

    /// CRYPTO-001 send-path sealing: encrypt the payload for the recipient
    /// (when addressed and the recipient X25519 key is known) then sign.
    ///
    /// P0 SOS broadcast / empty-recipient envelopes are **never encrypted**
    /// (no single recipient key) — they are signed only, per CRYPTO_DESIGN.md.
    /// Unicast envelopes whose recipient key is unknown stay plaintext (signed)
    /// until IDENT-001 distributes keys (v1 limitation).
    async fn seal_outbound(&self, envelope: &mut Envelope) -> Result<(), MsgEngineError> {
        if envelope.recipient_id.is_empty() || envelope.recipient_id == EMERGENCY_BROADCAST {
            return self
                .crypto
                .sign(envelope)
                .await
                .map_err(MsgEngineError::Crypto);
        }
        if envelope.recipient_id.len() != 32 {
            // No resolvable recipient — sign only (relay/control thin recipients).
            return self
                .crypto
                .sign(envelope)
                .await
                .map_err(MsgEngineError::Crypto);
        }
        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&envelope.recipient_id);
        let recipient_key = {
            let guard = self.key_directory.read().unwrap();
            guard.as_ref().and_then(|d| d.x25519_pubkey(&node_id))
        };
        if let Some(rkey) = recipient_key {
            // Field 11 (payload_size) is in the AAD scope and is
            // ciphertext-dependent — set it to the sealed length BEFORE
            // deriving the AAD (AEAD output is length-preserving).
            let sealed_len = crate::crypto::aead::sealed_len(envelope.payload.len());
            envelope.payload_size = sealed_len as u64;
            let info = message_kdf_info(envelope.message_id);
            let aad = codec::encode_for_aead(envelope)?;
            let (ciphertext, hdr) = self
                .crypto
                .encrypt(&info, &aad, &envelope.payload, &rkey)
                .await?;
            envelope.payload = ciphertext;
            envelope.encryption_hdr = Some(hdr);
            envelope.payload_hash = Envelope::compute_payload_hash(&envelope.payload);
        } else {
            // RED-0002: addressed unicast with an unresolvable recipient key
            // FAILS OPEN — the message transits relays unwrapped. v1 policy:
            // never silently. Emit a high-visibility warning + a dedicated
            // metric so the downgrade is always observable and reportable.
            // (P0 SOS broadcast is intentionally plaintext and returns above;
            // this path is addressed mail only.) IDENT-001 supplies keys.
            self.telemetry
                .increment(metric::MESSAGES_SENT_UNENCRYPTED_TOTAL);
            tracing::warn!(
                event = event::MSG_SENT_UNENCRYPTED,
                message_id = %envelope.message_id.short(),
                "addressed message sent WITHOUT encryption: recipient key absent from key directory",
            );
        }
        self.crypto
            .sign(envelope)
            .await
            .map_err(MsgEngineError::Crypto)
    }

    /// Enqueue an outbound envelope. Errors on TTL-expired / full queue.
    pub async fn send_message(&self, mut envelope: Envelope) -> Result<(), MsgEngineError> {
        let now = unix_now();
        if !check_ttl_before_insert(envelope.timestamp, envelope.ttl_seconds, now) {
            return Err(MsgEngineError::TtlExpired);
        }

        // SEC-001: outbound rate limiting (per sender, per class)
        let sender_short = self.sender_short_id();
        let class = crate::security::MessageClass::from(envelope.priority);
        let policy = self.security.read().unwrap().clone();
        let rl_decision = policy
            .rate_limit_with_claim(sender_short, class, emergency_claim(&envelope))
            .await;
        if matches!(rl_decision, crate::security::RateLimitDecision::SilentDrop) {
            self.metrics.delivery_failed.fetch_add(1, Ordering::Relaxed);
            self.telemetry
                .increment(metric::MESSAGES_RATE_LIMITED_TOTAL);
            tracing::debug!(
                event = event::MSG_RATE_LIMITED,
                message_id = %envelope.message_id.short(),
                priority = envelope.priority.as_u8(),
                "outbound message rate limited (silent drop)"
            );
            return Ok(()); // Silent drop — no error/NAK
        }

        // CRYPTO-001: seal (encrypt-when-addressed + sign) before persistence
        // so the stored/sent envelope is the sealed form.
        self.seal_outbound(&mut envelope).await?;

        // SEC-001: storage quota. `add_message` is the authoritative gate —
        // check-and-mutate happen under ONE account write-lock (no TOCTOU
        // between the old check_quota read and the persisted add_message write),
        // and its Rejected decision is honored, not discarded (SEC-RT-04).
        // On a later persist failure the reservation is refunded via
        // remove_message so accounting cannot leak.
        let policy = self.security.read().unwrap().clone();
        let quota_decision = policy
            .add_message(
                sender_short,
                envelope.payload.len() as u64,
                envelope.priority,
            )
            .await;
        if matches!(quota_decision, crate::security::QuotaDecision::Rejected) {
            self.metrics.delivery_failed.fetch_add(1, Ordering::Relaxed);
            self.telemetry
                .increment(metric::MESSAGES_QUOTA_EXCEEDED_TOTAL);
            tracing::warn!(
                event = event::MSG_QUOTA_EXCEEDED,
                message_id = %envelope.message_id.short(),
                priority = envelope.priority.as_u8(),
                "outbound message rejected: sender quota exceeded"
            );
            return Err(MsgEngineError::QueueFull); // Quota full
        }

        {
            let mut q = self.queue.lock().await;
            if q.len() >= self.config.max_queue_depth {
                policy
                    .remove_message(sender_short, envelope.payload.len() as u64)
                    .await;
                return Err(MsgEngineError::QueueFull);
            }
            // Register our own id so echo/loopback is deduped (R5).
            let _ = self.dedup.lock().await.seen(envelope.message_id);
            q.push(QueuedMessage::new(envelope.clone()));
        }
        if let Err(e) = self.storage.persist(&envelope).await {
            // Refund the quota reservation (SEC-RT-04) — nothing was stored.
            policy
                .remove_message(sender_short, envelope.payload.len() as u64)
                .await;
            self.queue.lock().await.remove_by_id(&envelope.message_id);
            return Err(MsgEngineError::Storage(e));
        }
        self.storage
            .update_status(&envelope.message_id, MessageStatus::PendingSend)
            .await?;

        tracing::info!(
            event = event::MSG_CREATED,
            message_id = %envelope.message_id.short(),
            priority = envelope.priority.as_u8(),
            size_bytes = envelope.payload.len(),
            ttl_secs = envelope.ttl_seconds,
            "message created and persisted"
        );
        Ok(())
    }

    /// Derive 16-byte sender short ID from this node's identity.
    fn sender_short_id(&self) -> SenderShort {
        // For now, derive from node_id (SHA-256 of node_id, truncated to 16 bytes)
        // In production, this would come from IDENT-001 identity manager
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.config.node_id);
        let result = hasher.finalize();
        let mut short = [0u8; 16];
        short.copy_from_slice(&result[..16]);
        short
    }

    /// Process one inbound transport message. Returns the disposition.
    pub async fn process_incoming(
        &self,
        msg: IncomingMessage,
    ) -> Result<InboundOutcome, MsgEngineError> {
        let envelope: Envelope = codec::decode(&msg.payload)?;

        if envelope.version != crate::protocol::PROTOCOL_VERSION {
            return Err(MsgEngineError::BadEnvelope("unsupported version"));
        }
        // R1: reject unsigned control-plane if provider authenticates and sig missing.
        if self.crypto.authenticates() && envelope.signature.is_none() {
            return Err(MsgEngineError::Crypto(
                crate::message_engine::crypto::CryptoError::BadSignature,
            ));
        }
        if !self.crypto.verify(&envelope).await? {
            return Err(MsgEngineError::Crypto(
                crate::message_engine::crypto::CryptoError::BadSignature,
            ));
        }

        let now = unix_now();
        if is_expired(envelope.timestamp, envelope.ttl_seconds, now) {
            let _ = self.dedup.lock().await.seen(envelope.message_id); // tombstone
            self.metrics.expired.fetch_add(1, Ordering::Relaxed);
            self.telemetry.increment(metric::MESSAGES_EXPIRED_TOTAL);
            tracing::warn!(
                event = event::MSG_EXPIRED,
                message_id = %envelope.message_id.short(),
                priority = envelope.priority.as_u8(),
                age_ms = now.saturating_sub(envelope.timestamp) * 1000,
                "message expired before processing"
            );
            return Ok(InboundOutcome::Expired);
        }

        // Dedup: first arrival wins (DEDUPLICATION.md). Scope the lock so the
        // emit below runs with the guard dropped (OBS-RT-05).
        let is_duplicate = { self.dedup.lock().await.seen(envelope.message_id) };
        if is_duplicate {
            self.metrics
                .dropped_duplicates
                .fetch_add(1, Ordering::Relaxed);
            self.telemetry
                .increment(metric::MESSAGES_DROPPED_DUPLICATES_TOTAL);
            tracing::debug!(
                event = event::MSG_DROPPED_DUPLICATE,
                message_id = %envelope.message_id.short(),
                "duplicate message dropped (first win)"
            );
            return Ok(InboundOutcome::Duplicate);
        }

        // SEC-001: replay protection (freshness window + per-sender high-water)
        // Derive sender short ID from envelope sender_id
        let sender_short = crate::identity::peer_short_from_sender(&envelope.sender_id);
        let policy = self.security.read().unwrap().clone();
        let replay_decision = policy
            .check_replay(
                sender_short,
                envelope.timestamp,
                // Derived from the SIGNED message id, not `hop_count`. Feeding
                // the replay high-water an unsigned, relay-incremented field
                // let any on-path node pin a victim at seq 255 with one crafted
                // packet — blocking that sender for the whole second — and
                // rejected legitimate messages that arrived over a shorter path
                // than an earlier one.
                envelope.message_id.sequence_hint(),
            )
            .await;
        match replay_decision {
            crate::security::ReplayDecision::TooOld => {
                self.metrics
                    .dropped_duplicates
                    .fetch_add(1, Ordering::Relaxed);
                self.telemetry
                    .increment(metric::MESSAGES_REPLAY_TOO_OLD_TOTAL);
                tracing::warn!(
                    event = event::MSG_REPLAY_TOO_OLD,
                    message_id = %envelope.message_id.short(),
                    timestamp = envelope.timestamp,
                    "inbound message rejected: timestamp too old (replay protection)"
                );
                return Ok(InboundOutcome::Duplicate); // Treat as duplicate for dedup
            }
            crate::security::ReplayDecision::TooFuture => {
                self.metrics
                    .dropped_duplicates
                    .fetch_add(1, Ordering::Relaxed);
                self.telemetry
                    .increment(metric::MESSAGES_REPLAY_TOO_FUTURE_TOTAL);
                tracing::warn!(
                    event = event::MSG_REPLAY_TOO_FUTURE,
                    message_id = %envelope.message_id.short(),
                    timestamp = envelope.timestamp,
                    "inbound message rejected: timestamp too far in future (replay protection)"
                );
                return Ok(InboundOutcome::Duplicate);
            }
            crate::security::ReplayDecision::Replay => {
                self.metrics
                    .dropped_duplicates
                    .fetch_add(1, Ordering::Relaxed);
                self.telemetry
                    .increment(metric::MESSAGES_REPLAY_DETECTED_TOTAL);
                tracing::warn!(
                    event = event::MSG_REPLAY_DETECTED,
                    message_id = %envelope.message_id.short(),
                    sender = %hex::encode(sender_short),
                    "inbound message rejected: replay detected (high-water)"
                );
                return Ok(InboundOutcome::Duplicate);
            }
            crate::security::ReplayDecision::Accepted => {
                // Proceed
            }
        }

        // SEC-001: receiver-side spam scoring (annotation only, NEVER relay hard-drop)
        let policy = self.security.read().unwrap().clone();
        let spam_decision = policy.score_spam(sender_short, &envelope).await;
        if matches!(spam_decision, crate::security::SpamDecision::LikelySpam) {
            self.telemetry.increment(metric::MESSAGES_LIKELY_SPAM_TOTAL);
            tracing::info!(
                event = event::MSG_LIKELY_SPAM,
                message_id = %envelope.message_id.short(),
                sender = %hex::encode(sender_short),
                "inbound message annotated as likely spam (receiver-side only)"
            );
            // Note: we do NOT drop here — only annotate for UI (AC-10)
        }

        // SEC-001: emergency ACL check (for emergency content)
        // This complements EMERG-001 emergency_gate by checking authorization data
        if matches!(
            envelope.payload_type,
            ContentType::EmergencyAlert | ContentType::Sos | ContentType::KeyRotation
        ) {
            let now_unix = unix_now();
            let policy = self.security.read().unwrap().clone();
            let acl_decision = policy.check_emergency_acl(&envelope, now_unix).await;
            match acl_decision {
                crate::security::AclDecision::Unauthorized => {
                    self.metrics
                        .dropped_emergency_auth
                        .fetch_add(1, Ordering::Relaxed);
                    self.telemetry
                        .increment(metric::MESSAGES_EMERGENCY_ACL_DENIED_TOTAL);
                    tracing::warn!(
                        event = event::MSG_EMERGENCY_ACL_DENIED,
                        message_id = %envelope.message_id.short(),
                        sender = %hex::encode(sender_short),
                        "emergency message denied by ACL-1 (unauthorized sender)"
                    );
                    return Ok(InboundOutcome::EmergencyDropped);
                }
                crate::security::AclDecision::InvalidAuthority => {
                    self.metrics
                        .dropped_emergency_auth
                        .fetch_add(1, Ordering::Relaxed);
                    self.telemetry
                        .increment(metric::MESSAGES_EMERGENCY_ACL_DENIED_TOTAL);
                    tracing::warn!(
                        event = event::MSG_EMERGENCY_ACL_DENIED,
                        message_id = %envelope.message_id.short(),
                        sender = %hex::encode(sender_short),
                        "emergency message denied by ACL-1 (invalid authority chain)"
                    );
                    return Ok(InboundOutcome::EmergencyDropped);
                }
                crate::security::AclDecision::SosRateLimited => {
                    // EMERG-001 handles the actual rate limiting; ACL just notes it
                    tracing::debug!(
                        event = event::MSG_EMERGENCY_SOS_RATE_LIMITED,
                        message_id = %envelope.message_id.short(),
                        "SOS rate limited via ACL-1"
                    );
                }
                crate::security::AclDecision::Authorized => {
                    // Proceed
                }
            }
        }

        // Fragment reassembly path (R8).
        if envelope.payload_type == ContentType::Fragment {
            let reassembled = {
                let mut f = self.fragments.lock().await;
                f.feed(envelope)
            };
            return match reassembled {
                Some(original) => {
                    // RED-0004 part 3: the reassembled envelope carries the
                    // ORIGINAL whole-ADU signature (restored from the fragment
                    // header) — verify it so a mixed/mutated fragment set or a
                    // tampered reassembly is rejected, not silently delivered.
                    if self.crypto.authenticates() && original.signature.is_none() {
                        return Err(MsgEngineError::Crypto(
                            crate::message_engine::crypto::CryptoError::BadSignature,
                        ));
                    }
                    if !self.crypto.verify(&original).await? {
                        return Err(MsgEngineError::Crypto(
                            crate::message_engine::crypto::CryptoError::BadSignature,
                        ));
                    }
                    let _ = self.dedup.lock().await.seen(original.message_id);
                    tracing::debug!(
                        event = event::MSG_FRAGMENTS_REASSEMBLED,
                        message_id = %original.message_id.short(),
                        "ADU reassembled + whole-ADU signature verified"
                    );
                    self.deliver_or_relay(original, msg.peer_id).await
                }
                None => Ok(InboundOutcome::FragmentBuffered),
            };
        }

        self.deliver_or_relay(envelope, msg.peer_id).await
    }

    /// Route a validated, deduped envelope: deliver if addressed to us or to
    /// the broadcast group, otherwise enqueue for relay.
    async fn deliver_or_relay(
        &self,
        mut envelope: Envelope,
        _via: PeerId,
    ) -> Result<InboundOutcome, MsgEngineError> {
        // EMERG-001 receive-path gate (AC-8/AC-10): when armed, emergency
        // content is classified BEFORE delivery/relay. Unverifiable
        // `EmergencyAlert` is dropped without reply (anti-probing), a
        // verified drill is relayed but never surfaced, and SOS rate-limit
        // bookkeeping runs per sender. Unarmed (Noop default) ⇒ no change to
        // pre-emergency behavior (AC-11).
        let emergency_effect = {
            // RT-009: poison-safe acquire — a panicked emergency callback must
            // not wedge the receive path (read-lock poison recovered via into_inner).
            let provider = self.emergency.read().unwrap_or_else(|e| e.into_inner());
            if !provider.armed() {
                None
            } else {
                Some(emergency_gate(provider.as_ref(), &envelope))
            }
        };
        if let Some(gate_outcome) = emergency_effect {
            match gate_outcome {
                EmergencyGateOutcome::EmergencyDropped(reason) => {
                    // Anti-probing: drop + audit, never reply (DEC-EMERG-0002).
                    self.metrics
                        .dropped_emergency_auth
                        .fetch_add(1, Ordering::Relaxed);
                    self.telemetry
                        .increment(metric::MESSAGES_EMERGENCY_AUTH_DROPPED_TOTAL);
                    tracing::warn!(
                        event = event::MSG_EMERGENCY_AUTH_DROPPED,
                        message_id = %envelope.message_id.short(),
                        reason = %reason,
                        "unverifiable emergency message dropped (no reply)"
                    );
                    return Ok(InboundOutcome::EmergencyDropped);
                }
                EmergencyGateOutcome::EmergencyAlertDrillSuppressed => {
                    tracing::debug!(
                        event = event::MSG_EMERGENCY_DRILL_SUPPRESSED,
                        message_id = %envelope.message_id.short(),
                        "verified drill broadcast relayed (surface suppressed)"
                    );
                }
                EmergencyGateOutcome::EmergencyAlertRelay => {
                    tracing::debug!(
                        event = event::MSG_EMERGENCY_BROADCAST_RELAYED,
                        message_id = %envelope.message_id.short(),
                        "verified emergency broadcast relayed"
                    );
                }
                EmergencyGateOutcome::Proceed => {
                    // Rate-limit bookkeeping already done; proceed.
                }
                EmergencyGateOutcome::SosDowngraded => {
                    // EMERG-RT-002: SOS beyond the P0 allowance is still
                    // delivered per AC-5 (degraded delivery), but at P3 so the
                    // degrade policy applies route-wide. Audit + metrics here
                    // (the gate's log line stays content-free).
                    self.metrics
                        .emergency_sos_rate_limited
                        .fetch_add(1, Ordering::Relaxed);
                    self.telemetry
                        .increment(metric::MESSAGES_EMERGENCY_SOS_RATE_LIMITED_TOTAL);
                    envelope.priority = MessagePriority::P3;
                }
            }
        }

        let mine = recipient_matches(&envelope, &self.config.node_id);
        let broadcast = is_broadcast(&envelope);

        // A broadcast is addressed to everyone, so `recipient_matches` sent it
        // down the deliver branch and returned — it was never relayed. A P0 SOS
        // broadcast therefore propagated exactly one hop, which defeats the
        // primary emergency use case. Broadcasts are now delivered locally AND
        // forwarded (subject to the same hop cap and dedup as any relay).
        if mine && broadcast {
            let outcome = self.deliver_to_self(envelope.clone()).await?;
            // A relay failure must not mask a successful local delivery.
            if let Err(e) = self.enqueue_relay(envelope).await {
                tracing::debug!(
                    event = event::MSG_QUEUED,
                    error = %e,
                    "broadcast delivered locally but not relayed"
                );
            }
            return Ok(outcome);
        }

        if mine {
            return self.deliver_to_self(envelope).await;
        }
        self.enqueue_relay(envelope).await
    }

    /// Deliver an envelope addressed to this node.
    async fn deliver_to_self(&self, envelope: Envelope) -> Result<InboundOutcome, MsgEngineError> {
        {
            // CRYPTO-001 deliver path: decrypt when the envelope is E2EE
            // (engine policy — absent hdr ⇒ plaintext, e.g. P0 SOS broadcast).
            let mut e = envelope;
            if e.encryption_hdr.is_some() {
                let info = message_kdf_info(e.message_id);
                let aad = codec::encode_for_aead(&e)?;
                let hdr = e.encryption_hdr.as_ref().ok_or(MsgEngineError::Crypto(
                    crate::message_engine::crypto::CryptoError::KeyUnavailable,
                ))?;
                let plaintext = self.crypto.decrypt(&info, &aad, &e.payload, hdr).await?;
                e.payload = plaintext;
                e.payload_size = e.payload.len() as u64;
                e.payload_hash = Envelope::compute_payload_hash(&e.payload);
            }

            // An inbound ACK resolves a pending send rather than surfacing as a
            // message. Nothing consumed these before, so no sender ever learned
            // of delivery and every message retried its full budget after
            // arriving successfully.
            if e.payload_type == ContentType::Ack {
                if let Some(ref_id) = e.payload_ref {
                    self.consume_ack(ref_id, &e.sender_id).await;
                }
                return Ok(InboundOutcome::Delivered);
            }

            self.storage
                .update_status(&e.message_id, MessageStatus::Delivered)
                .await?;
            let _ = self.delivered_tx.send(e.clone());
            self.metrics.delivered.fetch_add(1, Ordering::Relaxed);
            self.telemetry.increment(metric::MESSAGES_DELIVERED_TOTAL);
            tracing::info!(
                event = event::MSG_DELIVERED,
                message_id = %e.message_id.short(),
                priority = e.priority.as_u8(),
                hops = e.hop_count,
                "message delivered to final recipient"
            );

            // Acknowledge unicast deliveries so the sender's retry loop can
            // terminate. Never acknowledge an ACK (that would ping-pong) and
            // never acknowledge a broadcast (no single sender is waiting, and
            // every receiver replying would be an amplification vector).
            if !is_broadcast(&e) && !e.sender_id.is_empty() {
                if let Err(err) = self.send_ack(&e).await {
                    tracing::debug!(
                        event = event::MSG_DELIVERED,
                        error = %err,
                        "delivered but ACK could not be sent"
                    );
                }
            }
            Ok(InboundOutcome::Delivered)
        }
    }

    /// Apply an inbound ACK to the pending-send table.
    ///
    /// Only the message's own recipient may acknowledge it. `AckTracker::
    /// acknowledge` is an unguarded remove, so without this binding any peer
    /// could cancel any in-flight message by echoing its id — a one-packet
    /// denial primitive.
    async fn consume_ack(&self, original_id: MessageId, ack_sender: &[u8]) {
        let original = match self.storage.load(&original_id).await {
            Ok(Some(env)) => env,
            _ => return, // unsolicited ACK for something we never sent
        };
        if original.recipient_id.is_empty() || original.recipient_id != ack_sender {
            tracing::debug!(
                event = event::MSG_ACKNOWLEDGED,
                message_id = %original_id.short(),
                "ACK rejected: sender is not the message recipient"
            );
            return;
        }
        self.acknowledge(original_id).await;
    }

    /// Enqueue an envelope for forwarding, enforcing the hop ceiling.
    async fn enqueue_relay(&self, envelope: Envelope) -> Result<InboundOutcome, MsgEngineError> {
        // Nothing bounded hop count on this path before: the signed `max_hops`
        // field was decoded, stored and never read, and the per-priority flood
        // policy lives in the routing module, which the live path does not call.
        // Loop termination rested entirely on per-node dedup.
        // Relayed traffic was governed by nothing at all: neither `rate_limit`
        // nor `add_message` runs on this path, so a remote peer's flood cost it
        // nothing and consumed this node's queue and airtime without limit.
        // The sender is authenticated by this point (signature verified in
        // `process_incoming`), so keying the limit on it is sound.
        let relay_sender = crate::identity::peer_short_from_sender(&envelope.sender_id);
        let relay_class = crate::security::MessageClass::from(envelope.priority);
        let policy = self
            .security
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if matches!(
            policy
                .rate_limit_with_claim(relay_sender, relay_class, emergency_claim(&envelope))
                .await,
            crate::security::RateLimitDecision::SilentDrop
        ) {
            self.telemetry
                .increment(metric::MESSAGES_RATE_LIMITED_TOTAL);
            tracing::debug!(
                event = event::MSG_RATE_LIMITED,
                message_id = %envelope.message_id.short(),
                priority = envelope.priority.as_u8(),
                "relay dropped: sender rate limit"
            );
            return Ok(InboundOutcome::Expired);
        }

        let cap = hop_cap(&envelope);
        if envelope.hop_count >= cap {
            self.metrics.expired.fetch_add(1, Ordering::Relaxed);
            tracing::debug!(
                event = event::MSG_QUEUED,
                message_id = %envelope.message_id.short(),
                hops = envelope.hop_count,
                cap,
                "relay dropped: hop ceiling reached"
            );
            return Ok(InboundOutcome::Expired);
        }

        let mut e = envelope.clone();
        e.hop_count = e.hop_count.saturating_add(1);
        let mut q = self.queue.lock().await;
        if q.len() >= self.config.max_queue_depth {
            return Err(MsgEngineError::QueueFull);
        }
        let mut item = QueuedMessage::new(e);
        item.local = false;
        q.push(item);
        let depth = q.len();
        drop(q);
        self.metrics.relayed.fetch_add(1, Ordering::Relaxed);
        self.telemetry.increment(metric::MESSAGES_RELAYED_TOTAL);
        tracing::debug!(
            event = event::MSG_QUEUED,
            message_id = %envelope.message_id.short(),
            priority = envelope.priority.as_u8(),
            queue_depth = depth,
            "message queued for relay"
        );
        Ok(InboundOutcome::Relayed)
    }

    /// Re-queue an already-sealed stored envelope for another delivery attempt.
    ///
    /// Deliberately not `send_message`: that path re-runs `seal_outbound`,
    /// which does not check whether the envelope is already sealed. Retrying
    /// through it encrypted the ciphertext again on every cycle — the payload
    /// grew by an AEAD tag each time and the recipient could never decrypt it.
    /// It also re-charged the sender's storage quota per retry (with a single
    /// refund) and re-inserted into dedup. A retry re-sends bytes that were
    /// already accepted; none of that bookkeeping should run twice.
    async fn requeue_for_retry(&self, envelope: Envelope) -> Result<(), MsgEngineError> {
        let now = unix_now();
        if is_expired(envelope.timestamp, envelope.ttl_seconds, now) {
            return Err(MsgEngineError::TtlExpired);
        }
        let mut q = self.queue.lock().await;
        if q.len() >= self.config.max_queue_depth {
            return Err(MsgEngineError::QueueFull);
        }
        q.push(QueuedMessage::new(envelope));
        Ok(())
    }

    /// Acknowledge a pending message (positive ACK received).
    pub async fn acknowledge(&self, id: MessageId) -> bool {
        let acked = self.acks.lock().await.acknowledge(id);
        if acked {
            let _ = self
                .storage
                .update_status(&id, MessageStatus::Acknowledged)
                .await;
            // SEC-RT-05: deliverable terminal — release the quota reservation
            // for a locally-originated outbound message (charged at send).
            if let Ok(Some(env)) = self.storage.load(&id).await {
                let policy = self.security.read().unwrap().clone();
                let sender_short = crate::identity::peer_short_from_sender(&env.sender_id);
                if sender_short == self.sender_short_id() {
                    policy
                        .remove_message(sender_short, env.payload.len() as u64)
                        .await;
                }
            }
            tracing::info!(
                event = event::MSG_ACKNOWLEDGED,
                message_id = %id.short(),
                "positive acknowledgement received"
            );
        }
        acked
    }

    /// Build an ACK envelope for `original` (control plane, never fragmented).
    pub fn build_ack(&self, original: &Envelope) -> Envelope {
        let mut ack = Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: original.recipient_id.clone(),
            recipient_id: original.sender_id.clone(),
            priority: MessagePriority::P4,
            ttl_seconds: original.ttl_seconds,
            timestamp: unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Ack,
            payload_size: 16,
            payload_hash: [0; 32],
            payload: original.message_id.to_bytes().to_vec(),
            payload_ref: Some(original.message_id),
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        ack.payload_hash = Envelope::compute_payload_hash(&ack.payload);
        ack
    }

    /// Send an ACK for a received message (best-effort; errors surface).
    pub async fn send_ack(&self, original: &Envelope) -> Result<(), MsgEngineError> {
        let ack = self.build_ack(original);
        self.send_message(ack).await
    }

    /// Reject (fail) a pending message after attempts exhausted.
    pub async fn fail_message(&self, id: MessageId) {
        let _ = self.acks.lock().await.acknowledge(id);
        let _ = self
            .storage
            .update_status(&id, MessageStatus::DeliveryFailed)
            .await;
        self.metrics.delivery_failed.fetch_add(1, Ordering::Relaxed);
        self.telemetry
            .increment(metric::MESSAGES_DELIVERY_FAILED_TOTAL);
        tracing::warn!(
            event = event::MSG_DELIVERY_FAILED,
            message_id = %id.short(),
            "delivery attempt budget exhausted"
        );
    }

    /// Stop background tasks (idempotent).
    pub async fn shutdown(&self) {
        for h in self.tasks.lock().await.drain(..) {
            h.abort();
        }
    }

    /// Encode an envelope for transport (fragmenting P4–P7 if the largest
    /// candidate transport cannot carry it). Returns payloads to send.
    async fn serialize_for_transport(
        &self,
        envelope: &Envelope,
        max_mtu: usize,
    ) -> Result<Vec<Envelope>, MsgEngineError> {
        let bytes = codec::encode(envelope)?;
        if bytes.len() <= max_mtu || !is_fragmentable(envelope) {
            return Ok(vec![envelope.clone()]);
        }
        // R8: two-fragment model.
        let usable = max_mtu.saturating_sub(fragment::FRAGMENT_HEADER_LEN);
        let parts = split_payload(&envelope.payload, usable.max(64));
        if parts.len() > fragment::MAX_FRAGMENTS as usize {
            return Err(MsgEngineError::BadEnvelope(
                "payload needs >2 fragments (R8)",
            ));
        }
        let total = parts.len() as u16;
        let mut frags = Vec::with_capacity(parts.len());
        for (i, chunk) in parts.into_iter().enumerate() {
            let mut f = envelope.clone();
            // RED-0004: each wire fragment gets a DISTINCT message_id (derived
            // from the original) so the receiver's dedup sees each fragment as
            // a fresh message — the shared original id made fragment 1 a
            // "duplicate" and multi-fragment ADUs never reassembled. Reassembly
            // restores the original id from the payload header.
            f.message_id = fragment::derive_fragment_message_id(&envelope.message_id, i as u16);
            f.payload_type = ContentType::Fragment;
            f.payload = encode_fragment(
                i as u16,
                total,
                envelope.message_id,
                envelope.payload_type,
                envelope.signature,
                &chunk,
            );
            f.payload_size = f.payload.len() as u64;
            f.payload_hash = Envelope::compute_payload_hash(&f.payload);
            f.payload_ref = Some(envelope.message_id);
            // CRYPTO-001: each wire fragment is a distinct carrier of the sealed
            // plaintext-chunk — re-sign it so the receiver's per-fragment
            // verification succeeds (fragment payload ≠ sealed envelope payload).
            self.crypto
                .sign(&mut f)
                .await
                .map_err(MsgEngineError::Crypto)?;
            frags.push(f);
        }
        Ok(frags)
    }

    fn spawn_delivery_loop(self: &Arc<Self>) -> AbortHandle {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(this.config.poll_interval).await;
                let Some(item) = this.queue.lock().await.dequeue() else {
                    continue;
                };
                this.deliver_outbound(item).await;
            }
        })
        .abort_handle()
    }

    async fn deliver_outbound(&self, item: QueuedMessage) {
        let now = unix_now();
        let id = item.envelope.message_id;
        let priority = item.envelope.priority;

        // TTL at dequeue (acceptance #3).
        if is_expired(item.envelope.timestamp, item.envelope.ttl_seconds, now) {
            let _ = self
                .storage
                .update_status(&id, MessageStatus::Expired)
                .await;
            self.metrics.expired.fetch_add(1, Ordering::Relaxed);
            self.telemetry.increment(metric::MESSAGES_EXPIRED_TOTAL);
            tracing::warn!(
                event = event::MSG_EXPIRED,
                message_id = %id.short(),
                priority = priority.as_u8(),
                age_ms = now.saturating_sub(item.envelope.timestamp) * 1000,
                "message expired at dequeue"
            );
            // SEC-RT-05: release the quota reservation. Only locally-originated
            // messages were charged (sender == us); relayed messages were never
            // charged, so their terminal events must NOT touch our account.
            if item.local {
                let policy = self.security.read().unwrap().clone();
                policy
                    .remove_message(self.sender_short_id(), item.envelope.payload.len() as u64)
                    .await;
            }
            return;
        }

        let bytes = match codec::encode(&item.envelope) {
            Ok(b) => b,
            Err(_) => return,
        };
        let dest = recipient_peer(&item.envelope);

        // Pick transports (multipath for P0/P1 emergencies).
        let req = TransportSelectionRequest {
            target_peer: dest,
            message_size: bytes.len(),
            priority,
            max_latency_ms: None,
            prefer_low_cost: false,
            multipath: priority.is_emergency(),
            // MSG-001 R8: oversized P4–P7 bulk is split into `max_message_size`
            // fragments by serialize_for_transport, so size no longer excludes
            // the transport from selection (fragmentable).
            fragmentable: is_fragmentable(&item.envelope),
        };
        let ranked = self.manager.select_transports(&req).await;
        if ranked.is_empty() {
            // No transport available right now → retry later (P0 never dropped).
            self.requeue_or_fail(item).await;
            return;
        }

        // Effective MTU = the SMALLEST selected transport's limit (the
        // bottleneck): fragments must fit every transport the message is sent
        // over. (Previously initialized to usize::MAX with `.max()` — so
        // serialize_for_transport never fragmented in the real path.)
        let mut max_mtu = usize::MAX;
        for r in &ranked {
            if let Some(t) = self.manager.get(&r.transport_id).await {
                max_mtu = max_mtu.min(t.capabilities().max_message_size);
            }
        }
        let to_send = match self.serialize_for_transport(&item.envelope, max_mtu).await {
            Ok(v) => v,
            Err(_) => {
                self.requeue_or_fail(item).await;
                return;
            }
        };

        let mut sent_any = false;
        for r in &ranked {
            let Some(t) = self.manager.get(&r.transport_id).await else {
                continue;
            };
            t.set_send_priority_hint(priority);
            for frag in &to_send {
                let payload = match codec::encode(frag) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let Some(target) = dest.or(Some(broadcast_peer())) else {
                    continue;
                };
                let sm = SerializedMessage {
                    message_id: frag.message_id,
                    priority,
                    payload,
                };
                if t.send(&target, &sm).await.is_ok() {
                    sent_any = true;
                }
            }
        }

        if sent_any {
            let _ = self
                .storage
                .update_status(&id, MessageStatus::InTransit)
                .await;
            self.acks
                .lock()
                .await
                .register_sent(id, priority, std::time::Instant::now());
            self.metrics.sent.fetch_add(1, Ordering::Relaxed);
            self.telemetry.increment(metric::MESSAGES_SENT_TOTAL);
            tracing::info!(
                event = event::MSG_SENT,
                message_id = %id.short(),
                priority = priority.as_u8(),
                "message handed to transport"
            );
        } else {
            self.requeue_or_fail(item).await;
        }
    }

    async fn requeue_or_fail(&self, item: QueuedMessage) {
        let policy = crate::message_engine::ack::retry_policy(item.envelope.priority);
        let attempt = item.attempts + 1;
        let budget_ok = match policy.max_attempts {
            Some(max) => attempt <= max,
            None => true,
        };
        if !budget_ok {
            // SEC-RT-05: attempts exhausted → terminal. Refund the quota
            // reservation charged at send_message time (local origin only).
            self.fail_message(item.envelope.message_id).await;
            if item.local {
                let policy = self.security.read().unwrap().clone();
                policy
                    .remove_message(self.sender_short_id(), item.envelope.payload.len() as u64)
                    .await;
            }
            return;
        }
        let mut it = item;
        it.attempts = attempt;
        it.next_retry = Some(std::time::Instant::now() + policy.timeout_for_attempt(attempt));
        let _ = self
            .storage
            .update_status(&it.envelope.message_id, MessageStatus::PendingSend)
            .await;
        self.queue.lock().await.push(it);
    }

    fn spawn_ack_task(self: &Arc<Self>) -> AbortHandle {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let due = this
                    .acks
                    .lock()
                    .await
                    .due_retries(std::time::Instant::now());
                for id in due {
                    // Re-queue a retry for the stored envelope.
                    let env = this.storage.load(&id).await.ok().flatten();
                    let Some(env) = env else {
                        // No stored copy means no retry can ever succeed. Stop
                        // tracking it, otherwise the entry stays permanently due
                        // and is re-examined every tick forever.
                        this.acks.lock().await.forget(id);
                        continue;
                    };
                    // Record the attempt BEFORE dispatching. This is what
                    // advances `next_retry` and the attempt count; without it
                    // `due_retries` returned the same id on every tick, so every
                    // message ever sent was retransmitted once per second for
                    // the life of the process — a self-inflicted storm that grew
                    // linearly with lifetime message count and needed no attacker.
                    if this
                        .acks
                        .lock()
                        .await
                        .record_retry(id, std::time::Instant::now())
                        .is_none()
                    {
                        continue; // acknowledged concurrently
                    }
                    let _ = this.requeue_for_retry(env).await;
                }
                let failed: Vec<_> = this.acks.lock().await.exhausted();
                for (id, _attempts) in failed {
                    this.fail_message(id).await;
                    // SEC-RT-05: ack-exhausted terminal. Refund quota if the
                    // message was locally originated (loaded from storage).
                    if let Ok(Some(env)) = this.storage.load(&id).await {
                        let policy = this.security.read().unwrap().clone();
                        let sender_short = crate::identity::peer_short_from_sender(&env.sender_id);
                        if sender_short == this.sender_short_id() {
                            policy
                                .remove_message(sender_short, env.payload.len() as u64)
                                .await;
                        }
                    }
                }
            }
        })
        .abort_handle()
    }

    fn spawn_gc_task(self: &Arc<Self>) -> AbortHandle {
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(this.config.gc_interval_secs.max(1))).await;
                // Expire queue entries (TTL at relay).
                let now = unix_now();
                let expired = this
                    .queue
                    .lock()
                    .await
                    .drain_expired(|e| is_expired(e.timestamp, e.ttl_seconds, now));
                for e in expired {
                    let _ = this
                        .storage
                        .update_status(&e.message_id, MessageStatus::Expired)
                        .await;
                    this.metrics.expired.fetch_add(1, Ordering::Relaxed);
                    this.telemetry.increment(metric::MESSAGES_EXPIRED_TOTAL);
                    tracing::warn!(
                        event = event::MSG_EXPIRED,
                        message_id = %e.message_id.short(),
                        priority = e.priority.as_u8(),
                        "message expired by GC sweep"
                    );
                }
                // Expire + quota in storage.
                let _ = this.storage.evict_expired(now).await;
                if let Ok(usage) = this.storage.usage_bytes().await {
                    if usage > this.config.storage_quota_bytes {
                        let _ = this
                            .storage
                            .evict_by_priority(usage - this.config.storage_quota_bytes)
                            .await;
                    }
                }
            }
        })
        .abort_handle()
    }
}

/// Resolve the concrete destination peer for an envelope.
fn recipient_peer(e: &Envelope) -> Option<PeerId> {
    if e.recipient_id.len() == 32 {
        let mut b = [0u8; 32];
        b.copy_from_slice(&e.recipient_id);
        Some(PeerId(b))
    } else if e.recipient_id == EMERGENCY_BROADCAST || e.recipient_id.is_empty() {
        Some(broadcast_peer())
    } else {
        None
    }
}

/// Whether this envelope is addressed to `node_id` (or is a broadcast).
fn recipient_matches(e: &Envelope, node_id: &[u8; 32]) -> bool {
    e.recipient_id.is_empty()
        || e.recipient_id == EMERGENCY_BROADCAST
        || (e.recipient_id.len() == 32 && e.recipient_id.as_slice() == node_id)
}

/// Whether an envelope's payload justifies the P0/P1 rate-limit exemption.
///
/// Read from the content type, never from the sender-chosen priority.
fn emergency_claim(e: &Envelope) -> crate::security::EmergencyClaim {
    if e.payload_type.is_emergency() {
        crate::security::EmergencyClaim::Emergency
    } else {
        crate::security::EmergencyClaim::Ordinary
    }
}

/// Whether this envelope is addressed to everyone rather than to one node.
///
/// A broadcast matches every recipient, so it must be both delivered locally
/// and forwarded — and must never be individually acknowledged.
fn is_broadcast(e: &Envelope) -> bool {
    e.recipient_id.is_empty() || e.recipient_id == EMERGENCY_BROADCAST
}

/// Maximum hop count an envelope may reach before it stops being forwarded.
///
/// The sender-signed `max_hops` is authoritative when present — it is inside
/// the signature scope, so a relay cannot raise it. Absent that, the
/// per-priority flood policy applies.
///
/// NOTE: that policy is `u8::MAX` for P0. Combined with `saturating_add` the
/// relay does terminate (at 255 hops), but 255 is not a meaningful bound for a
/// mesh and P0 is exactly the class an attacker would forge. Tightening it is a
/// protocol-policy decision, deliberately left to the design owners rather than
/// changed here; senders can bound their own P0 traffic today via `max_hops`.
fn hop_cap(e: &Envelope) -> u8 {
    e.max_hops
        .unwrap_or_else(|| crate::routing::flood::max_hops_for_priority(e.priority).max_hops)
}

/// Result of the EMERG-001 receive-path gate for one emergency envelope.
enum EmergencyGateOutcome {
    /// Unverifiable or rejected emergency content — drop + audit, no reply.
    EmergencyDropped(String),
    /// Verified, non-drill `EmergencyAlert` — proceed to deliver/relay.
    EmergencyAlertRelay,
    /// Verified drill `EmergencyAlert` — relay, never surface (AC-10).
    EmergencyAlertDrillSuppressed,
    /// Accepted SOS within allowance — proceed (bookkeeping done).
    Proceed,
    /// Accepted SOS but rate-limited (EMERG-RT-002): downgrade delivery to P3
    /// while still delivering/relaying (AC-5 degraded delivery). The caller
    /// lowers the envelope priority so the P3 policy applies route-wide.
    SosDowngraded,
}

/// EMERG-001 receive-path gate (AC-5/AC-8/AC-10). `armed` is verified by the
/// caller. Classifies emergency content before delivery/relay:
/// - `EmergencyAlert` → `verify_envelope` pipeline (chain + scope + validity);
///   unverifiable → drop; drill → suppress surface; else relay.
/// - `Sos` → classifier + rate-limit bookkeeping (AC-5/AC-6): accepted SOS is
///   recorded; a verified Cancel resets the window; a payload the module
///   classifier rejects is dropped + audited (never delivered). The 4th-in-
///   window downgrade-to-P3 is a **send-side** policy (the sender builds the
///   SOS at P3 once the allowance is exhausted) — AC-5; receive-side records
///   the bookkeeping per this design's §10 receive hook.
/// - Non-emergency content is untouched (fell-through `Proceed`).
fn emergency_gate(
    provider: &dyn crate::emergency::EmergencyProvider,
    envelope: &Envelope,
) -> EmergencyGateOutcome {
    use crate::emergency::VerifyOutcome;
    let now_unix = crate::message_engine::expiry::unix_now();
    match envelope.payload_type {
        ContentType::EmergencyAlert => match provider.verify_envelope(envelope, now_unix) {
            VerifyOutcome::Relay { drill: true, .. } | VerifyOutcome::Suppressed { .. } => {
                EmergencyGateOutcome::EmergencyAlertDrillSuppressed
            }
            VerifyOutcome::Relay { drill: false, .. } => EmergencyGateOutcome::EmergencyAlertRelay,
            VerifyOutcome::Drop(reason) => EmergencyGateOutcome::EmergencyDropped(reason),
        },
        ContentType::Sos => match provider.classify_sos(&envelope.payload, now_unix, None) {
            Ok(crate::emergency::SosOutcome::AcceptedSos { .. }) => {
                // EMERG-RT-002: enforce the receive-side rate gate BEFORE
                // recording. Within allowance → record + proceed; beyond →
                // still deliver, but degraded to P3 (AC-5 4th-in-window rule,
                // enforced here on the receive path, not only send-side).
                match provider.sos_rate(&envelope.sender_id, now_unix) {
                    crate::emergency::RateLimitDecision::Allowed { .. } => {
                        provider.sos_record(&envelope.sender_id, now_unix);
                        EmergencyGateOutcome::Proceed
                    }
                    crate::emergency::RateLimitDecision::Downgraded { retry_after_secs } => {
                        provider.sos_record(&envelope.sender_id, now_unix);
                        tracing::warn!(
                            event = event::MSG_EMERGENCY_SOS_RATE_LIMITED,
                            message_id = %envelope.message_id.short(),
                            retry_after_secs,
                            "SOS rate-limited (EMERG-RT-002): downgraded to P3 degraded delivery"
                        );
                        // Audit: metadata-only row (AC-9; no payload content).
                        provider.audit(crate::emergency::EmergencyAuditRecord {
                            timestamp: now_unix,
                            event: crate::emergency::AuditEvent::SosRateLimited,
                            subject: crate::emergency::authority::authority_short_id(
                                &envelope.sender_id,
                            ),
                            note: Some(format!("sos_downgraded:{retry_after_secs}")),
                        });
                        EmergencyGateOutcome::SosDowngraded
                    }
                }
            }
            Ok(crate::emergency::SosOutcome::AcceptedCancel) => {
                provider.sos_reset(&envelope.sender_id);
                EmergencyGateOutcome::Proceed
            }
            Err(e) => EmergencyGateOutcome::EmergencyDropped(e.to_string()),
            Ok(crate::emergency::SosOutcome::Rejected(e)) => {
                EmergencyGateOutcome::EmergencyDropped(e.to_string())
            }
        },
        _ => EmergencyGateOutcome::Proceed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_engine::crypto::DevCryptoProvider;
    use crate::message_engine::storage::MemoryStorage;
    use crate::transport::simulated::{SimConfig, SimulatedTransport};
    use crate::transport::Transport;
    use futures_util::StreamExt;

    const ALICE: [u8; 32] = [0xAA; 32];
    const BOB: [u8; 32] = [0xBB; 32];

    fn envelope_for(recipient: [u8; 32], priority: MessagePriority, payload: &[u8]) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: ALICE.to_vec(),
            recipient_id: recipient.to_vec(),
            priority,
            ttl_seconds: 3600,
            timestamp: unix_now(),
            hop_count: 0,
            max_hops: None,
            payload_type: ContentType::Text,
            payload_size: payload.len() as u64,
            payload_hash: Envelope::compute_payload_hash(payload),
            payload: payload.to_vec(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        }
    }

    async fn alice_engine() -> (Arc<MessageEngine>, Arc<SimulatedTransport>) {
        let transport = Arc::new(SimulatedTransport::new(
            "sim-alice",
            "Sim A",
            SimConfig {
                packet_loss_rate: 0.0,
                ..Default::default()
            },
        ));
        let manager = Arc::new(TransportManager::new());
        manager.register(transport.clone()).await.unwrap();
        let cfg = MessageEngineConfig {
            node_id: ALICE,
            gc_interval_secs: 3600, // keep GC out of the way in tests
            ..Default::default()
        };
        (
            MessageEngine::new(
                cfg,
                Arc::new(MemoryStorage::new()),
                Arc::new(DevCryptoProvider::new()),
                manager,
            ),
            transport,
        )
    }

    /// Drain one inbound message from a SimulatedTransport stream and feed it
    /// to the given engine's `process_incoming`.
    async fn pump_one(
        transport: &Arc<SimulatedTransport>,
        engine: &Arc<MessageEngine>,
    ) -> Result<InboundOutcome, MsgEngineError> {
        let mut stream = transport.incoming_messages();
        let msg = tokio::time::timeout(Duration::from_millis(500), stream.next())
            .await
            .expect("transport must deliver")
            .expect("stream item");
        engine.process_incoming(msg).await
    }

    const CAROL: [u8; 32] = [0xCC; 32];

    #[tokio::test(flavor = "multi_thread")]
    async fn relay_stops_at_the_signed_hop_ceiling() {
        // Regression: nothing bounded hop count on the live relay path. The
        // signed `max_hops` was decoded, stored and never read, and the
        // per-priority flood policy lives in the routing module, which the
        // live path never calls. Loop termination rested entirely on dedup.
        let (alice, _t) = alice_engine().await;

        let mut under = envelope_for(CAROL, MessagePriority::P4, b"transit");
        under.max_hops = Some(4);
        under.hop_count = 3;
        assert!(
            matches!(
                alice.deliver_or_relay(under, PeerId(BOB)).await.unwrap(),
                InboundOutcome::Relayed
            ),
            "below the ceiling the message is forwarded"
        );

        let mut at_cap = envelope_for(CAROL, MessagePriority::P4, b"transit");
        at_cap.max_hops = Some(4);
        at_cap.hop_count = 4;
        assert!(
            matches!(
                alice.deliver_or_relay(at_cap, PeerId(BOB)).await.unwrap(),
                InboundOutcome::Expired
            ),
            "at the ceiling the relay must stop"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn relay_falls_back_to_priority_policy_without_max_hops() {
        let (alice, _t) = alice_engine().await;
        // P4 policy caps at 3 hops.
        let mut e = envelope_for(CAROL, MessagePriority::P4, b"transit");
        e.max_hops = None;
        e.hop_count = 3;
        assert!(matches!(
            alice.deliver_or_relay(e, PeerId(BOB)).await.unwrap(),
            InboundOutcome::Expired
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn broadcast_is_delivered_and_also_relayed() {
        // Regression: `recipient_matches` treats a broadcast as "mine", so the
        // deliver branch returned early and the message was never forwarded.
        // A P0 SOS broadcast therefore propagated exactly one hop.
        let (alice, _t) = alice_engine().await;

        let mut e = envelope_for(BOB, MessagePriority::P0, b"sos");
        e.recipient_id = EMERGENCY_BROADCAST.to_vec();
        e.hop_count = 0;

        let before = alice.queue.lock().await.len();
        let out = alice.deliver_or_relay(e, PeerId(BOB)).await.unwrap();
        let after = alice.queue.lock().await.len();

        assert!(matches!(out, InboundOutcome::Delivered));
        assert!(
            after > before,
            "a broadcast must be forwarded as well as delivered"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forged_ack_cannot_cancel_another_peers_message() {
        // `AckTracker::acknowledge` is an unguarded remove, so the ACK must be
        // bound to the original's recipient — otherwise any peer can cancel any
        // in-flight message by echoing its id.
        let (alice, _t) = alice_engine().await;

        let original = envelope_for(BOB, MessagePriority::P1, b"payload");
        let id = original.message_id;
        alice.storage.persist(&original).await.unwrap();
        alice
            .acks
            .lock()
            .await
            .register_sent(id, MessagePriority::P1, std::time::Instant::now());

        // CAROL is not the recipient — her ACK must be ignored.
        alice.consume_ack(id, CAROL.as_ref()).await;
        assert!(
            alice.acks.lock().await.is_pending(&id),
            "an ACK from a non-recipient must not clear the pending send"
        );

        // BOB is the recipient — his ACK resolves it.
        alice.consume_ack(id, BOB.as_ref()).await;
        assert!(
            !alice.acks.lock().await.is_pending(&id),
            "the recipient's ACK must clear the pending send"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn engine_roundtrip_delivers_to_peer_across_transport() {
        let (alice, a_transport) = alice_engine().await;
        // Make the management register on the same runtime:
        // (already registered block_on above)

        // Bob's engine receives from ALICE's simulated transport link.
        let b_manager = Arc::new(TransportManager::new());
        let bob = MessageEngine::new(
            MessageEngineConfig {
                node_id: BOB,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            b_manager,
        );

        let mut bob_delivered = bob.delivered_messages();

        let env = envelope_for(BOB, MessagePriority::P4, b"hello bob");
        let id = env.message_id;
        alice.send_message(env).await.unwrap();

        let outcome = pump_one(&a_transport, &bob).await.unwrap();
        assert_eq!(outcome, InboundOutcome::Delivered);

        let delivered = tokio::time::timeout(Duration::from_millis(500), bob_delivered.recv())
            .await
            .expect("delivered event")
            .expect("event");
        assert_eq!(delivered.message_id, id);
        assert_eq!(delivered.payload, b"hello bob");
        assert_eq!(alice.metrics().sent, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn dedup_drops_second_arrival() {
        let (_alice, _a_transport) = alice_engine().await;
        let bob = MessageEngine::new(
            MessageEngineConfig {
                node_id: BOB,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );

        // Inject the same envelope twice (e.g. relayed via two paths).
        let env = envelope_for(BOB, MessagePriority::P3, b"dup me");
        let bytes = codec::encode(&env).unwrap();
        let first = IncomingMessage {
            peer_id: PeerId([1u8; 32]),
            transport_id: "sim".into(),
            payload: bytes.clone(),
            received_at: std::time::Instant::now(),
        };
        let second = IncomingMessage {
            peer_id: PeerId([2u8; 32]),
            transport_id: "sim".into(),
            payload: bytes,
            received_at: std::time::Instant::now(),
        };
        assert_eq!(
            bob.process_incoming(first).await.unwrap(),
            InboundOutcome::Delivered
        );
        assert_eq!(
            bob.process_incoming(second).await.unwrap(),
            InboundOutcome::Duplicate
        );
        assert_eq!(bob.metrics().dropped_duplicates, 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn outbound_expired_rejected_at_insert() {
        let (alice, _t) = alice_engine().await;
        let mut env = envelope_for(BOB, MessagePriority::P0, b"too old");
        env.timestamp = unix_now().saturating_sub(7200);
        env.ttl_seconds = 60;
        assert_eq!(
            alice.send_message(env).await,
            Err(MsgEngineError::TtlExpired)
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn transit_failure_requeues_and_retries() {
        // 100% loss transport → send "succeeds" but no packet arrives; the engine
        // keeps the message in PendingSend/InTransit with retries scheduled.
        let transport = Arc::new(SimulatedTransport::new(
            "sim-loss",
            "Sim Loss",
            SimConfig {
                packet_loss_rate: 1.0,
                ..Default::default()
            },
        ));
        let manager = Arc::new(TransportManager::new());
        manager.register(transport.clone()).await.unwrap();
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            manager,
        );
        let env = envelope_for(BOB, MessagePriority::P4, b"retry me");
        let env_id = env.message_id;
        engine.send_message(env).await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        // Loss registers as sent (sim send Ok) → InTransit, retries pending.
        assert_eq!(engine.metrics().sent, 1);
        assert!(engine.acks.lock().await.is_pending(&env_id));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn relay_path_reaches_next_hop_engine() {
        // Simulate a relay: Alice → (Carol relay) → Bob. Carol's engine relays
        // (not addressed to her), and forwards to Bob via her own transport.
        let (alice, a_transport) = alice_engine().await;

        let carol_t = Arc::new(SimulatedTransport::new(
            "sim-carol",
            "Sim C",
            SimConfig::default(),
        ));
        let carol_manager = Arc::new(TransportManager::new());
        carol_manager.register(carol_t.clone()).await.unwrap();
        let carol = MessageEngine::new(
            MessageEngineConfig {
                node_id: [0xCC; 32],
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            carol_manager,
        );

        let env = envelope_for(BOB, MessagePriority::P2, b"relay me to bob");
        alice.send_message(env).await.unwrap();

        // 1) Alice→Carol: delivered to Carol's engine — NOT addressed to her → relayed.
        let outcome = pump_one(&a_transport, &carol).await.unwrap();
        assert_eq!(outcome, InboundOutcome::Relayed);

        // 2) Carol's engine relays to her transport's peer (Bob) after the loop picks it up.
        // Bob = recipient; Carol's engine will send to BOB as target, which Carol's
        // simulated transport INGESTS into *Carol's* incoming stream (sim semantics),
        // so we drive it to Bob's engine from there.
        let bob = MessageEngine::new(
            MessageEngineConfig {
                node_id: BOB,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        let mut bob_delivered = bob.delivered_messages();
        let pumped = pump_one(&carol_t, &bob).await.unwrap();
        assert_eq!(pumped, InboundOutcome::Delivered);
        let delivered = tokio::time::timeout(Duration::from_millis(500), bob_delivered.recv())
            .await
            .expect("bob delivery")
            .expect("event");
        assert_eq!(delivered.payload, b"relay me to bob");

        // Carol's relay metric counts the hop.
        assert!(carol.metrics().relayed >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn build_ack_produces_ack_envelope() {
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        let env = envelope_for(BOB, MessagePriority::P4, b"req");
        let ack = engine.build_ack(&env);
        assert_eq!(ack.payload_type, ContentType::Ack);
        assert_eq!(ack.payload_ref, Some(env.message_id));
        assert_eq!(ack.sender_id, env.recipient_id);
        assert_eq!(ack.recipient_id, env.sender_id);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn emergency_provider_defaults_to_noop_and_switches() {
        // EMERG-001 AC-11: the engine is mechanically no-op on emergencies
        // until a provider is armed via set_emergency_provider.
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        // Default is unarmed (NoopEmergencyProvider).
        assert!(!engine.emergency.read().unwrap().armed());

        // Arm a real gateway-backed provider; the seam flips.
        let gw = crate::emergency::EmergencyGateway::new(
            crate::identity::trust_store::TrustStore::new(),
            64,
        );
        engine.set_emergency_provider(Arc::new(gw));
        assert!(engine.emergency.read().unwrap().armed());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn emergency_gate_drops_unverifiable_alert_no_reply() {
        // EMERG-001 AC-10: an armed gateway's receive-path gate drops an
        // unverifiable EmergencyAlert without delivery, relay, or any reply.
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        engine.set_emergency_provider(Arc::new(crate::emergency::EmergencyGateway::new(
            crate::identity::trust_store::TrustStore::new(),
            16,
        )));
        let mut delivered = engine.delivered_messages();

        // A forged EmergencyAlert: no auth chain, garbage body, any sender.
        let mut env = envelope_for(BOB, MessagePriority::P3, b"forged");
        env.payload_type = ContentType::EmergencyAlert;
        env.recipient_id = crate::protocol::EMERGENCY_BROADCAST.to_vec();
        env.payload_size = env.payload.len() as u64;
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);

        let outcome = engine
            .process_incoming(IncomingMessage {
                payload: codec::encode(&env).unwrap(),
                peer_id: PeerId(BOB),
                received_at: std::time::Instant::now(),
                transport_id: "sim".into(),
            })
            .await
            .unwrap();
        assert_eq!(outcome, InboundOutcome::EmergencyDropped);
        // No local delivery (nothing surfaced to the app layer).
        assert!(
            tokio::time::timeout(Duration::from_millis(100), delivered.recv())
                .await
                .is_err(),
            "unverifiable alert must not surface"
        );
        assert!(engine.metrics().dropped_emergency_auth >= 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn emergency_gate_unarmed_noop_keeps_behavior_identical() {
        // EMERG-001 AC-11: with the default Noop provider (unarmed), the same
        // forged EmergencyAlert flows through the normal pipeline unchanged —
        // the seam adds no behavior to a node that has not opted in.
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        assert!(!engine.emergency.read().unwrap().armed());

        let mut env = envelope_for(BOB, MessagePriority::P3, b"forged");
        env.payload_type = ContentType::EmergencyAlert;
        env.recipient_id = crate::protocol::EMERGENCY_BROADCAST.to_vec();
        env.payload_size = env.payload.len() as u64;
        env.payload_hash = Envelope::compute_payload_hash(&env.payload);

        // Broadcast recipient → delivered locally (identical to pre-EMERG-001).
        let outcome = engine
            .process_incoming(IncomingMessage {
                payload: codec::encode(&env).unwrap(),
                peer_id: PeerId(BOB),
                received_at: std::time::Instant::now(),
                transport_id: "sim".into(),
            })
            .await
            .unwrap();
        assert_eq!(outcome, InboundOutcome::Delivered);
        assert_eq!(engine.metrics().dropped_emergency_auth, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn emergency_gate_downgrades_fourth_sos_to_p3() {
        // EMERG-RT-002: the receive gate enforces the AC-5 allowance. The first
        // three SOS from one sender are accepted at P0; the 4th within the
        // rolling window is still delivered but downgraded to P3 (degraded
        // delivery), with audit metrics incremented.
        let engine = MessageEngine::new(
            MessageEngineConfig {
                node_id: ALICE,
                gc_interval_secs: 3600,
                ..Default::default()
            },
            Arc::new(MemoryStorage::new()),
            Arc::new(DevCryptoProvider::new()),
            Arc::new(TransportManager::new()),
        );
        engine.set_emergency_provider(Arc::new(crate::emergency::EmergencyGateway::new(
            crate::identity::trust_store::TrustStore::new(),
            16,
        )));
        let mut delivered = engine.delivered_messages();

        let sos_msg = crate::emergency::model::SosMessage {
            format_version: 1,
            latitude: 23_022_500,
            longitude: 72_571_250,
            accuracy_m: 10,
            location_source: crate::emergency::model::LocationSource::Gps,
            timestamp: 1_000,
            kind: crate::emergency::model::SosKind::Sos,
            original_message_id: None,
            reason: Some(crate::emergency::model::SosReason::Injury),
        };
        let sos_payload = crate::emergency::encode_sos(&sos_msg).unwrap();

        let mut priorities = Vec::new();
        let mut delivered_count = 0;
        for _ in 0..4 {
            let mut env = envelope_for(ALICE, MessagePriority::P0, &sos_payload);
            env.payload_type = ContentType::Sos;
            env.payload_size = sos_payload.len() as u64;
            env.payload_hash = Envelope::compute_payload_hash(&sos_payload);
            let outcome = engine
                .process_incoming(IncomingMessage {
                    payload: codec::encode(&env).unwrap(),
                    peer_id: PeerId(BOB),
                    received_at: std::time::Instant::now(),
                    transport_id: "sim".into(),
                })
                .await
                .unwrap();
            assert!(matches!(
                outcome,
                InboundOutcome::Delivered | InboundOutcome::Relayed
            ));
            delivered_count += 1;
            if let Ok(received) =
                tokio::time::timeout(Duration::from_millis(200), delivered.recv()).await
            {
                priorities.push(received.unwrap().priority);
            }
        }
        assert_eq!(delivered_count, 4, "all four SOS must be delivered");
        assert_eq!(
            priorities.len(),
            delivered_count,
            "each delivered SOS surfaced"
        );
        assert_eq!(priorities[0], MessagePriority::P0);
        assert_eq!(priorities[1], MessagePriority::P0);
        assert_eq!(priorities[2], MessagePriority::P0);
        // 4th-in-window downgraded to P3 (EMERG-RT-002 / AC-5).
        assert_eq!(priorities[3], MessagePriority::P3);
        assert_eq!(engine.metrics().emergency_sos_rate_limited, 1);
    }
}
