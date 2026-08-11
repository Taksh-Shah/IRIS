# IRIS Resilient Communication Fabric — Project Charter

**Document ID:** IRIS-CHARTER-001  
**Version:** 1.0  
**Status:** Active  
**Last Updated:** 2025-08-10  
**Owners:** IRIS Core Team

---

## 1. Mission Statement

The IRIS Resilient Communication Fabric project exists to ensure that no human being loses the ability to call for help, coordinate with others, or receive critical information during a disaster, emergency, or infrastructure failure — regardless of whether cellular networks, the internet, or power grids are functional.

IRIS builds a **communication substrate** — a multi-transport, heterogeneous, disruption-tolerant mesh network — that works when everything else fails. It is infrastructure for the last mile of human communication, designed first for India and built for the world.

---

## 2. Vision

A world in which every mobile device, laptop, and low-cost embedded system participates in a self-healing communication mesh that:

- Requires **zero incremental infrastructure spend** during normal operation
- Activates **automatically** when conventional networks degrade
- Carries **SOS messages with absolute priority** regardless of network state
- Scales from two devices in a collapsed building to ten million devices across a disaster zone
- Is **open, auditable, and free** of proprietary lock-in at every layer

---

## 3. Project Scope

### 3.1 In-Scope

| Domain | Description |
|--------|-------------|
| **Core Networking** | Multi-transport mesh: BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite |
| **Disruption Tolerance** | Store-Carry-Forward (DTN) routing, epidemic, spray-and-wait, PRoPHET |
| **Emergency Priority** | P0 SOS system with mandatory propagation and no-drop guarantees |
| **Security** | End-to-end encryption (X25519 + ChaCha20-Poly1305), identity (Ed25519), Noise Protocol |
| **Multi-Platform** | Android 8+, iOS 14+, Linux/Windows/macOS desktop (Tauri), Raspberry Pi edge nodes |
| **Application Layer** | SDK + reference app for disaster response, safety, emergency coordination |
| **Gateway Bridge** | Internet bridging when partial connectivity exists |
| **Edge Node Management** | Dedicated always-on relay/gateway nodes for critical sites |

### 3.2 Out-of-Scope

| Item | Reason Excluded |
|------|----------------|
| Social networking / content platform | Mission creep; changes threat model |
| Financial transactions | Regulatory complexity; not emergency-critical |
| Anonymous darknet / evasion of lawful authority | Explicitly against design principles |
| Replacement for E911/112 emergency services | Complement, not replace; legal liability |
| Consumer messaging app | IRIS is infrastructure; apps are built on top |
| Centralized backend services | Contradicts zero-infrastructure philosophy |
| Voice/video calls in degraded modes | Too bandwidth-intensive for constrained transports |

---

## 4. Stakeholders

### 4.1 Primary Stakeholders

| Role | Responsibility |
|------|---------------|
| **Disaster-affected communities** | End users; their survival depends on correct operation |
| **Emergency responders** | NDRF, SDRF, fire, police, medical teams |
| **Government agencies** | NDMA, state disaster management authorities, municipal bodies |
| **Hospital systems** | Patient coordination, staff deployment during mass casualty events |
| **Women's safety networks** | SOS deployment in harassment, assault, trafficking scenarios |
| **Campus safety offices** | University, school, industrial campus deployment |

### 4.2 Secondary Stakeholders

| Role | Responsibility |
|------|---------------|
| **Open source community** | Code review, protocol analysis, security audits |
| **Telecom regulators** | TRAI (India), FCC (US), OFCOM (UK) — frequency licensing |
| **OEM partners** | Device manufacturers integrating IRIS at OS level |
| **NGOs / humanitarian orgs** | UNHCR, Red Cross — deployment in refugee, conflict zones |
| **Academic institutions** | Research, DTN protocol evolution, security analysis |

### 4.3 Anti-Stakeholders (Explicitly Not Served)

- Actors attempting to evade lawful interception for criminal purposes
- State actors attempting mass surveillance via protocol abuse
- Commercial advertisers seeking to exploit emergency context

---

## 5. Success Criteria

### 5.1 Technical Success Criteria

| Criterion | Target | Measurement Method |
|-----------|--------|--------------------|
| P0 SOS delivery in BLE-only scenario | ≤ 90 seconds within 5-hop radius | Controlled field test |
| Message delivery ratio in partial partition | ≥ 95% over 2 hours | Simulation + field test |
| Cold start time (app closed → mesh active) | ≤ 3 seconds | Automated test |
| Battery overhead (mesh active, no messages) | ≤ 5% per hour | Android/iOS power profiling |
| Max message overhead (P0 SOS envelope) | ≤ 128 bytes | Wire format analysis |
| Relay throughput (10 active nodes) | ≥ 50 messages/second | Benchmarking |
| Storage efficiency (deduplication rate) | ≥ 90% in epidemic scenario | Simulation |
| Security: key establishment time | ≤ 500ms for Noise handshake | Benchmarking |

### 5.2 Deployment Success Criteria

| Criterion | Target |
|-----------|--------|
| First field deployment (disaster exercise) | 6 months post v1.0 |
| Government agency evaluation | 12 months post v1.0 |
| Active daily nodes in India | ≥ 10,000 within 18 months |
| Independent security audit | Before v1.0 release |
| Protocol interoperability test | ≥ 3 independent implementations |

### 5.3 Mission Success Criteria

A life saved or harm prevented due to a message delivered over IRIS when all other communication had failed.

---

## 6. Budget Philosophy

**Principle: ₹0 incremental infrastructure cost during normal operation.**

IRIS is designed to run entirely on existing hardware: smartphones, laptops, and low-cost embedded systems (Raspberry Pi class). The only infrastructure costs are:

- **LoRa hardware** (₹3,000–₹15,000 per edge node) for geographic coverage in remote areas
- **Satellite terminal** (₹50,000–₹5,00,000 for gateway nodes at critical facilities)
- **Developer time** (primary ongoing cost)

IRIS explicitly rejects any architecture that requires:
- Centralized servers for message relay
- Paid cloud services for core functionality
- Per-message or per-user fees
- Proprietary hardware for basic operation

The software is open source. The protocol is open. The reference implementation is free. Value-added services (management dashboard, SLA guarantees for enterprise deployments) may be commercial, but the protocol and core software must remain free and open.

---

## 7. Guiding Principles

### 7.1 Core Engineering Principles

1. **Offline-first by design.** Every feature must work without internet. Internet connectivity is an enhancement, never a requirement.

2. **Emergency priority is inviolable.** P0 SOS messages cannot be dropped, rate-limited, or delayed by any code path except TTL expiration. Any change to relay logic must prove it does not harm P0 delivery.

3. **No single point of failure.** Any single device, link, or gateway failure must not partition the network in ways that could have been avoided.

4. **Minimal attack surface.** Every feature added increases attack surface. Features must justify their security cost.

5. **Cryptographic identity, not account-based identity.** Users are their keys. No account creation, no password reset via email, no centralized identity authority.

6. **Honest about limitations.** The system must communicate clearly when delivery is uncertain. False assurance that a message was delivered when it was not is a safety hazard.

7. **Open protocol.** The wire format, routing algorithms, and security primitives are fully documented and independently implementable.

### 7.2 Ethical Principles

1. **Privacy as a safety feature.** In contexts of domestic violence, political persecution, or criminal targeting, privacy is not a preference — it is survival. Location data is handled with extreme care.

2. **Not a surveillance tool.** IRIS must not expose network topology, user location, or communication patterns to any party not authorized by the communicating users. This includes the IRIS developers.

3. **Lawful interception compliance, not facilitation.** IRIS complies with lawfully issued interception orders through the same mechanisms as any communication platform. It does not build surveillance backdoors or facilitate mass surveillance.

4. **Accessibility.** Emergency communication must be accessible to users with disabilities, low digital literacy, and devices with limited capability.

5. **No dark patterns.** No dark UX, no hidden data collection, no consent by default.

---

## 8. Governance Model

### 8.1 Decision Authority

| Decision Type | Authority | Process |
|---------------|-----------|---------|
| Protocol changes (backward-compatible) | Core team consensus | PR review, 2 approvers |
| Protocol changes (breaking) | Core team + community RFC | 30-day comment period |
| Security architecture changes | Security lead + external audit | Mandatory review gate |
| Cryptographic primitive changes | Security lead + 2 external experts | Formal analysis required |
| New transport support | Core team consensus | Reference implementation required |
| Legal/regulatory decisions | Legal counsel + leadership | Case-by-case |
| Emergency feature freeze | Any core team member | Override possible; requires post-hoc review |

### 8.2 Approval Gates for High-Risk Decisions

#### Gate 1: Cryptography Changes
Any change to cryptographic algorithms, key sizes, or security protocol versions requires:
- [ ] Written rationale documenting threat model impact
- [ ] Review by at least one cryptographer external to the team
- [ ] Test vectors demonstrating correct implementation
- [ ] Migration path for existing keys and messages
- [ ] 60-day community review period before merge

#### Gate 2: Privacy Architecture Changes
Any change that affects what data nodes collect, store, or transmit requires:
- [ ] Privacy impact assessment
- [ ] Legal review in at least India and EU jurisdiction
- [ ] Explicit consent mechanism design review
- [ ] Documentation of what data cannot be avoided (technical necessity) vs. what is optional

#### Gate 3: Emergency Authority Changes
Any change to the emergency broadcast mechanism, SOS escalation, or emergency override behavior requires:
- [ ] Formal threat model update (adversarial abuse scenarios)
- [ ] Government/NGO partner review
- [ ] Field test with emergency responders
- [ ] Abuse mitigation analysis (can this be weaponized?)

#### Gate 4: Legal/Regulatory Scope
Before deployment in any new jurisdiction:
- [ ] Legal review of radio frequency licensing requirements
- [ ] Assessment of end-to-end encryption legality
- [ ] Emergency services interoperability review
- [ ] Data localization compliance check

---

## 9. Non-Goals (Explicit)

These are design decisions, not oversights. IRIS will not be:

**9.1 An anonymity-at-all-costs platform.**  
IRIS provides privacy, not anonymity. Users can be pseudonymous, but the system is not designed to make communication completely unattributable. Anonymous messaging enables abuse in emergency contexts (false SOS, panic induction). The design allows lawful attribution in appropriate circumstances.

**9.2 A tool to evade lawful authority.**  
IRIS is designed to help people communicate during emergencies, not to evade police, hide criminal activity, or circumvent lawful government processes. The system complies with applicable law in its operating jurisdictions. This is a non-negotiable design constraint.

**9.3 A replacement for emergency services.**  
IRIS routes SOS to other users. It does not interface with PSAP (Public Safety Answering Points), does not guarantee response, and must not be presented to users as a substitute for calling 112/911 when cellular service is available.

**9.4 A censorship circumvention tool.**  
While IRIS's distributed architecture incidentally resists some forms of communication disruption, it is not designed or marketed as a tool for circumventing lawful network restrictions. Political speech implications are acknowledged and the project takes no position.

**9.5 A mass surveillance infrastructure.**  
Governments, ISPs, and commercial entities must not be able to use IRIS as a monitoring platform. The protocol is designed to prevent network-wide metadata harvesting even by parties who operate multiple nodes.

---

## 10. Revision History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-08-10 | IRIS Core Team | Initial charter |

