# Graph Validator Skill

## Purpose

Validate the PROJECT_GRAPH.yaml for structural integrity and consistency.

## Validation Rules

### Structural
1. All node IDs are unique
2. All dependencies reference existing nodes
3. All edges reference valid nodes
4. Every dependency has a corresponding edge
5. Every edge has a valid relationship type
6. Required states are valid enum values

### Logical
7. No circular dependencies (DFS detection)
8. Orphan nodes flagged (no edges in or out)
9. State transitions are valid (per NODE_SCHEMA status enum)
10. Graph metadata is internally consistency

### Policy
11. Acceptance criteria exist for all nodes
12. Evidence requirements exist for implementable nodes
13. High-risk nodes have `requires_human_approval: true`
14. Security-critical nodes have security verification
15. Legal-critical nodes have legal-review state
16. Crypto nodes cannot be autonomously implemented
17. P0 nodes have appropriate priority

### Cross-check
18. PROJECT_STATE.yaml matches graph state
19. Memory files reference valid nodes
20. No node claims COMPLETE without evidence reference

## Relationship Types (from EDGE_SCHEMA.yaml)

- depends_on, blocks, enables, implements, tests, validates
- integrates_with, improves, replaces, conflicts_with, derived_from

## Node Status Values (from NODE_SCHEMA.yaml)

DISCOVERED, RESEARCHING, RESEARCH_COMPLETE, REQUIREMENTS_DEFINED,
DESIGNING, DESIGN_REVIEW, IMPLEMENTING, UNIT_TESTING, INTEGRATION_TESTING,
ADVERSARIAL_TESTING, SECURITY_REVIEW, PERFORMANCE_REVIEW, DOCUMENTING,
ACCEPTANCE, COMPLETE, FAILED, BLOCKED, NEEDS_RESEARCH, NEEDS_REDESIGN,
NEEDS_REFACTOR, SECURITY_BLOCKED, PERFORMANCE_BLOCKED, HUMAN_APPROVAL_REQUIRED,
DEPRECATED

## Output Format

```
GRAPH VALIDATION REPORT
=======================
Version: [version]
Total Nodes: [N]
Validation: [PASS | FAIL]

ISSUES:
  [ERROR/WARNING]: [description]
  [ERROR/WARNING]: [description]

CIRCULAR DEPENDENCIES: [NONE | list]
ORPHAN NODES: [NONE | list]
MISSING EDGES: [NONE | list]
INVALID REFERENCES: [NONE | list]

STATE CONSISTENCY:
  Graph vs PROJECT_STATE: [MATCH | MISMATCH]
  Graph vs memory files: [MATCH | MISMATCH]

RECOMMENDATIONS:
  [if any]
```
