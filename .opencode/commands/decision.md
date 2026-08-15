# /decision — Record an Architectural Decision

When invoked, create a durable decision record.

## Steps

1. Read the decision context from the user/active node
2. Identify alternatives considered
3. Record the decision with:
   - DEC-XXXX ID (from ALLOCATION.md)
   - Context (why needed)
   - Options considered (at least 2)
   - Decision (which option chosen)
   - Rationale (why)
   - Consequences (positive, negative, neutral)
   - Evidence (sources)
   - Affected nodes
4. Create record in `engineering/memory/records/decisions/DEC-XXXX.md`
5. Update `engineering/memory/DECISIONS.md`
6. Update `engineering/memory/records/ALLOCATION.md`
7. Write ADR if significant decision
8. Report

## Output Format

```
IRIS DECISION RECORDED
======================
ID: DEC-XXXX
Title: [title]
Status: [APPROVED | PROPOSED]
Node: [node ID]
Affected: [nodes]

OPTIONS CONSIDERED
  A: [option] — [why rejected]
  B: [option] — [why rejected]
  C: [option] — [chosen]

RATIONALE
  [why chosen]

CONSEQUENCES
  Positive: [list]
  Negative: [list]
  Neutral: [list]
```
