# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T11:30:00Z

---

## Priority: IOS-001 (P0 platform) — ACCEPT (iter ~151)

**Pipeline POSITION: IOS-001 IMPLEMENT + TEST + DOCUMENT + SECURITY_REVIEW + VERIFY COMPLETE (AC-1..AC-21):**
- **Tranche 1 (iter ~144)**: `crates/iris-ios` scaffold (11-op `FfiBleAdapter`
  + SimBle tests; `BleBridge` all 11 ops; `IrisEngine` explicit tokio
  `Handle`); `uniffi generate --library` → committed `ios/IRIS/RustFFI/`.
- **Tranche 2 (iter ~145/146, AC-4..AC-15)**: Swift shell authored in full
  (`IosBleAdapter` 11-op + gatt_read connect-to-identify + C003/C004 +
  MTU/UUID contracts; CoreBluetoothSeam + willRestoreState; KeychainEd25519/
  X25519; SessionRecovery; LiveActivity; BGTaskWiring; ios.yml full AC-15;
  6 Swift test files).
- **TEST (iter ~147, AC-16..AC-18)**: `IOS-001_TEST.md` (AC-16 33 XCTest cases
  env-gated → macOS CI; AC-18 PASS live **658/0/1** clippy 0 fmt clean; AC-19
  GATED/BLK-0005). Commit `110ef5b`.
- **DOCUMENT (iter ~148, AC-17)**: 6 EXTERNAL-FACTS corrections applied. Commit
  `86cfaa8`. Baseline **658/0/1** held.
- **SECURITY_REVIEW (iter ~149, AC-20)**: `IOS-001_SECURITY_REVIEW.md` authored
  (pattern ANDROID-001) — **FAIL → RESOLVED**: IOS-RT-101..108 dispositioned
  (no CRITICAL/HIGH; 5 FIXED + 4 RECORDED). +2 regression XCTest (35 total).
  Swift fixes + records **uncommitted** — committed at ACCEPT (this iteration).
- **VERIFY (iter ~150, AC-21)**: `IOS-001_VERIFICATION.md` authored (pattern
  BLE_002_VERIFICATION.md) — AC-1..AC-21 evidence table + independent verifier
  **APPROVE**; live reproduction **658/0/1** (23 suites), iris-ios 11/11,
  clippy 0, fmt clean; all RT dispositions re-located in source; no code changes.

## Next Action (iter ~151 — IOS-001 ACCEPT)

1. **Commit** the SECURITY_REVIEW (iter ~149) Swift fixes + new records:
   `ios/IRIS/App/AppDelegate.swift`, `ios/IRIS/Services/CBManagerPeripheral.swift`,
   `ios/IRIS/Services/IosBleAdapter.swift`, `ios/IRIS/Services/Notifications.swift`,
   `ios/Tests/IosBleAdapterTests.swift`, `engineering/memory/records/IOS-001_SECURITY_REVIEW.md`,
   `engineering/memory/records/IOS-001_VERIFICATION.md`.
2. **Graph/state**: PROJECT_GRAPH IOS-001 status IMPLEMENTING → **COMPLETE**
   (evidence 9 + stage_note full pipeline); PROJECT_STATE completed 27→**28**
   implementing 1→0; CHANGELOG entry.
3. **Context refresh**: ACTIVE_NODE / CURRENT_STATE / NEXT_ACTION → point at
   the new node; validate YAML triple.
4. **NODE_TRANSITION** → **PILOT-001** (DISCOVER, iter ~152).

→ **After ACCEPT, PIPELINE STATE = IOS-001 → PILOT-001.** Baseline
**658/0/1** clippy 0 fmt clean. Swift legs remain env-gated (ios.yml macOS CI).

## Context

27 COMPLETE nodes. Pipeline: **IOS-001 → PILOT-001**. Env-gates: Swift
compile/run = macOS CI leg (`test-macos`/`test-simulator`); device rows
BLK-0005.