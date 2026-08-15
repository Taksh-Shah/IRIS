# /graph — Validate and Report Graph Health

When invoked, validate the PROJECT_GRAPH.yaml and report health.

## Steps

1. Read `engineering/PROJECT_GRAPH.yaml`
2. Validate:
   - All node IDs are unique
   - All dependencies reference existing nodes
   - All edges reference valid nodes
   - Every dependency has a corresponding edge
   - Required states are valid
   - No circular dependencies
   - Orphan nodes are flagged
   - High-risk nodes have approval requirements
   - Security-critical nodes have security verification
3. Read `engineering/PROJECT_STATE.yaml`
4. Cross-check state matches graph
5. Report

## Output Format

```
IRIS GRAPH VALIDATION
=====================
Version: [version]
Total Nodes: [N]
Validation: [PASS | FAIL with issues]

NODE STATUS BREAKDOWN
  COMPLETE: [N]
  RESEARCH_COMPLETE: [N]
  DESIGNING: [N]
  DISCOVERED: [N]
  BLOCKED: [N]
  FAILED: [N]

DEPENDENCY HEALTH
  Circular Dependencies: [NONE | list]
  Orphan Nodes: [NONE | list]
  Missing Edges: [NONE | list]
  Invalid References: [NONE | list]

CRITICAL PATH
  [node] → [node] → ...

HIGH RISK NODES
  [node]: [reason] ([status])

NEXT ELIGIBLE NODES
  [node]: [why eligible]
```
