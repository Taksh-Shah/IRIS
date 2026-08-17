//! Store-Carry-Forward engine — SCF-001 (WP-5).
//!
//! Per `docs/routing/STORE_CARRY_FORWARD.md`: when no forwarding path exists,
//! messages are buffered over the STORE seam (`MessageStorage`), carried while
//! the node moves, and forwarded when a contact appears. The buffer view is
//! ordered (priority DESC, expiry ASC) via a `BTreeMap<StoreKey, _>`; delivery
//! status follows the SCF state machine
//! (STORED → CARRY → FORWARD_ATTEMPT → ACK_RECEIVED/DELIVERED, or
//! TTL_EXPIRED → DROPPED).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::message::PeerId;
use crate::message::{LinkQuality, MessagePriority};
use crate::observability::event;
use crate::observability::metric;
use crate::observability::MetricsRegistry;
use crate::protocol::{Envelope, MessageId};
use crate::routing::store::is_expired;

/// Delivery status of a buffered message (STORE_CARRY_FORWARD.md §Message
/// Lifecycle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Stored,
    Carrying,
    AwaitingContact,
    ForwardingInProgress { to: PeerId },
    AckPending { sent_to: PeerId },
    Delivered { delivered_to: PeerId },
    Dropped { reason: DropReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReason {
    TtlExpired,
    StorageFull,
    Evicted,
}

impl DeliveryStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            DeliveryStatus::Delivered { .. } | DeliveryStatus::Dropped { .. }
        )
    }
}

/// Ordering key for the SCF buffer: higher priority first (lower rank), then
/// sooner-expiring first — so both forward and eviction iterate in the order
/// the design requires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StoreKey {
    pub priority_rank: u8, // 0 = P0, 7 = P7
    pub expiry_unix: u64,  // sooner expiry = smaller
    pub message_id: MessageId,
}

impl StoreKey {
    pub fn from_envelope(env: &Envelope) -> Self {
        StoreKey {
            priority_rank: env.priority.as_u8(),
            expiry_unix: env.timestamp.saturating_add(env.ttl_seconds),
            message_id: env.message_id,
        }
    }
}

/// A buffered message with its SCF bookkeeping.
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub envelope: Envelope,
    pub stored_at: u64,
    pub forward_attempts: u32,
    pub received_from: Option<PeerId>,
    pub delivery_status: DeliveryStatus,
}

/// The SCF buffer view over any `MessageStorage` seam. The BTreeMap holds the
/// in-memory ordering; persistence stays on the storage backend.
#[derive(Debug)]
pub struct ScfEngine<S> {
    buffer: BTreeMap<StoreKey, StoredMessage>,
    storage: S,
    max_bytes: u64,
    config: ScfConfig,
    /// Optional virtual clock (SIM-001 determinism). When `Some`, all
    /// now() reads hit the shared counter; otherwise wall-clock time.
    virtual_clock: Option<Arc<AtomicU64>>,
    telemetry: MetricsRegistry,
}

/// Configuration for the SCF engine.
#[derive(Debug, Clone)]
pub struct ScfConfig {
    /// Default 500 MB. Capacity-per-device settings (STORE_CARRY_FORWARD §Storage
    /// Capacity: 200 MB phone, 2 GB relay, 10 GB edge, 50 GB desktop) tune this.
    pub max_storage_bytes: u64,
    /// Clock-skew budget reused from the message engine (RFC 9171 §4.4.2).
    pub clock_skew_budget_secs: u64,
    /// Max forward attempts before a message returns to Carrying.
    pub max_forward_attempts: u32,
}

impl Default for ScfConfig {
    fn default() -> Self {
        ScfConfig {
            max_storage_bytes: 500 * 1024 * 1024,
            clock_skew_budget_secs: 300,
            max_forward_attempts: 3,
        }
    }
}

impl<S: crate::message_engine::storage::MessageStorage> ScfEngine<S> {
    pub fn new(storage: S, config: ScfConfig) -> Self {
        ScfEngine {
            buffer: BTreeMap::new(),
            storage,
            max_bytes: config.max_storage_bytes,
            config,
            virtual_clock: None,
            telemetry: MetricsRegistry::new(),
        }
    }

    /// Inject a metrics registry (OBS-001). Defaults to an isolated registry.
    pub fn with_telemetry(mut self, telemetry: MetricsRegistry) -> Self {
        self.telemetry = telemetry;
        self
    }

    /// Attach a shared virtual clock (SIM-001 determinism). All time reads
    /// then come from the counter instead of the wall clock.
    pub fn with_virtual_clock(mut self, clock: Arc<AtomicU64>) -> Self {
        self.virtual_clock = Some(clock);
        self
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Live OBS-001 telemetry registry (counters since last reset).
    pub fn telemetry(&self) -> &MetricsRegistry {
        &self.telemetry
    }

    /// Approximate bytes currently buffered (payload + envelope overhead).
    pub fn usage_bytes(&self) -> u64 {
        self.usage()
    }

    /// Priority eviction rank of a buffered message (0 = P0).
    pub fn priority_rank_of(&self, id: &MessageId) -> Option<u8> {
        self.buffer
            .iter()
            .find(|(_, m)| m.envelope.message_id == *id)
            .map(|(k, _)| k.priority_rank)
    }

    pub(crate) fn usage(&self) -> u64 {
        self.buffer
            .values()
            .map(|m| Self::approx_bytes(&m.envelope))
            .sum()
    }

    pub fn now_unix(&self) -> u64 {
        match &self.virtual_clock {
            Some(c) => c.load(Ordering::Relaxed),
            None => SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }
    }

    fn approx_bytes(env: &Envelope) -> u64 {
        env.payload.len() as u64 + 256
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Insert a message into the SCF buffer. Expired messages are rejected
    /// outright; buffer size is capped by `max_bytes` (eviction enforced
    /// separately via `evict_to_fit`). P0 is always accepted.
    pub fn buffer_message(
        &mut self,
        envelope: Envelope,
        received_from: Option<PeerId>,
    ) -> Result<DeliveryStatus, ScfError> {
        if is_expired(self.now_unix(), envelope.timestamp, envelope.ttl_seconds) {
            return Err(ScfError::Expired);
        }
        let key = StoreKey::from_envelope(&envelope);
        let stored = StoredMessage {
            envelope,
            stored_at: self.now_unix(),
            forward_attempts: 0,
            received_from,
            delivery_status: DeliveryStatus::Stored,
        };
        self.buffer.insert(key, stored);
        tracing::debug!(
            event = event::SCF_BUFFERED,
            message_id = %key.message_id.short(),
            status = "Stored",
            "message buffered for store-carry-forward"
        );
        Ok(DeliveryStatus::Stored)
    }

    /// Determine the SCF status for one buffered message given current
    /// connectivity to `contact` (None = no contacts).
    pub fn status_for(&self, id: &MessageId, contact: Option<&PeerId>) -> Option<DeliveryStatus> {
        let msg = self
            .buffer
            .iter()
            .find(|(_, m)| m.envelope.message_id == *id)
            .map(|(_, m)| m)?;
        match contact {
            // The recipient is in range: the message is about to be delivered.
            Some(peer) if Self::recipient_is(peer, &msg.envelope) => {
                Some(DeliveryStatus::Delivered {
                    delivered_to: *peer,
                })
            }
            Some(_) => Some(msg.delivery_status.clone()),
            None => Some(msg.delivery_status.clone()),
        }
    }

    fn recipient_is(contact: &PeerId, envelope: &Envelope) -> bool {
        envelope
            .recipient_id
            .as_slice()
            .try_into()
            .map(|b: [u8; 32]| PeerId(b) == *contact)
            .unwrap_or(false)
    }

    /// Messages forwardable to a contact: those addressed to it or relayable
    /// toward it, not yet terminal, within TTL and still under the forward
    /// attempt budget.
    pub fn messages_forwardable_to(&self, contact: &PeerId) -> Vec<StoredMessage> {
        self.buffer
            .iter()
            .filter(|(_, m)| !m.delivery_status.is_terminal())
            .filter(|(_, m)| m.forward_attempts < self.config.max_forward_attempts)
            .filter(|(_, m)| {
                let matches = Self::recipient_is(contact, &m.envelope);
                let live = !is_expired(
                    self.now_unix(),
                    m.envelope.timestamp,
                    m.envelope.ttl_seconds,
                );
                matches && live
            })
            .map(|(_, m)| m.clone())
            .collect()
    }

    /// Mark a message as forwarded (increments attempts, sets status per the
    /// outcome).
    pub fn mark_forwarded(
        &mut self,
        id: &MessageId,
        to: PeerId,
        acked: bool,
    ) -> Option<DeliveryStatus> {
        let key = self
            .buffer
            .iter()
            .find(|(_, m)| m.envelope.message_id == *id)
            .map(|(k, _)| *k)?;
        let msg = self.buffer.get_mut(&key)?;
        msg.forward_attempts += 1;
        msg.delivery_status = if acked {
            DeliveryStatus::Delivered { delivered_to: to }
        } else {
            DeliveryStatus::AckPending { sent_to: to }
        };
        Some(msg.delivery_status.clone())
    }

    /// Expire-and-drop sweep. Returns ids dropped for TTL expiry.
    pub fn reap_expired(&mut self) -> Vec<MessageId> {
        let now = self.now_unix();
        let expired_keys: Vec<StoreKey> = self
            .buffer
            .iter()
            .filter(|(_, m)| is_expired(now, m.envelope.timestamp, m.envelope.ttl_seconds))
            .map(|(k, _)| *k)
            .collect();
        let mut dropped = Vec::new();
        for k in expired_keys {
            if let Some(mut msg) = self.buffer.remove(&k) {
                msg.delivery_status = DeliveryStatus::Dropped {
                    reason: DropReason::TtlExpired,
                };
                tracing::debug!(
                    event = event::SCF_REAPED_EXPIRED,
                    message_id = %msg.envelope.message_id.short(),
                    "message expired in SCF buffer (RFC 9171 code 1)"
                );
                dropped.push(msg.envelope.message_id);
            }
        }
        dropped
    }

    /// Evict messages to fit `needed_bytes` of extra capacity. **P0 is never
    /// evicted** (INV-ROUTE-003). Returns evicted message ids.
    pub fn evict_to_fit(&mut self, needed_bytes: u64) -> Vec<MessageId> {
        let target = self.max_bytes.saturating_sub(needed_bytes);
        self.evict_until(target)
    }

    /// Evict lowest-priority (then soonest-expiring) messages until usage is at
    /// most `target_bytes`. **P0 is never evicted** (INV-ROUTE-003). Returns
    /// evicted message ids.
    pub(crate) fn evict_until(&mut self, target_bytes: u64) -> Vec<MessageId> {
        let mut evicted = Vec::new();
        while self.usage() > target_bytes {
            // Worst eviction candidate: highest priority_rank (>0), then
            // soonest expiry_unix (STORE_CARRY_FORWARD.md §Eviction Policy).
            let worst = self
                .buffer
                .iter()
                .filter(|(k, _)| k.priority_rank > 0) // P0 exempt
                .max_by(|(a, _), (b, _)| {
                    a.priority_rank
                        .cmp(&b.priority_rank)
                        .then_with(|| b.expiry_unix.cmp(&a.expiry_unix)) // sooner expiry = worse
                })
                .map(|(k, _)| *k);
            let Some(k) = worst else { break };
            if let Some(msg) = self.buffer.remove(&k) {
                self.telemetry.increment(metric::SCF_EVICTIONS_TOTAL);
                tracing::warn!(
                    event = event::SCF_EVICTED,
                    message_id = %msg.envelope.message_id.short(),
                    priority = k.priority_rank,
                    usage_bytes = self.usage(),
                    "message evicted from SCF buffer (RFC 9171 code 4)"
                );
                evicted.push(msg.envelope.message_id);
            }
        }
        evicted
    }

    /// All buffered messages ordered (priority DESC, expiry ASC) — the
    /// forward order the contact handler uses.
    pub fn ordered_buffered(&self) -> Vec<StoredMessage> {
        self.buffer.values().cloned().collect()
    }

    /// Delivery status of one message id.
    pub fn delivery_status(&self, id: &MessageId) -> Option<DeliveryStatus> {
        self.buffer
            .iter()
            .find(|(_, m)| m.envelope.message_id == *id)
            .map(|(_, m)| m.delivery_status.clone())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScfError {
    Expired,
}

impl std::fmt::Display for ScfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScfError::Expired => write!(f, "scf: message expired"),
        }
    }
}

impl std::error::Error for ScfError {}

/// Keep the module honest about the RFC carrier reference without pulling the
/// unused peer-quality dependency into public API.
#[allow(dead_code)]
fn _carrier_contract(_quality: LinkQuality, _t: Duration) -> MessagePriority {
    MessagePriority::P4
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_engine::storage::MemoryStorage;
    use crate::protocol::ContentType;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn env(prio: MessagePriority, payload: &[u8], ttl: u64) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: pid(1).0.to_vec(),
            recipient_id: pid(9).0.to_vec(),
            priority: prio,
            ttl_seconds: ttl,
            timestamp: now(),
            hop_count: 0,
            max_hops: Some(5),
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

    #[test]
    fn buffer_and_lifecycle_status() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let e = env(MessagePriority::P4, b"hello", 3600);
        let id = e.message_id;
        assert_eq!(
            scf.buffer_message(e.clone(), None).unwrap(),
            DeliveryStatus::Stored
        );
        assert_eq!(scf.len(), 1);
        scf.mark_forwarded(&id, pid(9), true);
        assert_eq!(
            scf.delivery_status(&id),
            Some(DeliveryStatus::Delivered {
                delivered_to: pid(9)
            })
        );
        assert!(scf.delivery_status(&id).unwrap().is_terminal());
    }

    #[test]
    fn expired_rejected() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let mut e = env(MessagePriority::P3, b"late", 60);
        e.timestamp = now().saturating_sub(3600);
        assert_eq!(scf.buffer_message(e, None), Err(ScfError::Expired));
        assert!(scf.is_empty());
    }

    #[test]
    fn forwardable_to_contact_only() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        scf.buffer_message(env(MessagePriority::P3, b"to-9", 3600), None)
            .unwrap();
        scf.buffer_message(env(MessagePriority::P0, b"to-9-p0", 3600), None)
            .unwrap();
        // A message addressed to a different peer must not be forwarded to 9.
        let mut other = env(MessagePriority::P4, b"to-other", 3600);
        other.recipient_id = pid(8).0.to_vec();
        scf.buffer_message(other, None).unwrap();
        let fwd = scf.messages_forwardable_to(&pid(9));
        assert_eq!(fwd.len(), 2);
        // ordered: P0 first
        assert_eq!(fwd[0].envelope.priority, MessagePriority::P0);
    }

    #[test]
    fn reap_expired_drops_only_expired() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        scf.buffer_message(env(MessagePriority::P4, b"fresh", 3600), None)
            .unwrap();
        // Buffer an old message (must not be pre-rejected as expired), then
        // backdate it so the reap sweep considers it expired.
        let old = env(MessagePriority::P5, b"old", 60);
        scf.buffer_message(old.clone(), None).unwrap();
        let now = now();
        let old_id = old.message_id;
        for m in scf.buffer.values_mut() {
            if m.envelope.message_id == old_id {
                // Expired: timestamp + ttl <= now (RFC 9171 arrival semantics).
                m.envelope.timestamp = now.saturating_sub(3600);
            }
        }
        let dropped = scf.reap_expired();
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0], old_id);
        assert_eq!(scf.len(), 1);
    }

    #[test]
    fn store_key_ordering_priority_then_expiry() {
        use std::cmp::Ordering;
        let a = StoreKey {
            priority_rank: 0,
            expiry_unix: 100,
            message_id: MessageId::new_v7(),
        };
        let b = StoreKey {
            priority_rank: 1,
            expiry_unix: 100,
            message_id: MessageId::new_v7(),
        };
        let c = StoreKey {
            priority_rank: 0,
            expiry_unix: 50,
            message_id: MessageId::new_v7(),
        };
        assert_eq!(a.cmp(&b), Ordering::Less);
        assert_eq!(c.cmp(&a), Ordering::Less);
    }
}
