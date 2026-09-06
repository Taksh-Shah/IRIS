# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-09-06T09:30:00+05:30

## Current action: hardware-verification continuation

The repository is no longer awaiting phones. The bench is available and the
software graph is complete/deferred. Continue from the hardware loop, not from
the obsolete August setup instructions.

### Verified baseline

- P1: `10BCA20F4M000BB`, vivo V2205, Android 14/API 34.
- P2: `b2fbcd39`, vivo 2004, Android 12/API 31.
- Tier-2 shipping-app Wi-Fi Direct cold-cycle gate: **10/10**.
- One group per cycle, bidirectional delivery, persistent keys, no invitation
  dialog.
- Current head: `b7a876c`; worktree was clean during reconciliation.

### Next work boundary

1. Read the procedure and status in
   `docs/bug-hunting/hardware_verification/hardware_problems_loop.md`,
   `hardware_problems.md`, and `hardware_fix_log.md`.
2. Treat `tier2_wifidirect_cold_cycle.sh` as the current Tier-2 gate of record.
3. Keep `iris_bench/test_tier2_wifidirect.py` marked HW-PENDING until its
   instrumented discovery path emits credible device evidence.
4. Select the next finding only from an explicit hardware procedure and preserve
   the loop order: Research → Design/implementation → Hardware test → Iterate →
   Close.
5. Record every hardware attempt with device models, conditions, logcat, dumpsys,
   mesh snapshot, result, and commit/documentation status.

### Constraints

- This reconciliation pass changes documentation/state only; it does not install,
  launch, or test the APK and does not modify implementation code.
- Do not convert `✅ HW-pending` or harness evidence into `🟢 HW-verified` without
  the required physical evidence.
- Multi-hop remains gated on a third phone.
- LoRa/satellite modem legs remain hardware-gated.
