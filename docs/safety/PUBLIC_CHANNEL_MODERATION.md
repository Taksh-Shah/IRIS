# IRIS Public Channel Moderation

**Document ID:** IRIS-SAFETY-002  
**Version:** 1.0  
**Status:** Active  

---

## 1. Overview

Public emergency channels in IRIS are broadcast channels that any node can receive but only authorized entities can post to. They serve as the primary coordination medium for emergency authorities during disaster response.

Moderation in a DTN context is fundamentally different from centralized moderation (e.g., Slack, Twitter). There is no central moderator with real-time visibility of all messages. Content that has been transmitted cannot be recalled. The moderation model must work through:
1. Access control at post time (preventing unauthorized posts)
2. Rate limiting and content filtering at the transport layer
3. Community flagging that propagates like other IRIS bundles
4. Accepting that some harmful content will propagate before moderation can act

---

## 2. Public Emergency Channels

### 2.1 Channel Types

| Channel Type | Who Can Post | Who Can Receive | Geographic Scope |
|-------------|-------------|----------------|-----------------|
| National Emergency Broadcast | NDRF, NDMA | All IRIS nodes | National |
| State Emergency Broadcast | SDRF, State Nodal Agency | All nodes in state | State (e.g., Gujarat) |
| District Coordination | District collector's office, verified NDRF | Nodes in district | District |
| Medical Resource Channel | Verified hospitals, Red Cross | All nodes | Configurable |
| NGO Coordination | Registered NGOs (Type 5) | Subscribed nodes | Regional |

### 2.2 Channel Membership

Channels use a pub-sub model. A channel is identified by a `ChannelId` (32-byte BLAKE3 hash of channel name + scope + creation timestamp).

- **Subscription:** Any node can subscribe to any public channel. Subscription is local state only — no central registry.
- **Publishing:** Requires `CHANNEL_PUBLISH` capability scoped to the channel or channel category.
- **Delivery:** Messages published to a channel are bundled with the `channel_id` field set. Routing engine delivers to all subscribed peers on contact.

### 2.3 Channel Bootstrapping

Pre-configured national and state channels are included in the IRIS app. Channel IDs for NDRF national broadcast and state SDRF channels are hardcoded constants:

```rust
pub const CHANNEL_NDRF_NATIONAL: ChannelId = ChannelId([/* 32 bytes */]);
pub const CHANNEL_SDRF_GUJARAT: ChannelId = ChannelId([/* 32 bytes */]);
pub const CHANNEL_REDCROSS_MEDICAL: ChannelId = ChannelId([/* 32 bytes */]);
```

These channels cannot be forged: posting requires a valid authority capability token. Subscribing is always free.

---

## 3. Access Control at Post Time

### 3.1 Permission Enforcement at Sender

The IRIS app enforces publishing permissions before accepting a message for transmission:

```rust
pub async fn publish_to_channel(
    &self,
    channel_id: ChannelId,
    content: MessageContent,
    capability_token: &CapabilityToken,
) -> Result<BundleId, PublishError> {
    // Check capability
    let auth = self.authorizer.authorize(
        self.node_id,
        OperationType::PublishToPublicChannel,
        Some(capability_token),
        Some(&self.cert_chain),
    ).await?;
    
    // Check channel-specific scope
    if !auth.grants_channel_access(&channel_id) {
        return Err(PublishError::InsufficientScope);
    }
    
    // Check rate limit
    self.rate_limiter.check_channel(&channel_id, &self.node_id)?;
    
    // Check content filters
    self.content_filter.check(&content)?;
    
    // Construct and sign the bundle
    let bundle = self.bundle_builder
        .channel_message(channel_id, content, &self.signing_key, capability_token)
        .build()?;
    
    self.routing_engine.submit(bundle).await
}
```

### 3.2 Permission Enforcement at Relay Nodes

Relay nodes also verify channel publish permissions before forwarding channel messages. This provides defense-in-depth: even if a malicious node bypasses sender-side checks, relay nodes will drop unauthorized channel posts.

```rust
fn should_relay_channel_bundle(
    bundle: &Bundle,
    authorizer: &dyn Authorizer,
) -> bool {
    // Extract channel publish token from bundle
    let token = match bundle.get_capability_token() {
        Some(t) => t,
        None => return false,  // No token → drop channel message
    };
    
    // Verify token is valid for this channel
    authorizer.has_capability_for_channel(
        bundle.source_node_id(),
        CapabilityCode::ChannelPublish,
        bundle.channel_id(),
    )
}
```

---

## 4. Content Filtering

### 4.1 Rate Limiting

Rate limiting prevents a single node from flooding a channel, even if they hold a valid publish capability.

```rust
pub struct ChannelRateLimiter {
    // Per (channel_id, source_node_id) → token bucket
    buckets: DashMap<(ChannelId, NodeId), TokenBucket>,
}

impl ChannelRateLimiter {
    pub fn check_channel(
        &self,
        channel_id: &ChannelId,
        source: &NodeId,
    ) -> Result<(), RateLimitError> {
        let bucket = self.buckets.entry((*channel_id, *source))
            .or_insert_with(|| TokenBucket::new(
                10,  // burst: up to 10 messages
                Duration::from_secs(3600),  // refill: 10 per hour
            ));
        
        if bucket.try_consume(1) {
            Ok(())
        } else {
            Err(RateLimitError::ExceededChannelRate {
                channel_id: *channel_id,
                next_allowed: bucket.next_available(),
            })
        }
    }
}
```

**Default rate limits by channel type:**

| Channel Type | Max Burst | Sustained Rate |
|-------------|-----------|---------------|
| National Emergency Broadcast | 5 messages | 5 per hour |
| State Emergency Broadcast | 10 messages | 10 per hour |
| District Coordination | 20 messages | 20 per hour |
| Medical Resource | 30 messages | 30 per hour |
| NGO Coordination | 10 messages | 10 per hour |

### 4.2 Size Limits

| Content Type | Maximum Size |
|-------------|-------------|
| Text message | 4 KB |
| Location bundle | 512 bytes |
| Medical resource update | 2 KB |
| Image (P5) | 2 MB |
| Audio announcement | 5 MB |

Messages exceeding size limits are rejected at the sender with a clear error. The UI prevents composing oversized messages.

### 4.3 Content Type Restrictions

Public emergency channels apply strict content type allowlisting:

```rust
pub enum AllowedContentType {
    PlainText,          // UTF-8 text, no markup
    StructuredLocation, // CBOR location bundle
    StructuredResource, // CBOR medical/supply resource bundle
    ImageJpeg,          // JPEG only, no animated
    AudioOpus,          // Opus audio, max 5 min
}

// Explicitly disallowed:
// - HTML, Markdown (XSS-adjacent risks in untrusted rendering environments)
// - JavaScript, WASM, any executable content
// - ZIP, TAR, or other archive formats
// - Video (bandwidth constrained; P7 video in personal channels only)
// - PDF (execution risk from malformed PDF)
```

Content type is verified by MIME sniffing (not trusting the sender-provided type) at both sender and relay.

---

## 5. Moderation in DTN Context

### 5.1 The Fundamental Constraint

Once a bundle is transmitted via IRIS, it cannot be recalled. Unlike a centralized system where a moderator can delete a post and it disappears from all users, a bundle that has been received by even one device will persist there until TTL expiry (up to 72 hours for P0, 6 hours for P3).

This constraint shapes all moderation decisions: **prevention is always preferable to remediation**.

### 5.2 What Can Be Moderated

| Action | Possible | Notes |
|--------|----------|-------|
| Prevent unauthorized post | Yes | Access control at sender and relay |
| Rate-limit legitimate poster | Yes | Token bucket rate limiter |
| Block content from known bad node | Partial | Blacklist propagates via peer sync; not instant |
| Remove content already propagated | No | Fundamental DTN limitation |
| Suppress further propagation of flagged content | Partial | Flag bundles propagate; but relay decisions are local |
| Revoke poster's capability | Yes | Emergency revocation bundle (delayed propagation) |

### 5.3 Relay-Layer Content Filtering

When a relay node receives a flagged bundle (see Section 6), it can choose not to relay it to new peers. This slows propagation of flagged content without removing what's already been received.

```rust
fn should_relay(
    bundle: &Bundle,
    flag_filter: &FlagFilter,
    local_policy: &RelayPolicy,
) -> bool {
    let flag_count = flag_filter.count_flags(&bundle.id());
    
    match local_policy.flag_threshold {
        Some(threshold) if flag_count >= threshold => {
            // Suppress further relay; log suppression for audit
            false
        },
        _ => true,
    }
}
```

The default relay policy does not suppress flagged content (threshold = None) because false flagging (coordinated flag abuse) could suppress legitimate emergency information. Relay suppression based on flags is an operator-configurable option for specific deployment contexts.

---

## 6. Community Reporting (Flag Mechanism)

### 6.1 Flag Bundle Format

Any IRIS node can flag a bundle they believe violates channel rules. Flags propagate like ACKs (P4 priority):

```
FlagBundle = {
  1: bstr,    ; flagged_bundle_id: 32-byte bundle ID being flagged
  2: uint,    ; flag_reason: SPAM=0, DISINFORMATION=1, INAPPROPRIATE=2, IMPERSONATION=3
  3: bstr,    ; flagger_node_id: NodeId of the flagging node
  4: uint,    ; flagged_at: unix timestamp
  5: bstr,    ; flagger_signature: Ed25519 signature
}
```

### 6.2 Flag Propagation

Flag bundles propagate using standard IRIS routing (P4 priority). They accumulate in a local flag counter per bundle ID:

```rust
pub struct FlagCounter {
    // bundle_id → set of flagging node_ids (to prevent double-counting from same node)
    flags: DashMap<BundleId, HashSet<NodeId>>,
}

impl FlagCounter {
    pub fn record_flag(&self, flag: &FlagBundle) {
        self.flags
            .entry(flag.flagged_bundle_id)
            .or_default()
            .insert(flag.flagger_node_id);
    }
    
    pub fn flag_count(&self, bundle_id: &BundleId) -> usize {
        self.flags.get(bundle_id).map_or(0, |s| s.len())
    }
}
```

### 6.3 Automated Response to Flags

The IRIS system does not take automated action based on flag count alone. Flag counts are:
- Surfaced to channel moderators (verified authority nodes monitoring the channel)
- Included in anomaly reports to IRIS operators (in connected mode)
- Available to relay nodes for local suppression policy (operator configurable)

The reason for not automating flag-based removal: coordinated flag abuse could suppress legitimate emergency broadcasts. In a disaster scenario, this is a critical failure mode.

### 6.4 Authority Moderator Action

Verified authority nodes with `CHANNEL_PUBLISH` capability can post a "moderation notice" bundle:

```rust
pub struct ModerationNotice {
    pub flagged_bundle_id: BundleId,
    pub action: ModerationAction,
    pub reason: String,
    pub moderator_cert: AuthorityCertificate,
    pub moderator_signature: Ed25519Signature,
}

pub enum ModerationAction {
    Endorsed,     // Authority confirms bundle is legitimate
    Flagged,      // Authority confirms bundle is problematic
    Impersonation, // Authority confirms impersonation attempt
}
```

When a user's IRIS app receives a `ModerationAction::Flagged` or `ModerationAction::Impersonation` notice from a verified authority, it displays a warning on the flagged bundle's UI display. The content is not hidden (users can still read it) but is marked with the authority's flag.

---

## 7. Known Limitations

| Limitation | Consequence | Mitigation |
|------------|-------------|------------|
| Cannot recall transmitted content | Harmful content persists up to 72h | Prevention through access control; short TTL for P3+ |
| Flag abuse by coordinated group | Legitimate content may appear flagged | Flags require signature; authority endorsement overrides flags |
| Relay suppression based on flags risks censorship | Emergency info could be suppressed | Default: no relay suppression. Must be explicitly enabled |
| Rate limits ineffective if attacker has many NodeIds | Spam possible with Sybil identities | Capability tokens required for channel publish (not issued to Sybils) |
| Moderation notice itself can be forged | Fake moderation notices | Moderation notices require valid authority cert; verified in UI |

---

## 8. Design Rationale

The IRIS moderation philosophy prioritizes:
1. **Never suppress real emergency information** — even imperfect or unverified information may save lives
2. **Authority verification as primary gate** — access control before transmission is the main moderation tool
3. **Transparency** — flagged content is marked, not hidden; users can see it and make informed decisions
4. **Distributed by necessity** — there is no moderator with global view; the system is designed to function without one

This is fundamentally different from social media moderation, where the platform has both authority and capability to remove content globally. IRIS accepts the impossibility of global content control in exchange for resilience in adversarial environments.
