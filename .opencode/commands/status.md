# /status — Show Project Health and State

When invoked, report comprehensive project health.

## Steps

1. Read `engineering/memory/CURRENT_STATE.md`
2. Read `engineering/memory/ACTIVE_NODE.md`
3. Read `engineering/memory/BLOCKERS.md`
4. Read `engineering/memory/NEXT_ACTION.md`
5. Run `git status`
6. Validate state consistency:
   - Do all referenced files exist?
   - Do graph node statuses match across files?
   - Is there a next action?
7. Report

## Output Format

```
IRIS ENGINEERING STATUS
=======================
Project: IRIS Resilient Communication Fabric
Phase: [phase]
Active Node: [node ID] — [name]
Node Status: [status]
Next Phase: [target phase]

GRAPH
  Total: [N]
  Complete: [N]
  In Progress: [N]
  Blocked: [N]
  Not Started: [N]

CRITICAL PATH
  [node] → [node] → [node] → ...

BLOCKERS
  [BLK-XXX]: [description] ([severity])

NEXT ACTION
  [immediate next step]

STATE HEALTH
  Durable State: [VALID | NEEDS_UPDATE]
  Graph: [VALID | INVALID]
  Checkpoint: [AGE since last]
  Git: [clean | dirty]

LAST CHECKPOINT
  ID: CHK-XXX
  Timestamp: [time]
  Node: XXXX-XXX
```
