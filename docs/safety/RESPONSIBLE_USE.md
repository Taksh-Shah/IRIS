# Responsible Use Policy

## Purpose

This policy defines what IRIS is for, who it serves, what uses are permitted, what uses
are prohibited, and how enforcement works. It is paired with the SAFETY_CHARTER.md which
governs the underlying principles. This policy governs user-facing use.

## Intended Users

IRIS is designed for:
- **General public** in disaster-prone areas (coastal, seismic, flood zones)
- **Women and vulnerable individuals** seeking personal safety communication
- **Family units** wanting reliable emergency communication between members
- **Community organizations** coordinating local disaster response
- **NGOs and relief organizations** providing disaster relief
- **Emergency services** (NDRF, SDRF, fire brigades, medical teams) in field operations
- **Local governments** disseminating verified emergency information
- **Enterprises** providing safety communication for remote workers, construction sites,
  mines, offshore facilities

## Permitted Uses

The following uses are explicitly supported and encouraged:

### Personal Safety
- Sending SOS when in danger or distress
- Sharing real-time location with trusted contacts during unsafe situations
- Communicating with emergency contacts when cellular networks are down
- Women's safety apps integration (SOS trigger, location sharing)

### Disaster Response
- Text communication during natural disasters when towers are down
- Coordinating evacuation routes and shelter locations with community
- Relay of official emergency broadcast from verified authorities
- Family reunification messaging after displacement

### Community Coordination
- Local community groups for neighborhood safety
- Village emergency response coordination
- Gram panchayat level disaster communication

### Official Emergency Services
- Field team coordination (NDRF, SDRF, fire, medical)
- Search and rescue team coordination
- Medical status updates from field to base
- Official emergency broadcast to affected population (with authority certificate)

### Enterprise Safety
- Worker safety communication in remote locations (mining, construction, offshore)
- Off-grid site communication
- Supply chain emergency communication
- Safety compliance monitoring

### Research and Education
- Academic research on mesh networking and disruption-tolerant networking
- Testing and development using sandbox/testnet mode
- Training for emergency responders using DRILL mode (non-production certificates)
- Education about communication resilience

## Prohibited Uses

The following uses are prohibited by this policy and where technically possible, by the system:

### Prohibited — Safety-Critical

**1. Sending false emergency alerts**
Sending SOS or using emergency channels for non-emergencies, pranks, testing without DRILL
mode, or to cause a response to a false situation. This is:
- A technical violation (rate limits, accountability)
- A safety violation (diverts resources from real emergencies)
- A legal violation (IPC Section 505, public mischief; Section 182, false information to public servant)

**2. Emergency broadcast without authority certificate**
Attempting to broadcast emergency messages to the public without a valid authority certificate.
Technical enforcement: unsigned broadcasts dropped at every relay.

**3. Interference with emergency response**
Any action that degrades the ability of emergency services to communicate, including DoS
attacks, flooding P0-P2 channels, route poisoning.

### Prohibited — Legal and Ethical

**4. Harassment and stalking**
Using IRIS to harass, threaten, or stalk other users. This includes:
- Repeated unwanted messages after being blocked
- Threatening messages
- Using location sharing features to track someone without consent

**5. Criminal coordination**
Using IRIS to plan or coordinate criminal activity, including but not limited to:
- Violent crime
- Organized financial crime
- Drug trafficking coordination
- Human trafficking

**6. Terrorism and extremism**
Any use to plan, coordinate, fund, or incite terrorism or violent extremism. This
includes propaganda for proscribed organizations under India's Unlawful Activities
(Prevention) Act (UAPA).

**7. Illegal content distribution**
Distributing child sexual abuse material (CSAM) or other content illegal under
Indian law via IRIS public channels (note: IRIS cannot inspect E2EE private messages).

**8. Unauthorized surveillance**
Using IRIS to monitor another user's location or communications without their knowledge
and consent. See PRIVACY_THREATS.md — IRIS is designed to prevent this.

**9. Mass spam**
Using IRIS or automated tools to send unsolicited messages at volume. See SPAM_RESISTANCE.md
for technical enforcement.

## User Responsibilities

By using IRIS, you accept the following responsibilities:

### Identity Accountability
Your IRIS identity is linked to the device that generated your key pair. Messages you send
are signed with your private key. You are responsible for:
- Keeping your private key secure
- Reporting compromise immediately (key compromise invalidates your identity)
- All messages sent with your key (do not share your key)

### Relay Responsibility
IRIS nodes relay messages for the mesh. By running IRIS, you are a relay node. You agree:
- To relay messages for the mesh in good faith
- Not to selectively block messages for anti-competitive or discriminatory reasons
- To apply rate limits and spam filters as configured
- To report abuse through the appropriate channels

### Gateway Operator Compliance
If you operate a gateway node (connecting IRIS mesh to Internet), you additionally agree:
- To comply with local telecommunications regulations
- To maintain records as required by law
- To respond to valid legal process (see LAW_ENFORCEMENT_REQUESTS.md)
- Not to use gateway position to surveil mesh traffic

## Enforcement

### Technical Enforcement
- Rate limits enforced at every relay (see DOS_RESISTANCE.md)
- Block lists applied at device level
- Authority certificate verification for emergency channels
- SOS rate limits and audit trails

### Community Enforcement
- Public channel moderation by community admins
- Community spam reporting
- Shared block lists (opt-in)

### Legal Enforcement
- Abuse reports forwarded to law enforcement with valid legal process
- IRIS cooperates with lawful investigation (see LAW_ENFORCEMENT_REQUESTS.md)
- Violation of this policy may result in legal action by IRIS Foundation

## Policy Updates

This policy may be updated when:
- New use cases emerge that require explicit policy
- Legal environment changes (new laws, court decisions)
- Technical capabilities change enforcement options

Material changes published 30 days before effective date. Emergency changes (safety-critical)
may be effective immediately with simultaneous publication.
