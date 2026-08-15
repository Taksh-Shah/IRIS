# IRIS — Autonomous Engineering Bootloader

> **CONVERSATION IS EPHEMERAL. ENGINEERING STATE IS DURABLE.**

You are executing the IRIS Resilient Communication Fabric project.
This is a large, multi-phase engineering effort. Your conversation context
is finite. The project state is stored in durable repository artifacts.

## FIRST ACTION

Before doing anything else:

```
1. Read engineering/memory/execution-state.yaml (authoritative)
2. Read engineering/memory/CURRENT_STATE.md
3. Read engineering/memory/ACTIVE_NODE.md
4. Read engineering/memory/NEXT_ACTION.md
5. Read engineering/memory/CHECKPOINT.md
6. Verify durable state matches repository reality (git status, ls)
7. Begin executing NEXT_ACTION.md
8. After every action: update execution-state.yaml and CONTINUE
```

Do NOT ask "what should I do?" — the answer is in NEXT_ACTION.md.
Do NOT stop after one action — update state and CONTINUE to the next.

## IF CONTEXT WAS COMPACTED

If you suspect context was lost:

```
1. Run /resume command
2. OR manually read the 5 state files above
3. Cross-check with engineering/memory/CHECKPOINT.md
4. Continue from NEXT_ACTION.md
```

## PROJECT ESSENCE

IRIS = multi-transport, disruption-tolerant mesh communication fabric.
Works when cellular/internet/power fail. Built for India, designed for the world.

- **Core docs**: docs/00_PROJECT_CHARTER.md, docs/01_VISION.md
- **Architecture**: docs/architecture/SYSTEM_ARCHITECTURE.md, docs/architecture/LAYER_MODEL.md
- **Engineering graph**: engineering/PROJECT_GRAPH.yaml (source of truth for work structure)
- **Current phase**: See CURRENT_STATE.md
- **Current node**: See ACTIVE_NODE.md

## RULES

1. **Durable state is authoritative** — conversation memory is not
2. **Graph nodes have acceptance criteria** — never mark COMPLETE without evidence
3. **High-risk gates exist** — crypto, identity, emergency, legal require human approval
4. **Checkpoints save progress** — use /checkpoint before major transitions
5. **Research produces evidence** — claims need sources, maturity levels matter
6. **Failed approaches are recorded** — prevent repeating mistakes
7. **The loop is state-driven** — never rely on conversation for loop position

## COMMANDS AVAILABLE

| Command | Purpose |
|---------|---------|
| `/autonomous` | Start autonomous supervisor — runs continuously until gate or completion |
| `/resume` | Reconstruct state and AUTO-CONTINUE execution |
| `/checkpoint` | Save current progress to durable state |
| `/status` | Show project health and state |
| `/graph` | Validate and report graph health |
| `/next` | Show next action with context |
| `/verify` | Run verification on active node |
| `/health` | Full engineering health check |

## AUTONUOUS EXECUTION

This project uses a **supervisor pattern** for continuous execution:

- **LongCat** = reasoning engine (worker)
- **Graph** = planner (what to do next)
- **Durable state** = memory (engineering/memory/)
- **Node loop** = quality process (research → design → implement → test → verify)
- **Supervisor** = executive controller (coordinates everything, never stops early)

The supervisor reads `engineering/memory/execution-state.yaml` after every action and continues to the next eligible work item. It stops ONLY for: mission completion, human gates, unrecoverable blockers, or fatal failures.

## CONTEXT MANAGEMENT

Context management is model/provider-aware. The current IRIS execution environment uses LongCat-2.0 with a 1M-token context window. The system must dynamically determine the effective context limit and remain portable to smaller or larger models.

The Context Builder must dynamically determine the effective model/provider context limit and allocate the working context accordingly. The system must remain functional if the effective context is: 128K, 200K, 500K, 1M, or larger.

No engineering state may depend on the model remembering conversational history.

Be economical with context:

- Load only P0/P1 context automatically (state + active node + next action)
- Load P2 context on demand (research, tests)
- Never load P3/P4 (historical, unrelated)
- Use /next to get targeted context for the active node

## WHEN IN DOUBT

1. Read CURRENT_STATE.md
2. Read NEXT_ACTION.md
3. Run /status
4. Continue from where the durable state says you are

## PERFECT LOOP RULE (CONTINUOUS AUTONOMOUS ENGINEERING) — v1, ratified 2026-08-12

This is the governing operating doctrine. It overrides any tendency to stop early,
single-agent execution, or stale-doc drift.

### The Rule

1. **NEVER STOP.** Run the autonomous loop continuously. Stop ONLY for: mission
   completion, human gates, unrecoverable blockers, or fatal failures.
2. **STATE-DRIVEN, DOCS-FRESH.** After EVERY action: update ALL durable artifacts
   before continuing — `engineering/memory/execution-state.yaml`,
   `CURRENT_STATE.md`, `ACTIVE_NODE.md`, `NEXT_ACTION.md`, `PROJECT_GRAPH.yaml`,
   `PROJECT_STATE.yaml`, `records/execution-log.md`, and any doc touched by the
   work. Re-read docs before implementing from them. Docs are the contract.
3. **FULL AGENT POOL.** The supervisor must orchestrate the specialized agents for
   every node, not act alone: `researcher` (evidence, best practices, newest
   protocols), `orchestrator` (sequencing/planning), `graph-manager` (graph
   integrity + next-node selection), `protocol` (conformance to PROTO-001/ADRs),
   `redteam` (adversarial security review), `security`, `tester`, `verifier`
   (acceptance evidence), `state-manager` (durable-state consistency), plus
   `architect`/`documentation` for major designs. Launch them in parallel where
   independent; consolidate their outputs into durable records before advancing.
4. **BEST-IN-CLASS RESEARCH.** For each node, research the newest standards and
   protocols (RFCs, IEEE, IETF drafts, academic state-of-the-art). Record maturity
   (RES-XXXX) with sources. Do not implement from memory alone.
5. **VERIFY TO COMPLETE.** A node is COMPLETE only when `verifier` evidence exists
   and all acceptance criteria from `ACCEPTANCE_POLICY.yaml` are met or explicitly
   gated in `known_limitations`/blockers. Never fake completeness.
6. **INTEGRATE AS YOU GO.** When a node's output changes interfaces, update
   dependent docs and code in the same loop pass.
7. **CONTEXT-FRESH.** After each action, the supervisor updates the durable context
   files so the next iteration starts with fresh, accurate P0/P1 context.
8. **STRENGTH, NOT SPEED.** Depth before velocity: each node gets research →
   design → implement → test → adversarial review → verification. Skipping depth
   is a violation.
9. **GRAPH + LOOP ENGINEERING DRIVEN.** The engineering graph (`PROJECT_GRAPH.yaml`)
   and the loop-engineering artifacts (`ORCH-0001.md`, `LOOP_POLICY.yaml`,
   `ACCEPTANCE_POLICY.yaml`, `PRIORITY_POLICY.yaml`) are the runtime contract for
   every iteration. Before touching any node: (a) load + validate the graph
   (graph-manager), (b) read the work-package definition for the active node
   (ORCH-0001 / NEXT_ACTION.md), (c) confirm acceptance criteria exist for the
   node before implementing, (d) verify node status/type/priority/dependencies
   against the graph — never work a node whose deps are not COMPLETE. If docs
   conflict with the graph, resolve the conflict into the docs (docs are the
   contract) and re-validate the graph before proceeding. The loop only advances
   node-by-node through the graph with evidence at every vertex.
10. **FRESH-EVERY-ITERATION, RATIFY-EACH-PASS.** After every action, the supervisor
    must re-read the docs it implements from and the state it advances, and write
    its pass into `records/execution-log.md` with files touched. Doc/state/graph/
    context freshness is not an afterthought — it is the loop's heartbeat.

### Loop Mechanics (per iteration)

```
0. Load graph + loop-engineering artifacts (PROJECT_GRAPH.yaml, ORCH-0001,
   LOOP_POLICY.yaml, ACCEPTANCE_POLICY.yaml, PRIORITY_POLICY.yaml) and validate
   against the durable state (graph-manager).
1. Read execution-state.yaml → identify active node + stage
2. Read node docs + acceptance criteria (re-read, don't assume; pull the WP
   definition from ORCH-0001) + read the node's dependencies' outputs
3. Dispatch agents (parallel where possible):
   researcher → RES-XXXX evidence for the node's open questions
   protocol   → conformance check vs PROTO-001/ADRs
   redteam    → adversarial review of new code/design
   graph-manager → next-node selection, graph validation
   verifier   → acceptance evidence for node under test
4. Execute the stage (design/implement/test) in the workspace
5. Consolidate all agent outputs into durable records (engineering/memory/records/)
6. Update ALL state/docs/graph/context (Rule 2) — including PROJECT_GRAPH.yaml
   node status/evidence and PROJECT_STATE.yaml
7. Validate the graph again, then loop → next stage or next node
```

### Violations

- Stopping after one node without being at a gate
- Completing a node without verifier evidence
- Implementing without reading the current docs
- Failing to update durable state after an action
- Using stale research when newer standards exist
- Working a node whose graph dependencies are not COMPLETE
- Advancing the graph without a validated PROJECT_GRAPH.yaml
- Skipping the DOCS-FRESH re-read before every implementation pass
