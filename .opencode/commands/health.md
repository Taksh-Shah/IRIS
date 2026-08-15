# /health — Full Engineering Health Check

When invoked, run comprehensive health check across all engineering systems.

## Steps

1. Check GRAPH: validate PROJECT_GRAPH.yaml
2. Check STATE: validate all memory files exist and are consistent
3. Check MEMORY: validate records system integrity
4. Check CHECKPOINT: verify latest checkpoint exists and is recent
5. Check DOCUMENTATION: verify docs/ matches graph
6. Check GIT: verify clean state or document dirty state
7. Check TESTS: verify test infrastructure status
8. Check DEPENDENCIES: verify no circular dependencies
9. Check ACTIVE NODE: verify node state is valid
10. Check BLOCKERS: verify blockers are current
11. Check ACCEPTANCE: verify all complete nodes have evidence
12. Check EVIDENCE: verify evidence exists for claims

## Output Format

```
IRIS ENGINEERING HEALTH
=======================
Timestamp: [time]
Model: [detected model + context limit]

GRAPH: [PASS | FAIL with details]
STATE: [PASS | FAIL with details]
MEMORY: [PASS | FAIL with details]
CHECKPOINT: [PASS | FAIL with details]
DOCUMENTATION: [PASS | FAIL with details]
GIT: [PASS | FAIL with details]
TESTS: [PASS | FAIL with details]
DEPENDENCIES: [PASS | FAIL with details]
ACTIVE NODE: [PASS | FAIL with details]
BLOCKERS: [PASS | FAIL with details]
ACCEPTANCE: [PASS | FAIL with details]
EVIDENCE: [PASS | FAIL with details]

OVERALL: [HEALTHY | NEEDS_ATTENTION | CRITICAL]

RECOMMENDATIONS
  1. [if any]
  2. [if any]
```
