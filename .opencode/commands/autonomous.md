# /autonomous — Start IRIS Autonomous Engineering Supervisor

This command starts or continues the IRIS autonomous execution supervisor.

## Purpose

Run the IRIS engineering graph continuously from the current durable state. The supervisor advances through nodes, stages, and actions without requiring additional user prompts.

The supervisor stops ONLY for:
1. Mission completion
2. Human approval gates
3. Unrecoverable blockers
4. Fatal environment failure

## Steps

### 1. Reconstruct State

Read these files in order:
1. `engineering/memory/execution-state.yaml`
2. `engineering/memory/CURRENT_STATE.md`
3. `engineering/memory/ACTIVE_NODE.md`
4. `engineering/memory/NEXT_ACTION.md`
5. `engineering/memory/BLOCKERS.md`
6. `engineering/PROJECT_GRAPH.yaml`

Verify consistency. If conflicts exist, trust `execution-state.yaml` for execution decisions.

### 2. Validate

- Check graph integrity (no circular dependencies, all edges valid)
- Check state consistency (node statuses match, no orphans)
- Check for human gates (`execution_status == HUMAN_GATE`)
- Check for blockers (`blockers` list non-empty)

### 3. Determine Position

```
if execution_status == IDLE or BOOTSTRAPPING:
    select_first_eligible_node()

elif execution_status == RECONSTRUCTING:
    validate_state_and_resume()

elif execution_status == ACTIVE:
    continue_current_work()

elif execution_status == STALLED:
    run_recovery_protocol()

elif execution_status == BLOCKED:
    attempt_unblock_or_transition()

elif execution_status == HUMAN_GATE:
    report_gate_and_STOP()

elif execution_status == MISSION_COMPLETE:
    report_completion_and_STOP()

elif execution_status == ERROR_RECOVERY:
    attempt_error_recovery()
```

### 4. Execute One Cycle

Execute exactly ONE action from the current stage of the current node. One action means:
- One research task
- One design decision
- One file implementation
- One test
- One verification

Then update all durable state files.

### 5. Evaluate Continuation

After the action:

```
if action_succeeded:
    update_progress()
    reset_stall_counter()

    if stage_complete:
        advance_stage()
        if node_complete:
            accept_node()
            update_graph()
            discover_new_nodes()
            select_next_node()

    if mission_complete:
        persist_completion()
        STOP  # legitimate

    CONTINUE  # default

elif action_failed:
    classify_failure()

    if recoverable:
        persist_recovery_state()
        CONTINUE  # retry with recovery

    if requires_human:
        persist_human_gate()
        STOP  # legitimate

    if fatal:
        persist_error()
        STOP  # legitimate

if no_progress_for_N_iterations:
    enter_stalled_state()
    run_recovery_protocol()
    CONTINUE
```

### 6. Persist

After every cycle, update:

1. `engineering/memory/execution-state.yaml`
2. `engineering/memory/CURRENT_STATE.md`
3. `engineering/memory/ACTIVE_NODE.md`
4. `engineering/memory/NEXT_ACTION.md`
5. Any new records (RES-, DEC-, DISC-, FAIL-, VER-, CHK-)
6. Append to execution log

### 7. Continue or Stop

Default action: **CONTINUE**

Stop only when:
- `execution_status == MISSION_COMPLETE`
- `human_gate_required == true` (with full documentation)
- `execution_status == BLOCKED` and recovery exhausted
- `execution_status == ERROR_RECOVERY` and recovery fails

## Example Output

```
IRIS AUTONOMOUS ENGINE
────────────────────────────────
Iteration: 7
Node: STORE-001
Stage: UNDERSTAND
Action: Validate storage requirements against protocol spec

[1] Loading context...
[2] Reading STORAGE.md...
[3] Checking schema completeness...
[4] Verifying eviction policy matches priority model...
[5] Identifying gaps...

Result: STAGE_COMPLETE
Durable progress:
  - execution-state.yaml updated
  - ACTIVE_NODE.md stage advanced to RESEARCH

Next action: Research SQLCipher integration patterns

CONTINUING...
```

## Configuration

The supervisor uses these OpenCode agent settings:
- Agent: `supervisor`
- `maxSteps`: Configured in opencode.jsonc (default: 50)
- Context: model/provider-aware, adaptive budget

## Important

This command does NOT print a plan and stop. It executes one cycle, persists state, and continues to the next cycle. The supervisor loop runs until a legitimate stop condition is met.

If you want to see the current state without executing, use `/status`.
