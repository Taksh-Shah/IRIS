# Technical Abuse Prevention

## Overview

Abuse prevention in IRIS is primarily technical — the system enforces limits and controls
at the protocol level, regardless of policy claims. This document describes the technical
mechanisms that prevent system abuse, complementing the policy-level RESPONSIBLE_USE.md.

## Abuse Categories

| Category | Technical Enforcement | Policy Enforcement |
|----------|----------------------|-------------------|
| Spam / message flood | Rate limits, storage quotas | Block lists, community moderation |
| Storage exhaustion | Per-sender quotas | Relay operator policy |
| Emergency channel abuse | Certificate verification, rate limits | Legal action |
| Harassment via repeat messages | Block lists | Legal action |
| Fake identities (Sybil) | Resource costs, social graph filtering | N/A |
| Address scraping | Rate-limited discovery | N/A |
| Gateway surveillance | Encryption (gateway cannot read) | Gateway operator policy |

## Rate Limiting

### Per-Sender Rate Limits

Every relay node tracks message counts per sender using sliding window counters.
Token bucket implementation using the `governor` crate.

```rust
// From crates/iris-core/src/abuse_prevention.rs
pub struct RateLimiter {
    buckets: DashMap<NodeId, HashMap<Priority, TokenBucket>>,
    config: RateLimitConfig,
}

pub struct RateLimitConfig {
    // Messages per minute by priority
    pub p0_per_min: u32,    // 0 = unlimited
    pub p1_per_min: u32,    // 0 = unlimited
    pub p2_per_min: u32,    // 60 (1/second for location updates)
    pub p3_per_min: u32,    // 30
    pub p4_per_min: u32,    // 10
    pub p5_per_min: u32,    // 6
    pub p6_per_min: u32,    // 2
    pub p7_per_min: u32,    // 1
    
    // Burst capacity (max tokens in bucket)
    pub burst_factor: f32,  // 2.0 = burst to 2x rate limit
}
```

Rate limits are applied before signature verification to minimize computation cost
of enforcement. A node exceeding its rate limit has messages silently dropped —
no error response is sent to the sender (prevents probing).

### Trust-Level Rate Limit Adjustment

Trusted contacts receive relaxed rate limits; unknown senders receive stricter limits.

```
Unknown sender (Tier 4):
  P4: 5 msgs/min (half of default)
  P5: 2 msgs/min
  P6: 0 msgs/min (blocked)
  P7: 0 msgs/min (blocked)

Direct contact (Tier 1):
  P4: 60 msgs/min (6x default)
  P5: 20 msgs/min
  P6: 10 msgs/min
  P7: 5 msgs/min
```

Trust tiers computed from local contact list + social graph (see SPAM_RESISTANCE.md).

### SOS-Specific Rate Limits

P0 SOS messages have special rate limiting (described in detail in EMERGENCY_ABUSE.md):
- No rate limit per-message for delivery (P0 always delivered)
- Rate limit on SOS ORIGIN: max 3 SOS per hour per sender identity
- After limit: SOS still delivered, but without P0 routing priority

## Identity-Based Blocking

### Local Block List

```rust
pub struct BlockList {
    blocked_senders: HashSet<NodeId>,
    blocked_groups: HashSet<GroupId>,
    max_entries: usize,  // 10,000 default
}

impl BlockList {
    pub fn is_blocked(&self, sender: &NodeId) -> bool {
        self.blocked_senders.contains(sender)
    }
    
    pub fn block(&mut self, sender: NodeId) {
        // Silently drop all future messages from this sender
        self.blocked_senders.insert(sender);
    }
}
```

Block list is persisted to device storage. Applied before any processing — blocked messages
are dropped in O(1) time before relay, signature verification, or decryption.

**Emergency exception**: P0-P1 messages bypass block list. A blocked user can still send
SOS. This is intentional — if a user unintentionally blocked someone who then has a real
emergency, the SOS still reaches them. Risk of SOS bypass abuse is addressed by SOS
rate limits in EMERGENCY_ABUSE.md.

### Shared Block Lists

Users can export their block list and share with trusted contacts:

```json
{
  "type": "BLOCK_LIST_SHARE",
  "version": 1,
  "exported_by": "sender_node_id_hash",
  "exported_at": 1720000000,
  "entries": [
    {"node_id": "hash_of_blocked_id", "reason": "spam", "blocked_at": 1719000000},
    ...
  ],
  "signature": "ed25519_signature"
}
```

Recipients can selectively apply shared block lists. Applying a shared block list is
opt-in per-share. The system does not automatically propagate block lists.

**Caveat**: a malicious user could share a block list that blocks legitimate nodes.
Recipients should only trust block lists from known, trusted contacts.

## Storage Quotas

### Per-Sender Storage Quota

```rust
pub struct RelayStorageQuota {
    sender_usage: HashMap<NodeId, u64>,  // bytes used per sender
    max_per_sender: u64,                  // default: 10MB
    total_capacity: u64,                  // configured by operator
    emergency_reserved: u64,              // 100MB reserved for P0-P2
}

impl RelayStorageQuota {
    pub fn can_store(&self, sender: &NodeId, size: u64, priority: Priority) -> StoreDecision {
        if priority <= Priority::P2 {
            // Emergency: use reserved pool, always allow
            if self.emergency_pool_usage() + size <= self.emergency_reserved {
                return StoreDecision::Allow;
            }
        }
        
        let current = self.sender_usage.get(sender).copied().unwrap_or(0);
        if current + size > self.max_per_sender {
            return StoreDecision::DenyEvictOldest;  // Evict sender's oldest P4+ first
        }
        
        StoreDecision::Allow
    }
}
```

### Priority-Stratified Storage

When overall storage pressure exceeds 80% capacity:

```
Storage allocation under pressure:
  P0-P2: always stored (reserved pool)
  P3: stored up to 95% of remaining capacity
  P4: stored up to 80% of remaining capacity
  P5: stored up to 50% of remaining capacity
  P6: stored up to 20% of remaining capacity
  P7: not stored when pressure > 80%
```

## Message Size Limits

Hard limits enforced before signature verification:

```rust
pub fn check_message_size(size: u64, priority: Priority) -> Result<(), AbuseDenied> {
    let max = match priority {
        Priority::P0 => 700,
        Priority::P1 => 2_500,
        Priority::P2 => 400,
        Priority::P3 => 4_500,
        Priority::P4 => 10_240,
        Priority::P5 => 2_097_152,
        Priority::P6 => 5_242_880,
        Priority::P7 => 53_477_376,
    };
    if size > max {
        Err(AbuseDenied::MessageTooLarge { size, max })
    } else {
        Ok(())
    }
}
```

## Emergency Channel Access Control

Only nodes with valid authority certificates can:
- Originate emergency broadcast messages (P3-broadcast type)
- Send signed moderation commands to community emergency channels
- Activate disaster mode for their geographic scope

Certificate verification: at every relay, before forwarding any broadcast-type message.
Invalid certificate → DROP + log. Cannot be bypassed.

## Content-Agnostic Abuse Signals

IRIS does not inspect message content (E2EE). Abuse detection uses only metadata signals:

| Signal | Threshold | Action |
|--------|-----------|--------|
| High message frequency | >2× rate limit attempts | Reduce rate limit further |
| Large payload fraction | >80% of messages are P5+ | Flag for storage priority reduction |
| Many unique recipients | >100 unique recipients in 1h | Flag as potential spam node |
| High delivery failure | >50% of messages undeliverable | Reduce forwarding priority |

Signals are combined into an abuse score. Score thresholds trigger graduated responses
(log, reduce priority, rate limit further, drop). No automatic permanent ban — that
requires human review (via community moderation or admin action).

## Anti-Scraping Controls

Address harvesting is limited by:
- **Discovery rate limits**: node discovery responses limited to 10/minute per requester
- **No address directory**: no global lookup service
- **Rotating BLE IDs**: advertisement changes every 15 minutes
- **Contact list opacity**: full contact list never transmitted

Discovery request flooding:
```rust
pub struct DiscoveryRateLimiter {
    requesters: LruCache<NodeId, TokenBucket>,  // 10 responses/minute default
}
```

## Moderation in Offline Network

Community channel admins can issue moderation commands as signed messages:

```rust
pub enum ModerationCommand {
    RemoveMessage { 
        channel_id: ChannelId,
        message_id: MessageId,
        reason: String,
        admin_cert: AdminCertificate,
    },
    MuteSender {
        channel_id: ChannelId,
        sender_id: NodeId,
        duration_minutes: u32,
        reason: String,
        admin_cert: AdminCertificate,
    },
    RestoreSender {
        channel_id: ChannelId,
        sender_id: NodeId,
        admin_cert: AdminCertificate,
    },
}
```

Moderation commands propagate as P3 messages, signed by admin. Every relay applies
moderation commands to its local channel store when verified. Eventually consistent
across the mesh.
