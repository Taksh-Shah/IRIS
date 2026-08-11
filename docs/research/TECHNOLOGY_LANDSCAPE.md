# Technology Landscape: Disruption-Tolerant Mesh Networking

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research  

---

## 1. Delay/Disruption-Tolerant Networking (DTN)

### 1.1 Foundational RFCs

**RFC 4838 — Delay-Tolerant Networking Architecture (2007)**  
Cerf et al. Defines the architectural framework for networks where end-to-end connectivity cannot be assumed. Introduces the concept of a "bundle layer" above transport. IRIS adopts the store-carry-forward model described here. Key concept: custody transfer — a node accepts responsibility for delivering a bundle when it takes custody.

**RFC 9171 — Bundle Protocol Version 7 (2022)**  
Scott & Burleigh. The current IETF standard for the DTN bundle protocol. Replaces RFC 5050 (BPv6). Uses CBOR encoding (RFC 8949). IRIS wire format is inspired by but not strictly conformant to BPv7 — see ADR-0001 and STANDARDS_REVIEW.md for deviation rationale.

Key BPv7 structures IRIS adopts:
- Bundle: primary block + payload block + extension blocks
- Endpoint ID (EID) scheme: IRIS uses `iris://` URI scheme analogous to BPv7 `dtn://`
- Bundle lifetime / TTL field
- Custody signaling concept (IRIS implements a simplified variant)

Key BPv7 structures IRIS deviates from:
- IRIS does not implement the full extension block registry
- IRIS uses a simplified EID format for on-wire compactness on LoRa
- IRIS does not implement administrative records (status reports) in v1

**RFC 9172 — BPSec: Bundle Protocol Security (2022)**  
Ed. Birrane & McKeever. Defines the security block extensions for BPv7. Specifies Block Integrity Block (BIB) and Block Confidentiality Block (BCB). IRIS implements equivalent security semantics using Ed25519 signatures (integrity) and ChaCha20-Poly1305 (confidentiality) — see ADR-0002 and ADR-0006.

---

## 2. DTN Routing Algorithms

### 2.1 Epidemic Routing

Vahdat & Becker (2000). Flooding-based: every node replicates every bundle to every encountered node. Maximizes delivery probability at the cost of storage and bandwidth. IRIS does not use epidemic routing as the primary algorithm due to resource constraints in disaster environments (finite storage, limited bandwidth on LoRa). Used in IRIS as a fallback for high-priority SOS bundles only.

### 2.2 PRoPHET — Probabilistic Routing Protocol using History of Encounters and Transitivity

**Lindgren, Doria, Schelén (2004)** — "Probabilistic Routing in Intermittently Connected Networks." MobiHoc 2004.

The primary IRIS routing algorithm. Core concepts:
- **Delivery predictability** P(a,b): probability that node a can deliver to node b, learned from contact history
- **Aging**: P(a,b) decreases over time without contact (configurable decay)
- **Transitivity**: if a meets c and c has high P(c,b), then a increases its P(a,b) via β-weighted update
- **Forwarding rule**: forward bundle to node with higher P toward destination

Parameters (IRIS defaults, subject to field tuning — see GAP-001 in RESEARCH_GAPS.md):

| Parameter | Symbol | IRIS Default | PRoPHET Paper Range |
|-----------|--------|-------------|-------------------|
| Encounter constant | P_init | 0.75 | 0.5–0.9 |
| Aging factor | γ | 0.98 | 0.95–0.99 |
| Transitivity scaling | β | 0.25 | 0.1–0.5 |
| History window | — | 30 days | application-dependent |

### 2.3 Spray-and-Wait

**Spyropoulos, Psounis, Raghavendra (2005)** — "Spray and Wait: An Efficient Routing Scheme for Intermittently Connected Mobile Networks." SIGCOMM WDTN 2005.

IRIS secondary routing algorithm; activates when PRoPHET history is insufficient (new node, cold start). Two phases:
1. **Spray phase**: distribute L copies of the bundle to L distinct relays
2. **Wait phase**: each relay delivers directly when it encounters the destination

Binary Spray-and-Wait variant (Spyropoulos et al. 2006) is used: each forward transfers L/2 copies, halving until L=1 (direct delivery only). More efficient than naive spray.

The optimal L parameter for Indian disaster scenarios is an open research gap — see RESEARCH_GAPS.md GAP-001.

### 2.4 MaxProp

Burgess et al. (2006). Priority-based routing that considers both delivery probability and bundle age. More complex than PRoPHET; higher delivery rate in dense networks but higher computational cost. Not adopted for IRIS v1 due to complexity; documented for future consideration.

### 2.5 RAPID

Balasubramanian et al. (2007). Utility-based routing with resource allocation. Maximizes a global utility function. Requires global state that is not available in partitioned networks. Not suitable for IRIS disaster scenarios.

---

## 3. LoRa / LoRaWAN Landscape

### 3.1 Physical Layer (LoRa)

LoRa is a Semtech-proprietary chirp spread spectrum (CSS) modulation — not an open standard. The key silicon:

**Semtech SX1262** — current-generation LoRa transceiver (2019).
- Frequency range: 150–960 MHz (configurable)
- Output power: up to +22 dBm
- Receive sensitivity: −148 dBm at SF12, BW125
- Data rates: 0.018–37.5 kbps depending on SF/BW/CR
- Current draw: 4.2 mA receive, 22 mA transmit at 14 dBm

Key LoRa parameters and IRIS defaults:

| Parameter | Symbol | IRIS Default | Rationale |
|-----------|--------|-------------|-----------|
| Spreading Factor | SF | SF10 | Balance range/datarate; ~980 bps at SF10 BW125 |
| Bandwidth | BW | 125 kHz | Standard; lower BW improves sensitivity |
| Coding Rate | CR | 4/5 | Minimal FEC overhead |
| Preamble length | — | 8 symbols | Standard |
| Sync word | — | 0x12 | Private network (vs 0x34 LoRaWAN public) |

**Semtech SX1276** — previous generation; still widely available on cheap modules (RFM95, Ra-02). Lower output power (+20 dBm), similar sensitivity. IRIS supports both.

### 3.2 LoRaWAN (not used by IRIS core)

LoRaWAN is the MAC/network layer from the LoRa Alliance built on top of LoRa modulation. It requires gateway infrastructure and a network server. IRIS does not use LoRaWAN because:
- IRIS operates in fully infrastructure-free mode
- LoRaWAN does not support multi-hop relay between end devices
- IRIS uses raw LoRa with a custom MAC layer

IRIS gateway nodes (RPi-based) may optionally bridge to LoRaWAN networks for reach extension, but this is not a core dependency.

### 3.3 India WPC Regulatory Landscape

The Wireless Planning and Coordination (WPC) wing of the Ministry of Communications governs spectrum in India.

**ISM Band 865–867 MHz:**
- Designated for Industrial, Scientific, Medical use — no individual license required
- Maximum EIRP: 1 W (30 dBm) — however, IRIS targets ≤25 mW EIRP (14 dBm) for duty-cycle compliance
- Duty cycle: WPC does not specify a duty cycle limit for 865–867 MHz (unlike EU's 1% limit on 868 MHz)
- Channel plan: India uses 865.0625, 865.4025, 865.985 MHz (per TRAI recommendations)

See SPECTRUM_CONSIDERATIONS.md and INDIA_COMPLIANCE.md for full regulatory analysis.

### 3.4 Academic LoRa Modeling

**Bor, Roedig, Harvey, Friedman (2016)** — "Do LoRa Low-Power Wide-Area Networks Scale?" ACM SenSys 2016. Demonstrates LoRa capacity limits in dense deployments; relevant to urban IRIS deployments. Key finding: with 1% duty cycle and standard SF7-SF12 mix, a single gateway supports ~120 nodes before collision probability becomes significant.

**Croce, Gaillard, Gifford, Ferré, Bui (2017)** — "Impact of LoRa Imperfect Orthogonality." IEEE Communications Letters. Shows inter-SF interference is higher than originally claimed; affects IRIS channel planning in dense urban scenarios.

---

## 4. Cryptographic Primitives Landscape

### 4.1 Digital Signatures

**Ed25519 (RFC 8032)** — Edwards-curve Digital Signature Algorithm on Curve25519.
- Signature size: 64 bytes
- Public key size: 32 bytes
- Verify speed: ~70,000 verifications/second on ARM Cortex-A53 (RPi)
- Security level: 128-bit equivalent
- Constant-time implementation: yes (by construction; no timing side-channels)

**Why Ed25519 over RSA-2048:**
- RSA-2048 signature: 256 bytes vs Ed25519: 64 bytes — 4x size advantage critical on LoRa (255-byte max payload)
- RSA-2048 verify: ~3,000/s vs Ed25519: ~70,000/s on ARM
- RSA key generation is expensive on constrained devices; Ed25519 generation is fast
- RSA is vulnerable to side-channel attacks without careful implementation; Ed25519 is constant-time

**Why Ed25519 over ECDSA-P256:**
- ECDSA requires a random nonce per signature; nonce reuse is catastrophic (as demonstrated in PS3 attack)
- Ed25519 is deterministic — no nonce required
- Ed25519 implementation complexity is lower; fewer failure modes
- P256 is NIST-standardized (favorable for FIPS compliance, unfavorable for IRIS today — see ADR-0002)

### 4.2 Hash Functions

**BLAKE3** — modern hash function (2020). 
- Speed: 2–3 GB/s on commodity hardware (vs SHA-256: ~500 MB/s without hardware acceleration)
- Security: 256-bit output, no length extension vulnerability
- Tree hashing: native parallelism for large payloads

**Why BLAKE3 over SHA-256:**
- SHA-256 hardware acceleration (SHA-NI) is not present on low-cost Android SoCs
- BLAKE3 software performance is 4–6x faster on ARM without SIMD
- BLAKE3 is immune to length extension attacks (SHA-256 is not without HMAC)
- BLAKE3's tree structure allows streaming verification

IRIS uses BLAKE3 for: bundle integrity checks, contact history Bloom filter hashing, routing table checksums.

### 4.3 Symmetric Encryption

See ADR-0006 for ChaCha20-Poly1305 decision rationale.

### 4.4 Key Exchange

X25519 (Diffie-Hellman on Curve25519, RFC 7748) for session key establishment. Combined with Ed25519 identity keys via a separation of signing and DH keys (same underlying curve, different key pairs as per best practice).

---

## 5. Standards Bodies

### 5.1 IETF DTNWG (Delay-Tolerant Networking Working Group)

Active working group producing RFC 9171 (BPv7), RFC 9172 (BPSec), and ongoing work on convergence layers. IRIS monitors DTNWG output. Relevant active drafts:
- draft-ietf-dtn-bpbis (BPv7 updates)
- draft-ietf-dtn-tcpclv4 (TCP convergence layer)

### 5.2 ITU-T

ITU-T Study Group 17 (Security) and Study Group 13 (Future Networks) produce standards relevant to emergency communications. ITU-T X.1303bis covers Common Alerting Protocol (CAP) — relevant to IRIS authority broadcast feature.

### 5.3 TSDSI — Telecommunications Standards Development Society of India

India's national standards body for telecommunications. TSDSI produces India-specific 5G NR standards and has liaison relationships with 3GPP. Relevant to IRIS:
- TSDSI STD T56-2021: Telecom equipment security standards
- TSDSI work on M2M/IoT standards for constrained devices

IRIS should engage TSDSI during government procurement discussions to understand certification requirements.

### 5.4 BIS — Bureau of Indian Standards

BIS IS 13252 (Part 1): Safety of information technology equipment. Relevant for hardware kit certification. IRIS RPi-based gateway kits will require BIS certification for commercial sale in India.

---

## 6. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
