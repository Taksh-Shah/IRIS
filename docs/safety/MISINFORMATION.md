# Misinformation in Emergency Scenarios

## The Problem

During disasters, misinformation can kill as surely as the disaster itself. In the 2001 Gujarat
earthquake, rumors of aftershocks caused rescuers to abandon partially-excavated survivors.
In the 2013 Uttarakhand floods, false "all clear" signals caused families to return to flooded
areas. Fake evacuation routes have led people toward danger rather than away from it.

IRIS, as a communication platform for disaster scenarios, has a responsibility to address this.
But IRIS also has hard limits on what it can do — honest acknowledgment of these limits is
essential to designing around them rather than pretending they do not exist.

## What IRIS Cannot Do

### Cannot Inspect E2EE Private Messages

Private messages between contacts are end-to-end encrypted. IRIS relay nodes cannot read
message content. A misinformation message in a private conversation is invisible to any
centralized moderation system. This is intentional — E2EE protects victims of abuse from
surveillance. We cannot compromise E2EE to fight misinformation.

### Cannot Moderate in Real Time Across Offline Mesh

When a node is offline and the mesh is partitioned, moderation commands cannot reach that
partition. A misinformation message may circulate in an isolated area for the duration of
the partition — which could be hours during a disaster.

### Cannot Verify Factual Claims

IRIS has no fact-checking capability. We cannot determine whether "Evacuate Block 7" is
accurate or false. This is a human judgment call requiring domain knowledge we do not have.

### Cannot Prevent All Forwarding

In a mesh network designed to relay emergency information, we cannot prevent forwarding
of ordinary messages — that would break the core mission. We can rate-limit; we cannot block.

## What IRIS Can Do

### 1. Source Authenticity — Verifiable Sender Identity

Every IRIS message carries a verifiable sender identity (Ed25519 signature → NodeId).
Recipients can always know WHO sent a message, and that identity is non-repudiable.

This does not tell you if the information is correct, but it tells you:
- Is this from a verified emergency authority? (certificate chain)
- Is this from someone I know and trust? (in my contact list)
- Is this from a stranger? (unknown NodeId)
- Has this sender been flagged as unreliable by my contacts?

Source identity is the most scalable anti-misinformation tool available in a decentralized
offline network. It shifts responsibility to the source and enables human-layer judgment.

**UI implementation**: every message displayed in IRIS shows:
- Sender display name (if in contacts) or NodeId (if unknown)
- Trust tier indicator (verified authority / contact / unknown)
- For emergency broadcasts: authority name and certificate verification status

### 2. Verified Emergency Channels — Authenticated Official Communication

Only holders of valid authority certificates can broadcast to the official emergency channel.
This means:
- An NDMA or SDMA broadcast is cryptographically authenticated
- Recipients know it came from a verified official source
- Fake "official" emergency broadcasts are impossible without compromising an authority key

This creates a two-tier information environment:
- **Verified channel**: official, authenticated, trust at emergency level
- **Community channels**: social trust, source-verifiable but not authority-verified

Recipients are trained to check which channel a message came from.

### 3. User Education and UI Design

The IRIS UI prominently displays information provenance:

```
┌─────────────────────────────────────────────────────┐
│ ⚠️  EMERGENCY BROADCAST                              │
│ From: Gujarat SDMA [✓ VERIFIED AUTHORITY]            │
│ Scope: IN-GJ-19 (Kutch District)                    │
│ Severity: CRITICAL                                  │
│                                                     │
│ "Evacuate Bhuj district immediately.                │
│  Move to NH-27 northbound."                         │
│                                                     │
│ Always verify with official sources when possible.  │
└─────────────────────────────────────────────────────┘
```

vs.

```
┌─────────────────────────────────────────────────────┐
│ 💬 Community Channel: Bhuj Residents               │
│ From: Ramesh Kumar [Community Member]               │
│                                                     │
│ "I heard the bridge is closed, use Station Road"    │
│                                                     │
│ ⓘ This message is NOT from a verified authority.   │
│    Verify before acting on routing advice.          │
└─────────────────────────────────────────────────────┘
```

The distinction between verified authority communication and community communication
is always visually clear and impossible to spoof (backed by certificate verification).

### 4. Community Channel Moderation

For public community channels, designated admins can remove messages that are:
- Demonstrably false (e.g., "evacuation route" that leads to danger)
- Panic-inducing without basis
- Coordinated misinformation campaigns

Moderation commands are signed by admin certificate and propagate as P3 messages.
Affected users see "This message was removed by channel moderator" with reason.

Limitation: moderation is delayed (not instantaneous), eventually consistent, and requires
the admin to be present in the partition where the misinformation circulates.

### 5. Reputation Flagging

Community members can flag messages as potentially misleading in public channels:
- Flagged messages are shown with a warning indicator
- After threshold (10% of channel members) → message shown with prominent warning
- Admin is notified of flagged message for review

Flagging does NOT remove the message automatically — prevents over-censorship.
It adds a signal that the message may warrant verification before acting on.

## Partnership With Official Authorities

The strongest anti-misinformation measure is having verified official communication
channels active before and during disasters.

IRIS partners with NDMA and state SDMAs to:
- Pre-issue authority certificates to designated district coordinators
- Train officials on sending verified emergency broadcasts via IRIS
- Ensure official IRIS broadcasts are the first and most visible communication
- Coordinate with AIR (All India Radio) and Doordarshan for cross-platform consistency

Partnership program target: all 775 district-level disaster management authorities
with issued IRIS authority certificates before the 2026 monsoon season.

## What Users Should Do

User training (in IRIS onboarding flow) covers:

1. **Check the source badge**: authority broadcast (green verified badge) vs. community message
2. **For routing advice**: verify with official sources before acting (call, check official apps)
3. **During rapid-onset emergencies**: follow verified authority broadcasts; hold on community
   rumors until verified
4. **Report suspected misinformation**: use the flag button in community channels
5. **Share verified information**: if you have ground truth, share it with your identity —
   your credibility is your community's credibility

## The Fundamental Limitation

A decentralized, offline-capable mesh network cannot solve the problem of misinformation at
a technical level. The internet with centralized platforms and unlimited moderation resources
cannot either. The best we can do:

1. Make source verifiable (we do this)
2. Make official communication distinguishable (we do this)
3. Give communities moderation tools (we do this)
4. Be honest about what we cannot do (we do this)
5. Partner with official authorities to be present in the verified channel (ongoing)
