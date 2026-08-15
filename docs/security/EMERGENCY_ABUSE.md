# Emergency System Abuse Prevention

## Overview

The IRIS emergency system (P0 SOS, P1 Medical, emergency broadcast) receives privileged
treatment: higher priority routing, reserved storage, rate-limit bypass, authority-verified
broadcast channels. This privileged access creates an incentive for abuse. This document
defines the attack scenarios and the defenses that protect the emergency system's integrity
without compromising its availability for genuine emergencies.

## Attack Scenarios

### Scenario 1: Fake SOS Flood

An attacker sends thousands of SOS messages, overwhelming emergency responders with false alerts.
Real SOSes cannot be found among the noise. Responders waste time and resources on fakes.

**Risk level**: Critical. Could prevent response to real emergencies.

### Scenario 2: Emergency Broadcast Spoofing

An attacker without authority certificate crafts a fake emergency broadcast claiming an
evacuation order, mass casualty event, or all-clear for a real disaster. Recipients panic,
evacuate unnecessarily, or fail to evacuate when they should.

**Risk level**: Critical. Potential for mass panic, stampede, or delayed real response.

### Scenario 3: Fake Location Leading Responders Astray

An attacker sends a valid SOS but with false GPS coordinates. Responders are directed to
an empty location, wasting time while the real emergency victim goes unhelped.

**Risk level**: High. Diverts limited emergency resources.

### Scenario 4: Coordinated Mass SOS

Multiple coordinated attackers each send SOS simultaneously from different identities,
overwhelming the system and preventing real SOS from being noticed.

**Risk level**: High. Requires Sybil attack as prerequisite.

### Scenario 5: SOS Cancel Attack

Attacker intercepts (or predicts) a real SOS message_id and sends a forged CANCEL_SOS,
leading responders to believe the emergency is resolved.

**Risk level**: High. CANCEL_SOS must be authenticated.

## Defenses for SOS (P0)

### SOS Rate Limiting

Every relay node enforces SOS rate limits per sender identity:
- Maximum 3 SOS messages per hour per NodeId
- After 3rd SOS: subsequent SOS from same identity downgraded to P3 (not dropped, but
  no longer receives P0 routing priority or storage guarantee)
- Rate limit resets on successful CANCEL_SOS receipt (genuine emergency resolved)
- Rate limit window: rolling 60-minute window

Rationale: a genuine emergency requires at most a few SOS retransmissions. 3 per hour
is generous for legitimate use and prohibitive for mass flooding.

### SOS Cryptographic Accountability

Every SOS is signed with the sender's Ed25519 private key. The sender_id (public key hash)
is embedded in the SOS. This means:
- Every fake SOS is attributable to a specific identity
- That identity can be blocked by recipients and emergency responders
- Post-incident audit can identify source of fake SOS
- Social cost of fake SOS: identity gets permanently blocked in community

This does not prevent abuse by throwaway identities (Sybil) — addressed separately.

### Signed SOS Cancel

CANCEL_SOS requires the same signing key as the original SOS. Format:
```
CANCEL_SOS {
  original_message_id: "abc123",
  cancel_timestamp: now,
  reason: "false_alarm" | "resolved" | "testing",
  sender_id: sender_id,
  signature: Ed25519_sign(sender_private_key, [original_message_id, cancel_timestamp, reason])
}
```

A CANCEL_SOS from a different identity than the original SOS sender is rejected.
An attacker cannot cancel another user's genuine SOS.

Window for cancel: CANCEL_SOS must arrive within 60 minutes of original SOS.
After 60 minutes, SOS cannot be cancelled (responders have already been dispatched;
cancellation at that point goes through responder coordination).

### SOS Audit Log

Every relay node logs SOS metadata (not content) to a local audit store:
```
SOS_LOG_ENTRY {
  message_id: hash,
  sender_id: hash,  # pseudonymous, not plaintext
  timestamp: unix_timestamp,
  relay_received_at: unix_timestamp,
  hop_count: u8,
  priority: 0,
  was_cancelled: bool,
  cancel_timestamp: Option<unix_timestamp>
}
```

Audit log retained for 30 days. Used for:
- Post-incident investigation (was there a flood attack?)
- Responder coordination (how many SOSes from same area?)
- Aggregate statistics for system improvement

Audit log contains no message content, only routing metadata.

## Defenses for Emergency Broadcast

### Authority Certificate Requirement

Emergency broadcast messages MUST include a valid authority certificate chain.
Every relay verifies the certificate chain before forwarding.

```
Certificate chain verification:
  1. Extract certificate chain from broadcast message header
  2. Verify chain up to embedded root certificate (preloaded on all nodes)
  3. Check certificate not expired (valid_from <= now <= valid_until)
  4. Check certificate geographic scope matches current area_code
  5. Check certificate has BROADCAST permission
  6. Verify broadcast message signed with authority private key
  
  Any failure → DROP immediately, log rejection
```

Without a valid certificate chain from a recognized authority, the broadcast is dropped
at every relay. No certificate → no broadcast. Period.

### Certificate Preloading

Root authority certificates and the first 2 levels of the hierarchy are preloaded on
all IRIS installations. This enables offline certificate verification without any
Internet connection.

Preloaded certificates:
- National authority public keys (NDMA equivalents)
- State authority public keys (all 28 states + 8 UTs for India deployment)
- District authority certificates distributed at onboarding (via QR code or NFC)

Certificate updates: when Internet is available, IRIS fetches updated certificate lists
from well-known IRIS authority servers. Updates signed with master key.

### Certificate Revocation

Revoked certificates are broadcast via the normal IRIS message network (P1 priority)
as signed revocation notices. Every relay caches the revocation list.

Revocation propagates within minutes to all connected nodes. Nodes that were offline
receive revocation list upon reconnection.

## Defenses for Fake Location

### Explicit User Consent Required

Location is NEVER automatically attached to a SOS without user consent. The SOS flow:
1. User triggers SOS (3-second hold)
2. Confirmation screen shows: "Include your GPS location? (Recommended)"
3. User explicitly taps "Yes" or "No"
4. GPS location included only if "Yes"

User can pre-configure a preference in settings, but first-use always asks.

### Location Flagged as User-Provided

When location IS included, the SOS message indicates:
```
location_source: GPS | NETWORK | USER_MANUAL | NONE
location_accuracy_m: u16
```

Responders know the location accuracy. A `USER_MANUAL` location (user typed coordinates)
should be treated with appropriate skepticism.

### Out-of-Band Verification

The SOS message includes the sender's IRIS contact information. Emergency responders can:
- Send a reply message asking for voice call
- Call the attached phone number (if included)
- Send a location verification request (recipient confirms on their device)

Location verification is a human process, not automated. IRIS enables it; responders use it.

## Governance and Accountability

### Emergency Authority Hierarchy

```
National Emergency Authority (NDMA level)
  └── State Emergency Authorities (SDMA level, 28 states + 8 UTs)
        └── District Emergency Authorities
              └── Local Emergency Authorities (municipality / tehsil)
```

Each level can issue certificates to the level below. National authority cannot be
issued certificates from below (bootstrap trust anchor).

### Certificate Scope Enforcement

Each authority certificate has a geographic scope:
- National: entire country
- State: one state code (e.g., "IN-GJ" for Gujarat)
- District: one district code (e.g., "IN-GJ-19" for Kutch)

A district authority cannot send broadcasts beyond their district. Scope is verified
at every relay using the area_code field.

### Consequence for Abuse

- Certificate revocation: immediate, propagates to all nodes within minutes
- Legal action: emergency system abuse is a criminal offense under the Bharatiya
  Nyaya Sanhita (BNS) **2023**, Section **318** (public mischief; successors IPC
  §505) and potentially Section **353(2)** (fraud/dishonest inducement;
  successors IPC §420). Communal-disharmony offences under IPC §153A have no
  direct BNS groove yet — mapping flagged for <LEGAL-001> (C4).
- Permanent identity block: abusing identity blocked by all nodes that saw the abuse

## Testing Emergency Defenses

A dedicated test mode exists for drills:
- Test authority certificates (marked TEST, not valid for real emergencies)
- Test broadcasts are visually distinguished (yellow border, "DRILL" label)
- Test SOSes logged separately from real SOSes
- Test cancel available at any time without window restriction

Do not use real authority certificates for drills. Real certificates are audited.
