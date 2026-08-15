# OBS-001 Design — Observability System

**Document ID**: IRIS-OBS-DESIGN-001
**Version**: 1.0
**Node**: OBS-001 (WP-9), type OPERATIONS, priority P1
**Dependencies**: MSG-001 (COMPLETE), ROUTE-001 (COMPLETE)
**Date**: 2026-08-14
**Research basis**: RES-0012 (RFC 9171 Bundle Status Report §6.1.1/§6.2, OTel
specs, tracing/tracing-subscriber docs, Meshtastic/ION/µD3TN telemetry,
DPDPA 2023 + DPDP Rules 2025, Microsoft LDP telemetry paper)
**Supersedes/extends**: docs/implementation/OBSERVABILITY.md (pre-implementation
spec — this design refines its `metrics`-crate + SQLite/Prometheus approach per
RES-0012: tracing events + injected AtomicU64 registry, no heavy SDK, no
network exporter in v1)

---

## Scope

Add an **observability layer** to `crates/iris-core` — structured logging,
metrics, and diagnostics emitted from the MSG-001 / ROUTE-001 / SCF-001 /
GW-001 seams, privacy-respecting by design, battery-aware, with no global
state and no OTLP/Prometheus network exporter in v1. Today the engine is
log-free with 6 `AtomicU64` counters (`EngineMetrics`); OBS-001 gives it a
typed event registry, coarse lifecycle spans, and an injected metrics
registry aligned with the existing `SimMetrics` vocabulary.

## Design decisions (RES-0012 headline findings)

1. **`tracing` macros directly, no wrapper.** iris-core gains
   `tracing.workspace = true` (workspace already declares tracing 0.1;
   iris-storage already consumes it). Disabled callsites are near-zero-cost
   (interest cache — event never constructed when no subscriber is
   interested). Centralize event names + attribute keys + privacy helpers
   (`MessageId::short()`, `PeerId::short()`) as typed constants/functions in
   `src/observability/` so taxonomy and privacy live in one place.
2. **OTel mental model, not the SDK.** Borrow the signal taxonomy
   (events / coarse spans / metrics instruments Counter-UpDownCounter-Gauge-
   Histogram) and naming grammar (`iris.<domain>.<name>` with unit suffixes
   per repo style `latency_p50_ms`). No OTel SDK/OTLP in v1 — battery-first,
   offline-first, no network endpoint, no heavy dependency tree.
3. **RFC 9171 status-report vocabulary.** Reuse the Bundle Status Report
   reason-code registry (0 no-info, 1 lifetime expired, 4 depleted storage,
   6 no route, 7 no timely contact) mapping onto IRIS `DropReason` /
   `ForwardingDecision` seams. RFC 9171 §6.2: status-report generation MUST
   be disabled by default — telemetry is opt-in, matching OBSERVABILITY.md
   "production INFO and above".
4. **Privacy is a design-time property.** Default-deny attribute allow-lists,
   payload exclusion by default, 8-byte truncated IDs in all local/exported
   text (`MessageId::short()`/`PeerId::short()`), full IDs only in memory.
   Pseudonymized (hashed) data is still personal data under DPDPA §4(1)(b)
   data minimization + §6(1) privacy-by-design — remote telemetry must be
   opt-in with bounded retention (7-day metric retention per
   OBSERVABILITY.md flush).
5. **Battery/conservative device discipline.** Metrics are lock-free
   `AtomicU64` counters in an injected registry (no global state — same
   pattern as the existing `EngineMetrics`), read on demand at flush time
   (interval or on contact), never per-event I/O. Ring-buffer bounded event
   batch + flush on connectivity. Per-message latency = monotonic deltas
   (`age_ms`/`latency_ms`), never wall-clock deltas (DTN clock skew, RFC 9171
   DTN-time "time unknown").

## Event taxonomy (24 events)

Naming: `<seam>.<verb>`, dotted. Levels per OBSERVABILITY.md table.
All `message_id`/`peer` values are truncated 8-byte prefixes (privacy P2).

### MSG-001 (message_engine) — 9 events

| Event | Level | Key attrs | Emitted at |
|---|---|---|---|
| `msg.created` | INFO | message_id, priority, size_bytes, ttl_secs | `send_message` accepted + persisted |
| `msg.queued` | DEBUG | message_id, priority, queue_depth | enqueue into priority queue |
| `msg.sent` | INFO | message_id, priority, transport | handed to transport `send()` (InTransit) |
| `msg.delivered` | INFO | message_id, priority, hops, latency_ms | inbound delivery to final recipient |
| `msg.acknowledged` | INFO | message_id, rtt_ms, transport | positive ACK received |
| `msg.expired` | WARN | message_id, priority, age_ms | TTL elapsed (engine or queue) |
| `msg.delivery_failed` | WARN | message_id, attempts | attempt budget exhausted |
| `msg.dropped_duplicate` | DEBUG | message_id | dedup first-win drop |
| `msg.fragments_reassembled` | DEBUG | message_id, fragment_count | ADU reassembled |

### ROUTE-001/002 (routing) — 5 events

| Event | Level | Key attrs | Emitted at |
|---|---|---|---|
| `route.decision` | DEBUG | message_id, priority, algorithm, next_hop | `decide()` returned a Forward variant |
| `route.flood` | DEBUG | message_id, recipients, budget_used | Flood hop dispatched |
| `route.stored` | DEBUG | message_id, reason(no_route\|budget_exhausted) | `ForwardingDecision::Store` |
| `route.dropped` | WARN | message_id, reason(RFC code) | `ForwardingDecision::Drop` |
| `route.pruned` | DEBUG | entries, reason(expired\|unreachable) | `prune()`/invalidation |

### SCF-001 (store-carry-forward) — 5 events

| Event | Level | Key attrs | Emitted at |
|---|---|---|---|
| `scf.buffered` | DEBUG | message_id, status | `buffer_message` transition |
| `scf.awaiting_contact` | DEBUG | message_id | no forwardable contact |
| `scf.forward_attempt` | DEBUG | message_id, to, decision, bandwidth_used | contact ranking |
| `scf.evicted` | WARN | message_id, device_class, usage_bytes | eviction (RFC code 4) |
| `scf.reaped_expired` | DEBUG | message_id | TTL re-check (RFC code 1) |

### GW-001 (gateway) — 4 events

| Event | Level | Key attrs | Emitted at |
|---|---|---|---|
| `gw.adopted` | INFO | gateway, type, quality | new candidate adopted |
| `gw.withdrawn` | INFO | gateway, reason | diff-reconcile withdrawal |
| `gw.health_transition` | WARN | gateway, from, to | any `GatewayHealthState` transition |
| `gw.selected` | INFO | gateway, selection, priority | selection resolution + mesh fallback |

### Topology bus (DISCO-001/transport) — 1 event

| Event | Level | Key attrs |
|---|---|---|
| `topo.event` | INFO | kind, peer, transport, quality |

Every event carries `node.id` (truncated) at batch/span level, not per-event.

## Metrics list (RES-0012 §C, aligned to SimMetrics vocabulary)

Grammar `iris.<domain>.<name>` + unit suffix. All counters `AtomicU64` in an
injected `MetricsRegistry` (no global state), read on demand.

`iris.messages.{sent,delivered,relayed,dropped_duplicates,expired,
delivery_failed}_total` (Counters, by priority), `iris.delivery_ratio`
(Gauge, 1h window), `iris.latency.delivery_{p50,p95}_ms` (Gauge w/ bounded
buckets), `iris.routing.{decisions,floods}_total` (Counter, by algorithm),
`iris.scf.evictions_total` (Counter), `iris.storage.usage_bytes` (Gauge),
`iris.queue.depth` (Gauge, by priority), `iris.hops.{max_seen,avg}` (Gauge),
`iris.overhead_ratio` (Gauge), `iris.gateway.{quality,health}` (Gauge, by
gateway).

## Privacy rules (hard seam constraints)

- P1: payload bytes never in any event/log/metric.
- P2: identifiers in local/exported text are truncated 8-byte prefixes.
- P3: default-deny attribute allow-list (fields come from the registry, not
  free-form).
- P4: telemetry opt-in; production default INFO, TRACE never enabled.
- P5: bounded retention (7-day metric window per OBSERVABILITY.md).
- P6: no per-event metric writes on the hot path (metrics read at flush).

## Module layout (IMPLEMENT target)

```
src/observability/mod.rs        — MetricsRegistry (injected AtomicU64),
                                  MetricsSnapshot, event-name constants,
                                  short-id helpers, resource attrs
src/lib.rs                       — pub mod observability; exports
crates/iris-core/Cargo.toml     — tracing.workspace = true
seams: message_engine/mod.rs, routing/{mod,scf,opportunistic}.rs,
       gateway/mod.rs           — emit events + increment counters
```

`MetricsRegistry` is injected (like `EngineMetrics`) into
`MessageEngine`/`ScfEngine`/`RoutingEngine`/`GatewayManager` — no global
`static`.

## Acceptance criteria (AC 1-6)

1. Structured telemetry events (key-value, typed, time-stamped) emitted from
   MSG-001 + ROUTE-001 seams.
2. Diagnostic log stream with severity levels + message-id correlation
   (truncated).
3. Metrics counters/gauges (delivered, relayed, evicted, storage used, hops)
   without global state.
4. Privacy-respecting: no payload/PII in telemetry by default (P1-P3).
5. Bounded overhead: no per-message allocation hot path; metrics lock-free
   AtomicU64 read at flush; no network exporter in v1.
6. Tests cover 1-5.

## Failure-mode analysis

| Failure | Handling |
|---|---|
| No subscriber initialized | tracing disabled callsites near-zero cost; no panic |
| Privacy leak in a new event | event registry + short-id helpers centralize allowed fields; red-team review of event attrs |
| Counter overflow | AtomicU64 (2^64); reset on flush |
| Telemetry hot-path allocation | rule P6; registry is AtomicU64 increments, no Vec/alloc |
| Missing subscriber filter | EnvFilter default `iris_core=info` per OBSERVABILITY.md |

## Out of scope (deferred)

- OTLP/Prometheus exporter, network telemetry export (v1 = local only).
- Wire trace-context propagation — message_id IS the correlation key.
- Adaptive sampling, full histograms with percentile aggregation.
- `iris-ctl` binary + health endpoint (follow-up nodes).
