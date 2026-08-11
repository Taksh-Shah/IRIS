# IRIS Threat Model

## Methodology

This threat model uses two complementary approaches:
1. **STRIDE** (Spoofing, Tampering, Repudiation, Information Disclosure, Denial of Service, Elevation of Privilege) categorizes threats by type.
2. **Attack Trees** decompose specific high-value attacks into achievable sub-goals.

The model is reviewed on every significant protocol or architecture change.

---

## System Assets

| Asset | Value | Security Goal |
|---|---|---|
| Message plaintext | High | Confidentiality |
| Sender/recipient identity binding | High | Privacy |
| Emergency broadcast authenticity | Critical | Integrity |
| Identity private keys | Critical | Integrity, Non-repudiation |
| Routing state / contact history | Medium | Privacy |
| User location | High | Privacy |
| SOS messages | Critical | Availability, Integrity |
| Community channel content | Medium | Integrity |

---

## Threat Actors

### TA-1: Passive Eavesdropper

**Description:** An attacker who monitors radio signals (BLE, Wi-Fi, LoRa) in the vicinity without transmitting.

**Goals:**
- Read message content
- Identify communication relationships (who talks to whom)
- Track physical location of devices
- Determine timing patterns (when a person is active)

**Capabilities:**
- Software-defined radio equipment (BLE/Wi-Fi: laptop + USB dongle; LoRa: RTLSDR)
- Packet capture and analysis tools
- Long-duration monitoring (hours to days)
- Cannot modify or inject packets

**Access Level:** Physical proximity to target (BLE: ~100m, Wi-Fi: ~200m, LoRa: ~5km)

**Attack Vectors:**
- Capture BLE advertisements to extract device identifier
- Capture Wi-Fi probe requests to track device
- Capture LoRa packets to analyze communication frequency
- Correlate signal strength triangulation for location

**STRIDE mapping:** Information Disclosure

**Mitigations:**
- BLE MAC address randomization (15–60 min rotation)
- Link-layer encryption on all transports (L1)
- E2EE on message content (L2)
- Minimal metadata in advertisements
- No location in routing headers

**Residual Risk:** Traffic analysis — even with encryption, the *pattern* of communication (when, how much, between which anonymous identifiers) is visible to a sufficiently resourced passive adversary. This is acknowledged; full traffic analysis resistance would require a mix network or onion routing, which is incompatible with DTN/store-carry-forward requirements.

---

### TA-2: Active Attacker on Network

**Description:** An attacker who can transmit, modify, inject, and replay packets on the radio network.

**Goals:**
- Inject messages appearing to come from legitimate users
- Modify messages in transit
- Replay old messages
- Disrupt communication by jamming or flooding

**Capabilities:**
- Transmit on BLE, Wi-Fi, and potentially LoRa frequencies
- Modify captured packets before retransmission
- Replay previously captured packets
- May have multiple devices

**Access Level:** Physical proximity to target or relay network

**Attack Vectors:**
- Inject fake message with spoofed sender identity
- Capture and replay a valid message to a different recipient
- Modify packet fields (e.g., hop count, TTL, destination)
- Flood channel with junk packets (DoS)
- Downgrade attack: present weaker crypto suite

**STRIDE mapping:** Spoofing, Tampering, Denial of Service

**Mitigations:**
- Ed25519 signature on all messages (injection/tampering detected)
- Message ID uniqueness + seen-message cache (replay blocked)
- Timestamp + replay window (old replays rejected)
- Crypto suite negotiation: minimum suite enforced, downgrade rejected
- Rate limiting per transmitter identity

**Residual Risk:** Radio jamming (denial of service at physical layer) cannot be prevented cryptographically. LoRa frequency hopping provides partial mitigation.

---

### TA-3: Malicious Relay Node

**Description:** An attacker who operates a legitimate-looking relay node (e.g., installs IRIS on a device but modified to behave maliciously).

**Goals:**
- Drop messages selectively (blackhole attack)
- Collect metadata on communications passing through the node
- Deanonymize communicating parties
- Selectively delay high-priority messages

**Capabilities:**
- Full access to relay protocol logic
- Can see plaintext routing headers (sender ID, destination ID, message ID)
- Cannot decrypt message content (E2EE)
- Can choose to drop or delay messages

**Access Level:** Participating relay node; may be mobile and positioned near targets

**Attack Vectors:**
- Blackhole: advertise reachability to many nodes, then drop all messages
- Selective drop: drop messages from/to specific identities
- Metadata collection: log all sender/destination pairs seen
- Delay emergency messages to degrade response time

**STRIDE mapping:** Information Disclosure, Denial of Service, Tampering (via selective forwarding)

**Mitigations:**
- Delivery receipts: recipients send signed ACK; senders detect non-delivery
- Multi-path routing: messages sent via multiple relay paths when possible
- Relay reputation: nodes that consistently fail to deliver are deprioritized
- E2EE: relay node cannot read content
- Routing diversity: rely on multiple independent relay nodes

**Residual Risk:** A single-path delivery (no alternative route exists) is vulnerable to a blackhole on that path. In a sparse network, this can be a real constraint. Detection is possible (no ACK received) but recovery requires an alternative path to exist.

---

### TA-4: Sybil Attacker

**Description:** An attacker who creates many fake identities (Sybil nodes) to gain disproportionate influence over the network.

**Goals:**
- Control routing decisions (send traffic through attacker-controlled nodes)
- Eclipse legitimate nodes (surround a target node with attacker nodes)
- Distort reputation/trust scores
- Amplify DoS effectiveness

**Capabilities:**
- Can create an arbitrary number of IRIS identities (key generation is free)
- Can run many nodes simultaneously
- Each node appears legitimate

**Access Level:** Network-wide (virtual presence via many devices)

**Attack Vectors:**
- Create hundreds of Sybil nodes that advertise excellent routing to all destinations
- Use Sybil nodes to redirect all traffic through attacker infrastructure
- Use Sybil nodes to partition a target from the real network

**STRIDE mapping:** Elevation of Privilege, Denial of Service

**Mitigations:**
- Rate limiting per physical device (limits practical number of Sybil nodes)
- Social vouching: contacts from verified users (limits Sybil reputation)
- Routing does not rely purely on centrality (a newcomer with no history cannot immediately appear as the best relay)
- Multi-path routing: not relying on single paths

**Residual Risk:** Sybil resistance in a permissionless system is fundamentally hard. IRIS does not claim strong Sybil resistance. The mitigations slow and limit Sybil attacks but cannot eliminate them. See `SYBIL_RESISTANCE.md` for detailed analysis.

---

### TA-5: Spam Attacker

**Description:** An attacker who sends large volumes of unsolicited or junk messages to degrade system performance or harass users.

**Goals:**
- Exhaust storage on relay nodes
- Degrade routing performance (high message volume)
- Harass specific users
- Drown emergency channels in noise

**Capabilities:**
- Can send messages at high rate (limited only by transport bandwidth)
- Can target specific users or channels
- Can use multiple identities

**Attack Vectors:**
- Flood a target's message queue with junk
- Flood relay node storage to cause legitimate message eviction
- Spam community channels with irrelevant content
- Spam emergency channels with fake alerts

**STRIDE mapping:** Denial of Service

**Mitigations:**
- Rate limiting per sender identity (per-hop enforcement)
- Storage quotas per sender identity
- Priority inversion: spam at P7 evicted first under storage pressure
- Social trust filter: optional "known contacts only" mode
- Message size limits (maximum 64KB per message)
- Emergency channel access controls (only verified authorities)

---

### TA-6: Emergency Abuser

**Description:** An attacker (or careless user) who misuses the emergency system — sending fake SOS, spoofing emergency broadcasts, or sending false location information.

**Goals:**
- Cause panic (fake emergency broadcast)
- Waste emergency responder resources (fake SOS)
- Discredit the system (false alerts cause people to ignore real alerts)

**Capabilities:**
- Has a valid IRIS identity (user or verified responder)
- For broadcast spoofing: may have obtained an emergency authority certificate

**Attack Vectors:**
- User sends repeated fake SOS
- Verified responder sends false emergency broadcast (insider abuse)
- Attacker compromises a responder node and sends broadcast
- Fake location in SOS (mislead rescuers)

**STRIDE mapping:** Spoofing, Tampering

**Mitigations:**
- SOS rate limit (3 per device per hour)
- Cancel mechanism for false SOS
- Emergency authority certificates have revocation
- All emergency broadcasts logged with identity (accountability)
- Audit trail enables post-incident investigation
- Verified responder program (vetting process)

See `EMERGENCY_ABUSE.md` for detailed design.

---

### TA-7: Government Surveillance Adversary

**Description:** A state actor seeking to monitor communications, identify dissidents, or disrupt political communication.

**Goals:**
- Identify who is communicating with whom
- Deanonymize IRIS users
- Read message content
- Disrupt communication during civil unrest
- Compel platform to provide user data

**Capabilities:**
- IMSI catchers (cell-network tracking)
- Mass radio monitoring
- Legal compulsion of platform operators
- Network-level blocking
- Physical access to arrested users' devices

**Access Level:** Network-wide; can compel ISPs and may operate physical infrastructure

**Important Context (India-Specific):**
IRIS is explicitly designed for beneficial communication — disaster response, women's safety, emergency coordination — NOT for evading lawful authority. The India context includes legitimate government emergency coordination (NDMA, state disaster management) that IRIS is designed to support.

The adversary model for government surveillance is specifically about *unlawful mass surveillance* and *overreach beyond lawful process*, not about lawful law enforcement under court order.

**Attack Vectors:**
- Compel platform to provide metadata
- Deploy IMSI catchers to correlate IRIS traffic with phone identities
- Block IRIS network at ISP level during internet-dependent relay
- Physical seizure of devices

**STRIDE mapping:** Information Disclosure, Denial of Service

**Mitigations:**
- E2EE: we cannot provide content we cannot read
- Minimal data retention: we hold the minimum necessary
- Transparent legal process requirements (see `LAW_ENFORCEMENT_REQUESTS.md`)
- BLE MAC randomization: limits physical tracking
- Offline operation: not dependent on internet infrastructure

**Out of Scope:** Nation-state SIGINT (classified intercept capabilities, compromised crypto implementations at chip level) — see Out-of-Scope Threats below.

---

### TA-8: Physical Device Attacker

**Description:** An attacker with physical access to an IRIS device — either a stolen device or a device seized by authorities.

**Goals:**
- Read stored messages
- Extract private keys
- Access contact list
- Impersonate the device's identity

**Capabilities:**
- Physical access to device
- May be able to perform forensic extraction
- May have user's PIN/biometric under coercion

**Attack Vectors:**
- Cold-boot attack on device RAM
- Forensic extraction of app data (on unencrypted or weakly encrypted device)
- Bruteforce device PIN
- Extract key from hardware security element (requires sophisticated equipment)

**STRIDE mapping:** Information Disclosure, Elevation of Privilege

**Mitigations:**
- Database encryption with key in hardware security element
- Keys bound to biometric/PIN authentication
- Message TTL and deletion (reduces data at risk)
- Panic mode: optional quick-wipe on multiple failed PIN attempts
- Key storage in Secure Enclave / StrongBox: resistant to most forensic extraction

**Residual Risk:** A device that is unlocked (user authenticated) has keys accessible to the OS. Sophisticated forensic tools used by law enforcement can in some cases extract data from unlocked devices. Coerced biometric authentication cannot be prevented at the software level.

---

### TA-9: Insider Threat

**Description:** A person with legitimate access to IRIS infrastructure — an employee, contractor, or authorized emergency responder — who misuses their access.

**Goals:**
- Exfiltrate user data
- Modify server-side components (if any)
- Abuse emergency authority
- Leak user information

**Capabilities:**
- Access to production infrastructure
- Knowledge of system internals

**Attack Vectors:**
- Emergency responder sends unauthorized broadcast
- Infrastructure operator accesses logs to identify users
- Developer introduces backdoor in code
- Operator compelled by government to abuse access

**STRIDE mapping:** Information Disclosure, Elevation of Privilege, Spoofing

**Mitigations:**
- Minimal infrastructure: IRIS is designed to be decentralized; no central server holds user data
- Emergency authority certificates have audit trail
- Open-source code: community can audit for backdoors
- Code signing: official builds are signed and verifiable
- Emergency authority revocation for misuse

---

## STRIDE Threat Matrix

| Component | Spoofing | Tampering | Repudiation | Info Disclosure | DoS | Elevation |
|---|---|---|---|---|---|---|
| BLE Discovery | Medium | Low | N/A | High | Medium | Low |
| Message Transport | Medium | Low | Low | Medium | High | Low |
| Message Content | Low | Low | Low | Low | N/A | N/A |
| Routing Tables | Low | Medium | N/A | Medium | Medium | Low |
| Emergency Broadcast | High | Medium | Low | Low | Medium | High |
| Storage | Low | Low | N/A | Medium | Low | Low |
| Identity System | Medium | Low | Low | Low | Low | Medium |
| SOS System | High | Low | Low | Low | Medium | Low |

Risk levels: High / Medium / Low — before mitigations applied.

---

## Attack Trees

### AT-1: Intercepting Message Content

**Goal:** Attacker reads plaintext content of a message between Alice and Bob.

```
[Read message content]
├── [Break E2EE encryption]
│   ├── [Obtain Alice's private key]
│   │   ├── [Physical device access + extract from hardware] (VERY HARD)
│   │   ├── [Compromise OS keystore] (OS-level attack, out of scope)
│   │   └── [Social engineering / coercion] (out of scope: physical coercion)
│   ├── [Obtain session key]
│   │   ├── [Break key derivation] (cryptographically infeasible)
│   │   └── [Memory forensics on live device] (requires OS compromise)
│   └── [Break AES-256-GCM] (computationally infeasible)
├── [Compromise relay node and intercept before encryption]
│   └── [Not possible: encryption applied before transmission]
└── [Compromise Alice's or Bob's device app process]
    ├── [Exploit app vulnerability to RCE] (mitigated by Rust safety)
    └── [Compromise OS (root/jailbreak)] (platform-level, partially out of scope)
```

**Minimum required attacker effort:** Physical device access (TA-8) OR OS-level exploit. Neither is achievable by a network-only attacker.

---

### AT-2: Injecting Fake Emergency Broadcast

**Goal:** Attacker sends an emergency broadcast that relays accept and forward as legitimate.

```
[Relay accepts fake emergency broadcast]
├── [Obtain valid Emergency Authority Certificate]
│   ├── [Steal EAC private key from legitimate authority]
│   │   ├── [Compromise authority device] (TA-8 against authority)
│   │   └── [Social engineering authority official] (out of scope)
│   ├── [Compromise CA and self-issue certificate]
│   │   └── [Break CA key security] (mitigated by air-gapped CA)
│   └── [Forge certificate without CA]
│       └── [Break Ed25519] (computationally infeasible)
└── [Exploit relay verification logic to skip verification]
    ├── [Find bug in signature verification code] (mitigated by Rust safety, testing)
    └── [Exploit parser vulnerability in certificate parsing] (mitigated by fuzzing)
```

**Minimum required attacker effort:** Compromise of a legitimate emergency authority device or certificate storage.

---

### AT-3: Disrupting Routing (Blackhole)

**Goal:** Attacker prevents messages from being delivered across the network.

```
[Disrupt message delivery]
├── [Physical jamming] (radio interference — not cryptographically preventable)
├── [Blackhole attack via malicious relay]
│   ├── [Deploy single blackhole relay in critical position]
│   │   └── [Detected via: no ACK received, multi-path routing]
│   └── [Deploy many Sybil relays to overwhelm routing]
│       └── [Mitigated by: rate limiting, social trust, multi-path]
├── [Eclipse attack on target node]
│   └── [Surround target with attacker nodes]
│       └── [Mitigated by: routing diversity, direct BLE peer discovery]
└── [Storage exhaustion on relay nodes]
    └── [Flood with junk messages to evict legitimate ones]
        └── [Mitigated by: per-sender quotas, priority-based eviction]
```

---

### AT-4: DoS via Message Flooding

**Goal:** Attacker renders the network or a specific node unusable by flooding with messages.

```
[Render target node unusable]
├── [CPU exhaustion via complex message processing]
│   ├── [Send messages with expensive-to-verify crypto] (mitigated: constant-time verify)
│   └── [Send malformed messages triggering complex parse paths] (mitigated: fuzzing, size limits)
├── [Storage exhaustion]
│   ├── [Flood with many small messages] (mitigated: per-sender quota)
│   └── [Flood with maximum-size messages] (mitigated: size limit 64KB + quota)
├── [Battery exhaustion via radio flooding]
│   └── [Continuous BLE/Wi-Fi scan triggering] (mitigated: backpressure, rate limit)
└── [Memory exhaustion via large message queues]
    └── [Mitigated: bounded queue, backpressure propagation]
```

---

## India-Specific Threat Context

### Communal Violence Scenarios
During communal tensions, coordinated misinformation via mesh networks could be weaponized. Mitigations: source authentication (you can always verify WHO sent a message), public channels require admin authorization, emergency broadcast limited to verified authorities.

### Natural Disaster Environment
India experiences frequent cyclones (Bay of Bengal, Arabian Sea), earthquakes (Himalayan belt, Gujarat), floods (North India), and droughts. In these scenarios:
- Communication infrastructure fails (most significant threat to IRIS operation)
- Power infrastructure fails (battery life becomes critical)
- Emergency responders are primary users under high stress

### Low-Literacy Context
Threat: users may not understand security indicators (verified vs. unverified source). Mitigation: UI uses clear visual cues, not complex technical language.

### Feature Phone / Low-End Device Context
Some users in Tier-3 cities and rural areas use low-spec Android devices. These may not have StrongBox hardware security. Mitigation: graceful degradation to software keystore with additional software protections.

---

## Out-of-Scope Threats

The following threats are explicitly outside the IRIS threat model. Addressing them would require infrastructure that is incompatible with IRIS's design goals (offline operation, minimal infrastructure, mobile devices).

### Nation-State SIGINT
- Classified signal intelligence capabilities
- Compromised hardware (supply chain attacks on chips)
- Cryptographic backdoors in hardware implementations
- Side-channel attacks requiring lab-grade equipment

### Physical Coercion
- $5 wrench attack (coercing user to reveal key or unlock device)
- Torture or legal coercion of key holders
- This is a social/legal/political problem, not a cryptographic one

### Zero-Day OS Exploits
- Exploits that achieve kernel-level code execution on target device
- Bypassing hardware security elements via OS vulnerabilities
- These are mitigated by OS vendor patching, not by IRIS

### Theoretical Cryptographic Breaks
- Polynomial-time algorithm for discrete logarithm
- Quantum computers breaking Curve25519 (timeline uncertain; migration planned)

---

## Threat Model Review Schedule

| Event | Review Required |
|---|---|
| Protocol version change | Full review |
| New transport added | Transport-specific review |
| Emergency system change | Emergency section review |
| Security incident | Immediate targeted review |
| Annual | Full review regardless of changes |

Last reviewed: [Date of last review]  
Next scheduled review: [Annual from last review]  
Owner: Security Engineering Lead
