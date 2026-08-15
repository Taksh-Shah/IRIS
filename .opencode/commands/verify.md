# /verify — Run Verification on Active Node

When invoked, verify the active node meets its acceptance criteria.

## Steps

1. Read `engineering/memory/ACTIVE_NODE.md`
2. Read `engineering/PROJECT_GRAPH.yaml` for node definition
3. For each acceptance criterion in the node:
   - Check if evidence exists
   - Verify evidence is credible
   - Verify no contradictions
4. Check invariant compliance for node type:
   - Security nodes: threat model coverage
   - Protocol nodes: specification consistency
   - Transport nodes: platform limitation documentation
   - Platform nodes: build/lint/test pass
5. Report

## Output Format

```
IRIS NODE VERIFICATION
======================
Node: [node ID] — [name]
Status: [status]

ACCEPTANCE CRITERIA
  [PASS] [criterion — evidence reference]
  [FAIL] [criterion — what's missing]
  [N/A] [criterion]

INVARIANT COMPLIANCE
  [PASS/FAIL/N/A]: [invariant]

EVIDENCE QUALITY
  [ADEQUATE | INADEQUATE]: [explanation]

VERDICT
  [READY_FOR_COMPLETE | NEEDS_WORK | BLOCKED]
  [specific items needed if not ready]
```
