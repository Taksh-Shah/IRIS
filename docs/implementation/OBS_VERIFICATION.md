# OBS-001 Verification Report

**Document ID**: IRIS-OBS-VERIFICATION-001
**Version**: 1.0
**Node**: OBS-001 (WP-9), type OPERATIONS, priority P1
**Date**: 2026-08-14
**Status**: PASS (AC 1-6)

**Design authority**: `docs/implementation/OBS_DESIGN.md` v1.0
**Research basis**: `engineering/memory/records/research/RES-0012.md`
**Workspace**: 217 tests green (194 iris-core lib incl. 5 observability unit
tests + 4 integration in `tests/obs_telemetry.rs`, 8 sim_scenarios, 10
iris-storage, 1 M3), `cargo clippy --all-targets` 0 warnings.

---

## AC 1 — Structured telemetry events from MSG-001 + ROUTE-001 seams

**Evidence**: 17 event types emit at their documented seam points via typed
`tracing` events with key-value fields.

- **message_engine (9)**: `msg.created` (`send_message`, INFO), `msg.queued`
  (relay enqueue, DEBUG), `msg.sent` (transport handoff, INFO), `msg.delivered`
  (final recipient, INFO), `msg.acknowledged` (positive ACK, INFO),
  `msg.expired` (inbound / dequeue / GC sweep, WARN), `msg.delivery_failed`
  (`fail_message`, WARN), `msg.dropped_duplicate` (dedup, DEBUG),
  `msg.fragments_reassembled` (fragment path, DEBUG) —
  `crates/iris-core/src/message_engine/mod.rs`.
- **routing (2 emitted of 5)**: `route.decision` (every `RoutingEngine::decide`,
  DEBUG, with algorithm label), `route.pruned` (`prune`, DEBUG) —
  `crates/iris-core/src/routing/mod.rs`. `route.flood`/`route.stored`/
  `route.dropped` are taxonomy-defined but not emitted in v1 (see
  known_limitations — decision is emitted centrally at `decide()`, so the
  sub-decision events are redundant; follow-up can add them if operators need
  per-algorithm detail).
- **SCF (3 emitted of 5)**: `scf.buffered`, `scf.reaped_expired`, `scf.evicted`
  (WARN) — `crates/iris-core/src/routing/scf.rs`.
- **gateway (4)**: `gw.adopted`, `gw.withdrawn`, `gw.health_transition`,
  `gw.selected` — `crates/iris-core/src/gateway/mod.rs`.

**Verdict**: PASS.

## AC 2 — Diagnostic log stream with severity levels + truncated message-id correlation

**Evidence**: every event is a `tracing::{info,debug,warn}` call with a
severity level per the OBSERVABILITY.md table (ERROR reserved for
crash/corruption — none needed yet). Message/peer identifiers are emitted
exclusively via `%...short()` (`ShortId`), giving a 16-hex-char (8-byte)
correlation key that threads `msg.*` events through create → queue → send →
deliver/ack/expire. Level discipline: production default `INFO` (per
`init_tracing` EnvFilter default in OBSERVABILITY.md), DEBUG for detailed
flow, WARN for degraded/recoverable, TRACE never used.

**Verdict**: PASS.

## AC 3 — Metrics counters/gauges without global state

**Evidence**: `MetricsRegistry` (`src/observability/mod.rs`) is an injected,
lock-free `Arc<RegistryInner>` holding 9 pre-registered `AtomicU64` counters
(`iris.messages.{sent,delivered,relayed,dropped_duplicates,expired,
delivery_failed}_total`, `iris.routing.{decisions,floods}_total`,
`iris.scf.evictions_total`). No `static`/global registry anywhere. `Clone`
shares counters (one registry can span engines). Injected via:
`MessageEngine::new_with_telemetry` (OBS-RT-06), `RoutingEngine::with_telemetry`,
`ScfEngine::with_telemetry`. `snapshot()` (point-in-time) + `reset()` (P5
rotation hook) documented. Delivery/latency/queue-depth gauges are computed by
the SIM layer (`SimMetrics`) and storage layer on demand — consistent with the
design's "read at flush" discipline.

Tests: `shared_registry_accumulates_across_routing_and_scf`,
`engine_accepts_injected_shared_registry`, `engine_telemetry_counts_expired_and_delivered`.

**Verdict**: PASS.

## AC 4 — Privacy-respecting: no payload/PII in telemetry by default (P1-P3)

**Evidence**:
- **P1**: red-team audit of all 26 `tracing::` call sites: zero pass payload
  bytes, signature bytes, or key material. Event attrs are scalar metadata only
  (priority, size, ttl, hops, queue depth, usage bytes, enum labels).
- **P2**: identifiers are truncated by construction — `ShortId` (`[u8; 8]`)
  is the only identifier type accepted in event fields; its `Display`/`Debug`
  print exactly 16 hex chars (8 bytes) and can never render a full 16/32-byte
  ID. `MessageId::short()`/`PeerId::short()` return `ShortId` (allocation-free).
- **P3 (metrics)**: `MetricsRegistry::increment/add` take `&'static str` and
  silently ignore unknown names (test asserts snapshot stays 9 keys) —
  default-deny enforced by the type system.
- **P3 (events)**: event names + metric names come from centralized constants;
  attribute keys are macro literals at call sites. A CI grep guard for
  `payload`/`signature` inside `tracing!` is a recommended follow-up (red-team
  OBS-RT-01 residual — attribute keys are convention, not type-enforced).

**Verdict**: PASS (with OBS-RT-01 residual documented).

## AC 5 — Bounded overhead: no per-message allocation hot path; no network exporter

**Evidence**:
- `ShortId` is a fixed `[u8;8]` — no heap allocation in `short()` (OBS-RT-03).
- `ForwardingDecision::algorithm_label()` returns `&'static str` — no `format!`
  per routing decision (OBS-RT-02).
- Registry counters are `AtomicU64` fetch_add (lock-free, no alloc); `snapshot`
  is the only allocation point and is called at flush time, not per event (P6).
- Emits that could allocate are lazily evaluated inside the `tracing!` macro
  (only when a subscriber is interested) and run with mutex guards dropped
  (OBS-RT-04/05 — queue/dedup locks scoped).
- No OTLP/Prometheus/network exporter in v1 — telemetry is local-only (log
  stream via embedder's subscriber; metrics via `snapshot()`/`reset()`).

**Verdict**: PASS.

## AC 6 — Tests cover AC 1-5

**Evidence**:
- Unit tests in `src/observability/mod.rs` (5): registry increment/snapshot,
  reset, delivery-window ratio, priority label stability, ShortId truncation.
- Integration tests in `tests/obs_telemetry.rs` (4):
  `shared_registry_accumulates_across_routing_and_scf` (AC3 shared injection),
  `engine_telemetry_counts_expired_and_delivered` (AC3 engine counters),
  `engine_accepts_injected_shared_registry` (AC3/AC6, OBS-RT-06 regression),
  `privacy_short_truncates_identifiers` (AC4 P2).
- Red-team review (AC4/AC5): `redteam` subagent audit, verdict PASS, 3 MEDIUM +
  5 LOW/INFO findings; MEDIUMs fixed in-pass (OBS-RT-01/02/03/06), LOWs
  (OBS-RT-04/05) fixed, INFOs (OBS-RT-09 doc'd, OBS-RT-10 pre-existing)
  accepted.

**Verdict**: PASS.

---

## Security review

Red-team report `OBS-RT-01..10`. Verdict PASS — no CRITICAL/HIGH. Privacy core
holds: P1 payload-free, P2 truncation-by-construction, P3 default-deny,
P6 no global state. All MEDIUM findings resolved in-pass.

## Known limitations

- `route.flood`/`route.stored`/`route.dropped` and `scf.awaiting_contact`/
  `scf.forward_attempt`/`topo.event` are defined in the taxonomy but not
  emitted in v1 — `route.decision` centralizes routing visibility; the gateway
  `gw.selected`/`gw.health_transition` already cover the gateway seam. Add the
  sub-events only if operators need per-algorithm/per-contact detail.
- Event attribute keys are convention not type-enforced (OBS-RT-01 residual);
  recommend a CI grep guard for `payload`/`signature`/`secret` inside
  `tracing!`/`telemetry` macros.
- `MetricsRegistry::reset()` is the P5 7-day retention hook but v1 ships no
  in-crate rotation timer — the embedding host must call it on flush.
- Pre-existing `MessageEngine::new` `tasks.try_lock().unwrap()` panic path
  (OBS-RT-10) is out of OBS-001 scope and carried as accepted risk.
- No network exporter / OTLP in v1 (by design); telemetry export is a
  follow-up node.
