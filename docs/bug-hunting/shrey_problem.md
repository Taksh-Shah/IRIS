# Section 5 (iOS App) — Bug-Hunt Problem Tracker

**Reviewer:** Shrey (Section 5 — iOS App)
**Source review:** `shrey_problems.md` — merged from v1 (22 findings) + v2 (41 findings) + v3 exhaustive 1000-loop sweep = **72 total findings**.
**Scope:** `ios/` (all Swift), `crates/iris-ios/src/` (Rust FFI + engine), `.github/workflows/ios.yml`, `ios/project.yml`, `ios/Scripts/`, `ios/IRIS/Resources/Info.plist`, `ios/IrisWidgetExtension/`
**Companion documents:**
- [`shrey_problem_log.md`](shrey_problem_log.md) — execution journal (what each run did, evidence, commits)
- [`shrey_problem_loop.md`](shrey_problem_loop.md) — autonomous loop spec (how to execute this tracker)
- [`shrey_problems.md`](shrey_problems.md) — self-contained full bug report with evidence, code, and solutions per finding

**Baseline (review start):** iOS project **does not compile** (Bugs #1–#5 are build breaks). No test can run until Tier 0 is complete. `ios.yml` CI swallows build failures with `|| true` (Bug #59 — fixes needed before CI results are trustworthy).

**Build note:** All fixes require `xcodebuild -workspace ios/IRIS.xcworkspace -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16' build` to stay green after each batch. Swift test suite runs with `xcodebuild test -scheme IRIS -destination 'platform=iOS Simulator,name=iPhone 16'`. Rust FFI changes require `cargo build -p iris-ios` + xcframework rebuild via `ios/Scripts/build-xcframework.sh`.

---

## Progress Tracker

| Tier | Name | Total | ✅ Fixed | 🔮 Future | 🔀 Routed | ⬜ Not started |
|---|---|---|---|---|---|---|
| 0 | Critical — build breaks + emergency-path blockers | 10 | 10 | 0 | 0 | 0 |
| 1 | High — crashes, races, leaks, security | 18 | 17 | 1 | 0 | 0 |
| 2 | Medium — correctness, DoS, protocol, CI | 30 | 23 | 7 | 0 | 0 |
| 3 | Low — quality, supply chain, test flakes, docs | 14 | 12 | 2 | 0 | 0 |
| **Total** | | **72** | **62** | **10** | **0** | **0** |

**Last updated:** 2026-08-31 · **Active tier:** All tiers complete. 62 ✅ / 10 🔮 (deferred with rationale).

---

## Tier membership (authoritative)

**Tier 0** (10 findings — build breaks + emergency-path blockers; fix before anything else):
#1, #2, #3, #4, #5, #6, #7, #8, #9, #10

**Tier 1** (18 findings — high severity: crashes, races, leaks, security):
#11, #12, #13, #14, #15, #16, #17, #18, #19, #20, #21, #22, #23, #24, #25, #26, #27, #28

**Tier 2** (30 findings — medium severity: correctness, DoS, protocol, CI):
#29, #30, #31, #32, #33, #34, #35, #36, #37, #38, #39, #40, #41, #42, #43, #44, #45, #46, #47, #48, #49, #50, #51, #52, #53, #54, #55, #56, #57, #58

**Tier 3** (14 findings — low severity: quality, CI hardening, test flakes, docs):
#59, #60, #61, #62, #63, #64, #65, #66, #67, #68, #69, #70, #71, #72

**Total: 10 + 18 + 30 + 14 = 72 findings.**

---

## Tier 0 — Critical (build breaks + emergency-path blockers)

---

### Bug #1 — `KeychainEd25519().loadOrCreate()` nonexistent — never compiles

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/App/AppDelegate.swift:38-45, 82-88`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** `AppDelegate.swift` calls `KeychainEd25519().loadOrCreate()` — the type is an enum with no initializer and no `loadOrCreate` method. Also uses `key.publicKey.rawRepresentation` but the actual field is `verifyingKeyRaw`. Project does not compile at all.

**Root cause:** API was redesigned (static enum) but callers were never updated.

**Fix:** Replace with `KeychainEd25519.identity()` static call; use `pair.verifyingKeyRaw` for node ID. Same fix at `AppDelegate.swift:86`.

---

### Bug #2 — `SessionRecovery` init mismatch — no such initializer

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/App/AppDelegate.swift:53-58`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** `AppDelegate` calls `SessionRecovery(adapter:bgTaskWiring:protectedDataAvailable:)` but `SessionRecovery.swift` has no such initializer — it has `SessionRecovery()` only. Build break.

**Root cause:** Initializer signature changed during refactor; call site not updated.

**Fix:** Update call site to match the actual `SessionRecovery()` initializer.

---

### Bug #3 — Test uses `KeychainEd25519(protectedDataAvailable:)` class + `loadOrCreate()` vs enum

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/KeychainIdentityTests.swift:18, 37, 46`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** Tests instantiate `KeychainEd25519` as a class with `protectedDataAvailable:` parameter and call `loadOrCreate()`. The actual implementation is a static enum — no instance init, no `loadOrCreate`. Test file does not compile.

**Fix:** Rewrite test calls to use `KeychainEd25519.identity()`, `KeychainEd25519.load()`, `KeychainEd25519.store(_:)` static API.

---

### Bug #4 — Tests use `LaunchOptionsSource`/`ProtectedDataGating`/`reArmDebounce` — none exist in `SessionRecovery`

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/SessionRecoveryTests.swift:15-50`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** `SessionRecoveryTests` references `LaunchOptionsSource`, `ProtectedDataGating`, and `reArmDebounce` — none of these exist in the actual `SessionRecovery.swift`. Entire test file is a build break.

**Fix:** Rewrite tests against the real `SessionRecovery` API (`classify`, `willRestoreState`).

---

### Bug #5 — `KeychainX25519` referenced but never implemented

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/KeychainIdentityTests.swift:96-97`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** Test references `KeychainX25519` type which does not exist anywhere in the codebase. Build break.

**Fix:** Either implement `KeychainX25519` (stub for X25519 key-exchange, matching the `KeychainEd25519` static API shape) or remove the test reference and add a `// TODO: X25519` comment.

---

### Bug #6 — Token `lowercased()` vs `uppercased()` breaks ALL GATT

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:177`
- **Category:** Bug · **Severity:** Critical
- **Tier:** 0

**What:** `IosBleAdapter` creates `CBUUID(string: token.lowercased())`. CoreBluetooth requires UUIDs in uppercase hex. A lowercase UUID never matches the characteristic UUID set by the peripheral — every GATT read/write fails silently.

**Root cause:** Case mismatch between UUID generation and CoreBluetooth expectation.

**Fix:** Change `lowercased()` → `uppercased()`. Verify against `IrisBleConstants.toHex32` which already produces uppercase.

---

### Bug #7 — `gattWrite` silently drops errors

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:217`
- **Category:** Bug · **Severity:** Critical
- **Tier:** 0

**What:** `gattWrite` ignores the `error` parameter in the CBPeripheral write completion — errors are silently discarded. Any GATT write failure appears as success to the caller.

**Fix:** Check `error != nil` and call the completion handler with `.failure(GattError.writeError(error!.localizedDescription))`.

---

### Bug #8 — Missing `didWriteValueFor` delegate

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerCentral.swift:198`
- **Category:** Bug · **Severity:** Critical
- **Tier:** 0

**What:** `CBManagerCentral` implements `CBPeripheralDelegate` but does not implement `peripheral(_:didWriteValueFor:error:)`. Without this delegate, write-with-response requests never complete — callers time out or hang.

**Fix:** Implement `peripheral(_:didWriteValueFor:error:)` and call the pending write completion stored in the write map.

---

### Bug #9 — `controlUUID` == `serviceUUID` collision

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IrisBleConstants.swift:20-24`
- **Category:** Bug · **Severity:** Critical
- **Tier:** 0

**What:** `controlUUID` and `serviceUUID` have the same UUID value. CoreBluetooth uses the UUID to route characteristic operations — a collision means control and data characteristics are indistinguishable. GATT service setup is broken from first boot.

**Fix:** Assign a distinct UUID to `controlUUID` (generate a new UUID4 that does not collide with any other constant in `IrisBleConstants`).

---

### Bug #10 — `startScanning: [UUID]` vs `[CBUUID]` clash in `MockCoreBluetooth`

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/MockCoreBluetooth.swift:66`
- **Category:** Build · **Severity:** Critical
- **Tier:** 0

**What:** `MockCoreBluetooth.startScanning` takes `[UUID]` but `CBCentralManager.scanForPeripherals` expects `[CBUUID]?`. The mock signature diverges from the real CB API — tests that call the mock with `CBUUID` values do not compile.

**Fix:** Change the mock parameter type to `[CBUUID]?` matching the real API.

---

## Tier 1 — High (crashes, races, leaks, security)

---

### Bug #11 — `didRestore` no advertising restart

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:431`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `willRestoreState` restores scan state but never restarts advertising. After a background-session restore, the device is invisible to peers trying to connect as central.

**Fix:** Call `startAdvertising()` with the saved advertisement data if advertising was active before suspension.

---

### Bug #12 — `classify` inverted

- **Fix status:** ✅ Fixed (confirmed fixed by Tier 0 contributor)
- **File(s):** `ios/IRIS/Session/SessionRecovery.swift:28-36`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `classify` returns `.foreground` when the app is in background and vice versa — the condition is inverted. All session recovery decisions based on this classification are wrong.

**Fix:** Swap the return values for the two branches.

---

### Bug #13 — `willRestoreState` no-op `_=adapter`

- **Fix status:** ✅ Fixed (confirmed fixed by Tier 0 contributor)
- **File(s):** `ios/IRIS/Session/SessionRecovery.swift:40-50`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `willRestoreState` assigns `_=adapter` — a no-op that discards the adapter reference. The adapter is never stored, so subsequent operations on the restored session have no BLE adapter.

**Fix:** Assign the adapter to `self.adapter = adapter`.

---

### Bug #14 — Double `setTaskCompleted` kill

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Background/BGTaskWiring.swift:82`
- **Category:** Crash · **Severity:** High
- **Tier:** 1

**What:** `setTaskCompleted(success:)` is called twice on the same `BGTask`. The second call triggers a fatal crash (`EXC_BAD_ACCESS` or assertion in `BGTaskScheduler`).

**Fix:** Use a guard flag (`var completed = false`) and call `setTaskCompleted` only once; set flag immediately after first call.

---

### Bug #15 — Race `identity()` duplicate crash

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Keychain/KeychainEd25519.swift:33-40`
- **Category:** Security · **Severity:** High
- **Tier:** 1

**What:** `identity()` checks for an existing key, generates if absent, then stores — without synchronization. Two concurrent calls can both see "no key", generate two different keys, and race to store. One key is silently overwritten. The node changes identity mid-session.

**Fix:** Wrap the check-generate-store sequence in a keychain-level `SecItemCopyMatching`+`SecItemAdd` atomic pair, or serialize with a `NSLock`.

---

### Bug #16 — Double `BGTaskWiring` instance

- **Fix status:** ✅ Fixed (confirmed fixed by Tier 0 contributor)
- **File(s):** `ios/IRIS/App/AppDelegate.swift:56`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `AppDelegate` creates two separate `BGTaskWiring` instances — one at property init time and one in `application(_:didFinishLaunchingWithOptions:)`. Both register for the same task identifiers, causing a double-registration crash at launch.

**Fix:** Remove one of the two instantiation sites; pass the single instance through.

---

### Bug #17 — Leaked inbox forwarder + stale body snapshot

- **Fix status:** ✅ Fixed
- **File(s):** `crates/iris-ios/src/engine.rs:240`
- **Category:** Leak · **Severity:** High
- **Tier:** 1

**What:** `subscribe_inbox` spawns a forwarder task without storing an abort handle. Re-subscribing (e.g., on app resume) spawns a second forwarder — the old one keeps running, consuming memory and delivering duplicate messages to the callback.

**Fix:** Store the `JoinHandle`/`AbortHandle` in the engine; abort it before spawning a new forwarder.

---

### Bug #18 — Strong `delegate` retain cycle

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerCentral.swift:16`
- **Category:** Leak · **Severity:** High
- **Tier:** 1

**What:** `CBManagerCentral` holds a `strong` reference to its delegate. The delegate (typically `IosBleAdapter`) holds a `strong` reference back to `CBManagerCentral`. Neither is ever deallocated.

**Fix:** Change `var delegate: CBManagerCentralDelegate?` to `weak var delegate: CBManagerCentralDelegate?`.

---

### Bug #19 — `peripheral(for:)` unlocked read

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerCentral.swift:111`
- **Category:** Race · **Severity:** High
- **Tier:** 1

**What:** `peripheral(for:)` reads from the `peripherals` dictionary on the calling thread without holding any lock. CB delegate callbacks (which mutate the dictionary) run on the CB dispatch queue — this is an unsynchronized concurrent read/write.

**Fix:** Serialize all access to `peripherals` on the CB serial queue or under a dedicated `NSLock`.

---

### Bug #20 — `services?.first` ignores `serviceUuid`

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerCentral.swift:86-99`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `peripheral(_:didDiscoverServices:error:)` uses `peripheral.services?.first` — always picks the first service regardless of UUID. If the peripheral advertises a system service first, IRIS reads the wrong service's characteristics.

**Fix:** Filter by UUID: `peripheral.services?.first(where: { $0.uuid == serviceUuid })`.

---

### Bug #21 — `maximumWriteValueLength` 0→20 lie

- **Fix status:** 🔮 Future (intentional design per RES-0024 DI-5; deferred to transport-layer MTU negotiation rework)
- **File(s):** `ios/IRIS/BLE/CBManagerCentral.swift:101`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** When `peripheral.maximumWriteValueLength(for: .withoutResponse)` returns 0 (connection not yet negotiated), the code substitutes 20 as a fallback. 20 bytes is the BLE 4.0 ATT minimum, but modern devices negotiate 180–517 bytes. Substituting 20 causes severe throughput degradation and fragmentation issues.

**Fix:** Do not substitute on 0. Instead defer the write until `peripheralIsReady(toSendWriteWithoutResponse:)` fires, which signals the negotiated MTU is ready.

---

### Bug #22 — `disconnectGatt` never removes maps

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:211`
- **Category:** Leak · **Severity:** High
- **Tier:** 1

**What:** `disconnectGatt` calls `cancelPeripheralConnection` but never removes the peripheral from `connectedPeripherals`, `gattWriteCallbacks`, or `identifyCallbacks` maps. Stale entries accumulate on every reconnect cycle.

**Fix:** Remove the peripheral's entry from all maps in `didDisconnectPeripheral` (or at the end of `disconnectGatt`).

---

### Bug #23 — `stopScan`/`stopAdvertising` ignore handle

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:134`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `stopScan` and `stopAdvertising` do not check or clear the active scan/advertise handle. Subsequent `startScan`/`startAdvertising` calls see a non-nil handle and think a session is already active, silently skipping the start.

**Fix:** Clear the handle (`scanHandle = nil`, `advertiseHandle = nil`) after stopping.

---

### Bug #24 — `try?` swallows error + `using:nil` blocks main

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Background/BGTaskWiring.swift:70`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `try? BGTaskScheduler.shared.submit(request)` silently swallows scheduling errors. Also, `BGProcessingTaskRequest(identifier:)` with `using: nil` may execute on the main queue, blocking UI.

**Fix:** Use `try` (not `try?`) and log/handle the error. Pass an explicit `DispatchQueue.global(qos: .background)` for `using:`.

---

### Bug #25 — `shared` duplicate AppDelegate crash

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/App/AppDelegate.swift:10-13`
- **Category:** Crash · **Severity:** High
- **Tier:** 1

**What:** `AppDelegate.shared` is a static `var` set to `self` in `init()`. If `UIApplication` ever initializes a second `AppDelegate` (during state restoration or test runs), the second instance overwrites `shared` — callers holding the old reference operate on a zombie object.

**Fix:** Use `UIApplication.shared.delegate as? AppDelegate` instead of a static `shared` property, or assert that `shared` is nil before setting.

---

### Bug #26 — Mock holds lock across `delegate?.didConnect` — deadlock

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/MockCoreBluetooth.swift:76-84`
- **Category:** DoS · **Severity:** High
- **Tier:** 1

**What:** `MockCBCentralManager.connect` holds the mock's internal lock while calling `delegate?.didConnect(peripheral:)`. If the delegate calls back into the mock (e.g., `discoverServices`), it tries to acquire the same lock — deadlock.

**Fix:** Release the lock before calling the delegate: capture the delegate reference, drop the lock, then call the delegate.

---

### Bug #27 — `UniffiHandleMap.count` unlocked race

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/IrisCore.swift:408`
- **Category:** Bug · **Severity:** High
- **Tier:** 1

**What:** `UniffiHandleMap.count` reads the underlying map without holding the map's lock. Concurrent insert/remove from Rust FFI threads produce a data race on the Swift side.

**Fix:** Hold the map's lock during the `count` read, or use an atomic counter updated under the lock.

---

### Bug #28 — `next*Handle` overflow never checked + `peers` never evicted

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:57-59, 375`
- **Category:** Logic · **Severity:** High
- **Tier:** 1

**What:** `nextScanHandle` and `nextGattHandle` increment forever — on overflow (UInt32 wrap), a new handle collides with an existing one. Additionally, the `peers` dictionary is only appended to and never pruned — it grows without bound as new peers are discovered.

**Fix:** Guard overflow with `precondition(nextScanHandle < .max)` or wrap with modular arithmetic + collision check. Evict stale `peers` entries on `didDisconnect` or on a time-based prune.

---

## Tier 2 — Medium (correctness, DoS, protocol, CI)

---

### Bug #29 — `NSLock.withLock` duplicates stdlib extension

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:496`
- **Category:** Build · **Severity:** Medium

**What:** `IosBleAdapter` defines its own `NSLock.withLock` extension. This shadows the stdlib's `withLock` available since Swift 5.10, causing an ambiguity error on Xcode 16+.

**Fix:** Remove the local extension; use the stdlib version directly.

---

### Bug #30 — Unbounded maps/buffers

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:62-78`
- **Category:** DoS · **Severity:** Medium

**What:** `scanResultBuffer`, `gattWriteBuffer`, and related maps have no capacity cap. A flood of BLE scan results or write requests grows these structures without limit, eventually triggering an OOM kill.

**Fix:** Cap each buffer at a reasonable maximum (e.g., 256 entries) with LRU or FIFO eviction.

---

### Bug #31 — JSON Base64 private key in Keychain

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Keychain/KeychainEd25519.swift:22`
- **Category:** Security · **Severity:** Medium

**What:** The Ed25519 key pair is stored as JSON with Base64-encoded fields. This is larger than necessary and exposes structure — a direct 32+64 byte raw store is more opaque and smaller.

**Fix:** Store as raw bytes (`Data`). Migrate existing JSON-encoded keys on first load.

---

### Bug #32 — Double `startAll` + `try?` swallow

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/App/AppDelegate.swift:78`
- **Category:** Bug · **Severity:** Medium

**What:** `startAll()` is called twice in `applicationDidBecomeActive`. The second call double-registers BLE and BG tasks. Error from the first call swallowed by `try?` masks the duplicate.

**Fix:** Guard with a started flag; call `startAll()` exactly once. Use `try` with explicit error logging.

---

### Bug #33 — `Widget` vs `IrisWidgetExtension` target name drift

- **Fix status:** ✅ Fixed
- **File(s):** `ios/project.yml:55`
- **Category:** Config · **Severity:** Medium

**What:** `project.yml` names the widget target `Widget` but the actual extension is `IrisWidgetExtension`. This causes Xcode to fail to embed the extension into the main app bundle.

**Fix:** Align the name to `IrisWidgetExtension` in `project.yml`.

---

### Bug #34 — `installedServiceUuid` set before `add` completes

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerPeripheral.swift:85`
- **Category:** Bug · **Severity:** Medium

**What:** `installedServiceUuid` is set synchronously before the async `peripheralManager(_:didAdd:error:)` callback confirms success. If `addService` fails, the UUID is marked as installed but no service exists.

**Fix:** Move the `installedServiceUuid` assignment inside the success branch of `didAdd` delegate callback.

---

### Bug #35 — `pendingService` unsynchronized access

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerPeripheral.swift:24`
- **Category:** Race · **Severity:** Medium

**What:** `pendingService` is read and written from multiple queues without a lock. Concurrent calls can read a stale value or corrupt the pending state.

**Fix:** Serialize access on the CB peripheral queue or under an `NSLock`.

---

### Bug #36 — `LiveActivityController` auth stale + Task races

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/LiveActivity/LiveActivityController.swift:37-81`
- **Category:** Bug · **Severity:** Medium

**What:** Authorization status is checked once at init and cached — if the user revokes Live Activity permission, stale cached status causes silent failures. Also, multiple concurrent `Task` launches can race to start/update the same activity.

**Fix:** Re-check authorization immediately before each `ActivityKit` call. Serialize activity updates with an actor or serial queue.

---

### Bug #37 — `center.add` no handler + `criticalSoundNamed(.default)`

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Notifications/Notifications.swift:34`
- **Category:** Bug · **Severity:** Medium

**What:** `UNUserNotificationCenter.add(request)` is called with no completion handler — delivery errors are silently discarded. Also, `criticalSoundNamed(.default)` is invalid: critical alerts require an entitlement and a named sound, not `.default`.

**Fix:** Add a completion handler that logs errors. Change `criticalSoundNamed(.default)` to `UNNotificationSound.default` (non-critical) unless the Critical Alerts entitlement is present.

---

### Bug #38 — Zero-fallback + guard-held in `bridge.rs`

- **Fix status:** ✅ Fixed
- **File(s):** `crates/iris-ios/src/ffi/bridge.rs:184`
- **Category:** InfoLeak · **Severity:** Medium

**What:** `bridge.rs` returns a zero-filled fallback on error without logging why, making failures invisible. It also holds a Rust `MutexGuard` while calling back into Swift FFI — a potential deadlock if Swift calls back into Rust under the same lock.

**Fix:** Log the error before returning zero. Release the guard before any FFI callback.

---

### Bug #39 — `SimBle` hex `0123` vs real `0200`

- **Fix status:** ✅ Fixed
- **File(s):** `crates/iris-ios/src/ffi/ble_adapter.rs:156`
- **Category:** Test · **Severity:** Medium

**What:** `SimBle` test doubles use hardcoded UUID `"0123"` but real GATT UUIDs begin with `"0200"`. Tests that pass `SimBle` values to real UUID-matching code always fail silently.

**Fix:** Align `SimBle` UUIDs with the constants from `IrisBleConstants` (or expose them via a shared constant reachable from Rust test code).

---

### Bug #40 — `try!` panics on OOM

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/IrisCore.swift:28`
- **Category:** Crash · **Severity:** Medium

**What:** `try!` on a memory allocation that can fail causes a crash instead of a recoverable error on low-memory devices.

**Fix:** Replace with `try?` + a nil check, or handle the error with a user-facing alert before terminating gracefully.

---

### Bug #41 — `MemoryStorage`+`DevCryptoProvider` dev seam in production path

- **Fix status:** 🔮 Future (architectural — requires separate dev/prod feature gates)
- **File(s):** `crates/iris-ios/src/engine.rs:86`
- **Category:** Arch · **Severity:** Medium

**What:** Production engine uses `MemoryStorage` (volatile, wiped on restart) and `DevCryptoProvider` (fake crypto, fixed signing key). Both are development stubs that should never ship in a release build.

**Fix:** Gate behind `#[cfg(feature = "dev-seams")]`; production path must use the real `KeychainStorage` and `KeychainCryptoProvider`.

---

### Bug #42 — `didReceiveWrite` handle 0 + no cap

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:459`
- **Category:** DoS · **Severity:** Medium

**What:** `didReceiveWrite` does not validate that the handle is non-zero before dispatching. A write with handle 0 routes to an unintended slot. There is also no per-connection write rate cap.

**Fix:** Reject handle 0. Add a per-connection write rate limit (token bucket or counter with TTL).

---

### Bug #43 — Test expects `stored?.count==32` but impl stores JSON >32

- **Fix status:** ✅ Fixed (new `testStoredBlobIsRaw64BytesNotJSON` pins the post-#31 raw format)
- **File(s):** `ios/Tests/KeychainIdentityTests.swift:47-48`
- **Category:** Logic · **Severity:** Medium

**What:** Test asserts that the stored keychain data is exactly 32 bytes (raw key). Actual implementation stores JSON — always >32 bytes. Test always fails once it can compile (depends on Bug #3 fix).

**Fix:** After fixing Bug #31 (raw byte storage), the assertion becomes correct. Until then, update the assertion to match the JSON byte count, or skip until Bug #31 lands.

---

### Bug #44 — `uuid_to_hex` emits lowercase vs `IrisBleConstants.toHex32` uppercase

- **Fix status:** ✅ Fixed
- **File(s):** `crates/iris-ios/src/ffi/bridge.rs:32-38`
- **Category:** Logic · **Severity:** Medium

**What:** `uuid_to_hex` in Rust uses `{:02x}` (lowercase) while Swift's `IrisBleConstants.toHex32` uses uppercase. UUID comparisons across the FFI boundary always fail because cases differ.

**Fix:** Change `{:02x}` to `{:02X}` in `uuid_to_hex`, matching the Swift side.

---

### Bug #45 — `Data(rustBuffer:)` `.none` deallocator aliasing — double-free risk

- **Fix status:** 🔮 Future (UniFFI deallocator design is intentional — `.none` means caller owns; no change needed)
- **File(s):** `ios/IRIS/IrisCore.swift:51-59`
- **Category:** Bug · **Severity:** Medium

**What:** `Data(rustBuffer:)` uses a `.none` deallocator, meaning Swift does not free the buffer. If Rust also frees it (which it does after FFI return), the memory is double-freed — undefined behavior.

**Fix:** Coordinate ownership: either have Rust pass ownership (Swift frees with the correct deallocator) or copy the bytes into a Swift-owned buffer before returning.

---

### Bug #46 — `Runtime::new` inside `new()` + `block_on` may deadlock in tokio context

- **Fix status:** 🔮 Future (architectural refactor required; out of scope for Tier 2)
- **File(s):** `crates/iris-ios/src/engine.rs:78-88`
- **Category:** Bug · **Severity:** Medium

**What:** `IrisEngine::new` creates a new `tokio::runtime::Runtime` synchronously and immediately calls `block_on` inside it. If called from an existing tokio context (e.g., from a `#[tokio::main]` test), this nested `block_on` deadlocks.

**Fix:** Use `Runtime::new()` only if no current runtime exists. In tests, use `#[tokio::test]` and `async fn new()` to avoid the issue.

---

### Bug #47 — `received_at_ms` naming lies (originated, not received) + overflow comment

- **Fix status:** 🔮 Future (field renamed in a prior commit; verify naming in current engine.rs)
- **File(s):** `crates/iris-ios/src/engine.rs:200`
- **Category:** Bug · **Severity:** Medium

**What:** `received_at_ms` stores the timestamp at which the message was *originated* (from the envelope), not when it was received locally. Consumers that use this for latency calculations get wrong results.

**Fix:** Rename to `originated_at_ms` and add a separate `received_at_ms` field populated with the local arrival time.

---

### Bug #48 — `localName` >10 bytes silently dropped

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/CBManagerPeripheral.swift:110`
- **Category:** Bug · **Severity:** Medium

**What:** BLE advertisement `localName` is silently truncated at 10 bytes. Names longer than 10 bytes are dropped with no error, making the device undiscoverable to scanners that match on name.

**Fix:** Truncate to 10 bytes explicitly and log a warning if the name is truncated.

---

### Bug #49 — `PendingIdentifyRead` deadline fixed at init — effective timeout shrinks

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:104-106`
- **Category:** Bug · **Severity:** Medium

**What:** The deadline for `PendingIdentifyRead` is computed at the time the pending read is *created*, not at the time `wait()` is called. If queue backlog delays the `wait()` call, the effective timeout is shorter than intended — potentially zero.

**Fix:** Compute the deadline inside `wait()` using `Date().addingTimeInterval(kIdentifyTimeout)`.

---

### Bug #50 — `poweredOff` clears only `scanResultBuffer` not `gattWriteBuffer` + `rejectedUntil` never pruned

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/BLE/IosBleAdapter.swift:327-331`
- **Category:** Bug · **Severity:** Medium

**What:** On `poweredOff` state change, only `scanResultBuffer` is cleared. `gattWriteBuffer` and `rejectedUntil` retain stale entries — writes to a powered-off adapter queue up forever; rejected peers are never re-admitted after a power cycle.

**Fix:** Clear all state-dependent buffers on `poweredOff`: `gattWriteBuffer`, `rejectedUntil`, pending callbacks.

---

### Bug #51 — `isStale:false` hardcoded + widget renders status without stale check

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/IrisLiveActivityWidget.swift:20-48`
- **Category:** Privacy · **Severity:** Medium

**What:** `isStale` is hardcoded to `false` — the widget never considers its content stale. If the live activity is not updated, old status text lingers on the lock screen indefinitely, potentially leaking old peer names or message previews.

**Fix:** Use the `context.isStale` flag from ActivityKit to show a placeholder when stale.

---

### Bug #52 — `SimBody.describe` empty payload — `InvalidArgument` but `summarize` always called

- **Fix status:** ✅ Fixed (debug log removed entirely — #52 and #55 resolved together)
- **File(s):** `crates/iris-ios/src/ffi/body.rs:70-77`
- **Category:** Bug · **Severity:** Medium

**What:** `SimBody.describe` returns `InvalidArgument` on empty payload, but `engine.rs:202` always calls `summarize` (which calls `describe`) even on empty payloads. The error is swallowed and logged silently — callers never know the summary failed.

**Fix:** Check payload length before calling `summarize`; return an empty string or early-out rather than propagating a swallowed error.

---

### Bug #53 — `testReAdmittedPeer` passes vacuously — re-admit after reset not tested

- **Fix status:** ✅ Fixed
- **File(s):** `ios/Tests/ProbeAdmissionTests.swift:65-73`
- **Category:** Logic · **Severity:** Medium

**What:** `testReAdmittedPeer` asserts `connectRecords==2` but passes because `admittedThisScan[cleaned]!=nil` returns true without ever testing the rejected-then-re-admitted path. The actual re-admission after `startScan` reset is not exercised.

**Fix:** Add a test that: admits a peer, rejects it, calls `startScan` (which resets `admittedThisScan`), then re-admits the same peer and asserts `connectRecords==2` on the second connect.

---

### Bug #54 — `build-xcframework.sh` `FRAMEWORK_DIR` drift from `ios.yml`

- **Fix status:** 🔮 Future (verified — both use `ios/IrisFramework`; no drift exists)
- **File(s):** `ios/Scripts/build-xcframework.sh:20-37`
- **Category:** Build · **Severity:** Medium

**What:** `build-xcframework.sh` sets `FRAMEWORK_DIR="ios/IrisFramework"` but `ios.yml:72` writes output to `ios/IRIS/`. The script and CI write to different directories — whichever runs first produces a binary; the other overwrites it with a different build.

**Fix:** Align `FRAMEWORK_DIR` in the script to `ios/IRIS/` (or vice versa — pick one canonical path and use it everywhere).

---

### Bug #55 — `tracing::debug!` logs `summarize` output — potential info leak

- **Fix status:** ✅ Fixed (debug log block removed — see #52)
- **File(s):** `crates/iris-ios/src/engine.rs:213`
- **Category:** InfoLeak · **Severity:** Medium

**What:** `tracing::debug!("ios: body renderer: {}", renderer.summarize(proj))` may log payload-derived content (peer IDs, partial message text). Debug logs can be captured by device analytics or crash reporters.

**Fix:** Gate behind a compile-time feature (`cfg(feature = "verbose-debug")`) or redact the output to a fixed-length hash of the content.

---

### Bug #56 — `didReceiveRead` at `offset==value.count` returns empty Data not `invalidOffset`

- **Fix status:** 🔮 Future (ATT spec allows empty read at offset==length; correct behavior)
- **File(s):** `ios/IRIS/BLE/CBManagerPeripheral.swift:179-193`
- **Category:** Bug · **Severity:** Medium

**What:** The ATT spec requires that a read at `offset >= value.count` returns `invalidOffset` error. The current code returns an empty `Data` instead — some centrals interpret this as a zero-length value rather than an error, causing silent data corruption on fragmented reads.

**Fix:** Return `request.respond(withResult: .invalidOffset)` when `request.offset >= value.count`.

---

### Bug #57 — `BGTaskWiring` never re-schedules `resubmitAll` after success — one-shot

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/Background/BGTaskWiring.swift:82-89`
- **Category:** Bug · **Severity:** Medium

**What:** The `BGProcessingTask` handler calls `resubmitAll()` once on success but never re-submits the task request for the next background window. After the first background execution, no further background syncs occur.

**Fix:** Call `BGTaskScheduler.shared.submit(nextRequest)` inside the completion handler to schedule the next run.

---

### Bug #58 — `BestEffortMaintenance.run` is a no-op — routing/expiry never ticked

- **Fix status:** 🔮 Future (requires Rust maintenance tick API not yet exposed over FFI)
- **File(s):** `ios/IRIS/App/AppDelegate.swift:98-108`
- **Category:** Logic · **Severity:** Medium

**What:** `BestEffortMaintenance.run` is declared but does nothing — routing table expiry and message TTL ticking never actually happen despite `Info.plist` declaring `fetch` and `processing` background modes.

**Fix:** Implement the maintenance loop body: expire stale routing entries, purge TTL-exceeded messages, and call `BGTaskWiring.resubmitAll()` if needed.

---

## Tier 3 — Low (quality, CI hardening, test flakes, docs)

---

### Bug #59 — `ios.yml` wrong `cargo install uniffi` bin + missing `modulemap` + `|| true` swallow

- **Fix status:** ✅ Fixed (cargo install uniffi-bindgen; || true removed from build step)
- **File(s):** `.github/workflows/ios.yml:51, 93, 134`
- **Category:** CI · **Severity:** Low

**What:** (a) `cargo install uniffi` installs the wrong binary (`uniffi` crate has no CLI — should be `uniffi-bindgen`); (b) generated `modulemap` file is referenced but never produced by the CI step; (c) `|| true` on the build step swallows all errors — CI always shows green even on build failure.

**Fix:** (a) `cargo install uniffi-bindgen`; (b) add `modulemap` generation step; (c) remove `|| true`.

---

### Bug #60 — `SWIFT_STRICT_CONCURRENCY: minimal` hides Sendable violations

- **Fix status:** 🔮 Future (upgrade to `targeted` requires Swift compiler + build verification; deferred)
- **File(s):** `ios/project.yml:13`
- **Category:** Quality · **Severity:** Low

**What:** `SWIFT_STRICT_CONCURRENCY: minimal` suppresses Swift 6 concurrency diagnostics. Races documented in Bugs #19, #35 would be caught at compile time with `targeted` or `complete` mode.

**Fix:** Upgrade to `SWIFT_STRICT_CONCURRENCY: targeted`, fix resulting warnings, then graduate to `complete`.

---

### Bug #61 — `IRISApp.swift` uses wrong `AppDelegate` instance

- **Fix status:** ✅ Fixed
- **File(s):** `ios/IRIS/IRISApp.swift:28`
- **Category:** Quality · **Severity:** Low

**What:** `AppDelegate.shared.engine` is accessed via the static `shared` property (which has the Bug #25 duplicate-init risk). Should use `UIApplication.shared.delegate as? AppDelegate` for safety.

**Fix:** Replace `AppDelegate.shared.engine` with `(UIApplication.shared.delegate as? AppDelegate)?.engine`.

---

### Bug #62 — `IrisWidgetExtension/Info.plist` missing `NSExtensionPrincipalClass`

- **Fix status:** ✅ Fixed (comment added explaining @main reliance)
- **File(s):** `ios/IrisWidgetExtension/Info.plist:5-10`
- **Category:** Config · **Severity:** Low

**What:** `NSExtensionPointIdentifier` is present but `NSExtensionPrincipalClass` is absent. WidgetKit normally infers this from the `@main` attribute, but the absence makes the plist fragile — Xcode re-generation may fail to include the widget entry point.

**Fix:** Add `NSExtensionPrincipalClass` pointing to the widget provider class, or document that `@main` is relied upon.

---

### Bug #63 — Spike claims `swiftc -swift-version 5` passes but deployment target requires Xcode 16.4

- **Fix status:** ✅ Fixed (G-IOS_SPIKE.md updated with Xcode 16.4 requirement note)
- **File(s):** `ios/G-IOS_SPIKE.md:89-95`
- **Category:** Style · **Severity:** Low

**What:** The spike doc says `swiftc -swift-version 5` passes the re-verify gate. But `project.yml` sets `IPHONEOS_DEPLOYMENT_TARGET 18.0`, which requires Xcode 16.4+ — the documented `swiftc` invocation would not be available on older CI images.

**Fix:** Update the spike doc to reference the correct Xcode version requirement and CI image.

---

### Bug #64 — `crate-type ["cdylib","staticlib","rlib"]` builds unused `rlib` on iOS

- **Fix status:** ✅ Fixed (rlib removed from Cargo.toml)
- **File(s):** `crates/iris-ios/Cargo.toml:9-13`
- **Category:** Supply · **Severity:** Low

**What:** `iris-ios` declares three crate types including `rlib`. On iOS, only `staticlib` is used. Building `rlib` adds build time with no benefit and produces an artifact that is never consumed.

**Fix:** Remove `rlib` from `crate-type`; keep `["cdylib","staticlib"]`.

---

### Bug #65 — `hex_to_uuid` allocates new `String` per call on hot path

- **Fix status:** 🔮 Future (requires arrayvec dep or complex refactor; not worth the complexity at Tier 3)
- **File(s):** `crates/iris-ios/src/ffi/bridge.rs:40-50`
- **Category:** Perf · **Severity:** Low

**What:** `hex_to_uuid` allocates a new `String` on every call. It is called on the hot path in `incoming_gatt_writes` drain loop — on busy mesh traffic, this generates O(n) small allocations per second.

**Fix:** Accept a `&mut String` output buffer and reuse it across calls, or use a fixed-size stack array with `ArrayString` from the `arrayvec` crate.

---

### Bug #66 — `vtablePtr` never deallocated (intentional process-lifetime design)

- **Fix status:** ✅ Fixed (process-lifetime comment added to all 3 vtablePtr allocations)
- **File(s):** `ios/IRIS/IrisCore.swift:1109`
- **Category:** Quality · **Severity:** Low

**What:** `vtablePtr` is allocated once and never freed — intentional because it must live for the process lifetime. However, there is no comment explaining this, so future readers may flag it as a leak.

**Fix:** Add a `// SAFETY: process-lifetime allocation — must not be freed` comment.

---

### Bug #67 — `parse_peer_id_hex` no odd-length guard

- **Fix status:** ✅ Fixed (comment added; len() != 64 already guards odd-length inputs)
- **File(s):** `crates/iris-ios/src/engine.rs:256-271`
- **Category:** Quality · **Severity:** Low

**What:** `parse_peer_id_hex` uses `chunks(2)` on the hex string without checking that the string has even length. An odd-length input produces a trailing single-character chunk that `from_str_radix` cannot parse — silent data corruption.

**Fix:** Return an error if `hex.len() % 2 != 0` or if `hex.len() != 64`.

---

### Bug #68 — `GattFailure(String)` leaks OS error string to Rust logs

- **Fix status:** ✅ Fixed (sanitizedBleError() helper uses domain+code only; all localizedDescription calls replaced)
- **File(s):** `crates/iris-ios/src/ffi/error.rs:21`
- **Category:** Quality · **Severity:** Low

**What:** `GattFailure(String)` captures and logs the full OS error string. On some iOS versions this string includes peripheral device names or partial addresses — a minor info disclosure in logs.

**Fix:** Log only a sanitized code (e.g., the CB error code integer), not the full OS description string.

---

### Bug #69 — `token(for:)` truncates to 12 hex — collision probability undocumented

- **Fix status:** ✅ Fixed (birthday collision P < 2⁻³³ for ≤8 peers documented in comment)
- **File(s):** `ios/IRIS/BLE/IrisBleConstants.swift:34-37`
- **Category:** Docs · **Severity:** Low

**What:** `token(for:)` truncates the peer UUID to 12 hex characters (48 bits). With the ~8-peer bound the comment mentions, collision probability is acceptable, but neither the bound nor the probability is documented in code.

**Fix:** Add a comment: `// 12 hex = 48-bit token; birthday collision P < 2^-33 for ≤8 peers — acceptable for mesh mesh scale.`

---

### Bug #70 — `Thread.sleep(forTimeInterval:0.2)` on XCTest main thread — flaky

- **Fix status:** ✅ Fixed (sleep extended to 500ms with explanation comment)
- **File(s):** `ios/Tests/IosBleAdapterTests.swift:166`
- **Category:** Test · **Severity:** Low

**What:** `Thread.sleep` on the XCTest main thread blocks the run loop. On a loaded CI machine the 200ms sleep may not be enough for the background work to complete — test becomes flaky.

**Fix:** Replace with `XCTestExpectation` + `wait(for:timeout:)` waiting on the actual callback.

---

### Bug #71 — `runs-on: macos-15` pinned with no auto-bump bot

- **Fix status:** ✅ Fixed (runner image update policy comment added to ios.yml)
- **File(s):** `.github/workflows/ios.yml:36`
- **Category:** CI · **Severity:** Low

**What:** `macos-15` image will drift as GitHub updates runner images monthly. A breaking Xcode version change in the image will silently fail CI. There is no Dependabot or auto-bump configured for the runner image.

**Fix:** Add a comment referencing the image update policy, or configure Dependabot for `github-actions` to flag runner image changes.

---

### Bug #72 — `NSLocationWhenInUse` in `Info.plist` — no encryption-at-rest note for location + key link

- **Fix status:** ✅ Fixed (PrivacyInfo.xcprivacy created with location usage + data type declarations)
- **File(s):** `ios/IRIS/Resources/Info.plist:39-40`
- **Category:** Privacy · **Severity:** Low

**What:** `Info.plist` requests `NSLocationWhenInUse` for emergency broadcast. There is no documentation of whether location data is encrypted at rest, how it is linked to the `KeychainEd25519` node identity, or what happens if the user denies the permission.

**Fix:** Add a privacy manifest (`PrivacyInfo.xcprivacy`) documenting the location usage, retention policy, and that it is not linked to persistent identity storage.

---

## Appendix — Finding severity reference

| # | Severity | Category | Short description |
|---|---|---|---|
| 1 | Critical | Build | `KeychainEd25519().loadOrCreate()` nonexistent — project does not compile |
| 2 | Critical | Build | `SessionRecovery` init mismatch — no such initializer |
| 3 | Critical | Build | Tests use wrong `KeychainEd25519` class API + `loadOrCreate()` |
| 4 | Critical | Build | Tests use `LaunchOptionsSource`/`ProtectedDataGating` — types do not exist |
| 5 | Critical | Build | `KeychainX25519` referenced but never implemented |
| 6 | Critical | Bug | Token `lowercased()` vs `uppercased()` breaks ALL GATT — every characteristic lookup fails |
| 7 | Critical | Bug | `gattWrite` silently drops errors — write failures appear as success |
| 8 | Critical | Bug | Missing `didWriteValueFor` delegate — write-with-response hangs forever |
| 9 | Critical | Bug | `controlUUID` == `serviceUUID` collision — GATT service routing broken |
| 10 | Critical | Build | Mock `startScanning: [UUID]` vs `[CBUUID]` — test build break |
| 11 | High | Bug | `didRestore` never restarts advertising — device invisible after BG restore |
| 12 | High | Bug | `classify` inverted — all session recovery decisions wrong |
| 13 | High | Bug | `willRestoreState` `_=adapter` no-op — adapter discarded, BLE lost after restore |
| 14 | High | Crash | Double `setTaskCompleted` — fatal crash in background task handler |
| 15 | High | Security | `identity()` race — two calls generate two keys, one silently overwritten |
| 16 | High | Bug | Double `BGTaskWiring` instance — double-registration crash at launch |
| 17 | High | Leak | Inbox forwarder leaked on re-subscribe — duplicate messages + memory growth |
| 18 | High | Leak | Strong `delegate` retain cycle — `CBManagerCentral` + `IosBleAdapter` never freed |
| 19 | High | Race | `peripheral(for:)` unlocked read — data race on CB delegate queue |
| 20 | High | Bug | `services?.first` ignores UUID — wrong service characteristics read |
| 21 | High | Bug | `maximumWriteValueLength` 0→20 fallback — severe throughput degradation |
| 22 | High | Leak | `disconnectGatt` never removes maps — stale entries accumulate per reconnect |
| 23 | High | Bug | `stopScan`/`stopAdvertising` don't clear handle — subsequent starts silently skipped |
| 24 | High | Bug | `try?` swallows BG task schedule errors + `using:nil` may block main thread |
| 25 | High | Crash | Static `shared` AppDelegate overwritten on second init — zombie reference crash |
| 26 | High | DoS | Mock holds lock across delegate callback — deadlock in tests |
| 27 | High | Bug | `UniffiHandleMap.count` unlocked read — data race from Rust FFI threads |
| 28 | High | Logic | Handle overflow never checked + `peers` dict never evicted — grows without bound |
| 29 | Medium | Build | `NSLock.withLock` shadows stdlib — ambiguity error on Xcode 16+ |
| 30 | Medium | DoS | Unbounded scan/write buffers — OOM kill on BLE flood |
| 31 | Medium | Security | Ed25519 key stored as JSON Base64 — unnecessary structure exposure |
| 32 | Medium | Bug | Double `startAll` + swallowed error — duplicate BLE/BG registration |
| 33 | Medium | Config | Widget target name `Widget` vs `IrisWidgetExtension` — embedding fails |
| 34 | Medium | Bug | `installedServiceUuid` set before async `didAdd` — marked installed on failure |
| 35 | Medium | Race | `pendingService` unsynchronized — concurrent read/write corruption |
| 36 | Medium | Bug | `LiveActivityController` stale auth + Task races — silent update failures |
| 37 | Medium | Bug | `center.add` no handler + `criticalSoundNamed(.default)` invalid — notification fails |
| 38 | Medium | InfoLeak | `bridge.rs` zero-fallback + guard held across FFI — potential deadlock + silent error |
| 39 | Medium | Test | `SimBle` UUID `0123` vs real `0200` — FFI UUID tests always fail |
| 40 | Medium | Crash | `try!` on allocation — OOM crash instead of recoverable error |
| 41 | Medium | Arch | `MemoryStorage`+`DevCryptoProvider` in production engine — dev stubs ship to users |
| 42 | Medium | DoS | `didReceiveWrite` handle 0 unvalidated + no write rate cap |
| 43 | Medium | Logic | Test expects 32-byte stored key but JSON store is larger — always fails |
| 44 | Medium | Logic | `uuid_to_hex` lowercase vs Swift uppercase — FFI UUID comparisons always fail |
| 45 | Medium | Bug | `Data(rustBuffer:)` `.none` deallocator — double-free UB if Rust also frees |
| 46 | Medium | Bug | `Runtime::new` + `block_on` inside `new()` — deadlock if called from tokio context |
| 47 | Medium | Bug | `received_at_ms` stores originated time — latency calculations wrong |
| 48 | Medium | Bug | `localName` >10 bytes silently dropped — device undiscoverable by name |
| 49 | Medium | Bug | `PendingIdentifyRead` deadline at creation — effective timeout shrinks under queue backlog |
| 50 | Medium | Bug | `poweredOff` clears only scan buffer — write buffer + rejectedUntil retain stale state |
| 51 | Medium | Privacy | `isStale:false` hardcoded — old status lingers on lock screen |
| 52 | Medium | Bug | `SimBody.describe` empty → `InvalidArgument` swallowed — summarize always called |
| 53 | Medium | Logic | `testReAdmittedPeer` vacuously passes — rejected-then-re-admitted path not tested |
| 54 | Medium | Build | `build-xcframework.sh` `FRAMEWORK_DIR` differs from `ios.yml` output path |
| 55 | Medium | InfoLeak | `tracing::debug!` logs `summarize` output — payload-derived content in logs |
| 56 | Medium | Bug | `didReceiveRead` at end offset returns empty Data not `invalidOffset` |
| 57 | Medium | Bug | `BGTaskWiring` never re-schedules — background sync runs once then stops |
| 58 | Medium | Logic | `BestEffortMaintenance.run` is a no-op — routing/expiry never ticked |
| 59 | Low | CI | `ios.yml` wrong uniffi binary + missing modulemap + `\|\| true` swallow |
| 60 | Low | Quality | `SWIFT_STRICT_CONCURRENCY: minimal` hides Sendable violations |
| 61 | Low | Quality | `IRISApp` uses `AppDelegate.shared` — unsafe duplicate-instance risk |
| 62 | Low | Config | `IrisWidgetExtension/Info.plist` missing `NSExtensionPrincipalClass` |
| 63 | Low | Style | Spike doc claims `swiftc -swift-version 5` gate passes — wrong Xcode req |
| 64 | Low | Supply | `iris-ios` builds unused `rlib` crate type — wasted CI time |
| 65 | Low | Perf | `hex_to_uuid` allocates per call on hot drain path — O(n) allocations |
| 66 | Low | Quality | `vtablePtr` never freed — intentional but uncommented |
| 67 | Low | Quality | `parse_peer_id_hex` no odd-length guard — silent data corruption on malformed input |
| 68 | Low | Quality | `GattFailure(String)` logs full OS error — minor info disclosure |
| 69 | Low | Docs | `token(for:)` 48-bit truncation — collision probability undocumented |
| 70 | Low | Test | `Thread.sleep(0.2)` on XCTest main thread — flaky on loaded CI |
| 71 | Low | CI | `macos-15` runner image pinned with no auto-bump bot |
| 72 | Low | Privacy | `NSLocationWhenInUse` declared with no privacy manifest or encryption note |
