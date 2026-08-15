# DISCO-001 Verification Report

**Document ID**: IRIS-DISCO-VER-001
**Version**: 1.0
**Node**: DISCO-001 (WP-3)
**Date**: 2026-08-13

---

## Implementation

`crates/iris-core/src/discovery/` — transport-agnostic discovery engine:

- **`mod.rs`** — `DiscoveryManager`: registered-`Transport` scan loop
  (`scan_once` drives `Transport::discover_peers()`), dedup-by-peer_id via the
  `NeighborTable`, `TopologyEvent` broadcast channel, `DiscoveryMode`
  (Active/Passive), background `start`/`stop`, reactive `report_peer`,
  first-contact handshake dispatch (`run_handshake` → `try_send`), handshake
  ingestion (`ingest_handshake` → capabilities + dedup Bloom captured on the
  neighbor record).
- **`neighbor_table.rs`** — `NeighborTable`: linked-up/linked-down lifecycle,
  per-transport `LinkRecord`, TTL `sweep`, `transports_to()` for ROUTE-001,
  capability + `peer_bloom` capture.
- **`handshake.rs`** — payload type 11 (CAPABILITY) bundle, canonical-CBOR,
  **≤ 256 B budget enforced**; `BloomExchange` for the dedup-filter handshake
  (DEDUPLICATION.md); `build_handshake`, `parse_handshake`.

## Acceptance Criteria Verification

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | DiscoveryManager starts/stops cleanly, exposes discovery-mode | `mode_round_trips`, `background_loop_scans_periodically`, `start`/`stop` (JoinHandle abort, idempotent) | ✅ PASS |
| 2 | NeighborTable tracks linked-up → linked-down + TTL eviction | `first_seen_emits_peer_discovered`, `last_link_down_emits_peer_lost_and_linked_down`, `ttl_sweep_evicts_idle_neighbors`, `neighbor_lifecycle_up_update_down` | ✅ PASS |
| 3 | Handshake exchange over SimulatedTransport: two engines discover + record capabilities | `handshake_round_trip_records_capabilities_and_bloom` — CAPABILITY + Bloom envelopes cross the SimulatedTransport to the peer's manager | ✅ PASS |
| 4 | Capability Bloom filters exchanged correctly (cross-verified) | `bloom_exchange_round_trip` (insert→exchange→restore→contains), `envelope_carries_bundle_and_bloom`, peer captured bloom verified (`contains([0x11;16])`) | ✅ PASS |
| 5 | No new clippy warnings; workspace tests green | `cargo test --workspace` 121 tests green; `cargo clippy --workspace --all-targets` 0 warnings | ✅ PASS |

## Test Evidence (2026-08-13)

- 16 discovery tests (handshake 4, neighbor_table 6, manager 6)
- Workspace: iris-core 110 + iris-storage 10 + M3 integration 1 = 121 green

## Protocol Conformance (PROTO-001)

- CAPABILITY uses `ContentType::Capability` (payload type 11) in a standard
  `Envelope` (canonical CBOR codec).
- 256 B payload budget from MESSAGE_ENVELOPE.md enforced at encode time
  (`BudgetExceeded`).
- Dedup Bloom exchange aligns with DEDUPLICATION.md (double-hash
  Kirsch–Mitzenmacher, same shapeless params exchanged via `m`/`k`/`bits`).

## Known Limitations (recorded in PROJECT_GRAPH.yaml)

- Full-size dedup Bloom (~180 KB at BLOOM_CAPACITY=100 K) exceeds the
  SimulatedTransport max message size; the exchange assumes the Fragment path
  (payload type 13) for real deployments — transfer logic lands with the
  fragment integration pass.
- Peers discovered via `discover_peers` get default `LinkQuality::Good`; LQI
  refinement lands with transport adapters (SIM-001).
- BLE/Wi-Fi-Aware adapters deferred (BLK-0005, SCF-001) — engine is fully
  transport-agnostic.

## Verdict: IMPLEMENTED

All 5 acceptance criteria pass with implementation evidence. No blockers on the
WP-4 (ROUTE-001) critical path.