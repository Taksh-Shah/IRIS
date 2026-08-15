# GW-001 Design — Gateway Discovery & Selection

**Document ID**: IRIS-GW-DESIGN-001
**Version**: 1.0
**Node**: GW-001 (WP-7)
**Date**: 2026-08-13
**Contract docs**: `docs/routing/GATEWAY_SELECTION.md`,
`docs/architecture/GATEWAY_ARCHITECTURE.md`
**Dependencies (COMPLETE)**: DISCO-001 (neighbor capability tags + topology),
INTERNET-001 (self-uplink detection via transport state/cost)

---

## Scope

A **gateway** is an IRIS node with an uplink to a network not shared by all
peers (Internet/relay, LoRa backbone, satellite). GW-001 delivers:

1. **Detect** gateway capability — both *self* (own Internet transport
   Connected) and *peers* (DISCO-001 capability bundle tags).
2. **Track** gateway candidates — registry of known gateways with quality
   metrics, hop distance, and health.
3. **Select** the best gateway for a message deterministically, honoring the
   priority-strategy matrix from GATEWAY_SELECTION.md.
4. **Handle failure** — ACK-timeout health monitor, probation,
   failover/re-selection, mesh-only fallback (never stall).

## Out of Scope (v1)

- Live radio adapters for LoRa/satellite (BLK-0005; capability structs model
  their metrics, wired later).
- Cloud relay bridge service (gateway node process) — selection engine only.
- Multicast/gossip propagation beyond DISCO-001 handshake tags
  (`GatewayAnnouncement`/hop>1 adversarial rebroadcast is a future extension).

## Design

### 1. Capability model

```rust
pub enum GatewayType { Internet, LoRa, Satellite, UsbEthernet }

pub struct GatewayCapability {
    pub gateway_type: GatewayType,
    pub transport_id: String,
    pub bandwidth_bps: u64,
    pub latency_ms: u32,
    pub reliability_score: f32, // 0.0–1.0 (ACK-rate history)
    pub cost_factor: f32,       // 0.0 free – 1.0 max
    pub current_load: f32,      // 0.0–1.0
    pub queue_depth: u32,
    pub duty_cycle_remaining: f32, // LoRa-relevant, else 1.0
    pub max_priority: MessagePriority, // worst-served priority (all above ok)
}
```

`serves_priority(p)` = `p.as_u8() <= max_priority.as_u8()` (P0 numeric 0).
Defaults per type (GATEWAY_SELECTION.md): Internet/USB → all (P7);
LoRa → ≤P3; Satellite → ≤P2.

### 2. Quality score (pure, deterministic)

```
q = 0.30*reliability
  + 0.20*min(1, bandwidth/MAX)^0.5
  + 0.15*(1 − min(1, latency/60_000))
  + 0.20*(1 − cost_factor)
  + 0.10*(1 − load)*(1 − queue/100)^+
  + 0.05*duty_cycle  (LoRa else 1.0)
q *= serves_priority ? 1 : 0
```

### 3. Selection matrix (GATEWAY_SELECTION.md)

| Priority | Strategy |
|----------|----------|
| P0 | `All` — every eligible gateway simultaneously (multi-path, first-wins dedup) |
| P1–P2 | `WithBackup` — best + second-best |
| P3–P7 | `Single` — best gateway |
| none | `None` — mesh-only fallback, message stays in mesh/SCF, never dropped |

Eligibility = serves-priority AND not-failed AND probation-score > 0.

### 4. Health monitor

- `FAILURE_THRESHOLD = 3` consecutive ACK timeouts → `Failed` set.
- `record_success` resets counter, removes from Failed, records last success.
- `handle_advertisement` on a Failed gateway → **probation** (score factor 0.5)
  for `RECOVERY_WINDOW` (default 300 s); a successful ACK restores full trust.
- `is_failed`/`score_factor` gate the selection eligibility.

### 5. Registry & integration

`GatewayManager`:
- `set_self_uplink(GatewayCapability)` — called when INTERNET-001 reports
  Connected/Degraded (INTERNET.md §Mesh Fallback).
- `reconcile(&NeighborTable)` — scans DISCO-001 neighbors' `CapabilityBundle`
  for `gateway`/`gateway:internet|lora|satellite` tags and adopts candidates
  (no changes to DISCO-001 code — read-only consumer).
- `select(priority) -> GatewaySelection` — deterministic, sorted-by-score.
- Failure API: `record_ack_timeout`, `record_success`,
  `handle_advertisement`, `adopt_from_neighbor`.
- Emits `TopologyEvent::GatewayChanged` (existing seam) on adopt/failure.

## Acceptance Criteria

| # | Criterion | Test evidence |
|---|-----------|---------------|
| AC-1 | Self gateway capability derives from an Internet uplink (INTERNET-001 Connected); `is_gateway()` true | `self_uplink_connected_marks_gateway` |
| AC-2 | Peer gateway detected from DISCO-001 capability tags; candidate registry populated with quality metrics | `reconcile_adopts_gateway_tagged_neighbor`, M7 handshake |
| AC-3 | `serves_priority` enforced per type (LoRa ≤P3, Satellite ≤P2, Internet all) | `priority_compatibility_matrix` |
| AC-4 | Deterministic scoring; higher reliability/bandwidth + lower latency/cost wins; priority-incompatible scores 0 | `quality_score_ranks_expected_order` |
| AC-5 | Selection matrix: P0 → All, P1–P2 → WithBackup, P3+ → Single, none → None (mesh fallback) | `selection_matrix_single_backup_all`, `no_candidates_returns_none` |
| AC-6 | 3 consecutive timeouts mark failed; success recovers; advertisement→probation 0.5; excluded from selection | `health_thresholds_and_recovery` |
| AC-7 | Failover: primary failure re-selection picks backup; mesh-only fallback never stalls traffic | `failover_picks_backup_after_timeouts`, M7 |
| AC-8 | Workspace green + clippy 0 warnings | `cargo test --workspace`, `cargo clippy` |

## Known Limitations (record in PROJECT_GRAPH.yaml after pass)

- Reliability/load are modeled inputs — live refinement needs transport
  ACK/probe telemetry (AUTH-001/SEC-001 integration).
- Multi-hop gateway propagation (hop_count>0, gossip) deferred; v1 selects
  from direct-neighbor advertisements.