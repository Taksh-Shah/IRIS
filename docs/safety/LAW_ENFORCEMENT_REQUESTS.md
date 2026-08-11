# Law Enforcement Requests Policy

## Core Principle: Engineering First

IRIS is designed with privacy-by-engineering, not privacy-by-promise. The goal is to minimize
what data we hold so that what can be legally compelled is minimal. This is not obstruction of
justice — it is responsible data minimization that protects all users, including those involved
in legitimate emergencies and disaster response.

This policy covers what data IRIS holds, what we can legally be asked to provide, how we
handle such requests, and how we communicate about this with users.

## What IRIS Holds

### On Relay Nodes (Transient)

Relay nodes temporarily store messages for delivery. This is operational necessity for
disruption-tolerant networking.

| Data Type | Stored | Duration | Who Can Access |
|-----------|--------|----------|----------------|
| Message payload | Encrypted (E2EE, we cannot decrypt) | Until TTL expiry | No one but recipient |
| Routing metadata | Yes, plaintext | Until message TTL + 1 day | Relay operator, lawful process |
| Sender NodeId | Yes | Until message TTL + 1 day | Relay operator, lawful process |
| Recipient NodeId | Yes | Until message TTL + 1 day | Relay operator, lawful process |
| Message timestamp | Yes | Until message TTL + 1 day | Relay operator, lawful process |
| Message priority | Yes | Until message TTL + 1 day | Relay operator, lawful process |
| Message size | Yes | Until message TTL + 1 day | Relay operator, lawful process |

Relay nodes do NOT store:
- Message content (encrypted, key not held by relay)
- User's contact list
- User's location history
- User identities linked to real names (NodeId is a cryptographic hash)

### On User Devices

User devices hold their own messages and contact information. IRIS Foundation does NOT
have access to user device data. Device data is only accessible via:
- The user themselves (normal app access)
- Device law enforcement extraction (physical device + device PIN, if device is seized)
- Cloud backup decryption (if user has enabled device backup — outside IRIS's control)

### What IRIS Foundation Holds (Aggregate Only)

IRIS Foundation (the operating entity) holds:
- Aggregate telemetry (message counts, delivery rates, node counts — not attributable to individuals)
- App store registration (email address at registration — if any; some deployments are anonymous)
- Bug reports (submitted voluntarily, may contain log data)
- In-app purchase records (if applicable, via App Store/Play Store — held by Apple/Google)

IRIS Foundation does NOT hold:
- Individual user message history
- Individual user contact lists
- Individual user location history
- Relay node storage (each relay operator holds their own node data)

## What We Can Be Legally Asked to Provide

### Under Indian Law

**Information Technology Act, 2000, Section 69:**
Government may order interception, monitoring, or decryption of information.

IRIS response:
- We cannot decrypt private messages (E2EE — key is not held by us)
- We can provide aggregate routing metadata we hold on our own infrastructure
- Individual relay operators are separately responsible for their node data

**Information Technology Act, Section 79:**
Intermediary liability safe harbor — requires intermediaries to comply with government
direction to remove or disable access to content.

IRIS response:
- We can block a user NodeId at the app layer (prevent them from using IRIS apps we control)
- We cannot block a NodeId at the mesh layer (decentralized, other operators' nodes continue)
- We can push a revocation list for authority certificates we have issued

**Criminal Procedure Code (CrPC) / Bharatiya Nagarik Suraksha Sanhita (BNSS):**
Allows seizure of devices, search and seizure of digital records with court order.

IRIS response:
- Cooperate with lawful search and seizure orders
- Provide whatever data we hold (aggregate telemetry, registration data)
- Cannot compel decryption of E2EE messages — key is on user device, not with us

### Legal Process Requirements

To receive any data from IRIS Foundation:
1. Valid court order or magistrate order under applicable Indian law
2. Specifying the data category requested
3. Specifying the time range
4. Name of the requesting law enforcement agency and case reference
5. For national security: Ministry of Home Affairs or CERT-In authorization

We do NOT respond to:
- Informal requests (email, phone calls) without formal legal order
- Requests from foreign law enforcement without Indian mutual legal assistance treaty (MLAT) compliance
- Requests that are overbroad or lack specificity
- Requests to break encryption (legally impossible to do; technically impossible)

## Assistance to Emergency Response (Lives at Risk)

When lives are immediately at risk, IRIS provides maximum legally available assistance:

**Without court order** (emergency exceptions under BNSS Section 94):
- Relay node geographic range (which area did this NodeId's messages come from?)
- Message timestamps (when did the last message arrive?)
- Whether a specific NodeId is currently active in the mesh

**With court order:**
- Full routing metadata from our infrastructure (subject to TTL/retention limits)
- NodeId → registration email mapping (if we have it)
- Any other data we hold as described above

We do NOT delay response when lives are at risk while waiting for paperwork.
But we document all such disclosures for later review.

## Transparency Reporting

IRIS publishes an annual transparency report including:
- Total number of law enforcement requests received
- Breakdown by request type (court order, emergency, informal)
- Number of requests fully complied with
- Number of requests partially complied with (and why partial)
- Number of requests rejected (and why)
- Number of gag orders received (count only; content not disclosed)

First transparency report: published within 12 months of first production deployment.

## User Notification

When IRIS receives a law enforcement request about a specific user:
- **If no gag order**: we notify the user within 7 days of receiving the request
- **If gag order**: we notify the user when the gag order expires (if it does)
- **Emergency exception**: no notification if notification would endanger a third party
  (e.g., a domestic violence victim's location being sought by abuser posing as law enforcement)

## What We Cannot Do

We want to be explicit about capabilities that do not exist:

1. **We cannot decrypt private messages.** The encryption key exists only on the communicating
   devices. This is not a policy choice we can override — it is a cryptographic fact.

2. **We cannot provide plaintext of any E2EE message.** Including to court order. We can
   explain this to courts and cooperate with device seizure, which may allow device forensics
   to access device-stored messages.

3. **We cannot block a NodeId from the entire decentralized mesh.** We can block it from
   IRIS-operated infrastructure and apps, but independent relay nodes are outside our control.

4. **We cannot identify the real-world identity behind a NodeId.** NodeIds are cryptographic
   hashes. The link to a real person exists only if the person registered with identifying
   information, which is not required.

This is not obstruction of justice. This is privacy by design, operating within the law,
with full cooperation for what we can do.
