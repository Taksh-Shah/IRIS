# IRIS Formal Adversary Model

## Purpose

This document defines the formal adversary model for the IRIS protocol. It specifies:
- What adversaries can observe and do
- What cryptographic guarantees IRIS provides against each adversary class
- What IRIS explicitly does not protect against

This model is the formal basis for security proofs and design decisions. Claims like "IRIS provides forward secrecy" are meaningful only in the context of a specific adversary model.

---

## Adversary Classes

### Class A: Passive Network Adversary (Eavesdropper)

**Formal Definition:**
A passive adversary Adv_P is a probabilistic polynomial-time (PPT) algorithm that:
- Observes all radio transmissions in the network
- Cannot transmit, modify, or delay packets
- Observes the timing, volume, and source/destination of all transmissions
- Has unlimited storage and computational resources for offline analysis
- Cannot compromise the endpoints (devices)

**Notation:** Adv_P(transcripts) → {guess, ⊥}

**What Adv_P CAN do:**
- Capture and analyze all ciphertext
- Perform traffic analysis (who communicates with whom, how often)
- Correlate timing across multiple links
- Run offline dictionary attacks against weak keys
- Build long-term communication pattern databases

**What Adv_P CANNOT do:**
- Transmit packets
- Modify packets
- Access device memory

---

### Class B: Active Network Adversary

**Formal Definition:**
An active adversary Adv_A is a PPT algorithm that:
- Can perform all operations of Adv_P
- Can additionally transmit arbitrary packets on any transport
- Can replay captured packets
- Can delay, reorder, or drop packets (unreliable network model)
- Cannot compromise endpoints

**What Adv_A CAN do:**
- Inject crafted packets claiming any sender identity
- Replay previously captured valid messages
- Drop or delay messages selectively
- Perform man-in-the-middle on unestablished sessions
- Modify packets in transit (where not protected by MAC)

**What Adv_A CANNOT do:**
- Access device key material
- Forge signatures on messages from legitimate nodes
- Break the underlying cryptographic primitives

---

### Class C: Relay Node Adversary (Honest-but-Curious Relay)

**Formal Definition:**
A relay adversary Adv_R is an adversary that:
- Controls one or more IRIS relay nodes
- The controlled nodes follow the IRIS protocol faithfully (they relay messages) but log everything they observe
- Adv_R sees all plaintext routing headers, timing of messages, and identity of other peers that connect

**What Adv_R CAN observe:**
- Sender node ID in message header
- Destination node ID in message header
- Message ID, timestamp, size, priority
- Which devices connected to the relay node and when
- Traffic patterns (frequency, volume, timing)
- Route advertisements from peers

**What Adv_R CANNOT observe:**
- Message content (E2EE)
- The complete sender-recipient relationship (they see segments, not end-to-end path)

---

### Class D: Malicious Relay Node Adversary

**Formal Definition:**
A malicious relay adversary Adv_M extends Adv_R by also:
- Deviating from the protocol in any way (dropping, delaying, modifying within allowed fields)
- Sending crafted routing advertisements
- Colluding with other Adv_M nodes

**What Adv_M CAN do:**
- Everything Adv_R can do
- Selectively drop messages (blackhole)
- Selectively delay messages
- Send false routing advertisements
- Collude with other malicious relays to combine observations

**What Adv_M CANNOT do:**
- Forge message signatures (requires private key)
- Modify message content (detected via AES-GCM auth tag)
- Modify signed routing advertisements
- Break forward secrecy guarantees

---

### Class E: Compromised Node Adversary

**Formal Definition:**
A node-compromise adversary Adv_C has:
- Full access to the software and storage of a compromised IRIS node
- Read access to the compromised device's key material (subject to hardware security)
- The ability to act as that node going forward

**What Adv_C CAN do (after compromise):**
- Read all messages stored on the compromised node
- Send messages as the compromised node's identity
- Access the compromised node's contact list and routing state
- Compromise future messages to the extent of the compromised node's conversations

**What Adv_C CANNOT do:**
- Read past messages protected by forward secrecy (if message keys deleted after reading)
- Read messages between other nodes not passing through the compromised node
- Impersonate a different node (only has the compromised node's keys)

---

### Class F: Quantum-Capable Adversary (Future)

**Formal Definition:**
A quantum adversary Adv_Q has access to a cryptographically relevant quantum computer capable of running Shor's algorithm. This adversary can:
- Break elliptic curve discrete logarithm (breaks X25519 key exchange)
- Break elliptic curve signatures (breaks Ed25519)
- Cannot break AES-256 or SHA-256 (Grover's algorithm provides only quadratic speedup; AES-256 still requires 2^128 operations)

**Current IRIS Status:**
IRIS is currently NOT quantum-resistant. The identity layer and key exchange are based on Curve25519, which is broken by a quantum computer.

**Migration Plan:**
- Phase 1: Add ML-KEM-512 (CRYSTALS-Kyber) as a hybrid KEM alongside X25519
- Phase 2: Add ML-DSA-44 (CRYSTALS-Dilithium) as a hybrid signature alongside Ed25519  
- Phase 3: Remove classical algorithms after transition period

Post-quantum migration requires message format version bump and is planned for v2.0.

---

## Cryptographic Guarantees by Adversary Class

### Message Confidentiality

| Property | Adv_P | Adv_A | Adv_R | Adv_M | Adv_C |
|---|---|---|---|---|---|
| Cannot read message content | ✅ | ✅ | ✅ | ✅ | ❌ (own messages) |
| Cannot determine plaintext from ciphertext | ✅ | ✅ | ✅ | ✅ | ❌ |
| Cannot correlate sender-recipient content-wise | ✅ | ✅ | ✅ | ✅ | ❌ |

**Formal Property:** IND-CCA2 security of the message encryption scheme (AES-256-GCM with proper nonce management).

### Message Integrity and Authentication

| Property | Adv_P | Adv_A | Adv_R | Adv_M | Adv_C |
|---|---|---|---|---|---|
| Cannot forge message from another identity | ✅ | ✅ | ✅ | ✅ | ❌ (own identity) |
| Cannot modify message content undetected | ✅ | ✅ | ✅ | ✅ | N/A |
| Cannot replay old messages | ✅ | ✅ | ✅ | ✅ | ❌ (limited window) |

**Formal Property:** EUF-CMA security of Ed25519 signatures.

### Forward Secrecy

| Property | Adv_P | Adv_A | Adv_R | Adv_M | Adv_C |
|---|---|---|---|---|---|
| Compromise of current keys does not expose past messages | ✅ | ✅ | N/A | N/A | ✅ (if keys deleted) |

**Formal Property:** Forward secrecy via Double Ratchet — each message key is derived from a ratchet chain and deleted after use.

**Important caveat:** Forward secrecy applies only if the device actually deletes old message keys. If a device is compromised before keys are deleted (e.g., immediately after receiving a message), past messages may be readable.

### Break-in Recovery (Post-Compromise Security)

**Property:** After an adversary compromises the ratchet state, forward security is restored within a bounded number of messages (as new DH ratchet steps are taken).

**Formally:** Within `n` message exchanges after compromise is ended (where n is bounded by the ratchet step frequency), the adversary can no longer derive future message keys.

### Identity Authentication

| Property | Adv_P | Adv_A | Adv_R | Adv_M | Adv_C |
|---|---|---|---|---|---|
| Identity is cryptographically bound to key pair | ✅ | ✅ | ✅ | ✅ | ❌ |
| Cannot claim another's identity | ✅ | ✅ | ✅ | ✅ | ❌ |

**Caveat:** Identity binding requires manual key verification (safety number comparison). Without verification, an adversary performing active MITM during initial session setup could intercept.

### Routing Security

| Property | Adv_P | Adv_A | Adv_R | Adv_M | Adv_C |
|---|---|---|---|---|---|
| Cannot forge route advertisements | ✅ | ✅ | ✅ | ❌ (can drop/delay) | ❌ |
| Cannot redirect traffic to attacker-controlled path | ✅ | ✅ | ✅ | Partial | ❌ |

Adv_M can selectively drop messages but cannot inject false routing state that bears a forged signature. They can suppress route advertisements to isolate nodes.

---

## What IRIS Explicitly Does NOT Protect Against

### 1. Traffic Analysis by Relay Nodes (Adv_R)
IRIS does not provide anonymity. Relay nodes see sender and destination node IDs in routing headers. A relay node or an observer watching all links can correlate traffic patterns. This is by design — anonymity networks (Tor, I2P) have incompatible latency properties with real-time emergency communication.

**Acknowledged:** Contact graph inference is possible by any party that observes sufficient traffic.

### 2. Network-Level Traffic Analysis (Adv_P)
A passive observer watching all radio transmissions can determine:
- Which nodes are active and when
- Communication frequency between nodes
- Approximate location of communicating nodes (via signal strength)

### 3. Availability Guarantees Against Adv_M
A malicious relay node can always drop messages it receives. IRIS provides detection (no delivery ACK) and mitigation (multi-path routing, reputation) but cannot guarantee delivery if all paths pass through malicious nodes.

### 4. Metadata Confidentiality from Relay Nodes
The following metadata is visible to relay nodes:
- Sender node ID
- Destination node ID
- Message ID
- Timestamp
- Priority
- Message size

Reducing this would require onion routing, which has incompatible latency and complexity properties.

### 5. Physical Coercion
If an adversary physically coerces a user into unlocking their device and providing keys, IRIS provides no protection. This is outside the cryptographic threat model.

### 6. Compromise of Hardware Security Elements
If the Secure Enclave or StrongBox is compromised via a hardware vulnerability (physical attack, side channel), key material may be extracted. IRIS relies on OS and hardware vendor security for this layer.

### 7. Long-term Compromise Before Key Deletion
If a device is compromised and the adversary reads memory before the message key is deleted, forward secrecy of that message is lost. The deletion timing depends on the implementation.

---

## Session Establishment Security Model

### Pre-Authentication Phase
Before two nodes complete the handshake:
- They are communicating over the encrypted transport link (L1)
- They do not yet have authenticated session keys
- An active adversary can perform MITM if the user has not previously verified keys

**Deficiency:** First-use (TOFU) trust model — the first connection to a new identity is vulnerable to MITM if the adversary can intercept the initial key exchange. This is a fundamental limitation of any PKI-free system without out-of-band verification.

**Mitigation:** Safety number display and QR code verification for high-security use cases.

### Post-Authentication Phase
After handshake completes with verified keys:
- Message keys derived from authenticated shared secret
- Active adversary cannot perform MITM
- Adv_A can attempt replay (mitigated by message ID uniqueness + replay window)

---

## Formal Security Properties Summary

```
Property                                          Guaranteed Against
─────────────────────────────────────────────────────────────────────
Message confidentiality (semantic security)       Adv_P, Adv_A, Adv_R, Adv_M
Message integrity (no undetected modification)    Adv_P, Adv_A, Adv_R, Adv_M
Sender authentication (forge prevention)          Adv_P, Adv_A, Adv_R, Adv_M
Forward secrecy (past messages safe)              Adv_P, Adv_A (given key deletion)
Replay protection                                 Adv_P, Adv_A, Adv_R
Emergency broadcast authentication                Adv_P, Adv_A, Adv_R, Adv_M
─────────────────────────────────────────────────────────────────────
Traffic analysis resistance                       NOT GUARANTEED
Anonymity (hiding who communicates with whom)     NOT GUARANTEED
Availability (DoS resistance)                     PARTIAL (rate limits, multi-path)
Post-compromise secrecy                           PARTIAL (depends on ratchet steps)
Quantum resistance                                NOT GUARANTEED (migration planned)
─────────────────────────────────────────────────────────────────────
```

---

## Comparison to Related Systems

| Property | IRIS | Signal | Tor | LoRa mesh (typical) |
|---|---|---|---|---|
| E2EE | ✅ | ✅ | Partial | ❌ |
| Forward secrecy | ✅ | ✅ | Session | ❌ |
| Anonymity | ❌ | Partial | ✅ | ❌ |
| Offline operation | ✅ | ❌ | ❌ | ✅ |
| DTN/store-carry | ✅ | ❌ | ❌ | Sometimes |
| Emergency priority | ✅ | ❌ | ❌ | Sometimes |

IRIS makes a different set of tradeoffs than Signal (optimized for online mobile use) and Tor (optimized for anonymity). The tradeoffs are driven by the requirement for offline-first, emergency-capable operation.
