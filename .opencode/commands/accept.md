# /accept — Mark Active Node as Complete

When invoked, verify and mark the active graph node as COMPLETE.

## Steps

1. Read `engineering/memory/ACTIVE_NODE.md`
2. Read `engineering/ACCEPTANCE_POLICY.yaml`
3. For each acceptance criterion:
   - Verify evidence exists
   - Verify evidence meets maturity requirements
   - Check for consistency
4. Verify universal acceptance criteria:
   - Requirements satisfied
   - Tests passing (or N/A)
   - Known limitations documented
   - Evidence populated
5. For high-risk nodes: verify human approval exists
6. If all criteria met:
   - Update node status to COMPLETE in PROJECT_GRAPH.yaml
   - Update PROJECT_STATE.yaml
   - Create verification record
   - Update CURRENT_STATE.md, ACTIVE_NODE.md
   - Determine next eligible node
   - Update NEXT_ACTION.md
7. If criteria NOT met:
   - Report what is missing
   - Create remediation steps

## Output Format

```
IRIS NODE ACCEPTANCE
====================
Node: [node ID] — [name]

ACCEPTANCE CHECK
  [PASS/FAIL]: [criterion]
  ...

VERDICT: [COMPLETE | NOT_COMPLETE]

[ACTION]
  Node marked COMPLETE. Next node: [node ID]
  OR
  Missing: [list of missing items]
```
