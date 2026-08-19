# IOS-001 TEST — iOS Platform Integration (AC-16..AC-18 evidence mapping)

**Node**: IOS-001 (P0 PLATFORM — Swift/SwiftUI shell + CoreBluetooth device layer
over Rust `iris-ios` FFI)
**Stage**: TEST · **Iteration**: ~147 · **Date**: 2026-08-19
**Pattern**: ANDROID-001_TEST.md / BLE-002_TEST.md
**Upstream evidence**: `IOS_DESIGN.md` v1.0 (AC-1..AC-21), iter 141-146 records

---

## 1. Acceptance-criteria evidence table

| AC | Criterion | Verdict | Evidence |
|----|-----------|---------|----------|
| AC-16 | macOS-host unit tests green w/ IOS-CoreBluetooth-Mock | **AUTHORED** (execution env-gated → CI) | 6 test files under `ios/Tests/` (XCTest, compiled onto the macOS target via `project.yml` `IRIS-MacOSTests` scheme): **`MockCoreBluetooth.swift`** (MockPeer + MockBleSeam implementing the `BleCentralSeam`/`BlePeripheralSeam` protocol seam → deterministic central/peripheral simulation, no real CoreBluetooth); **`IosBleAdapterTests.swift`** (AC-4..AC-9 — 13 tests: `testStartScanRegistersIrisServiceOnly`, `testStartScanRejectsUuidlessFilter`, `testStartAdvertisingProgramsIdentifyServiceOnly`, `testGattReadReturnsIdentifyBeacon` (AC-5: connect → read → DiscoveryBeacon), `testGattReadRejectsNonIdentifyCharacteristic`, `testGattReadTimesOutAndCancels` (AC-6 BLE-RT-C003), `testProbeBudgetCapsProbeConnects` + `testProbeRejectionWindowBlocksReprobe` (AC-7 C004), `testSetMtuReturnsNegotiatedPayload20Guard` (AC-8), `testScanResultsDrainIsEmptyPayload`, `testIncomingGattWritesDrainAttributesHandle`, `testGattWriteWithResponse`); **`ProbeAdmissionTests.swift`** (AC-7 — 3 tests incl. budget reset on new scan window + re-admission within window); **`KeychainIdentityTests.swift`** (AC-12 — 5 tests: accessibility class pinned, seed/pubkey round-trip, protected-data gate, **RFC 8032 KAT byte-compat**, static-ad X25519 key round-trip); **`SessionRecoveryTests.swift`** (AC-10/11 — 9 tests: launch-reason classification, BGTask re-submit on every launch, restoration re-arm gated on protected-data, force-quit no-op, debounced re-arm); **`FfiConformanceTests.swift`** (AC-3 — 3 tests: adapter conforms to generated protocol, Sendable records, UUID translations match generated constants). **Execution = `test-macos` job** (.github/workflows/ios.yml): `xcodebuild -scheme IRIS-MacOSTests -destination 'platform=macOS' test` after staging `ios/Build/macos/libIrisCore.a` + FFI header/modulemap from the rust-bindings job → **env-gated on macOS runner** (Windows dev host has no Xcode/swiftc) — recorded known_limitation, not blocker. |
| AC-17 | Doc reconciliation: 6 EXTERNAL-FACTS corrections (IOS.md + SWIFT_LAYER.md) applied | **DEFERRED → DOCUMENT stage** | IOS_DESIGN.md §13 records all 6 corrections as file:line old→new (IOS.md Xcode floor 15→16.4 + UniFFI `generate --library` UDL-command stale + Secure-Enclave-no-Ed25519 section + BGTask re-submit EVERY launch wording; SWIFT_LAYER.md state-restoration re-arm; Live-Activity 16.2→16.1 floor). Doc edits intentionally deferred to the DOCUMENT stage per AC-17 to keep TEST focused on executable evidence — carried as next-stage contract. |
| AC-18 | Baseline held: workspace ≥647/0/1, clippy 0 | **PASS** | **Live re-verify THIS pass (iter ~147 host)**: `cargo test --workspace --all-features` = **658 passed / 0 failed / 1 ignored** (iris-core 567 + iris-ios 11 + others; ≥647 target MET); `cargo clippy --workspace --all-targets --all-features -- -D warnings` = **0 warnings**; `cargo fmt --all -- --check` = clean. |
| AC-19 | Physical-device rows GATED/BLK-0005 | **GATED/BLK-0005** | Recorded known_limitation per DEC-0009/BLK-0005 — real iPhone required (iOS Simulator has no CoreBluetooth): scan/advertise/probe/MTU on-air, battery/throughput, Live Activity on-device rendering, state-restoration relaunch. Not a blocker. |
| AC-20 | SECURITY_REVIEW stage next | **NEXT** | Redteam adversarial review of `IosBleAdapter.swift` + seam + identity path — after DOCUMENT. |
| AC-21 | VERIFY stage next | **NEXT** | Independent verifier reproduction — after SECURITY_REVIEW, before ACCEPT. |

> Note on AC-1..AC-15 evidence: fully recorded in PROJECT_GRAPH IOS-001
> (evidence 6) + execution-log iter 144/145/146 — tranche 1 (Rust leg:
> crates/iris-ios 11/11, generated Swift bindings, tokio Handle) and tranche 2
> (Swift shell: adapter/seam/identity/recovery/app/CI/ios.yml). The Swift
> compile/build/test legs are env-gated to the macOS CI workflow (AC-15) —
> this document adds the AC-16..AC-18 evidence layer per the C2-testable-on-CI
> requirement.

---

## 2. Live re-verify — iter ~147 (this host)

- `cargo test --workspace --all-features` → **658 passed / 0 failed / 1 ignored**
  (iris-core 567 + iris-ios 11 net-new in tranche 1). AC-18 origin: 647/0/1
  pre-`iris-ios` baseline + 11 `iris-ios` FFI/bridge/engine tests.
- `cargo test -p iris-ios` (spike leg) → **11 passed / 0 failed**.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings` →
  **0 warnings**.
- `cargo fmt --all -- --check` → clean (exit 0).
- Commit: `110ef5b` (IOS-001 IMPLEMENT complete, 49 files / +9097).
- Toolchain probe: `swiftc` / `xcodebuild` / Xcode **NOT PRESENT** on Windows
  dev host → Swift compile + XCTest + Simulator legs env-gated to the macOS CI
  runner (`test-macos`, `test-simulator`, `swift-build` jobs).

## 3. Swift unit-test inventory (authored; run on macOS CI leg)

Total **33** XCTest cases across 6 files (`ios/Tests/`):
- `MockCoreBluetooth.swift` — seam doubles (MockPeer, MockBleSeam), no test
  count (support).
- `IosBleAdapterTests.swift` — 13 (AC-4..AC-9).
- `ProbeAdmissionTests.swift` — 3 (AC-7, C004 probe budget).
- `KeychainIdentityTests.swift` — 5 (AC-12, RFC 8032 KAT).
- `SessionRecoveryTests.swift` — 9 (AC-10/11).
- `FfiConformanceTests.swift` — 3 (AC-3, generated-protocol conformance).

## 4. Known limitations recorded (not blockers)

- **Env-gated Swift legs**: `swiftc -parse` (AC-3 check), `xcodebuild` app build
  (AC-15), `test-macos` XCTest (AC-16), `test-simulator` UI leg — macOS runner
  only; dev host is Windows (no Xcode). Wired in `.github/workflows/ios.yml`
  (standalone workflow, `workflow_dispatch` — not yet wired into ci.yml; per
  loop contract a separate platform-gate leg).
- **GATED/BLK-0005**: physical-device rows (AC-19) — on-air CoreBluetooth,
  battery/perf, Live Activity, state-restoration relaunch require a real
  iPhone.
- **Generated-Swift diff guard**: rust-bindings job runs `git diff --exit-code
  -- ios/IRIS/RustFFI/` — first full CI run any generated-Swift whitespace
  drift must be reconciled (recorded residual from tranche 1).
- **First-CI-run risks** (G1..G10, IOS_DESIGN.md §11): XCFramework per-slice
  headers now staged explicitly (lines 73-79), but runner-image/Xcode drift on
  monthly `macos-15` updates is a live risk; Swift-5 pin carries
  up-version debt until uniffi-rs#2929 resolves.

## 5. Next

DOCUMENT (iter ~148, AC-17 — apply the 6 EXTERNAL-FACTS corrections to
`docs/platforms/IOS.md` + `docs/implementation/SWIFT_LAYER.md`) →
**SECURITY_REVIEW (AC-20)** → **VERIFY (AC-21)** → ACCEPT → **PILOT-001**.