# Privacy Requirements

**Status:** Draft — pending counsel review  
**Last updated:** 2026-08-11  
**Owner:** Legal / Engineering  

---

## 1. Purpose and Scope

This document maps DPDPA 2023 requirements to specific IRIS engineering features. It defines what data IRIS collects, the legal basis for processing each category, and the engineering requirements that implement each privacy obligation.

Scope: IRIS Android app, IRIS iOS app, IRIS desktop app (Tauri), IRIS edge server, IRIS gateway node.

---

## 2. Data Inventory

### 2.1 Data Collected by IRIS

| Data Type | Description | Collected By | Legal Basis | Retention |
|-----------|-------------|-------------|------------|-----------|
| IRIS Node ID | Ed25519 public key hash (32 bytes); user's mesh identity | All nodes | Legitimate use (technical necessity) | Permanent (user's own identity) |
| Location (active share) | GPS lat/lng or cell-ID estimate; shared with mesh on request | Sender's device | Consent | Session (cleared when location sharing off) |
| Contact history | Timestamp + peer Node ID for each BLE/Wi-Fi/LoRa contact event | Local device | Consent | 30 days (configurable; user can reduce) |
| Messages (sent/received) | Encrypted bundle; decryptable only by intended recipient | Local device | Consent | Per-message TTL (default 7 days; user can set) |
| Routing table | PRoPHET P(a,b) values for known destinations | Local device | Legitimate use (routing function) | 30 days (tied to contact history) |
| Bundle metadata | Source EID, destination EID, bundle ID, TTL, hop count | Relay devices | Legitimate use (routing function) | Duration of bundle TTL only |
| Edge server logs | Connection timestamps, node IDs of connected devices | Edge server | Legitimate use (operational necessity) | 7 days max |

### 2.2 Data NOT Collected by IRIS

The following data is explicitly not collected and not derivable by IRIS relay infrastructure:

| Data | Why Not Collected |
|------|------------------|
| Message plaintext content (at relay) | End-to-end encryption: relay nodes cannot decrypt |
| User's real name | IRIS identity is a public key; no name required |
| Phone number | No account, no SMS verification |
| Email address | No account registration |
| Device IMEI | IRIS does not request IMEI permission |
| Full contact graph (at central server) | No central server; contact history is local only |
| Location history beyond active session | Location cleared when user turns off sharing |
| Message content at edge server | Edge servers relay encrypted bundles; no decryption capability |

---

## 3. Consent Framework

### 3.1 Consent Categories

IRIS uses granular, revocable consent for each data category:

**Consent 1 — Contact history accumulation:**  
"IRIS records when your device is near other IRIS devices. This is used to route your messages more effectively. No message content is included. You can disable this and delete all history."

- Required for: PRoPHET routing (core function)
- If denied: IRIS falls back to Spray-and-Wait (reduced routing quality; not fatal)
- Revocable: Yes; contact history deleted on revocation

**Consent 2 — Active location sharing:**  
"When you choose to share your location, IRIS will include your GPS coordinates in messages sent to selected contacts. Your location is shared only when you actively choose to share it."

- Required for: location-sharing feature (P2 requirement)
- If denied: location features disabled; app still works
- Revocable: Yes; location cleared immediately

**Consent 3 — Crash reports and diagnostics:**  
"Help improve IRIS by sending anonymous crash reports and performance data. No message content, no locations, no contact identifiers."

- Required for: quality improvement
- Default: off (opt-in)
- Revocable: Yes

### 3.2 Onboarding Consent Flow

```
App first launch
    ↓
Welcome screen (purpose of IRIS explained in 3 sentences)
    ↓
Consent 1: Contact history (Yes / No)
    ↓
Consent 2: Location sharing (Yes / No — can be changed later)
    ↓
Consent 3: Diagnostics (Yes / No — default No)
    ↓
Node ID generated (Ed25519 key pair in Keystore / Secure Enclave)
    ↓
App ready (no server contact, no account)
```

Consent is not bundled — each consent is presented separately. No consent is mandatory for basic app function.

---

## 4. Data Minimization

Data minimization is a design principle, not just a policy.

### 4.1 Contact History Minimization

Contact history records contain only:
- Timestamp (Unix seconds — not milliseconds; precision limited to prevent exact location inference)
- Peer Node ID (32-byte public key hash — not derived from personal identifier)
- Transport type (BLE / Wi-Fi / LoRa — to calibrate contact quality for routing)
- Contact duration (seconds, rounded to nearest 10s)

Contact history does NOT record:
- RSSI / signal strength (could allow proximity inference)
- Exact GPS location (not available at contact event)
- Message content or bundle IDs
- Contact frequency breakdown (only cumulative contact duration)

### 4.2 Routing Metadata Minimization at Relay

Bundle headers processed by relay nodes contain:
- Source EID (32 bytes)
- Destination EID (32 bytes)
- Bundle ID (16 bytes, random)
- TTL (4 bytes)
- Hop count (2 bytes)

Relay nodes do NOT add:
- Own identity to bundle header in plaintext (relay identity only added in an encrypted routing extension not accessible to downstream relays)
- Timestamp of relay (prevents traffic analysis of relay path timing)

### 4.3 Edge Server Minimization

Edge server logs contain:
- Connection timestamp (rounded to hour — not exact)
- Connecting node's public key hash
- Bytes transferred (no content)

Edge servers retain no bundle content — bundles are stored in the bundle store during transit and cleared after delivery or TTL expiry.

---

## 5. Retention Policy

| Data | Retention | User Control |
|------|-----------|-------------|
| Contact history | 30 days rolling (configurable 1–90 days) | User can reduce or delete all |
| Own messages (sent) | Until user deletes or TTL expires (default 7 days) | Delete individually or all |
| Received messages | Until user deletes or TTL expires | Delete individually or all |
| Routing table (P values) | 30 days (tied to contact history) | Cleared when contact history cleared |
| Location shared | Session only; cleared when sharing toggled off | Immediate toggle in app |
| Edge server connection logs | 7 days | Not user-controllable (operational logs); aggregate only |
| Crash reports | 30 days at IRIS infrastructure | Not individually deletable (anonymous) |

---

## 6. User Rights Implementation

### 6.1 Right of Access (DPDPA Section 12)

**Engineering implementation:** Settings > Privacy > Export My Data  
Produces a ZIP containing:
- `contact_history.csv` — contact events (anonymized peer Node IDs, timestamps, durations)
- `messages.json` — own sent and received messages (including plaintext of own decrypted messages)
- `routing_table.json` — current PRoPHET P values for known nodes
- `node_id.txt` — own IRIS Node ID (public key)

Export is generated locally; no server involved.

### 6.2 Right to Erasure (DPDPA Section 13)

**Engineering implementation:** Settings > Privacy > Delete All My Data  
Actions taken:
1. Delete all messages from local SQLite store
2. Delete contact history from local SQLite store
3. Clear routing table (P values)
4. Clear location cache
5. Option: regenerate Node ID (new identity; severs all contact history links)

Deletion from relay nodes: IRIS sends a "bundle invalidation" signal for own bundles still in transit, requesting relay nodes delete them. Relay nodes honor this if the invalidation is signed by the original sender's key. This is best-effort — IRIS cannot guarantee deletion from relay nodes that are unreachable.

### 6.3 Right to Correction (DPDPA Section 13)

User can correct: display name (local only; not propagated), own location (stop sharing stale location). There are no correctible data fields at central server (no central server for most data).

### 6.4 Right to Grievance Redressal (DPDPA Section 13(3))

Contact: privacy@iris-comms.in (to be established)  
Response SLA: 30 days  
Escalation: Data Protection Board of India (DPBI) if unresolved

---

## 7. Cross-Border Data Flows

IRIS follows a data localization policy by default:
- All message processing occurs on-device or on India-based edge servers
- Routing tables are local to each device
- No contact history, message content, or location data leaves India except via satellite (which is subject to satellite operator's own data handling — see SATELLITE_REGULATION.md)

If IRIS edge servers are deployed outside India (for diaspora or international expansion), explicit user consent for cross-border transfer is required. This is a v2+ concern.

---

## 8. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
