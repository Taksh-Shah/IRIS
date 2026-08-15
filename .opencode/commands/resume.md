# /resume — Reconstruct State and Continue Execution

When invoked, reconstruct full project state from durable artifacts and **automatically continue execution** if work is eligible.

## Steps

1. Read `AGENTS.md` (bootloader)
2. Read `engineering/memory/execution-state.yaml`
3. Read `engineering/memory/CURRENT_STATE.md`
4. Read `engineering/memory/ACTIVE_NODE.md`
5. Read `engineering/memory/NEXT_ACTION.md`
6. Read `engineering/memory/BLOCKERS.md`
7. Read `engineering/memory/CHECKPOINT.md`
8. Run `git status` to inspect repository reality
9. Run `git log --oneline -5` to see recent commits
10. Verify durable state matches repository reality
11. Validate graph integrity
12. Validate state consistency

## Decision Point

After reconstruction, determine:

```
if execution_status == MISSION_COMPLETE:
    Report completion. STOP.

elif execution_status == HUMAN_GATE:
    Report gate details. STOP.

elif execution_status == BLOCKED:
    Report blockers. Suggest resolution. STOP.

elif execution_status == STALLED:
    Run recovery protocol. CONTINUE.

elif execution_status == ERROR_RECOVERY:
    Attempt error recovery. CONTINUE.

elif execution_status == IDLE or ACTIVE:
    # WORK IS ELIGIBLE — DO NOT STOP
    Begin executing NEXT_ACTION.md
    Continue through the node loop.
    Do not return control to user until:
      - Node completes (then transition to next node)
      - Human gate encountered
      - Unrecoverable blocker
      - Mission complete
```

## CRITICAL RULE

**If work is eligible, `/resume` MUST NOT stop after reconstruction.**

`/resume` is not a "tell me where we are" command. It is a "reconstruct and continue" command.

After reading state, if `execution_status` is `ACTIVE` or `IDLE` and there are eligible nodes, you MUST begin executing the next action immediately.

## Output

```
IRIS RESUME
===========
Mission: [one line]
Phase: [current phase]
Active Node: [node ID] — [name]
Node Status: [status]
Execution Status: [ACTIVE | BLOCKED | HUMAN_GATE | ...]

[If ACTIVE:]
Next Action: [immediate next step]
Beginning execution...

[If BLOCKED:]
Blockers: [list]
Suggested resolution: [action]

[If HUMAN_GATE:]
Gate: [what is needed]
Evidence: [what was collected]
Recommendation: [suggested option]
```
