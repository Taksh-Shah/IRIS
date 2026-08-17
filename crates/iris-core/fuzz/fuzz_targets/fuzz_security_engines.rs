//! Fuzz target for the SEC-001 security engines (AC-15).
//!
//! LibFuzzer drives a persistent set of engines via a byte stream that is
//! decoded into a sequence of engine operations. The engines are the real
//! tokio-based implementations; a single current-thread runtime is created
//! once and reused so that fuzzing exercises long-running state machines
//! (token buckets, quotas, high-water marks, reputation scores) the same way
//! the message engine does.
//!
//! Input layout:
//!   byte 0          → op (which engine / operation)
//!   bytes 1..17     → sender short id (16B)
//!   bytes 17..33    → peer short id (16B) (for reputation)
//!   remaining bytes → parameter bytes (decoded via a cursor; missing bytes
//!                     decode to 0 so the fuzzer can start from tiny inputs)

#![no_main]

use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;

use iris_core::message_engine::expiry::unix_now;
use iris_core::identity::TrustStore;
use iris_core::MessagePriority;
use iris_core::protocol::{ContentType, Envelope, MessageId};
use iris_core::security::{
    FullSecurityPolicy, MessageClass, ReputationEvent, ReplaySnapshot,
    SecurityPolicy, SenderShort,
};

// Engine state persists across fuzz iterations.
static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 1 }
    }

    fn u8(&mut self) -> u8 {
        let b = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }

    fn u32(&mut self) -> u32 {
        let mut out = [0u8; 4];
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.u8();
            let _ = i;
        }
        u32::from_le_bytes(out)
    }

    fn u64(&mut self) -> u64 {
        let mut out = [0u8; 8];
        for o in out.iter_mut() {
            *o = self.u8();
        }
        u64::from_le_bytes(out)
    }

    fn short(&mut self) -> SenderShort {
        let mut s = [0u8; 16];
        for o in s.iter_mut() {
            *o = self.u8();
        }
        s
    }
}

fn class_from_byte(b: u8) -> MessageClass {
    match b % 9 {
        0 => MessageClass::P0,
        1 => MessageClass::P1,
        2 => MessageClass::P2,
        3 => MessageClass::P3,
        4 => MessageClass::P4,
        5 => MessageClass::P5,
        6 => MessageClass::P6,
        7 => MessageClass::P7,
        _ => MessageClass::Unknown,
    }
}

fn priority_from_byte(b: u8) -> MessagePriority {
    MessagePriority::from_u8(b % 8).unwrap_or(MessagePriority::P4)
}

fn event_from_byte(b: u8) -> ReputationEvent {
    match b % 5 {
        0 => ReputationEvent::LocalForward,
        1 => ReputationEvent::LocalDrop,
        2 => ReputationEvent::SecondhandPositive {
            from_verified_peer: [b; 16],
        },
        3 => ReputationEvent::AuditPositive,
        _ => ReputationEvent::AuditNegative,
    }
}

fn envelope_from(c: &mut Cursor) -> (Envelope, u64) {
    let priority = priority_from_byte(c.u8());
    let now = unix_now();
    // timestamp squeezed near now so the freshness window is the interesting region.
    let skew = c.u64() % (24 * 3600 + 600);
    let ts = now.saturating_add(skew).saturating_sub(12 * 3600);
    let payload_len = (c.u8() as usize) % 256;
    let mut payload = vec![0u8; payload_len];
    for p in payload.iter_mut() {
        *p = c.u8();
    }
    let mut sender = [0u8; 32];
    for s in sender.iter_mut() {
        *s = c.u8();
    }
    let mut recipient = [0u8; 32];
    for r in recipient.iter_mut() {
        *r = c.u8();
    }
    let env = Envelope {
        version: iris_core::protocol::PROTOCOL_VERSION,
        message_id: MessageId::new_v7(),
        sender_id: sender.to_vec(),
        recipient_id: recipient.to_vec(),
        priority,
        ttl_seconds: c.u64(),
        timestamp: ts,
        hop_count: c.u8(),
        max_hops: None,
        payload_type: ContentType::from_u8(c.u8() % 20)
            .unwrap_or(ContentType::Text),
        payload_size: payload_len as u64,
        payload_hash: [0u8; 32],
        payload,
        payload_ref: None,
        signature: None,
        encryption_hdr: None,
        routing_hints: None,
        auth_cert_chain: None,
    };
    (env, now)
}

fn build_policy() -> &'static FullSecurityPolicy {
    static POLICY: OnceLock<FullSecurityPolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        let trust_store = TrustStore::new();
        FullSecurityPolicy::new(std::sync::Arc::new(trust_store))
    })
}

fuzz_target!(|data: &[u8]| {
    let rt = RT.get_or_init(|| tokio::runtime::Builder::new_current_thread().build().unwrap());
    let policy = build_policy();
    let mut c = Cursor::new(data);
    let op = c.u8();
    let sender = c.short();
    let peer = c.short();

    match op % 14 {
        // Rate-limiter: per-sender + unknown-aggregate paths.
        0 => {
            let _ = rt.block_on(policy.rate_limit(sender, class_from_byte(c.u8())));
        }
        1 => {
            let _ = rt.block_on(policy.rate_limit_unknown(class_from_byte(c.u8())));
        }
        // Quota: check + add + remove interplay (state machine).
        2 => {
            let size = c.u64();
            let prio = priority_from_byte(c.u8());
            let _ = rt.block_on(policy.check_quota(sender, size, prio));
        }
        3 => {
            let size = c.u64();
            let prio = priority_from_byte(c.u8());
            let _ = rt.block_on(policy.add_message(sender, size, prio));
        }
        4 => {
            let size = c.u64();
            rt.block_on(policy.remove_message(sender, size));
        }
        // Replay: freshness + high-water with attacker-controlled ts/seq and sender.
        5 => {
            let ts = unix_now().saturating_sub(c.u32() as u64 % (24 * 3600 + 3600));
            let seq = c.u64();
            let _ = rt.block_on(policy.check_replay(sender, ts, seq));
        }
        6 => {
            let ts = unix_now().saturating_add(c.u32() as u64 % 3600);
            let _ = rt.block_on(policy.check_freshness(ts));
        }
        // Replay snapshot round-trip (cross-reboot restore, AC-5).
        7 => {
            let snap = rt.block_on(policy.generate_replay_snapshot());
            if let Some(s) = snap {
                rt.block_on(policy.load_replay_snapshot(s));
            }
        }
        // Reputation: update + routing weight with mixed event types.
        8 => {
            rt.block_on(policy.update_reputation(peer, event_from_byte(c.u8())));
        }
        9 => {
            let _ = rt.block_on(policy.routing_weight(peer));
        }
        // Spam: receiver-side annotation over adversarial envelopes.
        10 => {
            let (env, _now) = envelope_from(&mut c);
            let _ = rt.block_on(policy.score_spam(sender, &env));
        }
        // Emergency ACL: forged/absent authority against P0/P2 envelopes.
        11 => {
            let (env, now) = envelope_from(&mut c);
            let _ = rt.block_on(policy.check_emergency_acl(&env, now));
        }
        // Replay extreme boundaries: u64::MAX ts/seq and 0 (overflow / ordering).
        12 => {
            let _ = rt.block_on(policy.check_replay(sender, u64::MAX, u64::MAX));
            let _ = rt.block_on(policy.check_replay(sender, 0, 0));
            let _ = rt.block_on(policy.check_replay(peer, u64::MAX, 0));
        }
        // Snapshot serde round-trip via a raw struct (persistence layer is the
        // caller; corrupt snapshots must not panic restore).
        _ => {
            let now = unix_now();
            let snap = ReplaySnapshot {
                highwater: std::collections::HashMap::new(),
                snapshot_timestamp: now,
                version: c.u32(),
            };
            rt.block_on(policy.load_replay_snapshot(snap));
        }
    }

    // Sanity guard: after any op the armed policy must report decisions, never panic.
    let _ = rt.block_on(policy.routing_weight([0; 16]));
    let _ = rt.block_on(policy.rate_limit([0; 16], MessageClass::P4));
});