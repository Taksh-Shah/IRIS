# Standards Review

**Status:** Living document  
**Last updated:** 2026-08-11  
**Owner:** Research / Architecture  

---

## Purpose

This document evaluates each relevant standard against IRIS design. For each standard, it states: what IRIS adopts, what IRIS deviates from, and the rationale. Deviations are not bugs — they are deliberate decisions made with full awareness of the standard, documented here for future auditors.

---

## 1. RFC 9171 — Bundle Protocol Version 7

**Published:** January 2022  
**Status:** Proposed Standard  
**Reference:** https://datatracker.ietf.org/doc/rfc9171/

### 1.1 What IRIS Adopts

**Bundle structure:** IRIS uses the conceptual three-part bundle structure: primary block (routing metadata) + payload block (content) + optional extension blocks. The semantic roles are identical to BPv7.

**Bundle lifetime / TTL:** IRIS implements message TTL semantically identical to BPv7's bundle lifetime. A bundle with elapsed TTL is dropped and not forwarded. Default TTL values differ by priority class (see protocol specification).

**Endpoint Identifier (EID) concept:** IRIS uses URN-style endpoint identifiers: `iris://node/{public_key_hex}`. The scheme is analogous to BPv7's `dtn://` scheme. The namespace is different.

**Custody concept:** IRIS implements simplified custody signaling: a node that accepts a bundle for relay records it in the bundle store. If the bundle TTL elapses or storage pressure occurs, custody is released. This is a simplified version of BPv7 custody transfer.

**Fragmentation concept:** IRIS supports bundle fragmentation for LoRa (maximum 222-byte LoRa payload). The fragmentation scheme is semantically equivalent to BPv7 fragmentation (fragment offset + total ADU length in primary block).

### 1.2 What IRIS Deviates From

**Wire encoding:** BPv7 mandates CBOR (RFC 8949) encoding for all blocks. IRIS uses CBOR for the wire format at the transport layer but with a simplified field set. IRIS does not implement the full BPv7 CBOR map structure — it uses a compact array encoding to reduce overhead on LoRa.

**Rationale:** BPv7 CBOR encoding of a primary block is 60–100 bytes. With a 222-byte LoRa payload, that leaves 122–162 bytes for payload. IRIS compact encoding reduces primary block to 40–60 bytes. The difference is meaningful for small emergency messages.

**Extension block registry:** IRIS does not implement the IANA-registered BPv7 extension block types (Previous Node Block, Bundle Age Block, Hop Count Block). These are planned for v2.

**Administrative records:** BPv7 defines status reports and custody signals as administrative records. IRIS uses a simplified in-band acknowledgment mechanism rather than BPv7 administrative records.

**EID scheme:** IRIS uses `iris://` not `dtn://`. An IRIS node is not directly addressable by a BPv7-compliant DTN node. Interoperability with the broader DTN ecosystem requires a gateway that translates between schemes.

### 1.3 Interoperability Implications

IRIS is not BPv7-compliant today. A gateway translating between IRIS wire format and BPv7 is a planned feature (roadmap, not MVP). For the disaster response use case, interoperability with external DTN nodes is not a current requirement.

---

## 2. RFC 9172 — BPSec: Bundle Protocol Security

**Published:** January 2022  
**Status:** Proposed Standard  
**Reference:** https://datatracker.ietf.org/doc/rfc9172/

### 2.1 What IRIS Adopts from BPSec Security Model

**Block Integrity Block (BIB) semantics:** BPSec BIB provides integrity protection for specified target blocks. IRIS implements equivalent functionality: every bundle payload is signed with the sender's Ed25519 private key. Any relay node can verify the signature without decrypting the payload. This matches BIB's design intent.

**Block Confidentiality Block (BCB) semantics:** BPSec BCB provides encryption of specified target blocks. IRIS encrypts payloads using ChaCha20-Poly1305 with a key derived from X25519 key agreement between sender and recipient. Relay nodes cannot decrypt payloads. This matches BCB's design intent.

**Source authentication:** BPSec requires the source EID to be cryptographically authenticated. IRIS achieves this via Ed25519 signing: the source public key IS the source EID (truncated to 32 bytes in IRIS compact format).

### 2.2 What IRIS Deviates From

**Security context identifiers:** BPSec uses a Security Context Identifier (SCID) registry for algorithm identification. IRIS does not use SCID; algorithm is implicit (Ed25519 + ChaCha20-Poly1305 is the only supported suite in v1).

**Security blocks as extension blocks:** BPSec security blocks are extension blocks within the BPv7 bundle. IRIS does not use BPv7 extension blocks — security fields are embedded in the IRIS primary block and a security suffix following the payload.

**Key management:** BPSec is silent on key management (by design). IRIS implements a specific key management approach: contact-based key exchange using X25519 ephemeral keys exchanged at first BLE/Wi-Fi contact.

---

## 3. IEEE 802.15.1 — Bluetooth

**Relevant version:** Bluetooth Core Specification 5.4 (2023)  
**Reference:** Bluetooth SIG Core Specification

### 3.1 BLE 5.x Features Used by IRIS

**Coded PHY (LE Coded):** Introduced in BLE 5.0. Adds FEC coding at 125 kbps (S=8, 4x range gain) or 500 kbps (S=2, 2x range gain). IRIS uses LE Coded PHY at S=8 for discovery beacons, trading bandwidth for range. Supported on Android 8.0+ (Pixel 3 and later). Not universally supported on cheap Indian Android devices — IRIS falls back to 1M PHY when Coded PHY is not available.

**Extended Advertising:** BLE 5.0. Allows advertising payloads up to 255 bytes (vs 31 bytes in BLE 4.x). IRIS uses extended advertising to include node capability announcement and contact ID in the beacon, eliminating a separate connection for capability discovery.

**Periodic Advertising / Observer:** BLE 5.1. Allows scheduled broadcast without connection. IRIS uses periodic advertising for presence beacons (low-power, no connection overhead).

**Direction Finding (AoA/AoD):** BLE 5.1. Not used by IRIS (hardware support limited; not relevant for distance estimation at mesh hop scale).

### 3.2 Minimum BLE Version Requirement

IRIS requires BLE 5.0 (LE Coded PHY + Extended Advertising). Devices with only BLE 4.x will function but with reduced range and slower capability discovery. BLE 4.x support is a tracked compatibility issue, not a blocking requirement.

---

## 4. IEEE 802.11 — Wi-Fi

### 4.1 Wi-Fi Direct (P2P)

**Standard:** IEEE 802.11-2020 §10 (Wi-Fi P2P), refined by Wi-Fi Alliance Wi-Fi Direct specification.

IRIS uses Wi-Fi Direct for medium-range (30–100m) high-bandwidth transport between nodes. Key aspects:
- Group Owner (GO) / Group Client (GC) negotiation: IRIS prefers the node with more routing knowledge to be GO
- P2P discovery: IRIS triggers P2P Find periodically when BLE contact rate drops below threshold
- Group formation: IRIS uses persistent groups where possible to reduce re-association overhead

**Android constraints:** Wi-Fi Direct on Android does not support simultaneous GO and station mode on most devices — a node acting as Wi-Fi Direct GO cannot simultaneously connect to Wi-Fi infrastructure. IRIS handles this by: (a) Wi-Fi Direct operates only in relay mode (no infrastructure connection needed), or (b) Wi-Fi Direct uses a separate Wi-Fi chip if dual-chip Android (some Qualcomm-based devices).

### 4.2 Wi-Fi Aware (NAN — Neighbor Awareness Networking)

**Standard:** IEEE 802.11aq (NAN), implemented by Wi-Fi Aware certification.

Wi-Fi Aware provides lower-latency discovery than Wi-Fi Direct with connectionless data transfer via NDP (NAN Data Path). Advantages over Wi-Fi Direct for IRIS:
- No GO/GC negotiation required
- Sub-second discovery
- Can coexist with Wi-Fi STA connection (depends on driver)

IRIS plans Wi-Fi Aware support as a preferred alternative to Wi-Fi Direct on Android 8.0+ devices (Wi-Fi Aware support is broader than Wi-Fi Direct on recent Android). Wi-Fi Aware is documented in IRIS transport spec but not implemented in MVP.

### 4.3 5 GHz Wi-Fi and DFS

IRIS does not use 5 GHz Wi-Fi for transport. 5 GHz Wi-Fi Direct and Wi-Fi Aware are theoretically possible but:
- DFS (Dynamic Frequency Selection) channels require radar detection; not suitable for disaster deployment
- 5 GHz has shorter range than 2.4 GHz at equal power
- 5 GHz support is less universal on cheap Indian Android devices

---

## 5. India WPC ISM Band Regulations

**Authority:** Wireless Planning and Coordination Wing, Ministry of Communications, Government of India  
**Relevant document:** WPC Gazette Notification on De-licensed Spectrum (last updated 2023)

### 5.1 LoRa: 865–867 MHz

| Parameter | WPC Rule |
|-----------|----------|
| Frequency range | 865.0–867.0 MHz |
| Maximum EIRP | 1 W (30 dBm) — IRIS targets ≤25 mW (14 dBm) |
| License required | No (de-licensed) |
| Duty cycle | Not specified (WPC does not impose duty cycle unlike EU ETSI EN 300 220) |
| Channel plan | Three 200 kHz channels: 865.0625, 865.4025, 865.985 MHz |
| Interference protection | User accepts interference; no protection from other users |

IRIS operates within these constraints. IRIS's 14 dBm target (SX1262 default) provides a comfortable margin below the 30 dBm limit.

### 5.2 BLE and Wi-Fi: 2.4 GHz

2.4 GHz is de-licensed in India for low-power applications. Bluetooth and Wi-Fi devices with TRAI/WPC type approval operate without individual license. IRIS Android app uses Bluetooth and Wi-Fi chipsets that are pre-approved as part of device type approval. No additional spectrum license is required.

---

## 6. DPDPA 2023 — Digital Personal Data Protection Act

**Reference:** Digital Personal Data Protection Act, 2023. No. 22 of 2023.

### 6.1 Sections Affecting IRIS Design

**Section 4 (Grounds for processing):** Personal data may be processed only for a lawful purpose for which consent has been given, or for certain legitimate uses. IRIS processes: user's own location (for sharing), contact history (for routing). Both require either consent or fall under the legitimate use provisions.

**Section 6 (Notice):** Data fiduciary must provide notice before consent. IRIS provides notice during onboarding of: what data is collected, how it is used, who can access it.

**Section 9 (Data fiduciary obligations):** Includes data minimization, accuracy, storage limitation, and security safeguards. IRIS design reflects these requirements — see PRIVACY_REQUIREMENTS.md.

**Section 12 (Right of access):** Data principal has the right to access their personal data. IRIS implements data export (SQLite dump of own messages, own contact history).

**Section 13 (Right to correction and erasure):** Data principal has the right to correct and delete their personal data. IRIS implements local data deletion (wipe own data) and propagates deletion requests to relay nodes where technically feasible.

**Section 16 (Significant data fiduciary):** Entities processing large volumes of sensitive data may be designated as Significant Data Fiduciaries with additional obligations. IRIS gateway operators processing data at scale must evaluate whether SDef classification applies.

### 6.2 What DPDPA Does Not Affect in IRIS Design

Relay nodes never decrypt message content — the relay node processes encrypted bundles but has no access to personal data within the bundle. Whether a relay node is a "data fiduciary" for the routing metadata (source/destination EIDs, TTL) it processes is an open legal question — see LEGAL_RESEARCH.md LQ-002.

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
