# /discover — Record a New Discovery

When invoked, create a durable discovery record.

## Steps

1. Read the discovery claim
2. Identify evidence maturity level (per RESEARCH_POLICY.yaml)
3. Identify affected node(s)
4. Check if this contradicts any existing decision or discovery
5. Create record in `engineering/memory/records/discoveries/DISC-XXXX.md`
6. Update `engineering/memory/DISCOVERIES.md`
7. Update `engineering/memory/records/ALLOCATION.md`
8. If discovery affects active node: update ACTIVE_NODE.md
9. If discovery creates new work: create new graph node if appropriate
10. Report

## Evidence Maturity Levels

HYPOTHESIZED → RESEARCHED → DESIGNED → IMPLEMENTED → UNIT_VALIDATED → INTEGRATION_VALIDATED → SIMULATION_VALIDATED → HARDWARE_VALIDATED → FIELD_VALIDATED → PRODUCTION_VALIDATED → CERTIFIED

Never allow one level to masquerade as another.

## Output Format

```
IRIS DISCOVERY RECORDED
=======================
ID: DISC-XXXX
Title: [title]
Maturity: [level]
Node: [node ID]
Impact: [what this changes]

RELATED
  Decisions affected: [DEC-XXXX or none]
  Contradictions: [DISC-XXXX or none]
  New work needed: [description or none]
```
