# IRIS Safety Charter

## Preamble

IRIS — the Resilient Communication Fabric — exists because communication saves lives.
When a flood separates a family, when an earthquake buries a city under silence, when a
woman walking home at night needs help, communication is the difference between survival
and tragedy. IRIS is built to provide that communication when all other systems have failed.

This charter defines the safety principles that govern every design decision in IRIS.
It is not a legal document. It is an engineering and ethical commitment that the IRIS
team holds itself to — even when complying with it is difficult, expensive, or unpopular.

## Core Mission

IRIS is designed to **save lives and improve safety**. This is not an aspiration — it is
the filter through which all design decisions are evaluated. A feature that saves lives is
in. A feature that enables harm is out. When the answer is ambiguous, the safety board
(defined below) makes the call.

## Four Principles

### 1. Privacy with Accountability

Every IRIS user has a right to private communication. No third party — including the IRIS
team — should be able to read private messages between consenting adults.

Simultaneously, privacy cannot be absolute. A person who abuses the emergency system,
who harasses others, or who uses IRIS for violence has forfeited some claim to privacy.
IRIS is designed so that:
- **Message content is private** (E2EE, we cannot decrypt)
- **Identity is accountable** (non-anonymous, every message signed)
- **Abuse has consequences** (identity-linked rate limits, blocks, reporting)

The balance: protect the innocent from surveillance; provide accountability for abusers.
This means IRIS is NOT anonymous. This is a deliberate choice, documented in
PRIVACY_THREATS.md.

### 2. Emergency Integrity

The emergency system must work correctly, or people die. Emergency integrity means:

- **P0 SOS always gets resources**: no spam, no DoS, no other traffic can prevent SOS delivery
- **Emergency broadcasts are authenticated**: only verified authorities can broadcast
- **Emergency channels cannot be weaponized**: defenses in EMERGENCY_ABUSE.md
- **False emergency has consequences**: rate limits, accountability, legal exposure

Emergency integrity supersedes other considerations. If a feature increases functionality
but degrades emergency reliability, the feature is rejected or modified.

### 3. Responsible Governance

IRIS is not owned by a single company or government. Its governance must reflect the
interests of the communities it serves:

- Safety decisions made by a multi-stakeholder safety board (civil society, government,
  emergency services, technologists, affected communities)
- Architecture decisions evaluated against this charter before implementation
- Transparency: safety policies published publicly
- No unilateral decisions by the IRIS engineering team on safety matters

This charter is a governance document, not just a technical document.

### 4. Legal Compliance

IRIS operates within the law of the jurisdictions where it is deployed. For India:
- Information Technology Act, 2000 and its amendments
- CERT-In directions
- Applicable state laws for emergency communication

Compliance does not mean compliance at any cost. Where law conflicts with user safety
(e.g., a law requiring surveillance of vulnerable domestic violence victims), IRIS will:
1. Publish transparency reports disclosing the conflict
2. Work with civil society to address the conflict through legal/advocacy channels
3. Implement the minimum technically compliant interpretation

## What IRIS Is Designed To Do

- Enable communication when infrastructure fails (earthquake, flood, cyclone)
- Protect personal safety, especially for women, vulnerable populations, lone workers
- Coordinate disaster response for NGOs, community organizations, emergency services
- Support emergency services (NDRF, SDRF, fire, medical) in field coordination
- Enable family reunification during displacement events
- Bridge the last mile when towers and fiber are down

## What IRIS Is NOT Designed To Do

- Enable anonymous criminal coordination
- Evade lawful investigation through technical obfuscation
- Facilitate terrorism or violent extremism
- Replace regulated telecommunications (IRIS is supplementary, not a replacement)
- Provide anonymity (see Privacy Principle above)
- Operate outside applicable law

Note: "not designed to do" does not mean "technically impossible to do." IRIS cannot
inspect E2EE content. Bad actors may attempt to misuse any communication platform.
IRIS minimizes misuse through accountability design, not through content inspection.

## Prohibited Features

The following features are explicitly prohibited in IRIS, regardless of demand:

1. **Anonymous messaging**: all messages must be signed with a persistent identity
2. **Content-visible to operator**: E2EE is inviolable; the IRIS team cannot read messages
3. **Emergency bypass for police/government without court order**: see LAW_ENFORCEMENT.md
4. **Surveillance mode**: no feature that enables one user to silently monitor another
5. **Location tracking without consent**: see LOCATION_SHARING.md

## Safety Board

The IRIS Safety Board is a multi-stakeholder body that:
- Reviews and approves changes to this charter
- Adjudicates safety disputes in design decisions
- Reviews post-mortems for safety incidents
- Advises on law enforcement cooperation policy

**Composition:**
- 2 representatives from civil society organizations (e.g., NGOs serving disaster victims)
- 1 representative from emergency services (NDRF/SDRF)
- 1 representative from women's safety organizations
- 1 technologist (rotating, independent)
- 1 legal expert in technology law

**Process:** Safety board meets quarterly. Emergency meeting for P0 incidents within 24h.
Decisions by simple majority. Charter amendments require unanimous vote.

## Architecture Decision Review

All architecture decisions that affect:
- Emergency system reliability
- User privacy
- Authentication and identity
- Law enforcement cooperation
- Data retention

Must include a safety impact section in the design document that references this charter.
Design reviews for these decisions include a safety board member.

## Living Document

This charter is reviewed annually by the safety board. Version history maintained in git.
All IRIS team members are required to read and acknowledge this charter annually.

**Version 1.0** — First publication
**Effective date**: IRIS v0.1 initial design
**Next review**: annually from first publication date
