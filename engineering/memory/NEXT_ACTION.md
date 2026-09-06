# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-09-06T18:55:00+05:30

## Current action: hardware-verification continuation

The repository is no longer awaiting phones. The bench is available and the
software graph is complete/deferred. Continue from the hardware loop, not from
the obsolete August setup instructions.

### Verified baseline

- P1: `10BCA20F4M000BB`, vivo V2205, Android 14/API 34.
- P2: `b2fbcd39`, vivo 2004, Android 12/API 31.
- Tier-2 shipping-app Wi-Fi Direct cold-cycle gate: **10/10 on the original
  Vivo-only bench; not a cross-OEM acceptance claim**.
- One group per cycle, bidirectional delivery, persistent keys, no invitation
  dialog.
- Current head: `b7a876c`; worktree was clean during reconciliation.

### Next work boundary

1. Commit the Session-32 Wi-Fi Direct dialog-containment repair after verifying
   the durable record and clean worktree.
2. Read the procedure and status in
   `docs/bug-hunting/hardware_verification/hardware_problems_loop.md`,
   `hardware_problems.md`, and `hardware_fix_log.md`.
2. Treat `tier2_wifidirect_cold_cycle.sh` as the current Tier-2 gate of record.
3. Use `iris_bench/test_tier2_wifidirect.py` as the real-app Mobly harness.
   Its default remains the 10-cycle acceptance gate; do not replace the
   shipping-app gate evidence with a shorter diagnostic run.
4. Deploy the authorized build with Wi-Fi Direct advertising, discovery, and
   explicit association enabled. Treat Android's invitation dialog as expected
   evidence; capture Samsung's Sharing dialog as a known limitation.
5. Select the next finding only from an explicit hardware procedure and preserve
   the loop order: Research → Design/implementation → Hardware test → Iterate →
   Close.
5. For the next controlled P2P run, expect and record the invitation UI; keep
   the Samsung Sharing dialog as a documented known limitation. Record every
   hardware attempt with device models, conditions, logcat, dumpsys,
   mesh snapshot, result, and commit/documentation status.

### Constraints

- This reconciliation pass changes documentation/state only; it does not install,
  launch, or test the APK and does not modify implementation code.
- Do not convert `✅ HW-pending` or harness evidence into `🟢 HW-verified` without
  the required physical evidence.
- Multi-hop remains gated on a third phone.
- LoRa/satellite modem legs remain hardware-gated.
