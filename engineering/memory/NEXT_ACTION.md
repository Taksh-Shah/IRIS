# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T10:00:00Z

---

## Priority: IOS-001 (P0 platform) — SECURITY_REVIEW (AC-20, iter ~149)

**Pipeline POSITION: IOS-001 IMPLEMENT + TEST + DOCUMENT COMPLETE (AC-1..AC-18):**
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
- **DOCUMENT (iter ~148, AC-17)**: 6 EXTERNAL-FACTS corrections applied
  (`docs/platforms/IOS.md` Xcode 16+/`generate --library`/Keychain-Ed25519/
  BGTask; `SWIFT_LAYER.md` willRestoreState re-arm; DISCOVER 16.2→16.1).
  Baseline **658/0/1** held.

## Next Action (iter ~149 — IOS-001 SECURITY_REVIEW, AC-20)

Redteam adversarial review of the Swift platform layer (pattern
ANDROID-001/BLE-002 SECURITY_REVIEW):
- **Adapter**: `IosBleAdapter.swift` — 11-op contract fidelity, gatt_read
  connect-to-identify, BLE-RT-C003 timeout/cancellation, probe-admission
  budget/rejection-window exhaustion, MTU negotiation, peer/char handle
  attribution, thread-safety (@unchecked Sendable + lock discipline).
- **Seam**: `CoreBluetoothSeam.swift` + real CB managers — state restoration,
  delegate races, address/UUID parsing.
- **Identity**: `KeychainEd25519.swift` — accessibility class, no-biometric
  gate, protected-data gating, KAT integrity.
- **Lifecycle**: `SessionRecovery.swift` / `BGTaskWiring.swift` /
  `LiveActivityController.swift` / App wiring — launch classification,
  force-quit no-op, re-submit cadence, entitlement/min-info.plist claims.
- Findings dispositioned FIXED/RECORDED in `IOS-001_SECURITY_REVIEW.md`.

→ **VERIFY (iter ~150, AC-21)** → ACCEPT → PILOT-001. Baseline **658/0/1**
clippy 0 fmt clean.

## Context

27 COMPLETE nodes. Pipeline: **IOS-001 → PILOT-001**. Env-gates: Swift
compile/run = macOS CI leg (`test-macos`/`test-simulator`); device rows
BLK-0005.