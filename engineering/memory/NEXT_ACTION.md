# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-19T09:45:00Z

---

## Priority: IOS-001 (P0 platform) — DOCUMENT (AC-17, iter ~148)

**IMPLanted (iter ~144..146) + TEST COMPLETE (iter ~147) — AC-1..AC-18:**
- **Tranche 1 (iter ~144, AC-1..AC-3 + AC-15 pre-seed)**: `crates/iris-ios`
  scaffold (11-op `FfiBleAdapter` incl `gatt_read` + SimBle tests; `BleBridge`
  all 11 ops; `IrisEngine` explicit tokio `Handle`); `uniffi-bindgen 0.31.2
  generate --library` → committed `ios/IRIS/RustFFI/` (`IrisCore.swift`);
  `G-IOS_SPIKE.md` (Swift-5 pin, @unchecked Sendable, tokio Handle).
- **Tranche 2 (iter ~145/146, AC-4..AC-15)**: `IosBleAdapter` 11-op (+gatt_read
  connect-to-identify, BLE-RT-C003 timeout, C004 probe budget, MTU ≤512 + 20-B
  guard, UUID filter); CoreBluetoothSeam + willRestoreState; KeychainEd25519/
  X25519 (no biometric, RFC 8032 KAT); SessionRecovery; LiveActivity iOS 16.1;
  BGTaskWiring; AppDelegate/IRISApp; ios.yml full AC-15 + build-xcframework.sh;
  6 Swift test files.
- **TEST (iter ~147, AC-16..AC-18)**: `IOS-001_TEST.md` evidence map — AC-16
  (33 XCTest cases across 6 files, macOS-host IOS-CoreBluetooth-Mock; execution
  env-gated → `test-macos` CI job), AC-17 (DEFERRED → this stage), AC-18 PASS
  (live: workspace **658/0/1**, clippy 0, fmt clean), AC-19 GATED/BLK-0005.
- **Commit**: `110ef5b` (IOS-001 IMPLEMENT complete). Baseline **658/0/1**.

## Next Action (iter ~148 — IOS-001 DOCUMENT, AC-17)

Apply the **6 EXTERNAL-FACTS corrections** recorded in `IOS_DESIGN.md` §13
(file:line old → new), deferred from DESIGN per AC-17:
1. `docs/platforms/IOS.md` — Xcode floor **15 → 16.4** (App Store mandate
   2025-04-24, iOS 18 SDK; RES-0025 RQ-3).
2. `docs/platforms/IOS.md` — UniFFI flow: stale **UDL command** → **`generate
   --library`** (0.31.x, proc-macro; no UDL).
3. `docs/platforms/IOS.md` — Secure Enclave section: **ECDSA P-256 ONLY** (no
   Ed25519) → iOS identity = CryptoKit Ed25519 in Keychain, SE rescue deferred
   (RES-0025 RQ-2).
4. `docs/platforms/IOS.md` — BGTask wording: re-submit **every launch** (not
   once).
5. `docs/implementation/SWIFT_LAYER.md` — state-restoration **re-arm** in
   `willRestoreState` + launch-reason classification.
6. Live-Activity floor **16.2 → 16.1** (RES-0025 RQ-5).

Docs-only pass — workspace untouched (baseline **658/0/1**, clippy 0, fmt
clean held).

→ **SECURITY_REVIEW (AC-20)** → **VERIFY (AC-21)** → ACCEPT → PILOT-001.

## Context

Pipeline: **IOS-001 → PILOT-001**. 27 COMPLETE nodes. Env-gates: Swift
compile/run = macOS CI leg; device rows BLK-0005.