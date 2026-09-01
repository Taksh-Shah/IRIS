# Cross-Section Integration — Bug-Hunt Report

**Reviewer:** Sohan (cross-section integration sweep)
**Scope:** All section boundaries — Priyam (Section 1: Crypto/Security) · Rahul (Section 2: Message Engine/Protocol) · Taksh (Section 3: Routing/Transport/Storage) · Sohan (Android FFI) · Shrey (Section 5: iOS FFI) · Dhairya (Desktop)
**Date:** 2026-09-01
**Commit reviewed:** `3b808f5` (branch `main`, clean tree)
**Toolchain:** cargo 1.97.1 · Windows 11

---

## 1. What this document is

A defect inventory focused exclusively on **cross-section integration points** — bugs that are invisible when reviewing any single section alone and only surface at the boundaries where two sections meet. Each finding identifies what Section A defines/produces and what Section B expects/consumes, why they conflict, and what the production impact is.

Per-section internal bugs are covered in each owner's own tracker. This document covers only mismatches, missing wiring, and incompatible assumptions that cross ownership boundaries.

**Reference format:** [`taksh_problems.md`](taksh_problems.md)

---

## 2. Headline — read this first

Three results dominate this report.

**First: Android and iOS cannot discover each other over BLE.** Android's peripheral advertises BLE service UUID `3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0d` and its scanner filters for that same UUID. iOS (following Rust core) advertises service UUID `01000000-0000-0000-0000-000000000000`. Neither side can see the other's advertising packets. The two platforms' BLE layers are invisible to each other. Every cross-platform mesh scenario silently fails at the first discovery step. See CROSS-001.

**Second: Priyam's entire `security/` module is dead code at runtime — across all platforms.** `MessageEngine` initialises with `NoopSecurityPolicy` (all checks permissive). The method `set_security_policy()` exists and is documented, but no platform engine — Android, iOS, or Desktop — ever calls it. Rate limiting, replay protection, quota enforcement, and the emergency ACL are silently bypassed for every message on every device in production. See CROSS-003.

**Third: iOS ships unsigned, unencrypted messages in debug builds and crashes at startup in release builds.** `IrisEngine::new()` on iOS hardcodes `DevCryptoProvider`, a stub with no real signing or encryption. Android wires its real TEE-backed `AndroidCryptoProvider` correctly. iOS has no production crypto path at all. See CROSS-004.

---

## Progress Tracker — living section, updated as fixes land

**This section is rewritten by the fix loop after every finding.** Individual findings carry their own `Fix status` line for in-place detail; this table is the roll-up.

**Last updated:** 2026-09-01 — initial sweep complete. 8 cross-section integration findings. 0 fixed.

### Status legend
⬜ Not started · 🔵 In progress · ✅ Fixed & tested · 🟢 Fixed & verified · 🔒 Blocked (reason recorded) · ❌ Attempted, reverted (reason recorded) · ⚪ Not applicable

### Tier roll-up

| Tier | Name | Findings | ⬜ Not started | 🔵 In progress | ✅/🟢 Done | 🔒 Blocked | ❌ Reverted | Gate to enter |
|---|---|---|---|---|---|---|---|---|
| **0** | Critical — cross-platform data path broken | 1 | 1 | 0 | 0 | 0 | 0 | Fix first — nothing else matters if platforms can't see each other |
| **1** | High — security regressions and build breaks | 4 | 4 | 0 | 0 | 0 | 0 | Tier 0 complete |
| **2** | Medium — missing wiring, compile breaks on regen | 3 | 3 | 0 | 0 | 0 | 0 | Can run in parallel with Tier 1 |
| **Total** | | **8** | **8** | **0** | **0** | **0** | **0** | |

### Tier membership

**Tier 0** (1 finding — cross-platform data path):
CROSS-001

**Tier 1** (4 findings — security regressions, build breaks, data integrity):
CROSS-002, CROSS-003, CROSS-004, CROSS-005

**Tier 2** (3 findings — missing wiring, compile breaks on binding regen):
CROSS-006, CROSS-007, CROSS-008

---

## 3. Per-finding detail

---

### CROSS-001 — Android BLE Service UUID mismatch: Android ↔ iOS/Rust are invisible to each other

- **Fix status:** ⬜ Not started
- **Severity:** Critical
- **Sections:** Sohan (Android) ↔ Shrey (iOS) / Taksh (Rust Transport)
- **Files:**
  - `android/app/src/main/kotlin/iriscore/adapter/AndroidBleTransportAdapter.kt:69`
  - `android/app/src/main/kotlin/iriscore/service/BleScanSession.kt:111`
  - `crates/iris-core/src/transport/ble.rs:59`
  - `ios/IRIS/Services/IrisBleConstants.swift:22` (iOS matches Rust correctly)

**What Rust core defines:**
```rust
// crates/iris-core/src/transport/ble.rs:59
pub const IRIS_SERVICE_UUID: Uuid = Uuid([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
// = 01000000-0000-0000-0000-000000000000
```

**What Android defines and scans for:**
```kotlin
// AndroidBleTransportAdapter.kt:69
val IRIS_SERVICE_UUID: UUID = UUID.fromString("3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0d")

// BleScanSession.kt:111 — filters on Android's UUID, not Rust's
.setServiceUuid(ParcelUuid(AndroidBleTransportAdapter.IRIS_SERVICE_UUID))
```

**What iOS defines:**
```swift
// IrisBleConstants.swift:22 — matches Rust core correctly
static let serviceUUID = "01000000-0000-0000-0000-000000000000"
```

**Why it is wrong.** Android's peripheral GATT server registers under `3e5c6b1a-...-0c0d` and Android's BLE scanner filters for that same UUID. iOS advertises `01000000-...` (matching Rust core). An Android scanner looking for `3e5c6b1a-...-0c0d` receives no scan results from iOS peripherals advertising `01000000-...`, so iOS nodes are completely invisible to Android. In the other direction, iOS performs connect-to-identify (it connects to any visible peripheral and reads `IRIS_IDENTIFY_CHARACTERISTIC`), but `CBCentralManager` would still need the Android service UUID to find Android peripherals. The BLE mesh between Android and iOS cannot form — all cross-platform discovery silently drops.

Note: the GATT **write** characteristic UUID (`IRIS_WRITE_CHARACTERISTIC` = `3e5c6b1a-...-0c0e`) is correctly aligned between Rust and Android (confirmed at `ble.rs:82-84` and `AndroidBleTransportAdapter.kt:72`). Only the service UUID is wrong.

**Root cause.** Android's UUID was likely chosen independently from Rust core's constants. The iOS constants were taken from Rust directly; Android was not.

**Fix.** Align Android's `IRIS_SERVICE_UUID` to `01000000-0000-0000-0000-000000000000` (Rust core's value). Update `AndroidBleTransportAdapter.kt:69` and re-verify `BleScanSession.kt:111`, `ensureGattServer()`, and any `setServiceUuid` call in the Android BLE stack. No Rust or iOS change needed.

---

### CROSS-002 — iOS sends `ContentType::Text` for P0/SOS messages; Android sends `ContentType::Sos`

- **Fix status:** ⬜ Not started
- **Severity:** High
- **Sections:** Shrey (iOS engine) ↔ Rahul (message engine / routing policy)
- **Files:**
  - `crates/iris-ios/src/engine.rs:371`
  - `crates/iris-android/src/engine.rs:598-601`
  - `crates/iris-core/src/message.rs` (ContentType definition)

**Android (correct):**
```rust
// crates/iris-android/src/engine.rs:598-601
payload_type: if priority == MessagePriority::P0 {
    ContentType::Sos
} else {
    ContentType::Text
},
```

**iOS (wrong):**
```rust
// crates/iris-ios/src/engine.rs:371
payload_type: ContentType::Text,  // always Text — P0 not handled
```

**Why it is wrong.** `ContentType::Sos` is the wire type that activates emergency routing, security ACL priority handling, and the `EmergencyAcl::check_emergency_acl` gate in the message engine. When a P0 message carries `ContentType::Text`, every downstream component that branches on content type treats it as an ordinary message. Emergency flood suppression (`max_hops_for_priority` for SOS), the ACL's broadcast-authority check, and any UI layer that renders SOS specially are all bypassed. An iOS-originated SOS is functionally indistinguishable from a regular text message to every other node in the mesh.

**Root cause.** The `send_text` entry point on iOS never received the priority-sensitive content type logic that Android's parallel `send_text` did.

**Fix.** In `crates/iris-ios/src/engine.rs`, apply the same priority check as Android:
```rust
payload_type: if priority == MessagePriority::P0 {
    ContentType::Sos
} else {
    ContentType::Text
},
```

---

### CROSS-003 — `NoopSecurityPolicy` hardcoded; no platform engine ever installs `FullSecurityPolicy`

- **Fix status:** ⬜ Not started
- **Severity:** High
- **Sections:** All platforms ↔ Priyam (security) ↔ Rahul (message engine)
- **Files:**
  - `crates/iris-core/src/message_engine/mod.rs:306`
  - `crates/iris-core/src/message_engine/mod.rs:352-356` (`set_security_policy` docs)
  - `crates/iris-android/src/engine.rs` (no call to `set_security_policy`)
  - `crates/iris-ios/src/engine.rs` (no call to `set_security_policy`)
  - `crates/iris-desktop/src/` (no call to `set_security_policy`)

**What the message engine initialises with:**
```rust
// message_engine/mod.rs:306
security: std::sync::RwLock::new(Arc::new(NoopSecurityPolicy)),
```

**What `NoopSecurityPolicy` does:**
```rust
// security/mod.rs:151-154
#[derive(Default)]
pub struct NoopSecurityPolicy;

#[async_trait]
impl SecurityPolicy for NoopSecurityPolicy {}  // all default methods → permissive
```

**Why it is wrong.** The `set_security_policy()` method is public and documented ("Install a SEC-001 security policy (AC-12)") but is never called from any platform crate. Grep of `crates/iris-android/`, `crates/iris-ios/`, and `crates/iris-desktop/` returns zero matches for `set_security_policy`. This means:
- Rate limiting (`RateLimiter`): disabled — a peer can flood unlimited messages
- Replay protection (`ReplayEngine`): disabled — replayed old messages are accepted
- Quota enforcement (`QuotaManager`): disabled — storage can be exhausted without limit
- Emergency ACL (`EmergencyAcl`): disabled — authority-chain checks for P0/P1 broadcast never run
- Spam scoring (`SpamEngine`): disabled — content-hash deduplication never fires

Priyam's entire `security/` module is dead code at runtime. All of the fixes from the Priyam bug-hunt session are unreachable in production.

**Root cause.** `set_security_policy()` was added as a post-construction wiring step but no platform engine builder was updated to call it.

**Fix.** In each platform engine's constructor (or `build()` / `new()` function), after creating the `MessageEngine`, wire in `FullSecurityPolicy`:
```rust
// Example for iris-android/src/engine.rs
let trust_store = Arc::new(TrustStore::new());
engine.set_security_policy(Arc::new(FullSecurityPolicy::new(trust_store)));
```
Requires importing `iris_core::security::{FullSecurityPolicy}` and `iris_core::identity::TrustStore` in each platform crate's engine file. The `TrustStore` instance should be shared with the identity provisioning path so revocations propagate to the policy.

---

### CROSS-004 — iOS hardcodes `DevCryptoProvider`; no production crypto path exists for iOS

- **Fix status:** ⬜ Not started
- **Severity:** High
- **Sections:** Shrey (iOS engine) ↔ Priyam (crypto)
- **Files:**
  - `crates/iris-ios/src/engine.rs:23` (import)
  - `crates/iris-ios/src/engine.rs:120-135` (constructor)
  - `crates/iris-android/src/engine.rs:168-169` (correct Android path for comparison)
  - `crates/iris-android/src/ffi/x25519_provider.rs` (the pattern iOS needs)

**iOS constructor (broken):**
```rust
// iris-ios/src/engine.rs:135
Arc::new(DevCryptoProvider::new()),  // dev stub, no real signing
```

**Android constructor (correct):**
```rust
// iris-android/src/engine.rs:168-169
Arc::new(AndroidCryptoProvider::with_x25519_provider(signer, x25519_provider))
// signer = Arc<dyn FfiCryptoSigner> — TEE-backed Ed25519 via Keystore
```

**Why it is wrong.** `DevCryptoProvider` is an explicit dev-only stub. In debug builds it silently produces unsigned and unencrypted envelopes. The `#[cfg(not(debug_assertions))]` guard at line 122–126 returns `Err` in release builds, so any production iOS build crashes at engine startup — the app cannot launch. There is no code path in `iris-ios` that accepts a real `FfiCryptoSigner` from the Swift layer, despite the iOS `KeychainEd25519` class existing and being provisioned in `AppDelegate.swift`.

**Root cause.** The Android pattern of passing a `FfiCryptoSigner` foreign trait across the UniFFI boundary was implemented for Android (fixed as AN-1/AN-6) but the equivalent was never added to the iOS FFI bridge.

**Fix.** Mirror the Android pattern:
1. Expose `FfiCryptoSigner` in `crates/iris-ios/src/ffi/` (same trait as `iris-android`)
2. Create `IosCryptoProvider` (equivalent of `AndroidCryptoProvider`) that wraps the FFI signer
3. Add a `new_with_signer(ble, signer: Arc<dyn FfiCryptoSigner>) -> Result<Arc<Self>, IrisFfiError>` UniFFI constructor
4. In `ios/IRIS/App/AppDelegate.swift`, implement `FfiCryptoSigner` over `KeychainEd25519` and call the new constructor

---

### CROSS-005 — Desktop uses sender-controlled origination timestamp as `received_at_unix`

- **Fix status:** ⬜ Not started
- **Severity:** High
- **Sections:** Dhairya (Desktop) ↔ Rahul (Protocol / Envelope)
- **Files:**
  - `crates/iris-desktop/src/types.rs:60`
  - `crates/iris-android/src/engine.rs:504` (correct Android path)
  - `crates/iris-ios/src/engine.rs:259` (correct iOS path)

**Desktop (wrong):**
```rust
// iris-desktop/src/types.rs:60
received_at_unix: env.timestamp,  // env.timestamp = sender's self-declared origination time
```

**Android and iOS (correct):**
```rust
// iris-android/src/engine.rs:504
received_at_ms: unix_now().saturating_mul(1000),  // local wall-clock receipt time

// iris-ios/src/engine.rs:259
received_at_ms: unix_now().saturating_mul(1000),  // local wall-clock receipt time
```

**Why it is wrong.** `Envelope::timestamp` is set by the **sender** at message creation time. It is an unauthenticated, peer-controlled field — any node can set it to any value. Using it as `received_at_unix` means the Desktop inbox sort order is entirely under attacker control: a malicious node backdates messages to push them to the top of the inbox, or future-dates them to keep them pinned there indefinitely. Android and iOS both explicitly use `unix_now()` (local receipt time) to avoid exactly this. The field name `received_at_unix` implies local receipt, making the bug invisible to a reader not checking what `env.timestamp` actually contains.

**Root cause.** The desktop `IncomingMessage` type was written independently and used `env.timestamp` as the most obvious timestamp field on the envelope, without cross-checking the Android/iOS implementations.

**Fix.** Replace `env.timestamp` with the local receipt time in `crates/iris-desktop/src/types.rs`:
```rust
// use iris_core::message_engine::expiry::unix_now; (already imported or add it)
received_at_unix: unix_now(),
```

---

### CROSS-006 — `RoutingEngine::decide()` and `ScfEngine` never called from `MessageEngine`

- **Fix status:** ⬜ Not started
- **Severity:** Medium
- **Sections:** Rahul (Message Engine) ↔ Taksh (Routing / DTN Store-Carry-Forward)
- **Files:**
  - `crates/iris-core/src/message_engine/mod.rs` (no import of `RoutingEngine` or `ScfEngine`)
  - `crates/iris-core/src/routing/mod.rs:220` (`RoutingEngine::decide`)
  - `crates/iris-core/src/routing/scf.rs` (`ScfEngine::buffer_message`)

**Evidence.**
```rust
// message_engine/mod.rs — only routing import
use crate::routing::flood::max_hops_for_priority;
// RoutingEngine, ScfEngine — not imported, not instantiated, not called
```

A grep of `message_engine/mod.rs` for `RoutingEngine`, `ScfEngine`, `buffer_message`, `decide(`, `RoutingDecision` returns zero matches.

**Why it is wrong.** `MessageEngine::deliver_outbound` selects a transport via `TransportManager` but never calls `RoutingEngine::decide()` to choose between direct, opportunistic, Prophet/DTN, and flood strategies. DTN store-carry-forward (`ScfEngine`) is never invoked, so messages to temporarily unreachable peers are dropped rather than buffered. All 25 DTN findings fixed by Taksh (DTN-1 through DTN-25) and all 36 routing fixes (ROUT-1 through ROUT-36) have zero effect in production — the code runs only in its own unit tests and the sim.

**Root cause.** The routing engine was built and refined as a self-contained module. The wiring commit that inserts it into the `MessageEngine`'s delivery loop was never written. Taksh's own tracker flags this: "GAP-7 — nothing ever calls `Transport::connect()`" and "the DTN/SCF layer has no production caller."

**Fix.** The `MessageEngine` needs a `RoutingEngine` field. `deliver_outbound` should call `routing.decide(destination, message_class, available_transports)` and act on the returned `RoutingDecision` variant (direct, flood, DTN-buffer, etc.). When `RoutingDecision::StoreAndForward` is returned, call `ScfEngine::buffer_message` instead of immediately calling `transport.send()`. This is a substantial wiring commit — requires coordination between Rahul (owns `message_engine/`) and Taksh (owns `routing/`).

---

### CROSS-007 — Android Kotlin imports non-existent UniFFI class names

- **Fix status:** ⬜ Not started
- **Severity:** Medium
- **Sections:** Sohan (Android Kotlin) ↔ Rust FFI generation
- **Files:**
  - `android/app/src/main/kotlin/iriscore/adapter/AndroidBleTransportAdapter.kt:38`
  - `android/app/src/main/kotlin/iriscore/service/AdapterLifecycle.kt:3` (approx)

**What the Kotlin files import:**
```kotlin
// AndroidBleTransportAdapter.kt:38
import iriscode.TransportFailure   // does not exist

// AdapterLifecycle.kt:3
import iriscode.FfiTimeout         // does not exist
```

**What UniFFI actually generates from current Rust source.** UniFFI names Kotlin sealed-class variants using the exact Rust enum variant name. The Rust `IrisFfiError` enum has variants `Transport(String)` and `Timeout` — UniFFI generates these as `IrisFfiError.Transport` and `IrisFfiError.Timeout`, not `TransportFailure` or `FfiTimeout`.

**Why it is wrong.** Both import statements will produce "unresolved reference" compile errors the moment the Kotlin bindings are regenerated from the current Rust source. The Android project currently compiles only because it uses pre-built `.so` files and a pre-generated bindings snapshot whose variant names may differ from what the current Rust would produce. Any binding regeneration (required before a real release) breaks the build.

**Fix.** Replace the imports:
```kotlin
// before
import iriscode.TransportFailure
import iriscode.FfiTimeout
// after
import iriscode.IrisFfiError   // use IrisFfiError.Transport and IrisFfiError.Timeout
```
Update all catch/throw sites in both files to use the correct variant names.

---

### CROSS-008 — `performMaintenance()` absent from committed `IrisCore.swift`; `AppDelegate` calls it

- **Fix status:** ⬜ Not started
- **Severity:** Medium
- **Sections:** Shrey (iOS app / AppDelegate) ↔ UniFFI bridge (Rust + committed Swift bindings)
- **Files:**
  - `ios/IRIS/App/AppDelegate.swift:119`
  - `ios/IRIS/RustFFI/IrisCore.swift` (committed auto-generated Swift bindings)
  - `crates/iris-ios/src/engine.rs:288` (`perform_maintenance` Rust export)

**What AppDelegate calls:**
```swift
// AppDelegate.swift:119
engine?.performMaintenance()
```

**What the committed `IrisCore.swift` contains.** Grep of the committed `ios/IRIS/RustFFI/IrisCore.swift` finds zero occurrences of `performMaintenance`. The Rust source does export `perform_maintenance` via `#[uniffi::export]` (added as bug #58 fix at `engine.rs:288`), but the committed Swift bindings file was not regenerated after that change.

**Why it is wrong.** The committed `IrisCore.swift` is the file that ships with the app and that Xcode compiles against. `IrisEngine` in that file has no `performMaintenance()` method. `AppDelegate.swift:119` calls a method that does not exist in the type — this is a Swift compile error that blocks every iOS build. Additionally, if the bindings are regenerated (to add `performMaintenance`), the existing checksum table in `IrisCore.swift` will no longer match the recompiled `.dylib`, triggering a `fatalError("UniFFI API checksum mismatch")` at runtime.

**Root cause.** Bug #58's fix updated `AppDelegate.swift` and `engine.rs` but did not regenerate and commit the `IrisCore.swift` bindings from the new Rust source.

**Fix.**
1. Run `ios/Scripts/build-xcframework.sh` to recompile the Rust crate and regenerate `IrisCore.swift` from the updated UniFFI surface (with `perform_maintenance` now exported).
2. Commit the regenerated `IrisCore.swift` alongside the existing `AppDelegate.swift` change so both sides of the FFI boundary are in sync.

---

## 4. Section ownership cross-reference

| Bug | Priyam (S1) | Rahul (S2) | Taksh (S3) | Sohan (Android) | Shrey (iOS) | Dhairya (Desktop) |
|---|---|---|---|---|---|---|
| CROSS-001 | | | reads UUID | writes UUID | reads UUID | |
| CROSS-002 | | affected | | | **fix here** | |
| CROSS-003 | **fix: expose** | **fix: wire** | | fix: call | fix: call | fix: call |
| CROSS-004 | | | | reference impl | **fix here** | |
| CROSS-005 | | reference impl | | | | **fix here** |
| CROSS-006 | | **owns engine** | **owns routing** | | | |
| CROSS-007 | | | | **fix here** | | |
| CROSS-008 | | | | | **fix here** | |

---

## 5. Severity definitions

- **Critical** — prevents basic cross-platform operation; silent failure with no error
- **High** — security regression, data integrity loss, or build break
- **Medium** — compile error on regeneration, missing feature wiring, behavioral inconsistency
- **Low** — documentation drift, misleading naming

---

## 6. What was checked (scope)

- BLE UUID constants: `transport/ble.rs`, `AndroidBleTransportAdapter.kt`, `IrisBleConstants.swift`
- FFI trait shapes: `iris-android/src/ffi/ble_adapter.rs`, `iris-ios/src/ffi/ble_adapter.rs`
- Content type handling: `iris-android/src/engine.rs`, `iris-ios/src/engine.rs`
- Security policy wiring: `message_engine/mod.rs`, all platform engine constructors
- Crypto provider wiring: `iris-ios/src/engine.rs`, `iris-android/src/engine.rs`
- Timestamp semantics: `iris-desktop/src/types.rs`, `iris-android/src/engine.rs`, `iris-ios/src/engine.rs`
- Routing/SCF wiring: `message_engine/mod.rs` imports and field inventory
- Kotlin FFI imports: `AndroidBleTransportAdapter.kt`, `AdapterLifecycle.kt`
- Swift bindings freshness: `IrisCore.swift` vs `engine.rs` exports, `AppDelegate.swift`
