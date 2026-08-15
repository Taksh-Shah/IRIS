# Architecture Baseline — ARCH-001 (System Architecture)

**Node:** ARCH-001 (P0, ARCHITECTURE)  
**Status:** DESIGNING (iter 49/50) — reconciled `docs/architecture/` against implemented system  
**Last updated:** 2026-08-14  
**Owner:** Architecture / Engineering  

---

## Purpose

This baseline reconciles the documented architecture
(`docs/architecture/SYSTEM_ARCHITECTURE.md`, `LAYER_MODEL.md`,
`ARCHITECTURAL_PRINCIPLES.md`, `REFERENCE_ARCHITECTURE.md`,
`GATEWAY_ARCHITECTURE.md`, `CONTROL_FLOW.md`, `DATA_FLOW.md`,
`FAILURE_ARCHITECTURE.md`, `EDGE_ARCHITECTURE.md`, `NODE_MODEL.md`,
`NETWORK_GRAPH_MODEL.md`, `TEMPORAL_GRAPH.md`, `PLATFORM_CAPABILITY_MODEL.md`)
against the **implemented** system. It continues the graph-anomaly fix begun by
REQ-001: fifteen implementation nodes are COMPLETE while their ancestor
ARCH-001 lagged at RESEARCH_COMPLETE. This document makes the architecture
current with reality, records which documented layers/components map to
implemented modules, and flags deviations.

Status values:

| Status | Meaning |
|--------|---------|
| IMPLEMENTED | Proven by a COMPLETE node / code in `crates/iris-core`, `iris-storage`, `iris-desktop` |
| PARTIAL | Core mechanism present; some sub-components or transports deferred |
| DEFERRED | Gated by BLK-0001 (crypto human gate) / BLK-0005 (hardware) / human-gated node |
| GAP | Documented in architecture but not yet addressed by any COMPLETE node |

---

## 1. Seven-Layer Model vs Implemented System

| Layer | Documented (LAYER_MODEL.md) | Implemented | Status |
|-------|------------------------------|-------------|--------|
| L0 Physical | BLE/Wi-Fi/LoRa/Ethernet/Satellite/Cellular radios | None directly (hardware, OS-managed) | DEFERRED — BLK-0005 hardware |
| L1 Transport Adapter | `TransportAdapter` trait, adapters per radio | `Transport` trait + `TransportManager` (`crates/iris-core/src/transport/`): `simulated.rs` (loopback), `internet.rs` (TCP relay), `ble.rs` (scaffold), `manager.rs` | IMPLEMENTED (trait + manager + loopback/Internet); BLE/Wi-Fi/LoRa/SAT adapters DEFERRED (BLK-0005) |
| L2 Discovery & Neighbor Mgmt | neighbor table, per-transport discovery, capability exchange | `DiscoveryManager` + `NeighborTable` + `handshake.rs` (`crates/iris-core/src/discovery/`): scan loop, LinkRecord per transport, TTL sweep, CAPABILITY bundle (type 11, 256 B budget), BloomExchange | IMPLEMENTED (transport-agnostic core); per-radio discovery DEFERRED (BLK-0005) |
| L3 Transport Manager | transport selection, battery policy, failure detection | `TransportManager` ranking (Connected > Degraded), multipath exclusion, state machine | IMPLEMENTED (core); battery policy modes PARTIAL (hardware-bound) |
| L4 Routing Engine | Direct / Spray-and-Wait / PRoPHET / Epidemic, dedup | `RoutingEngine` (`crates/iris-core/src/routing/`): `direct.rs` (Algorithm 1), `known_path.rs` (Algorithm 2), `flood.rs` (Algorithm 3, P0-unlimited = epidemic), `store.rs` (Algorithm 4), `dedup_cache.rs` (Bloom + LRU), `prophet.rs` + `opportunistic.rs` (L2 PRoPHET v2 GTMX+ + binary SaW, ROUTE-002), `scf*.rs` (store-carry-forward) | IMPLEMENTED |
| L5 Message Engine | priority queue, store-and-forward buffer, TTL, fragmentation | `MessageEngine` (`crates/iris-core/src/message_engine/`): lifecycle, priority queue (P0 preempt), TTL expiry (RFC 9171 arrival-time), ACK tracker (RFC 6298), Bloom+LRU dedup, fragment reassembler, `MessageStorage` seam | IMPLEMENTED (core + MemoryStorage); SQLite → PostgreSQL via `iris-storage` (DEC-0002); fragment split path PARTIAL (BLK-0005) |
| L6 Security | Ed25519 sign/verify, X25519+ChaCha20-Poly1305, key mgmt | Signing scope + `encode_for_signing`/`verify_signing_bytes` (PROTO-001); actual crypto = CRYPTO-001 | PARTIAL / DEFERRED — BLK-0001 human gate |
| L7 Application API | `send_message`/`on_message_received`/SOS/status APIs, platform adapters | `DesktopEngine` (`crates/iris-desktop`): `send_message`/`get_mesh_status`/`get_telemetry`/`subscribe_inbox` over Tauri `State<'_, Arc<DesktopEngine>>` | IMPLEMENTED (desktop); Kotlin/Swift/TS SDKs DEFERRED (ANDROID-001/IOS-001, BLK-0005) |

**Layer verdict:** L1/L2/L4/L5/L7 core IMPLEMENTED; L0, L1-radios, L2-radios
DEFERRED (BLK-0005); L6 PARTIAL/DEFERRED (BLK-0001).

---

## 2. System Architecture Components vs Implemented

| Documented component (SYSTEM_ARCHITECTURE.md §2) | Implemented | Status |
|--------------------------------------------------|-------------|--------|
| TransportManager (BLE/Wi-Fi Direct/Wi-Fi Aware/LoRa/Satellite/Internet) | `transport/manager.rs` + `internet.rs` + `simulated.rs` + `ble.rs` scaffold | IMPLEMENTED (core); BLE/Wi-Fi/LoRa/SAT DEFERRED (BLK-0005) |
| Message Engine (SCF, dedup, TTL, priority) | `message_engine/` + `routing/scf*.rs` | IMPLEMENTED |
| Security Subsystem (Ed25519 identity, Noise, key mgmt) | CRYPTO-001 + IDENT-001 (DESIGNING) | DEFERRED (BLK-0001) |
| Discovery Subsystem (BLE scanning, Wi-Fi peer discovery, LoRa beacon) | `discovery/` transport-agnostic core | IMPLEMENTED (core); radios DEFERRED (BLK-0005) |
| Gateway architecture (GATEWAY_ARCHITECTURE.md) | `gateway/mod.rs` (GatewayType/Capability/Candidate/Selection, quality score, health state machine, diff-based reconcile) | IMPLEMENTED (GW-001 COMPLETE) |
| Edge node architecture (EDGE_ARCHITECTURE.md) | No edge-daemon crate; `DesktopEngine` is nearest in-process host | GAP / PARTIAL (edge deployment not built) |
| Control plane vs data plane (SYSTEM_ARCHITECTURE.md §6) | Telemetry (`OBS-001` observability module) + `TopologyEvent` broadcast | PARTIAL (observability IMPLEMENTED; full control-plane split not a distinct module) |
| Failure architecture (FAILURE_ARCHITECTURE.md) | Layer failure behaviors in LAYER_MODEL.md; per-layer failure handling partially in transport states (Available/Degraded), SCF buffer, gateway health | PARTIAL (core paths exercised in tests; device-mode failures deferred) |
| Node model / capability model / network graph model | `PeerId`, `NeighborTable` capabilities, `CapabilityBundle` | IMPLEMENTED (core); per-platform capability mapping deferred |
| Temporal graph (TEMPORAL_GRAPH.md) | Contact log + `record_contact` (ROUTE-001); SCF virtual clock (SIM-001) | IMPLEMENTED (core structures); full temporal-graph ML analysis deferred (ML-001) |

---

## 3. Message Flow vs Implemented

| Flow (SYSTEM_ARCHITECTURE.md §3) | Implemented evidence | Status |
|----------------------------------|----------------------|--------|
| Normal origination → delivery | MSG-001 `MessageEngine` round-trip + relay path integration tests | IMPLEMENTED |
| Emergency override (P0 SOS) | PROTO-001 P0 255 B envelope + flood P0-unlimited + SIM-001 SOS scenario ratio 1.0 | IMPLEMENTED |
| Store-carry-forward under partition | SCF-001 buffer + SIM-001 partition_carry scenario + M6 integration | IMPLEMENTED |
| Gateway selection / Internet handoff | GW-001 `GatewayManager` + INTERNET-001 relay transport + `TopologyEvent::GatewayChanged` | IMPLEMENTED |

---

## 4. Architectural Principles Compliance (ARCHITECTURAL_PRINCIPLES.md)

The 20 named principles from `ARCHITECTURAL_PRINCIPLES.md` mapped to
implementation evidence:

| # | Principle | Implementation evidence | Status |
|---|-----------|-------------------------|--------|
| 1 | Never Assume Internet Availability | Mesh transports + SCF-001 buffer; offline mesh-first routing | IMPLEMENTED |
| 2 | Never Assume Cellular Availability | Same — Internet treated as optional gateway (INTERNET-001 + GW-001) | IMPLEMENTED |
| 3 | Never Assume Continuous Connectivity | SCF-001 store-carry-forward + SIM partition scenarios | IMPLEMENTED |
| 4 | Never Assume Every Device Supports Every Transport | Transport trait per-adapter; `NeighborTable` capability capture + `CapabilityBundle` | IMPLEMENTED |
| 5 | Never Put LLM in Critical Packet-Forwarding Path | L0-L7 routing is deterministic (ROUTE-001/002); no ML in forwarding | IMPLEMENTED (ML-001 deferred, P3, separate) |
| 6 | Never Make Emergency Communication Depend on ML | P0 flood (epidemic) never routed via PRoPHET/ML | IMPLEMENTED |
| 7 | Never Invent Cryptography Casually | Crypto deferred to CRYPTO-001 + ADR/security-review gate (BLK-0001) | DEFERRED (BLK-0001) |
| 8 | Never Fabricate Benchmarks | REQ-004 targets flagged GAP (no BENCHMARK node); no fabricated numbers | IMPLEMENTED (policy) |
| 9 | Never Claim Theoretical Capability as Production | Desktop v1 known_limitations recorded; hardware transports DEFERRED | IMPLEMENTED (policy) |
| 10 | Never Hide Platform Restrictions | iOS/Android limitations documented (LAYER_MODEL.md); DISCOVERED nodes labeled | IMPLEMENTED (policy) |
| 11 | Never Ignore Legal and Regulatory Constraints | REQ-006 baseline; LEGAL-001 human gate; WPC LoRa limits in REQ-002-05/REQ-006-06 | DEFERRED (LEGAL-001) |
| 12 | Never Sacrifice Safety for Autonomy | EMERG-001 human gate; P0 SOS path deterministic | IMPLEMENTED (policy) |
| 13 | Never Allow Documentation to Drift from Implementation | REQ-001 + ARCH-001 baselines reconcile docs to code (this pass) | IMPLEMENTED |
| 14 | Never Call a Component Production-Ready Without Evidence | Every COMPLETE node carries verification doc + evidence | IMPLEMENTED (policy) |
| 15 | Never Let Autonomous Agents Silently Modify High-Risk Architecture | CRYPTO/IDENT/EMERG require_human_approval in graph | IMPLEMENTED (policy) |
| 16 | Every Discovered Bug Becomes a Regression Test | ROUTE-002 role-swap A/B regression; RED-0001/RT regression tests | IMPLEMENTED |
| 17 | Every Significant Architectural Decision Becomes an ADR | ADR-0011, DEC-0002 (storage pivot) | IMPLEMENTED |
| 18 | Every Major Uncertainty Becomes an Experiment | RES-0010..0013 experiments documented | IMPLEMENTED |
| 19 | Every Experiment Produces Evidence | RES-0001..0013 with maturity/sources | IMPLEMENTED |
| 20 | Every Meaningful Discovery Updates the Project Graph | Graph updated every iteration (validation_status) | IMPLEMENTED |

---

## 5. Deviations / Gaps

1. **No edge-daemon crate** — EDGE_ARCHITECTURE.md describes a Linux systemd
   daemon; nearest implemented host is the desktop in-process `DesktopEngine`.
   Edge deployment = GAP (future EDGE-001-like work package).
2. **No full control-plane / data-plane split** — observability (OBS-001) is
   implemented as a seam; a distinct control-plane module is not separate.
3. **Security layer (L6)** — documented full stack (Ed25519 + Noise + key
   management) deferred to CRYPTO-001/IDENT-001 (BLK-0001 human gate).
4. **Battery policy modes** (FULL/BALANCED/LOW_BATTERY/CRITICAL/OVERRIDE) —
   documented in LAYER_MODEL.md L3; not implemented as a policy module —
   DEFERRED to BLK-0005 (hardware transports that would drive the modes).
5. **Temporal graph** — contact structures implemented; ML-001 (P3) analysis
   deferred.
6. **Fragmentation** — reassembler implemented (MSG-001); active split path
   deferred to real transports (BLK-0005).

---

## 6. Baseline Verdict

The documented architecture is substantively implemented: the seven-layer
model's transport-agnostic core (L1 trait, L2 discovery, L4 routing, L5
message engine, L7 desktop API) maps to COMPLETE nodes, with gateway
architecture (GW-001) and observability (OBS-001) also implemented. Deviations
are either deferred by recorded gates (crypto BLK-0001, hardware BLK-0005) or
documented as gaps (edge daemon, control-plane split, battery-policy module).
No architecture change is required to the implemented system.

**ACCEPT criteria:**
1. Every documented layer (L0-L7) appears with an implementation status. ✅
2. Every IMPLEMENTED layer/component maps to a COMPLETE node or module path. ✅
3. Every DEFERRED item names its blocking gate. ✅
4. Architecture gaps (edge daemon, control-plane split) surfaced. ✅

## Next

- Verifier: confirm AC 1-4 → ACCEPT ARCH-001.
- Mark ARCH-001 COMPLETE in PROJECT_GRAPH.yaml → next-node selection.
