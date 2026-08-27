//! Store-Carry-Forward engine — SCF-001 (WP-5).
//!
//! Per `docs/routing/STORE_CARRY_FORWARD.md`: when no forwarding path exists,
//! messages are buffered over the STORE seam (`MessageStorage`) and forwarded
//! when a contact appears. The buffer view is ordered via a
//! `BTreeMap<StoreKey, _>` (see `StoreKey`'s `Ord` impl for the exact order).
//!
//! DTN-22: the state machine actually implemented is STORED ->
//! (`AckPending`) -> DELIVERED, or TTL_EXPIRED — and delivery is a *removal*
//! event (DTN-5: a terminal message is removed from `buffer` immediately,
//! not retained with a terminal status), not a status a caller ever reads
//! back. Three additional states the spec describes (`Carrying`,
//! `AwaitingContact`, `ForwardingInProgress`) needed a mobility/contact-
//! presence signal no part of this codebase produces, and a `Dropped`
//! status (for TTL expiry / eviction) was, by the same DTN-5 removal-on-
//! terminal logic, never actually constructed either — both were deleted
//! rather than left as decoration that misrepresents implemented behaviour
//! (the same principle ROUT-34, this tier, applied to dead placeholder
//! code). TTL expiry and eviction remain fully observable via
//! `tracing` (`SCF_REAPED_EXPIRED`, `SCF_EVICTED`) at the point of removal.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::message::PeerId;
use crate::message::{LinkQuality, MessagePriority};
use crate::observability::event;
use crate::observability::metric;
use crate::observability::MetricsRegistry;
use crate::protocol::{Envelope, MessageId};
use crate::routing::scf_eviction::ScfEvictionPolicy;
use crate::routing::store::ttl_expired;

/// Delivery status of a buffered message (STORE_CARRY_FORWARD.md §Message
/// Lifecycle) — see the module doc comment (DTN-22) for which of the
/// spec's states are actually implemented and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryStatus {
    Stored,
    AckPending { sent_to: PeerId },
    Delivered { delivered_to: PeerId },
}

impl DeliveryStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, DeliveryStatus::Delivered { .. })
    }
}

/// Ordering key for the SCF buffer.
///
/// DTN-9: `Ord` is implemented by hand (not derived) so that ascending
/// `BTreeMap` iteration (`.values()`, `ordered_buffered()`) yields
/// priority-rank ASC / soonest-expiry-last order, and — the property
/// `evict_until` actually depends on — descending iteration
/// (`.keys().next_back()`) yields exactly the worst eviction candidate:
/// highest `priority_rank` (least urgent class) first, then, within that
/// class, the *soonest-expiring* entry (STORE_CARRY_FORWARD.md §Eviction
/// Policy). A plain `#[derive(Ord)]` on `(priority_rank, expiry_unix)`
/// cannot do both at once — reversing a composite ascending order reverses
/// every field uniformly, but "worst" here needs `expiry_unix` reversed
/// relative to `priority_rank`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreKey {
    pub priority_rank: u8, // 0 = P0, 7 = P7
    pub expiry_unix: u64,
    pub message_id: MessageId,
}

impl PartialOrd for StoreKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StoreKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority_rank
            .cmp(&other.priority_rank)
            .then_with(|| other.expiry_unix.cmp(&self.expiry_unix))
            .then_with(|| self.message_id.cmp(&other.message_id))
    }
}

impl StoreKey {
    /// `stored_at` is this node's own clock reading at buffering time.
    ///
    /// DTN-6: `expiry_unix` must be derived from locally observed arrival,
    /// not the sender's claimed `timestamp` — otherwise a forged
    /// far-future `timestamp` (or an unclamped `ttl_seconds`) lets a
    /// message claim to expire arbitrarily late, so the eviction
    /// comparator above always ranks it "not yet worth evicting" ahead of
    /// honest, soon-expiring traffic. `env.ttl_seconds` is expected to
    /// already be clamped (`buffer_message` does this before calling here).
    fn new(env: &Envelope, stored_at: u64) -> Self {
        StoreKey {
            // DTN-11: sourced from ScfEvictionPolicy::eviction_weight (the
            // policy module's canonical priority->rank mapping) rather
            // than re-deriving the same value inline — the two happen to
            // agree numerically today (both are just the priority's u8),
            // but this makes the policy module load-bearing instead of
            // unreachable decoration.
            priority_rank: ScfEvictionPolicy::eviction_weight(env.priority),
            expiry_unix: stored_at.saturating_add(env.ttl_seconds),
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
    /// DTN-20: when the most recent forward attempt was made (unix secs).
    /// `AckPending` was previously an absorbing state — nothing ever timed
    /// it out, so once `mark_forwarded(id, peer, false)` records `peer` in
    /// `offered` (DTN-3), that specific peer could never be retried again
    /// even if the ack frame was simply lost. Feeds
    /// `reap_ack_timeouts`'s backoff.
    pub last_forward_attempt: Option<u64>,
}

/// The SCF buffer view over any `MessageStorage` seam. The BTreeMap holds the
/// in-memory ordering; persistence stays on the storage backend.
#[derive(Debug)]
pub struct ScfEngine<S> {
    buffer: BTreeMap<StoreKey, StoredMessage>,
    /// DTN-2: `MessageId` -> its current `StoreKey`, so by-id accessors
    /// (`priority_rank_of`, `status_for`, `mark_forwarded`,
    /// `delivery_status`) are O(1) instead of an O(n) `.iter().find()`
    /// scan — and, more importantly, `buffer_message` can detect a
    /// replayed `MessageId` and replace its existing entry instead of
    /// inserting a second one with a different `StoreKey`.
    index: HashMap<MessageId, StoreKey>,
    /// DTN-8: incrementally maintained, so `usage()` is O(1) instead of an
    /// O(n) fold over the whole buffer on every call (`evict_until` used to
    /// call it twice per evicted message).
    total_bytes: u64,
    /// DTN-3: `message_id` -> peers this message has already been offered
    /// to. Without this, a flapping link (repeated contact events with the
    /// same peer) re-offers the same message every reconnect, and — because
    /// the old repeat-limiter was a single global `forward_attempts`
    /// counter shared across every peer — meeting a handful of *different*
    /// peers could exhaust that counter and permanently lock the message
    /// out for everyone. Entries are removed alongside the message itself
    /// (`remove_entry`), so this cannot outlive what it is about.
    offered: HashMap<MessageId, HashSet<PeerId>>,
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
    /// DTN-2: hard cap on distinct buffered messages, independent of byte
    /// accounting — bounds a duplicate-id/tiny-payload amplification
    /// attack (many entries, each cheap in accounted bytes) that a pure
    /// byte ceiling does not.
    pub max_messages: usize,
}

impl Default for ScfConfig {
    fn default() -> Self {
        ScfConfig {
            max_storage_bytes: 500 * 1024 * 1024,
            clock_skew_budget_secs: 300,
            max_forward_attempts: 3,
            max_messages: 100_000,
        }
    }
}

/// DTN-6: upper bound on a message's claimed TTL, regardless of what the
/// sender requests. Without this, `ttl_seconds = u64::MAX` makes a message
/// immortal (its computed expiry saturates to `u64::MAX`, so it can never
/// TTL-expire) even though the far-future-*timestamp* half of this attack
/// is separately closed by `message_engine::expiry::is_expired`'s
/// skew-suspect handling (ROUT-5).
///
/// `pub` (TAK-2): the persistent `iris-storage` PG store needs the exact
/// same clamp on ingest — the finding's own text requires "the same
/// reasoning" in both layers so they agree, and a second, independently
/// chosen constant would be free to drift from this one.
pub const MAX_TTL_SECS: u64 = 30 * 24 * 3600; // 30 days

impl<S: crate::message_engine::storage::MessageStorage> ScfEngine<S> {
    pub fn new(storage: S, config: ScfConfig) -> Self {
        ScfEngine {
            buffer: BTreeMap::new(),
            index: HashMap::new(),
            total_bytes: 0,
            offered: HashMap::new(),
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

    /// DTN-11: crate-visible so `scf_eviction.rs`'s `with_device_class` can
    /// apply a device-class ceiling without exposing raw field mutation as
    /// public API.
    pub(crate) fn set_max_bytes(&mut self, bytes: u64) {
        self.max_bytes = bytes;
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
        self.index.get(id).map(|k| k.priority_rank)
    }

    /// DTN-8: O(1) — see `total_bytes`.
    pub(crate) fn usage(&self) -> u64 {
        self.total_bytes
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

    /// DTN-10: accounts every heap-allocated field, not just the payload —
    /// `auth_cert_chain`, `routing_hints` and `encryption_hdr` were
    /// previously invisible to capacity accounting, so a node could be
    /// resident far beyond `max_bytes` while `usage()` reported it as
    /// healthy. `FIXED_OVERHEAD_BYTES` (192) is chosen so that a 32-byte
    /// `sender_id`/`recipient_id` with no optional fields — the shape of
    /// every existing test envelope — accounts identically to the old
    /// `payload.len() + 256` formula (192 + 32 + 32 = 256): this only
    /// *increases* the accounted size when the previously-invisible fields
    /// are actually present.
    const FIXED_OVERHEAD_BYTES: u64 = 192;

    /// `pub(crate)`: DTN-25's bandwidth-aware ranking (`scf_contact.rs`)
    /// needs the same per-message byte estimate this module uses for
    /// capacity accounting.
    pub(crate) fn approx_bytes(env: &Envelope) -> u64 {
        let mut n = Self::FIXED_OVERHEAD_BYTES
            + env.sender_id.len() as u64
            + env.recipient_id.len() as u64
            + env.payload.len() as u64;
        if env.signature.is_some() {
            n += 64;
        }
        if env.encryption_hdr.is_some() {
            n += 32 + 12 + 4; // ephemeral_pubkey + nonce + key_id (worst case)
        }
        if let Some(hints) = &env.routing_hints {
            n += hints
                .last_known_region
                .as_ref()
                .map(|s| s.len() as u64)
                .unwrap_or(0);
            if hints.gateway_seen_via.is_some() {
                n += 32;
            }
            n += hints
                .preferred_relays
                .as_ref()
                .map(|v| (v.len() * 32) as u64)
                .unwrap_or(0);
        }
        if let Some(chain) = &env.auth_cert_chain {
            n += chain.iter().map(|c| c.len() as u64).sum::<u64>();
        }
        n
    }

    /// Remove one entry by key: updates the index and the incremental byte
    /// total together, so they can never drift apart. The single place
    /// every removal path (`buffer_message`'s replace, `mark_forwarded`'s
    /// delivered-removal (DTN-5), `reap_expired`, `evict_until`) goes
    /// through.
    fn remove_entry(&mut self, key: &StoreKey) -> Option<StoredMessage> {
        let msg = self.buffer.remove(key)?;
        self.index.remove(&msg.envelope.message_id);
        self.offered.remove(&msg.envelope.message_id); // DTN-3: cannot outlive the message
        self.total_bytes = self
            .total_bytes
            .saturating_sub(Self::approx_bytes(&msg.envelope));
        Some(msg)
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
        mut envelope: Envelope,
        received_from: Option<PeerId>,
    ) -> Result<DeliveryStatus, ScfError> {
        let now = self.now_unix();
        // DTN-6: clamp the claimed TTL in place so every downstream reader
        // of `envelope.ttl_seconds` (this function, `messages_forwardable_to`,
        // `reap_expired`) sees the clamped value automatically. An
        // unclamped `ttl_seconds = u64::MAX` would make `StoreKey::new`'s
        // `expiry_unix` saturate to `u64::MAX` below — immortal under both
        // the TTL sweep and eviction ranking. (The complementary
        // far-future-*timestamp* attack is already closed by
        // `message_engine::expiry::is_expired`'s skew-suspect handling,
        // ROUT-5 — `ttl_expired` below still fails closed on that.)
        envelope.ttl_seconds = envelope.ttl_seconds.min(MAX_TTL_SECS);
        if ttl_expired(now, envelope.timestamp, envelope.ttl_seconds) {
            return Err(ScfError::Expired);
        }

        let message_id = envelope.message_id;
        // DTN-2: a replayed MessageId (possibly with a bumped ttl_seconds
        // to dodge the identical-key case) replaces its existing entry
        // rather than adding a second, independently-tracked one.
        if let Some(old_key) = self.index.get(&message_id).copied() {
            self.remove_entry(&old_key);
        }

        let need = Self::approx_bytes(&envelope);

        // DTN-1: actually enforce the byte ceiling — previously
        // `buffer_message` checked TTL only and never compared usage
        // against `max_bytes`, so the buffer grew without bound.
        if self.total_bytes.saturating_add(need) > self.max_bytes {
            let target = self.max_bytes.saturating_sub(need.min(self.max_bytes));
            self.evict_until(target);
            if self.total_bytes.saturating_add(need) > self.max_bytes {
                // DTN-7: this reject path is reached even when the buffer
                // holds only P0 (evict_until has nothing left it may
                // touch) — so a P0-only flood is now bounded at
                // `max_bytes` instead of growing until OOM, without
                // needing a separate P0 sub-quota: the incoming message is
                // simply refused once the ceiling is genuinely full.
                return Err(ScfError::StorageFull);
            }
        }
        // DTN-2: message-COUNT cap, independent of byte accounting — bounds
        // a many-tiny-messages amplification a byte ceiling alone would not.
        if self.buffer.len() >= self.config.max_messages {
            return Err(ScfError::StorageFull);
        }

        let key = StoreKey::new(&envelope, now);
        let stored = StoredMessage {
            envelope,
            stored_at: now,
            forward_attempts: 0,
            received_from,
            delivery_status: DeliveryStatus::Stored,
            last_forward_attempt: None,
        };
        self.index.insert(message_id, key);
        self.total_bytes += need;
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
    /// DTN-21: this used to fabricate `Delivered` whenever `contact`
    /// happened to be the envelope's recipient — purely because the peer
    /// was in range, before any forward was attempted or ack received.
    /// `Delivered` is a terminal state STORE_CARRY_FORWARD.md defines as
    /// reached only via `ACK_RECEIVED`; returning it on mere proximity
    /// meant any caller trusting this for a UI receipt, a delivery metric,
    /// or a decision to stop carrying acted on a delivery that had not
    /// happened — a false positive a life-safety application cannot
    /// afford. `contact` is retained in the signature for API
    /// compatibility (this function had zero callers at the time of this
    /// fix) but no longer special-cased.
    pub fn status_for(&self, id: &MessageId, contact: Option<&PeerId>) -> Option<DeliveryStatus> {
        let _ = contact;
        let key = self.index.get(id)?;
        let msg = self.buffer.get(key)?;
        Some(msg.delivery_status.clone())
    }

    /// Messages forwardable to a contact: those addressed to it or relayable
    /// toward it, not yet terminal, within TTL and still under the forward
    /// attempt budget.
    pub fn messages_forwardable_to(&self, contact: &PeerId) -> Vec<StoredMessage> {
        let now = self.now_unix();
        self.buffer
            .iter()
            .filter(|(_, m)| !m.delivery_status.is_terminal())
            .filter(|(_, m)| {
                // DTN-3: skip a peer this message has already been offered
                // to (dedup across repeated contact events with the same
                // peer). Deliberately *not* gated by the old global
                // `forward_attempts < max_forward_attempts` check any
                // more — that counter was shared across every peer, so
                // meeting a handful of different peers could exhaust it
                // and permanently lock the message out for everyone.
                // `forward_attempts` is still recorded by `mark_forwarded`
                // for telemetry/future retry-backoff use (DTN-20).
                !self
                    .offered
                    .get(&m.envelope.message_id)
                    .is_some_and(|peers| peers.contains(contact))
            })
            .filter(|(_, m)| {
                // DTN-4: any live, not-yet-offered message is a relay
                // candidate — not only the exact final recipient. The old
                // exact-match-only filter defeated store-carry-forward's
                // entire premise (STORE_CARRY_FORWARD.md §Core Concept /
                // §Vehicle Relay Scenario: a carrier relays messages that
                // are not addressed to it). Exact-recipient match keeps
                // its higher-confidence ranking downstream
                // (scf_contact.rs::delivery_probability's 0.9 vs 0.6).
                !ttl_expired(now, m.envelope.timestamp, m.envelope.ttl_seconds)
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
        let key = *self.index.get(id)?;
        // DTN-3: record the attempt so a later contact with the same peer
        // is filtered out of `messages_forwardable_to` instead of being
        // re-offered every reconnect.
        self.offered.entry(*id).or_default().insert(to);
        if acked {
            // DTN-5: `Delivered` is terminal (STORE_CARRY_FORWARD.md
            // §Message Lifecycle: ACK_RECEIVED -> DELIVERED -> remove from
            // store) — previously the entry was left in place forever,
            // still counted by `usage()`, the primary OOM vector on an
            // otherwise-healthy (non-partitioned) node.
            let mut msg = self.remove_entry(&key)?;
            msg.forward_attempts += 1;
            msg.delivery_status = DeliveryStatus::Delivered { delivered_to: to };
            Some(msg.delivery_status)
        } else {
            let now = self.now_unix(); // DTN-20: feeds reap_ack_timeouts's backoff
            let msg = self.buffer.get_mut(&key)?;
            msg.forward_attempts += 1;
            msg.last_forward_attempt = Some(now);
            msg.delivery_status = DeliveryStatus::AckPending { sent_to: to };
            Some(msg.delivery_status.clone())
        }
    }

    /// DTN-20: `AckPending` was previously an absorbing state — nothing
    /// ever timed it out, so a single lost ack frame (not a rejection, just
    /// a dropped packet on a lossy link) permanently excluded that peer
    /// from receiving the message again, since `mark_forwarded` had already
    /// recorded them in `offered` (DTN-3). Returns a message to `Stored`
    /// once it has been `AckPending` for longer than an exponentially
    /// backed-off timeout (scaled by `forward_attempts`, so a repeatedly
    /// unresponsive peer is retried less eagerly each time) — and, crucially,
    /// clears *that one peer* from `offered` so a retry with them specifically
    /// becomes possible again. Does not increment `forward_attempts` itself
    /// (only an actual `mark_forwarded` call does that); does not touch
    /// `offered` entries for any other peer. Not on an active timer today —
    /// no part of this crate schedules a production SCF tick yet (the same
    /// wiring gap `reap_expired`, DTN-1, was found in) — but is fully
    /// unit-tested and ready to be called from one.
    pub fn reap_ack_timeouts(&mut self, timeout_secs: u64) -> Vec<MessageId> {
        let now = self.now_unix();
        let mut to_reset: Vec<(StoreKey, MessageId, PeerId)> = Vec::new();
        for (key, msg) in self.buffer.iter() {
            if let DeliveryStatus::AckPending { sent_to } = &msg.delivery_status {
                let since = msg.last_forward_attempt.unwrap_or(msg.stored_at);
                let backoff = timeout_secs.saturating_mul(1u64 << msg.forward_attempts.min(16));
                if now.saturating_sub(since) >= backoff {
                    to_reset.push((*key, msg.envelope.message_id, *sent_to));
                }
            }
        }
        let mut reset = Vec::with_capacity(to_reset.len());
        for (key, id, peer) in to_reset {
            if let Some(msg) = self.buffer.get_mut(&key) {
                msg.delivery_status = DeliveryStatus::Stored;
            }
            if let Some(peers) = self.offered.get_mut(&id) {
                peers.remove(&peer);
            }
            reset.push(id);
        }
        reset
    }

    /// Expire-and-drop sweep. Returns ids dropped for TTL expiry.
    pub fn reap_expired(&mut self) -> Vec<MessageId> {
        let now = self.now_unix();
        let expired_keys: Vec<StoreKey> = self
            .buffer
            .iter()
            .filter(|(_, m)| ttl_expired(now, m.envelope.timestamp, m.envelope.ttl_seconds))
            .map(|(k, _)| *k)
            .collect();
        let mut dropped = Vec::new();
        for k in expired_keys {
            if let Some(msg) = self.remove_entry(&k) {
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
    /// most `target_bytes`. **P0 is never evicted by this function**
    /// (INV-ROUTE-003) — `buffer_message` is what actually bounds a P0-only
    /// flood (DTN-7), by rejecting new P0 messages once nothing here is
    /// left to evict. Returns evicted message ids.
    ///
    /// DTN-8/DTN-9: the worst candidate is now found via
    /// `.keys().next_back()` on `StoreKey`'s hand-written `Ord` (an O(log n)
    /// BTreeMap lookup) instead of an O(n) `max_by` scan repeated per
    /// evicted message; `usage()` is an O(1) field read (`total_bytes`)
    /// instead of an O(n) fold. Evicting m of n messages was O(m*n); it is
    /// now O(m log n).
    pub(crate) fn evict_until(&mut self, target_bytes: u64) -> Vec<MessageId> {
        let mut evicted = Vec::new();
        while self.total_bytes > target_bytes {
            // DTN-11: gate through ScfEvictionPolicy::is_evictable instead
            // of an inline `priority_rank > 0` — the policy module's own
            // P0-exemption rule is now the one actually enforced here.
            let worst = match self.buffer.keys().next_back() {
                Some(k)
                    if ScfEvictionPolicy::is_evictable(
                        MessagePriority::from_u8(k.priority_rank).unwrap_or(MessagePriority::P7),
                    ) =>
                {
                    *k
                }
                _ => break, // buffer empty, or only P0 remains — nothing evictable here
            };
            if let Some(msg) = self.remove_entry(&worst) {
                self.telemetry.increment(metric::SCF_EVICTIONS_TOTAL);
                tracing::warn!(
                    event = event::SCF_EVICTED,
                    message_id = %msg.envelope.message_id.short(),
                    priority = worst.priority_rank,
                    usage_bytes = self.total_bytes,
                    "message evicted from SCF buffer (RFC 9171 code 4)"
                );
                evicted.push(msg.envelope.message_id);
            }
        }
        evicted
    }

    /// All buffered messages, ascending `StoreKey` order: priority_rank ASC
    /// (P0 first), then within a priority band, soonest-expiry LAST.
    ///
    /// DTN-9: the previous doc comment claimed "priority DESC, expiry ASC"
    /// — neither was true of the derived `Ord` it described (which was
    /// priority ASC, and — because `next_back()` is what `evict_until`
    /// needs to be correct, see `StoreKey`'s `Ord` impl — the expiry
    /// component had to be reversed relative to a naive ascending reading).
    /// This does not currently have a caller that depends on a specific
    /// forward order (`scf_contact.rs` re-sorts its candidates from
    /// scratch), so no behavioural fix is needed here, only an accurate
    /// comment.
    pub fn ordered_buffered(&self) -> Vec<StoredMessage> {
        self.buffer.values().cloned().collect()
    }

    /// Delivery status of one message id.
    pub fn delivery_status(&self, id: &MessageId) -> Option<DeliveryStatus> {
        let key = self.index.get(id)?;
        self.buffer.get(key).map(|m| m.delivery_status.clone())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScfError {
    Expired,
    /// DTN-1: the buffer is at `max_bytes`/`max_messages` capacity and
    /// eviction could not free enough room for the incoming message.
    StorageFull,
    /// DTN-24: `mark_forwarded`/`enqueue_forward` was asked to act on a
    /// message id no longer in the buffer (e.g. reaped or evicted between
    /// candidate selection and the actual forward attempt).
    NotFound,
}

impl std::fmt::Display for ScfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScfError::Expired => write!(f, "scf: message expired"),
            ScfError::StorageFull => write!(f, "scf: storage full"),
            ScfError::NotFound => write!(f, "scf: message not found"),
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
    fn dtn21_status_for_never_fabricates_delivered_on_proximity() {
        // Regression: status_for used to return Delivered whenever
        // `contact` happened to equal the envelope's recipient — purely
        // because the peer was in range, before any forward was attempted
        // or ack received. A message that is merely Stored (or
        // AckPending) must report that real status regardless of `contact`.
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let e = env(MessagePriority::P4, b"sos", 3600);
        let id = e.message_id;
        let recipient = pid(9); // env()'s recipient_id is pid(9)
        scf.buffer_message(e, None).unwrap();

        assert_eq!(scf.status_for(&id, None), Some(DeliveryStatus::Stored));
        assert_eq!(
            scf.status_for(&id, Some(&recipient)),
            Some(DeliveryStatus::Stored),
            "the recipient merely being in range must not fabricate Delivered"
        );
        assert_eq!(
            scf.status_for(&id, Some(&recipient)),
            scf.delivery_status(&id),
            "status_for must agree with delivery_status for every contact argument"
        );
    }

    #[test]
    fn dtn22_delivery_status_only_terminal_when_delivered() {
        // DTN-22: DeliveryStatus was pared down to the three states this
        // module actually produces (Stored, AckPending, Delivered) — the
        // other four (Carrying, AwaitingContact, ForwardingInProgress,
        // Dropped) were never constructed anywhere in the crate. Confirms
        // is_terminal's real, exhaustively-checked semantics: only
        // Delivered is terminal.
        assert!(!DeliveryStatus::Stored.is_terminal());
        assert!(!DeliveryStatus::AckPending { sent_to: pid(9) }.is_terminal());
        assert!(DeliveryStatus::Delivered { delivered_to: pid(9) }.is_terminal());
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
        let status = scf.mark_forwarded(&id, pid(9), true);
        // DTN-5: mark_forwarded still reports the terminal status by value...
        assert_eq!(
            status,
            Some(DeliveryStatus::Delivered {
                delivered_to: pid(9)
            })
        );
        assert!(status.unwrap().is_terminal());
        // ...but the entry itself is removed immediately (STORE_CARRY_FORWARD.md
        // §Message Lifecycle: DELIVERED -> remove from store) rather than
        // retained forever, still counted by usage().
        assert_eq!(scf.delivery_status(&id), None);
        assert_eq!(scf.len(), 0);
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
    fn forwardable_to_includes_relay_candidates() {
        // DTN-4: messages_forwardable_to used to require an exact
        // recipient match — a carrier meeting anyone other than the exact
        // final recipient could forward nothing, defeating
        // store-carry-forward's entire premise (STORE_CARRY_FORWARD.md
        // §Vehicle Relay Scenario: a volunteer's phone carries messages
        // addressed to other people). A message addressed to a different
        // peer is now also a valid relay candidate for any live contact.
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let to_9 = env(MessagePriority::P3, b"to-9", 3600);
        scf.buffer_message(to_9.clone(), None).unwrap();
        let to_9_p0 = env(MessagePriority::P0, b"to-9-p0", 3600);
        scf.buffer_message(to_9_p0.clone(), None).unwrap();
        let mut other = env(MessagePriority::P4, b"to-other", 3600);
        other.recipient_id = pid(8).0.to_vec();
        scf.buffer_message(other.clone(), None).unwrap();

        let fwd = scf.messages_forwardable_to(&pid(9));
        assert_eq!(
            fwd.len(),
            3,
            "a message not addressed to the contact is still a relay candidate"
        );
        let ids: std::collections::HashSet<_> =
            fwd.iter().map(|m| m.envelope.message_id).collect();
        assert!(ids.contains(&to_9.message_id));
        assert!(ids.contains(&to_9_p0.message_id));
        assert!(ids.contains(&other.message_id));
    }

    #[test]
    fn dtn3_already_offered_peer_is_skipped_but_others_are_not() {
        // DTN-3: a peer this message was already offered to (via
        // mark_forwarded) must not see it again from
        // messages_forwardable_to — but a *different* peer must still be
        // offered it. Previously the only repeat-limiter was a global
        // forward_attempts counter shared across every peer.
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let e = env(MessagePriority::P4, b"hello", 3600);
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();

        assert_eq!(scf.messages_forwardable_to(&pid(9)).len(), 1);
        scf.mark_forwarded(&id, pid(9), false);
        // Same peer, reconnecting (a flapping link) — no longer offered.
        assert_eq!(scf.messages_forwardable_to(&pid(9)).len(), 0);
        // A different peer has never seen it — still offered.
        assert_eq!(scf.messages_forwardable_to(&pid(3)).len(), 1);
    }

    #[test]
    fn dtn3_meeting_several_peers_does_not_lock_message_out() {
        // Regression for the finding's own trigger scenario: meeting three
        // different peers must not exhaust a shared budget and lock the
        // message out for everyone — each peer gets its own independent
        // offer.
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_forward_attempts: 3,
                ..Default::default()
            },
        );
        let e = env(MessagePriority::P4, b"sos", 3600);
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();
        for peer in [pid(2), pid(3), pid(4)] {
            assert_eq!(scf.messages_forwardable_to(&peer).len(), 1);
            scf.mark_forwarded(&id, peer, false);
        }
        // A fourth, brand-new peer must still be offered the message even
        // though forward_attempts (3) has been reached — the old global
        // counter would have permanently excluded it here.
        assert_eq!(scf.messages_forwardable_to(&pid(5)).len(), 1);
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
    fn dtn20_ack_timeout_returns_message_to_stored_and_allows_retry_with_same_peer() {
        let clock = Arc::new(AtomicU64::new(1_000));
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default())
            .with_virtual_clock(clock.clone());
        let mut e = env(MessagePriority::P4, b"retry-me", 3600);
        e.timestamp = 1_000; // align with the virtual clock's start
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();
        let peer = pid(9);

        scf.mark_forwarded(&id, peer, false);
        assert_eq!(
            scf.delivery_status(&id),
            Some(DeliveryStatus::AckPending { sent_to: peer })
        );
        // DTN-3: already offered to `peer` — excluded until the timeout.
        assert!(scf.messages_forwardable_to(&peer).is_empty());

        // Not yet past the timeout: no-op.
        clock.store(1_050, Ordering::Relaxed);
        assert!(scf.reap_ack_timeouts(100).is_empty());
        assert_eq!(
            scf.delivery_status(&id),
            Some(DeliveryStatus::AckPending { sent_to: peer })
        );

        // Past the timeout: back to Stored, and retryable with the same peer.
        clock.store(1_200, Ordering::Relaxed);
        let reset = scf.reap_ack_timeouts(100);
        assert_eq!(reset, vec![id]);
        assert_eq!(scf.delivery_status(&id), Some(DeliveryStatus::Stored));
        assert_eq!(scf.messages_forwardable_to(&peer).len(), 1);
    }

    #[test]
    fn dtn20_ack_timeout_backoff_grows_with_attempts() {
        let clock = Arc::new(AtomicU64::new(0));
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default())
            .with_virtual_clock(clock.clone());
        let mut e = env(MessagePriority::P4, b"backoff", 3600);
        e.timestamp = 0; // align with the virtual clock's start
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();
        let peer = pid(9);

        // First attempt (forward_attempts -> 1): backoff = 100 * 2^1 = 200.
        scf.mark_forwarded(&id, peer, false);
        clock.store(150, Ordering::Relaxed);
        assert!(
            scf.reap_ack_timeouts(100).is_empty(),
            "150s must not clear a 200s backoff"
        );
        clock.store(250, Ordering::Relaxed);
        assert_eq!(scf.reap_ack_timeouts(100), vec![id]);

        // Second attempt (forward_attempts -> 2): backoff = 100 * 2^2 = 400.
        scf.mark_forwarded(&id, peer, false);
        clock.store(250 + 350, Ordering::Relaxed);
        assert!(
            scf.reap_ack_timeouts(100).is_empty(),
            "350s must not clear a 400s backoff after a second failed attempt"
        );
        clock.store(250 + 450, Ordering::Relaxed);
        assert_eq!(scf.reap_ack_timeouts(100), vec![id]);
    }

    #[test]
    fn store_key_ordering_priority_then_expiry() {
        // DTN-9: priority_rank is still ascending (P0 sorts first — a < b),
        // but expiry_unix is now DESCENDING within a priority band, so that
        // `.keys().next_back()` (evict_until's O(1) victim lookup) lands on
        // the soonest-expiring entry — the worst eviction candidate. c
        // (expiry=50, sooner) must therefore sort GREATER than a
        // (expiry=100), the opposite of plain ascending expiry.
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
        assert_eq!(c.cmp(&a), Ordering::Greater);
    }

    #[test]
    fn dtn1_capacity_rejects_when_full() {
        // DTN-1: buffer_message must actually enforce max_bytes — previously
        // it checked TTL only and accepted messages unconditionally.
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 300,
                ..Default::default()
            },
        );
        scf.buffer_message(env(MessagePriority::P0, b"first", 3600), None)
            .unwrap();
        // P0 is never evicted (INV-ROUTE-003), so a second message that
        // does not fit in the remaining headroom must be rejected outright
        // — this is also DTN-7's bound: it stops a P0-heavy buffer from
        // accepting unlimited further traffic once genuinely full.
        let result = scf.buffer_message(env(MessagePriority::P4, b"second-msg", 3600), None);
        assert_eq!(result, Err(ScfError::StorageFull));
        assert_eq!(scf.len(), 1);
    }

    #[test]
    fn dtn2_replayed_message_id_replaces_not_duplicates() {
        // DTN-2: the same MessageId buffered twice (e.g. a peer replaying an
        // envelope with a bumped ttl_seconds) must replace the existing
        // entry, not create a second independently-tracked one.
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let mut e = env(MessagePriority::P4, b"payload", 3600);
        let id = e.message_id;
        scf.buffer_message(e.clone(), None).unwrap();
        assert_eq!(scf.len(), 1);
        e.ttl_seconds = 7200; // same id, different ttl — a naive re-key would double-insert
        scf.buffer_message(e, None).unwrap();
        assert_eq!(scf.len(), 1, "replay must replace, not duplicate");
        assert_eq!(scf.priority_rank_of(&id), Some(MessagePriority::P4.as_u8()));
    }

    #[test]
    fn dtn2_max_messages_cap_rejects_beyond_count() {
        // DTN-2: a message-count cap independent of byte accounting — many
        // tiny messages must not be able to bypass a byte ceiling.
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_messages: 2,
                ..Default::default()
            },
        );
        scf.buffer_message(env(MessagePriority::P4, b"a", 3600), None)
            .unwrap();
        scf.buffer_message(env(MessagePriority::P4, b"b", 3600), None)
            .unwrap();
        let result = scf.buffer_message(env(MessagePriority::P4, b"c", 3600), None);
        assert_eq!(result, Err(ScfError::StorageFull));
        assert_eq!(scf.len(), 2);
    }

    #[test]
    fn dtn6_ttl_seconds_is_clamped() {
        // DTN-6: an absurd ttl_seconds (e.g. u64::MAX) must not make a
        // message immortal — it is clamped to MAX_TTL_SECS before StoreKey
        // construction and storage, so eviction ranking and reap_expired
        // both see a bounded value.
        let mut scf = ScfEngine::new(MemoryStorage::new(), ScfConfig::default());
        let e = env(MessagePriority::P4, b"forever?", u64::MAX);
        let id = e.message_id;
        scf.buffer_message(e, None).unwrap();
        let key = scf.index[&id];
        assert!(
            key.expiry_unix < u64::MAX / 2,
            "expiry_unix must be bounded by MAX_TTL_SECS, not u64::MAX-scale (got {})",
            key.expiry_unix
        );
    }

    #[test]
    fn dtn10_approx_bytes_counts_cert_chain_and_hints() {
        // DTN-10: the old formula (payload.len() + 256) made an
        // attacker-controlled auth_cert_chain or routing_hints invisible to
        // capacity accounting.
        let base = env(MessagePriority::P4, b"x", 3600);
        let bare = ScfEngine::<MemoryStorage>::approx_bytes(&base);
        let mut with_chain = base.clone();
        with_chain.auth_cert_chain = Some(vec![vec![0u8; 10_000]]);
        let inflated = ScfEngine::<MemoryStorage>::approx_bytes(&with_chain);
        assert!(
            inflated >= bare + 10_000,
            "a 10 KB cert chain must be reflected in accounted bytes (bare={bare}, inflated={inflated})"
        );
    }
}
