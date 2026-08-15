# Engineering Memory Records

Addressable durable records for IRIS project.

## Record Types

| Prefix | Type | Directory | Purpose |
|--------|------|-----------|---------|
| DEC- | Decision | decisions/ | Architectural decisions with context |
| RES- | Research | research/ | Research findings with evidence |
| EXP- | Experiment | experiments/ | Experiment results and data |
| FAIL- | Failure | failures/ | Failed approaches and lessons |
| DISC- | Discovery | discoveries/ | Discovered facts and insights |
| CHG- | Change | changes/ | Change log entries |
| VER- | Verification | verification/ | Verification results |
| CHK- | Checkpoint | checkpoints/ | Session checkpoints |
| ORCH- | Orchestration | ./ | Phase/implementation orchestration plans |

## Record Format

Each record is a markdown file with this structure:

```markdown
# [PREFIX]-XXXX: [Title]

**Type**: [Record type]
**Date**: YYYY-MM-DD
**Node**: XXXX-XXX
**Author**: [Agent name]
**Status**: ACTIVE | RESOLVED | SUPERSEDED

## Summary
[One-line summary]

## Detail
[Full content]

## Evidence
[Links to evidence]

## Related
- Files: [file paths]
- Decisions: [DEC-XXXX]
- Nodes: [XXXX-XXX]
```

## ID Allocation

IDs are allocated sequentially within each type.
See allocation file for next available ID.

## Usage

1. Create record when a decision is made, research completed, or failure occurs
2. Reference records by ID from memory files and graph nodes
3. Never delete records — mark SUPERSEDED if replaced
4. Commit records with the work that created them
