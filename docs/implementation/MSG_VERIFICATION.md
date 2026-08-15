# MSG-001 Verification Report

**Document ID**: IRIS-MSG-VER-001
**Version**: 1.1
**Node**: MSG-001
**Date**: 2026-08-11 (v1.0 design) / 2026-08-13 (v1.1 implementation evidence)

---

## Acceptance Criteria Verification

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | Message lifecycle management (CREATED→PENDING→IN_TRANSIT→DELIVERED/EXPIRED) | `message_engine/lifecycle.rs` — `transition()` guards legal transitions, terminal states absorbing; tests `happy_path_transitions_in_order`, `illegal_skips_are_rejected`, `terminal_states_are_absorbing`, `failure_and_expiry_reachable_from_transit` | ✅ PASS |
| 2 | Priority queuing (weighted fair queue with P0 preemption) | `message_engine/queue.rs` — QueuedMessage Ord by (priority, seq); tests `p0_always_on_top`, `fifo_within_priority` | ✅ PASS |
| 3 | TTL enforcement (absolute expiry with clock skew budget) | `message_engine/expiry.rs` — `check_ttl_before_insert` at enqueue, `drain_expired` at dequeue, `remaining_lifetime`/RFC 9171 §4.4.2 arrival-time lifetime for skew; tests `ttl_check_at_boundary`, `ttl_rejects_expired_at_insert`, `skew_suspect_clock_does_not_grant_infinite_lifetime`, `within_skew_budget_is_trusted`; integration `outbound_expired_rejected_at_insert` | ✅ PASS |
| 4 | Deduplication (Bloom filter + LRU cache) | `message_engine/dedup.rs` — double-hash Bloom (Kirsch–Mitzenmacher) + LRU; tests `bloom_sizing_matches_design`, `bloom_has_no_false_negatives`, `lru_evicts_oldest`, `lru_rearms_recency`, `dedup_reports_first_win`; integration `dedup_drops_second_arrival` | ✅ PASS |
| 5 | ACK tracking and retry logic | `message_engine/ack.rs` — per-priority retry table with exponential backoff capped at 1 h (RFC 6298-style); tests `retry_table_matches_msg_design`, `backoff_is_exponential_and_capped`, `due_retries_respects_initial_window_and_budget`, `p0_retries_never_exhaust`, `ack_clears_pending`; integration `transit_failure_requeues_and_retries` | ✅ PASS |
| 6 | Fragment reassembly | `message_engine/fragment.rs` — reassembler with 30 s timeout, MAX_FRAGMENTS guard, control/P0 never fragment; tests `reassembles_two_fragment_adu`, `refuses_more_than_max_fragments`, `timeout_drops_partial_sets`, `control_and_p0_never_fragment` | ✅ PASS |
| 7 | Storage interface (persist, load, evict) | `message_engine/storage.rs` — `MessageStorage` trait (persist/load/update_status/delete/evict_expired/evict_by_priority/usage_bytes/get_queue) + `MemoryStorage`; SQLite backend is WP-2 (STORE-001) | ✅ PASS (seam) |
| 8 | Transport interface (send via TransportManager) | `message_engine/mod.rs` — engine sends via `TransportManager`; integration `engine_roundtrip_delivers_to_peer_across_transport`, `relay_path_reaches_next_hop_engine` | ✅ PASS |
| 9 | Thread-safe concurrent access | MessageEngine `Arc`-shared + tokio tasks, multi_thread integration tests pass; lifecycle/queue/ack/dedup/fragment all `Send + Sync` (compiled under tokio multi_thread) | ✅ PASS |
| 10 | Graceful degradation under storage pressure | Queue depth guard: `send_message` returns `MsgEngineError` when `max_queue_depth` reached; storage eviction APIs present (WP-2 wires SQLite quota + P0-never-evicted) | ⏳ PARTIAL (queue-depth guard PASS; storage-pressure eviction lands with WP-2 STORE-001) |

## Implementation Test Evidence (2026-08-13)

- **`cargo test -p iris-core`**: 94/94 pass (message_engine: lifecycle 4, queue 4, expiry 5, ack 5, dedup 5, fragment 4, engine integration 6)
- **`cargo clippy -p iris-core --all-targets`**: 0 warnings
- Integration tests run two engines over `SimulatedTransport` (round-trip + relay path) and verify requeue-on-transit-failure with retry bookkeeping

## Design Integration Check

| Dependency | Integration | Status |
|------------|-------------|--------|
| PROTO-001 (Envelope format) | `Envelope` from protocol crate used throughout; `ContentType::Ack` produced by `build_ack` (test `build_ack_produces_ack_envelope`) | ✅ |
| STORE-001 (Storage) | `MessageStorage` trait mirrors STORE-001 schema ops; SQLite impl pending (WP-2) | ✅ (seam) |
| TRANSPORT-001 (Transport) | Sends/receives via `TransportManager`; SimulatedTransport integration tested | ✅ |
| ROUTE-001 (Routing) | `recipient_peer`/`recipient_matches` decide deliver-or-relay; routing engine pending (WP-4) | ✅ (seam) |
| CRYPTO-001 (Security) | `CryptoProvider` trait + `DevCryptoProvider`; production crypto awaits BLK-0001 gate | ✅ (seam) |

## Known Limitations (recorded in PROJECT_GRAPH.yaml)

- ACID persistence requires WP-2 STORE-001 SQLite backend (in-memory `MemoryStorage` in tests)
- Production crypto awaits BLK-0001 human approval (dev provider only)
- Fragment reassembly not yet exercised end-to-end over a lossy transport in the sim harness
- Retry backoff caps at 1 h — longer than RFC 6298 60 s RTO for higher-priority classes (documented per RES-0010)

## Verdict: IMPLEMENTED — ACCEPT PENDING WP-2/WP-4

All core criteria pass with implementation evidence. Criterion 10 completes with WP-2 (STORE-001); routing/relay integration completes with WP-4 (ROUTE-001).
