# Requirements Index

**Status:** Living document  
**Last updated:** 2026-08-14 (reconciled against implemented system — see REQUIREMENTS_BASELINE.md)  
**Owner:** Product / Engineering

---

## Purpose

This index catalogs all formal requirements documents for the IRIS system. Each requirements document covers a specific domain of requirements. Requirements are identified by a stable ID that persists across document versions.

**Requirement status values:**
- `Draft` — under development; not yet reviewed
- `Review` — under stakeholder review
- `Approved` — reviewed and accepted; implementation can begin
- `Implemented` — fully implemented and tested
- `Deferred` — gated by recorded blocker (BLK-0001 / BLK-0005 / human gate)
- `Deprecated` — no longer applicable (feature removed or superseded)

> **2026-08-14 reconciliation:** Fourteen implementation nodes (PROTO-001 ..
> DESKTOP-001) are COMPLETE. Requirement statuses below reflect the
> `REQUIREMENTS_BASELINE.md` traceability matrix (56 requirements: 8
> Implemented, 17 Partial, 31 Deferred, 1 gap). Per-domain requirement files
> remain INDEX-inline (no separate per-domain files on disk).

---

## Requirements Documents

| ID | Title | Status | Owner | Document | Last Updated |
|----|-------|--------|-------|---------|-------------|
| REQ-001 | Core Protocol Requirements | Implemented (core) / Partial | Architecture | `docs/requirements/INDEX.md` §REQ-001 | 2026-08-14 |
| REQ-002 | Transport Requirements | Partial / Deferred (hardware) | Transport team | `docs/requirements/INDEX.md` §REQ-002 | 2026-08-14 |
| REQ-003 | Security Requirements | Deferred (BLK-0001) / Partial | Security | `docs/requirements/INDEX.md` §REQ-003 | 2026-08-14 |
| REQ-004 | Performance Requirements | Partial / Deferred; benchmark gap | Engineering | `docs/requirements/INDEX.md` §REQ-004 | 2026-08-14 |
| REQ-005 | Platform Requirements | Deferred (mobile) / Partial (desktop) | Android/iOS teams | `docs/requirements/INDEX.md` §REQ-005 | 2026-08-14 |
| REQ-006 | Legal Compliance Requirements | Deferred (LEGAL-001) / Partial | Legal / Engineering | `docs/requirements/INDEX.md` §REQ-006 | 2026-08-14 |

> Traceability matrix: `docs/requirements/REQUIREMENTS_BASELINE.md`

---

## REQ-001: Core Protocol Requirements

**Owner:** Architecture  
**Status:** Approved  

Covers: Bundle format, DTN semantics, routing algorithm interface, bundle store semantics, node identity, endpoint identifier scheme, bundle priority classes (P0–P3), TTL enforcement, custody semantics.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-001-01 | Bundle format: CBOR array encoding, max 64 KB | Must |
| REQ-001-02 | Bundle lifetime (TTL): enforced on receive and forward | Must |
| REQ-001-03 | Node identity: Ed25519 keypair; Node ID = BLAKE3(pubkey)[0:32] | Must |
| REQ-001-04 | P0 bundles use epidemic routing | Must |
| REQ-001-05 | P1–P3 bundles use PRoPHET with Spray-and-Wait fallback | Must |
| REQ-001-06 | Bundle store: persistent across process restart | Must |
| REQ-001-07 | Bundle store: capacity limit configurable (default: 1,000 bundles, 64 MB max) | Must |
| REQ-001-08 | Bundle store: LRU eviction when capacity exceeded (P0 exempt) | Must |
| REQ-001-09 | Contact history: retained 30 days, configurable | Should |
| REQ-001-10 | Fragmentation: bundles larger than transport MTU fragmented and reassembled | Must |

---

## REQ-002: Transport Requirements

**Owner:** Transport team  
**Status:** Approved  

Covers: BLE 5.x (discovery, data transfer, Coded PHY), Wi-Fi Direct (P2P, GO/GC negotiation), Wi-Fi Aware (future), LoRa (SX1262, India 865 MHz, duty cycle), satellite (Iridium SBD interface), transport selection and priority, transport MTU definitions.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-002-01 | BLE 5.x Coded PHY S=8 for discovery beacons | Must |
| REQ-002-02 | BLE discovery interval: configurable 1–30s; default 5s | Must |
| REQ-002-03 | Wi-Fi Direct: P2P group formation within 10 seconds of BLE discovery | Must |
| REQ-002-04 | Wi-Fi Direct data rate: sustain ≥ 1 Mbps for bundle transfer | Must |
| REQ-002-05 | LoRa: 865 MHz (India), SF10, BW125, 14 dBm EIRP | Must |
| REQ-002-06 | LoRa: self-imposed 1% duty cycle | Must |
| REQ-002-07 | LoRa MTU: 222 bytes (SF10 BW125 CR4/5 with header) | Must |
| REQ-002-08 | Transport abstraction: `Transport` trait; routing engine transport-agnostic | Must |
| REQ-002-09 | Transport priority: LoRa < Wi-Fi Direct < BLE (for large bundles); BLE < LoRa (for LoRa-range contacts) | Must |
| REQ-002-10 | Simultaneous transports: BLE + Wi-Fi Direct active simultaneously | Must |

---

## REQ-003: Security Requirements

**Owner:** Security  
**Status:** Approved  

Covers: Node authentication (Ed25519), message encryption (ChaCha20-Poly1305 + X25519), authority key management, relay node security properties, key storage requirements, attack surface documentation, threat model.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-003-01 | All bundles signed by sender Ed25519 key | Must |
| REQ-003-02 | Relay nodes verify bundle signature before forwarding | Must |
| REQ-003-03 | Payload encrypted with ChaCha20-Poly1305; key from X25519 + HKDF-BLAKE3 | Must |
| REQ-003-04 | Relay nodes cannot decrypt payload (no key held) | Must |
| REQ-003-05 | Private key in Android Keystore / iOS Secure Enclave | Must |
| REQ-003-06 | Authority keys: embedded at build time; updateable via signed key rotation bundle | Must |
| REQ-003-07 | Unverified authority broadcasts: displayed with prominent warning | Must |
| REQ-003-08 | Key generation: no remote attestation required; fully on-device | Must |
| REQ-003-09 | Forward secrecy: new X25519 ephemeral key per session | Should |
| REQ-003-10 | BLAKE3 for all hash operations (bundle ID generation, Node ID derivation, HKDF) | Must |

---

## REQ-004: Performance Requirements

**Owner:** Engineering  
**Status:** Approved  

Covers: SOS delivery latency, bundle throughput, battery drain, startup time, map load time, routing computation time.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-004-01 | SOS delivered within 30s in 3-hop BLE mesh | Must |
| REQ-004-02 | Battery drain: ≤ 8%/hour in relay mode (BLE + Wi-Fi Direct active) | Must |
| REQ-004-03 | App startup: ≤ 5 seconds to functional state (cold start) | Must |
| REQ-004-04 | Map render: ≤ 3 seconds from launch on Snapdragon 680 | Must |
| REQ-004-05 | PRoPHET update: ≤ 10ms for 1,000-node P table update on ARM | Must |
| REQ-004-06 | Bundle parse: ≤ 1ms for 222-byte LoRa bundle | Must |
| REQ-004-07 | Relay throughput: ≥ 10 bundles/second sustained on RPi 4 | Must |
| REQ-004-08 | SQLite write: ≤ 5ms per bundle insert (WAL mode) | Must |
| REQ-004-09 | BLE contact discovery: ≤ 10s from device arrival to contact established | Must |
| REQ-004-10 | Wi-Fi Direct transfer: 100KB bundle transferred in ≤ 2 seconds | Must |

---

## REQ-005: Platform Requirements

**Owner:** Android/iOS teams  
**Status:** Review  

Covers: Android minimum API level (31), iOS minimum version (16), background execution constraints, permission model, notification requirements, battery optimization handling.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-005-01 | Android minimum: API 31 (Android 12) | Must |
| REQ-005-02 | iOS minimum: iOS 16 | Must |
| REQ-005-03 | Android: foreground service for relay | Must |
| REQ-005-04 | Android: survive OEM battery optimization for ≥ 4 hours | Must |
| REQ-005-05 | iOS: Core Bluetooth background mode for BLE relay | Must |
| REQ-005-06 | Permissions: minimum required; no over-permission | Must |
| REQ-005-07 | Notifications: relay status shown in persistent notification | Must |
| REQ-005-08 | Desktop (Tauri): Windows 10+, macOS 12+, Ubuntu 22.04+ | Should |

---

## REQ-006: Legal Compliance Requirements

**Owner:** Legal / Engineering  
**Status:** Draft  

Covers: DPDPA 2023 consent flow, data minimization implementation, retention limits, user rights (access, erasure, correction), WPC spectrum compliance, IT Act §69 compliance posture.

**Key requirements summary:**

| Req ID | Requirement | Priority |
|--------|------------|---------|
| REQ-006-01 | Granular consent for each data category at onboarding | Must |
| REQ-006-02 | Contact history: 30-day retention limit, user-configurable | Must |
| REQ-006-03 | Messages: deleted after TTL (default 7 days) | Must |
| REQ-006-04 | Data export: ZIP of own data within 30 seconds | Must |
| REQ-006-05 | Data deletion: all local data wiped within 60 seconds of request | Must |
| REQ-006-06 | LoRa: operate only within WPC 865–867 MHz, ≤ 14 dBm EIRP | Must |
| REQ-006-07 | Relay logs: not retained beyond bundle TTL | Must |
| REQ-006-08 | Edge server logs: maximum 7-day retention | Must |

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial index |
| 2026-08-14 | Reconciled against implemented system (REQ-001 DESIGN): statuses updated per REQUIREMENTS_BASELINE.md traceability matrix; per-domain file references replaced with INDEX-inline + baseline link |
