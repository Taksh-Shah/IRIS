//! SCF ordered eviction — SCF-001 (WP-5).
//!
//! Per STORE_CARRY_FORWARD.md §Storage Capacity: eviction runs in strict
//! priority order P7→P0, oldest-expiry first within each priority, and **P0
//! messages are never evicted** (INV-ROUTE-003). Capacity defaults are
//! device-class aware: 200 MB phone, 2 GB relay, 10 GB edge, 50 GB desktop.

use crate::message::MessagePriority;
use crate::message_engine::storage::MessageStorage;
use crate::routing::scf::ScfEngine;

/// Device-class storage capacities (STORE_CARRY_FORWARD.md §Storage Capacity).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceClass {
    Phone,
    Relay,
    Edge,
    Desktop,
}

impl DeviceClass {
    pub fn max_bytes(self) -> u64 {
        match self {
            DeviceClass::Phone => 200 * 1024 * 1024,
            DeviceClass::Relay => 2 * 1024 * 1024 * 1024,
            DeviceClass::Edge => 10 * 1024 * 1024 * 1024,
            DeviceClass::Desktop => 50 * 1024 * 1024 * 1024,
        }
    }
}

/// Eviction policy for the SCF buffer: strict priority order P7→P1, then
/// soonest-expiry first. P0 is exempt (INV-ROUTE-003).
#[derive(Debug, Default)]
pub struct ScfEvictionPolicy;

impl ScfEvictionPolicy {
    /// True if the policy may evict messages of this priority.
    pub fn is_evictable(priority: MessagePriority) -> bool {
        priority != MessagePriority::P0
    }

    /// The eviction rank for a priority class: 0 (P0, never evicted) through
    /// 7 (P7, evicted first). Higher is worse. This is `StoreKey`'s
    /// `priority_rank` — the canonical mapping the buffer's ordering (and
    /// so `evict_until`'s `next_back()` victim lookup) is built on.
    ///
    /// DTN-11: the previous doc comment here ("compute the maximum bytes a
    /// target_bytes request may evict") did not describe what this
    /// function does or returns.
    pub fn eviction_weight(priority: MessagePriority) -> u8 {
        match priority {
            MessagePriority::P0 => 0,
            MessagePriority::P1 => 1,
            MessagePriority::P2 => 2,
            MessagePriority::P3 => 3,
            MessagePriority::P4 => 4,
            MessagePriority::P5 => 5,
            MessagePriority::P6 => 6,
            MessagePriority::P7 => 7,
        }
    }
}

impl<S: MessageStorage> ScfEngine<S> {
    /// DTN-11: apply a device-class storage ceiling
    /// (STORE_CARRY_FORWARD.md §Storage Capacity), capped at whichever is
    /// smaller: the class ceiling or the engine's own configured
    /// `max_storage_bytes`. Without this, `DeviceClass`'s per-platform
    /// ceilings (200 MB phone .. 50 GB desktop) had no way to reach
    /// `ScfEngine` at all — grep confirmed zero production callers —
    /// so every node ran with the flat `ScfConfig::default()` 500 MB
    /// regardless of platform. Call once, right after `new()` (mirrors
    /// `with_telemetry`/`with_virtual_clock`).
    pub fn with_device_class(mut self, class: DeviceClass) -> Self {
        let ceiling = class.max_bytes().min(self.max_bytes());
        self.set_max_bytes(ceiling);
        self
    }

    /// Evict down to the device-class capacity, respecting P0 exemption. The
    /// policy runs lowest-priority (then soonest-expiring) first.
    pub fn evict_to_capacity(&mut self, class: DeviceClass) -> Vec<crate::protocol::MessageId> {
        let ceiling = class.max_bytes();
        if self.usage() <= ceiling {
            return Vec::new();
        }
        // Trim from the current max down to the class ceiling in one pass.
        self.evict_until(ceiling)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message_engine::storage::MemoryStorage;
    use crate::protocol::{ContentType, Envelope, MessageId};
    use crate::routing::scf::{ScfConfig, ScfError};

    fn pid(id: u8) -> crate::message::PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        crate::message::PeerId::from_bytes(b)
    }

    fn env(prio: MessagePriority, payload: &[u8]) -> Envelope {
        Envelope {
            version: crate::protocol::PROTOCOL_VERSION,
            message_id: MessageId::new_v7(),
            sender_id: pid(1).0.to_vec(),
            recipient_id: pid(9).0.to_vec(),
            priority: prio,
            ttl_seconds: 3600,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
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

    fn mk(prio: MessagePriority, bytes: u64) -> Envelope {
        let payload = vec![0u8; (bytes.saturating_sub(256)) as usize];
        env(prio, &payload)
    }

    #[test]
    fn p7_evicted_before_p1() {
        // DTN-1: buffer_message now enforces max_bytes on every insert, so
        // the budget here must have headroom for both messages to actually
        // land (previously they were admitted unconditionally regardless of
        // budget, and eviction was only ever tested via a later, separate
        // evict_to_fit(0) call — that call now needs an explicit target
        // instead of relying on max_bytes having already been exceeded).
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 4096,
                ..Default::default()
            },
        );
        let p1 = mk(MessagePriority::P1, 1500);
        let p7 = mk(MessagePriority::P7, 1500);
        scf.buffer_message(p1.clone(), None).unwrap();
        scf.buffer_message(p7.clone(), None).unwrap();
        assert_eq!(scf.len(), 2);
        let evicted = scf.evict_until(1500);
        assert_eq!(evicted, vec![p7.message_id]);
        assert!(scf.delivery_status(&p1.message_id).is_some());
        assert!(scf.delivery_status(&p7.message_id).is_none());
    }

    #[test]
    fn p0_never_evicted_under_pressure() {
        // DTN-1/DTN-7: with insert-time enforcement, P0 survives eviction
        // pressure from P7 traffic filling every byte around it — this now
        // exercises the *real* admission path (each buffer_message call
        // evicts P7 to make room for the next), not just the eviction
        // policy in isolation.
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 10_000,
                ..Default::default()
            },
        );
        let p0 = mk(MessagePriority::P0, 4000);
        scf.buffer_message(p0.clone(), None).unwrap();
        // Fills exactly to the ceiling (4000 + 4*1500 = 10_000).
        for _ in 0..4 {
            scf.buffer_message(mk(MessagePriority::P7, 1500), None)
                .unwrap();
        }
        // One more P7 must still be admitted by evicting an existing P7 —
        // P0 must never be touched even though it is the single largest
        // entry and eviction pressure is constant.
        scf.buffer_message(mk(MessagePriority::P7, 500), None)
            .unwrap();
        assert!(
            scf.delivery_status(&p0.message_id).is_some(),
            "P0 must survive eviction pressure no matter how full the buffer gets"
        );
    }

    #[test]
    fn eviction_ordered_lowest_priority_then_soonest_expiry() {
        // See p7_evicted_before_p1: headroom for all three inserts, then an
        // explicit evict_until target reproduces the original scenario.
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 4096,
                ..Default::default()
            },
        );
        // A P4 expiring sooner must be evicted before a P4 expiring later.
        let mut soon = mk(MessagePriority::P4, 1300);
        soon.ttl_seconds = 60;
        let mut later = mk(MessagePriority::P4, 1300);
        later.ttl_seconds = 3600;
        scf.buffer_message(soon.clone(), None).unwrap();
        scf.buffer_message(later.clone(), None).unwrap();
        // A higher-priority P2 must stay regardless.
        scf.buffer_message(mk(MessagePriority::P2, 300), None)
            .unwrap();
        // Usage 2900 B; shrinking to 1600 forces exactly one eviction. The
        // soonest-expiring P4 is the worst candidate.
        let evicted = scf.evict_until(1600);
        assert_eq!(evicted, vec![soon.message_id]);
        let remaining = scf.ordered_buffered();
        assert!(scf.delivery_status(&later.message_id).is_some());
        assert!(remaining
            .iter()
            .any(|m| m.envelope.priority == MessagePriority::P2));
    }

    #[test]
    fn device_class_ceiling_trims_to_class() {
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default());
        // Three ~90 MB payloads overflow a 200 MB phone ceiling.
        for _ in 0..3u8 {
            scf.buffer_message(mk(MessagePriority::P4, 90_000_000), None)
                .unwrap();
        }
        assert!(scf.usage() > DeviceClass::Phone.max_bytes());
        let evicted = scf.evict_to_capacity(DeviceClass::Phone);
        assert!(!evicted.is_empty());
        assert!(scf.usage() <= DeviceClass::Phone.max_bytes());
    }

    #[test]
    fn dtn11_with_device_class_lowers_the_insert_time_ceiling() {
        // DTN-11: DeviceClass previously had no way to reach ScfEngine at
        // all — evict_to_capacity was the only entry point, and nothing
        // called it in production, so every node ran with the flat
        // ScfConfig::default() 500 MB regardless of platform.
        // with_device_class must actually lower buffer_message's own
        // enforced ceiling (DTN-1), not just be available for a manual
        // evict_to_capacity call.
        let mut scf = ScfEngine::new(MemoryStorage::new(), Default::default())
            .with_device_class(DeviceClass::Phone);
        assert_eq!(scf.max_bytes(), DeviceClass::Phone.max_bytes());

        // A payload comfortably under the 500 MB default but over the 200
        // MB phone ceiling must now be rejected at insert time.
        let e = mk(MessagePriority::P4, 250_000_000);
        assert_eq!(scf.buffer_message(e, None), Err(ScfError::StorageFull));
    }

    #[test]
    fn dtn11_with_device_class_never_raises_the_configured_ceiling() {
        // A device class larger than the engine's own configured budget
        // must not raise the ceiling above what was explicitly configured.
        let scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 1000,
                ..Default::default()
            },
        )
        .with_device_class(DeviceClass::Desktop); // 50 GB — far larger than 1000
        assert_eq!(scf.max_bytes(), 1000);
    }
}
