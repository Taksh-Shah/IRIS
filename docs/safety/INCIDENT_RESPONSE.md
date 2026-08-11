# Incident Response

## Overview

IRIS handles incidents systematically with clear severity categories, defined response
owners, timelines, and post-mortem processes. Because IRIS is used for emergency
communication, incidents that affect system reliability have safety consequences beyond
normal software systems.

## Incident Severity Categories

### P0 — Emergency System Compromise

**Definition:** The IRIS emergency system is actively compromised in a way that could
cause harm to real emergency response:
- Fake emergency broadcasts being accepted and displayed as legitimate across the network
- Emergency authority private key compromise (key exposure or theft)
- Active, ongoing fake SOS flood causing responders to be overwhelmed
- Routing attack that is successfully blocking delivery of real P0 SOS messages at scale

**Response time:** Response within 1 hour, 24/7/365.
**Team:** All-hands. CTO, Head of Security, Safety Board representative.
**User notification:** Public disclosure within 24 hours. No "wait and see."
**Resolution target:** Emergency mitigations within 4 hours. Full fix within 48 hours.

### P1 — Privacy Breach

**Definition:** User data has been exposed, transmitted to wrong parties, or decryption
has been compromised for a significant number of users:
- Bug causing messages routed to wrong recipient
- Encryption bug exposing payload to relay nodes
- Key management bug exposing private keys
- Metadata exposure beyond what protocol intends

**Response time:** Acknowledgment within 2 hours, investigation within 4 hours.
**Team:** Security team + affected-area engineering team.
**User notification:** Notify affected users within 48 hours.
**Resolution target:** Fix within 7 days. Emergency mitigation within 24 hours.

### P2 — Service Abuse Campaign

**Definition:** Active, organized abuse of the IRIS service affecting a meaningful
fraction of users:
- Coordinated spam campaign overloading relay storage
- DoS attack successfully degrading service for a region
- Coordinated fake SOS campaign (below P0 threshold — not yet causing safety harm)
- Sybil attack at network scale

**Response time:** Triage within 4 hours.
**Team:** Engineering team (abuse prevention), with safety board notification.
**Resolution target:** Mitigations deployed within 72 hours.

### P3 — Security Vulnerability Found (Not Yet Exploited)

**Definition:** A security vulnerability has been discovered but is not currently being
exploited or causing user harm:
- External researcher responsible disclosure
- Internal security review finding
- Fuzzing crash that reveals a security-relevant code path
- Protocol design weakness identified in analysis

**Response time:** Acknowledgment within 24 hours. Triage within 1 week.
**Team:** Security team.
**Resolution target:** Fix within 90 days (coordinated disclosure window).

### P4 — Individual User Complaint

**Definition:** An individual user reports an issue:
- Harassment via IRIS messages
- Specific spam messages not blocked by filters
- Suspected account compromise
- Content moderation dispute

**Response time:** Acknowledgment within 5 business days.
**Team:** User support, escalate to security if warranted.
**Resolution target:** Response to user within 10 business days.

## P0 Response Playbook

### Immediate Actions (within 1 hour)

1. **Confirm incident is real**: triage team confirms P0 criteria are met
2. **Assemble response team**: page all-hands via emergency response channel
3. **Assess scope**: which geographic areas? Which IRIS versions? How many users?
4. **Isolate if possible**: can we prevent spread without taking down service?
   - If fake broadcast: revoke the authority certificate immediately
   - If key compromise: revoke the compromised key + all certificates issued by it
   - If routing attack: push routing defense update

### Emergency Certificate Revocation (if applicable)

```bash
# Generate revocation message (on secure signing device)
iris-admin revoke-certificate \
  --serial <certificate_serial> \
  --reason "COMPROMISED" \
  --issuer-key <issuer_signing_key> \
  --output revocation_signed.cbor

# Broadcast revocation to mesh
iris-admin broadcast-revocation \
  --revocation revocation_signed.cbor \
  --priority P1 \
  --force-all-transports
```

Revocation propagates to all connected nodes within minutes. Offline nodes receive
revocation on next connection.

### Communication (P0)

- **Internal**: safety board notified immediately via emergency contact list
- **Public** (within 24 hours): post to IRIS status page, social media, and notify
  registered media contacts
- **Authorities**: notify NDMA/CERT-In if incident involves emergency system compromise
- **Affected users**: in-app notification if contact information is available

### Post-Mortem Requirements (P0 and P1)

Within 30 days of resolution:
- Complete post-mortem document published at trust.iris.app
- Root cause analysis
- Timeline of events
- What worked in the response
- What did not work
- Action items with owners and deadlines
- Systemic changes to prevent recurrence

Post-mortem is public. No blame, but full technical detail.

## Responsible Disclosure Process

### For External Researchers

1. **Report to**: security@iris.app (PGP key: [trust.iris.app/pgp.asc])
2. **Include**: description, steps to reproduce, affected versions, potential impact
3. **We acknowledge**: within 24 hours (business days), faster for critical issues
4. **We provide**: a case number and point of contact
5. **Fix window**: 90 days standard; less for active exploitation
6. **Coordinated disclosure**: we publish details at time of patch release, crediting researcher
7. **Rewards**: see bug bounty program at trust.iris.app/bounty

### What Researchers Should NOT Do

- Exploit the vulnerability against real users
- Access, modify, or delete user data
- Conduct DoS attacks against production infrastructure
- Disclose publicly before coordinated disclosure window

We commit to not pursue legal action against researchers who follow this process.

## Bug Bounty Program

| Severity | Examples | Reward (INR) |
|----------|----------|--------------|
| Critical | Auth bypass, RCE, fake emergency broadcast at scale | ₹5,00,000+ |
| High | Privacy breach, E2EE bypass, authority cert forgery | ₹2,00,000 |
| High | SOS cancel forgery, fake SOS ACK | ₹1,00,000 |
| Medium | DoS that works in production, significant metadata leak | ₹50,000 |
| Low | Informational disclosure, minor bugs | ₹10,000 |

Rewards paid in INR via bank transfer or UPI. Halved for duplicate reports.
Paid only after fix is deployed and researcher confirms fix is acceptable.

## Incident Communication Channels

- **Internal**: #iris-security-incident (private Slack channel, response team only)
- **Status page**: status.iris.app (updated within 1 hour of incident confirmation)
- **User notification**: in-app banner for P0-P1
- **Developer notification**: security advisory email list (sign up: trust.iris.app)
- **Media**: press@iris.app (routed to communications team)

## Lessons Incorporated

Each P0/P1 incident generates:
1. A security regression test (ensures bug cannot recur)
2. An update to this document (if response process needs improvement)
3. A safety board review (if emergency system was affected)
4. A review of detection mechanisms (why was this not caught earlier?)

The IRIS security posture improves with each incident through systemic learning,
not through blame assignment.
