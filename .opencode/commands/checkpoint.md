# /checkpoint — Save Current Progress to Durable State

When invoked, save all current progress to durable state files.

## Steps

1. Read current `engineering/memory/CHECKPOINT.md`
2. Read current `engineering/memory/CURRENT_STATE.md`
3. Read current `engineering/memory/ACTIVE_NODE.md`
4. Run `git status` and `git diff --stat`
5. Identify what changed since last checkpoint
6. Update `CHECKPOINT.md` with:
   - New checkpoint ID (sequential)
   - Current timestamp
   - Last completed action
   - Current action
   - Next action
   - Modified files list
   - New decisions
   - New blockers
   - New discoveries
   - New failed approaches
   - Test results
   - Verification state
7. Update `CURRENT_STATE.md` with latest position
8. Update `ACTIVE_NODE.md` with current progress
9. Update `NEXT_ACTION.md` with remaining steps
10. Update `LAST_ACTION.md` with what was just done
11. Create record in `engineering/memory/records/checkpoints/CHK-XXXX.md`
12. Update `SESSION_STATE.md`
13. Report checkpoint created

## When to Checkpoint

- Before context compaction (when possible)
- After completing a graph node
- After important decisions
- Before changing graph state
- After major research findings
- Before ending an autonomous work period
- Before switching major tasks

## Output

```
=== IRIS CHECKPOINT ===
Checkpoint: CHK-XXXX
Node: XXXX-XXX
Action Saved: [what was done]
Next: [what comes next]
State: DURABLE
```
