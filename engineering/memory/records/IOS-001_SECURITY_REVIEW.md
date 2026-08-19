# IOS-001 SECURITY_REVIEW — redteam findings (iter ~149, AC-20)

**Node**: IOS-001 (P0 PLATFORM — iOS app shell + Swift/CoreBluetooth FFI adapters)
**Stage**: SECURITY_REVIEW · **Iteration**: ~149 · **Date**: 2026-08-19
**Redteam**: supervisor redteam pass on `ios/IRIS/` — `IosBleAdapter.swift` (11-op
CoreBluetooth + connect-to-identify `gatt_read`), `CoreBluetoothSeam.swift` +
`CBManagerCentral.swift`/`CBManagerPeripheral.swift` (real CB managers +
mock seam), `IrisBleConstants.swift`, `KeychainEd25519.swift` (identity.md),
`SessionRecovery.swift`, `BGTaskWiring.swift`, `LiveActivityController.swift`,
`Notifications.swift`, `AppDelegate.swift`/`IRISApp.swift` + the Rust FFI
contract (`crates/iris-ios` engine.rs / bridge.rs / ble_adapter.rs) it must satisfy
**Verdict**: **FAIL → RESOLVED** — 4 MEDIUM / 1 LOW FIXED, 1 LOW DOC-FIXED,
4 RECORDED (no CRITICAL/HIGH; env-gated Swift fixes verified by source-review —
compile/run deferred to the ios.yml macOS CI leg)

---

## Disposition summary

| ID | Severity | Title | Disposition |
|----|----------|-------|-------------|
| IOS-RT-101 | MEDIUM | Pre-warm retry in `applicationDidBecomeActive` is dead: adapter constructed AFTER the identity guard, so an early return (protected data unavailable) leaves `self.adapter == nil` and the retry can never reconstruct the engine → node never joins the mesh when pre-warmed before first unlock (AC-10/11 availability gap) | FIXED |
| IOS-RT-102 | MEDIUM | `RealBlePeripheralSeam.addService` re-adds the IRIS service UUID on every re-arm; CoreBluetooth errors on a second `add` (silent — no delegate forwarding), and the static characteristic value never refreshes → stale identify beacon served after `willRestoreState` re-arm (stale freshness on the wire) | FIXED |
| IOS-RT-103 | MEDIUM | `gattRead` BLE-RT-C003 timeout cancels the connection but does NOT set `rejectedUntil` — a connect-to-identify timeout IS an identify failure, so the AC-7 admission window is not armed and the next scan cycle re-probes a peer that already stalled once (probe-budget exhaustion against persistent non-ident peers) | FIXED (+regression) |
| IOS-RT-104 | MEDIUM | Concurrent `gattRead` for the same token overwrites `pendingReads[token]` — the first caller's `PendingIdentifyRead` is orphaned and it strands until its own hard timeout (bounded liveness; second caller steals the read outcome) | FIXED (+regression) |
| IOS-RT-105 | LOW | `Notifications.swift` doc claims the critical-alert sound "falls back to `.default`" when the entitlement is absent; no such fallback exists (`criticalSoundNamed` posts silently without the entitlement) — doc-vs-code honesty | DOC-FIXED |
| IOS-RT-106 | LOW | `scanResultBuffer`/`gattWriteBuffer` unbounded (polar poller cadence) — no remote-triggered growth beyond discovery/admitted-peer rates; bounded by the 8-probe budget + single active 11-op surface | RECORDED (→ ANDROID-001 RT-106 ring-buffer parity, follow-up) |
| IOS-RT-107 | INFO | Inbound `didReceiveWrite` from a peer never connect-`connectGatt`'d attributes handle 0 — the Rust core partitions it as foreign and drops it (verified ble.rs:756-771); no panic, design-secure admission boundary | RECORDED (positive control) |
| IOS-RT-108 | INFO | `SessionRecovery.classifyLaunchReason` `.backgroundTask` key is not a documented `didFinishLaunching` signal; BGTask re-submit runs at EVERY launch regardless → classification branch is effectively dead, zero impact | RECORDED |

## Findings detail

### IOS-RT-101 (MEDIUM) — pre-warm retry is dead code → availability gap
`AppDelegate.application(_:didFinishLaunchingWithOptions:)` built the adapter
AFTER the `identity.loadOrCreate()` guard. On a pre-warm launch (protected data
unavailable, RES-0025 RQ-4 — the exact scenario the app documents), the guard
returns `true` early with `self.adapter == nil`. `applicationDidBecomeActive`
then guards `if let adapter = self.adapter` (AppDelegate.swift:88) → nil → the
retry path never constructs the engine → **the node never comes up until a full
process relaunch** (the pre-warm retry the design promises is empirically dead).
**Fix**: construct the `IosBleAdapter` before the identity guard so the retry
reuses it (AppDelegate.swift:27-50). The adapter's CB managers are lazy (created
on first use, CBManagerCentral.swift:30-38), so early construction has no
CoreBluetooth side-effect on the pre-warm path.

### IOS-RT-102 (MEDIUM) — addService not idempotent + stale identify beacon
`IosBleAdapter.startAdvertising(data:)` calls `peripheral.addService(...)` on
every call (IosBleAdapter.swift:148-164). `RealBlePeripheralSeam.addService`
unconditionally rebuilt the `CBMutableService` and called `manager.add(service)`
(CBManagerPeripheral.swift:44-64). CoreBluetooth rejects a second `add` of the
same published service UUID — silently (no `didAdd` forwarding in the seam) —
and the re-arm beacon's `data.payload` was never re-programmed into the served
characteristic's value. Consequence: after a `willRestoreState` re-arm, remote
scanners connect-to-identify and receive a **stale beacon** (old freshness),
undermining AC-5/AC-10 freshness semantics on restored paths.
**Fix**: `addService` is now idempotent — it records `installedServiceUuid` on
the first publish and honors the cold-start `poweredOn` re-apply exactly once
(the same-`installedServiceUuid` guard in `peripheralManagerDidUpdateState`),
while keeping the LATEST `identifyBeacon` in app state (lock-guarded) and
serving it from `didReceiveRead` instead of the static characteristic value
(CBManagerPeripheral.swift:52-87,151-158,183-189). Re-arms refresh the beacon
on every `addService` call; the published service is never re-added.

### IOS-RT-103 (MEDIUM) — timeout does not arm the AC-7 admission window
`gattRead`'s `timedOut` branch called `central.cancelConnection` and threw
`IrisFfiError.timeout` but did not touch `rejectedUntil` (IosBleAdapter.swift:
262-268 pre-fix). `failRead` (the didFailToConnect/didDisconnect path) arms the
window (IosBleAdapter.swift:481-490), but a peer that accepts the connect and
then stalls the identify read (didConnect arrives, no read value ever) times out
via the C003 path only — so the same peer can be re-probed on every scan cycle,
depleting the 8-probe budget against a single dead/broken peer (BLESSED
self-DoS until AC-7's per-scan reset). A timeout IS a connect-to-identify
failure within C004's semantics ("never re-probe a peer that fails
connect-to-identify").
**Fix**: race-free `rejectedUntil[token] = now + probeRejectionWindow` in the
`timedOut` branch (IosBleAdapter.swift:276-286) + new regression test
`testGattReadTimeoutGatesAdmissionWindow` (IosBleAdapterTests.swift).

### IOS-RT-104 (MEDIUM) — concurrent gatt_read orphan/clobber
`pendingReads[token] = pending` was an unconditional overwrite (IosBleAdapter.
swift:248 pre-fix). Two concurrent `gattRead` calls for the SAME token (possible
if the Rust core double-drives a peer — e.g. discover + explicit read racing)
leave the first `PendingIdentifyRead` parked-but-orphaned: its semaphore is never
signaled by the delegate callbacks (they find the NEW pending), so the first
caller waits the full C003 timeout on a read that already succeeded for the
second caller. Bounded (10 s) but an avoidable stall + double-connect on the
radio path.
**Fix**: refuse a concurrent read — `link.withLock { guard pendingReads[token]
== nil ... }`, throw `IrisFfiError.invalidArgument("gatt_read already in flight
for this peer")` (IosBleAdapter.swift:247-259) + new regression test
`testConcurrentGattReadRejected` (IosBleAdapterTests.swift).

### IOS-RT-105 (LOW) — critical-alert doc-vs-code dishonesty
`Notifications.swift` protocol comment claimed `criticalSoundNamed` "falls back
to the standard sound when the app lacks the critical-alert entitlement" — no
such fallback exists in implementation (Notifications.swift:37-39); without the
entitlement the OS posts the notification with no sound. AC-9-style honesty on a
delivery-adjacent surface demands the doc match reality.
**Fix**: corrected both comment sites to state the true posture ("without the
entitlement the OS delivers the notification silently ... no runtime fallback
swap exists; P0/P1 delivery NEVER depends on notifications").

### IOS-RT-106 (LOW, RECORDED) — unbounded scan/write buffers
`scanResultBuffer`/`gattWriteBuffer` append per discovery/inbound-write and are
drained whole by the Rust poller every tick (bridge.rs `scan_results`/
`incoming_gatt_writes` drain). Remote-triggered growth is bounded: discovery
rate is capped by CoreBluetooth + `allowDuplicates:false`, and inbound-write
peers must be probe-admitted (8/scan) first. A poller halt would grow memory
indefinitely only if the engine also stopped shutting down the transport —
outside the reviewed trust scope. Carry-forward: ring-buffer/eviction parity with
ANDROID-001 RT-109 (NEW-WA-RT-112 pattern) if the core gains high-rate
polling. RECORDED.

### IOS-RT-107 (INFO, RECORDED — positive control) — handle-0 attribution
An inbound GATT write from a central the app never `connectGatt`'d attributes
`FfiGattWriteEvent.handle = 0` (IosBleAdapter.swift:449 `handleByToken[token] ??
0`). Verified safe on the Rust side: the core partitions the shared drain by
poller own-handle and drops everything not matching a live handle
(ble.rs:756-771) — handle 0 reaches no poller, no panic, and the unauthored
frame never enters reassembly. Unprobed centrals cannot deliver frames. Verified
positive control (admission boundary holds).

### IOS-RT-108 (INFO, RECORDED) — BGTask launch-key classification §-dead
`IrisLaunchOptionKeys.backgroundTask` is not a documented
`didFinishLaunchingWithOptions` signal for BGTaskScheduler-launched processes;
BGTask launches arrive with an empty/foreground-agnostic options dict.
`classifyLaunchReason()` therefore collapses to `.user` for BG-driven launches.
Impact is nil: `run()` re-submits BG tasks at EVERY launch regardless of reason
(SessionRecovery.swift:96-102) and re-arm only requires the BLE-restoration path
to be truthful. RECORDED (no code change; classification is advisory only).

---

## Positive controls re-verified HELD (redteam spot-check, post-fix)

- **FFI contract fidelity**: `IosBleAdapter` implements all 11 `FfiBleAdapter`
  ops (SWIFT layer), `BleBridge` implements all 11 core `BleAdapter` ops
  (bridge.rs:107-210); `gattRead` restricted to the identify characteristic
  (IosBleAdapter.swift:243-245, DEC-BLE-002-0015); `IrisFfiError` typed mapping
  via `ffi_err_to_ble` (bridge.rs:75-84) — C003 `Timeout` → `GattFailure`
  asserted by `timeout_mapping_contract_is_typed` (engine.rs:432-438).
- **Lock discipline**: all shared mutable state confined behind `self.lock`
  (NSLock); delegate callbacks arrive on the seam queue and mutate only under
  the lock; `pending.result` writes are ordered by `DispatchSemaphore.signal`-to-
  `wait` (happens-before), never a separate mutex domain. The new
  `installedServiceUuid`/`identifyBeacon` state in the peripheral seam is
  lock-guarded (CBManagerPeripheral.swift:74-80,180-185).
- **AC-9 ad honesty**: advertisement dictionary = service-UUID + local-name only;
  `localName` ≤ 10 bytes gate; no service/manufacturer data key
  (CBManagerPeripheral.swift:110-122); scan requires IRIS_SERVICE_UUID
  (IosBleAdapter.swift:111-119); inbound payloads stay empty on the ad path
  (centralSeam didDiscover).
- **AC-12 identity posture**: Keychain generic-password Ed25519,
  `kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly`, NO `kSecAccessControl`
  biometric flag, protected-data gate on load/`loadOrCreate` (KeychainEd25519.
  swift:100-148); upsert delete-then-add scoped to a single-key provider.
- **AC-11/AC-14 lifecycle**: launch-reason classify + debounced re-arm +
  BGTask re-submit at every launch; force-quit paths are documented no-ops;
  `setTaskCompleted` on every BGTask path incl. expiration
  (BGTaskWiring.swift:82-89).
- **Handle-0 inbound drop** (IOS-RT-107): verified safe at ble.rs:756-771.
- **MTU**: `setMtu` returns `maximumWriteValueLength` capped 512 with 20-B
  degraded guard for negotiation failure (IosBleAdapter.swift:292-306).

## Recorded residuals (known_limitations carry-forward)

1. Concurrency on SC Android parity: buffers unbounded (IOS-RT-106) — follow-up
   with ANDROID-001 ring-buffer eviction pattern.
2. BGTask launch-key classification advisory-only (IOS-RT-108) — no behavior
   change.

## Env-gated verification

The 5 FIXED items are Swift source edits + 2 new XCTest regressions. Dev host is
Windows (no Xcode/swiftc) → compile/test/execution is deferred to the ios.yml
macOS CI leg (`swift-build` + `test-macos` IRIS-MacOSTests scheme, macos-15 +
Xcode 16.4). Rust baseline unchanged impossible to re-verify for Swift; durable
Rust baseline **658/0/1 held previous pass** — re-run at VERIFY (AC-21).

**STAGE-GATE: SECURITY_REVIEW CLEARED → VERIFY (iter ~150, AC-21).**