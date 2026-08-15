# /next — Show Next Action with Context

When invoked, display the next action with all relevant context for the active node.

## Steps

1. Read `engineering/memory/NEXT_ACTION.md`
2. Read `engineering/memory/ACTIVE_NODE.md`
3. Read `engineering/memory/CURRENT_STATE.md`
4. Load relevant context based on priority:
   - P0: Current state, active node, next action, hard constraints, acceptance criteria
   - P1: Relevant architecture, ADRs, decisions, invariants
   - P2: Relevant research, experiments, tests
5. Estimate context budget for the work ahead (adaptive to model context limit)
6. Report

## Output Format

```
IRIS NEXT ACTION
================
Node: [node ID] — [name]
Status: [status]

IMMEDIATE STEPS
  1. [step]
  2. [step]
  3. [step]

CONTEXT TO LOAD
  P0 (immediate):
    - [file path]
    - [file path]
  P1 (for this work):
    - [file path]
    - [file path]
  P2 (if needed):
    - [file path]

ACCEPTANCE CRITERIA
  - [criterion 1]
  - [criterion 2]

BLOCKERS
  [None | blocker list]

CONTEXT BUDGET ESTIMATE (adaptive)
  Model limit: [detected from environment]
  State files: ~3K
  Active node: ~4K
  Dependencies: ~8K
  Documentation: ~25K
  Total: ~40K of [limit] ([%] used)
```
