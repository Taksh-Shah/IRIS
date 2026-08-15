# /research — Conduct Research on Active Node

When invoked, conduct research for the active graph node following RESEARCH_POLICY.yaml.

## Steps

1. Read `engineering/memory/ACTIVE_NODE.md` for research questions
2. Read `engineering/RESEARCH_POLICY.yaml` for evidence hierarchy
3. For each research question:
   - Search primary sources (RFCs, official docs, standards)
   - Search secondary sources (peer-reviewed papers)
   - Compare sources for contradictions
   - Identify gaps
   - Produce evidence-backed conclusion
4. Create research record in `engineering/memory/records/research/RES-XXXX.md`
5. Update node research_evidence field
6. Report findings

## Evidence Hierarchy (from RESEARCH_POLICY.yaml)

1. Official Specifications (RFCs, platform docs)
2. Peer-Reviewed Research
3. Reputable Technical Reports
4. High-Quality Open Source Implementations
5. Community Discussion (supplementary only)

## Output Format

```
IRIS RESEARCH RESULTS
=====================
Node: [node ID]
Questions: [N]

FINDINGS
  Q1: [question]
    A: [answer with evidence]
    Source: [source]
    Confidence: [HIGH | MEDIUM | LOW]
    Maturity: [RESEARCHED | ...]

  Q2: ...

GAPS REMAINING
  [gap description]

RECORDS CREATED
  RES-XXXX: [title]
```
