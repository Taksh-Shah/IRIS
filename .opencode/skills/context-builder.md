# Context Builder Skill

## Purpose

Given an active graph node, determine the minimum relevant context to load.
The Context Builder is model/provider-aware: it dynamically determines the
effective context limit and allocates the working context accordingly.

## Model Context Awareness

The current IRIS execution environment uses LongCat-2.0 with a 1M-token context window.
The system must dynamically determine the effective context limit and remain portable
to smaller or larger models.

The Context Builder must remain functional if the effective context is:
128K, 200K, 500K, 1M, or larger.

```
MODEL CONTEXT LIMIT (detected from environment)
        ↓
available input budget
        ↓
reserve output + safety margin (typically 10-15%)
        ↓
allocate to P0 → P1 → P2 → P3 → P4
```

## Context Priority Hierarchy

### P0 — Load Immediately
Always loaded regardless of context size. These are essential for any operation.
- Current state (CURRENT_STATE.md)
- Active node (ACTIVE_NODE.md)
- Next action (NEXT_ACTION.md)
- Hard constraints (from CURRENT_STATE.md)
- Acceptance criteria (from ACTIVE_NODE.md)
- Relevant dependencies

### P1 — Load for This Work
Loaded when P0 is satisfied and budget remains.
- Relevant architecture docs
- Relevant ADRs
- Relevant decisions (DECISIONS.md)
- Relevant invariants

### P2 — Load If Budget Allows
Loaded only when substantial budget remains after P0 + P1.
- Relevant research
- Relevant experiments
- Relevant tests

### P3 — Historical Context (on demand)
Never loaded automatically. Only when explicitly needed.
- Failed approaches
- Past discoveries
- Previous session context

### P4 — Never Load Automatically
- Unrelated project information
- Completed node details (unless dependency)
- Vision/charter (already known)

## Adaptive Budget Allocation

The budget is allocated as a percentage of available input budget, not fixed token counts.

| Priority | Allocation | Description |
|----------|-----------|-------------|
| P0 | ~10% of available | Essential state (always loaded) |
| P1 | ~20% of available | Work-specific context |
| P2 | ~30% of available | Deep reference material |
| Output reserve | ~10-15% of limit | Space for model response |
| Safety margin | remaining | Overflow buffer |

### Example: 1M context window
```
Model limit:          1,048,576
Output reserve:         ~100,000 (10%)
Safety margin:           ~50,000
Available input:        ~898,576
  P0 (10%):              ~89,857
  P1 (20%):             ~179,715
  P2 (30%):             ~269,572
```

### Example: 200K context window
```
Model limit:              200,000
Output reserve:            ~20,000 (10%)
Safety margin:             ~10,000
Available input:           170,000
  P0 (10%):                17,000
  P1 (20%):                34,000
  P2 (30%):                51,000
```

### Example: 128K context window
```
Model limit:              128,000
Output reserve:            ~12,800 (10%)
Safety margin:              ~6,400
Available input:           108,800
  P0 (10%):                10,880
  P1 (20%):                21,760
  P2 (30%):                32,640
```

## Usage

When starting work on a node:
1. Detect the effective model context limit
2. Calculate available input budget (limit - output reserve - safety margin)
3. Load P0 context (always)
4. Load P1 context (if budget remains)
5. Load P2 context (if substantial budget remains)
6. Never load P3/P4 automatically
7. Monitor actual usage and adjust

## Output Format

```
CONTEXT PLAN for [node ID]
===========================
Model context limit: [detected limit]
Output reserve:      [reserved]
Available input:     [calculated]

P0 (load now ~X%):
  - [file] (~XK)
  - [file] (~XK)
  Subtotal: ~XK

P1 (load for work ~X%):
  - [file] (~XK)
  - [file] (~XK)
  Subtotal: ~XK

P2 (if budget allows ~X%):
  - [file] (~XK)
  Subtotal: ~XK

Total estimated: ~XK / [limit]
Budget remaining: ~XK
```
