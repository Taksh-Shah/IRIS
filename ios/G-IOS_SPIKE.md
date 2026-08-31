# G-IOS Spike — UniFFI Swift seam ground-truth (IOS-001 AC-3)

**Node**: IOS-001 (P0 PLATFORM) · **Tranche**: IMPLEMENT AC-1..AC-3 · **Date**: 2026-08-18
**Rust-leg host**: Windows dev host (no Xcode) · **Swift-leg**: macOS-CI env-gate (G-TI-1 pattern)
**Contract refs**: `docs/implementation/IOS_DESIGN.md` §4/§9 (AC-1..AC-3), §10 DEC-IOS-0002/0003;
RES-0025 §2/§9 (RQ-1 findings, G-IOS-1/G-IOS-2/G-IOS-5); ANDROID_DESIGN §10 (G-AND-3 spike, 3/3 PASS).

## 1. Verdict

**Rust leg PASS (11/11)** — `crates/iris-ios` compiles warning-free and its spike
suite passes; the UniFFI 0.31.2 `generate --library` flow runs on the host and
produces the committed Swift surface. **Swift compile leg ENV-GATED** — no
`swiftc`/Xcode on this Windows host; the Swift-5-mode compile proof and the
`@unchecked Sendable` conformance compile land on the `ios.yml` macOS runner
(AC-15), and are pre-wired here as concrete checkpoints.

## 2. Swift-5 language-mode pin (DEC-IOS-0002) vs mozilla/uniffi-rs#2929

**Decision**: `SWIFT_VERSION=5` in the Xcode project (XcodeGen `project.yml`) for
the app + test + widget-extension targets. **NOT** `.v6` (strict concurrency).

**Rationalization (from RES-0025 RQ-1 finding 5 + generated ground truth)**:
UniFFI 0.31.2 generated Swift for an **async foreign-trait method** emits a
`Task { @MainActor in … }` foreign-future bridge whose closure captures a
non-`Sendable` `RustBuffer`. Under Swift-6 language mode (`.v6`) this is a hard
error (`#SendingClosureRisksDataRace`), tracked open as mozilla/uniffi-rs#2929
(as of 2026-06-18; 0.32.0 did not close it — re-verify on any 0.32.x upgrade).
The sync callback-interface path was fixed in 0.31.2 (#2886), which is why the
11 sync `BleAdapter` ops alone would be Swift-6-safe — but the `IrisBody`
envelope surface intentionally carries an async method (`describe`) to keep the
async-over-FFI proof alive, so the pin is required by the combined surface.

**Ground truth in generated code** (`ios/IRIS/RustFFI/IrisCore.swift`):
- `public protocol FfiBleAdapter: AnyObject, Sendable` — 11 sync ops (line ~586).
- `public protocol IrisBody: AnyObject, Sendable { func summarize(...) -> String;
  func describe(...) async throws -> String }` (line ~1391) — the async method
  generates the foreign-future `Task` bridge (makeCall/handleSuccess pattern,
  lines ~1467-1500) that #2929 flags under `.v6`.
- `public protocol FfiInboxListener: AnyObject, Sendable` (line ~1184).

**Fallback if an upgrade forces `.v6`** (committed plan, not applied): the
documented post-codegen patch — `extension RustBuffer: @unchecked Sendable {}`
plus `@Sendable` on `makeCall`/`handleSuccess`/`handleError` helpers and the
task closures. Applied as a post-generation patch step in `ios.yml` only if the
Swift-5 pin is ever dropped.

**Re-verify**: G-IOS-1 (on UniFFI 0.32.0 upgrade, test whether #2929 closed and
the `.v6` pin can be restored); `ios.yml` runs `swiftc -swift-version 5` over
the generated file + the `FfiConformanceTests` target.

**Note (Bug #63)**: `swiftc -swift-version 5` is available only on Xcode 16.4+.
`IPHONEOS_DEPLOYMENT_TARGET 18.0` in `ios/project.yml` requires Xcode 16.4 or
later. The `ios.yml` CI runner is pinned to `macos-15` with `XCODE_VERSION:
"16.4"` (see `env:` block). Any CI image upgrade must preserve Xcode ≥ 16.4 or
the Swift-5-mode compile step will fail.

## 3. `@unchecked Sendable` usage points (foreign-trait adapters)

The generated protocols are `Sendable`; every Rust-side `Arc<dyn Ffi…>` bound
(`FfiBleAdapter`, `IrisBody`, `FfiInboxListener`) requires the Swift impl to be
`Sendable`. Usage points:

| Swift type | Conformance | Why `@unchecked` (not `Sendable`) |
|---|---|---|
| `IosBleAdapter: FfiBleAdapter` | `final class IosBleAdapter : FfiBleAdapter, @unchecked Sendable` | Owns mutable CoreBluetooth state (`CBCentralManager`, `CBPeripheralManager`, `CBPeripheral` map, probe-admission cache) — confined to a dedicated serial queue/actor; Rust calls methods from its runtime via UniFFI. `@unchecked` is the documented pattern (RES-0025 finding 6; IOS_DESIGN D-1). |
| `IrisBodyRenderer: IrisBody` (shell) | `final class IrisBodyRenderer : IrisBody, @unchecked Sendable` | Owns notification/Live-Activity presentation state; state confined to the app's serial context. |
| `InboxListener: FfiInboxListener` | `final class InboxListener : FfiInboxListener, @unchecked Sendable` | Forwards into Combine/`AsyncStream` sinks; cheap because `on_message` is sync. |

Rule (IOS_DESIGN §8.4): never share mutable state across the FFI seam — the
Swift side confines CoreBluetooth/keychain state to its own serial queue and
hands UniFFI **value snapshots** (owned records/`Data`/`UInt64` handles).

## 4. Explicit tokio `Handle` wiring (issue #2576 workaround, DEC-IOS-0003)

`#[uniffi::export(async_runtime="tokio")]` is INEFFECTIVE on exported-trait
impls in 0.31.x (mozilla/uniffi-rs#2576; fixed only in 0.32.0 via #2899). The
workaround — same as DEC-AND-0002/G-AND-3 — is applied in `crates/iris-ios`:

- `IrisEngine` (`#[derive(uniffi::Object)]`) owns a `tokio::runtime::Runtime`
  and stores `handle: Handle` (engine.rs).
- Every async poll path runs through that handle: engine construction
  (`runtime.block_on` for `TransportManager::register` + `MessageEngine`
  wiring), `start_all`/`send_text`/`stop_all` via `handle.block_on`, and the
  per-transport inbox forwarder + `subscribe_inbox` via `runtime.spawn`.
- Foreign-trait methods themselves are **sync** (`FfiBleAdapter` 11 ops) or
  async-over-FFI (`IrisBody::describe`), so the Rust side never runs its own
  event-loop thread for the adapter seam.

**Proof on host**: `engine_owns_a_live_runtime` + `async_body_surface_runs_on_engine_handle`
+ `start_all_and_discovery_reach_adapter_over_handle` tests PASS (Rust leg).

## 5. Host compile ground-truth (Rust leg) — commands + results

| Command | Result |
|---|---|
| `cargo check -p iris-ios --lib` | PASS, 0 warnings |
| `cargo test -p iris-ios` | **11 passed / 0 failed** (0.02 s) |
| `cargo clippy -p iris-ios --all-targets --all-features` | PASS, 0 warnings |
| `cargo fmt --all -- --check` | PASS |
| `cargo build -p iris-ios` | PASS (cdylib + staticlib; rlib removed in Bug #64) |
| `uniffi-bindgen generate --library target/debug/IrisCore.dll --language swift --out-dir ios/IRIS/RustFFI` | PASS → `IrisCore.swift` (96.7 KB), `IrisCoreFFI.h`, `IrisCoreFFI.modulemap` committed |

Crate-level spike tests (G-IOS-1/2/5 Rust leg): `projection_trait_drives_ops`,
`gatt_read_round_trip_returns_identify_beacon` (11-op incl. `gatt_read`),
`bridge_forwards_gatt_read_for_identify` (connect-to-identify seam),
`iris_body_async_surface_completes` + `async_body_surface_runs_on_engine_handle`
(async-over-FFI + tokio handle), `start_all_and_discovery_reach_adapter_over_handle`
(`ble-ios` transport ↔ adapter), `send_text_accepts_envelope`, `timeout_mapping_contract_is_typed`
(BLE-RT-C003 seam), `engine_owns_a_live_runtime`.

## 6. Swift compile leg — ENV-GATED (CI statement)

`swiftc`/Xcode are absent on the Windows dev host → the Swift-5-mode compile,
the `@unchecked Sendable` conformance compile, and the `IosBleAdapter`
CoreBluetooth-mock tests are **CI-leg** rows:

1. `ios.yml` (AC-15 pre-seed) — macOS runner: staticlibs for
   `aarch64-apple-ios` + `aarch64-apple-ios-sim`, regenerate bindings,
   `xcodebuild` build with `SWIFT_VERSION=5`, `IOS-CoreBluetooth-Mock` unit
   tests (`FfiConformanceTests`). TODO markers where the app/test targets land.
2. G-IOS-5 (CryptoKit Ed25519 ↔ Keychain ↔ Rust `ed25519-dalek` KAT) — Swift
   Keychain round-trip unit test, macOS-host, later tranche (AC-12).
3. Physical-device rows stay BLK-0005 gated (AC-19).

## 7. Files touched by this spike

- `crates/iris-ios/Cargo.toml`, `src/lib.rs`, `src/engine.rs`, `src/bridge.rs`,
  `src/ffi/{mod,ble_adapter,body,error}.rs`
- `ios/IRIS/RustFFI/{IrisCore.swift,IrisCoreFFI.h,IrisCoreFFI.modulemap}`
  (committed generated bindings)
- Root `Cargo.toml` (workspace member add)
- `.github/workflows/ios.yml` (AC-15 pre-seed, this spike's CI-leg)
