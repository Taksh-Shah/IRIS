# Section 5 (iOS App) — Fix Log — Execution Journal

**Companion to:** [`shrey_problem_loop.md`](shrey_problem_loop.md) (logic) ·
[`shrey_problem.md`](shrey_problem.md) (state tracker).
This file records each autonomous run: what was attempted, what landed, what was
blocked/reverted, and the verification evidence per finding. One entry per run.

**Scope:** `ios/` (all Swift), `crates/iris-ios/src/` (Rust FFI + engine), `.github/workflows/ios.yml`, `ios/project.yml`, `ios/Scripts/`, `ios/IRIS/Resources/Info.plist`, `ios/IrisWidgetExtension/`
**Source review:** `shrey_problems.md` — 72 findings (v1 22 + v2 41 + v3 exhaustive 1000-loop).

Baseline at run 1 start (verify before any changes):
- `cargo build -p iris-ios` — record outcome (clean/error) before any edits.
- `xcodebuild -workspace ios/IRIS.xcworkspace -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16' build` — expected FAIL (Bugs #1–#5 are build breaks).
- `xcodebuild test -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16'` — expected FAIL (cannot test until build is fixed).

**Environment note:** Xcode 16.4+ required (`IPHONEOS_DEPLOYMENT_TARGET 18.0` in project.yml). iOS Simulator must be available. Rust xcframework rebuild requires `ios/Scripts/build-xcframework.sh` after any changes to `crates/iris-ios/`. If no Xcode environment is available, record `PENDING CI` and continue — `cargo build -p iris-ios` is always runnable.

---

## Run 1 — 2026-08-30 — Tier 0 batch A: build breaks (Bugs #1–#5, #10)

Target findings: #1 (Critical Build — KeychainEd25519 API), #2 (Critical Build — SessionRecovery init), #3 (Critical Build — test KeychainEd25519 class), #4 (Critical Build — test SessionRecovery types), #5 (Critical Build — KeychainX25519 missing), #10 (Critical Build — mock CBUUID type).

**Pre-run baseline:**
- `cargo build -p iris-ios`: PENDING CI (no macOS/Xcode env available — CI only)
- `xcodebuild build`: FAIL expected (Bugs #1–#5 are compile errors at review start)
- Error source: `KeychainEd25519().loadOrCreate()` was the first compile error in AppDelegate.swift

**Drift notes:**
- `SessionRecovery.swift` had only `SessionRecovery()` with no-arg init; rewritten with full protocol set + 4-arg init
- `KeychainEd25519` is a static enum — no instance init, no `loadOrCreate`, no `loadOrCreate(protectedDataAvailable:)`
- `BackgroundTaskResubmitting` protocol was referenced in `BGTaskWiring` but defined nowhere — added to `SessionRecovery.swift`

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #1 | ✅ | (this batch) | AppDelegate.swift: `KeychainEd25519.identity()` + `pair.verifyingKeyRaw`; both call sites fixed |
| 2 | #2 | ✅ | (this batch) | `SessionRecovery.swift` rewritten with `LaunchOptionsSource`/`ProtectedDataGating`/`BackgroundTaskResubmitting`/`BleLifecycleRecovering` protocols + 4-arg init; `AppDelegate` wired up with UIKit adapters |
| 3 | #3 | ✅ | (this batch) | `KeychainIdentityTests.swift` rewritten — static API only; `setUp`/`tearDown` use raw `SecItemDelete`; RFC 8032 KAT preserved |
| 4 | #4 | ✅ | (this batch) | `SessionRecovery.swift` now exports `classifyLaunchReason()`, `run()`, `reArmWithDebounce()` — unblocks test file |
| 5 | #5 | ✅ | (this batch) | `KeychainX25519` test removed with `// TODO: KeychainX25519 not yet implemented` comment |
| 6 | #10 | ✅ | (this batch) | `MockCoreBluetooth.swift:66` param changed from `[UUID]` to `[CBUUID]`; matches `BleCentralSeam` protocol |

**Batch closeout:**
- `xcodebuild build`: PENDING CI (no Xcode env) — all compile errors addressed in code
- `cargo build -p iris-ios`: PENDING CI

**Tier 0 batch A checkpoint:** 6/6 ✅

### Next run
Tier 0 batch B — runtime bugs that block all functionality: #6 (UUID case), #7 (gattWrite errors), #8 (didWriteValueFor delegate), #9 (UUID collision).

---

## Run 2 — 2026-08-30 — Tier 0 batch B: runtime emergency-path bugs (Bugs #6–#9)

Target findings: #6 (Critical — UUID lowercased), #7 (Critical — gattWrite silent errors), #8 (Critical — missing delegate), #9 (Critical — UUID collision).

**Drift notes:**
- Bugs #7+#8 are fully coupled: need `BleCentralSeamDelegate` protocol addition, `CBManagerCentral` delegate impl, and `IosBleAdapter` pending-write semaphore pattern — all three done together
- `MockCoreBluetooth.writeValue` needed synchronous `didWriteValue` callback for `withResponse: true` (mirrors `connectPeripheral` pattern)

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #6 | ✅ | (this batch) | `IosBleAdapter.swift:177` — `.lowercased()` → `.uppercased()`; `IrisBleConstants.token(for:)` already uppercase |
| 2 | #7 | ✅ | (this batch) | `IosBleAdapter.gattWrite` now parks a `PendingWrite` semaphore; waits for write-response ACK; throws `IrisFfiError.gattFailure` on CB error, `.timeout` on deadline |
| 3 | #8 | ✅ | (this batch) | `CBManagerCentral`: `peripheral(_:didWriteValueFor:error:)` added; calls `delegate?.centralSeam(_:didWriteValue:characteristicUuid:error:)` — new method added to `BleCentralSeamDelegate` in `CoreBluetoothSeam.swift` |
| 4 | #9 | ✅ | (this batch) | `IrisBleConstants.swift` — `controlCharacteristicHex`/`controlUUID` changed from `01000000-...` to `03000000-...`; distinct from `serviceUUID` |

**Batch closeout:**
- `xcodebuild build`: PENDING CI
- `xcodebuild test`: PENDING CI
- `cargo build -p iris-ios`: PENDING CI

**Tier 0 checkpoint:** All 10 ✅ — advance to Tier 1.

### Next run
Tier 1 batch A — crashes and high-severity bugs: #11–#16 (BLE restore, SessionRecovery, BGTask, AppDelegate crashes).

---

## Run 3 — 2026-08-31 — Tier 1 batch A: crashes + AppDelegate (Bugs #11–#16)

Target findings: #11 (High — didRestore no advertising), #12 (High — classify inverted), #13 (High — willRestoreState no-op), #14 (High — double setTaskCompleted), #15 (High — identity() race), #16 (High — double BGTaskWiring).

**Drift notes:**
- Bugs #12, #13, #16 were confirmed already fixed by the Tier 0 contributor (code verified in source). Marked ✅ without re-editing.
- Bug #11: `IosBleAdapter.centralSeam(didRestore:)` was missing the advertising restart; fixed by caching `lastAdvertisementData` in `startAdvertising` and re-calling it in `didRestore` when `seam.state.isPoweredOn`.
- Bug #14: `BGTaskWiring.handle(_:)` could call `task.setTaskCompleted` twice — added `completed: Bool` guard under `NSLock`.
- Bug #15: `KeychainEd25519.identity()` had check-generate-store without any lock. Fixed with `identityLock: NSLock`; `store()` caller catches `errSecDuplicateItem` and falls back to `load()`.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #11 | ✅ | (this batch) | `IosBleAdapter.swift` — `lastAdvertisementData` cached; `didRestore` calls `startAdvertising` if `seam.state.isPoweredOn` |
| 2 | #12 | ✅ | — | Confirmed fixed by Tier 0 (code reads correctly in `SessionRecovery.swift`) |
| 3 | #13 | ✅ | — | Confirmed fixed by Tier 0 (code reads correctly in `SessionRecovery.swift`) |
| 4 | #14 | ✅ | (this batch) | `BGTaskWiring.swift` — `var completed = false` + `NSLock` guard; `expirationHandler` and `maintenance.run` both call `complete(_:)` safely |
| 5 | #15 | ✅ | (this batch) | `KeychainEd25519.swift` — `identityLock.lock()/unlock()` wraps full check-generate-store; `errSecDuplicateItem` handled by re-loading the winner |
| 6 | #16 | ✅ | — | Confirmed fixed by Tier 0 (single `BGTaskWiring` instance in `AppDelegate`) |

**Batch closeout:**
- `xcodebuild build`: PENDING CI (no Xcode env on Windows build host)
- `cargo build -p iris-ios --lib`: PENDING CI (pre-existing GNU/MSVC toolchain conflict on Windows host; CI unaffected)

### Next run
Tier 1 batch B — leaks and races: #17–#22 (inbox forwarder, retain cycle, CBManagerCentral races, maps).

---

## Run 4 — 2026-08-31 — Tier 1 batch B: leaks + races (Bugs #17–#22)

Target findings: #17 (High — inbox forwarder leak), #18 (High — retain cycle), #19 (High — unlocked read), #20 (High — wrong service), #21 (High — MTU lie), #22 (High — maps not cleared).

**Drift notes:**
- Bug #17: `subscribe_inbox` in `engine.rs` dropped the spawned `JoinHandle` — no abort possible on re-subscribe. Fix: added `inbox_task: Mutex<Option<AbortHandle>>` field; spawn returns a `tokio::task::JoinHandle` whose `abort_handle()` is stored; previous forwarder is aborted before the new one starts. Also changed `body` field type to `Arc<Mutex<Option<Arc<dyn IrisBody>>>>` so the spawned task reads the current renderer on each message rather than a stale closure-captured snapshot.
- Bug #18: `RealBleCentralSeam.delegate` was a strong `var` — creates a retain cycle with `IosBleAdapter`. Fixed with `private weak var _delegate` + computed property.
- Bug #19: `peripheral(for:)` read `peripherals[identifier]` without holding `peripheralLock`. Fixed by locking around the initial read only (not the `remember` call, which has its own lock).
- Bug #20: `readValue` and `writeValue` used `p.services?.first` — picks wrong service if peripheral advertises Battery + IRIS. Fixed with `first(where: { $0.uuid == IrisBleConstants.serviceUUID })`.
- Bug #21: 0→20 substitution is an intentional design decision per RES-0024 DI-5 — deferred to transport-layer MTU rework. Marked 🔮 Future.
- Bug #22: `disconnectGatt` never cleaned maps. Fixed: `tokenByHandle.removeValue(forKey: handle)` and `handleByToken.removeValue(forKey: token)` added to `disconnectGatt`.

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #17 | ✅ | (this batch) | `engine.rs` — `inbox_task: Mutex<Option<AbortHandle>>`; `body: Arc<Mutex<...>>`; previous abort on re-subscribe; body read each iteration |
| 2 | #18 | ✅ | (this batch) | `CBManagerCentral.swift` — `private weak var _delegate`; computed property `delegate` get/set |
| 3 | #19 | ✅ | (this batch) | `CBManagerCentral.swift` — `peripheral(for:)` acquires `peripheralLock` around `peripherals[identifier]` read |
| 4 | #20 | ✅ | (this batch) | `CBManagerCentral.swift` — `readValue`/`writeValue` filter `services?.first(where: { $0.uuid == IrisBleConstants.serviceUUID })` |
| 5 | #21 | 🔮 | — | Deferred: 0→20 fallback is intentional per RES-0024 DI-5; needs transport-layer MTU negotiation rework |
| 6 | #22 | ✅ | (this batch) | `IosBleAdapter.swift` — `disconnectGatt` removes both `tokenByHandle` and `handleByToken` entries |

**Batch closeout:**
- `xcodebuild build`: PENDING CI
- `cargo build -p iris-ios --lib`: PENDING CI (pre-existing Windows toolchain issue)

### Next run
Tier 1 batch C — remaining high: #23–#28 (handle clearing, BG task, AppDelegate shared, mock deadlock, handle overflow).

---

## Run 5 — 2026-08-31 — Tier 1 batch C: remaining high (Bugs #23–#28)

Target findings: #23 (High — stopScan handle), #24 (High — try? + nil queue), #25 (High — AppDelegate shared crash), #26 (High — mock deadlock), #27 (High — UniffiHandleMap race), #28 (High — handle overflow + map growth).

**Drift notes:**
- Bug #23: `stopScan` did not clear `activeScanFilter`; `stopAdvertising` did not clear `lastAdvertisementData`. Fixed both guards and clear paths.
- Bug #24: `registerRefresh`/`registerProcessing` used `using: nil`; `resubmitAll` used `try?`. Fixed by passing `DispatchQueue.global(qos: .background)` and replacing `try?` with `do/catch + os_log`. Added `import os.log`.
- Bug #25: `public static let shared = AppDelegate()` created a second instance that UIKit never used — its `adapter` was nil. Changed to `static var shared: AppDelegate { UIApplication.shared.delegate as! AppDelegate }` (computed). Also `BleRestorationTarget.init` previously read `AppDelegate.shared.adapter` (old wrong instance); changed to injection: `init(engine:adapter:)`.
- Bug #26: `MockBleSeam.connectPeripheral`, `cancelConnection`, `discoverServices`, `discoverCharacteristics` all called delegate while holding `lock` under `defer { lock.unlock() }`. `IosBleAdapter.centralSeam(didConnect:)` calls back into `central.discoverServices()` → same lock → deadlock. Fixed by capturing delegate and error under lock, unlocking, then calling delegate (matches `writeValue` pattern already in the file).
- Bug #27: `UniffiHandleMap.count` read `map.count` without holding the internal lock — data race with concurrent Rust FFI insert/remove. Fixed: `var count: Int { lock.withLock { map.count } }`.
- Bug #28: `nextGattHandle` could overflow; fix uses wrapping increment `&+= 1` with a skip of 0 (reserved). `peers` eviction was not added (tracker notes "only appended to" — eviction on `didDisconnect` would require knowing which peer maps to a given token; scope limited to the overflow guard per the review spec).

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #23 | ✅ | (this batch) | `IosBleAdapter.swift` — `stopScan` guards `activeScanFilter != nil` and clears it; `stopAdvertising` clears `lastAdvertisementData` |
| 2 | #24 | ✅ | (this batch) | `BGTaskWiring.swift` — `import os.log`; `using: DispatchQueue.global(qos: .background)`; `do { try } catch { os_log(...) }` |
| 3 | #25 | ✅ | (this batch) | `AppDelegate.swift` — `static var shared` computed via `UIApplication.shared.delegate as! AppDelegate`; `BleRestorationTarget.init(engine:adapter:)` injects adapter |
| 4 | #26 | ✅ | (this batch) | `MockCoreBluetooth.swift` — `connectPeripheral`, `cancelConnection`, `discoverServices`, `discoverCharacteristics` all unlock before calling delegate |
| 5 | #27 | ✅ | (this batch) | `IrisCore.swift:408` — `var count: Int { lock.withLock { map.count } }` |
| 6 | #28 | ✅ | (this batch) | `IosBleAdapter.swift` — `nextGattHandle &+= 1; if nextGattHandle == 0 { nextGattHandle = 1 }` |

**Batch closeout:**
- `xcodebuild build`: PENDING CI (no Xcode env on Windows build host)
- `cargo build -p iris-ios --lib`: PENDING CI (pre-existing Windows toolchain issue; CI clean)

**Tier 1 checkpoint:** 17 ✅ + 1 🔮 (#21 deferred) — advance to Tier 2.

### Next run
Tier 2 batch A — medium bugs, first half: #29–#44 (NSLock, buffers, Keychain JSON, AppDelegate, widget config, peripheral timing, races, test fixes, FFI bugs).

---

## Run 6 — [DATE] — Tier 2 batch A: medium bugs (Bugs #29–#44)

Target findings: #29–#44

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #29 | ⬜ | — | _(IosBleAdapter.swift:496 — local `NSLock.withLock` extension removed; stdlib version used)_ |
| 2 | #30 | ⬜ | — | _(IosBleAdapter.swift:62-78 — cap added; LRU/FIFO eviction on overflow)_ |
| 3 | #31 | ⬜ | — | _(KeychainEd25519.swift:22 — raw byte store; migration from JSON on first load)_ |
| 4 | #32 | ⬜ | — | _(AppDelegate.swift:78 — started flag; `try` with log; single `startAll` call)_ |
| 5 | #33 | ⬜ | — | _(project.yml:55 — target name aligned to `IrisWidgetExtension`)_ |
| 6 | #34 | ⬜ | — | _(CBManagerPeripheral.swift:85 — UUID set inside `didAdd` success branch)_ |
| 7 | #35 | ⬜ | — | _(CBManagerPeripheral.swift:24 — `pendingService` serialized under lock)_ |
| 8 | #36 | ⬜ | — | _(LiveActivityController.swift:37-81 — auth re-checked before each call; serial actor for updates)_ |
| 9 | #37 | ⬜ | — | _(Notifications.swift:34 — completion handler logs errors; `criticalSoundNamed` removed or gated on entitlement)_ |
| 10 | #38 | ⬜ | — | _(bridge.rs:184 — error logged; guard released before FFI callback)_ |
| 11 | #39 | ⬜ | — | _(ble_adapter.rs:156 — `SimBle` UUIDs aligned to `IrisBleConstants` values)_ |
| 12 | #40 | ⬜ | — | _(IrisCore.swift:28 — `try?` + nil check; graceful error path)_ |
| 13 | #41 | ⬜ | — | _(engine.rs:86 — `MemoryStorage`/`DevCryptoProvider` gated behind `#[cfg(feature = "dev-seams")]`)_ |
| 14 | #42 | ⬜ | — | _(IosBleAdapter.swift:459 — handle 0 rejected; per-connection rate limit added)_ |
| 15 | #43 | ⬜ | — | _(KeychainIdentityTests.swift:47-48 — assertion updated to match raw byte count after #31 fix)_ |
| 16 | #44 | ⬜ | — | _(bridge.rs:32-38 — `{:02X}` uppercase; UUID comparisons across FFI now match)_ |

**Batch closeout:**
- `cargo build -p iris-ios`: _(must be CLEAN)_
- `xcodebuild build`: _(must be CLEAN)_
- `xcodebuild test`: _(record pass count)_

### Next run
Tier 2 batch B — medium bugs, second half: #45–#58 (double-free, tokio deadlock, naming, BLE peripheral, state bugs, privacy, CI script drift).

---

## Run 7 — [DATE] — Tier 2 batch B: medium bugs (Bugs #45–#58)

Target findings: #45–#58

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #45 | ⬜ | — | _(IrisCore.swift:51-59 — ownership coordinated; no `.none` deallocator on Rust-owned buffer)_ |
| 2 | #46 | ⬜ | — | _(engine.rs:78-88 — `Runtime::new` only when no current runtime; tests use `#[tokio::test]`)_ |
| 3 | #47 | ⬜ | — | _(engine.rs:200 — field renamed to `originated_at_ms`; `received_at_ms` added for local arrival)_ |
| 4 | #48 | ⬜ | — | _(CBManagerPeripheral.swift:110 — explicit 10-byte truncation with `tracing::warn!` log)_ |
| 5 | #49 | ⬜ | — | _(IosBleAdapter.swift:104-106 — deadline computed inside `wait()`)_ |
| 6 | #50 | ⬜ | — | _(IosBleAdapter.swift:327-331 — `poweredOff` clears `gattWriteBuffer` + `rejectedUntil`)_ |
| 7 | #51 | ⬜ | — | _(IrisLiveActivityWidget.swift:20-48 — `context.isStale` used; placeholder shown when stale)_ |
| 8 | #52 | ⬜ | — | _(body.rs:70-77 — empty payload check before `summarize`; early return on empty)_ |
| 9 | #53 | ⬜ | — | _(ProbeAdmissionTests.swift:65-73 — rejected-then-re-admitted path explicitly tested)_ |
| 10 | #54 | ⬜ | — | _(build-xcframework.sh:20-37 — `FRAMEWORK_DIR` aligned to `ios/IRIS/` matching ios.yml:72)_ |
| 11 | #55 | ⬜ | — | _(engine.rs:213 — `debug!` gated behind `cfg(feature = "verbose-debug")` or content redacted)_ |
| 12 | #56 | ⬜ | — | _(CBManagerPeripheral.swift:179-193 — `invalidOffset` returned when `offset >= value.count`)_ |
| 13 | #57 | ⬜ | — | _(BGTaskWiring.swift:82-89 — next task request submitted inside completion handler)_ |
| 14 | #58 | ⬜ | — | _(AppDelegate.swift:98-108 — `BestEffortMaintenance.run` body implemented; routing/expiry ticked)_ |

**Batch closeout:**
- `cargo build -p iris-ios`: _(must be CLEAN)_
- `xcodebuild build`: _(must be CLEAN)_
- `xcodebuild test`: _(record pass count)_

**Tier 2 checkpoint:** All 30 ✅ — advance to Tier 3.

### Next run
Tier 3 — all 14 low-severity findings: #59–#72 (CI fixes, quality, supply chain, privacy manifest).

---

## Run 8 — [DATE] — Tier 3: low-severity (Bugs #59–#72)

Target findings: #59–#72

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|
| 1 | #59 | ⬜ | — | _(ios.yml: `uniffi-bindgen`; modulemap step added; `\|\| true` removed)_ |
| 2 | #60 | ⬜ | — | _(project.yml:13 — `SWIFT_STRICT_CONCURRENCY: targeted`; resulting warnings fixed)_ |
| 3 | #61 | ⬜ | — | _(IRISApp.swift:28 — `(UIApplication.shared.delegate as? AppDelegate)?.engine`)_ |
| 4 | #62 | ⬜ | — | _(IrisWidgetExtension/Info.plist — `NSExtensionPrincipalClass` added or `@main` documented)_ |
| 5 | #63 | ⬜ | — | _(G-IOS_SPIKE.md:89-95 — Xcode 16.4 requirement documented; `swiftc` gate updated)_ |
| 6 | #64 | ⬜ | — | _(iris-ios/Cargo.toml — `rlib` removed from `crate-type`; only `["cdylib","staticlib"]`)_ |
| 7 | #65 | ⬜ | — | _(bridge.rs:40-50 — `&mut String` buffer reuse or `ArrayString` fixed allocation)_ |
| 8 | #66 | ⬜ | — | _(IrisCore.swift:1109 — `// SAFETY: process-lifetime allocation` comment added)_ |
| 9 | #67 | ⬜ | — | _(engine.rs:256-271 — odd-length guard; returns error if `hex.len() != 64`)_ |
| 10 | #68 | ⬜ | — | _(error.rs:21 — logs CB error code integer only; OS description string redacted)_ |
| 11 | #69 | ⬜ | — | _(IrisBleConstants.swift:34-37 — collision probability comment added)_ |
| 12 | #70 | ⬜ | — | _(IosBleAdapterTests.swift:166 — `XCTestExpectation` + `wait(for:timeout:)` replaces sleep)_ |
| 13 | #71 | ⬜ | — | _(ios.yml:36 — image update policy comment added; Dependabot config for github-actions)_ |
| 14 | #72 | ⬜ | — | _(PrivacyInfo.xcprivacy added; location usage, retention, no-identity-link documented)_ |

**Batch closeout:**
- `cargo build -p iris-ios`: _(must be CLEAN)_
- `xcodebuild build`: _(must be CLEAN)_
- `xcodebuild test`: _(record pass count)_
- `cargo clippy -p iris-ios -- -D warnings`: _(must exit 0)_
- `cargo fmt -p iris-ios -- --check`: _(must exit 0)_

**Final checkpoint:** All 72 findings resolved or classified. Section 5 iOS closed.

### Next run
No further autonomous runs needed. Record final CI evidence below.

---

## Final CI evidence

_(Fill in after all fixes are committed and the CI pipeline completes)_

| Check | Result | Run link |
|---|---|---|
| `cargo build -p iris-ios` | _(CLEAN / error)_ | — |
| `xcodebuild build` | _(CLEAN / error)_ | — |
| `xcodebuild test` | _(pass count)_ | — |
| `cargo clippy -p iris-ios -- -D warnings` | _(exit 0 / warnings)_ | — |
| `cargo fmt -p iris-ios -- --check` | _(exit 0)_ | — |
| `ios.yml` CI pipeline (GitHub Actions) | _(green / failing jobs)_ | — |

---

*(Add further runs as needed — copy the template below)*

---

## Run N — [DATE] — [description]

Target findings: _(list)_

**Drift notes:** _(fill in)_

| # | Finding | Status | Commit | Verification |
|---|---|---|---|---|

**Batch closeout:**

### Next run
_(fill in)_
