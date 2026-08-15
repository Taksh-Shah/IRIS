# GRAPH-0002 — Graph Integrity Findings & BLK-0002 Expansion Candidates

**Date:** 2026-08-14  
**Source:** graph-manager subagent selection pass (post-ARCH-001, iter 50)  
**Status:** RECORDED — fixes deferred to a graph-maintenance pass

---

## Summary

Post-ARCH-001 next-node selection surfaced graph-integrity issues and
expansion candidates. ML-001 was selected (sole software-only, un-gated,
dependency-eligible node). The issues below do not block execution but should
be resolved in a graph-maintenance pass (BLK-0002).

## Integrity findings

1. **Missing dependency edges** (node `dependencies` without a matching edge):
   - DISCO-001 declares PROTO-001, MSG-001 — no forward edge exists
   - SIM-001 declares TRANSPORT-001 — only reverse `SIM-001→TRANSPORT-001 validates`
   - SEC-001 declares ROUTE-001 — only reverse `SEC-001→ROUTE-001 validates`
   - LEGAL-001 declares VISION-001 — only reverse `LEGAL-001→VISION-001 validates`
2. **Edge-direction convention inconsistent** — early edges are
   dependency→node; later edges are node→dependency (`depends_on`,
   `validates`). Automated edge-based eligibility checks are ambiguous.
   Recommendation: rely on the typed `dependencies` field (as done in
   selection) and/or upgrade to NODE_SCHEMA v0.3 typed relationships.
3. **EMERG-001 omits CRYPTO-001 dep** — edge `CRYPTO-001→EMERG-001
   depends_on` exists but EMERG-001 `dependencies` lists only MSG-001,
   IDENT-001, ROUTE-001. Add CRYPTO-001 to EMERG-001 deps or remove the edge.
4. **Cycle risk pattern** — `SEC-001→ROUTE-001 validates` + SEC-001 dep on
   ROUTE-001 forms a bidirectional pair (incomplete cycle today; graph is
   acyclic under `dependencies`). Same for SIM-001/TRANSPORT-001.
5. **Status/blocker mismatch** — CRYPTO-001/IDENT-001 show DESIGNING but
   BLK-0001 (human approval) means they cannot ACCEPT without operator
   decision; effectively blocked-at-gate. Consider a `blocker: BLK-0001` field.

## Counts reconcile

16 COMPLETE / 2 DESIGNING / 1 RESEARCH_COMPLETE / 1 BLOCKED / 12 DISCOVERED
= 32 total (matches meta.total_nodes). No orphan nodes; no hard dependency
cycles.

## BLK-0002 expansion candidates (add to graph so the loop is not starved)

| Candidate node | Type | Rationale |
|----------------|------|-----------|
| BENCHMARK-001 | TEST/BENCHMARK | REQ-004-05/07 GAP (no measured performance evidence) |
| EDGE-001 | PLATFORM | ARCH-001 GAP (EDGE_ARCHITECTURE.md systemd daemon not built) |
| CONTROL-PLANE-001 | COMPONENT | ARCH-001 GAP (no full control-plane/data-plane split) |
| FED-001 | OPERATIONS | federation/multi-region relay (future ops) |

These nodes would keep the autonomous loop working after ML-001 (a leaf)
completes, while BLK-0001/BLK-0005/LEGAL-001 gates await human decisions.

## Next

- Add BENCHMARK-001 (highest-value: closes REQ-004 GAP) to the graph in a
  graph-maintenance pass.
- Apply the edge/dependency fixes (items 1-3) in the same pass.
