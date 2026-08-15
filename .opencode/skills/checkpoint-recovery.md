# Checkpoint Recovery Skill

## Purpose

Recover from context loss by reconstructing project state from durable artifacts.

## Recovery Procedure

### Step 1: Read Bootloader
Read `AGENTS.md` — confirms this is an IRIS project and gives first-action instructions.

### Step 2: Read Durable State (in order)
1. `engineering/memory/CURRENT_STATE.md` — project phase, active node
2. `engineering/memory/ACTIVE_NODE.md` — node details, acceptance criteria
3. `engineering/memory/NEXT_ACTION.md` — exact next steps
4. `engineering/memory/BLOCKERS.md` — what's blocking
5. `engineering/memory/CHECKPOINT.md` — latest checkpoint

### Step 3: Verify Repository Reality
1. Run `git status` — what files exist, what's modified
2. Run `git log --oneline -5` — recent activity
3. Check if files referenced in state actually exist
4. Check if graph node statuses match across files

### Step 4: Reconcile
- If durable state matches reality: continue from NEXT_ACTION.md
- If durable state conflicts with reality: trust repository + evidence
- If state files are missing: run `/health` to diagnose
- If state is corrupted: reconstruct from git history + graph

### Step 5: Report
```
RECOVERY COMPLETE
=================
Node: [node ID] — [name]
Status: [status]
Next: [immediate action]
Blockers: [count]
State: [VALID | RECONSTRUCTED]
```

## Failure Modes

| Mode | Detection | Response |
|------|-----------|----------|
| Missing state files | File not found | Reconstruct from graph + git |
| Corrupted state | Invalid markdown/YAML | Reconstruct from graph + git |
| Stale state | Checkpoint >24h old | Verify against repository |
| Graph/state mismatch | Node status differs | Trust graph, update memory |
| Missing next action | NEXT_ACTION.md empty | Derive from active node |
| All files missing | Complete loss | Reconstruct from PROJECT_GRAPH.yaml |

## Priority of Authority

1. **Highest**: Repository files (git tracked)
2. **High**: PROJECT_GRAPH.yaml
3. **Medium**: engineering/memory/ files
4. **Low**: Conversation memory
5. **Never**: Assumptions without evidence
