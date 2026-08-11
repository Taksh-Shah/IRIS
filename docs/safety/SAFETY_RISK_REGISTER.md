# IRIS Safety Risk Register

**Document ID:** IRIS-SAFETY-003  
**Version:** 1.0  
**Status:** Active — Living Document  
**Review Cycle:** Quarterly or after any significant incident  

---

## 1. Risk Assessment Methodology

### 1.1 Scoring Framework

| Dimension | Score | Meaning |
|-----------|-------|---------|
| **Likelihood** | 1 | Rare: < once per 5 years |
| | 2 | Unlikely: once per 2–5 years |
| | 3 | Possible: once per year |
| | 4 | Likely: multiple times per year |
| | 5 | Almost certain: frequent occurrence |
| **Impact** | 1 | Negligible: inconvenience, no safety consequence |
| | 2 | Minor: degraded service, manageable |
| | 3 | Moderate: significant disruption, some safety consequence |
| | 4 | Major: serious harm to persons, severe disruption |
| | 5 | Catastrophic: deaths or serious irreversible harm |
| **Risk Score** | | Likelihood × Impact |
| **Risk Level** | 1–5 | Low |
| | 6–10 | Medium |
| | 11–16 | High |
| | 17–25 | Critical |

### 1.2 Residual Risk

Residual risk is the risk level remaining after mitigations are applied. A residual risk > 10 requires escalation to IRIS safety board and explicit acceptance. A residual risk > 15 blocks release.

---

## 2. Risk Register

### SR-001: False SOS Floods Overwhelm Emergency Responders

**ID:** SR-001  
**Category:** Misuse / Denial of Service  
**Description:** An attacker or prankster sends large volumes of false P0 SOS messages, causing emergency responders to dispatch resources to non-existent emergencies. In a genuine disaster, this directly kills people by diverting resources from real victims.

| Attribute | Value |
|-----------|-------|
| Likelihood | 4 (likely — SOS capability is intentionally unrestricted) |
| Impact | 5 (catastrophic — misdirected response may result in deaths) |
| **Risk Score** | **20 — Critical** |

**Mitigations:**
1. **Rate limiting at transport layer:** Each NodeId is rate-limited to 1 SOS per 15 minutes per transport. Sending a second SOS within 15 minutes requires user confirmation ("You already sent an SOS 8 minutes ago. Send again?").
2. **SOS deduplication:** Identical SOS bundles (same source, same location, same text within 30 minutes) are deduplicated and count as one event to responders.
3. **Trust scoring for repeated false SOS:** Nodes whose previous SOS events were confirmed false (by responder feedback) receive a visual warning flag on subsequent SOS from the same NodeId. This does not suppress delivery — responders still receive it but with context.
4. **UI friction for SOS:** The SOS UI requires a 5-second hold on the SOS button (prevents accidental sends). A confirmation dialog follows. The SOS message requires at minimum a location or a text description.
5. **Responder SOS management tool:** IRIS Command (authority-facing interface) provides a triage dashboard where responders can mark SOS events as confirmed/false, and flag repeat offenders.
6. **Legal notice in app:** The IRIS app clearly states that false SOS is a criminal offense under IPC Section 188 and equivalent state laws.

**Residual Risk:** Likelihood 3, Impact 5 = **15 — High**  
**Accepted:** Yes, with annual review. P0 delivery cannot be gated on identity verification without unacceptable risk of blocking genuine SOS.

---

### SR-002: Attacker Impersonates Emergency Authority (NDRF/Red Cross)

**ID:** SR-002  
**Category:** Identity Fraud / Disinformation  
**Description:** An attacker impersonates NDRF, Red Cross, or other emergency authority, sending false evacuation orders, fake medical instructions, or political messaging under the authority's name.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — forging signature requires key theft; social engineering possible) |
| Impact | 5 (catastrophic — false evacuation could cause stampede or delay real response) |
| **Risk Score** | **15 — High** |

**Mitigations:**
1. **Cryptographic authority verification:** All authority messages require a valid Ed25519 certificate chain back to a pre-installed root CA. Forging an authority identity requires stealing the private key of a certified authority device. (Covered in `docs/safety/AUTHORITY_VERIFICATION.md`)
2. **Prominent UI warning for unverified authority claims:** Messages claiming authority but without valid cert chain are shown with a prominent red warning, never as legitimate authority messages.
3. **Short-lived credentials:** Authority certificates expire within 90 days; capability tokens within 30 days. A stolen device's credentials expire naturally, limiting the attack window.
4. **Emergency revocation:** If an authority device is stolen, the issuing CA can revoke the certificate within minutes (propagation to all nodes may take hours).
5. **Root CA key custody:** Root CA private keys are on air-gapped HSMs with 2-of-3 custodian quorum. Cannot be stolen via software attack.

**Residual Risk:** Likelihood 2, Impact 5 = **10 — Medium**  
**Accepted:** Yes. Residual risk primarily from physical theft of authority device with valid credentials.

---

### SR-003: Emergency Broadcast Used for Political Disinformation

**ID:** SR-003  
**Category:** Misuse / Information Integrity  
**Description:** A compromised or politically motivated authority uses their legitimate IRIS broadcast capability to distribute political propaganda, religious incitement, or false information under the cover of emergency communications.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — authority participants may include politically motivated actors) |
| Impact | 4 (major — mass disinformation during disaster could cause secondary violence or panic) |
| **Risk Score** | **12 — High** |

**Mitigations:**
1. **Scope constraints in capability tokens:** Authority certificates are issued with geographic and functional scope constraints. A Gujarat SDRF certificate cannot send a national broadcast; a medical coordination certificate cannot send evacuation orders.
2. **Content logging (with consent):** In non-partitioned mode, IRIS nodes optionally log received authority broadcasts for post-incident audit. This creates accountability.
3. **Community flagging:** Receivers can flag authority messages as disinformation. Multiple flags from independent nodes trigger alert to IRIS governance team (in connected mode).
4. **IRIS governance response:** IRIS maintains a 24/7 incident response team during declared disasters. Reported abuse results in emergency revocation of the offending authority certificate.
5. **Content format restrictions:** Authority broadcasts are text-only or structured data. No executable content, no embedded links, no rich media that could be manipulated.

**Residual Risk:** Likelihood 2, Impact 4 = **8 — Medium**  
**Accepted:** Yes. Cannot eliminate risk from legitimate-but-bad-faith actors with valid credentials; process controls (governance response) are the primary residual mitigation.

---

### SR-004: Location Data Enables Tracking of Vulnerable Persons

**ID:** SR-004  
**Category:** Privacy / Personal Safety  
**Description:** P2 location messages are designed to help rescuers find victims. However, the same data could be used to track dissidents, domestic abuse victims, marginalized communities, or other persons who are vulnerable to harm if their location is known.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — threat model includes state actors and abusers) |
| Impact | 4 (major — targeted harm to individuals based on location tracking) |
| **Risk Score** | **12 — High** |

**Mitigations:**
1. **Location sharing is always opt-in:** IRIS never sends P2 location automatically. Users explicitly initiate location sharing. The UI is clear about who can receive the location.
2. **Targeted location sharing:** Users can choose to share location with a specific NodeId (direct delivery, not broadcast) rather than as a public P2 broadcast. Direct delivery uses end-to-end encryption.
3. **Short TTL for location bundles:** P2 location bundles default TTL is 30 minutes (not the standard 6 hours). Old locations expire quickly from the network.
4. **No location in personal messages by default:** The IRIS UI does not auto-attach location to messages. Users must explicitly add location.
5. **Relay node anonymization:** Location bundles in transit are encrypted; relay nodes see the destination NodeId and bundle size but not location content.
6. **DPDPA 2023 compliance:** Location is classified as sensitive personal data. Processing requires explicit consent. IRIS logs consent at time of location share (local log only; not centrally stored).
7. **No historical location storage:** IRIS does not maintain a location history database. Locations are routed and delivered; not persisted beyond delivery (except in the sender's own device).

**Residual Risk:** Likelihood 2, Impact 4 = **8 — Medium**  
**Accepted:** Yes. Vulnerability cannot be fully eliminated when a user voluntarily shares location; informed consent and minimal retention reduce risk.

---

### SR-005: Emergency App Battery Drain Prevents Actual Emergency Use

**ID:** SR-005  
**Category:** Availability / Reliability  
**Description:** IRIS consumes battery through constant BLE scanning, Wi-Fi Direct probing, and LoRa transmission. In a disaster scenario, a user's phone battery is critically important. If IRIS drains the battery, the user cannot send an SOS when genuinely needed.

| Attribute | Value |
|-----------|-------|
| Likelihood | 4 (likely — mesh networking is power-intensive; disasters often include power outages) |
| Impact | 4 (major — inability to send SOS when needed) |
| **Risk Score** | **16 — High** |

**Mitigations:**
1. **Adaptive power management:** IRIS reduces scanning frequency as battery drops. At < 30%: scan interval increases from 5s to 30s. At < 15%: scan interval 90s, non-essential transports disabled. At < 5%: only P0 transmission capability maintained.
2. **Emergency reserve mode:** User can explicitly activate "SOS Reserve" mode, which suspends all non-critical IRIS functions and reserves battery only for P0 capability.
3. **Battery usage transparency:** IRIS displays its battery consumption in real-time in the settings screen. Users can make informed decisions.
4. **Baseline measurement:** IRIS targets < 5% battery drain per hour in idle mesh mode (no active contacts) on a mid-range Android phone (Snapdragon 695, 5000 mAh battery). Tested on target hardware.
5. **BLE-only low-power mode:** When battery < 20%, IRIS switches to BLE-only mode (most power-efficient transport), disabling Wi-Fi Direct and LoRa scanning.
6. **Background kill protection:** IRIS registers as a foreground service on Android to prevent OS background-kill. On iOS, uses background modes appropriately.
7. **Pre-disaster guidance:** IRIS shows a notification when battery < 50% in a declared disaster area, advising to charge if possible.

**Residual Risk:** Likelihood 2, Impact 4 = **8 — Medium**  
**Accepted:** Yes. Power management improvements are ongoing. Cannot fully mitigate hardware constraints.

---

### SR-006: Network Congestion During Mass Casualty Event

**ID:** SR-006  
**Category:** Availability / Reliability  
**Description:** During a mass casualty event, thousands of people simultaneously send P0 SOS messages. The IRIS network becomes congested, and individual SOS messages are delayed or lost due to queuing and collision.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — mass casualty events do occur) |
| Impact | 5 (catastrophic — SOS delay in mass casualty = deaths) |
| **Risk Score** | **15 — High** |

**Mitigations:**
1. **BLE advertisement backoff:** In crowd mode (> 50 devices detected in range), IRIS applies exponential random backoff to BLE advertisements, reducing collision probability.
2. **LoRa channel allocation:** LoRa uses different spreading factors for high-density scenarios, effectively creating parallel sub-channels.
3. **SOS deduplication at relay:** Relay nodes deduplicate SOS bundles before forwarding, reducing redundant transmissions.
4. **Priority queue enforcement:** P0 always transmits ahead of all other priorities; congestion from P3–P7 traffic does not affect P0.
5. **Satellite uplift for aggregated SOS:** In an IRIS deployment with a satellite-connected gateway node (Iridium/Starlink), the gateway aggregates P0 SOS locations and uploads to the IRIS cloud dashboard for emergency services — bypassing the mesh for first notification.
6. **Expected congestion analysis:** See `docs/emergency/CROWD_MANAGEMENT.md` for the full analysis of 1000-node density scenarios.

**Residual Risk:** Likelihood 2, Impact 5 = **10 — Medium**  
**Accepted:** Yes. Some delay in mass casualty SOS is unavoidable; mitigations minimize but cannot eliminate it.

---

### SR-007: Routing Attack Suppresses P0 SOS

**ID:** SR-007  
**Category:** Security / Availability  
**Description:** A malicious node (blackhole attacker) intercepts P0 SOS bundles and drops them, preventing emergency services from receiving the distress signal.

| Attribute | Value |
|-----------|-------|
| Likelihood | 2 (unlikely — requires deliberate adversary with IRIS node) |
| Impact | 5 (catastrophic — suppressed SOS = potential death) |
| **Risk Score** | **10 — Medium** |

**Mitigations:**
1. **Epidemic routing for P0:** SOS bundles use Spray-and-Wait (L=5) at minimum, Epidemic when no receipt is received in 30 seconds. Multiple copies in multiple hands reduces the impact of any single blackhole.
2. **Delivery receipt tracking:** Non-delivery triggers anomaly scoring and re-routing. (Covered in `docs/routing/ROUTING_SECURITY.md`)
3. **Multi-transport simultaneous broadcast for P0:** SOS is transmitted on all available transports simultaneously — a blackhole on BLE does not suppress a simultaneous Wi-Fi Direct transmission.
4. **L=5 minimum spray:** At least 5 copies of each SOS bundle are in circulation before Spray-and-Wait stops, making blackhole suppression require 5 colluding nodes.

**Residual Risk:** Likelihood 1, Impact 5 = **5 — Low**  
**Accepted:** Yes. Multi-copy epidemic routing makes SOS suppression extremely difficult.

---

### SR-008: Private Messages Intercepted by Relay Nodes

**ID:** SR-008  
**Category:** Privacy / Confidentiality  
**Description:** IRIS messages travel through relay nodes. A malicious relay node reads the content of private messages between users.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — relay nodes are operated by unknown parties) |
| Impact | 3 (moderate — privacy violation; could be serious for sensitive communications) |
| **Risk Score** | **9 — Medium** |

**Mitigations:**
1. **End-to-end encryption:** All IRIS bundles are encrypted with ChaCha20-Poly1305 using keys derived from X25519 key agreement between sender and recipient. Relay nodes see only ciphertext.
2. **Metadata minimization:** Relay nodes see source NodeId, destination NodeId, bundle priority, and size. They do not see content, sender name, or message type beyond priority class.
3. **Forward secrecy:** X25519 ephemeral keys are used per message; compromise of a long-term key does not expose past messages.
4. **NodeId pseudonymity:** NodeIds are BLAKE3 hashes of public keys, not user-identifiable. A relay node seeing traffic between NodeId A and NodeId B cannot directly identify the persons involved without additional correlation.

**Residual Risk:** Likelihood 1, Impact 3 = **3 — Low**  
**Accepted:** Yes. E2E encryption ensures content confidentiality; metadata exposure is a known DTN characteristic.

---

### SR-009: Child Safety — Minors Using IRIS During Disaster

**ID:** SR-009  
**Category:** Safeguarding / Child Protection  
**Description:** During a disaster, minors (children) may use IRIS as their primary communication device. They may be exposed to distressing emergency broadcasts, or may be identified and located by bad actors via location sharing.

| Attribute | Value |
|-----------|-------|
| Likelihood | 4 (likely — disasters affect all demographics; children will use IRIS) |
| Impact | 3 (moderate — psychological harm, privacy risk) |
| **Risk Score** | **12 — High** |

**Mitigations:**
1. **No age verification:** IRIS intentionally does not collect age data (DPDPA compliance, ₹0 infra philosophy). Age-gating would block children from sending SOS.
2. **SOS designed for children:** P0 SOS requires no text — just a button tap. Children can send SOS without knowing how to type a message.
3. **Location sharing opt-in:** Children should not share location publicly. The UI defaults to direct-only location sharing, requiring explicit steps to enable broadcast.
4. **Content in emergency channels:** Emergency channels are authority-moderated; inappropriate content is an authority misuse violation (SR-003 mitigations apply).
5. **Parental guidance:** IRIS app store description includes guidance that the app is designed for disaster use and may contain distressing emergency information.

**Residual Risk:** Likelihood 3, Impact 2 = **6 — Medium**  
**Accepted:** Yes. Fundamental tension between child safety and access to emergency communication is resolved in favor of access.

---

### SR-010: Solar/EMP Event Disables IRIS Hardware

**ID:** SR-010  
**Category:** Environmental / Hardware  
**Description:** A large solar storm or targeted EMP (electromagnetic pulse) event disables the electronic devices running IRIS, rendering the mesh network non-functional at the moment it is most needed.

| Attribute | Value |
|-----------|-------|
| Likelihood | 1 (rare — Carrington-level events or military EMP) |
| Impact | 5 (catastrophic — entire mesh unavailable in disaster) |
| **Risk Score** | **5 — Low** |

**Mitigations:**
1. **Hardware is not IRIS-controlled:** IRIS is a software application on commodity hardware. Hardware hardening against EMP is outside IRIS scope.
2. **Graceful partial failure:** If some nodes survive an EMP event (e.g., devices in Faraday shielding, newer phones with better hardening), the IRIS mesh degrades gracefully — fewer nodes, smaller network, but still functional.
3. **LoRa hardware option:** For critical infrastructure nodes (emergency control centers), IRIS supports dedicated LoRa hardware devices which can be stored in EMP-hardened enclosures.
4. **Satellite fallback is separate infrastructure:** Iridium/Starlink satellite links operate independently; satellite constellation is not affected by ground-level EMP.

**Residual Risk:** Likelihood 1, Impact 5 = **5 — Low**  
**Accepted:** Yes. Risk is outside IRIS software control; hardware-level mitigation is responsibility of deploying organizations.

---

### SR-011: Data Retention Violation (DPDPA 2023)

**ID:** SR-011  
**Category:** Legal / Privacy Compliance  
**Description:** IRIS stores location data, communication metadata, and possibly sensitive personal information in the bundle store. Retention beyond what is necessary, or retention without adequate consent, violates India's Digital Personal Data Protection Act 2023.

| Attribute | Value |
|-----------|-------|
| Likelihood | 3 (possible — edge cases in data lifecycle not fully audited) |
| Impact | 3 (moderate — regulatory penalty, reputational harm, potential enforcement) |
| **Risk Score** | **9 — Medium** |

**Mitigations:**
1. **TTL-based automatic deletion:** All bundles are automatically deleted at TTL expiry. No data is retained beyond TTL. Maximum TTL is 72 hours.
2. **No cloud synchronization without explicit consent:** IRIS is local-first. Data does not leave the device unless satellite/cellular uplift is explicitly enabled.
3. **Privacy audit:** Annual DPDPA compliance audit by third-party privacy professional.
4. **Privacy notice in app:** Plain-language privacy notice in app complying with DPDPA Section 5 (notice) requirements.
5. **Data Principal rights implementation:** Users can delete all local IRIS data via Settings → Delete All Data (wipes bundle store, contact history, routing tables).

**Residual Risk:** Likelihood 2, Impact 3 = **6 — Medium**  
**Accepted:** Yes. Active compliance program in place.

---

## 3. Risk Register Summary

| Risk ID | Description | Score | Residual | Level | Status |
|---------|-------------|-------|----------|-------|--------|
| SR-001 | False SOS floods | 20 | 15 | High | Accepted with review |
| SR-002 | Authority impersonation | 15 | 10 | Medium | Accepted |
| SR-003 | Authority broadcast misuse | 12 | 8 | Medium | Accepted |
| SR-004 | Location tracking of vulnerable persons | 12 | 8 | Medium | Accepted |
| SR-005 | Battery drain prevents emergency use | 16 | 8 | Medium | Active mitigation |
| SR-006 | Network congestion in mass casualty | 15 | 10 | Medium | Accepted |
| SR-007 | Routing attack suppresses SOS | 10 | 5 | Low | Accepted |
| SR-008 | Private message interception | 9 | 3 | Low | Accepted |
| SR-009 | Minor safety during disaster | 12 | 6 | Medium | Accepted |
| SR-010 | Solar/EMP hardware failure | 5 | 5 | Low | Accepted |
| SR-011 | DPDPA retention violation | 9 | 6 | Medium | Accepted |

---

## 4. Review History

| Date | Reviewer | Changes |
|------|----------|---------|
| 2024-01-15 | IRIS Safety Board | Initial register created |
| 2024-04-01 | IRIS Safety Board | SR-011 added post DPDPA 2023 enactment |
| 2024-07-01 | Scheduled review | — |
