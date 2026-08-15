# GRAPH-0001: Graph Integrity Audit + Next-Node Selection

**Type**: Graph Audit
**Date**: 2026-08-13
**Node**: PROJECT_GRAPH (meta)
**Author**: Graph_Manager agent
**Status**: ACTIVE

## Summary

Full integrity audit of `engineering/PROJECT_GRAPH.yaml` (32 nodes) against `NODE_SCHEMA.yaml`, `EDGE_SCHEMA.yaml`, `PRIORITY_POLICY.yaml`, and `ACCEPTANCE_POLICY.yaml`. Structural integrity **PASSES** (unique IDs, valid enums, resolvable deps, no cycles). **6 anomalies found**, 4 of them requiring remediation before the graph can be trusted for autonomous selection. Most significant: **MSG-001 is marked COMPLETE with design-only evidence and no implementation code — a premature COMPLETE under acceptance policy.**

## Detail

### 1. Audit Findings Table

| Check | Result | Details |
|-------|--------|---------|
| Node count | ✅ PASS | meta.total_nodes=32; actual node list = 32 |
| ID uniqueness | ✅ PASS | No duplicate IDs |
| Status enum validity | ✅ PASS | All 32 statuses in NODE_SCHEMA.status.values |
| Type enum validity | ✅ PASS | All 32 types in NODE_SCHEMA.type.values |
| Priority enum validity | ✅ PASS | All priorities P0–P3 valid |
| Platform enum validity | ✅ PASS | ANDROID/IOS values valid |
| Dependencies resolve | ✅ PASS | No dependency references a missing node |
| Edges resolve | ✅ PASS | No edge references a missing node |
| Circular dependencies | ✅ PASS | DFS cycle check on node dependencies: none |
| COMPLETE dep satisfiability | ✅ PASS | Every COMPLETE node's deps are COMPLETE or RESEARCH_COMPLETE (design-eligible): PROTO-001/TRANSPORT-001 have dep ARCH-001 (RESEARCH_COMPLETE) |
| COMPLETE → evidence non-empty | ⚠️ FAIL | VISION-001 has **0 evidence entries** |
| COMPLETE → known_limitations/note | ⚠️ FAIL | VISION-001, PROTO-001, STORE-001, TRANSPORT-001, MSG-001 lack `known_limitations` (only INTERNET-001 has it) |
| Every dep ↔ corresponding edge | ⚠️ PARTIAL | 16 deps lack forward-direction edge; 5 deps represented only by `validates`/`tests` edges with no ordering semantics; 1 extra edge (CRYPTO-001→EMERG-001 depends_on) has no dependency backing |
| Acceptance criteria recorded | ⚠️ FAIL | Only STORE-001 has `acceptance_criteria`; schema marks it required for all |
| `created_at`/`updated_at` present | ⚠️ FAIL | All 32 nodes lack timestamps (schema-required) — systemic schema drift |
| Duplicate YAML key | ❌ FAIL | STORE-001 has two `evidence:` blocks; last-wins parsing silently drops 3 evidence entries including STORE_VERIFICATION.md, STORE_SECURITY_REVIEW.md, RES-0004 |
| PROJECT_STATE summary reconciliation | ✅ PASS (counts) | Completed 6, ResearchComplete 3, Designing 2, Discovered 20, Blocked 1 — exact match |
| PROJECT_STATE derived content | ⚠️ DRIFT | `next_recommended_nodes` stale (PROTO-001 COMPLETE, CRYPTO-001 DESIGNING, BLE-001 BLOCKED, SIM-001 ineligible); critical_path lists 10 nodes but claims 9 |

### 2. Status Distribution (reconciled)

| Status | Count | Node IDs |
|--------|-------|----------|
| COMPLETE | 6 | VISION-001, PROTO-001, STORE-001, TRANSPORT-001, MSG-001, INTERNET-001 |
| RESEARCH_COMPLETE | 3 | REQ-001, ARCH-001, LEGAL-001 |
| DESIGNING | 2 | CRYPTO-001, IDENT-001 |
| BLOCKED | 1 | BLE-001 (BLK-0005 hardware) |
| DISCOVERED | 20 | BLE-002, WIFIAWARE-001, WIFIDIRECT-001, LORA-001, SAT-001, DISCO-001, ROUTE-001, ROUTE-002, SCF-001, GW-001, EMERG-001, SEC-001, SIM-001, ANDROID-001, IOS-001, DESKTOP-001, OBS-001, TEST-001, ML-001, PILOT-001 |
| **Total** | **32** | |

### 3. Anomalies List (priority-ordered)

1. **MSG-001 COMPLETE without implementation (MAJOR / policy violation).** Evidence = `MSG_DESIGN.md` (design), `MSG_VERIFICATION.md` (verifies the **design**; verdict is "READY_FOR_ACCEPT" — a design-acceptance report), `RES-0006` (research). No `acceptance_criteria` on the node, no `known_limitations`, no `tests` list, and **no message-engine code exists** in `crates/iris-core/src/` (only `message.rs` type scaffolding: MessagePriority, PeerId, SerializedMessage — no queue, TTL enforcer, dedup, ACK tracker, fragment reassembler). Per `ACCEPTANCE_POLICY`, a COMPONENT requires "All public API documented; Error cases explicitly tested; Component tested in integration with adjacent components" plus universal "100% of tests pass" — **has not occurred**. **Verdict: MSG-001's COMPLETE is a policy violation, not an acceptable design-phase completion.** The graph records no explicit gate note authorizing design-phase COMPLETE. The correct state is `DESIGN_REVIEW` (design done, awaiting review/accept) with implementation queued. This pollutes downstream eligibility (e.g., DESKTOP-001 becomes eligible only because MSG-001 is COMPLETE). **Action recommended (reporting only): demote MSG-001 to DESIGN_REVIEW with known_limitations noting "design accepted; engine implementation pending".**

2. **VISION-001 COMPLETE with empty evidence (MAJOR).** Zero evidence entries; violates universal `evidence_populated` and `requirements_satisfied`. Needs a verification record (charter/vision acceptance sign-off).

3. **STORE-001 duplicate `evidence:` YAML key — silent data loss (MAJOR).** Two `evidence:` blocks (lines 102–108 and 109–111). YAML last-key-wins means parsers see only `[ADR-0004, STORAGE.md]`; the first block (storage security review, verification doc, RES-0004) is dropped. Graph must be de-duplicated to the intended merged 8-entry list. All six COMPLETE nodes plus PROJECT_STATE counts are otherwise consistent.

4. **5/6 COMPLETE nodes missing `known_limitations` (MODERATE).** VISION-001, PROTO-001, STORE-001, TRANSPORT-001, MSG-001. Violates universal `known_limitations_documented` ("non-empty list OR explicit 'No known limitations'"). PROTO-001 additionally carries a PENDING acceptance criterion (`Round-trip property test: PENDING (needs implementation)`) — a COMPLETE node with an explicitly unverified criterion contradicts `requirements_satisfied`.

5. **Edge/dependency correspondence is PARTIAL (MODERATE).** 16 dependencies have no forward-direction edge (e.g., BLE-001→ANDROID-001, MSG-001→DESKTOP-001, ROUTE-001→SEC-001); 5 of them (ROUTE-001→SEC-001, TRANSPORT-001→SIM-001, VISION-001→LEGAL-001, MSG-001→TEST-001, CRYPTO-001→TEST-001) are represented only by `validates`/`tests` edges which carry no ordering semantics. One orphan edge exists: `CRYPTO-001→EMERG-001 depends_on` with no matching dependency (EMERG-001 deps are MSG-001, IDENT-001, ROUTE-001). The graph mixes two direction conventions (`enables` = prerequisite→dependent, `depends_on` = dependent→prerequisite per EDGE_SCHEMA); rule #4 should be enforced with a **canonical direction convention** and a typed `{node, relationship, required_state}` dependency model (already pending in meta.known_gaps).

6. **Systemic schema drift (LOW/MODERATE).** `created_at`/`updated_at` (schema-required) absent on all 32 nodes; `acceptance_criteria` absent on 31/32. `meta.last_validated` (2026-08-11) predates the 2026-08-12 graph changes (BLE-001 BLOCKED, INTERNET-001 COMPLETE). PROJECT_STATE `next_recommended_nodes` list is stale relative to PRIORITY_POLICY (references PROTO-001/CRYPTO-001/BLE-001/SIM-001, none of which are next-eligible). Summary **counts** match exactly; derived **content** drifts.

### 4. Next-Eligible Node Set (per PRIORITY_POLICY)

Filter: status ∈ {DISCOVERED, RESEARCH_COMPLETE} AND all dependencies COMPLETE. Ranked by priority, then critical-path position (`PROJECT_STATE.critical_path`: VISION→REQ→ARCH→PROTO→MSG→ROUTE→SCF→EMERG→ANDROID→PILOT).

| Rank | Node | Priority | Rationale |
|------|------|----------|-----------|
| **1** | **REQ-001** | **P0** | **Recommendation.** Critical path position 2; all deps COMPLETE (VISION-001); RESEARCH_COMPLETE → ready to move to design; finishing it unblocks ARCH-001 (pos 3), keeping the entire critical path in motion. Highest dependency centrality of any eligible node (feeds ARCH-001 → all layers). No human gate required. |
| 2 | BLE-002 | P0 | Dep TRANSPORT-001 COMPLETE. iOS counterpart to blocked BLE-001; unblocks IOS-001 → PILOT-001. Not on the stated critical path, lower centrality than REQ-001. |
| 3 | LEGAL-001 | P1 | Dep VISION-001 COMPLETE. RESEARCH_COMPLETE; **requires_human_approval** (legal gate) — eligible to select, but reaches a human gate, so lower autonomy value. |
| 4 | WIFIAWARE-001 | P1 | Dep TRANSPORT-001 COMPLETE. Unblocks DISCO-001 (currently blocked on it + BLE-001) and ANDROID-001; high centrality. |
| 5 | WIFIDIRECT-001 | P1 | Dep TRANSPORT-001 COMPLETE. Medium centrality (unblocks ANDROID-001 file transfer; not on critical path). |
| 6 | DESKTOP-001 | P1 | Deps TRANSPORT-001 + MSG-001 both marked COMPLETE — **eligibility is contingent on the suspect MSG-001 COMPLETE** (anomaly #1). Demote before touching. |
| 7 | LORA-001 | P2 | Dep TRANSPORT-001 COMPLETE; requires hardware. |
| 8 | SAT-001 | P2 | Dep TRANSPORT-001 COMPLETE; requires hardware + human approval (regulatory). |

Not eligible: ARCH-001 (dep REQ-001 is RESEARCH_COMPLETE, not COMPLETE — eligible only under loose "design-eligible" reading; becomes eligible immediately after REQ-001 advances), DISCO-001 (BLE-001 BLOCKED), ROUTE-001 (DISCO-001 not complete), SIM-001 (ROUTE-001 not complete), SCF/GW/EMERG/SEC/ANDROID/IOS/OBS/TEST/ML/PILOT (non-complete deps).

### 5. Recommendation

**#1 next node: REQ-001 (System Requirements).**

Justification:
- **Strictly eligible** per PRIORITY_POLICY (status RESEARCH_COMPLETE; only dep VISION-001 is COMPLETE).
- **P0 and on the critical path** at position 2 — the earliest path position with an available eligible node (PROTO-001 is done; CRYPTO-001/IDENT-001 are DESIGNING and pinned on human gate BLK-0001).
- **Highest dependency centrality** of all eligible nodes: consumes the charter (VISION-001) and produces the requirements contract that ARCH-001 refines and every implementation layer traces to.
- **No human gate** — unlike LEGAL-001/SAT-001, it can run fully autonomously.
- Finishing REQ-001 immediately unlocks ARCH-001 (pos 3), preserving the phase-2 momentum (CORE_PROTOCOL_IMPLEMENTATION).

**Governance prerequisite (report-only):** Before the supervisor selects any dependent of MSG-001 (currently DESKTOP-001), the graph owner must resolve anomaly #1 — either demote MSG-001 to DESIGN_REVIEW and queue the message-engine implementation as the true P0 critical-path work (in which case MSG-001 implementation would outrank REQ-001 by critical-path position), or record an explicit gated-completion note. The MSG-001 issue does not affect REQ-001/BLE-002 selection.

## Evidence

- `engineering/PROJECT_GRAPH.yaml` — audited source (32 nodes, 59 edges)
- `engineering/NODE_SCHEMA.yaml`, `engineering/EDGE_SCHEMA.yaml`, `engineering/PRIORITY_POLICY.yaml`, `engineering/ACCEPTANCE_POLICY.yaml` — policy/schema basis
- `engineering/PROJECT_STATE.yaml` — derived projection reconciled (counts match; content drift noted)
- `docs/implementation/MSG_DESIGN.md`, `docs/implementation/MSG_VERIFICATION.md` — MSG-001 evidence inspected (design-level only)
- `docs/implementation/INTERNET_TRANSPORT_VERIFICATION.md` — INTERNET-001 evidence (real code + 27/27 tests)
- `crates/iris-core/src/` — code reality check (message engine not implemented)
- `engineering/memory/records/execution-log.md` — iteration log (iter 19: "Mark MSG-001 COMPLETE (design + verification)")
- Automation: `graph_audit.py` (duplicate-ID, enum, dep-resolution, cycle, evidence, edge-correspondence, eligibility checks)

## Related

- Files: `engineering/PROJECT_GRAPH.yaml`, `engineering/PROJECT_STATE.yaml`, `engineering/PROJECT_GRAPH.yaml` (meta), `engineering/memory/records/execution-log.md`
- Decisions: DEC-0008; Research: RES-0004, RES-0005, RES-0006, RES-0007
- Nodes: REQ-001 (recommended), MSG-001 (anomalous COMPLETE), VISION-001, STORE-001, PROTO-001, DESKTOP-001
- Note: record uses `GRAPH-` prefix per task instruction; `records/README.md` type table may be extended with `GRAPH-` (graph audit) entries going forward.