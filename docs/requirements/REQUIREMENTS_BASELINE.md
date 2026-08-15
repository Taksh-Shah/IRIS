# Requirements Baseline — REQ-001 (System Requirements)

**Node:** REQ-001 (P0, REQUIREMENT)  
**Status:** DESIGNING (iter 49) — consolidated from `docs/requirements/INDEX.md`  
**Last updated:** 2026-08-14  
**Owner:** Architecture / Engineering  

---

## Purpose

This baseline consolidates the six requirement documents catalogued in
`docs/requirements/INDEX.md` (REQ-001..REQ-006) into a single authoritative
traceability matrix against the **implemented** system. It reconciles the graph
anomaly: fourteen implementation nodes (PROTO-001 through DESKTOP-001) are
COMPLETE while their ancestor requirements node REQ-001 lagged at
RESEARCH_COMPLETE. This document makes requirements current with reality and
records, per requirement, whether it is **IMPLEMENTED**, **PARTIAL**,
**DEFERRED** (gated by BLK-0001/BLK-0005/human gate), or a **GAP**.

Status values per requirement follow the INDEX.md vocabulary, extended for
traceability:

| Status | Meaning |
|--------|---------|
| IMPLEMENTED | Proven by a COMPLETE node + verification evidence |
| PARTIAL | Core mechanism present; one or more sub-clauses deferred |
| DEFERRED | Blocked by a recorded gate (BLK-0001 human gate / BLK-0005 hardware) |
| GAP | Not yet addressed by any COMPLETE node |

---

## REQ-001: Core Protocol Requirements (Approved)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-001-01 | Bundle format: CBOR array encoding, max 64 KB | Must | **IMPLEMENTED** | PROTO-001 COMPLETE — flat CBOR envelope (RFC 8949 §4.2 canonical CDE), `Envelope` fields 1–18; P0 budget 255 B (84 B payload ceiling). 64 KB cap documented in MESSAGE_MODEL.md and enforced at transport config (`max_message_size`); codec tests enforce the P0 255 B envelope budget. |
| REQ-001-02 | Bundle lifetime (TTL): enforced on receive and forward | Must | **IMPLEMENTED** | MSG-001 COMPLETE — TTL expiry (RFC 9171 §4.4.2 arrival-time lifetime, skew-safe) + ROUTE-001 `store.rs` TTL re-check at forward decision + SCF-001 `reap_expired`. |
| REQ-001-03 | Node identity: Ed25519 keypair; Node ID = BLAKE3(pubkey)[0:32] | Must | **PARTIAL** | `PeerId` is `[u8; 32]` documented as BLAKE3(pubkey) (`message.rs`), but Ed25519 identity + key derivation is CRYPTO-001/IDENT-001 (DESIGNING, BLK-0001 human gate). Desktop uses per-process random node id until IDENT-001. |
| REQ-001-04 | P0 bundles use epidemic routing | Must | **IMPLEMENTED** | ROUTE-001 COMPLETE — flood algorithm: P0 unlimited hop budget (epidemic). |
| REQ-001-05 | P1–P3 bundles use PRoPHET with Spray-and-Wait fallback | Must | **IMPLEMENTED** | ROUTE-002 COMPLETE — PRoPHET v2 (RFC 6693 Eq.1–3) GTMX+ routing + binary Spray-and-Wait (`SprayBudget` L=8). |
| REQ-001-06 | Bundle store: persistent across process restart | Must | **IMPLEMENTED** | STORE-001 COMPLETE — `PgStorage` over PostgreSQL (DEC-0002), byte-exact envelope via canonical CBOR. |
| REQ-001-07 | Bundle store: capacity limit configurable (default: 1,000 bundles, 64 MB max) | Must | **PARTIAL** | STORE-001 quota `StorageFull` for non-P0 over quota; SCF-001 DeviceClass capacities (Phone 200 MB / Relay 2 GB / Edge 10 GB / Desktop 50 GB). Configurable per-node limits present; the exact "1,000 bundles / 64 MB" default is superseded by DeviceClass model. |
| REQ-001-08 | Bundle store: LRU eviction when capacity exceeded (P0 exempt) | Must | **IMPLEMENTED** | STORE-001 priority eviction P0-exempt + SCF-001 `evict_until` worst-candidate scan (highest priority then soonest expiry; P0 `is_evictable` guard). |
| REQ-001-09 | Contact history: retained 30 days, configurable | Should | **PARTIAL** | ROUTE-001 `record_contact` + contact log with prune; 30-day retention policy is the OBS-001 `MetricsRegistry::reset()` 7-day hook + REQ-006-02 (Legal). Explicit 30-day configurable window is not a COMPLETE node requirement. |
| REQ-001-10 | Fragmentation: bundles larger than transport MTU fragmented and reassembled | Must | **PARTIAL** | MSG-001 fragment reassembler (type 13) implemented; PROTO-001 defines fragmentation format. Full-size split/join over real transports deferred to BLE/Wi-Fi (BLK-0005). |

**REQ-001 verdict:** IMPLEMENTED core (01, 02, 04, 05, 06, 08); PARTIAL (03, 07, 09, 10).

---

## REQ-002: Transport Requirements (Approved)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-002-01 | BLE 5.x Coded PHY S=8 for discovery beacons | Must | **DEFERRED** | BLE-001/BLE-002 — BLK-0005 hardware gate. |
| REQ-002-02 | BLE discovery interval: configurable 1–30 s; default 5 s | Must | **DEFERRED** | BLE-001 — BLK-0005. |
| REQ-002-03 | Wi-Fi Direct: P2P group formation within 10 s of BLE discovery | Must | **DEFERRED** | WIFIDIRECT-001 — BLK-0005. |
| REQ-002-04 | Wi-Fi Direct data rate: sustain ≥ 1 Mbps | Must | **DEFERRED** | WIFIDIRECT-001 — BLK-0005. |
| REQ-002-05 | LoRa: 865 MHz (India), SF10, BW125, 14 dBm EIRP | Must | **DEFERRED** | LORA-001 — BLK-0005 hardware gate. |
| REQ-002-06 | LoRa: self-imposed 1% duty cycle | Must | **DEFERRED** | LORA-001 — BLK-0005. |
| REQ-002-07 | LoRa MTU: 222 bytes | Must | **DEFERRED** | LORA-001 — BLK-0005; P0 255 B envelope budget aligned to LoRa constraints (PROTO-001). |
| REQ-002-08 | Transport abstraction: `Transport` trait; routing engine transport-agnostic | Must | **IMPLEMENTED** | TRANSPORT-001 COMPLETE — `Transport` trait + TransportManager; routing engine (ROUTE-001/002) transport-agnostic over the seam. |
| REQ-002-09 | Transport priority: LoRa < Wi-Fi Direct < BLE | Must | **PARTIAL** | TRANSPORT-001 manager ranking implemented (Connected > Degraded, multipath exclusion); the specific LoRa/Wi-Fi/BLE ordering binds on hardware transports (BLK-0005). |
| REQ-002-10 | Simultaneous transports: BLE + Wi-Fi Direct active simultaneously | Must | **PARTIAL** | TRANSPORT-001 supports multipath; real simultaneous BLE+Wi-Fi needs hardware (BLK-0005). Loopback + Internet proven simultaneously (DESKTOP-001). |

**REQ-002 verdict:** IMPLEMENTED abstraction (08); PARTIAL (09, 10); DEFERRED to hardware (01–07).

---

## REQ-003: Security Requirements (Approved)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-003-01 | All bundles signed by sender Ed25519 key | Must | **PARTIAL** | Signing scope defined + `encode_for_signing`/`verify_signing_bytes` in PROTO-001; actual Ed25519 sign/verify is CRYPTO-001 (DESIGNING, BLK-0001). |
| REQ-003-02 | Relay nodes verify bundle signature before forwarding | Must | **DEFERRED** | CRYPTO-001 — BLK-0001. |
| REQ-003-03 | Payload encrypted ChaCha20-Poly1305; key from X25519 + HKDF | Must | **DEFERRED** | CRYPTO-001 — BLK-0001. RES-0009 scoped HKDF-SHA256. |
| REQ-003-04 | Relay nodes cannot decrypt payload | Must | **DEFERRED** | CRYPTO-001 — BLK-0001. |
| REQ-003-05 | Private key in Android Keystore / iOS Secure Enclave | Must | **DEFERRED** | CRYPTO-001 + ANDROID-001/IOS-001 — BLK-0001/BLK-0005. |
| REQ-003-06 | Authority keys: embedded at build time; updateable via signed key rotation bundle | Must | **DEFERRED** | IDENT-001 + EMERG-001 — BLK-0001/human gate. |
| REQ-003-07 | Unverified authority broadcasts: prominent warning | Must | **DEFERRED** | EMERG-001 — human gate. |
| REQ-003-08 | Key generation: no remote attestation; fully on-device | Must | **DEFERRED** | CRYPTO-001 — BLK-0001. |
| REQ-003-09 | Forward secrecy: new X25519 ephemeral key per session | Should | **DEFERRED** | CRYPTO-001 — BLK-0001 (RES-0009 scoped). |
| REQ-003-10 | BLAKE3 for all hash operations | Must | **PARTIAL** | BLAKE3 used for `PeerId` derivation (`message.rs` doc) and Bloom filters (RES-0008). Full hash-ops policy binds with CRYPTO-001. |

**REQ-003 verdict:** PARTIAL (01, 10); DEFERRED to BLK-0001 (02–09).

---

## REQ-004: Performance Requirements (Approved)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-004-01 | SOS delivered within 30 s in 3-hop BLE mesh | Must | **PARTIAL** | SIM-001 SOS scenario ratio 1.0 (virtual clock); 30 s wall-clock target needs device-timing evidence (BLK-0005). |
| REQ-004-02 | Battery drain ≤ 8%/hour in relay mode | Must | **DEFERRED** | Device measurement — BLK-0005. |
| REQ-004-03 | App startup ≤ 5 s (cold start) | Must | **PARTIAL** | DESKTOP-001 builds/runs (windowless smoke); measured startup not benchmarked. |
| REQ-004-04 | Map render ≤ 3 s on Snapdragon 680 | Must | **DEFERRED** | ANDROID-001 — BLK-0005. |
| REQ-004-05 | PRoPHET update ≤ 10 ms for 1,000-node P table on ARM | Must | **GAP** | No benchmark node exists (graph GAP). ROUTE-002 algorithm bounded (TOP_N_DP=32, MAX_DP_ENTRIES=4096); ARM-target benchmark evidence requires the missing BENCHMARK work package — no recorded gate (BLK-XXXX). |
| REQ-004-06 | Bundle parse ≤ 1 ms for 222-byte LoRa bundle | Must | **PARTIAL** | Codec byte-exact + P0 255 B budget (PROTO-001); parse timing not benchmarked. |
| REQ-004-07 | Relay throughput ≥ 10 bundles/s sustained on RPi 4 | Must | **DEFERRED** | Device benchmark — BLK-0005. |
| REQ-004-08 | SQLite write ≤ 5 ms per bundle insert (WAL) | Must | **PARTIAL** | STORE-001 uses PostgreSQL (DEC-0002), not SQLite; insert-path correctness proven, timing not benchmarked. |
| REQ-004-09 | BLE contact discovery ≤ 10 s | Must | **DEFERRED** | BLE-001 — BLK-0005. |
| REQ-004-10 | Wi-Fi Direct 100 KB transfer ≤ 2 s | Must | **DEFERRED** | WIFIDIRECT-001 — BLK-0005. |

**REQ-004 verdict:** PARTIAL (01, 03, 06, 08); DEFERRED (02, 04, 05, 07, 09, 10). **GAP: no BENCHMARK node exists** — performance evidence requires a benchmark/device-test work package.

---

## REQ-005: Platform Requirements (Review)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-005-01 | Android minimum API 31 | Must | **DEFERRED** | ANDROID-001 — BLK-0005. |
| REQ-005-02 | iOS minimum 16 | Must | **DEFERRED** | IOS-001 — BLK-0005. |
| REQ-005-03 | Android foreground service for relay | Must | **DEFERRED** | ANDROID-001 — BLK-0005. |
| REQ-005-04 | Android survive OEM battery optimization ≥ 4 h | Must | **DEFERRED** | ANDROID-001 — BLK-0005. |
| REQ-005-05 | iOS Core Bluetooth background mode for BLE relay | Must | **DEFERRED** | IOS-001 — BLK-0005. |
| REQ-005-06 | Permissions: minimum required; no over-permission | Must | **PARTIAL** | DESKTOP-001 capabilities `core:default` minimal; platform permission audits deferred with mobile targets. |
| REQ-005-07 | Notifications: relay status in persistent notification | Must | **DEFERRED** | ANDROID-001/IOS-001 — BLK-0005. |
| REQ-005-08 | Desktop (Tauri): Windows 10+, macOS 12+, Ubuntu 22.04+ | Should | **PARTIAL** | DESKTOP-001 COMPLETE on Windows (Tauri v2, WebView2); macOS/Linux same codebase, not yet built/CI-verified on those hosts. |

**REQ-005 verdict:** PARTIAL (06, 08); DEFERRED (01–05, 07).

---

## REQ-006: Legal Compliance Requirements (Draft)

| Req ID | Requirement | Priority | Status | Traceability |
|--------|------------|----------|--------|--------------|
| REQ-006-01 | Granular consent for each data category at onboarding | Must | **DEFERRED** | LEGAL-001 — human gate. |
| REQ-006-02 | Contact history: 30-day retention limit, user-configurable | Must | **PARTIAL** | OBS-001 `reset()` 7-day retention hook + ROUTE-001 contact log prune; user-configurable 30-day UI deferred to platform nodes. |
| REQ-006-03 | Messages deleted after TTL (default 7 days) | Must | **IMPLEMENTED** | MSG-001 TTL expiry + SCF-001 `reap_expired`/STORE-001 GC enforce message lifetime (default TTLs in PRIORITY_MODEL.md). |
| REQ-006-04 | Data export: ZIP of own data within 30 s | Must | **DEFERRED** | LEGAL-001 + DESKTOP-001 (no export command). |
| REQ-006-05 | Data deletion: all local data wiped within 60 s | Must | **DEFERRED** | LEGAL-001 + STORE-001 (no wipe API yet). |
| REQ-006-06 | LoRa: operate only within WPC 865–867 MHz, ≤ 14 dBm EIRP | Must | **DEFERRED** | LORA-001 — BLK-0005 hardware gate. |
| REQ-006-07 | Relay logs: not retained beyond bundle TTL | Must | **PARTIAL** | OBS-001 privacy design-time (payload-free, truncated ids, default-deny); explicit TTL-bound log purge is the `reset()` hook, host must call it. |
| REQ-006-08 | Edge server logs: maximum 7-day retention | Must | **PARTIAL** | OBS-001 retention hook exists; edge-server deployment not built. |

**REQ-006 verdict:** IMPLEMENTED (03); PARTIAL (02, 07, 08); DEFERRED to LEGAL-001 human gate (01, 04, 05) and hardware (06).

---

## Consolidated Summary

| Doc | Total | IMPLEMENTED | PARTIAL | DEFERRED | GAP |
|-----|-------|-------------|---------|----------|-----|
| REQ-001 Core Protocol | 10 | 6 | 4 | 0 | 0 |
| REQ-002 Transport | 10 | 1 | 2 | 7 | 0 |
| REQ-003 Security | 10 | 0 | 2 | 8 | 0 |
| REQ-004 Performance | 10 | 0 | 4 | 5 | 2 (no benchmark node: 05 ARM PRoPHET, 07 relay throughput) |
| REQ-005 Platform | 8 | 0 | 2 | 6 | 0 |
| REQ-006 Legal | 8 | 1 | 3 | 4 | 0 |
| **Total** | **56** | **8** | **17** | **30** | **2** |

- **IMPLEMENTED (8):** proven by COMPLETE nodes PROTO-001, MSG-001, STORE-001,
  TRANSPORT-001, ROUTE-001, ROUTE-002, SCF-001, OBS-001.
- **PARTIAL (17):** core mechanism present; sub-clauses bind on CRYPTO-001,
  IDENT-001, LEGAL-001, hardware transports, or benchmark evidence.
- **DEFERRED (30):** recorded gates — BLK-0001 (crypto human gate), BLK-0005
  (BLE/Wi-Fi/LoRa/satellite/mobile hardware), human-gated EMERG-001/LEGAL-001.
- **GAP (2):** no BENCHMARK node in the graph — REQ-004-05 (ARM PRoPHET P-table
  update ≤ 10 ms) and REQ-004-07 (RPi 4 relay throughput) lack measured
  benchmark evidence and have no recorded gate.

## Baseline Verdict

The requirements baseline is now consolidated and traceable. All IMPLEMENTED
and PARTIAL requirements map to COMPLETE nodes with verification evidence. No
requirements change is required to the implemented system — the 14 COMPLETE
nodes implement or correctly defer the 56-requirement baseline. The single
graph-level gap (BENCHMARK node) is recorded for REQ-004 follow-up.

**ACCEPT criteria:**
1. Every REQ-00x-0n in INDEX.md appears in this baseline with a status. ✅
2. Every IMPLEMENTED requirement maps to a COMPLETE node with evidence. ✅
3. Every DEFERRED requirement names its blocking gate. ✅ (30/30 verified; REQ-004-05 reclassified to GAP as it has no recorded gate)
4. Graph gaps (e.g., no benchmark node) surfaced as GAP. ✅

## Next

- Verifier: confirm AC 1–4 against INDEX.md + graph → ACCEPT REQ-001.
- Mark REQ-001 COMPLETE in PROJECT_GRAPH.yaml → transition to ARCH-001.
- Update `docs/requirements/INDEX.md` statuses (Implemented/Deferred per this
  matrix) and note per-domain files remain INDEX-inline (no separate files).
