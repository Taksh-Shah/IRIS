# SCF-001 Verification Report

**Document ID**: IRIS-SCF-VER-001
**Version**: 1.0
**Node**: SCF-001 (WP-5)
**Date**: 2026-08-13

---

## Implementation

`crates/iris-core/src/routing/` — store-carry-forward
(`docs/routing/STORE_CARRY_FORWARD.md`, ORCH-0001 WP-5):

- **`scf.rs`** — `ScfEngine<S: MessageStorage>`: buffer view over the STORE
  seam keyed by `StoreKey { priority_rank, expiry_unix, message_id }` (ordered
  priority ASC then expiry ASC). `DeliveryStatus` lifecycle (Stored → Carrying
  → AwaitingContact → ForwardingInProgress → AckPending → Delivered / Dropped
  {TtlExpired|StorageFull|Evicted}). `buffer_message` TTL-rejects expired;
  `messages_forwardable_to` (contact-addressed, live TTL, under forward-attempt
  budget); `mark_forwarded` (attempts++ → Delivered on ack / AckPending);
  `reap_expired` sweep; `evict_to_fit`/`evict_until` (worst-candidate scan:
  highest priority then soonest expiry, **P0 exempt** — INV-ROUTE-003);
  `ordered_buffered`, `delivery_status`, `status_for`, `priority_rank_of`;
  injectable virtual clock (`with_virtual_clock`) for deterministic sim.
- **`scf_contact.rs`** — `on_new_contact()`: evaluates buffered messages,
  ranks forwardable candidates (priority ASC then delivery-probability DESC),
  honors per-contact `bandwidth_limit` with deferred count;
  `ForwardCandidate::new` clamps probability to [0,1].
- **`scf_eviction.rs`** — `DeviceClass` capacities (Phone 200 MB, Relay 2 GB,
  Edge 10 GB, Desktop 50 GB), `ScfEvictionPolicy::is_evictable` (P0 guard),
  `evict_to_capacity` ceiling trim.

## Acceptance Criteria Verification (ORCH-0001 WP-5)

| # | Criterion | Evidence | Status |
|---|-----------|----------|--------|
| 1 | Buffer ordered by (priority DESC, expiry ASC) | `store_key_ordering_priority_then_expiry`, `forwardable_to_contact_only` (P0 first) | ✅ PASS |
| 2 | DeliveryStatus lifecycle per STORE_CARRY_FORWARD.md | `buffer_and_lifecycle_status` (Stored → Delivered), `relay_marks_relay_node_carry_state` (sim relay state) | ✅ PASS |
| 3 | Contact handler ranks candidates (priority, then delivery probability) | `contact_ranks_p0_first_then_probability` | ✅ PASS |
| 4 | Bandwidth-limited forwarding | `bandwidth_limit_defers_excess` (2 sent, 3 deferred) | ✅ PASS |
| 5 | Ordered eviction P7→P1, then soonest-expiry first | `p7_evicted_before_p1`, `eviction_ordered_lowest_priority_then_soonest_expiry` | ✅ PASS |
| 6 | **P0 never evicted under pressure** | `p0_never_evicted_under_pressure` (forced eviction, P0 survives) | ✅ PASS |
| 7 | Capacity-per-device defaults | `device_class_ceiling_trims_to_class` (Phone 200 MB ceiling enforced) | ✅ PASS |
| 8 | M6 integration: MSG→STORE→ROUTE→SCF (partition→carry→contact→deliver) | `m6_scf_carries_through_partition_and_delivers_on_contact` (Alice sends → decide()=Store → ScfEngine buffers → contact → forward → Delivered; P0 survives pressure) | ✅ PASS |
| 9 | No new clippy warnings; workspace tests green | `cargo test --workspace` 171 green; clippy 0 warnings | ✅ PASS |

## Test Evidence (2026-08-13)

- 13 SCF tests: scf 5, scf_contact 3, scf_eviction 4, M6 integration 1
- Workspace: iris-core 155 (incl. SCF 13 + SIM 8) + sim integration 5 +
  iris-storage 10 + M3 integration 1 = 171 green

## Protocol Conformance

- Buffer/dispatch honors the `MessageStorage` seam (STORE-001) — persistence
  is delegated; the ScfEngine owns ordering/status only.
- P0-never-evicted matches INV-ROUTE-003 and the PSTORE `priority > 0` guard.
- Forward attempts bounded by `ScfConfig::max_forward_attempts` (default 3).

## Known Limitations (recorded in PROJECT_GRAPH.yaml)

- ACK backpropagation + routing-update gossip (payload type 10) land with the
  engine-integration pass (SIM-001).
- SCF delivery probability is a heuristic (0.9 direct / 0.6 relay); refinement
  with delivery statistics is a follow-up.

## Verdict: IMPLEMENTED

All 9 acceptance criteria pass with implementation evidence. SCF-001 advances
to ACCEPT alongside SIM-001 (scenario evidence in SIM_VERIFICATION.md).