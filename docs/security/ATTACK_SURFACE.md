# IRIS Attack Surface Analysis

## Overview

This document enumerates every externally accessible interface of the IRIS system and analyzes the attack surface exposed at each. For each surface:
- **Attack vector**: how an attacker reaches this surface
- **Exposure level**: Low / Medium / High (how accessible to an untargeted attacker)
- **Data at risk**: what an attacker gains by exploiting this surface
- **Mitigations**: controls reducing the attack surface

---

## BLE Advertisement Surface

### Description
IRIS devices periodically broadcast BLE advertisements to announce presence and capabilities to nearby peers.

### What Is Exposed
```
BLE Advertisement Payload (max 31 bytes):
  [0x02, 0x01, 0x06]                  — Flags: LE General Discoverable, BR/EDR not supported
  [0x11, 0x07, <UUID 16 bytes>]       — 128-bit IRIS Service UUID
  [0x03, 0xFF, <cap_byte>, <ver>]     — Manufacturer-specific: capabilities, protocol version
```

No node identity, no user identity, no content in advertisements. Capability byte reveals which transports the device supports (BLE relay, Wi-Fi Direct, LoRa).

### Attack Vector
Any BLE-capable device within radio range (~100m outdoor, ~30m indoor) can receive advertisements without any pairing.

### Exposure Level
**High** — broadcast to all; no authentication required to receive.

### Risks
- **Device tracking**: rotating MAC mitigates but does not eliminate tracking with sophisticated timing correlation
- **Fingerprinting**: capability byte can identify device class and IRIS version
- **Presence detection**: broadcasts reveal device is present and active

### Mitigations
- MAC address rotation: randomized schedule 15–60 minutes (varies per device to prevent synchronized rotation correlation)
- Capability byte does not reveal identity or user information
- Advertisement interval randomized (±20% jitter) to prevent timing fingerprinting
- Advertisement content validated on transmit — implementation cannot accidentally include PII

### Residual Risk
A patient attacker with multiple receivers can correlate pre/post-rotation MAC by timing and signal strength. Full unlinkability would require content-based identifiers, which are expensive.

---

## BLE GATT Service Surface

### Description
After two IRIS nodes discover each other via advertisements, they connect over BLE GATT to exchange messages and routing state.

### What Is Exposed
```
IRIS GATT Service (UUID: iris-mesh-v1):
  Characteristic 0x0001: Message Write (WRITE, WRITE_NO_RESP)
  Characteristic 0x0002: Message Read (READ, NOTIFY)  
  Characteristic 0x0003: Routing State (READ)
  Characteristic 0x0004: Handshake (WRITE, NOTIFY)
```

### Attack Vector
Any BLE-capable device within range can connect to the GATT service without prior pairing.

### Exposure Level
**High** — open to any BLE device.

### What an Attacker Can Send
- Handshake initiation messages
- Message writes (attempted injection)
- Malformed TLV-encoded data

### Risks
- **Parser vulnerability**: malformed input to GATT message handler could trigger crash or logic error
- **State machine abuse**: unexpected message sequences could cause invalid state transitions
- **Resource exhaustion**: rapid GATT connection attempts consume connection slots

### Mitigations
- All GATT input parsed by Rust parser with length validation before any logic
- Fuzz testing of GATT parser (see `SECURITY_TESTING.md`)
- GATT characteristic write limited to `ATT_MTU * 128` bytes maximum
- Connection rate limiting: max 10 GATT connections per minute from single MAC
- State machine rejects out-of-order messages with clean error
- Handshake required before any message exchange — unauthenticated connections cannot inject messages

---

## Wi-Fi Direct Surface

### Description
Wi-Fi Direct peer-to-peer connections used for higher-bandwidth message transfer.

### What Is Exposed
- Wi-Fi Direct peer discovery (peer discovery frames visible to all Wi-Fi devices)
- TCP socket (port 7845) open on Wi-Fi Direct group interface
- WPS exchange during group formation

### Attack Vector
Wi-Fi devices within range (~200m outdoor) can see discovery frames; devices on same Wi-Fi Direct group can reach port 7845.

### Exposure Level
**Medium** — Wi-Fi Direct group formation requires interaction; not fully passive.

### Risks
- **WPS bruteforce**: IRIS uses P2P group formation with push-button or PIN; PIN mode has known WPS vulnerabilities
- **Man-in-the-middle on group formation**: attacker inserts itself into group
- **Port 7845 exposure**: any device on Wi-Fi Direct group can send TCP data

### Mitigations
- Prefer push-button WPS over PIN mode
- WPA3 SAE for group encryption (eliminates offline dictionary attacks)
- IRIS handshake over port 7845 — no message exchange before authenticated session established
- IRIS application-layer authentication (Ed25519 handshake) independent of Wi-Fi layer security
- Message validation at application layer — malformed input handled by Rust parser

### Residual Risk
Wi-Fi Direct group membership may be observable via probe requests. SSID name reveals IRIS usage.

---

## Wi-Fi Aware (NAN) Surface

### Description
Wi-Fi Aware (Neighbor Awareness Networking) used for low-latency peer discovery and data transfer.

### What Is Exposed
- NAN service discovery frames (publicly visible)
- NAN data path (point-to-point, once established)

### Attack Vector
Any Wi-Fi Aware-capable device in vicinity can receive service discovery frames.

### Exposure Level
**Medium** — service-specific; requires Wi-Fi Aware support.

### Risks
- **Service discovery information leakage**: service name reveals IRIS version
- **NAN data path injection**: before authentication established

### Mitigations
- Service name is generic ("iris-mesh") not version-specific (version negotiated in-band)
- Application-layer authentication before any data exchange
- NAN data path encrypted with PMKSA-derived keys

---

## Protocol Parser Surface

### Description
The IRIS protocol parser handles all incoming binary data from any transport. It is the most critical attack surface because it processes untrusted input from strangers.

### What Is Exposed
Every IRIS message format:
- Message envelope (TLV-encoded)
- Route advertisement
- Handshake messages
- Emergency broadcast
- Delivery receipt

### Attack Vector
Any peer that can establish any transport connection can send arbitrary bytes to the parser.

### Exposure Level
**High** — reachable without authentication; all transports feed the parser.

### Risks
- **Buffer overflow**: reading past end of buffer (mitigated by Rust)
- **Integer overflow in length calculations**: crafted length fields cause misparse
- **Infinite loop via malformed nested structures**: cyclic parsing
- **Memory exhaustion via deeply nested structures**: bomb payloads
- **Logic bugs in TLV parsing**: skipping fields incorrectly

### Mitigations

**Rust safety:** Safe Rust prevents buffer overflows and use-after-free in the parser. This eliminates the most common C/C++ parser vulnerabilities.

**Explicit length checks:**
```rust
fn parse_tlv(buf: &[u8]) -> Result<TlvRecord, ParseError> {
    if buf.len() < 2 { return Err(ParseError::TooShort); }
    let tag = buf[0];
    let len = buf[1] as usize;
    if buf.len() < 2 + len { return Err(ParseError::TruncatedValue); }
    // ...
}
```

**Nesting depth limit:** Maximum TLV nesting depth of 8; deeper structures rejected.

**Maximum message size:** Parser rejects any message > 64KB before parsing begins.

**Fuzzing:** Parser is a primary fuzzing target. See `SECURITY_TESTING.md`.

**Known-answer tests:** Parser tested against a corpus of crafted malformed inputs.

### Residual Risk
Logic bugs in semantic validation (not memory safety) remain possible. Fuzzing significantly reduces but cannot eliminate this risk.

---

## Message Handling Surface

### Description
After parsing, messages are processed: signature verified, content decrypted, routing decisions made, storage decisions made.

### What Is Exposed
- Signature verification logic
- Decryption logic
- Routing table update logic
- Storage write logic

### Attack Vector
Authenticated messages from any peer; also partially-authenticated (signature verification in progress).

### Exposure Level
**Medium** — requires message to be parseable.

### Risks
- **Timing side-channel in signature verification**: variable-time comparison leaks information
- **Chosen-ciphertext attack on decryption**: malformed ciphertexts probing decryption oracle
- **State corruption via crafted routing updates**: malicious routing info corrupts local table
- **Storage bomb**: message triggers excessive storage write

### Mitigations
- Constant-time Ed25519 verification (using `ed25519-dalek` crate with constant-time operations)
- AES-256-GCM provides authenticated encryption — decryption rejects invalid authentication tags before any plaintext is processed
- Routing table updates validated for plausibility before application (TTL bounds, distance bounds)
- Storage writes bounded by per-sender quota

---

## Storage Surface

### Description
IRIS reads and writes to an encrypted SQLite database for message store, routing state, and configuration.

### What Is Exposed
- Database query interface (indirect — accessed only by IRIS process)
- Database file on device storage

### Attack Vector
- **Local:** Another app with filesystem access (Android: unlikely in sandbox; iOS: impossible)
- **Physical:** Device theft/seizure

### Exposure Level
**Low** — requires device compromise or physical access.

### Risks
- **SQL injection**: if user-controlled data inserted into SQL query string without parameterization
- **Path traversal**: if filenames are user-controlled
- **Plaintext recovery from encrypted database**: if encryption key is accessible

### Mitigations
- All SQL queries use parameterized statements (no string interpolation)
- Database paths are fixed, not user-controlled
- SQLCipher AES-256-CBC encryption; key stored in hardware security element
- Integrity check on database open — tampered database detected

---

## Emergency System Surface

### Description
The emergency system accepts emergency broadcasts and SOS messages which receive elevated privilege (priority routing, alert delivery).

### What Is Exposed
- Emergency broadcast receiver: accepts and validates incoming emergency broadcasts
- SOS trigger: user-facing trigger that sends P0 priority message
- Emergency channel subscription: allows receiving emergency channel content

### Attack Vector
- Any IRIS node can attempt to inject an emergency broadcast
- Any user can trigger SOS (limited by rate limit)

### Exposure Level
**High** — emergency system must be reachable to work; cannot be gated by pre-authentication.

### Risks
- **Fake emergency broadcast**: inject broadcast that relay nodes accept without EAC verification
- **Emergency system overload**: flood with SOS messages to exhaust P0 queue
- **Certificate verification bypass**: bug in certificate chain verification allows unverified broadcast

### Mitigations
- Emergency broadcast requires valid EAC with verified signature chain
- Certificate verification in Rust with no unsafe code
- Certificate chain validation tested with invalid, expired, and revoked certificates
- SOS rate limit (3 per device per hour, enforced at device key level)
- P0 queue is bounded — oldest SOS evicted if queue fills, with logging

### Residual Risk
A compromised emergency authority certificate enables fake broadcasts until the certificate is revoked. Revocation propagation in an offline network may take hours. This is the highest-severity residual risk in the system.

---

## UI Surface (Android / iOS)

### Description
The application UI receives user input and displays output.

### What Is Exposed
- Text input fields (message composition, contact search)
- URL/deep link handling
- Share extension (receiving shared content from other apps)
- Notification actions
- Widget interactions

### Attack Vector
- Malicious deep links sent via other channels (SMS, email) and opened by user
- Malicious content shared to IRIS via share extension
- Notification action abuse

### Exposure Level
**Medium** — requires user interaction.

### Risks
- **Deep link injection**: crafted link causes unintended app behavior
- **Confused deputy via share extension**: malicious content triggers unexpected behavior
- **XSS in rendered content**: if HTML rendering is used (IRIS uses plain text + markdown, not arbitrary HTML)

### Mitigations
- Deep link allowlist: only recognized IRIS deep link schemas processed
- Share extension validates content type before accepting
- No arbitrary HTML rendering — markdown rendering with allowlisted elements only
- Notification actions are limited to predefined set

---

## IPC Surface (Android Intents / iOS XPC)

### Description
IRIS exposes inter-process communication surfaces for system integration (e.g., tiles, quick settings, widget).

### What Is Exposed
- Android: Intent receivers for system events (boot completed, connectivity changed)
- iOS: App extensions (widget, notification extension)
- Potential: future integration with emergency alert systems

### Attack Vector
- Malicious app on same device sends crafted Intent
- Other process exploits IPC mechanism

### Exposure Level
**Low** — requires code running on same device.

### Risks
- **Intent injection**: malicious app sends Intent to trigger emergency or other action
- **Permission confusion**: IRIS accepts Intent it should not

### Mitigations
- All received Intents validated for expected action, sender package, and data format
- Exported components limited to those required (not-exported by default)
- SOS and emergency functions not accessible via Intent (only via UI)
- Android permissions on broadcast receivers where applicable

---

## Summary Attack Surface Matrix

| Surface | Attacker Type | Exposure | Parser Risk | Crypto Risk | Priority |
|---|---|---|---|---|---|
| BLE Advertisements | Passive (TA-1) | High | None | None | Medium |
| BLE GATT | Active (TA-2) | High | High | Low | High |
| Wi-Fi Direct | Active (TA-2) | Medium | Medium | Low | High |
| Wi-Fi Aware | Active (TA-2) | Medium | Medium | Low | Medium |
| Protocol Parser | All | High | High | N/A | Critical |
| Message Handling | Authenticated | Medium | Low | Medium | High |
| Storage | Physical (TA-8) | Low | None | Medium | High |
| Emergency System | Active (TA-6) | High | Medium | High | Critical |
| UI | User-adjacent | Medium | Low | None | Medium |
| IPC | Local process | Low | Low | None | Low |

---

## Attack Surface Reduction Roadmap

### Short-term (v1.0)
- [ ] Complete fuzz test suite for protocol parser
- [ ] Audit all GATT characteristic handlers
- [ ] Certificate chain validation penetration test

### Medium-term (v1.x)
- [ ] Reduce BLE advertisement information (remove capability byte if possible)
- [ ] Wi-Fi Direct SSID randomization
- [ ] Formal verification of certificate chain validation logic

### Long-term (v2.x)
- [ ] NAN service name rotation
- [ ] Post-quantum key exchange (ML-KEM) in handshake
- [ ] Hardware attestation for relay nodes (reduces TA-3 risk)
