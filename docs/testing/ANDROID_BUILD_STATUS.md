# ANDROID BUILD STATUS — First Real Compilation Findings

**Date**: 2026-08-22 | **Finding**: 66 pre-existing Kotlin compilation errors

> **Update 2026-08-31:** the 66 Kotlin errors were fixed and BLE/Wi-Fi Direct
> delivery was demonstrated once on a bench, but there is still **no CI leg that
> builds or tests the Android app** (`cargo test -p iris-android` does not even
> compile — its test module calls `IrisEngine::new` with the wrong arity), and
> field reliability is poor. The hardware verification pass in
> `docs/bug-hunting/hardware_verification/` (HV-1, HV-2) adds a real CI gate plus
> a Mobly host-driven multi-device test harness. Until then, "the APK builds" is
> a manual, point-in-time fact.

## What happened

The ANDROID-001 node was ACCEPTED with env-gated builds (no Android SDK was
available at the time). Now that a real SDK + NDK + cross-compilation toolchain
has been installed, the first actual APK build reveals 66 pre-existing Kotlin
compilation errors across 7 files.

## Root causes

1. Generated UniFFI bindings exist but error variants (GattFailure,
   DeviceNotFound) are not re-exported through the api.kt facade
2. Wi-Fi Aware / Wi-Fi Direct adapters reference APIs with wrong signatures
   or missing imports
3. Type mismatches (Byte vs Int, ByteArray vs String) from Kotlin strict null
   safety + platform type differences
4. Missing imports for standard Android BLE/Wi-Fi classes

## What works

- Rust core compiles cleanly for Android ARM64 (`libiriscode.so` produced)
- All 727 Rust tests green
- Gradle build system functional (reaches Kotlin compilation stage)
- Both phones detected via adb

## What needs fixing before APK can build

All 66 Kotlin compilation errors must be resolved. These are code fixes in
the android/app/src/main/kotlin/ files, not configuration changes.
Estimated effort: focused debugging session (the errors cascade from ~10
root causes).

## Files with errors (by count)

| File | Errors |
|------|--------|
| AndroidWifiAwareTransportAdapter.kt | 26 |
| AndroidBleTransportAdapter.kt | 13 |
| AndroidWifiDirectTransportAdapter.kt | 12 |
| BleScanSession.kt | 9 |
| BleScanReceiver.kt | 3 |
| AdapterLifecycle.kt | 2 |
| KeystoreEd25519.kt | 1 |
