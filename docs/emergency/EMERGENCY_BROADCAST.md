# Emergency Broadcast System

## Overview

Emergency broadcasts are official messages from verified government and emergency authorities
to all IRIS users in a geographic area. Unlike personal SOS (which alerts specific people),
emergency broadcasts alert an entire population. Because of this reach and authority, they
require strict authentication and are the most tightly controlled message type in IRIS.

## Who Can Broadcast

Only nodes with a valid, non-expired, non-revoked authority certificate in the IRIS emergency
authority hierarchy. See EMERGENCY_GOVERNANCE.md for the full hierarchy.

In practice for India:
- NDMA: national-scope broadcasts
- SDMAs: state-scope broadcasts
- DDMAs: district-scope broadcasts
- Specialized authorities: medical, SAR, fire

A user without an authority certificate cannot send an emergency broadcast, regardless of
their intent. Technical enforcement makes this impossible to circumvent.

## Broadcast Message Format

```rust
pub struct EmergencyBroadcast {
    // Identification
    pub broadcast_id: [u8; 16],     // UUID v4 — unique per broadcast
    pub authority_id: NodeId,        // NodeId of the broadcasting authority
    
    // Certificate chain (embedded in message — enables offline verification)
    pub certificate_chain: Vec<AuthorityCertificate>,  // Leaf → root order
    
    // Geographic scope
    pub area_code: String,           // ISO 3166-2 + district (e.g., "IN-GJ-19" = Kutch)
    pub area_description: String,    // Human readable (e.g., "Kutch District, Gujarat")
    
    // Classification
    pub severity: BroadcastSeverity,
    pub message_type: EmergencyMessageType,
    pub language: String,            // BCP 47 (e.g., "hi", "gu", "en")
    
// Content
    pub headline: String,            // Compact ≤96 chars — displayed in notification
    pub instructions: Option<String>, // Compact ≤96 chars; full detail publishes as a
                                      // signed-follow-up UPDATE (EMERG_DESIGN.md C1)
    pub additional_info: Option<String>, // Links, contacts (when Internet available)
    
    // Timing
    pub issued_at: i64,             // Unix timestamp
    pub expires_at: i64,            // When broadcast is no longer relevant
    
    // Signature
    pub signature: Ed25519Signature, // Signed by authority's private key
}

pub enum BroadcastSeverity {
    Critical,   // Immediate life threat — mandatory alert sound + vibration
    High,       // Serious threat — alert sound + vibration
    Medium,     // Significant concern — in-app notification
    Low,        // Informational — in-app notification, no sound
    AllClear,   // Previous threat has passed — special UI treatment
}

pub enum EmergencyMessageType {
    Evacuation,         // Evacuate the area
    Shelter,            // Shelter in place
    Relief,             // Relief services available at location
    MedicalAlert,       // Medical emergency in area
    HazardWarning,      // Chemical, flood, earthquake hazard
    AllClear,           // Previous emergency resolved
    SearchAndRescue,    // SAR operation in area, report sightings
    CurfewRestriction,  // Movement restriction
    Infrastructure,     // Road closure, bridge damage, utilities
    General,            // General information
}
```

## Real-World Example

A district collector in Kutch (Gujarat) sends an evacuation order during Cyclone Biparjoy:

```json
{
  "broadcast_id": "550e8400-e29b-41d4-a716-446655440000",
  "authority_id": "ab12...cd34",
  "area_code": "IN-GJ-19",
  "area_description": "Kutch District, Gujarat",
  "severity": "Critical",
  "message_type": "Evacuation",
  "language": "gu",
  "headline": "ભૂજ જિલ્લો ખાલી કરો — ચક્રવાત ચેતવણી",
  "instructions": "ભૂજ જિલ્લામાં રહેતા તમામ નાગરિકો NH-27 પર ઉત્તર તરફ જાઓ. ભૂજ સ્ટેડિયમ ખાતે નોંધણી કેન્દ્ર ઉભું કરાયું છે. ID સાથે આવો. 3 દિવસ માટે ખોરાક-પાણી લઈ આવો.",
  "issued_at": 1687500000,
  "expires_at": 1687672800
}
```

The message is signed with the district authority's Ed25519 key. Every relay verifies this
before forwarding. The message includes the certificate chain up to NDMA.

## Certificate Verification at Every Relay

Before forwarding any emergency broadcast, a relay performs full certificate verification:

```rust
pub fn verify_and_relay_broadcast(
    broadcast: &EmergencyBroadcast,
    relay_state: &RelayState,
) -> ForwardDecision {
    
    // Step 1: Check we haven't seen this broadcast_id (deduplication)
    if relay_state.seen_cache.contains(&broadcast.broadcast_id) {
        return ForwardDecision::AlreadyProcessed;
    }
    
    // Step 2: Check expiry
    let now = unix_timestamp_now();
    if now > broadcast.expires_at {
        return ForwardDecision::Expired;
    }
    
    // Step 3: Verify certificate chain
    let verification = verify_certificate_chain(
        &broadcast.certificate_chain,
        &relay_state.root_cert_store,
        now,
        &AreaCode::parse(&broadcast.area_code),
    );
    
    match verification {
        Ok(verified) => {
            // Step 4: Verify broadcast signature against certified public key
            let authority_public_key = verified.cert.subject_id.to_public_key()?;
            let signed_bytes = broadcast.canonical_signed_bytes();
            
            if verify_ed25519(&authority_public_key, &signed_bytes, &broadcast.signature).is_err() {
                relay_state.log_rejection("BROADCAST_SIG_INVALID", &broadcast.broadcast_id);
                return ForwardDecision::Rejected;
            }
            
            // Step 5: Forward and display
            relay_state.seen_cache.insert(broadcast.broadcast_id);
            relay_state.alert_local_ui(broadcast);  // Display immediately
            ForwardDecision::ForwardToAll  // Relay to ALL neighbors
        }
        Err(e) => {
            relay_state.log_rejection(&format!("CERT_FAIL: {:?}", e), &broadcast.broadcast_id);
            ForwardDecision::Rejected
        }
    }
}
```

An invalid certificate or signature → DROP + log. No error is returned to sender (prevents probing).

## Mandatory Relay

Unlike normal messages where a relay can decline to forward (storage pressure, rate limits),
emergency broadcasts MUST be relayed by every node that receives a valid one:

```rust
// No storage quota check for emergency broadcasts
// No rate limit check for emergency broadcasts  
// No backpressure backoff for emergency broadcasts
// Emergency broadcasts are always relayed immediately
```

This is a protocol requirement. A node that refuses to relay a valid emergency broadcast
is violating IRIS protocol and should be considered a blackhole (see ROUTING_ATTACKS.md).

## Propagation Characteristics

- **Routing**: flood — sent to ALL neighbors, regardless of routing table
- **Replication factor**: full epidemic (every node that receives it sends to every neighbor)
- **Hop limit**: none (max_hops = 255)
- **TTL**: set to `expires_at - issued_at` (broadcasts expire, do not linger forever)
- **Priority**: P3 (emergency text) for routing purposes; special handling for display

## UI Requirements

When a valid emergency broadcast is received:
1. Override current UI immediately (even if user is in another screen)
2. Display full-screen alert with authority name, severity color, and message
3. Play alert sound (CRITICAL/HIGH) or notification sound (MEDIUM/LOW)
4. Vibrate according to severity
5. Require explicit acknowledgment before dismissal ("I understand")
6. Keep broadcast accessible in "Emergency Alerts" section after dismissal

CRITICAL broadcasts cannot be silenced by mute/DND. This is OS-level behavior on Android
(WEA channel) and iOS (Government Alerts). IRIS uses these OS channels when available.

## Broadcast Cancel

An authority can cancel a previous broadcast (e.g., false alarm, situation resolved):

```rust
pub struct BroadcastCancel {
    pub original_broadcast_id: [u8; 16],
    pub cancel_reason: String,  // e.g., "Threat has passed" or "Issued in error"
    pub authority_id: NodeId,
    pub signature: Ed25519Signature,  // Signed with same authority key
}
```

Cancel is delivered as P3 message. Receiving nodes update the stored broadcast with
cancelled status. UI shows cancelled indicator next to the original broadcast.

## Broadcast Audit at Every Node

```rust
pub struct BroadcastAuditEntry {
    pub broadcast_id: [u8; 16],
    pub authority_id_hash: [u8; 16],  // Pseudonymous hash
    pub area_code: String,
    pub severity: BroadcastSeverity,
    pub message_type: EmergencyMessageType,
    pub issued_at: i64,
    pub received_at: i64,          // When this relay first saw it
    pub was_cancelled: bool,
    pub cancel_received_at: Option<i64>,
}
```

Broadcast audit retained for 1 year. The audit log contains NO broadcast content —
only metadata. Useful for post-disaster analysis and accountability review.
