# Data Governance

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Legal / Engineering  

---

## 1. Governance Philosophy

IRIS data governance follows a **local-first, user-controlled** model. The core principle: data lives on the user's device and moves only when the user explicitly initiates communication. No central data repository exists for message content. No analytics platform processes user communication patterns.

This is not merely a privacy policy statement — it is a technical architecture constraint. Systems that cannot collect data cannot breach it.

---

## 2. Data Classification

### 2.1 What IRIS Holds

| Data | Location | Encrypted at Rest | Accessible To | Retention |
|------|----------|------------------|--------------|-----------|
| Own Ed25519 key pair | Android Keystore / iOS Secure Enclave | Yes (hardware) | User's device only | Permanent until user regenerates |
| Sent messages (own copy) | Local SQLite (encrypted) | Yes | User only (own key) | Per-message TTL |
| Received messages (decrypted) | Local SQLite (encrypted) | Yes | User only (own key) | Per-message TTL |
| In-transit bundles (relay) | Local SQLite (relay store) | Yes (payload), No (header) | Routing only (header); payload opaque | Bundle TTL |
| Contact history | Local SQLite (encrypted) | Yes | User only | 30 days rolling |
| Routing table (PRoPHET P values) | Local SQLite | No (not PII) | User and relay function | 30 days |
| Edge server bundle cache | Edge server storage | Yes (payload) | Edge server operator (header only); payload opaque | Bundle TTL, max 7 days |
| Edge server connection log | Edge server log storage | No | Edge server operator | 7 days |

### 2.2 What IRIS Never Holds

| Data | Technical Reason |
|------|-----------------|
| Private keys of other users | Each user generates and retains their own keys; IRIS infrastructure has no key escrow |
| Plaintext content of others' messages | End-to-end encryption; decryption requires recipient's private key |
| Full contact graph of any user | Contact history is local to each device; no central aggregation |
| User location history | Location is broadcast on demand, not logged at relay or server |
| Real identity mapping (name → Node ID) | No account registration; mapping is user-controlled in their own contact book |

---

## 3. Data Controller Definitions

### 3.1 User as Data Controller of Own Node

Under DPDPA, a "Data Fiduciary" determines the purpose and means of processing personal data. Each IRIS user who runs the IRIS app on their device is the Data Fiduciary (acting as an individual exercising personal use exemption) for:
- Their own messages and contact history on their device
- Location they choose to share

The personal use exemption in DPDPA Section 3(c) excludes "processing of personal data by an individual for any personal or domestic purpose." IRIS personal use falls within this exemption for individual users.

### 3.2 IRIS Technologies as Data Fiduciary

IRIS Technologies (the operating entity) is a Data Fiduciary for:
- Data processed on IRIS-operated edge servers (connection logs, bundle cache metadata)
- Analytics data collected with explicit consent (crash reports, opt-in diagnostics)
- Contact and support data (emails, support tickets)

IRIS Technologies is NOT a Data Fiduciary for message content processed by relay nodes, because relay nodes cannot access message content.

### 3.3 Enterprise Gateway Operators as Data Fiduciaries

Organizations that deploy IRIS gateway nodes under enterprise license are independent Data Fiduciaries for:
- Bundle metadata processed by their gateways (source/destination EIDs, timestamps, sizes)
- Connection logs from their gateway to end devices
- Edge server logs if they operate edge servers

Enterprise operators must establish their own privacy policies covering gateway operation, consistent with DPDPA requirements. IRIS provides a Data Processing Agreement (DPA) template for enterprise operators.

### 3.4 Government Operators

Government entities (NDRF, SDRF, district DMAs) operating IRIS under emergency use are Data Fiduciaries for data processed on government-operated infrastructure. Government operators are subject to DPDPA to the extent it applies to government entities (DPDPA Section 17 exemptions for national security and law enforcement apply to certain government activities but not all government data processing).

---

## 4. What IRIS Never Holds: Technical Guarantees

The following guarantees are structural, not policy:

**Private key isolation:** Ed25519 private keys are generated and stored in Android Keystore (hardware-backed on devices with a Secure Element) or iOS Secure Enclave. Keys are marked non-exportable. IRIS application code cannot read the private key value — it can only request the Keystore/Secure Enclave to perform signing operations.

**Relay opacity:** The relay bundle store in SQLite contains the full bundle (header + encrypted payload). The payload is ChaCha20-Poly1305 ciphertext addressable only with the recipient's private key. IRIS relay code never calls any decryption function on relay payloads. This is enforced at the type level in Rust: relay functions receive `Bundle<Encrypted>` typed values, which have no decrypt method in scope.

**No central message database:** There is no IRIS cloud service that receives and stores all messages. Edge servers are optional infrastructure that caches bundles in transit — they are relay nodes, not message stores. An edge server that goes offline loses nothing that isn't also replicated in the mesh.

---

## 5. Operator Responsibilities

### 5.1 Gateway Operators

Entities operating IRIS gateway nodes (typically RPi-based infrastructure nodes) are responsible for:
- Physical security of the gateway hardware
- Patching the IRIS gateway software within 30 days of security releases
- Configuring the gateway to enforce IRIS bundle TTL (gateways must not indefinitely retain bundles)
- Complying with lawful interception orders for the metadata they have access to (source/destination EIDs, connection logs)
- Reporting data breaches to DPBI within 72 hours if personal data is involved

### 5.2 Edge Server Operators

Edge server operators (organizations running IRIS edge server infrastructure) are responsible for:
- All gateway operator responsibilities above
- Maintaining TLS 1.3 encryption on all connections to edge server
- Log retention in accordance with their data retention policy (IRIS default: 7 days)
- Configuring appropriate firewall and access controls
- Providing IRIS Technologies with audit logs on request for incident response

### 5.3 IRIS Technologies Responsibilities

IRIS Technologies is responsible for:
- Maintaining and publishing the IRIS Privacy Policy
- Operating the IRIS opt-in analytics and crash reporting infrastructure in accordance with DPDPA
- Responding to data subject rights requests regarding IRIS Technologies-held data
- Maintaining Data Processing Agreements with enterprise operators
- Notifying users of material changes to data practices
- Designating a Data Protection Officer (required under DPDPA for Data Fiduciaries processing personal data at scale)

---

## 6. Government Data Requests

When a government authority issues a lawful order to disclose data about an IRIS user:

1. Verify the order is from a competent authority and meets the legal requirements of the applicable statute (IT Act §69, DPDPA Section 17, CrPC, etc.)
2. Contact legal counsel immediately
3. Disclose only the specific data requested; do not disclose beyond the scope of the order
4. Data available to IRIS Technologies for disclosure: connection logs (node IDs, timestamps, bytes) from IRIS-operated edge servers; no message content
5. Data not available for disclosure: message content (technically inaccessible — IRIS does not hold decryption keys); private keys; contact history (stored only on user devices, not on IRIS servers)
6. Inform the user of the disclosure after any confidentiality obligation expires, to the extent legally permitted
7. Log all government data requests in an internal transparency register

See a separate Law Enforcement Requests policy document (to be developed) for the detailed process.

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
