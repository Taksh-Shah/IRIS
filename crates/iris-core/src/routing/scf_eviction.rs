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

    /// Compute the maximum bytes a `target_bytes` request may evict without
    /// touching P0, deferring to the engine's own eviction when needed.
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
    use crate::routing::scf::ScfConfig;

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
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 2048,
                ..Default::default()
            },
        );
        let p1 = mk(MessagePriority::P1, 1500);
        let p7 = mk(MessagePriority::P7, 1500);
        scf.buffer_message(p1.clone(), None).unwrap();
        scf.buffer_message(p7.clone(), None).unwrap();
        assert_eq!(scf.len(), 2);
        let evicted = scf.evict_to_fit(0);
        assert_eq!(evicted, vec![p7.message_id]);
        assert!(scf.delivery_status(&p1.message_id).is_some());
        assert!(scf.delivery_status(&p7.message_id).is_none());
    }

    #[test]
    fn p0_never_evicted_under_pressure() {
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 5000,
                ..Default::default()
            },
        );
        let p0 = mk(MessagePriority::P0, 4000);
        scf.buffer_message(p0.clone(), None).unwrap();
        for _ in 0..4 {
            scf.buffer_message(mk(MessagePriority::P7, 1500), None)
                .unwrap();
        }
        let evicted = scf.evict_to_fit(0);
        // P0 must survive no matter how much pressure.
        assert!(scf.delivery_status(&p0.message_id).is_some());
        assert!(!evicted.contains(&p0.message_id));
    }

    #[test]
    fn eviction_ordered_lowest_priority_then_soonest_expiry() {
        let mut scf = ScfEngine::new(
            MemoryStorage::new(),
            ScfConfig {
                max_storage_bytes: 2600,
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
        // Usage ~3668 B > 2600 target; one eviction gets us under. The soonest
        // expiring P4 is the worst candidate.
        let evicted = scf.evict_to_fit(0);
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
}
