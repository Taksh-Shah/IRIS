# ANDROID-001 SECURITY_REVIEW — redteam findings (iter 119, pass 1)

**Node**: ANDROID-001 (P0 PLATFORM — Android app shell + Kotlin FFI adapters)
**Stage**: SECURITY_REVIEW · **Iteration**: 119 · **Date**: 2026-08-17
**Redteam**: subagent ses_feffd18edffezB7W4YvOE7NKij (independent adversarial
review of `crates/iris-android` FFI + `AdapterLifecycle.kt` + identity +
FGS/scan/worker path)
**Verdict**: **FAIL** — 2 CRITICAL / 6 HIGH / 5 MEDIUM (pre-integration blockers;
fix + re-review before VERIFY)

---

## Disposition summary

| ID | Severity | Title | Disposition |
|----|----------|-------|-------------|
| AND-RT-101 | CRITICAL | Boot/teardown main-thread deadlock (blocking FFI `startAll`/`stopAll` on main looper starves Wi-Fi Direct `ActionListener` → ANR) | FIXED |
| AND-RT-102 | CRITICAL | SoftwareBackend identity ephemeral/in-memory → PeerId rotates per process restart on API 26-32 (RED-0005 persistent-identity violated) | FIXED |
| AND-RT-103 | HIGH | Adapters throw `IllegalStateException` → `UNIFFI_CALL_UNEXPECTED_ERROR` → Rust panic, not typed `IrisFfiException`; createGroup/joinGroup snapshot race | FIXED |
| AND-RT-104 | HIGH | `WifiAwareManager.attach()` returns void (not session) — Kotlin type mismatch (would fail compile on integration host); session flow broken | FIXED |
| AND-RT-105 | HIGH | `SessionGate.invalidate()` (@Synchronized) vs `ensureStarted()` (kotlinx Mutex) — no happens-before; dead resource can re-latch as live | FIXED |
| AND-RT-106 | HIGH | `BleScanSession` never calls `stopScan` — sessions accumulate; 30s-on/off cadence and 5/30-s backoff not enforced on service path | FIXED |
| AND-RT-107 | HIGH | `X25519StaticAd.build()` regenerates X25519 key each call — "static" ad not static; breaks RT-108 verified-peer reuse | FIXED |
| AND-RT-108 | HIGH | `VerifiedPeerCache` never populated/evicted — RT-108 "verified-peer reuse key" unimplemented | FIXED |
| AND-RT-109 | MEDIUM | `RingBufferOutbox` destination bound not strictly enforced; closed-NDP/group zombie queues never evicted | FIXED |
| AND-RT-110 | MEDIUM | NDP handles registered before network exists; `onAvailable`/`onSessionTerminated` no-ops leak handles/specifiers | FIXED |
| AND-RT-111 | MEDIUM | BLE GATT write silently drops (characteristic not discovered) + MTU returns requested not negotiated | FIXED |
| AND-RT-112 | MEDIUM | FfiCallTimeout holes: `blockOn` dead code, BLE drains/`isAvailable()` untimed, nested `runBlocking` | FIXED |
| AND-RT-113 | MEDIUM | 31-bit `deviceHash` collision can misattribute GATT-server writes across peers | FIXED |

## Findings detail

### AND-RT-101 (CRITICAL) — Boot/teardown main-thread deadlock
`MeshViewModel.init` calls `repository.startMesh()` synchronously on main thread;
generated `IrisEngine.startAll()` is non-suspend and runs `runtime.block_on`
(engine.rs:157-182) parking the main thread; Wi-Fi Direct `startDnsSd` →
`SessionGate.ensureStarted` → `awaitAction` resumes only on the main looper
(`initialize(appContext, Looper.getMainLooper())`) → listener never fires →
`startAll()` never returns → ANR. Same on `stopAll()`/`onCleared`.
**Fix**: dispatch all engine calls via `viewModelScope.launch(Dispatchers.Default)`
in `MeshViewModel` (start/stop/subscribe); guard with Mutex; never block main.

### AND-RT-102 (CRITICAL) — Ephemeral software identity
`SoftwareBackend` holds `private var generated: KeyPair?` only (KeystoreEd25519.kt
:113-116); `AutoBackend` picks Software for API < 33 (minSdk 26) → every
API 26-32 device rotates PeerId on process kill → TOFU pins break, queued
messages undeliverable.
**Fix**: persist a device-scoped wrapped software key (AndroidKeyStore AES-GCM
wrap + `getNoBackupFilesDir()` blob `iris_sw_identity.bin`), restore on boot,
never rotate silently; X25519StaticAd reuses the persisted identity.

### AND-RT-103 (HIGH) — IllegalStateException → Rust panic + group-snapshot race
Adapter normal-error paths throw raw `IllegalStateException` (WD:140,143,150,153;
WA:115-116) → generated wrapper reports `UNIFFI_CALL_UNEXPECTED_ERROR` (iriscode.kt
:1544-1560) → Rust transport treats as fatal/untyped. `createGroup`/`joinGroup`
read `groupState.snapshot()` synchronously after `awaitAction`, but the snapshot
is only populated by the CONNECTION_CHANGED broadcast → guaranteed-null race.
**Fix**: map adapter errors to typed `IrisFfiException` variants; wait for / cache
group info instead of throwing on a momentarily-null snapshot.

### AND-RT-104 (HIGH) — WifiAwareManager.attach() return-type + session flow
`attach()` assigns `awareManager.attach(...)` to `WifiAwareSession?` (WA:179-185) —
real API returns Unit (session via `AttachCallback.onAttached`) → Kotlin type
mismatch (integration-host compile failure). Gate `live` seeded only later by
`markStarted`; `ensureStarted()` returns null in the interim and re-attaches every
call → session churn.
**Fix**: `attach()` returns Unit; `live` = platform session seeded by
`onAttached`/`onSessionStarted`; suspend-wait (RT-110 timeout) for the callback
before `openNdp`/`subscribe`/`publish`; cache session, do not re-attach.

### AND-RT-105 (HIGH) — SessionGate invalidate vs ensureStarted race
`invalidate()` @Synchronized (JVM monitor) vs `ensureStarted()` kotlinx Mutex —
two synchronization domains; a create that returned just before invalidate can
install a dead resource as live → stuck until a second (possibly never) invalidate.
**Fix**: single Mutex discipline shared by invalidate (suspend) and ensureStarted;
double-check `live == null` after create completes.

### AND-RT-106 (HIGH) — BleScanSession never stops scanning
Loop does `startScan(...)` then `delay(...)` (BleScanSession.kt:67,88) — no
`stopScan` on the service path → continuous scan sessions accumulate; the 5/30-s
backoff + bursty battery pattern not enforced.
**Fix**: `stopScan(pendingIntent)` before each off-period; genuine start/stop pairs
per window.

### AND-RT-107 (HIGH) — X25519StaticAd regenerates key every call
`build()` calls `supplier.generateKeyPair()` per invocation (X25519StaticAd.kt
:44-55) — ad rotates per beacon; peers can't pin; RT-108 reuse breaks. (Signature
binding itself is correct — `bindPayload(x25519 || ed25519Pub)` signed by
Ed25519 identity, verified :61-67.)
**Fix**: stable X25519 keypair cached per identity (persisted alongside identity);
rebuild ad only on change.

### AND-RT-108 (HIGH) — VerifiedPeerCache never populated/evicted
`rememberVerified`/`evict` never called (defs AdapterLifecycle.kt:314-337);
inbound attribution always null → RT-108 reuse key unimplemented.
**Fix**: call `rememberVerified` at the envelope-resolution point on inbound;
`evict` on NDP/P2P teardown.

### AND-RT-109 (MEDIUM) — Outbox bound + zombie queues
Destination cap only evicts EMPTY LRU slots (AdapterLifecycle.kt:208-212) →
`queues.size` unbounded under churn; `closeNdp`/group teardown never drains the
destination queue (up to capacity frames retained forever).
**Fix**: strict LRU eviction even when non-empty; drain/discard destination queue
on close/teardown.

### AND-RT-110 (MEDIUM) — NDP handle lifecycle
`openNdp` registers handle + specifier immediately after fire-and-forget
`requestAwareNetwork` whose `onAvailable` is a no-op (WA:113-125,245-257); a
handle whose NDP never becomes available stays open + enqueue-able forever;
`onSessionTerminated()` no-op leaks `peerHandles`/`ndpSpecifiers`.
**Fix**: track per-handle availability (populate onAvailable/onLost); prune
handles/specifiers on session termination.

### AND-RT-111 (MEDIUM) — BLE GATT silent drops + MTU
`gattWrite` resolves characteristic via `gatt.services` (populated only after
async discovery; no `onServicesDiscovered` override); pre-discovery write →
null → returns success (`Unit`) though nothing was written; `onCharacteristicWrite`
ignores status; `setMtu` returns requested, not negotiated.
**Fix**: cache characteristic in `onServicesDiscovered`; return typed error when
unknown; surface failed writes; return negotiated MTU from `onMtuChanged`.

### AND-RT-112 (MEDIUM) — FfiCallTimeout holes
`blockOn` defined (AdapterLifecycle.kt:94-98) but unused; BLE drains
(`incomingGattWrites`/`scanResults`, BLE:266-276) and all `isAvailable()`
untimed; `syncCall` nests `runBlocking` in worker context (class doc forbids it).
**Fix**: use/remove `blockOn`; gate drains + availability with `withTimeout`;
avoid nested `runBlocking` on tokio workers.

### AND-RT-113 (MEDIUM) — 31-bit deviceHash collision
`deviceHash = address.hashCode() and 0x7fffffff` (BLE:150) keys
`pendingGattWrites`/links → collision misattributes frames across peers.
**Fix**: key per-peer state by the raw `device.address` (or a wider derived key).

---

## Positive controls held (noted by redteam)
Foreign-trait injection + explicit tokio Handle (#2576 workaround) structurally
correct; ring-buffer per-destination oldest-drop logic correct in the steady
state; X25519 signature binding genuine; manifest perms + neverForLocation
present; WorkManager unique-work + CONNECTED constraint present.

## Re-review
After FIXED disposition implementation: redteam re-review (iter ~120) to confirm
no regression before VERIFY. Rust + generated bindings untouched by fixes.

## Re-review disposition (iter 120 — redteam pass 2, subagent
ses_fefa4ee9effevJmpNQs6KHdbqv)

**Verdict: FAIL → RESOLVED.** All 13 fix logics confirmed **real code**
(11 VERIFIED-FIXED; RT-110/RT-112 core mechanisms present). Two blockers found
and **FIXED in-pass**:

- **R1 (compile-blocker)** — `AndroidWifiAwareTransportAdapter.kt:145`
  (`openNdp`): `nextHandle.getAndIncrement()` yields `Long`; the `suspendCall`
  lambda returned it where the `FfiWifiAwareAdapter.openNdp(peerHandle): ULong`
  override requires `ULong` → "Type mismatch: inferred Long but ULong expected".
  **FIXED**: `ndpHandle.toULong()` as the lambda tail (line 145).
- **R2 (runtime on minSdk tier)** — `AndroidBleTransportAdapter.kt:299-303`
  (`gattWrite`): 3-arg `writeCharacteristic(gatt, data, WRITE_TYPE_DEFAULT)` is
  API 33+ only; app is minSdk 26 → `NoSuchMethodError` on the API 26-32 tier
  (exactly the tier that uses `SoftwareBackend`, RT-102). **FIXED**: SDK gate
  `Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU` → 3-arg path; legacy
  2-arg `writeCharacteristic(characteristic)` with explicit `writeType` + a
  `false` return surfaced as `IrisFfiException.GattFailure`.

Also fixed in-pass (same vulnerability classes as the round):

- **R3 (main-thread FFI residual)** — `MeshViewModel.syncRelayNow()` ran
  `drainRelayOutbox()` (blocking `engine.sendText`) on the Main.immediate
  `viewModelScope` — the exact ANR pattern RT-101 closed. **FIXED**: dispatched
  via `withContext(Dispatchers.IO)` (MeshViewModel.kt:72-73).
- **R5 (scan leak)** — `BleScanSession.stop()` cancelled the job but left an
  active PendingIntent window scanning. **FIXED**: `stop()` now calls
  `stopScanWindow(scanner)` (BleScanSession.kt:122-129).

Recorded residuals (non-blocking, → VERIFY known_limitations):

- **R4** — non-`IrisFfiException` exceptions escaping a suspend adapter op
  (e.g. `TimeoutCancellationException` from the RT-104/RT-112 `withTimeout`
  paths) still map to `UNIFFI_CALL_UNEXPECTED_ERROR` (iriscode.kt:298-316) —
  the same fatal class RT-103 described. Documented `FfiCallTimeout` semantics
  ("liveness, not cancellation"); accepted as known_limitation.
- **R6** — BLE `startAdvertising` reads `adData.serviceData.isEmpty()` on a
  platform-nullable map (BLE:254) — NPE only if `setServiceData` absent (it is
  always called); benign lint-level note.
- **RT-112 partial** — `FfiCallTimeout.syncCall` still uses `runBlocking`
  (AdapterLifecycle.kt:80-91); the "no nested runBlocking on tokio workers"
  constraint is doc-only. Bodies are trivial field-reads/list-copies → bounded
  practical impact; accepted as known_limitation.

Positive controls re-verified **HELD** after fixes: foreign-trait injection +
explicit tokio Handle (engine.rs:58-64,158,203,233); ring-buffer per-destination
oldest-drop outbox (AdapterLifecycle.kt:196-242); X25519 signature binding
(X25519StaticAd.kt:50-79); manifest perms + `neverForLocation`
(AndroidManifest.xml:12-13,21-22,26,28,31-32); WorkManager unique-work + CONNECTED
constraint (WorkScheduler.kt:35-39).

**STAGE-GATE: SECURITY_REVIEW CLEARED → VERIFY (iter ~121).**